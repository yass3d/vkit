use super::*;

impl AppState {
    pub(super) fn invalidate_vam_catalog_assets(&mut self) {
        self.vam_catalog_progress = 0.0;
        self.vam_skin_presets.clear();
        self.vam_hair_presets.clear();
        self.shared_scalp = None;
        self.builtin_hair_scalps = Arc::new(Vec::new());
        self.builtin_scalp_textures = Arc::new(Vec::new());
        self.vam_edit_sources.clear();
        self.vam_uv_mapping = None;
        self.neutral_skin_preview = None;
        self.face_uv_rows = None;
        self.vam_uv_mapping_warning = None;
        self.texture_project.forget_layer_rasters();

        self.skin_preview_lru.clear();
        self.clear_skin_preview_selection();
    }

    fn invalidate_vam_morph_assets(&mut self) -> Result<(), String> {
        self.vam_cached_morphs.clear();
        self.vam_morph_faces = None;
        self.vam_morph_cache_status = VaMMorphCacheStatus::Unavailable;
        self.morph_library
            .replace_source(MorphSource::VaMBuiltin, Vec::new());
        self.morph_preview_dirty = true;
        if self.sculpt.is_ready() {
            self.restore_sculpt_preview()?;
        }
        Ok(())
    }

    pub(super) fn queue_vam_catalog_refresh(&mut self) {
        self.queue_vam_catalog_scan(false);
    }

    pub(super) fn force_vam_catalog_refresh(&mut self) {
        self.queue_vam_catalog_scan(true);
    }

    fn queue_vam_catalog_scan(&mut self, force_rescan: bool) {
        if !force_rescan
            && matches!(self.vam_catalog_status, VaMCatalogStatus::Indexing)
            && let Some(root) = self.vam_root.as_ref()
        {
            let signature = (
                root.clone(),
                self.figure_sex,
                self.workspace
                    .template_geometry
                    .as_ref()
                    .map(|geometry| Arc::as_ptr(geometry) as usize),
            );
            if self.vam_scan_signature.as_ref() == Some(&signature) {
                return;
            }
        }
        self.vam_appearance_revision = self.vam_appearance_revision.saturating_add(1);
        self.pending_vam_work
            .retain(|request| !request.is_appearance_scan());
        if let Some(root) = self.vam_root.clone() {
            self.pending_vam_work
                .push_back(VaMWorkRequest::ScanAppearance {
                    catalog_revision: self.vam_appearance_revision,
                    requested_root: root.clone(),
                    figure_sex: self.figure_sex.skin_sex(),
                    template_geometry: self.workspace.template_geometry.as_ref().map(Arc::clone),
                    force_rescan,
                });
            self.vam_scan_signature = Some((
                root,
                self.figure_sex,
                self.workspace
                    .template_geometry
                    .as_ref()
                    .map(|geometry| Arc::as_ptr(geometry) as usize),
            ));
            self.vam_catalog_status = VaMCatalogStatus::Indexing;
            self.vam_catalog_progress = 0.0;
            self.status = StatusMessage::new(TextKey::VaMIndexing, StatusTone::Info);
        } else {
            self.vam_catalog_status = VaMCatalogStatus::Unconfigured;
            self.vam_catalog_progress = 0.0;
        }
    }

    pub(super) fn report_vam_catalog_progress(&mut self, catalog_revision: u64, fraction: f32) {
        if catalog_revision != self.vam_appearance_revision
            || !matches!(self.vam_catalog_status, VaMCatalogStatus::Indexing)
        {
            return;
        }
        self.vam_catalog_progress = self.vam_catalog_progress.max(fraction.clamp(0.0, 0.99));
    }

