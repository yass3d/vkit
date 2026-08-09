use super::*;

impl AppState {
    pub(super) fn provider_environment_path(sex: GeometrySex) -> Option<PathBuf> {
        env::var_os(match sex {
            GeometrySex::Female => VAM_FEMALE_BASE_ENV,
            GeometrySex::Male => VAM_MALE_BASE_ENV,
        })
        .map(PathBuf::from)
    }

    pub(super) fn vam_extraction_cache_dir() -> Option<PathBuf> {
        vkit_core::cache_root()
    }

    #[cfg(test)]
    fn opened_vam_root(&self) -> Option<VaMRoot> {
        VaMRoot::open(self.vam_root.as_deref()?).ok()
    }

    pub(super) fn provider_discovery_context(&self) -> ProviderDiscoveryContext {
        let explicit_base = self.vam_geometry_base_path.clone();
        ProviderDiscoveryContext {
            vam_root: self.vam_root.clone(),
            // The path and the provider are always installed together, so a loaded provider tells
            // us which figure the path holds.
            explicit_base_sex: explicit_base
                .as_ref()
                .and(self.vam_geometry_provider.as_deref())
                .map(VaMGeometryProvider::sex),
            explicit_base,
            locale: self.locale,
        }
    }

    fn discover_vam_geometry_base(
        &self,
        anchor: Option<&DazGeometry>,
        sex: GeometrySex,
    ) -> Result<DiscoveredGeometryBase, String> {
        self.provider_discovery_context().discover(anchor, sex)
    }

    fn resolve_provider_for_anchor(
        &self,
        anchor: &DazGeometry,
        sex: GeometrySex,
    ) -> Result<(Arc<VaMGeometryProvider>, PathBuf, VaMGeometryBaseProvenance), String> {
        self.provider_discovery_context()
            .resolve_provider(anchor, sex)
    }

