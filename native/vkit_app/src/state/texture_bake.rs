use super::*;

impl AppState {
    pub(super) fn add_texture_image(&mut self, path: PathBuf, source_mode: TextureSourceMode) {
        let layer_id = self
            .texture_project
            .add_image_layer(path.clone(), source_mode);
        let request_id = self.next_texture_request_id;
        self.next_texture_request_id = self.next_texture_request_id.saturating_add(1);
        self.pending_texture_work
            .push_back(TextureWorkRequest::Decode(TextureDecodeRequest {
                request_id,
                layer_id,
                path,
            }));
    }

    pub(super) fn ensure_scan_texture_layer(&mut self) {
        let has_diffuse = self
            .workspace
            .scan_materials
            .as_deref()
            .is_some_and(|materials| {
                materials
                    .materials
                    .iter()
                    .any(|material| material.diffuse_map.is_some())
            });
        if has_diffuse && self.workspace.scan_document.is_some() && self.scan_path.is_some() {
            self.texture_project.ensure_scan_layer(
                crate::i18n::text(self.locale, TextKey::ScanTextureLayer).to_owned(),
            );
        }
    }

    pub(super) fn request_texture_bake(&mut self, quality: TextureBakeQuality) {
        if self.texture_project.bake_loading {
            self.texture_project.bake_queued = Some(match self.texture_project.bake_queued {
                Some(TextureBakeQuality::Export) => TextureBakeQuality::Export,
                _ => quality,
            });
            return;
        }
        let Some(target) = self.workspace.result_output.as_ref().map(Arc::clone) else {
            self.texture_project.bake_error =
                Some("texture baking requires a generated G2 result".to_owned());
            return;
        };
        let Some(mapping) = self.vam_uv_mapping.as_ref().map(Arc::clone) else {
            self.texture_project.bake_error =
                Some("texture baking requires the G2 UV map from a VaM installation".to_owned());
            return;
        };
        let scan = self
            .workspace
            .scan_document
            .as_ref()
            .zip(self.workspace.scan_materials.as_ref())
            .zip(self.scan_path.as_ref())
            .map(
                |((document, materials), source_obj_path)| ScanTextureBakeSource {
                    document: Arc::clone(document),
                    materials: Arc::clone(materials),
                    source_obj_path: source_obj_path.clone(),
                    transform: self.current_scan_model_transform(),
                    symmetry_applied: self.scan_symmetry != ScanSymmetry::Original,
                },
            );
        let request_id = self.next_texture_request_id;
        let project_revision = self.texture_project.edit_revision();
        self.next_texture_request_id = self.next_texture_request_id.saturating_add(1);

        let resolution = match quality {
            TextureBakeQuality::Preview => {
                PREVIEW_BAKE_RESOLUTION.min(self.texture_project.resolution)
            }
            TextureBakeQuality::Export => self.texture_project.resolution,
        };
        let boundary_feather_pixels = u16::try_from(
            u32::from(self.texture_project.boundary_feather_pixels).saturating_mul(resolution)
                / self.texture_project.resolution.max(1),
        )
        .unwrap_or(u16::MAX);
        let face_mirror = self.face_mirror_map(&mapping);
        self.expected_texture_bake_request = Some((request_id, project_revision, resolution));
        self.texture_project.bake_loading = true;
        self.texture_project.bake_error = None;
        self.pending_texture_work
            .push_back(TextureWorkRequest::Bake(Box::new(
                crate::texture_project::TextureBakeRequest {
                    request_id,
                    project_revision,
                    layers: self.texture_project.layers.iter().map(Into::into).collect(),
                    face_mirror,
                    target,
                    mapping,
                    scan,
                    base_preview: self.skin_preview.as_ref().map(Arc::clone),
                    bake_base: self.texture_project.bake_base,
                    hide_skin_preview: self.texture_project.hide_vam_skin_preview,
                    neutral_base_rgb: self.g2_solid_color_rgb,
                    resolution,
                    boundary_feather_pixels,
                    cached_layer_rasters: self.texture_project.cached_layer_rasters(resolution),
                    cached_base_face: self.texture_project.cached_base_face(resolution),
                    base_face_source: self.selected_skin_face_diffuse(),
                    base_surface_sources: self.selected_skin_surface_maps(),
                },
            )));
    }