    pub(super) fn queue_builtin_morph_catalog_load(&mut self) {
        self.vam_catalog_revision = self.vam_catalog_revision.saturating_add(1);
        self.pending_vam_work
            .retain(VaMWorkRequest::is_appearance_scan);
        let invalidation_error = self.invalidate_vam_morph_assets().err();

        if let (Some(template_geometry), Some(vam_root)) = (
            self.workspace.template_geometry.as_ref().map(Arc::clone),
            self.vam_root.clone(),
        ) {
            self.vam_morph_cache_status = VaMMorphCacheStatus::Loading;
            self.pending_vam_work
                .push_back(VaMWorkRequest::LoadMorphCache {
                    catalog_revision: self.vam_catalog_revision,
                    figure_sex: self.figure_sex.skin_sex(),
                    template_geometry,
                    vam_root,
                });
        } else {
            self.vam_morph_cache_status = VaMMorphCacheStatus::Unavailable;
        }
        if let Some(error) = invalidation_error {
            self.status =
                StatusMessage::with_detail(TextKey::MorphLoadFailed, StatusTone::Error, error);
        }
    }

    pub(super) fn finish_vam_catalog(
        &mut self,
        catalog_revision: u64,
        outcome: Result<VaMCatalogPayload, String>,
    ) {
        if catalog_revision != self.vam_appearance_revision {
            return;
        }
        match outcome {
            Ok(payload) => {
                let skin_count = payload.skin_presets.len();
                self.vam_catalog_progress = 1.0;
                self.vam_root = Some(payload.canonical_root);
                self.vam_skin_presets = payload.skin_presets;
                self.vam_hair_presets = payload.hair_presets;
                self.shared_scalp = payload.shared_scalp;
                self.builtin_hair_scalps = payload.builtin_hair_scalps;
                self.builtin_scalp_textures = payload.builtin_scalp_textures;
                self.vam_edit_sources = payload.edit_sources;
                self.vam_morph_index = payload.morph_index;
                let _ = crate::diagnostics::record(
                    crate::diagnostics::Severity::Info,
                    "vam",
                    "morph_index_built",
                    &format!(
                        "{} morphs indexed from {} packages",
                        self.vam_morph_index.len(),
                        self.vam_morph_index.archives_scanned()
                    ),
                );
                self.vam_package_index = payload.package_index;
                let counts = self.vam_package_index.counts();
                let _ = crate::diagnostics::record(
                    crate::diagnostics::Severity::Info,
                    "vam",
                    "package_index_built",
                    &format!(
                        "{} assets across {} packages ({} poses, {} appearances, {} scenes)",
                        self.vam_package_index.assets.len(),
                        self.vam_package_index.packages.len(),
                        counts
                            .get(&vkit_core::vam::PackageAssetKind::Pose)
                            .copied()
                            .unwrap_or_default(),
                        counts
                            .get(&vkit_core::vam::PackageAssetKind::Appearance)
                            .copied()
                            .unwrap_or_default(),
                        counts
                            .get(&vkit_core::vam::PackageAssetKind::Scene)
                            .copied()
                            .unwrap_or_default(),
                    ),
                );
                self.vam_morph_groups = payload.morph_groups;
                self.vam_morph_regions = payload.morph_regions;
                self.vam_uv_mapping = payload.uv_mapping;
                self.neutral_skin_preview = None;
                if self.texture_project.hide_vam_skin_preview {
                    self.refresh_neutral_skin_preview();
                }
                self.rebuild_face_uv_rows();

                self.texture_project.forget_layer_rasters();

                self.vam_uv_mapping_warning = if self.vam_uv_mapping.is_none() {
                    payload
                        .appearance_warnings
                        .iter()
                        .find(|warning| warning.starts_with("G2 UV mapping:"))
                        .cloned()
                } else {
                    None
                };
                self.vam_catalog_status = VaMCatalogStatus::Ready {
                    morph_count: self.vam_cached_morphs.len(),
                    skin_count,
                };

                self.status = StatusMessage::new(TextKey::SkinReady, StatusTone::Success);

                self.apply_default_skin();
            }
            Err(detail) => {
                self.vam_catalog_progress = 0.0;
                self.vam_skin_presets.clear();
                self.vam_hair_presets.clear();
                self.shared_scalp = None;
                self.builtin_hair_scalps = Arc::new(Vec::new());
                self.builtin_scalp_textures = Arc::new(Vec::new());
                self.vam_edit_sources.clear();
                self.vam_uv_mapping = None;
                self.neutral_skin_preview = None;
                self.face_uv_rows = None;
                self.vam_uv_mapping_warning = None;
                self.clear_skin_preview_selection();
                self.vam_catalog_status = VaMCatalogStatus::Failed {
                    detail: detail.clone(),
                };
                self.status =
                    StatusMessage::with_detail(TextKey::VaMFailed, StatusTone::Error, detail);
            }
        }
    }

