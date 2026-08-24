use super::*;

impl AppState {
    pub(super) fn load_scan(&mut self, path: PathBuf) {
        if !is_supported_mesh_path(&path) {
            self.status = StatusMessage::new(TextKey::NeedScan, StatusTone::Warning);
            return;
        }
        self.scan_import = ScanImportLifecycle::Requested { path };
        self.import_progress = Some(ImportProgress::new(ImportPhase::MeshLoading, 0.0));
        self.scan_import_completion = None;
        self.progress = Some(Progress::new(JobStage::Import, 0.05));

        self.status = StatusMessage::new(TextKey::ScanLoading, StatusTone::Info);
    }

    pub(crate) fn ensure_visible_alignment_opacity(&mut self) {
        if self.alignment_opacity <= 0.5 {
            self.alignment_opacity = 0.7;
        }
    }

    pub(super) fn report_import_progress(&mut self, path: PathBuf, progress: ImportProgress) {
        if self.scan_import.active_path() == Some(path.as_path()) {
            let progress = ImportProgress::with_size(
                progress.phase,
                progress.fraction,
                progress.source_triangles,
            );
            let fraction = self.import_progress.map_or(progress.fraction, |current| {
                current.fraction.max(progress.fraction)
            });

            let triangles = progress.source_triangles.or_else(|| {
                self.import_progress
                    .and_then(|current| current.source_triangles)
            });
            self.import_progress = Some(ImportProgress::with_size(
                progress.phase,
                fraction,
                triangles,
            ));
        }
    }