    #[cfg(test)]
    pub(super) fn set_vam_geometry_base(&mut self, path: PathBuf) {
        let Some(anchor) = self.workspace.template_geometry.as_deref() else {
            self.status = StatusMessage::with_detail(
                TextKey::VaMGeometryBaseRejected,
                StatusTone::Error,
                "Load the canonical G2 template first; it is the clean-room DAZ anchor".to_owned(),
            );
            return;
        };
        if !is_obj_path(&path) {
            self.status = StatusMessage::with_detail(
                TextKey::VaMGeometryBaseRejected,
                StatusTone::Error,
                "Choose an OBJ. Provider bases cannot be GLB or FBX because their ordered labels are not an equivalent contract".to_owned(),
            );
            return;
        }
        let basis = match load_ordered_obj(&path) {
            Ok(basis) => basis,
            Err(error) => {
                self.status = StatusMessage::with_detail(
                    TextKey::VaMGeometryBaseRejected,
                    StatusTone::Error,
                    error.to_string(),
                );
                return;
            }
        };
        let Some(sex) = GeometrySex::shortlist_from_counts(basis.vertices.len(), basis.faces.len())
        else {
            self.status = StatusMessage::with_detail(
                TextKey::VaMGeometryBaseRejected,
                StatusTone::Error,
                format!(
                    "Unsupported VaM base counts: {} vertices, {} polygons",
                    basis.vertices.len(),
                    basis.faces.len()
                ),
            );
            return;
        };

        let neutrality = match self.opened_vam_root().map(|root| {
            load_or_extract_neutral_base(&root, sex, Self::vam_extraction_cache_dir().as_deref())
        }) {
            Some(Ok(handle)) => match validate_neutral_shared_prefix(&basis, &handle.base.mesh) {
                Ok(receipt) => Some(receipt),
                Err(error) => {
                    self.status = StatusMessage::with_detail(
                        TextKey::VaMGeometryBaseRejected,
                        StatusTone::Error,
                        format!(
                            "{}: {error}",
                            path.file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.to_string_lossy().into_owned())
                        ),
                    );
                    return;
                }
            },
            _ => None,
        };
        let provider = match VaMGeometryProvider::from_user_owned_pair(sex, anchor.clone(), basis) {
            Ok(provider) => Arc::new(provider),
            Err(error) => {
                self.status = StatusMessage::with_detail(
                    TextKey::VaMGeometryBaseRejected,
                    StatusTone::Error,
                    error.to_string(),
                );
                return;
            }
        };
        let provenance = VaMGeometryBaseProvenance {
            tier: if neutrality.is_some() {
                GeometryBaseTier::DazAnchorPair
            } else {
                GeometryBaseTier::UnverifiedUserObj
            },
            neutrality,
        };
        let figure_sex = geometry_sex_to_figure_sex(sex);
        let sex_changed = self.figure_sex != figure_sex;
        self.vam_geometry_base_path = Some(path.clone());
        self.vam_geometry_base_provenance = Some(provenance);
        self.vam_geometry_provider = Some(provider);
        self.vam_full_preview = None;
        self.figure_sex = figure_sex;
        if sex_changed {
            if !matches!(self.vam_catalog_status, VaMCatalogStatus::Unconfigured) {
                self.queue_vam_catalog_refresh();
            }
            self.queue_builtin_morph_catalog_load();
        }
        self.status = StatusMessage::with_detail(
            TextKey::VaMGeometryBaseReady,
            StatusTone::Success,
            format!(
                "{} · {}",
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned()),
                crate::i18n::text(self.locale, provenance.text_key())
            ),
        );
    }

    pub(super) fn ensure_vam_provider_for_current_template(&mut self) {
        let sex = figure_sex_to_geometry_sex(self.figure_sex);
        if self
            .vam_geometry_provider
            .as_deref()
            .is_some_and(|provider| provider.sex() == sex)
        {
            return;
        }
        let Some(anchor) = self.workspace.template_geometry.as_deref() else {
            return;
        };
        match self.resolve_provider_for_anchor(anchor, sex) {
            Ok((provider, path, provenance)) => {
                self.vam_geometry_provider = Some(provider);
                self.vam_geometry_base_path = Some(path);
                self.vam_geometry_base_provenance = Some(provenance);
                self.vam_full_preview = None;
            }
            Err(error) => {
                self.vam_geometry_provider = None;
                self.vam_geometry_base_provenance = None;
                self.vam_full_preview = None;
                self.status = StatusMessage::with_detail(
                    TextKey::VaMGeometryBaseRejected,
                    StatusTone::Warning,
                    error,
                );
            }
        }
    }

    pub(super) fn set_figure_sex(&mut self, value: FigureSex) {
        if self.figure_sex == value {
            return;
        }
        let previous_output_path = self.output_path.clone();
        let output_was_automatic = self.output_path_is_automatic_vam_default();
        self.figure_sex = value;

        // Cached previews were warped against the outgoing sex's UV mapping, and a
        // surviving skin selection keeps apply_default_skin from picking again once the
        // new sex's catalog lands.
        self.hair_preview_lru.clear();
        self.clear_hair_preview_selection();
        self.skin_preview_lru.clear();
        self.clear_skin_preview_selection();
        if output_was_automatic {
            self.output_path.clear();
            self.set_default_vam_output_path_if_empty();
        }

        if self.install_enrolled_base_from_cache(value) {
            if output_was_automatic {
                self.set_default_vam_output_path_if_empty();
            } else if !previous_output_path.trim().is_empty() {
                self.output_path = previous_output_path;
            }
            return;
        }
        if self.vam_root.is_some() {
            match self.enroll_vam_root_geometry(figure_sex_to_geometry_sex(value)) {
                Ok(()) => {
                    if output_was_automatic {
                        self.set_default_vam_output_path_if_empty();
                    } else if !previous_output_path.trim().is_empty() {
                        self.output_path = previous_output_path;
                    }
                    return;
                }
                Err(error) => {
                    self.status = StatusMessage::with_detail(
                        TextKey::VaMGeometryBaseRejected,
                        StatusTone::Error,
                        error,
                    );
                }
            }
        }
        if self
            .vam_geometry_provider
            .as_deref()
            .is_some_and(|provider| provider.sex() != figure_sex_to_geometry_sex(value))
        {
            self.vam_geometry_provider = None;
            self.vam_full_preview = None;
        }

        if !matches!(self.vam_catalog_status, VaMCatalogStatus::Unconfigured) {
            self.queue_vam_catalog_refresh();
        }
        self.queue_builtin_morph_catalog_load();
        if self.output_topology == OutputTopology::VaM {
            self.ensure_vam_provider_for_current_template();
        }
    }

    pub(super) fn set_vam_root(&mut self, path: PathBuf) {
        if path.as_os_str().is_empty() {
            return;
        }
        let previous_output_path = self.output_path.clone();
        let output_was_automatic = self.output_path_is_automatic_vam_default();
        let opened = VaMRoot::open(&path);
        self.vam_root = Some(
            opened
                .as_ref()
                .map_or_else(|_| path.clone(), |root| root.path().to_path_buf()),
        );

        self.enrolled_base_cache = [None, None];
        if output_was_automatic {
            self.output_path.clear();
        }

        self.vam_appearance_revision = self.vam_appearance_revision.saturating_add(1);
        self.pending_vam_work
            .retain(|request| !request.is_appearance_scan());
        self.vam_edit_query.clear();
        self.selected_vam_edit_source_id = None;
        self.pending_direct_edit_source = None;
        self.invalidate_vam_catalog_assets();
        self.vam_catalog_status = VaMCatalogStatus::Unconfigured;

        self.queue_builtin_morph_catalog_load();
        if let Err(error) = opened {
            self.status = StatusMessage::with_detail(
                TextKey::VaMGeometryBaseRejected,
                StatusTone::Error,
                error.to_string(),
            );
            return;
        }
        let sex = figure_sex_to_geometry_sex(self.figure_sex);
        if let Err(detail) = self.enroll_vam_root_geometry(sex) {
            self.status = StatusMessage::with_detail(
                TextKey::VaMGeometryBaseRejected,
                StatusTone::Error,
                detail,
            );
            return;
        }
        if output_was_automatic || previous_output_path.trim().is_empty() {
            self.set_default_vam_output_path_if_empty();
        } else {
            self.output_path = previous_output_path;
        }
    }

    fn install_enrolled_base_from_cache(&mut self, sex: FigureSex) -> bool {
        let Some(entry) = self.enrolled_base_cache[enrolled_cache_slot(sex)].clone() else {
            return false;
        };
        self.vam_geometry_provider = Some(Arc::clone(&entry.provider));
        self.vam_geometry_base_path = Some(entry.base_path.clone());
        self.vam_geometry_base_provenance = Some(entry.provenance);
        self.vam_full_preview = None;
        let prepared = PreparedTemplateLoad {
            geometry: (*entry.geometry).clone(),
            detected: entry.detected,
            detected_sex: Some(sex),
            sex_receipt: template_ingest::TemplateSexReceipt {
                detected: Some(sex),
                effective: sex,
                evidence: template_ingest::TemplateSexEvidence::Structural,
                conflicting_file_name: false,
                structural_ambiguity: false,
            },
            normalization: None,
            provider: Some(entry.provider),
            provider_path: Some(entry.base_path.clone()),
            provider_provenance: Some(entry.provenance),
        };
        self.commit_prepared_template(entry.base_path, prepared);
        true
    }

    fn enroll_vam_root_geometry(&mut self, sex: GeometrySex) -> Result<(), String> {
        let discovered = self.discover_vam_geometry_base(None, sex)?;
        let provenance = VaMGeometryBaseProvenance::from_discovery(&discovered);
        let provider = Arc::new(discovered.provider);
        self.vam_geometry_provider = Some(Arc::clone(&provider));
        self.vam_geometry_base_path = Some(discovered.base_path.clone());
        self.vam_geometry_base_provenance = Some(provenance);
        self.vam_full_preview = None;
        if discovered.tier == GeometryBaseTier::UnityBundle {
            self.install_vam_template_from_neutral_provider(
                &provider,
                discovered.base_path,
                provenance,
            );
        } else {
            self.load_template(discovered.base_path);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skin_preview::{
        SkinChannel, SkinCorner, SkinImage, SkinPreviewGeometry, SkinSurfaceMap, SkinTriangle,
    };

    fn worn_preview() -> Arc<SkinPreview> {
        let geometry = SkinPreviewGeometry::new(
            7,
            vec![SkinTriangle {
                source_triangle_id: 0,
                channel: SkinChannel::Face,
                corners: [
                    SkinCorner {
                        vertex_id: 0,
                        uv: [0.0, 0.0],
                    },
                    SkinCorner {
                        vertex_id: 1,
                        uv: [1.0, 0.0],
                    },
                    SkinCorner {
                        vertex_id: 2,
                        uv: [0.0, 1.0],
                    },
                ],
            }],
        )
        .unwrap();
        let image = Arc::new(SkinImage::solid(7, [128, 96, 64, 255]));
        Arc::new(SkinPreview {
            revision: 7,
            geometry: Arc::new(geometry),
            face: Arc::clone(&image),
            torso: Arc::clone(&image),
            sclera: Arc::clone(&image),
            iris: Arc::clone(&image),
            lacrimal: Arc::clone(&image),
            inner_mouth: Arc::clone(&image),
            teeth: Arc::clone(&image),
            gums: Arc::clone(&image),
            tongue: Arc::clone(&image),
            eyelashes: image,
            face_surface: SkinSurfaceMap::neutral(8),
            torso_surface: SkinSurfaceMap::neutral(9),
            mouth_surface_atlas: SkinSurfaceMap::neutral_mouth_atlas(10),
            sclera_surface: SkinSurfaceMap::neutral(11),
            iris_surface: SkinSurfaceMap::neutral(12),
            lacrimal_surface: SkinSurfaceMap::neutral(13),
            auxiliary_colors: [[255; 4]; 8],
            auxiliary_textured: [true; 8],
        })
    }

    #[test]
    fn a_sex_swap_takes_the_other_sexs_skin_off() {
        let preview = worn_preview();
        let mut state = AppState {
            selected_skin_id: Some("female-skin".to_owned()),
            skin_preview: Some(Arc::clone(&preview)),
            skin_preview_lru: VecDeque::from([("female-skin".to_owned(), preview)]),
            base_view_mode: BaseViewMode::Texture,
            ..AppState::default()
        };

        state.dispatch(Action::SetFigureSex(FigureSex::Male));

        // The preview was warped against the outgoing sex's UV map and the id points into
        // the outgoing sex's catalog; a survivor here also blocks apply_default_skin from
        // dressing the figure when the new catalog lands, and keeps the stale texture
        // eligible as a bake base.
        assert_eq!(state.selected_skin_id, None);
        assert!(state.skin_preview.is_none());
        assert!(state.skin_preview_lru.is_empty());
        assert_eq!(state.base_view_mode, BaseViewMode::Solid);
        assert!(!state.skin_base_available());
    }
}