    pub(super) fn finish_vam_morph_catalog(
        &mut self,
        catalog_revision: u64,
        outcome: Result<VaMMorphCatalogPayload, String>,
    ) {
        if catalog_revision != self.vam_catalog_revision {
            return;
        }
        match outcome {
            Ok(payload) => {
                let morph_count = payload.cached_morphs.len();
                self.vam_morph_faces = Some(payload.canonical_faces);
                self.install_vam_controls(&payload.cached_morphs);
                self.vam_cached_morphs = payload
                    .cached_morphs
                    .into_iter()
                    .map(|morph| (morph.internal_name.clone(), morph))
                    .collect();
                self.vam_morph_cache_status = VaMMorphCacheStatus::Ready { morph_count };
                if let VaMCatalogStatus::Ready { skin_count, .. } = self.vam_catalog_status {
                    self.vam_catalog_status = VaMCatalogStatus::Ready {
                        morph_count,
                        skin_count,
                    };
                }
            }
            Err(detail) => {
                self.vam_cached_morphs.clear();
                self.vam_morph_faces = None;
                self.vam_morph_cache_status = VaMMorphCacheStatus::Missing {
                    detail: detail.clone(),
                };
                self.morph_library
                    .replace_source(MorphSource::VaMBuiltin, Vec::new());

                self.status =
                    StatusMessage::with_detail(TextKey::MorphLoadFailed, StatusTone::Error, detail);
            }
        }
        self.start_pending_direct_edit_source_if_ready();
    }

    pub(super) fn clear_skin_preview_selection(&mut self) {
        self.next_skin_request_id = self.next_skin_request_id.saturating_add(1);
        self.expected_skin_request = None;
        self.pending_skin_work = None;
        self.selected_skin_id = None;
        self.skin_preview_loading = false;
        self.skin_preview = None;
        self.base_view_mode = BaseViewMode::Solid;
        self.invalidate_texture_bake_for_skin_change();
    }

    pub fn skin_base_available(&self) -> bool {
        self.skin_preview.is_some() && !self.texture_project.hide_vam_skin_preview
    }

    fn invalidate_texture_bake_for_skin_change(&mut self) {
        if self.texture_project.layers.is_empty() {
            return;
        }
        self.texture_project.mark_dirty();
    }

    pub(super) fn face_mirror_map(
        &mut self,
        mapping: &Arc<vkit_core::vam::G2UvMapping>,
    ) -> Option<Arc<vkit_core::texture_mirror::FaceMirrorMap>> {
        if let Some(cached) = self.face_mirror_map.clone() {
            return Some(cached);
        }

        let template = self.workspace.template_geometry.as_ref()?;
        match vkit_core::texture_mirror::FaceMirrorMap::build(mapping, &template.vertices) {
            Ok(map) if map.is_usable() => {
                let map = Arc::new(map);
                self.face_mirror_map = Some(Arc::clone(&map));
                Some(map)
            }

            Ok(_) | Err(_) => None,
        }
    }

    pub(super) fn apply_default_skin(&mut self) {
        if self.selected_skin_id.is_some() {
            return;
        }

        let starred = self.default_skin_id.clone();
        let Some(wanted) = starred.clone().or_else(|| self.last_skin_id.clone()) else {
            return;
        };
        let sex = self.figure_sex.skin_sex();
        match self
            .vam_skin_presets
            .iter()
            .find(|preset| preset.stable_id == wanted)
        {
            Some(preset) if preset.sex.is_compatible_with(sex, false) => {
                self.select_vam_skin(Some(&wanted));
                return;
            }
            Some(_) => {}
            None => {}
        }
        let missing_star = (starred.is_some()
            && !self
                .vam_skin_presets
                .iter()
                .any(|preset| preset.stable_id == wanted))
        .then_some(wanted);

        let fallback = self
            .vam_skin_presets
            .iter()
            .filter(|preset| preset.sex.is_compatible_with(sex, false))
            .min_by_key(|preset| {
                let label = preset.label.to_ascii_lowercase();
                (!label.contains("base"), label)
            })
            .map(|preset| preset.stable_id.clone());
        if let Some(id) = fallback {
            self.select_vam_skin(Some(&id));
        }
        if let Some(wanted) = missing_star {
            self.status =
                StatusMessage::with_detail(TextKey::DefaultSkinMissing, StatusTone::Error, wanted);
        }
    }

