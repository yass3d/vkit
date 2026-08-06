use super::*;
use vkit_core::vam::SkinSex;

impl AppState {
    pub(super) fn request_export(&mut self) {
        let Some(source_revision) = self.ready_export_source_revision() else {
            return;
        };

        self.set_default_vam_output_path_if_empty();
        let Some(path) = self.normalized_output_path() else {
            self.pending_dialog = Some(DialogIntent::ChooseOutput);
            self.active_tab = Tab::Result;
            self.status = StatusMessage::with_detail(
                TextKey::Ready,
                StatusTone::Info,
                format!("Choose a .{MORPH_EXPORT_EXTENSION} destination before exporting"),
            );
            return;
        };
        if !self.output_path_matches_format(&path) {
            self.status = StatusMessage::with_detail(
                TextKey::ExportFailed,
                StatusTone::Warning,
                format!(
                    "Output path must use the .{MORPH_EXPORT_EXTENSION} extension for                      {MORPH_EXPORT_LABEL} export"
                ),
            );
            return;
        }
        let existing = self
            .export_destination_paths(&path)
            .into_iter()
            .filter(|candidate| candidate.exists())
            .collect::<Vec<_>>();
        if !existing.is_empty() {
            self.pending_overwrite_path = Some(path.clone());
            self.active_tab = Tab::Result;
            self.status = StatusMessage::with_detail(
                TextKey::Ready,
                StatusTone::Warning,
                format!(
                    "Output already exists: {}. Confirm overwrite to continue.",
                    existing
                        .iter()
                        .map(|candidate| candidate.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
            return;
        }
        self.queue_export(source_revision);
    }

    pub(super) fn confirm_overwrite(&mut self) {
        let Some(confirmed_path) = self.pending_overwrite_path.take() else {
            return;
        };
        let Some(source_revision) = self.ready_export_source_revision() else {
            return;
        };
        let Some(current_path) = self.normalized_output_path() else {
            self.status = StatusMessage::new(TextKey::ResultUnavailable, StatusTone::Warning);
            return;
        };
        if current_path != confirmed_path || !self.output_path_matches_format(&current_path) {
            self.status = StatusMessage::with_detail(
                TextKey::ExportFailed,
                StatusTone::Warning,
                "Output destination changed before overwrite confirmation",
            );
            return;
        }
        self.queue_export(source_revision);
    }

    fn ready_export_source_revision(&mut self) -> Option<u64> {
        let ResultState::Ready {
            source_revision, ..
        } = self.result
        else {
            self.status = StatusMessage::new(TextKey::ResultUnavailable, StatusTone::Warning);
            return None;
        };
        if source_revision != self.revision || !self.export_ready() {
            self.status = StatusMessage::new(TextKey::ResultUnavailable, StatusTone::Warning);
            return None;
        }
        Some(source_revision)
    }

    fn queue_export(&mut self, source_revision: u64) {
        self.pending_overwrite_path = None;
        self.export = ExportLifecycle::Queued {
            revision: source_revision,
        };
        self.progress = Some(Progress::new(JobStage::Export, 0.0));
        self.active_tab = Tab::Result;
        self.status = StatusMessage::new(TextKey::ExportStarted, StatusTone::Info);
    }

    fn normalized_output_path(&self) -> Option<PathBuf> {
        let value = self.output_path.trim();
        if value.is_empty() {
            return None;
        }
        let path = PathBuf::from(value);
        if path.is_absolute() {
            Some(path)
        } else {
            std::env::current_dir().ok().map(|root| root.join(path))
        }
    }

    fn default_vam_output_path(&self) -> Option<PathBuf> {
        let root = VaMRoot::open(self.vam_root.as_deref()?).ok()?;
        let group_folder = sanitize_path_component(&self.vam_export_group, "Vkit");
        let file_stem = sanitize_path_component(&self.vam_export_display_name, "Vkit_Morph");
        Some(
            root.morph_output_directory(figure_sex_to_geometry_sex(self.figure_sex))
                .join(group_folder)
                .join(format!("{file_stem}.vmi")),
        )
    }

    pub(super) fn set_default_vam_output_path_if_empty(&mut self) {
        if !self.output_path.trim().is_empty() {
            return;
        }
        if let Some(path) = self.default_vam_output_path() {
            self.output_path = path.to_string_lossy().into_owned();
        }
    }

    pub(super) fn output_path_is_automatic_vam_default(&self) -> bool {
        self.default_vam_output_path()
            .is_some_and(|path| Path::new(self.output_path.trim()) == path)
    }

    pub(super) fn refresh_automatic_vam_output_path(&mut self, was_automatic: bool) {
        if was_automatic || self.output_path.trim().is_empty() {
            self.output_path.clear();
            self.pending_overwrite_path = None;
            self.set_default_vam_output_path_if_empty();
        }
    }

    fn output_path_matches_format(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case(MORPH_EXPORT_EXTENSION))
    }

    pub(super) fn export_destination_paths(&self, primary: &Path) -> Vec<PathBuf> {
        let mut vmb = primary.to_path_buf();
        vmb.set_extension("vmb");
        vec![primary.to_path_buf(), vmb]
    }

    pub fn can_save_textures(&self) -> bool {
        !self.texture_project.layers.is_empty()
            && !self.texture_project.export_prefix.trim().is_empty()
            && !self.texture_export_bake_pending()
            && self
                .texture_project
                .default_export_directory(self.vam_root.as_deref(), self.figure_sex)
                .is_some()
    }

    pub(super) fn save_textures(&mut self) {
        if self.block_mutation_while_busy() {
            return;
        }
        let (Some(baked), Some(directory)) = (
            self.texture_project.baked.clone(),
            self.texture_project
                .default_export_directory(self.vam_root.as_deref(), self.figure_sex),
        ) else {
            self.status = StatusMessage::with_detail(
                TextKey::ExportFailed,
                StatusTone::Warning,
                "Bake the textures and choose a VaM folder before saving".to_owned(),
            );
            return;
        };
        let snapshot = crate::texture_project::TextureExportSnapshot {
            directory,
            prefix: self.texture_project.export_prefix.clone(),
            pbr_convention: self.texture_project.output_pbr,
            images: baked.images.clone(),
        };

        let existing = crate::workflow::texture_bundle_paths(&snapshot)
            .into_iter()
            .filter(|path| path.exists())
            .collect::<Vec<_>>();
        if !existing.is_empty() {
            self.pending_texture_overwrite = existing;
            return;
        }
        self.write_texture_snapshot(&snapshot);
    }

    pub(super) fn write_textures(&mut self) {
        self.pending_texture_overwrite.clear();
        let (Some(baked), Some(directory)) = (
            self.texture_project.baked.clone(),
            self.texture_project
                .default_export_directory(self.vam_root.as_deref(), self.figure_sex),
        ) else {
            return;
        };
        let snapshot = crate::texture_project::TextureExportSnapshot {
            directory,
            prefix: self.texture_project.export_prefix.clone(),
            pbr_convention: self.texture_project.output_pbr,
            images: baked.images.clone(),
        };
        self.write_texture_snapshot(&snapshot);
    }

    pub(super) fn raise_toast(&mut self, key: TextKey, tone: StatusTone) {
        self.toast_serial = self.toast_serial.wrapping_add(1);
        self.toast = Some(Toast {
            key,
            tone,
            serial: self.toast_serial,
        });
    }

    fn write_texture_snapshot(&mut self, snapshot: &crate::texture_project::TextureExportSnapshot) {
        match crate::workflow::write_texture_bundle(snapshot) {
            Ok(paths) => {
                let detail = paths
                    .first()
                    .and_then(|path| path.parent())
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_default();
                self.status = StatusMessage::with_detail(
                    TextKey::ExportComplete,
                    StatusTone::Success,
                    detail,
                );
                self.raise_toast(TextKey::TextureSaveSection, StatusTone::Success);
            }
            Err(error) => {
                self.status = StatusMessage::with_detail(
                    TextKey::ExportFailed,
                    StatusTone::Warning,
                    error.to_string(),
                );
                self.raise_toast(TextKey::ExportFailed, StatusTone::Error);
            }
        }
    }

    pub(super) fn report_export_progress(&mut self, source_revision: u64, fraction: f32) {
        if self.export.writing_revision() != Some(source_revision) || !fraction.is_finite() {
            return;
        }
        let floor = self.progress.map_or(0.0, |current| current.fraction);
        self.progress = Some(Progress::new(JobStage::Export, fraction.max(floor)));
    }

    pub(super) fn finish_export(&mut self, source_revision: u64, outcome: ExportOutcome) {
        if self.export.writing_revision() != Some(source_revision) {
            return;
        }
        self.export = ExportLifecycle::Idle;
        self.progress = None;
        self.pending_overwrite_path = None;
        if !matches!(
            self.result,
            ResultState::Ready { source_revision: current, .. }
                if current == source_revision && current == self.revision
        ) {
            self.status = StatusMessage::new(TextKey::ResultStale, StatusTone::Warning);
            return;
        }
        match outcome {
            ExportOutcome::Success { receipt } => {
                if let Some(path) = receipt.primary_path() {
                    self.output_path = path.to_string_lossy().into_owned();
                }
                self.active_tab = Tab::Result;
                self.raise_toast(TextKey::MorphSave, StatusTone::Success);

                let hint = match morph_pair_sex(&receipt) {
                    Some(SkinSex::Male) => TextKey::MorphSavedReloadMale,
                    _ => TextKey::MorphSavedReloadFemale,
                };
                self.last_morph_save_hint = Some(hint);
                self.status = StatusMessage::with_detail(
                    TextKey::ExportComplete,
                    StatusTone::Warning,
                    crate::i18n::text(self.locale, hint).to_owned(),
                );
            }
            ExportOutcome::Failed(detail) => {
                self.active_tab = Tab::Result;
                self.raise_toast(TextKey::ExportFailed, StatusTone::Error);
                self.status =
                    StatusMessage::with_detail(TextKey::ExportFailed, StatusTone::Error, detail);
            }
        }
    }
}

impl AppState {
    pub(super) fn save_package(&mut self) {
        self.write_package(vkit_core::vam::ExistingPackage::Keep);
    }

    pub(super) fn replace_package(&mut self) {
        if self.pending_package_replace.take().is_none() {
            return;
        }
        self.write_package(vkit_core::vam::ExistingPackage::Replace);
    }

    pub fn package_metadata(&self) -> vkit_core::vam::VarMetadata {
        vkit_core::vam::VarMetadata {
            version: self.var_version_text.trim().parse().unwrap_or(1).max(1),
            ..self.var_metadata.clone()
        }
    }

    fn write_package(&mut self, existing: vkit_core::vam::ExistingPackage) {
        if self.block_mutation_while_busy() {
            return;
        }

        self.missing_package_fields = MissingPackageFields {
            creator: self.var_metadata.creator.trim().is_empty(),
            package: self.var_metadata.package.trim().is_empty(),
        };
        if self.missing_package_fields.any() {
            self.package_fields_want_attention = true;
            return;
        }
        let Some(root) = self.vam_root.clone() else {
            self.status = StatusMessage::with_detail(
                TextKey::ExportFailed,
                StatusTone::Warning,
                "Choose a VaM folder before packaging".to_owned(),
            );
            return;
        };

        let morph = self
            .package_from_this_head
            .then(|| crate::workflow::export_snapshot_from_state(self).ok())
            .flatten();
        let textures = self
            .package_from_this_head
            .then_some(())
            .and_then(|()| self.texture_project.baked.clone())
            .map(|baked| crate::texture_project::TextureExportSnapshot {
                directory: root.clone(),
                prefix: self.texture_project.export_prefix.clone(),
                pbr_convention: self.texture_project.output_pbr,
                images: baked.images.clone(),
            });

        if morph.is_none()
            && textures.is_none()
            && self.package_morphs.is_empty()
            && self.package_textures.is_empty()
        {
            self.status = StatusMessage::new(TextKey::PackageNothingToPack, StatusTone::Warning);
            return;
        }

        let directory = self
            .package_output_directory()
            .unwrap_or_else(|| root.join("AddonPackages"));
        let metadata = self.package_metadata();
        if existing == vkit_core::vam::ExistingPackage::Keep {
            let path = vkit_core::vam::var_package_path(&directory, &metadata);
            if path.exists() {
                self.pending_package_replace = Some(path);
                return;
            }
        }
        match crate::workflow::write_var_package_from_exports(
            &directory,
            &metadata,
            morph,
            textures.as_ref(),
            self.figure_sex,
            &crate::workflow::PackageExtras {
                morphs: self.package_morphs.iter().map(|f| f.path.clone()).collect(),
                textures: self
                    .package_textures
                    .iter()
                    .map(|f| f.path.clone())
                    .collect(),
            },
            existing,
        ) {
            Ok(package) => {
                let _ = crate::diagnostics::record(
                    crate::diagnostics::Severity::Info,
                    "export",
                    "var_package_written",
                    &format!(
                        "{} with {} file(s)",
                        package.path.display(),
                        package.contents.len()
                    ),
                );
                self.last_package_path = Some(package.path.clone());
                self.raise_toast(TextKey::PackageSection, StatusTone::Success);
                self.status = StatusMessage::with_detail(
                    TextKey::ExportComplete,
                    StatusTone::Success,
                    package
                        .path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                );
            }
            Err(error) => {
                let _ = crate::diagnostics::record(
                    crate::diagnostics::Severity::Error,
                    "export",
                    "var_package_failed",
                    &error.to_string(),
                );
                self.raise_toast(TextKey::ExportFailed, StatusTone::Error);
                self.status = StatusMessage::with_detail(
                    TextKey::ExportFailed,
                    StatusTone::Error,
                    error.to_string(),
                );
            }
        }
    }
}

fn morph_pair_sex(receipt: &ExportReceipt) -> Option<SkinSex> {
    receipt.committed_paths.iter().find_map(|path| {
        path.components().rev().find_map(|component| {
            match component
                .as_os_str()
                .to_str()?
                .to_ascii_lowercase()
                .as_str()
            {
                "female" => Some(SkinSex::Female),
                "male" => Some(SkinSex::Male),
                _ => None,
            }
        })
    })
}
