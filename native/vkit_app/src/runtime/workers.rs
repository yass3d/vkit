use super::*;

#[derive(Default)]
pub(super) struct ScanImportCoordinator {
    receiver: Option<Receiver<ScanImportEvent>>,
}

impl ScanImportCoordinator {
    fn is_active(&self) -> bool {
        self.receiver.is_some()
    }

    fn start(&mut self, path: PathBuf, wake: impl Fn() + Send + 'static) -> Result<(), String> {
        if self.is_active() {
            return Err("a scan import is already running".to_owned());
        }
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("vkit-scan-import".to_owned())
            .spawn(move || {
                let progress_sender = sender.clone();
                let mut last_phase = None;
                let mut last_sent_at = None;
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    PreparedScan::load_with_progress(&path, |progress| {
                        let now = Instant::now();
                        let phase_changed = last_phase != Some(progress.phase);
                        let interval_elapsed = last_sent_at.is_none_or(|last| {
                            now.duration_since(last) >= SCAN_IMPORT_PROGRESS_INTERVAL
                        });
                        if (phase_changed || progress.progress >= 1.0 || interval_elapsed)
                            && progress_sender
                                .send(ScanImportEvent::Progress {
                                    path: path.clone(),
                                    progress,
                                })
                                .is_ok()
                        {
                            last_phase = Some(progress.phase);
                            last_sent_at = Some(now);
                            wake();
                        }
                    })
                }))
                .map_err(|_| "scan import worker stopped unexpectedly".to_owned())
                .and_then(|result| result.map_err(|error| error.to_string()));
                let _ = sender.send(ScanImportEvent::Finished { path, outcome });
                wake();
            })
            .map_err(|error| error.to_string())?;
        self.receiver = Some(receiver);
        Ok(())
    }

    fn drain(&mut self) -> Vec<ScanImportEvent> {
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(receiver) = self.receiver.as_ref() {
            loop {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if disconnected
            || events
                .iter()
                .any(|event| matches!(event, ScanImportEvent::Finished { .. }))
        {
            self.receiver = None;
        }
        events
    }
}

enum WorkspaceLoadEvent {
    Finished {
        path: PathBuf,
        outcome: WorkspaceLoadOutcome,
    },
}

#[derive(Default)]
pub(super) struct WorkspaceLoadCoordinator {
    receiver: Option<Receiver<WorkspaceLoadEvent>>,
}

impl WorkspaceLoadCoordinator {
    fn is_active(&self) -> bool {
        self.receiver.is_some()
    }

    fn start(
        &mut self,
        job: WorkspaceLoadJob,
        wake: impl Fn() + Send + 'static,
    ) -> Result<(), String> {
        if self.is_active() {
            return Err("a workspace load is already running".to_owned());
        }
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("vkit-workspace-load".to_owned())
            .spawn(move || {
                let path = job.path().to_path_buf();
                let kind = job.kind();
                let outcome = catch_unwind(AssertUnwindSafe(|| job.run())).unwrap_or_else(|_| {
                    WorkspaceLoadOutcome::failure(
                        kind,
                        "workspace load worker stopped unexpectedly".to_owned(),
                    )
                });
                let _ = sender.send(WorkspaceLoadEvent::Finished { path, outcome });
                wake();
            })
            .map_err(|error| error.to_string())?;
        self.receiver = Some(receiver);
        Ok(())
    }

    fn drain(&mut self) -> Vec<WorkspaceLoadEvent> {
        let mut events = Vec::new();
        let mut disconnected = false;
        if let Some(receiver) = self.receiver.as_ref() {
            loop {
                match receiver.try_recv() {
                    Ok(event) => events.push(event),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }
        if disconnected
            || events
                .iter()
                .any(|event| matches!(event, WorkspaceLoadEvent::Finished { .. }))
        {
            self.receiver = None;
        }
        events
    }
}

impl Runtime {
    pub(super) fn ensure_scan_import_started(&mut self) {
        if self.scan_imports.is_active() {
            return;
        }
        let Some(path) = self.state.take_scan_import_request() else {
            return;
        };
        let failure_path = path.clone();
        let proxy = self.proxy.clone();
        if let Err(error) = self.scan_imports.start(path, move || {
            let _ = proxy.send_event(RuntimeEvent::WorkerWake);
        }) {
            self.state.dispatch(Action::FinishScanImport {
                path: failure_path,
                outcome: Err(error),
            });
        }
    }

    pub(super) fn poll_scan_import(&mut self) {
        for event in self.scan_imports.drain() {
            match event {
                ScanImportEvent::Progress { path, progress } => {
                    self.state.dispatch(Action::ReportImportProgress {
                        path,
                        progress: crate::state::ImportProgress::from_mesh_import(progress),
                    });
                }
                ScanImportEvent::Finished { path, outcome } => {
                    if let Err(error) = &outcome {
                        log(Severity::Error, "scan_import_failed", error);
                    }
                    self.state
                        .dispatch(Action::FinishScanImport { path, outcome });
                    self.persist_preferences();
                }
            }
        }
    }

    pub(super) fn ensure_workspace_load_started(&mut self) {
        if self.workspace_loads.is_active() {
            return;
        }
        let Some(job) = self.state.take_workspace_load_request() else {
            return;
        };
        let path = job.path().to_path_buf();
        let kind = job.kind();
        log(
            Severity::Info,
            "workspace_load_starting",
            &format!("kind={kind:?}; path={}", path.display()),
        );
        let proxy = self.proxy.clone();
        if let Err(error) = self.workspace_loads.start(job, move || {
            let _ = proxy.send_event(RuntimeEvent::WorkerWake);
        }) {
            log(Severity::Error, "workspace_load_start_failed", &error);
            self.state.dispatch(Action::FinishWorkspaceLoad {
                path,
                outcome: WorkspaceLoadOutcome::failure(kind, error),
            });
        }
    }

    pub(super) fn poll_workspace_load(&mut self) {
        for event in self.workspace_loads.drain() {
            match event {
                WorkspaceLoadEvent::Finished { path, outcome } => {
                    self.state
                        .dispatch(Action::FinishWorkspaceLoad { path, outcome });
                    self.persist_preferences();
                }
            }
        }
    }

    pub(super) fn ensure_worker_started(&mut self) {
        if self.jobs.is_active() || self.state.active_job().is_none() {
            return;
        }
        let (job_id, source_revision) = self.state.active_job().expect("checked active job");
        let snapshot_started = Instant::now();
        let snapshot = match snapshot_from_state(&self.state) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                log(Severity::Error, "job_snapshot_failed", &error.to_string());
                self.state.dispatch(Action::FinishGeneration {
                    job_id,
                    source_revision,
                    outcome: GenerationOutcome::Failed(error.to_string()),
                    morph_available: false,
                });
                return;
            }
        };
        let snapshot_elapsed = snapshot_started.elapsed();
        let proxy = self.proxy.clone();
        log(
            Severity::Info,
            "job_starting",
            &format!(
                "job_id={job_id}; revision={source_revision}; snapshot_ms={:.3}; pins={}; scan_vertices={}; template_vertices={}; scale_xyz=[{:.8},{:.8},{:.8}]; translation_cm=[{:.6},{:.6},{:.6}]; rotation_degrees=[{:.4},{:.4},{:.4}]",
                snapshot_elapsed.as_secs_f64() * 1_000.0,
                snapshot.pins.len(),
                snapshot.scan.vertices.len(),
                snapshot.template_base.vertices.len(),
                self.state.transform.scale_xyz[0],
                self.state.transform.scale_xyz[1],
                self.state.transform.scale_xyz[2],
                self.state.transform.translation_cm[0],
                self.state.transform.translation_cm[1],
                self.state.transform.translation_cm[2],
                self.state.transform.rotation_degrees[0],
                self.state.transform.rotation_degrees[1],
                self.state.transform.rotation_degrees[2],
            ),
        );
        if let Err(error) = self.jobs.start(snapshot, move || {
            let _ = proxy.send_event(RuntimeEvent::WorkerWake);
        }) {
            log(Severity::Error, "job_start_failed", &error);
            self.state.dispatch(Action::FinishGeneration {
                job_id,
                source_revision,
                outcome: GenerationOutcome::Failed(error),
                morph_available: false,
            });
            return;
        }
        self.persist_preferences();
    }

    pub(super) fn poll_worker(&mut self) {
        for event in self.jobs.drain() {
            match event {
                WorkerEvent::Progress {
                    job_id,
                    source_revision,
                    progress,
                } => {
                    if should_log_job_progress(
                        self.last_logged_job_progress,
                        job_id,
                        progress.stage,
                        progress.fraction,
                    ) {
                        self.last_logged_job_progress =
                            Some((job_id, progress.stage, progress.fraction));
                        log(
                            Severity::Debug,
                            "job_progress",
                            &format!(
                                "job_id={job_id}; stage={:?}; fraction={:.3}",
                                progress.stage, progress.fraction
                            ),
                        );
                    }
                    self.state.dispatch(Action::ReportProgress {
                        job_id,
                        source_revision,
                        progress,
                    });
                }
                WorkerEvent::Finished {
                    job_id,
                    source_revision,
                    outcome,
                } => {
                    match &outcome {
                        GenerationOutcome::Success {
                            output,
                            fit_reference_value,
                        } => log(
                            Severity::Info,
                            "job_completed",
                            &format!(
                                "job_id={job_id}; vertices={}; faces={}; eye_reference={fit_reference_value}",
                                output.vertices.len(),
                                output.faces.len()
                            ),
                        ),
                        GenerationOutcome::Failed(error) => {
                            log(Severity::Error, "job_failed", error)
                        }
                        GenerationOutcome::Cancelled => {
                            log(Severity::Info, "job_cancelled", &format!("job_id={job_id}"))
                        }
                    }
                    let morph_available = self.state.workspace.eye_morph.is_some();
                    self.state.dispatch(Action::FinishGeneration {
                        job_id,
                        source_revision,
                        outcome,
                        morph_available,
                    });
                }
            }
        }
    }

    pub(super) fn poll_export(&mut self) {
        for event in self.exports.drain() {
            match event {
                ExportWorkerEvent::Progress {
                    source_revision,
                    fraction,
                } => {
                    self.state.dispatch(Action::ReportExportProgress {
                        source_revision,
                        fraction,
                    });
                }
                ExportWorkerEvent::Finished {
                    source_revision,
                    outcome,
                } => {
                    match &outcome {
                        ExportOutcome::Success { receipt } => log(
                            Severity::Info,
                            "export_completed",
                            &format!(
                                "committed_paths={}",
                                receipt
                                    .committed_paths
                                    .iter()
                                    .map(|path| path.display().to_string())
                                    .collect::<Vec<_>>()
                                    .join(" | ")
                            ),
                        ),
                        ExportOutcome::Failed(error) => {
                            log(Severity::Error, "export_failed", error)
                        }
                    }
                    self.state.dispatch(Action::FinishExport {
                        source_revision,
                        outcome,
                    });
                    self.persist_preferences();
                }
            }
        }
    }

    pub(super) fn ensure_asset_catalog_workers_started(&mut self) {
        if !self.vam_appearance_assets.is_active()
            && let Some(request) = self.state.take_vam_appearance_work()
        {
            let fallback = request.clone();
            let proxy = self.proxy.clone();
            if let Err(error) = self.vam_appearance_assets.start(request, move || {
                let _ = proxy.send_event(RuntimeEvent::WorkerWake);
            }) {
                log(
                    Severity::Error,
                    "vam_appearance_worker_start_failed",
                    &error,
                );
                self.apply_asset_catalog_event(fallback.failure_event(error));
            }
        }
        if !self.builtin_morph_assets.is_active()
            && let Some(request) = self.state.take_vam_morph_work()
        {
            let fallback = request.clone();
            let proxy = self.proxy.clone();
            if let Err(error) = self.builtin_morph_assets.start(request, move || {
                let _ = proxy.send_event(RuntimeEvent::WorkerWake);
            }) {
                log(Severity::Error, "builtin_morph_worker_start_failed", &error);
                self.apply_asset_catalog_event(fallback.failure_event(error));
            }
        }
    }

    pub(super) fn poll_vam_assets(&mut self) {
        let appearance_events = self.vam_appearance_assets.drain();
        for event in appearance_events {
            self.apply_asset_catalog_event(event);
        }
        let morph_events = self.builtin_morph_assets.drain();
        for event in morph_events {
            self.apply_asset_catalog_event(event);
        }
    }

    pub(super) fn ensure_skin_asset_worker_started(&mut self) {
        if self.skin_assets.is_active() {
            return;
        }
        let Some(request) = self.state.take_skin_work() else {
            return;
        };
        let request_id = request.request_id;
        let preset_id = request.preset.stable_id.clone();
        let proxy = self.proxy.clone();
        if let Err(error) = self.skin_assets.start(request, move || {
            let _ = proxy.send_event(RuntimeEvent::WorkerWake);
        }) {
            log(Severity::Error, "skin_worker_start_failed", &error);
            self.state.dispatch(Action::FinishVaMSkin {
                request_id,
                preset_id,
                outcome: Err(error),
            });
        }
    }

    pub(super) fn poll_skin_assets(&mut self) {
        for SkinPreviewEvent {
            request_id,
            preset_id,
            outcome,
        } in self.skin_assets.drain()
        {
            match &outcome {
                Ok(preview) => log(
                    Severity::Debug,
                    "skin_preview_ready",
                    &format!(
                        "id={preset_id}; face={}x{}; torso={}x{}; triangles={}",
                        preview.face.width,
                        preview.face.height,
                        preview.torso.width,
                        preview.torso.height,
                        preview.geometry.triangles.len(),
                    ),
                ),
                Err(error) => log(Severity::Error, "skin_preview_failed", error),
            }
            self.state.dispatch(Action::FinishVaMSkin {
                request_id,
                preset_id,
                outcome,
            });
        }
    }

    pub(super) fn ensure_texture_asset_worker_started(&mut self) {
        if self.texture_assets.is_active() {
            return;
        }
        let Some(request) = self.state.take_texture_work() else {
            return;
        };
        let request_id = request.request_id();
        if let crate::texture_project::TextureWorkRequest::Bake(bake) = &request {
            log(
                Severity::Debug,
                "texture_bake_started",
                &format!(
                    "request_id={request_id}; resolution={}; layers={}; cached_rasters={}",
                    bake.resolution,
                    bake.layers.len(),
                    bake.cached_layer_rasters.len()
                ),
            );
            self.texture_bake_started = Some((request_id, Instant::now()));
        }
        let fallback = request.clone();
        let proxy = self.proxy.clone();
        if let Err(error) = self.texture_assets.start(request, move || {
            let _ = proxy.send_event(RuntimeEvent::WorkerWake);
        }) {
            log(Severity::Error, "texture_worker_start_failed", &error);
            match fallback {
                crate::texture_project::TextureWorkRequest::Decode(request) => {
                    self.state.dispatch(Action::FinishTextureDecode {
                        request_id,
                        layer_id: request.layer_id,
                        outcome: Err(error),
                    });
                }
                crate::texture_project::TextureWorkRequest::Bake(_) => {
                    self.state.dispatch(Action::FinishTextureBake {
                        request_id,
                        outcome: Err(error),
                    });
                }
            }
        }
    }

    pub(super) fn poll_texture_assets(&mut self) {
        for event in self.texture_assets.drain() {
            match event {
                TextureWorkerEvent::DecodeFinished {
                    request_id,
                    layer_id,
                    outcome,
                } => {
                    match &outcome {
                        Ok(image) => log(
                            Severity::Info,
                            "texture_layer_ready",
                            &format!("layer={layer_id}; image={}x{}", image.width, image.height),
                        ),
                        Err(error) => log(Severity::Error, "texture_layer_failed", error),
                    }
                    self.state.dispatch(Action::FinishTextureDecode {
                        request_id,
                        layer_id,
                        outcome,
                    });
                }
                TextureWorkerEvent::BakeFinished {
                    request_id,
                    outcome,
                } => {
                    let bake_ms = elapsed_ms(
                        self.texture_bake_started
                            .take_if(|(started, _)| *started == request_id)
                            .map(|(_, at)| at),
                    );
                    match &outcome {
                        Ok(baked) => log(
                            Severity::Debug,
                            "texture_bake_ready",
                            &format!(
                                "channels={}; request_id={}; bake_ms={bake_ms:.1}",
                                baked.images.len(),
                                baked.request_id
                            ),
                        ),
                        Err(error) => log(Severity::Error, "texture_bake_failed", error),
                    }
                    self.state.dispatch(Action::FinishTextureBake {
                        request_id,
                        outcome,
                    });
                }
            }
        }
    }

    fn apply_asset_catalog_event(&mut self, event: VaMWorkerEvent) {
        match event {
            VaMWorkerEvent::AppearanceProgress {
                catalog_revision,
                fraction,
            } => self.state.dispatch(Action::ReportVaMCatalogProgress {
                catalog_revision,
                fraction,
            }),
            VaMWorkerEvent::Appearance {
                catalog_revision,
                outcome,
            } => {
                match &outcome {
                    Ok(payload) => {
                        log(
                            Severity::Info,
                            "vam_appearance_ready",
                            &format!(
                                "revision={catalog_revision}; skins={}",
                                payload.skin_presets.len()
                            ),
                        );
                        if !payload.appearance_warnings.is_empty() {
                            log(
                                Severity::Warning,
                                "vam_catalog_appearance_warnings",
                                &summarise_catalog_warnings(&payload.appearance_warnings),
                            );
                        }
                    }
                    Err(error) => log(Severity::Error, "vam_catalog_failed", error),
                }
                self.state.dispatch(Action::FinishVaMCatalog {
                    catalog_revision,
                    outcome,
                });
                self.persist_preferences();
            }
            VaMWorkerEvent::MorphCatalog {
                catalog_revision,
                outcome,
            } => {
                match &outcome {
                    Ok(payload) => {
                        let receipt = &payload.morph_cache_receipt;
                        log(
                            Severity::Info,
                            "builtin_morph_catalog_ready",
                            &format!(
                                "revision={catalog_revision}; disposition={:?}; morphs={}; skipped={}; source_bank_bytes={}; cache={}",
                                receipt.disposition,
                                receipt.morph_count,
                                receipt.skipped_count,
                                receipt.source_bytes,
                                receipt.path.display()
                            ),
                        );
                        if let Some(first) = payload.morph_cache_warnings.first() {
                            log(
                                Severity::Warning,
                                "builtin_morph_catalog_warnings",
                                &format!(
                                    "count={}; first={first}",
                                    payload.morph_cache_warnings.len()
                                ),
                            );
                        }
                    }
                    Err(error) => {
                        log(
                            Severity::Warning,
                            "builtin_morph_catalog_unavailable",
                            error,
                        );
                    }
                }
                self.state.dispatch(Action::FinishVaMMorphCatalog {
                    catalog_revision,
                    outcome,
                });
            }
            VaMWorkerEvent::Morph {
                catalog_revision,
                geometry_revision,
                control_id,
                outcome,
            } => {
                match &outcome {
                    Ok(payload) => log(
                        Severity::Debug,
                        "builtin_morph_ready",
                        &format!(
                            "id={control_id}; active_vertices={}; unsupported_formulas={}",
                            payload.target.compatibility.active_vertex_count,
                            payload.unsupported_formula_count
                        ),
                    ),
                    Err(error) => log(Severity::Error, "builtin_morph_failed", error),
                }
                self.state.dispatch(Action::FinishVaMMorph {
                    catalog_revision,
                    geometry_revision,
                    control_id,
                    outcome,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_import_progress_keeps_the_receiver_alive_until_the_terminal_event() {
        let (sender, receiver) = mpsc::channel();
        let mut coordinator = ScanImportCoordinator {
            receiver: Some(receiver),
        };
        let path = PathBuf::from("head.obj");
        sender
            .send(ScanImportEvent::Progress {
                path: path.clone(),
                progress: MeshImportProgress {
                    phase: crate::importers::MeshImportPhase::MeshLoading,
                    progress: 0.25,
                    source_triangles: None,
                },
            })
            .unwrap();

        let first = coordinator.drain();
        assert!(matches!(
            first.as_slice(),
            [ScanImportEvent::Progress { path: event_path, .. }] if event_path == &path
        ));
        assert!(coordinator.is_active());

        sender
            .send(ScanImportEvent::Progress {
                path: path.clone(),
                progress: MeshImportProgress {
                    phase: crate::importers::MeshImportPhase::Simplification,
                    progress: 0.75,
                    source_triangles: None,
                },
            })
            .unwrap();
        sender
            .send(ScanImportEvent::Finished {
                path: path.clone(),
                outcome: Err("expected test terminal event".to_owned()),
            })
            .unwrap();

        let terminal = coordinator.drain();
        assert!(matches!(
            terminal.as_slice(),
            [
                ScanImportEvent::Progress { path: progress_path, .. },
                ScanImportEvent::Finished { path: finished_path, .. },
            ] if progress_path == &path && finished_path == &path
        ));
        assert!(!coordinator.is_active());
    }
}