    pub(super) fn select_vam_skin(&mut self, preset_id: Option<&str>) {
        let Some(preset_id) = preset_id.filter(|id| !id.trim().is_empty()) else {
            self.clear_skin_preview_selection();
            return;
        };
        if self.selected_skin_id.as_deref() == Some(preset_id)
            && (self.skin_preview.is_some() || self.skin_preview_loading)
        {
            return;
        }
        let Some(preset) = self
            .vam_skin_presets
            .iter()
            .find(|preset| {
                preset.stable_id == preset_id
                    && preset
                        .sex
                        .is_compatible_with(self.figure_sex.skin_sex(), false)
            })
            .cloned()
        else {
            self.status = StatusMessage::with_detail(
                TextKey::SkinLoadFailed,
                StatusTone::Error,
                format!("VaM skin preset is unavailable: {preset_id}"),
            );
            return;
        };
        let Some(mapping) = self.vam_uv_mapping.as_ref().map(Arc::clone) else {
            let detail = self.vam_uv_mapping_warning.clone().unwrap_or_else(|| {
                crate::i18n::text(self.locale, TextKey::VaMUvMappingUnavailable).to_owned()
            });
            self.status =
                StatusMessage::with_detail(TextKey::SkinLoadFailed, StatusTone::Error, detail);
            return;
        };
        if let Some(position) = self
            .skin_preview_lru
            .iter()
            .position(|(cached_id, _)| cached_id == &preset.stable_id)
        {
            let (cached_id, preview) = self.skin_preview_lru.remove(position).expect("valid index");
            self.skin_preview_lru
                .push_back((cached_id, Arc::clone(&preview)));
            self.next_skin_request_id = self.next_skin_request_id.saturating_add(1);
            self.expected_skin_request = None;
            self.pending_skin_work = None;
            self.selected_skin_id = Some(preset.stable_id.clone());
            self.skin_preview_loading = false;
            self.skin_preview = Some(preview);
            self.base_view_mode = BaseViewMode::Texture;
            self.invalidate_texture_bake_for_skin_change();
            self.status = StatusMessage::new(TextKey::SkinReady, StatusTone::Info);
            return;
        }
        let request_id = self.next_skin_request_id;
        self.next_skin_request_id = self.next_skin_request_id.saturating_add(1);
        self.expected_skin_request = Some(request_id);
        self.selected_skin_id = Some(preset.stable_id.clone());

        self.skin_preview_loading = true;
        self.base_view_mode = BaseViewMode::Texture;
        self.pending_skin_work = Some(SkinPreviewRequest {
            request_id,
            mapping,
            preset,
        });
        self.status = StatusMessage::new(TextKey::SkinLoading, StatusTone::Info);
    }

    pub(super) fn finish_vam_skin(
        &mut self,
        request_id: u64,
        preset_id: &str,
        outcome: Result<Arc<SkinPreview>, String>,
    ) {
        if self.expected_skin_request != Some(request_id)
            || self.selected_skin_id.as_deref() != Some(preset_id)
        {
            return;
        }
        self.expected_skin_request = None;
        self.skin_preview_loading = false;
        match outcome {
            Ok(preview) => {
                self.skin_preview_lru
                    .retain(|(cached_id, _)| cached_id != preset_id);
                self.skin_preview_lru
                    .push_back((preset_id.to_owned(), Arc::clone(&preview)));
                while self.skin_preview_lru.len() > 3 {
                    self.skin_preview_lru.pop_front();
                }
                self.skin_preview = Some(preview);
                self.base_view_mode = BaseViewMode::Texture;
                self.status = StatusMessage::new(TextKey::SkinReady, StatusTone::Info);
            }
            Err(detail) => {
                self.skin_preview = None;
                self.status =
                    StatusMessage::with_detail(TextKey::SkinLoadFailed, StatusTone::Error, detail);
            }
        }

        self.invalidate_texture_bake_for_skin_change();
    }