    pub(super) fn finish_texture_bake(
        &mut self,
        request_id: u64,
        outcome: Result<TextureBakedSet, String>,
    ) {
        let Some((expected_request_id, project_revision, resolution)) =
            self.expected_texture_bake_request
        else {
            return;
        };
        if expected_request_id != request_id {
            return;
        }
        self.expected_texture_bake_request = None;
        let succeeded = outcome.is_ok();
        let unblended = outcome.as_ref().map_or_else(
            |_| String::new(),
            |baked| {
                baked
                    .unblended_surface_channels
                    .iter()
                    .map(|channel| crate::texture_ui::channel_display(*channel))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        );
        let first_bake = self.texture_project.baked.is_none();
        let result_matches_request = match outcome.as_ref() {
            Ok(baked) => baked.source_revision == project_revision,
            Err(_) => true,
        };
        debug_assert!(result_matches_request);
        let request_is_current =
            result_matches_request && project_revision == self.texture_project.edit_revision();
        self.texture_project
            .finish_bake(outcome, request_is_current);
        if succeeded {
            if request_is_current {
                self.texture_project.baked_resolution = resolution;
            }
            if first_bake {
                self.base_view_mode = BaseViewMode::Texture;
            }
        }

        if !unblended.is_empty() {
            self.status = StatusMessage::with_detail(
                TextKey::SurfaceBaseUnblended,
                StatusTone::Warning,
                unblended,
            );
        }

        if let Some(queued) = self.texture_project.bake_queued.take() {
            self.request_texture_bake(queued);
        }
    }

    pub(crate) fn refresh_neutral_skin_preview(&mut self) {
        let Some(mapping) = self.vam_uv_mapping.as_ref() else {
            self.neutral_skin_preview = None;
            return;
        };
        let [red, green, blue] = self.g2_solid_color_rgb;
        let revision = crate::skin_preview::revision_domain::NEUTRAL_BASE
            | u64::from(red) << 24
            | u64::from(green) << 16
            | u64::from(blue) << 8;
        if self
            .neutral_skin_preview
            .as_ref()
            .is_some_and(|preview| preview.revision == revision)
        {
            return;
        }
        self.neutral_skin_preview =
            crate::texture_project::neutral_preview(revision, mapping, self.g2_solid_color_rgb)
                .ok()
                .map(Arc::new);
    }

    pub(super) fn edit_texture_layer(
        &mut self,
        id: u64,
        edit: impl FnOnce(&mut crate::texture_project::TextureLayer),
    ) {
        if let Some(layer) = self
            .texture_project
            .layers
            .iter_mut()
            .find(|layer| layer.id == id)
        {
            edit(layer);
            self.texture_project.mark_dirty();
        }
    }

    pub(super) fn warn_skin_preset_required(&mut self) {
        self.status = StatusMessage::new(TextKey::SkinPresetRequired, StatusTone::Warning);
        self.flash_attention(
            if self.viewport_tool_panel == Some(ViewportToolPanel::Skin) {
                AttentionTarget::SkinTextureMode
            } else {
                AttentionTarget::SkinPanel
            },
        );
    }

    pub(super) fn set_texture_hide_vam_skin(&mut self, value: bool) {
        self.texture_project.hide_vam_skin_preview = value;
        if value {
            self.refresh_neutral_skin_preview();
        }
        self.texture_project.mark_dirty();
    }

    fn selected_skin_face_diffuse(&self) -> Option<vkit_core::vam::AssetLocator> {
        if self.texture_project.bake_base != TextureBakeBase::CurrentSkin
            || self.texture_project.hide_vam_skin_preview
        {
            return None;
        }
        let selected = self.selected_skin_id.as_deref()?;
        let preset = self
            .vam_skin_presets
            .iter()
            .find(|preset| preset.stable_id == selected)?;
        preset
            .diffuse(vkit_core::vam::SkinRegion::Face)
            .or_else(|| preset.diffuse(vkit_core::vam::SkinRegion::Torso))
            .cloned()
    }

    fn selected_skin_surface_maps(
        &self,
    ) -> std::collections::BTreeMap<
        crate::texture_project::TextureChannel,
        vkit_core::vam::AssetLocator,
    > {
        use crate::texture_project::TextureChannel;

        let mut sources = std::collections::BTreeMap::new();
        if self.texture_project.bake_base != TextureBakeBase::CurrentSkin
            || self.texture_project.hide_vam_skin_preview
        {
            return sources;
        }
        let Some(preset) = self.selected_skin_id.as_deref().and_then(|selected| {
            self.vam_skin_presets
                .iter()
                .find(|preset| preset.stable_id == selected)
        }) else {
            return sources;
        };
        let face = preset.surface(vkit_core::vam::SkinRegion::Face);
        let torso = preset.surface(vkit_core::vam::SkinRegion::Torso);
        for (channel, locator) in [
            (TextureChannel::Normal, face.normal.or(torso.normal)),
            (TextureChannel::Specular, face.specular.or(torso.specular)),
            (TextureChannel::Glossiness, face.gloss.or(torso.gloss)),
        ] {
            if let Some(locator) = locator {
                sources.insert(channel, locator);
            }
        }
        sources
    }

    pub fn texture_composite_is_empty(&self) -> bool {
        !self
            .texture_project
            .layers
            .iter()
            .any(|layer| layer.visible)
    }

    pub(crate) fn settle_empty_texture_composite(&mut self) {
        if !self.texture_composite_is_empty() {
            return;
        }
        self.texture_project.discard_bake();
    }

    pub fn texture_export_bake_pending(&self) -> bool {
        self.texture_project.baked.is_none()
            || self.texture_project.dirty
            || self.texture_project.baked_resolution != self.texture_project.resolution
    }

    pub fn texture_auto_bake_ready(&self) -> bool {
        if self.workspace.result_output.is_none()
            || self.vam_uv_mapping.is_none()
            || self.texture_project.layers.is_empty()
            || self.texture_project.bake_refused_current_edit()
        {
            return false;
        }
        let visible = self
            .texture_project
            .layers
            .iter()
            .filter(|layer| layer.visible)
            .collect::<Vec<_>>();
        !visible.is_empty()
            && visible.iter().all(|layer| {
                let image_ready = layer.edited_image.is_some() || layer.image.is_some();
                match layer.source_mode {
                    TextureSourceMode::ScanMesh => {
                        self.workspace.scan_document.is_some()
                            && self.workspace.scan_materials.is_some()
                            && self.scan_path.is_some()
                    }

                    TextureSourceMode::LandmarkPins => {
                        image_ready && (layer.painted.is_some() || layer.landmark_warp_ready())
                    }

                    TextureSourceMode::MaterialUv => image_ready,
                }
            })
    }
}