    pub(super) fn finish_scan_import(
        &mut self,
        path: PathBuf,
        outcome: Result<PreparedScan, String>,
    ) {
        if self.scan_import.active_path() != Some(path.as_path()) {
            return;
        }
        self.scan_import = ScanImportLifecycle::Idle;
        self.import_progress = None;
        self.progress = None;
        let prepared = match outcome {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = crate::diagnostics::record(
                    crate::diagnostics::Severity::Error,
                    "state",
                    "scan_import_failed",
                    &format!("{}: {error}", path.display()),
                );
                self.status = StatusMessage::with_detail(
                    TextKey::ScanLoadFailed,
                    StatusTone::Error,
                    import_failure_reason(&error),
                );
                return;
            }
        };
        self.set_edit_source_mode(EditSourceMode::ScanHead);
        let source_triangles = prepared.source_triangles();
        let final_triangles = prepared.final_triangles();
        self.workspace.install_prepared_scan(prepared);
        let detail = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        self.scan_path = Some(path);
        self.clear_output_destination();
        self.clear_alignment_history();
        self.transform = self.suggested_alignment().unwrap_or_default();
        self.refresh_active_scan_symmetry();
        self.pending_symmetry_change = None;
        self.placed_head = false;
        self.complete_pairs = 0;
        self.active_tab = self.create_landing_tab();
        self.ensure_visible_alignment_opacity();
        let generation = self.next_scan_import_completion_generation;
        self.next_scan_import_completion_generation = generation.wrapping_add(1).max(1);
        self.scan_import_completion = Some(ScanImportCompletion {
            generation,
            completed_at: Instant::now(),
        });
        self.invalidate_geometry(TextKey::ScanLoaded);
        let detail = source_triangles
            .filter(|&source| source > final_triangles)
            .map_or(detail.clone(), |source| {
                format!("{detail}  ({source} → {final_triangles} triangles)")
            });
        self.status = StatusMessage::with_detail(TextKey::ScanLoaded, StatusTone::Info, detail);
    }

    pub(super) fn load_template(&mut self, path: PathBuf) {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);

        if matches!(extension.as_deref(), Some("vmi" | "vmb")) {
            self.load_vam_morph_pair(path);
            return;
        }
        self.request_workspace_load(WorkspaceLoadKind::Template, path, TextKey::TemplateLoading);
    }

    pub(super) fn request_workspace_load(
        &mut self,
        kind: WorkspaceLoadKind,
        path: PathBuf,
        key: TextKey,
    ) {
        self.workspace_load = WorkspaceLoadLifecycle::Requested { kind, path };
        self.import_progress = Some(ImportProgress::new(ImportPhase::MeshLoading, 0.0));
        self.progress = Some(Progress::new(JobStage::Import, 0.05));
        self.status = StatusMessage::new(key, StatusTone::Info);
    }

    pub fn take_workspace_load_request(&mut self) -> Option<WorkspaceLoadJob> {
        let WorkspaceLoadLifecycle::Requested { kind, path } = &self.workspace_load else {
            return None;
        };
        let kind = *kind;
        let path = path.clone();
        self.workspace_load = WorkspaceLoadLifecycle::Running {
            kind,
            path: path.clone(),
        };
        Some(match kind {
            WorkspaceLoadKind::Template => WorkspaceLoadJob::Template(TemplateLoadJob {
                anchor: self.workspace.template_geometry.clone(),
                cached_provider: self.vam_geometry_provider.clone(),
                cached_provider_path: self.vam_geometry_base_path.clone(),
                cached_provenance: self.vam_geometry_base_provenance,
                current_sex: self.figure_sex,
                discovery: self.provider_discovery_context(),
                path,
            }),
            WorkspaceLoadKind::Result => WorkspaceLoadJob::Result { path },
            WorkspaceLoadKind::VaMMorphPair => WorkspaceLoadJob::VaMMorphPair(VaMPairLoadJob {
                template_geometry: self.workspace.template_geometry.clone(),
                provider: self.vam_geometry_provider.clone(),
                figure_sex: self.figure_sex,
                allow_vertex_only_pose: false,
                path,

                origin: MorphSource::LocalVaM,
            }),
            WorkspaceLoadKind::DirectEditSource => {
                let Some(source) = self.pending_direct_edit_source.take().or_else(|| {
                    self.vam_edit_sources
                        .iter()
                        .find(|source| source.path == path)
                        .cloned()
                }) else {
                    self.workspace_load = WorkspaceLoadLifecycle::Idle;
                    self.import_progress = None;
                    self.progress = None;
                    self.status = StatusMessage::new(TextKey::GenerationFailed, StatusTone::Error);
                    return None;
                };
                WorkspaceLoadJob::DirectEditSource(DirectEditLoadJob {
                    source,
                    vam_root: self.vam_root.clone(),
                    template_geometry: self.workspace.template_geometry.clone(),
                    provider: self.vam_geometry_provider.clone(),
                    figure_sex: self.figure_sex,
                    cached_morphs: self.vam_cached_morphs.values().cloned().collect(),
                    morph_index: self.vam_morph_index.clone(),
                })
            }
        })
    }

    pub(super) fn finish_workspace_load(&mut self, path: PathBuf, outcome: WorkspaceLoadOutcome) {
        if self.workspace_load.active_path() != Some(path.as_path()) {
            return;
        }
        self.workspace_load = WorkspaceLoadLifecycle::Idle;
        self.import_progress = None;
        self.progress = None;
        match outcome {
            WorkspaceLoadOutcome::Template(Ok(prepared)) => {
                self.commit_prepared_template(path, *prepared);
            }
            WorkspaceLoadOutcome::Template(Err(error)) => {
                self.status =
                    StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, error);
            }
            WorkspaceLoadOutcome::Result(Ok(ordered)) => self.install_loaded_result(*ordered),
            WorkspaceLoadOutcome::Result(Err(error)) => {
                self.clear_generated_buffers();
                self.result = ResultState::Empty;
                self.status =
                    StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, error);
            }
            WorkspaceLoadOutcome::VaMMorphPair(Ok(imported)) => {
                let (control, detail) = *imported;
                self.morph_library.upsert_imported_vam(control);
                self.status = StatusMessage::with_detail(
                    TextKey::VaMMorphPairImported,
                    StatusTone::Success,
                    detail,
                );
            }
            WorkspaceLoadOutcome::VaMMorphPair(Err(detail)) => {
                self.status = StatusMessage::with_detail(
                    TextKey::VaMMorphPairRejected,
                    StatusTone::Error,
                    detail,
                );
            }
            WorkspaceLoadOutcome::DirectEditSource(Ok(prepared)) => {
                let prepared = *prepared;
                if let Err(detail) = self.install_direct_edit_output(
                    Arc::new(prepared.output),
                    prepared.overwrite_source.as_deref(),
                    prepared.missing_morphs,
                    prepared.resolved_morphs,
                ) {
                    self.release_failed_direct_edit_source();
                    self.status = StatusMessage::with_detail(
                        TextKey::GenerationFailed,
                        StatusTone::Error,
                        detail,
                    );
                }
            }
            WorkspaceLoadOutcome::DirectEditSource(Err(detail)) => {
                self.release_failed_direct_edit_source();
                self.status = StatusMessage::with_detail(
                    TextKey::VaMMorphPairRejected,
                    StatusTone::Error,
                    detail,
                );
            }
        }
        self.start_pending_direct_edit_source_if_ready();
    }

    #[cfg(test)]
    pub(crate) fn drive_pending_workspace_load(&mut self) {
        if let Some(job) = self.take_workspace_load_request() {
            let path = job.path().to_path_buf();
            let outcome = job.run();
            self.dispatch(Action::FinishWorkspaceLoad { path, outcome });
        }
    }

    pub(super) fn install_vam_template_from_neutral_provider(
        &mut self,
        provider: &Arc<VaMGeometryProvider>,
        bundle_path: PathBuf,
        provenance: VaMGeometryBaseProvenance,
    ) {
        if self.block_mutation_while_busy() {
            return;
        }
        let figure_sex = geometry_sex_to_figure_sex(provider.sex());
        let prepared = template_ingest::prepared_neutral_template_load(
            provider,
            bundle_path.clone(),
            figure_sex,
            provenance,
        );
        self.commit_prepared_template(bundle_path, prepared);
    }

    pub(super) fn commit_prepared_template(
        &mut self,
        path: PathBuf,
        prepared: PreparedTemplateLoad,
    ) {
        if let Err(detail) = self.try_commit_prepared_template(path, prepared) {
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, detail);
        }
    }

    fn try_commit_prepared_template(
        &mut self,
        path: PathBuf,
        prepared: PreparedTemplateLoad,
    ) -> Result<(), String> {
        let PreparedTemplateLoad {
            geometry,
            detected: detected_topology,
            detected_sex,
            sex_receipt,
            normalization,
            provider,
            provider_path,
            provider_provenance,
        } = prepared;
        let is_direct_g2 = normalization.is_some();
        let exact_g2_topology =
            matches_canonical_g2_topology(geometry.vertices.len(), &geometry.faces)
                .map_err(|error| error.to_string())?;
        let vam_derived_anchor = provider
            .as_deref()
            .is_some_and(VaMGeometryProvider::uses_vam_derived_anchor);
        let (eye_morph, curated_controls) = if should_request_g2_embedded_assets(
            detected_sex,
            exact_g2_topology || vam_derived_anchor,
        ) {
            let eye_morph = if vam_derived_anchor {
                embedded_g2f_eye_closed_morph_for_vam_anchor(&geometry)
            } else {
                embedded_g2f_eye_closed_morph(&geometry)
            }
            .map(Arc::new)
            .map_err(|error| error.to_string())?;
            let curated_pack =
                embedded_g2f_curated_morph_pack().map_err(|error| error.to_string())?;
            let curated_targets = if vam_derived_anchor {
                embedded_g2f_curated_morph_targets_for_vam_anchor(&geometry)
            } else {
                embedded_g2f_curated_morph_targets(&geometry)
            }
            .map_err(|error| error.to_string())?;
            let controls = curated_pack
                .morphs
                .iter()
                .zip(curated_targets)
                .map(|(entry, target)| {
                    MorphControl::new(
                        entry.id.clone(),
                        entry.display_label.clone(),
                        curated_morph_category(&entry.category),
                        MorphSource::Curated,
                        Arc::new(target),
                    )
                })
                .collect();
            (Some(eye_morph), controls)
        } else {
            (None, Vec::new())
        };

        let eyelid_conformation_plan = EyelidConformationPlan::build(&geometry).ok().map(Arc::new);
        self.workspace
            .load_template_geometry(geometry)
            .map_err(|error| error.to_string())?;
        self.eyelid_conformation_plan = eyelid_conformation_plan;
        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Info,
            "state",
            "template_eye_morph",
            &format!(
                "installed={}; detected_sex={:?}; exact_g2_topology={}; vam_derived_anchor={}",
                eye_morph.is_some(),
                detected_sex,
                exact_g2_topology,
                vam_derived_anchor,
            ),
        );
        self.workspace.eye_morph = eye_morph;

        self.install_cached_vam_controls();

        self.detected_template_topology = detected_topology;
        if is_direct_g2 {
            self.output_topology = OutputTopology::Daz;
        } else if let Some(output_topology) = detected_topology.output_topology() {
            self.output_topology = output_topology;
        }
        self.figure_sex = sex_receipt.effective;
        self.vam_geometry_provider = provider;
        self.vam_geometry_base_provenance = provider_provenance;
        self.vam_full_preview = None;
        if let Some(provider_path) = provider_path {
            self.vam_geometry_base_path = Some(provider_path);
        }

        self.clear_morph_reset_undo();
        self.morph_value_history.clear();
        self.morph_edit_open = None;
        self.morph_library.reset();
        self.morph_library
            .replace_source(MorphSource::Curated, curated_controls);
        let file_name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let mut receipts = vec![file_name.clone(), sex_receipt.detail()];
        if let Some(provenance) = self.vam_geometry_base_provenance {
            let mut source = crate::i18n::text(self.locale, provenance.text_key()).to_owned();
            if let Some(neutrality) = provenance.neutrality {
                source.push_str(&format!(" (RMS {:.3} cm)", neutrality.rms_cm));
            }
            receipts.push(source);
        }
        if let Some(receipt) = normalization {
            receipts.push(format!(
                "uniform scale x{:.8} (unit x{:.4}, height {:.6} -> {:.6} cm)",
                receipt.applied_uniform_scale,
                receipt.source_unit_scale,
                receipt.unit_normalized_height_cm,
                receipt.normalized_height_cm,
            ));
        }
        let detail = receipts.join("; ");
        self.template_path = Some(path);
        self.clear_output_destination();

        self.set_default_vam_output_path_if_empty();
        self.clear_alignment_history();
        self.active_tab = self.create_landing_tab();
        self.eye_state = EyeState::Open;
        self.eye_closure = 0.0;
        if let Some(transform) = self.suggested_alignment() {
            self.transform = transform;
        }
        self.complete_pairs = 0;
        self.invalidate_geometry(TextKey::PinsReset);

        self.template_install_generation = self.template_install_generation.wrapping_add(1);
        self.status = StatusMessage::with_detail(TextKey::TemplateLoaded, StatusTone::Info, detail);

        if self.vam_root.is_some() {
            self.queue_vam_catalog_refresh();
        }
        self.queue_builtin_morph_catalog_load();

        let slot = enrolled_cache_slot(self.figure_sex);
        self.enrolled_base_cache[slot] = match (
            self.vam_geometry_provider.as_ref(),
            self.vam_geometry_base_path.as_ref(),
            self.vam_geometry_base_provenance,
            self.workspace.template_geometry.as_ref(),
        ) {
            (Some(provider), Some(base_path), Some(provenance), Some(geometry)) => {
                Some(EnrolledBaseCache {
                    provider: Arc::clone(provider),
                    base_path: base_path.clone(),
                    provenance,
                    geometry: Arc::clone(geometry),
                    detected: self.detected_template_topology,
                })
            }
            _ => None,
        };
        Ok(())
    }

    pub(super) fn load_result(&mut self, path: PathBuf) {
        self.request_workspace_load(WorkspaceLoadKind::Result, path, TextKey::ResultLoading);
    }

    fn install_loaded_result(&mut self, ordered: OrderedObjMesh) {
        if let Err(error) = self.workspace.install_loaded_result(ordered) {
            self.clear_generated_buffers();
            self.result = ResultState::Empty;
            self.status = StatusMessage::with_detail(
                TextKey::GenerationFailed,
                StatusTone::Error,
                error.to_string(),
            );
            return;
        }
        self.clear_output_destination();
        self.clear_alignment_history();
        self.clear_morph_reset_undo();

        let Some(output) = self.workspace.result_output.as_ref() else {
            self.clear_generated_buffers();
            self.result = ResultState::Empty;
            self.status = StatusMessage::new(TextKey::ResultUnavailable, StatusTone::Error);
            return;
        };
        if let Err(error) = self.sculpt.load_applied(output) {
            self.clear_generated_buffers();
            self.result = ResultState::Empty;
            self.status = StatusMessage::with_detail(
                TextKey::GenerationFailed,
                StatusTone::Error,
                error.to_string(),
            );
            return;
        }
        self.result = ResultState::Ready {
            source_revision: self.revision,
            morph_available: self.workspace.eye_morph.is_some(),
        };
        self.morph_library.reset();
        self.fit_reference_value = Some(0.0);
        self.eye_closure = 0.0;
        self.sync_sculpt_preferences();
        self.install_sculpt_restore_basis();
        self.result_preview_phase = ResultPreviewPhase::Save;
        self.morph_preview_dirty = false;
        self.active_tab = Tab::Result;
        self.status = StatusMessage::new(TextKey::GenerationComplete, StatusTone::Success);
    }

    fn load_vam_morph_pair(&mut self, selected_path: PathBuf) {
        self.request_workspace_load(
            WorkspaceLoadKind::VaMMorphPair,
            selected_path,
            TextKey::VaMMorphPairLoading,
        );
    }

    #[cfg(test)]
    pub(super) fn prepare_vam_morph_pair(
        &self,
        selected_path: &Path,
    ) -> Result<(MorphControl, String), String> {
        Self::prepare_vam_morph_pair_job(&VaMPairLoadJob {
            path: selected_path.to_path_buf(),
            origin: MorphSource::LocalVaM,
            template_geometry: self.workspace.template_geometry.clone(),
            provider: self.vam_geometry_provider.clone(),
            figure_sex: self.figure_sex,
            allow_vertex_only_pose: false,
        })
    }

    pub(super) fn prepare_vam_morph_pair_job(
        job: &VaMPairLoadJob,
    ) -> Result<(MorphControl, String), String> {
        let selected_path = job.path.as_path();
        let (vmi_path, vmb_path) = selected_vam_pair_paths(selected_path)?;
        let vmi_bytes = crate::persistence::read_bounded_vam_pair_file(&vmi_path, "VMI")?;
        let vmb_bytes = crate::persistence::read_bounded_vam_pair_file(&vmb_path, "VMB")?;
        let canonical_path = fs::canonicalize(&vmi_path).unwrap_or_else(|_| vmi_path.clone());
        let id = format!("vam-pair:{}", canonical_path.to_string_lossy());
        Self::prepare_vam_morph_pair_payload(job, &vmi_path, id, &vmi_bytes, &vmb_bytes)
    }

    fn prepare_vam_morph_pair_payload(
        job: &VaMPairLoadJob,
        route_path: &Path,
        id: String,
        vmi_bytes: &[u8],
        vmb_bytes: &[u8],
    ) -> Result<(MorphControl, String), String> {
        let route = classify_vam_morph_route(route_path)
            .map_err(|error| format!("Ambiguous VaM morph route: {error}"))?
            .ok_or_else(|| {
                "The pair is not under the exact Custom/Atom/Person/Morphs/{female|male|female_genitalia|male_genitalia} route. VMI metadata cannot safely distinguish body-local from genital-local indices"
                    .to_owned()
            })?;
        let figure_sex = figure_sex_to_geometry_sex(job.figure_sex);
        if route.sex != figure_sex {
            return Err(format!(
                "This pair is routed for {:?}, but the current figure is {:?}. Switch figure sex or choose the matching canonical route",
                route.sex, figure_sex
            ));
        }
        let metadata = parse_vmi(vmi_bytes).map_err(|error| error.to_string())?;
        let raw_delta_count = vmb_raw_entry_count(vmb_bytes).map_err(|error| error.to_string())?;
        if let Some(declared) = metadata.num_deltas_value()
            && declared != raw_delta_count as u64
        {
            return Err(format!(
                "VMI declares {declared} deltas, but the paired VMB header declares {raw_delta_count}; the siblings may be mismatched or damaged"
            ));
        }
        if !job.allow_vertex_only_pose && metadata.is_pose_control_value() == Some(true) {
            return Err(
                "This VMI is a pose control. Vkit cannot evaluate VaM bone/controller semantics; choose an ordinary shape morph pair"
                    .to_owned(),
            );
        }
        if !job.allow_vertex_only_pose
            && metadata
                .formulas
                .as_ref()
                .is_some_and(|formulas| !formulas.is_empty())
        {
            return Err(
                "This VMI contains rig/formula targets. Vkit imports vertex-only shapes; bake the formulas to an ordinary VMB shape first"
                    .to_owned(),
            );
        }
        let minimum = metadata.min_value().unwrap_or(-1.0);
        let maximum = metadata.max_value().unwrap_or(1.0);
        if !minimum.is_finite()
            || !maximum.is_finite()
            || minimum > 0.0
            || maximum < 0.0
            || minimum > maximum
        {
            return Err(format!(
                "VMI slider range [{minimum}, {maximum}] is invalid for a zero-based shape morph"
            ));
        }
        let label = metadata
            .display_name()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| {
                route_path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .map(str::to_owned)
            })
            .ok_or_else(|| "The VMI pair has no usable display name or filename".to_owned())?;
        let (control, normalized_count, channel) = match route.routing {
            VmbVertexRouting::Full => {
                let geometry = job.template_geometry.as_deref().ok_or_else(|| {
                    "Select your VaM folder first so the canonical G2 base is ready, then import the body VMI/VMB pair"
                        .to_owned()
                })?;
                let expected_vertices = VAM_SHARED_BODY_VERTEX_COUNT as usize;
                if geometry.vertices.len() != expected_vertices {
                    return Err(format!(
                        "Body VMI/VMB import requires a canonical {expected_vertices}-vertex G2 template; the current template has {} vertices",
                        geometry.vertices.len()
                    ));
                }

                let full_vertices = match route.sex {
                    GeometrySex::Female => VAM_FEMALE_VERTEX_COUNT,
                    GeometrySex::Male => VAM_MALE_VERTEX_COUNT,
                };
                let sparse = decode_vmb_daz_cm_for_topology(
                    vmb_bytes,
                    full_vertices,
                    Some(VmbVertexRouting::Full),
                )
                .map_err(|error| error.to_string())?;
                let target_sparse = sparse
                    .iter()
                    .filter(|delta| (delta.vertex_index as usize) < expected_vertices)
                    .map(|delta| (delta.vertex_index, delta.delta_cm))
                    .collect::<Vec<_>>();

                let appended = sparse.len() - target_sparse.len();
                if target_sparse.is_empty() {
                    return Err(
                        "The VMB contains no nonzero deltas on the shared G2 body".to_owned()
                    );
                }
                let target = MorphTarget::from_sparse_deltas(
                    id.clone(),
                    expected_vertices,
                    &target_sparse,
                    geometry.faces.clone(),
                    MorphAuthoring::with_range(minimum, maximum),
                )
                .map_err(|error| error.to_string())?;
                (
                    MorphControl::new(id, label, MorphCategory::Head, job.origin, Arc::new(target)),
                    target_sparse.len(),
                    if appended == 0 { "body" } else { "shared-body" },
                )
            }
            VmbVertexRouting::Genitalia => {
                let provider = job.provider.as_deref().ok_or_else(|| {
                    "Genital-only VMI/VMB import requires a validated user-owned VaM full-body geometry provider"
                        .to_owned()
                })?;
                if provider.sex() != route.sex {
                    return Err(format!(
                        "The genital route is {:?}, but the active VaM provider is {:?}",
                        route.sex,
                        provider.sex()
                    ));
                }
                let full_vertex_count = provider.vam_basis().vertices.len();
                let sparse = decode_vmb_daz_cm_for_topology(
                    vmb_bytes,
                    full_vertex_count,
                    Some(VmbVertexRouting::Genitalia),
                )
                .map_err(|error| error.to_string())?;
                if sparse.is_empty() {
                    return Err("The VMB contains no nonzero genital-shape deltas".to_owned());
                }
                let target = Arc::new(
                    GenitalMorphTarget::new(
                        route.sex,
                        provider.receipt().clone(),
                        full_vertex_count,
                        sparse.clone(),
                    )
                    .map_err(|error| error.to_string())?,
                );
                (
                    MorphControl::new_genital_vam(
                        id,
                        label,
                        MorphCategory::Head,
                        target,
                        minimum,
                        maximum,
                        0.0,
                    )
                    .map_err(|error| error.to_string())?,
                    sparse.len(),
                    "genital",
                )
            }
        };
        let detail = format!(
            "{} — {} {channel} deltas (raw VMB rows: {raw_delta_count})",
            route_path
                .file_name()
                .map(|value| value.to_string_lossy())
                .unwrap_or_default(),
            normalized_count,
        );
        Ok((control, detail))
    }

    pub(super) fn prepare_direct_edit_source_job(
        job: &DirectEditLoadJob,
    ) -> Result<PreparedDirectEdit, String> {
        let geometry = job.template_geometry.as_deref().ok_or_else(|| {
            "Select the VaM folder first so the canonical G2 base is ready".to_owned()
        })?;
        let base = geometry
            .to_ordered_obj(None)
            .map_err(|error| error.to_string())?;
        let mut library = MorphLibrary::default();
        let mut missing_morphs = Vec::new();
        let overwrite_source = match job.source.kind {
            VaMEditSourceKind::MorphPair => {
                let (control, _) = Self::prepare_vam_morph_pair_job(&VaMPairLoadJob {
                    path: job.source.path.clone(),
                    origin: MorphSource::LocalVaM,
                    template_geometry: job.template_geometry.clone(),
                    provider: job.provider.clone(),
                    figure_sex: job.figure_sex,
                    allow_vertex_only_pose: true,
                })?;
                let id = control.id.clone();
                library.upsert_imported_vam(control);
                let _ = library.set_value(&id, 1.0);
                Some(job.source.path.clone())
            }
            VaMEditSourceKind::AppearancePreset => {
                let root = job.vam_root.as_deref().ok_or_else(|| {
                    "Select the VaM folder before loading an appearance preset".to_owned()
                })?;
                let recipe = parse_appearance_recipe(&job.source.path)?;
                if let Some(recipe_sex) = recipe.sex
                    && recipe_sex != job.figure_sex.skin_sex()
                {
                    return Err(format!(
                        "Appearance preset sex changed while loading: {:?}",
                        recipe_sex
                    ));
                }

                let mut builtin_bank = LiveBuiltinMorphs::new(root, job.figure_sex.skin_sex());
                let mut control_weights = BTreeMap::<String, f64>::new();
                let mut control_ids_by_identity = BTreeMap::<String, String>::new();
                let mut formulas_by_identity = BTreeMap::<String, Vec<(String, f64)>>::new();
                let mut formula_queue = VecDeque::<(String, f64, usize, Vec<String>)>::new();
                for entry in recipe.morphs {
                    let mut import_error = None;
                    let resolved = match resolve_morph_pair_asset(root, &entry.uid) {
                        Ok(Some(asset)) => Some(asset),

                        Ok(None) => job
                            .morph_index
                            .resolve(&entry.uid, job.figure_sex.skin_sex())
                            .and_then(morph_index_asset),
                        Err(error) => {
                            import_error = Some(error);
                            job.morph_index
                                .resolve(&entry.uid, job.figure_sex.skin_sex())
                                .and_then(morph_index_asset)
                        }
                    };
                    let imported = match resolved {
                        Some(ResolvedMorphPairAsset::Loose(path)) => {
                            let metadata =
                                crate::persistence::read_bounded_vam_pair_file(&path, "VMI")
                                    .ok()
                                    .and_then(|bytes| parse_vmi(&bytes).ok());
                            match Self::prepare_vam_morph_pair_job(&VaMPairLoadJob {
                                path,

                                origin: MorphSource::LocalVaM,
                                template_geometry: job.template_geometry.clone(),
                                provider: job.provider.clone(),
                                figure_sex: job.figure_sex,
                                allow_vertex_only_pose: true,
                            }) {
                                Ok((control, _)) => Some((control, metadata)),
                                Err(error) => {
                                    import_error = Some(error);
                                    None
                                }
                            }
                        }
                        Some(ResolvedMorphPairAsset::Packed(asset)) => {
                            let metadata = parse_vmi(&asset.vmi_bytes).ok();
                            let pair_job = VaMPairLoadJob {
                                path: asset.route_path.clone(),
                                origin: MorphSource::PackagedVaM,
                                template_geometry: job.template_geometry.clone(),
                                provider: job.provider.clone(),
                                figure_sex: job.figure_sex,
                                allow_vertex_only_pose: true,
                            };
                            match Self::prepare_vam_morph_pair_payload(
                                &pair_job,
                                &asset.route_path,
                                format!("vam-var:{}", asset.stable_id),
                                &asset.vmi_bytes,
                                &asset.vmb_bytes,
                            ) {
                                Ok((control, _)) => Some((control, metadata)),
                                Err(error) => {
                                    import_error = Some(error);
                                    None
                                }
                            }
                        }
                        None => None,
                    };
                    if let Some((control, metadata)) = imported {
                        let id = control.id.clone();
                        let mut identities = morph_identity_candidates(&entry.uid);
                        identities.extend(morph_identity_candidates(&entry.name));
                        let formulas = metadata
                            .as_ref()
                            .and_then(|document| document.formulas.as_deref())
                            .into_iter()
                            .flatten()
                            .filter_map(|formula| {
                                let target_type = formula.target_type.as_deref()?;
                                if !target_type.eq_ignore_ascii_case("MorphValue")
                                    && !target_type.eq_ignore_ascii_case("Morph")
                                {
                                    return None;
                                }
                                let target = formula.target.as_deref()?.trim();
                                let multiplier = formula.multiplier_value()?;
                                (!target.is_empty() && multiplier.is_finite() && multiplier != 0.0)
                                    .then(|| (target.to_owned(), multiplier))
                            })
                            .collect::<Vec<_>>();
                        if let Some(document) = metadata.as_ref() {
                            if let Some(value) = document.id.as_deref() {
                                identities.push(crate::vam_edit_sources::morph_identity(value));
                            }
                            if let Some(value) = document.display_name() {
                                identities.push(crate::vam_edit_sources::morph_identity(value));
                            }
                        }
                        identities.sort();
                        identities.dedup();
                        for identity in identities.iter().filter(|value| !value.is_empty()) {
                            control_ids_by_identity.insert(identity.clone(), id.clone());
                            formulas_by_identity.insert(identity.clone(), formulas.clone());
                        }
                        for (target, multiplier) in formulas {
                            formula_queue.push_back((
                                target,
                                f64::from(entry.value) * multiplier,
                                1,
                                identities.clone(),
                            ));
                        }
                        library.upsert_imported_vam(control);
                        *control_weights.entry(id).or_default() += f64::from(entry.value);
                        continue;
                    }

                    let mut identities = morph_identity_candidates(&entry.uid);
                    identities.extend(morph_identity_candidates(&entry.name));
                    let descriptor = job
                        .cached_morphs
                        .iter()
                        .find(|morph| {
                            let internal =
                                crate::vam_edit_sources::morph_identity(&morph.internal_name);
                            let label = crate::vam_edit_sources::morph_identity(&morph.label);
                            identities
                                .iter()
                                .any(|identity| identity == &internal || identity == &label)
                        })
                        .cloned()
                        .or_else(|| builtin_bank.resolve(&identities));
                    let Some(descriptor) = descriptor else {
                        let reason = import_error
                            .as_deref()
                            .unwrap_or("referenced VMI/VMB pair is unavailable");
                        missing_morphs.push(format!("{} [{}]: {reason}", entry.name, entry.uid));
                        continue;
                    };
                    let target = match MorphTarget::from_sparse_deltas(
                        format!("appearance:{}", descriptor.internal_name),
                        geometry.vertices.len(),
                        descriptor.deltas.as_slice(),
                        geometry.faces.clone(),
                        MorphAuthoring::vam(
                            descriptor.minimum,
                            descriptor.maximum,
                            descriptor.default,
                        ),
                    ) {
                        Ok(target) => target,
                        Err(error) => {
                            missing_morphs.push(format!("{} [{}]: {error}", entry.name, entry.uid));
                            continue;
                        }
                    };
                    let control = MorphControl::new(
                        format!("appearance:{}", descriptor.internal_name),
                        descriptor.label.clone(),
                        morph_category_from_vam(descriptor.category),
                        MorphSource::PackagedVaM,
                        Arc::new(target),
                    );
                    let id = control.id.clone();
                    for identity in identities.iter().filter(|value| !value.is_empty()) {
                        control_ids_by_identity.insert(identity.clone(), id.clone());
                    }
                    library.upsert_imported_vam(control);
                    *control_weights.entry(id).or_default() += f64::from(entry.value);
                }

                while let Some((target, weight, depth, ancestry)) = formula_queue.pop_front() {
                    if depth > 16 || !weight.is_finite() || weight == 0.0 {
                        continue;
                    }
                    let identity = crate::vam_edit_sources::morph_identity(&target);
                    if identity.is_empty() || ancestry.contains(&identity) {
                        missing_morphs
                            .push(format!("{target}: cyclic or invalid MorphValue dependency"));
                        continue;
                    }
                    if let Some(id) = control_ids_by_identity.get(&identity).cloned() {
                        *control_weights.entry(id).or_default() += weight;
                        if let Some(formulas) = formulas_by_identity.get(&identity) {
                            let mut next_ancestry = ancestry.clone();
                            next_ancestry.push(identity.clone());
                            for (child, multiplier) in formulas {
                                formula_queue.push_back((
                                    child.clone(),
                                    weight * multiplier,
                                    depth + 1,
                                    next_ancestry.clone(),
                                ));
                            }
                        }
                        continue;
                    }
                    let descriptor = job
                        .cached_morphs
                        .iter()
                        .find(|morph| {
                            crate::vam_edit_sources::morph_identity(&morph.internal_name)
                                == identity
                                || crate::vam_edit_sources::morph_identity(&morph.label) == identity
                        })
                        .cloned()
                        .or_else(|| builtin_bank.resolve(std::slice::from_ref(&identity)));
                    let Some(descriptor) = descriptor else {
                        missing_morphs.push(format!(
                            "{target}: referenced MorphValue dependency is unavailable"
                        ));
                        continue;
                    };
                    let target_id = format!("appearance:{}", descriptor.internal_name);
                    let morph_target = match MorphTarget::from_sparse_deltas(
                        target_id.clone(),
                        geometry.vertices.len(),
                        descriptor.deltas.as_slice(),
                        geometry.faces.clone(),
                        MorphAuthoring::vam(
                            descriptor.minimum,
                            descriptor.maximum,
                            descriptor.default,
                        ),
                    ) {
                        Ok(target) => target,
                        Err(error) => {
                            missing_morphs.push(format!("{target}: {error}"));
                            continue;
                        }
                    };
                    library.upsert_imported_vam(MorphControl::new(
                        target_id.clone(),
                        descriptor.label.clone(),
                        morph_category_from_vam(descriptor.category),
                        MorphSource::PackagedVaM,
                        Arc::new(morph_target),
                    ));
                    control_ids_by_identity.insert(identity, target_id.clone());
                    *control_weights.entry(target_id).or_default() += weight;
                }
                for (id, weight) in control_weights {
                    let _ = library.set_value(&id, weight as f32);
                }
                None
            }
        };
        let resolved_morphs = library.controls().len();
        let mut output = library
            .compose_onto(&base)
            .map_err(|error| error.to_string())?;
        if job.source.kind == VaMEditSourceKind::AppearancePreset {
            output = crate::look_head_filter::filter_appearance_to_head(&base, &output, geometry)?;
        }
        Ok(PreparedDirectEdit {
            output,
            overwrite_source,
            missing_morphs,
            resolved_morphs,
        })
    }

    pub(super) fn capture_edit_carry(&self) -> Option<CarriedEdit> {
        let morph_values = self.morph_library.snapshot_values();
        let sculpt = self
            .sculpt
            .displacement()
            .filter(|delta| delta.iter().flatten().any(|axis| *axis != 0.0));
        if sculpt.is_none() && !self.morph_library.has_modified_controls() {
            return None;
        }
        Some(CarriedEdit {
            morph_values,
            sculpt,
            eye_closure: self.eye_closure,
        })
    }

    pub(super) fn open_head_preset_file(&mut self, path: &std::path::Path) {
        let source = crate::vam_edit_sources::source_from_path(
            path,
            None,
            crate::vam_edit_sources::VaMEditSourceKind::AppearancePreset,
        );
        let id = source.stable_id.clone();
        if !self
            .vam_edit_sources
            .iter()
            .any(|known| known.stable_id == id)
        {
            self.vam_edit_sources.push(source);
        }
        self.select_vam_edit_source(&id);
    }

    pub(super) fn select_vam_edit_source(&mut self, id: &str) {
        let Some(source) = self
            .vam_edit_sources
            .iter()
            .find(|source| source.stable_id == id)
            .cloned()
        else {
            self.status = StatusMessage::with_detail(
                TextKey::VaMMorphPairRejected,
                StatusTone::Error,
                format!("VaM edit source is unavailable: {id}"),
            );
            return;
        };

        if !self.look_suits_current_figure(&source) {
            self.status = StatusMessage::with_detail(
                TextKey::LookBelongsToOtherFigure,
                StatusTone::Warning,
                source.label.clone(),
            );
            return;
        }

        if let Some(carry) = self.capture_edit_carry() {
            self.pending_edit_carry = Some(carry);
        }
        self.set_edit_source_mode(EditSourceMode::CustomMorph);
        self.selected_vam_edit_source_id = Some(source.stable_id.clone());
        self.pending_direct_edit_source = Some(source.clone());
        self.clear_generated_buffers();
        self.result = ResultState::Empty;
        self.start_pending_direct_edit_source_if_ready();
    }

    pub fn look_suits_current_figure(&self, source: &VaMEditSource) -> bool {
        match source.sex {
            Some(vkit_core::vam::SkinSex::Female) => self.figure_sex == FigureSex::Female,
            Some(vkit_core::vam::SkinSex::Male) => self.figure_sex == FigureSex::Male,
            Some(vkit_core::vam::SkinSex::Unknown) | None => true,
        }
    }

    pub(super) fn start_pending_direct_edit_source_if_ready(&mut self) {
        if self.edit_source_mode != EditSourceMode::CustomMorph {
            return;
        }
        let Some(source) = self.pending_direct_edit_source.as_ref() else {
            return;
        };
        if self.workspace_load.is_active() || self.workspace.template_geometry.is_none() {
            return;
        }
        if source.kind == VaMEditSourceKind::AppearancePreset
            && matches!(self.vam_morph_cache_status, VaMMorphCacheStatus::Loading)
        {
            return;
        }
        let source = self
            .pending_direct_edit_source
            .take()
            .expect("direct edit source checked above");
        self.request_workspace_load(
            WorkspaceLoadKind::DirectEditSource,
            source.path.clone(),
            TextKey::MorphLoading,
        );
        self.pending_direct_edit_source = Some(source);
    }

    pub(super) fn enter_direct_edit(&mut self, landing: Tab) {
        if self.edit_source_mode != EditSourceMode::CustomMorph {
            return;
        }
        self.begin_template_edit(false, landing);
    }

    pub(super) fn release_failed_direct_edit_source(&mut self) {
        self.selected_vam_edit_source_id = None;
        self.pending_direct_edit_source = None;
        self.begin_template_edit(true, Tab::Morph);
    }

    pub(super) fn select_base_face(&mut self) {
        if self.busy() {
            self.status = StatusMessage::new(TextKey::MorphLoading, StatusTone::Warning);
            return;
        }
        self.selected_vam_edit_source_id = None;
        self.pending_direct_edit_source = None;
        self.clear_morph_reset_undo();
        self.morph_value_history.clear();
        self.morph_edit_open = None;
        self.morph_library.reset();
        self.custom_morph_origin = None;
        self.morph_compare_blend = 1.0;
        self.begin_template_edit(true, Tab::Morph);
    }

    pub(super) fn begin_template_edit(&mut self, force_reinstall: bool, landing: Tab) {
        if self.busy() {
            self.status = StatusMessage::new(TextKey::MorphLoading, StatusTone::Warning);
            return;
        }
        let started = std::time::Instant::now();
        let mut install_ms = 0.0;
        if force_reinstall || !self.tab_available(Tab::Morph) {
            if self.pending_direct_edit_source.is_some() {
                self.status = StatusMessage::new(TextKey::MorphLoading, StatusTone::Warning);
                return;
            }

            let output = match self.workspace.template_ordered_obj() {
                Some(cached) => cached,
                None => {
                    let Some(template) = self.workspace.template_geometry.as_deref() else {
                        self.status =
                            StatusMessage::new(TextKey::TemplatePending, StatusTone::Warning);
                        return;
                    };
                    match template.to_ordered_obj(None) {
                        Ok(output) => Arc::new(output),
                        Err(error) => {
                            self.status = StatusMessage::with_detail(
                                TextKey::GenerationFailed,
                                StatusTone::Error,
                                error.to_string(),
                            );
                            return;
                        }
                    }
                }
            };
            let install_started = std::time::Instant::now();
            if let Err(detail) = self.install_direct_edit_output(output, None, Vec::new(), 0) {
                self.status = StatusMessage::with_detail(
                    TextKey::GenerationFailed,
                    StatusTone::Error,
                    detail,
                );
                return;
            }
            install_ms = install_started.elapsed().as_secs_f64() * 1000.0;
        }
        let compose_started = std::time::Instant::now();
        if let Err(detail) = self.compose_current_morphs(ResultPreviewPhase::Morph) {
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, detail);
            return;
        }
        self.active_tab = landing;
        if landing == Tab::Result
            && let Err(detail) = self.commit_save_preview()
        {
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, detail);
        }

        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Debug,
            "runtime",
            "direct_edit_timing",
            &format!(
                "install_ms={install_ms:.3}; compose_ms={:.3}; total_ms={:.3}",
                compose_started.elapsed().as_secs_f64() * 1000.0,
                started.elapsed().as_secs_f64() * 1000.0,
            ),
        );
    }

    pub(super) fn set_edit_source_mode(&mut self, mode: EditSourceMode) {
        if self.edit_source_mode == mode {
            return;
        }
        self.edit_source_mode = mode;
        self.active_tab = Tab::Alignment;
        self.custom_morph_origin = None;
        self.morph_compare_blend = 1.0;
        self.clear_generated_buffers();
        self.result = ResultState::Empty;
        self.clear_morph_reset_undo();
        self.morph_value_history.clear();
        self.morph_edit_open = None;
        self.morph_library.reset();
        match mode {
            EditSourceMode::ScanHead => {
                self.selected_vam_edit_source_id = None;
                self.pending_direct_edit_source = None;
            }
            EditSourceMode::CustomMorph => {
                self.scan_path = None;
                self.workspace.clear_scan();
                self.complete_pairs = 0;
                self.transform = ScanTransform::default();
                self.clear_alignment_history();
            }
        }
    }

    pub(super) fn unload_scan(&mut self) {
        if self.scan_path.is_none() && self.workspace.scan_source.is_none() {
            return;
        }
        self.scan_path = None;
        self.workspace.clear_scan();
        self.workspace.pins.clear_for_mesh_change();
        self.complete_pairs = 0;
        self.scan_symmetry = ScanSymmetry::Original;
        self.pending_symmetry_change = None;
        self.transform = ScanTransform::default();
        self.clear_alignment_history();
        self.placed_head = false;
        self.texture_project.invalidate_scan_projection();
        self.clear_generated_buffers();
        self.result = ResultState::Empty;
        self.status = StatusMessage::new(TextKey::ScanUnloaded, StatusTone::Info);
        self.settle_active_tab();
    }

    pub(super) fn install_direct_edit_output(
        &mut self,
        output: Arc<OrderedObjMesh>,
        source_path: Option<&Path>,
        missing_morphs: Vec<String>,
        resolved_morphs: usize,
    ) -> Result<(), String> {
        let carry = self.pending_edit_carry.take();
        self.clear_generated_buffers();
        self.clear_morph_reset_undo();
        self.morph_value_history.clear();
        self.morph_edit_open = None;
        self.morph_library.reset();
        let carried_morphs = carry
            .as_ref()
            .is_some_and(|carry| self.morph_library.restore_values(&carry.morph_values));
        self.custom_morph_origin = Some(Arc::new(output.vertices.clone()));
        self.workspace
            .install_result(output)
            .map_err(|error| error.to_string())?;
        self.morph_compare_blend = 1.0;
        self.result = ResultState::Ready {
            source_revision: self.revision,
            morph_available: self.workspace.eye_morph.is_some(),
        };
        self.fit_reference_value = Some(0.0);
        self.eye_closure = carry.as_ref().map_or(0.0, |carry| carry.eye_closure);
        self.begin_sculpt_stage()?;
        if let Some(delta) = carry.and_then(|carry| carry.sculpt) {
            match self.sculpt.graft_displacement(&delta) {
                Ok(_) => {}
                Err(_) => {
                    self.status =
                        StatusMessage::new(TextKey::SculptNotCarried, StatusTone::Warning);
                }
            }
        }
        self.result_preview_phase = ResultPreviewPhase::Sculpt;
        self.morph_preview_dirty = false;
        self.output_topology = OutputTopology::VaM;

        self.vam_export_display_name.clear();
        self.clear_output_destination();
        if let Some(path) = source_path.filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("vmi"))
        }) {
            self.output_path = path.to_string_lossy().into_owned();
        } else {
            self.set_default_vam_output_path_if_empty();
        }
        if !missing_morphs.is_empty() {
            let _ = crate::diagnostics::record(
                crate::diagnostics::Severity::Warning,
                "appearance",
                "missing_morphs",
                &format!(
                    "resolved={resolved_morphs}; missing={}; {}",
                    missing_morphs.len(),
                    missing_morphs.join(" | ")
                ),
            );
        }
        self.status = if missing_morphs.is_empty() || resolved_morphs > 0 {
            StatusMessage::new(TextKey::MorphPreviewUpdated, StatusTone::Success)
        } else {
            StatusMessage::new(TextKey::SourceMorphMissing, StatusTone::Warning)
        };
        if carried_morphs {
            self.morph_preview_dirty = true;
            if let Err(detail) = self.compose_current_morphs(ResultPreviewPhase::Morph) {
                self.morph_preview_dirty = false;
                self.result_preview_phase = ResultPreviewPhase::Sculpt;
                self.status = StatusMessage::with_detail(
                    TextKey::MorphPreviewUpdated,
                    StatusTone::Warning,
                    detail,
                );
            }
        }
        Ok(())
    }
}