    pub(super) fn install_cached_vam_controls(&mut self) {
        if self.vam_cached_morphs.is_empty()
            || self
                .morph_library
                .controls()
                .iter()
                .any(|control| control.source == MorphSource::VaMBuiltin)
        {
            return;
        }
        let morphs: Vec<CachedBuiltinMorph> = self.vam_cached_morphs.values().cloned().collect();
        self.install_vam_controls(&morphs);
    }

    pub(super) fn install_vam_controls(&mut self, morphs: &[CachedBuiltinMorph]) {
        let Some(topology_template) = self.workspace.eye_morph.as_deref() else {
            return;
        };
        let controls = morphs
            .iter()
            .map(|morph| {
                let contract = unresolved_slider_contract(
                    topology_template,
                    format!("vam-cache:{}", morph.internal_name),
                    morph.minimum,
                    morph.maximum,
                    morph.default,
                )
                .expect("validated cache slider contract");
                let mut control = MorphControl::new_unresolved(
                    morph.internal_name.clone(),
                    morph.label.clone(),
                    morph_category_from_vam(morph.category),
                    MorphSource::VaMBuiltin,
                    contract,
                );
                control.unsupported_formula_count = morph.unsupported_formula_count;
                control
            })
            .collect();
        self.morph_library
            .replace_source(MorphSource::VaMBuiltin, controls);
    }

    fn morph_loading_status(&mut self, control_id: &str) -> StatusMessage {
        let remaining = self.morph_library.loading_count();
        self.morph_resolution_peak = self.morph_resolution_peak.max(remaining + 1);
        if remaining == 0 {
            self.morph_resolution_peak = 0;
            return StatusMessage::new(TextKey::MorphPreviewUpdated, StatusTone::Success);
        }
        let total = self.morph_resolution_peak;
        let done = total.saturating_sub(remaining);
        let name = self
            .morph_library
            .label_of(control_id)
            .unwrap_or(control_id)
            .to_owned();
        StatusMessage::with_detail(
            TextKey::MorphResolving,
            StatusTone::Info,
            format!("{name} ({done}/{total})"),
        )
    }

    pub(super) fn finish_vam_morph(
        &mut self,
        catalog_revision: u64,
        geometry_revision: u64,
        control_id: &str,
        outcome: Result<VaMMorphPayload, String>,
    ) {
        if catalog_revision != self.vam_catalog_revision {
            return;
        }
        if geometry_revision != self.revision {
            let _ = self.morph_library.mark_resolution_deferred(control_id);
            return;
        }
        match outcome {
            Ok(payload) => match self
                .morph_library
                .install_resolved_target(control_id, payload.target)
            {
                Ok(true) => {
                    let _ = self.morph_library.set_unsupported_formula_count(
                        control_id,
                        payload.unsupported_formula_count,
                    );
                    self.morph_preview_dirty = true;
                    let recomposed = match self.result_preview_phase {
                        ResultPreviewPhase::Morph => {
                            self.compose_current_morphs(ResultPreviewPhase::Morph)
                        }
                        ResultPreviewPhase::Save => self
                            .compose_current_morphs(ResultPreviewPhase::Save)
                            .map(|()| self.sculpt.mark_applied()),
                        ResultPreviewPhase::Empty | ResultPreviewPhase::Sculpt => Ok(()),
                    };
                    if let Err(error) = recomposed {
                        self.status = StatusMessage::with_detail(
                            TextKey::MorphLoadFailed,
                            StatusTone::Error,
                            error,
                        );
                    } else {
                        self.status = self.morph_loading_status(control_id);
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    let _ = self.morph_library.mark_resolution_failed(control_id);
                    self.status = StatusMessage::with_detail(
                        TextKey::MorphLoadFailed,
                        StatusTone::Error,
                        error.to_string(),
                    );
                }
            },
            Err(detail) => {
                let _ = self.morph_library.mark_resolution_failed(control_id);
                self.status =
                    StatusMessage::with_detail(TextKey::MorphLoadFailed, StatusTone::Error, detail);
            }
        }
    }
}