const IMPORT_FAILURE_RESTATEMENTS: &[&str] = &[
    "failed to import mesh container: ",
    "failed to read mesh data: ",
    "native mesh import failed: ",
    "mesh could not be parsed: ",
];

fn import_failure_reason(error: &str) -> &str {
    let full = error.trim();
    let mut reason = full;
    while let Some(rest) = IMPORT_FAILURE_RESTATEMENTS
        .iter()
        .find_map(|prefix| reason.strip_prefix(prefix))
    {
        reason = rest.trim_start();
    }
    if reason.is_empty() { full } else { reason }
}

struct LiveBuiltinMorphs<'a> {
    root_path: &'a Path,
    sex: vkit_core::vam::SkinSex,

    opened: Option<Result<LiveBuiltinBank, ()>>,
}

struct LiveBuiltinBank {
    session: vkit_core::vam::BuiltinMorphSession,
    catalog: vkit_core::vam::MorphCatalog,
    whole_body_mask: Vec<bool>,
}

impl<'a> LiveBuiltinMorphs<'a> {
    fn new(root_path: &'a Path, sex: vkit_core::vam::SkinSex) -> Self {
        Self {
            root_path,
            sex,
            opened: None,
        }
    }

    fn resolve(&mut self, identities: &[String]) -> Option<CachedBuiltinMorph> {
        let root_path = self.root_path;
        let sex = self.sex;
        let bank = self
            .opened
            .get_or_insert_with(|| {
                let root = VaMRoot::open(root_path).map_err(|_| ())?;
                let session = vkit_core::vam::BuiltinMorphSession::open_for_sex(&root, sex)
                    .map_err(|_| ())?;
                let catalog = vkit_core::vam::MorphCatalog::from_session(&session);
                Ok(LiveBuiltinBank {
                    session,
                    catalog,
                    whole_body_mask: vec![true; VAM_SHARED_BODY_VERTEX_COUNT as usize],
                })
            })
            .as_ref()
            .ok()?;
        let wanted = |value: &str| {
            let identity = crate::vam_edit_sources::morph_identity(value);
            !identity.is_empty() && identities.iter().any(|candidate| candidate == &identity)
        };
        let descriptor = bank
            .catalog
            .entries()
            .iter()
            .find(|entry| wanted(&entry.internal_name) || wanted(&entry.label))?;
        let resolved = bank
            .session
            .load_resolved(&descriptor.source, &bank.whole_body_mask)
            .ok()?;
        Some(CachedBuiltinMorph {
            internal_name: resolved.internal_name,
            label: resolved.label,
            category: descriptor.category,
            minimum: descriptor.minimum,
            maximum: descriptor.maximum,
            default: descriptor.default,
            deltas: Arc::new(
                resolved
                    .sparse_deltas
                    .into_iter()
                    .map(|delta| (delta.vertex_index, delta.delta_cm))
                    .collect(),
            ),
            unsupported_formula_count: resolved.receipt.unsupported_formulas.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rejected_head_file_is_not_told_to_load_a_head_file() {
        let path = PathBuf::from("gltfpack-output.glb");
        let mut state = AppState::default();
        state.dispatch(Action::LoadScan(path.clone()));
        state.dispatch(Action::FinishScanImport {
            path,
            outcome: Err(
                "failed to import mesh container: native mesh import failed: invalid GLB: invalid glTF: extensionsRequired[0] = \"KHR_mesh_quantization\": Unsupported extension"
                    .to_owned(),
            ),
        });

        assert_eq!(state.status.key, TextKey::ScanLoadFailed);
        assert_eq!(state.status.tone, StatusTone::Error);
        assert_eq!(
            state.status.detail.as_deref(),
            Some(
                "invalid GLB: invalid glTF: extensionsRequired[0] = \"KHR_mesh_quantization\": Unsupported extension"
            )
        );
        assert!(state.scan_path.is_none());
    }

    #[test]
    fn an_unfamiliar_failure_reaches_the_status_line_whole() {
        for reason in [
            "the file ends before the header does",
            "failed to import mesh container: ",
        ] {
            assert_eq!(import_failure_reason(reason), reason.trim());
        }
    }
}
