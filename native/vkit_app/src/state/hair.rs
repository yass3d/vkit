use super::*;

pub(super) struct ExportParts<'a, Scalp> {
    pub carried: Vec<(&'a crate::hair_project::HairPart, Scalp)>,

    pub blank: Vec<String>,

    pub orphaned: Vec<String>,
}

pub(super) fn sort_parts_for_export<'a, Scalp>(
    parts: &'a [crate::hair_project::HairPart],
    scalp_for: impl Fn(&str) -> Option<Scalp>,
) -> ExportParts<'a, Scalp> {
    let mut sorted = ExportParts {
        carried: Vec::new(),
        blank: Vec::new(),
        orphaned: Vec::new(),
    };
    for part in parts {
        match scalp_for(&part.provider_name) {
            Some(scalp) if !part.strands.is_empty() || part.kind.is_scalp() => {
                sorted.carried.push((part, scalp));
            }
            Some(_) => sorted.blank.push(part.name.clone()),
            None if part.strands.is_empty() => sorted.blank.push(part.name.clone()),
            None => sorted.orphaned.push(part.name.clone()),
        }
    }
    sorted
}

pub(crate) const HAIR_SIMULATION_SECONDS: f32 = 10.0;

pub const HAIR_SETTLE_GRAVITY: f32 = 1.0;

const SETTLE_TRAVEL_LIMIT: f32 = 1.5;

fn strand_length(points: &[[f32; 3]]) -> f32 {
    points
        .windows(2)
        .map(|pair| {
            let span = [
                pair[1][0] - pair[0][0],
                pair[1][1] - pair[0][1],
                pair[1][2] - pair[0][2],
            ];
            (span[0] * span[0] + span[1] * span[1] + span[2] * span[2]).sqrt()
        })
        .sum()
}

pub const HAIR_SETTLE_SECONDS: f32 = 1.2;

#[derive(Clone, Debug)]
pub struct HairThumbnailJob {
    pub target: HairThumbnailTarget,
    pub square: Option<[f32; 4]>,
    pub shoot: bool,
}

#[derive(Clone, Debug)]
pub enum HairThumbnailTarget {
    Part(u64),
    Preset,
}

#[derive(Clone, Debug)]
pub struct HairPartThumbnail {
    pub jpeg: std::sync::Arc<Vec<u8>>,
    pub revision: u64,
}

#[derive(Clone, Debug)]
pub struct HairExportNotice {
    pub folder: std::path::PathBuf,
    pub overwrote: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct HairShotFlash {
    pub square: [f32; 4],
    pub part: Option<u64>,
    pub at: f64,
}

impl AppState {
    pub const fn shows_authored_hair(&self) -> bool {
        matches!(
            self.active_tab,
            Tab::Hair | Tab::Result | Tab::Morph | Tab::Texture
        )
    }

    pub fn hair_export_would_overwrite(&self) -> Option<std::path::PathBuf> {
        let root = self.vam_root.as_ref()?;
        let creator = crate::hair_export::sanitize_name(&self.hair_project.export_creator, "Vkit");
        let name = crate::hair_export::sanitize_name(
            &self.hair_project.export_name,
            self.hair_project
                .selected_part()
                .map_or("Hair", |part| part.name.as_str()),
        );
        let folders: Vec<std::path::PathBuf> = self
            .hair_project
            .export_sexes
            .folders()
            .iter()
            .map(|sex| {
                root.join("Custom")
                    .join("Hair")
                    .join(sex)
                    .join(&creator)
                    .join(&name)
            })
            .collect();
        let folder = folders.iter().find(|folder| folder.exists())?.clone();
        folder.is_dir().then_some(folder)
    }

    fn remove_existing_hair_installs(&mut self) -> bool {
        let Some(root) = self.vam_root.clone() else {
            return false;
        };
        let creator = crate::hair_export::sanitize_name(&self.hair_project.export_creator, "Vkit");
        let name = crate::hair_export::sanitize_name(
            &self.hair_project.export_name,
            self.hair_project
                .selected_part()
                .map_or("Hair", |part| part.name.as_str()),
        );
        let mut removed = false;
        for sex in self.hair_project.export_sexes.folders() {
            let folder = root
                .join("Custom")
                .join("Hair")
                .join(sex)
                .join(&creator)
                .join(&name);
            if folder.exists() && std::fs::remove_dir_all(&folder).is_ok() {
                removed = true;
            }
        }
        removed
    }

    fn write_export_thumbnails(&mut self, outcome: &crate::hair_export::HairExportOutcome) {
        let Some(preset) = self.hair_preset_thumbnail.as_ref() else {
            return;
        };
        let mut first_failure: Option<String> = None;
        for path in &outcome.preset_thumbnails {
            if let Err(detail) = crate::thumbnail::write_file(path, &preset.jpeg) {
                first_failure.get_or_insert_with(|| format!("{}: {detail}", path.display()));
            }
        }
        for (part_id, path) in &outcome.item_thumbnails {
            let jpeg = self
                .hair_part_thumbnails
                .get(part_id)
                .map(|thumb| std::sync::Arc::clone(&thumb.jpeg))
                .unwrap_or_else(|| std::sync::Arc::clone(&preset.jpeg));
            if let Err(detail) = crate::thumbnail::write_file(path, &jpeg) {
                first_failure.get_or_insert_with(|| format!("{}: {detail}", path.display()));
            }
        }
        if let Some(detail) = first_failure {
            self.status = StatusMessage::with_detail(
                TextKey::HairThumbnailFailed,
                StatusTone::Warning,
                detail,
            );
        }
    }

    pub(super) fn clear_hair_of_the_head(&mut self, part_id: u64, scalp_indices: &[u32]) {
        let Some(collider) = self.head_collider() else {
            return;
        };
        let Some(part) = self
            .hair_project
            .parts
            .iter_mut()
            .find(|part| part.id == part_id)
        else {
            return;
        };
        for index in scalp_indices {
            if let Some(strand) = part.strands.get_mut(index) {
                collider.clear_strand(
                    &mut strand.points_cm,
                    crate::hair_collision::SKIN_CLEARANCE_CM,
                );
            }
        }
    }

    pub(crate) fn head_collider(
        &mut self,
    ) -> Option<std::sync::Arc<crate::hair_collision::HeadCollider>> {
        let head = self.workspace.result.as_ref()?;
        let revision = head.revision;
        if let Some((cached, collider)) = self.hair_collider.as_ref()
            && *cached == revision
        {
            return Some(std::sync::Arc::clone(collider));
        }
        let collider = std::sync::Arc::new(crate::hair_collision::HeadCollider::for_surface(
            head, revision,
        )?);
        self.hair_collider = Some((revision, std::sync::Arc::clone(&collider)));
        Some(collider)
    }

    pub(crate) fn export_hair_style(&mut self) {
        if self.hair_project.parts.is_empty() {
            self.status = StatusMessage::new(TextKey::HairExportNeedsPart, StatusTone::Warning);
            return;
        }
        let Some(vam_root) = self.vam_root.clone() else {
            self.status = StatusMessage::new(TextKey::HairExportNeedsVaMRoot, StatusTone::Warning);
            return;
        };
        let overwrote = self.remove_existing_hair_installs();

        let sorted = sort_parts_for_export(&self.hair_project.parts, |provider| {
            self.hair_scalps.get(provider).cloned()
        });
        let ExportParts {
            carried,
            blank,
            orphaned,
        } = sorted;
        if !orphaned.is_empty() {
            self.status = StatusMessage::with_detail(
                TextKey::HairScalpMissing,
                StatusTone::Warning,
                orphaned.join(", "),
            );
            return;
        }
        let parts: Vec<_> = carried
            .iter()
            .map(|(part, scalp)| (*part, scalp.as_ref()))
            .collect();
        if parts.is_empty() {
            self.status = StatusMessage::with_detail(
                TextKey::HairExportNeedsPart,
                StatusTone::Warning,
                blank.join(", "),
            );
            return;
        }
        let skipped = blank.join(", ");
        let creator = self.hair_project.export_creator.clone();
        let style = if self.hair_project.export_name.trim().is_empty() {
            self.hair_project
                .parts
                .first()
                .map(|part| part.name.clone())
                .unwrap_or_default()
        } else {
            self.hair_project.export_name.clone()
        };
        let sexes = self.hair_project.export_sexes;
        match crate::hair_export::export_hair_style(&vam_root, &parts, &creator, &style, sexes) {
            Ok(outcome) => {
                let mut detail = format!(
                    "{} item(s), {} strands, {} triangles -> {}",
                    outcome.item_count,
                    outcome.strand_count,
                    outcome.triangle_count,
                    outcome.preset_path.display(),
                );
                if !skipped.is_empty() {
                    detail.push_str(&format!("; empty, left out: {skipped}"));
                }
                self.status = StatusMessage::with_detail(
                    TextKey::HairExportDone,
                    StatusTone::Success,
                    detail,
                );
                self.write_export_thumbnails(&outcome);
                self.hair_export_files.clone_from(&outcome.files);
                let folder = outcome
                    .item_thumbnails
                    .first()
                    .and_then(|(_, path)| path.parent().map(std::path::Path::to_path_buf))
                    .unwrap_or_else(|| outcome.preset_path.clone());
                self.hair_export_notice = Some(HairExportNotice { folder, overwrote });
            }
            Err(detail) => {
                self.status = StatusMessage::with_detail(
                    TextKey::HairExportFailed,
                    StatusTone::Error,
                    detail,
                );
            }
        }
    }

    pub(crate) fn package_hair_style(&mut self) {
        let Some(vam_root) = self.vam_root.clone() else {
            self.status = StatusMessage::new(TextKey::HairExportNeedsVaMRoot, StatusTone::Warning);
            return;
        };
        if self.hair_export_files.is_empty() {
            self.status = StatusMessage::new(TextKey::HairPackageNeedsInstall, StatusTone::Warning);
            return;
        }
        let creator = self.hair_project.export_creator.trim().to_owned();
        let style = self.hair_project.export_name.trim().to_owned();
        let metadata = vkit_core::vam::VarMetadata {
            creator: creator.clone(),
            package: style,
            version: vkit_core::vam::next_version(
                &vam_root.join("AddonPackages"),
                &vkit_core::vam::safe_identity(&creator),
                &vkit_core::vam::safe_identity(self.hair_project.export_name.trim()),
            ),
            ..self.var_metadata.clone()
        };
        match crate::hair_export::package_hair_style(
            &vam_root,
            &self.hair_export_files.clone(),
            &metadata,
            vkit_core::vam::ExistingPackage::Keep,
        ) {
            Ok(package) => {
                self.status = StatusMessage::with_detail(
                    TextKey::HairPackageDone,
                    StatusTone::Success,
                    format!(
                        "{} file(s) -> {}",
                        package.contents.len(),
                        package.path.display()
                    ),
                );
            }
            Err(detail) => {
                self.status = StatusMessage::with_detail(
                    TextKey::HairPackageFailed,
                    StatusTone::Error,
                    detail,
                );
            }
        }
    }

    #[must_use]
    pub(crate) fn hair_portrait_bounds(&self, part: Option<u64>) -> Option<crate::scene::Bounds3> {
        let mut bounds = self.result_head_bounds()?;
        for layer in &self.hair_project.parts {
            if part.is_some_and(|only| only != layer.id) {
                continue;
            }
            if part.is_none() && !layer.visible {
                continue;
            }
            for strand in layer.strands.values() {
                for point in &strand.points_cm {
                    let point = glam::Vec3::from_array(*point);
                    bounds.min = bounds.min.min(point);
                    bounds.max = bounds.max.max(point);
                }
            }
        }
        Some(bounds)
    }

    #[must_use]
    pub(crate) fn hair_portrait_camera(
        &self,
        part: Option<u64>,
    ) -> Option<crate::camera::TurntableCamera> {
        let mut camera = self.workspace.result_camera;
        camera.look_from_standard_view(crate::camera::StandardView::Front);
        camera.frame(self.hair_portrait_bounds(part)?);
        camera.distance *= crate::hair_portrait::PORTRAIT_CLOSENESS;
        Some(camera)
    }

    #[must_use]
    pub(crate) fn hair_portraits_wanted(&self) -> Vec<HairThumbnailTarget> {
        let mut wanted: Vec<HairThumbnailTarget> = self
            .hair_project
            .parts
            .iter()
            .filter(|part| !self.hair_part_thumbnails.contains_key(&part.id))
            .map(|part| HairThumbnailTarget::Part(part.id))
            .collect();
        if self.hair_preset_thumbnail.is_none() {
            wanted.push(HairThumbnailTarget::Preset);
        }
        wanted
    }

    #[must_use]
    pub(crate) fn hair_portrait_scene(
        &self,
        ctx: &egui::Context,
        camera: crate::camera::TurntableCamera,
        only: Option<u64>,
    ) -> Option<crate::hair_portrait::PortraitScene> {
        let head = self.workspace.result.clone()?;
        let view_projection = camera.view_projection(1.0);
        let model = crate::scene::ModelTransform::default().matrix();
        let eye = camera.eye();
        let grading = self.viewport_grading();

        let skin = self
            .active_skin_preview()
            .map(|skin| crate::renderer::SkinPaintCallback {
                spot: crate::renderer::SceneSpot::default(),
                scene_key: crate::hair_portrait::portrait_scene_key(u64::MAX),
                mesh: std::sync::Arc::clone(&head),
                skin,
                view_projection,
                model,
                eye,
                light_yaw_radians: grading.light_yaw_radians,
                light_preset: grading.preset,
                frame_radius: camera.frame_radius,
                light_brightness: grading.brightness,
                tone_mapping: grading.tone_mapping,
                depth_scope: crate::renderer::RenderDepthScope::Shared,
                skin_visibility: crate::renderer::SkinVisibilityGroups::ALL,
                show_tear_lacrimals: true,
                show_eyelashes: true,
                smooth_passes: self.surface_smooth_passes,
            });

        let mut scalps = Vec::new();
        let mut hair = Vec::new();
        for (index, part) in self.hair_project.parts.iter().enumerate() {
            if only.is_some_and(|id| id != part.id) || (only.is_none() && !part.visible) {
                continue;
            }
            let scalp = self.hair_scalps.get(&part.provider_name)?;
            let preview = crate::viewport::hair_portrait_preview(ctx, self, part, scalp)?;
            let scene_key = crate::hair_portrait::portrait_scene_key(index as u64);
            for (layer_index, layer) in preview.scalps.iter().enumerate() {
                scalps.push(crate::hair_renderer::ScalpPaintCallback {
                    spot: crate::renderer::SceneSpot::default(),
                    scene_key: crate::hair_portrait::portrait_scene_key(
                        ((index as u64) << 32) | (layer_index as u64 + 1),
                    ),
                    head: std::sync::Arc::clone(&head),
                    part: layer.clone(),
                    view_projection,
                    model,
                    eye,
                    light_yaw_radians: grading.light_yaw_radians,
                    light_preset: grading.preset,
                    light_brightness: grading.brightness,
                    tone_mapping: grading.tone_mapping,
                });
            }
            hair.push(crate::hair_renderer::HairPaintCallback {
                capture: None,
                spot: crate::renderer::SceneSpot::default(),
                scene_key,
                frame: 0,
                mesh: std::sync::Arc::clone(&head),
                preview,
                view_projection,
                model,
                eye,
                light_yaw_radians: grading.light_yaw_radians,
                light_preset: grading.preset,
                light_brightness: grading.brightness,
                tone_mapping: grading.tone_mapping,
                time_seconds: 0.0,
                settle_gravity: 0.0,
                simulate_hair: crate::hair_physics::HairSimulation::Off,
                solve: false,
                viewport_pixels: [
                    crate::hair_portrait::PORTRAIT_SIDE as f32,
                    crate::hair_portrait::PORTRAIT_SIDE as f32,
                ],
            });
        }
        if hair.is_empty() {
            return None;
        }
        Some(crate::hair_portrait::PortraitScene { skin, scalps, hair })
    }

    pub(crate) fn begin_hair_thumbnail(&mut self, target: HairThumbnailTarget) {
        self.hair_thumbnail = Some(HairThumbnailJob {
            target,
            square: None,
            shoot: false,
        });
    }

    pub(crate) fn hair_isolated_part(&self) -> Option<u64> {
        match self.hair_thumbnail.as_ref()?.target {
            HairThumbnailTarget::Part(id) => Some(id),
            HairThumbnailTarget::Preset => None,
        }
    }

    pub(crate) fn set_hair_preset_thumbnail(&mut self, jpeg: Vec<u8>) {
        let revision = self
            .hair_preset_thumbnail
            .as_ref()
            .map_or(1, |thumb| thumb.revision + 1);
        self.hair_preset_thumbnail = Some(HairPartThumbnail {
            jpeg: std::sync::Arc::new(jpeg),
            revision,
        });
    }

    pub(crate) fn set_hair_part_thumbnail(&mut self, part_id: u64, jpeg: Vec<u8>) {
        let revision = self
            .hair_part_thumbnails
            .get(&part_id)
            .map_or(1, |thumb| thumb.revision + 1);
        self.hair_part_thumbnails.insert(
            part_id,
            HairPartThumbnail {
                jpeg: std::sync::Arc::new(jpeg),
                revision,
            },
        );
    }

    pub(crate) fn complete_hair_thumbnail(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.status = StatusMessage::new(TextKey::HairThumbnailSaved, StatusTone::Success);
            }
            Err(detail) => {
                self.status = StatusMessage::with_detail(
                    TextKey::HairThumbnailFailed,
                    StatusTone::Warning,
                    detail,
                );
            }
        }
    }

    pub(super) fn reset_hair_shapes(&mut self) {
        let mut touched = false;
        for part_id in self.hair_project.editable_parts() {
            let Some(scalp) = self
                .hair_project
                .part(part_id)
                .and_then(|part| self.hair_scalps.get(&part.provider_name).cloned())
            else {
                continue;
            };
            let Some(part) = self
                .hair_project
                .parts
                .iter_mut()
                .find(|part| part.id == part_id)
            else {
                continue;
            };
            if part.reset_strand_shapes(&scalp) > 0 {
                touched = true;
                self.hair_project.touch(part_id);
            }
        }
        let _ = touched;
    }

    pub(super) fn plant_hair_strands(&mut self, part_id: u64, scalp_indices: &[u32]) {
        let Some(part) = self
            .hair_project
            .parts
            .iter_mut()
            .find(|part| part.id == part_id)
        else {
            return;
        };
        let Some(scalp) = self
            .posed_hair_scalps
            .get(&part.provider_name)
            .or_else(|| self.hair_scalps.get(&part.provider_name))
        else {
            return;
        };
        if part.plant(scalp, scalp_indices) > 0 {
            self.hair_project.touch(part_id);
        }
    }

    fn hair_scalp_for(
        &mut self,
        provider_name: &str,
    ) -> Option<std::sync::Arc<crate::hair_project::ScalpAuthoring>> {
        if let Some(scalp) = self.hair_scalps.get(provider_name) {
            return Some(std::sync::Arc::clone(scalp));
        }
        let scalp = self
            .builtin_hair_scalps
            .iter()
            .find(|scalp| scalp.provider_name == provider_name)?;
        let authoring = crate::hair_project::build_scalp_authoring(scalp).ok()?;
        let authoring = std::sync::Arc::new(authoring);
        self.hair_scalps
            .insert(provider_name.to_owned(), std::sync::Arc::clone(&authoring));
        Some(authoring)
    }

    fn preset_part_name(reference: &str) -> String {
        const SEPARATORS: [char; 4] = [' ', '-', '_', '.'];
        let tail = reference
            .rsplit(['/', '\\', ':'])
            .next()
            .unwrap_or(reference);
        let stem = tail.rsplit_once('.').map_or(tail, |(stem, _)| stem).trim();
        let mut name = stem;
        for token in reference
            .split(['/', '\\', ':'])
            .flat_map(|segment| segment.split('.'))
            .filter(|token| !token.is_empty())
        {
            let Some(rest) = name
                .get(..token.len())
                .filter(|head| head.eq_ignore_ascii_case(token))
                .and_then(|_| name.get(token.len()..))
                .filter(|rest| rest.starts_with(SEPARATORS))
            else {
                continue;
            };
            let rest = rest.trim_start_matches(SEPARATORS);
            if !rest.is_empty() {
                name = rest;
            }
        }
        if name.is_empty() {
            stem.to_owned()
        } else {
            name.to_owned()
        }
    }

    pub(super) fn import_hair_preset(&mut self, preset_id: &str) {
        let Some(preset) = self
            .vam_hair_presets
            .iter()
            .find(|preset| preset.stable_id == preset_id)
            .cloned()
        else {
            self.status = StatusMessage::new(TextKey::HairLoadFailed, StatusTone::Error);
            return;
        };

        let preset_storables = crate::vam_hair::preset_storables(&preset);
        let mut parts = Vec::new();
        let mut refused: Vec<String> = Vec::new();
        self.hair_project
            .record(crate::hair_project::HairEdit::PresetLoaded);
        let ids = self.hair_project.next_ids(preset.parts.len());
        let mut caps: Vec<(
            String,
            crate::hair_settings::HairSettings,
            crate::hair_project::HairScalpTexture,
        )> = Vec::new();
        for (reference, id) in preset.parts.iter().zip(ids) {
            let geometry = match vkit_core::vam::load_hair_part_geometry(reference) {
                Ok(geometry) => geometry,
                Err(error) => {
                    refused.push(format!("{}: {error}", reference.reference));
                    continue;
                }
            };
            let mut settings = crate::hair_settings::HairSettings::default();
            for storables in crate::vam_hair::part_storables(reference)
                .iter()
                .chain(preset_storables.iter())
            {
                settings.absorb_storables(
                    vkit_core::vam::hair_sim_storable(storables, &reference.internal_id),
                    vkit_core::vam::hair_scalp_material_storable(storables, &reference.internal_id),
                );
            }
            let scalp_texture = vkit_core::vam::load_hair_part_look(reference)
                .ok()
                .map(|look| crate::vam_hair::extract_scalp_sheets(reference, &look))
                .unwrap_or_default();

            if settings.shows_scalp_cap() {
                caps.push((
                    geometry.provider_name.clone(),
                    settings.clone(),
                    scalp_texture.clone(),
                ));
                settings.hide_scalp_cap();
            }

            let Some(scalp) = self.hair_scalp_for(&geometry.provider_name) else {
                if geometry.guides.is_empty() {
                    continue;
                }
                refused.push(format!(
                    "{}: {} is not a scalp we can author on",
                    reference.reference, geometry.provider_name
                ));
                continue;
            };
            let name = Self::preset_part_name(&reference.reference);
            match crate::hair_project::hair_part_from_preset(
                &geometry,
                &scalp,
                settings,
                scalp_texture,
                name,
                id,
                0,
            ) {
                Ok(part) => parts.push(part),
                Err(reason) => refused.push(format!("{}: {reason}", reference.reference)),
            }
        }

        if parts.is_empty() {
            self.status = StatusMessage::with_detail(
                TextKey::HairLoadFailed,
                StatusTone::Error,
                refused.join(" | "),
            );
            return;
        }
        self.hair_project.adopt_parts(parts);
        for (provider, settings, texture) in caps.into_iter().rev() {
            let id = self.hair_project.next_ids(1)[0];
            self.hair_project
                .adopt_scalp(&provider, settings, texture, id);
        }
        self.hair_project.active_tool = crate::hair_project::HairTool::Comb;
        self.hair_project.export_name = crate::hair_export::sanitize_name(&preset.label, "hair");
        self.status = if refused.is_empty() {
            StatusMessage::new(TextKey::HairReady, StatusTone::Info)
        } else {
            StatusMessage::with_detail(
                TextKey::HairPartsSkipped,
                StatusTone::Warning,
                refused.join(" | "),
            )
        };
    }

    pub(super) fn add_hair_scalp(&mut self, provider_name: &str) {
        if !self.ensure_hair_scalp(provider_name) {
            return;
        }
        self.hair_project
            .record(crate::hair_project::HairEdit::ScalpAdded);
        let id = self.hair_project.add_scalp_part(provider_name);
        self.hair_project.activate_part(id, false);
    }

    pub(super) fn set_hair_scalp_mesh(&mut self, id: u64, provider_name: &str) {
        if !self.ensure_hair_scalp(provider_name) {
            return;
        }
        if self
            .hair_project
            .part(id)
            .is_none_or(|part| part.provider_name == provider_name)
        {
            return;
        }
        self.hair_project
            .record(crate::hair_project::HairEdit::ScalpMesh);
        let worn = self
            .hair_project
            .part(id)
            .map(|part| part.provider_name.clone())
            .and_then(|name| self.hair_scalps.get(&name).map(std::sync::Arc::clone));
        let wanted = self
            .hair_scalps
            .get(provider_name)
            .map(std::sync::Arc::clone);
        let mut lost = 0;
        if let Some(part) = self
            .hair_project
            .parts
            .iter_mut()
            .find(|part| part.id == id)
        {
            part.provider_name = provider_name.to_owned();
            part.settings.wear(provider_name);
            part.scalp_texture = crate::hair_project::HairScalpTexture::default();
            if let (Some(worn), Some(wanted)) = (worn.as_ref(), wanted.as_ref())
                && !std::sync::Arc::ptr_eq(worn, wanted)
            {
                lost = part.reseat_onto(worn, wanted);
            }
        }
        if lost > 0 {
            self.status = StatusMessage::new(TextKey::HairScalpMeshCrowded, StatusTone::Warning);
        }
        self.hair_project.touch(id);
    }

    fn ensure_hair_scalp(&mut self, provider_name: &str) -> bool {
        if self.hair_scalps.contains_key(provider_name) {
            return true;
        }
        let Some(scalp) = self
            .builtin_hair_scalps
            .iter()
            .find(|scalp| scalp.provider_name == provider_name)
        else {
            self.status = StatusMessage::new(TextKey::HairScalpMissing, StatusTone::Warning);
            return false;
        };
        match crate::hair_project::build_scalp_authoring(scalp) {
            Ok(authoring) => {
                self.hair_scalps
                    .insert(provider_name.to_owned(), std::sync::Arc::new(authoring));
                true
            }
            Err(_) => {
                self.status = StatusMessage::new(TextKey::HairScalpMissing, StatusTone::Warning);
                false
            }
        }
    }

    pub(super) fn add_hair_part(&mut self, provider_name: &str) {
        if !self.hair_scalps.contains_key(provider_name) {
            let Some(scalp) = self
                .builtin_hair_scalps
                .iter()
                .find(|scalp| scalp.provider_name == provider_name)
            else {
                self.status = StatusMessage::new(TextKey::HairScalpMissing, StatusTone::Warning);
                return;
            };
            match crate::hair_project::build_scalp_authoring(scalp) {
                Ok(authoring) => {
                    self.hair_scalps
                        .insert(provider_name.to_owned(), std::sync::Arc::new(authoring));
                }
                Err(detail) => {
                    self.status = StatusMessage::with_detail(
                        TextKey::HairScalpMissing,
                        StatusTone::Error,
                        detail,
                    );
                    return;
                }
            }
        }
        self.hair_project.add_part(provider_name);
    }

    pub(super) fn unplant_hair_strands(&mut self, part_id: u64, scalp_indices: &[u32]) {
        if let Some(part) = self
            .hair_project
            .parts
            .iter_mut()
            .find(|part| part.id == part_id)
            && part.unplant(scalp_indices) > 0
        {
            self.hair_project.touch(part_id);
        }
    }

    pub(super) fn adopt_settled_hair(&mut self, part_id: u64, shifts: &[Vec<[f32; 3]>]) {
        let Some(part) = self
            .hair_project
            .parts
            .iter_mut()
            .find(|part| part.id == part_id)
        else {
            return;
        };
        if part.strands.len() != shifts.len() {
            return;
        }
        let mut moved = 0_usize;
        for ((_root, strand), shift) in part.strands.iter_mut().zip(shifts) {
            if strand.points_cm.len() != shift.len() {
                continue;
            }
            let reach = strand_length(&strand.points_cm) * SETTLE_TRAVEL_LIMIT;
            if shift.iter().any(|delta| {
                delta[0].abs() > reach || delta[1].abs() > reach || delta[2].abs() > reach
            }) {
                continue;
            }
            for (point, delta) in strand
                .points_cm
                .iter_mut()
                .skip(1)
                .zip(shift.iter().skip(1))
            {
                point[0] += delta[0];
                point[1] += delta[1];
                point[2] += delta[2];
            }
            moved += 1;
        }
        if moved == 0 {
            return;
        }
        self.hair_project.touch(part_id);
    }

    pub(super) fn grow_hair_vertex_selection(&mut self, by: i32) {
        if by == 0 || self.hair_vertex_selection.is_empty() {
            return;
        }
        let mut grown = std::collections::BTreeSet::new();
        for (part_id, strand_id, point) in &self.hair_vertex_selection {
            let Some(points) = self
                .hair_project
                .part(*part_id)
                .and_then(|part| part.strands.get(strand_id))
                .map(|strand| strand.points_cm.len())
            else {
                continue;
            };
            if by > 0 {
                grown.insert((*part_id, *strand_id, *point));
                for step in 1..=(by as usize) {
                    if *point > step {
                        grown.insert((*part_id, *strand_id, point - step));
                    }
                    if point + step < points {
                        grown.insert((*part_id, *strand_id, point + step));
                    }
                }
            }
        }
        if by > 0 {
            self.hair_vertex_selection = grown;
            return;
        }
        let held = self.hair_vertex_selection.clone();
        let inner: std::collections::BTreeSet<_> = held
            .iter()
            .filter(|(part, strand, point)| {
                let inward = *point <= 1 || held.contains(&(*part, *strand, point - 1));
                let outward = held.contains(&(*part, *strand, point + 1));
                inward && outward
            })
            .copied()
            .collect();
        if !inner.is_empty() {
            self.hair_vertex_selection = inner;
        }
    }

    pub(super) fn select_all_hair_strands(&mut self) {
        let mut taken = std::collections::BTreeSet::new();
        for part_id in self.hair_project.editable_parts() {
            let Some(part) = self.hair_project.part(part_id) else {
                continue;
            };
            for (strand_id, strand) in &part.strands {
                for point in 1..strand.points_cm.len() {
                    taken.insert((part_id, *strand_id, point));
                }
            }
        }
        self.hair_vertex_selection = taken;
    }

    pub(super) fn set_hair_part_segments(&mut self, id: u64, segments: usize) {
        let clamped = segments.clamp(2, crate::hair_project::MAX_HAIR_SEGMENTS);
        if self
            .hair_project
            .part(id)
            .is_none_or(|part| part.segments == clamped)
        {
            return;
        }
        self.hair_project
            .record(crate::hair_project::HairEdit::Segments);
        if let Some(part) = self
            .hair_project
            .parts
            .iter_mut()
            .find(|part| part.id == id)
        {
            part.segments = clamped;
            part.resample_all();
            self.hair_project.touch(id);
        }
    }

    pub(super) fn scale_hair_strands(&mut self, part_id: u64, scalp_indices: &[u32], factor: f32) {
        if let Some(part) = self
            .hair_project
            .parts
            .iter_mut()
            .find(|part| part.id == part_id)
            && part.scale_strands(scalp_indices, factor) > 0
        {
            self.hair_project.touch(part_id);
        }
    }

    pub(super) fn set_hair_strand_points(
        &mut self,
        part_id: u64,
        strands: Vec<(u32, Vec<[f32; 3]>)>,
    ) {
        let touched: Vec<u32> = strands.iter().map(|(index, _)| *index).collect();
        if let Some(part) = self
            .hair_project
            .parts
            .iter_mut()
            .find(|part| part.id == part_id)
            && part.set_strand_points(strands) > 0
        {
            self.hair_project.touch(part_id);
            self.hair_strands_awaiting_clearance
                .entry(part_id)
                .or_default()
                .extend(touched.iter().copied());
        }
    }

    pub(super) fn settle_hair_against_the_head(&mut self) {
        let owed = std::mem::take(&mut self.hair_strands_awaiting_clearance);
        for (part_id, strands) in owed {
            let strands: Vec<u32> = strands.into_iter().collect();
            self.clear_hair_of_the_head(part_id, &strands);
        }
    }

    pub(super) fn request_hair_export(&mut self) {
        if !self.hair_overwrite_confirmed && self.hair_export_would_overwrite().is_some() {
            self.pending_hair_overwrite = true;
            return;
        }
        let wanted = self.hair_portraits_wanted();
        if wanted.is_empty() {
            self.export_hair_style();
        } else {
            self.pending_hair_portraits = wanted;
        }
    }

    pub(super) fn set_hair_param(&mut self, id: u64, key: &str, value: f32) {
        if let Some(param) = crate::hair_settings::HAIR_PARAMS
            .iter()
            .find(|param| param.key == key)
        {
            self.hair_project
                .record(crate::hair_project::HairEdit::Param(param.key));
            if let Some(part) = self.hair_project.parts.iter_mut().find(|p| p.id == id) {
                part.settings.set(param, value);
                self.hair_project.touch(id);
            }
        }
    }

    pub(super) fn set_hair_color_channel(
        &mut self,
        id: u64,
        key: &str,
        channel: usize,
        value: f32,
    ) {
        if let Some(param) = crate::hair_settings::HAIR_PARAMS
            .iter()
            .find(|param| param.key == key)
        {
            self.hair_project
                .record(crate::hair_project::HairEdit::ColorChannel(
                    param.key,
                    channel as u8,
                ));
            if let Some(part) = self.hair_project.parts.iter_mut().find(|p| p.id == id) {
                part.settings.set_color_channel(param, channel, value);
                self.hair_project.touch(id);
            }
        }
    }

    pub(super) fn reset_hair_params(&mut self, id: u64) {
        self.hair_project
            .record(crate::hair_project::HairEdit::ParamsReset);
        if let Some(part) = self.hair_project.parts.iter_mut().find(|p| p.id == id) {
            for group in crate::hair_settings::HairParamGroup::ALL {
                part.settings.reset_group(group);
            }
            self.hair_project.touch(id);
        }
    }

    pub(super) fn set_hair_part_name(&mut self, id: u64, name: String) {
        let name = name.trim().to_owned();
        let changes = !name.is_empty()
            && self
                .hair_project
                .parts
                .iter()
                .any(|part| part.id == id && part.name != name);
        if changes {
            self.hair_project
                .record(crate::hair_project::HairEdit::PartRenamed);
            if let Some(part) = self.hair_project.parts.iter_mut().find(|p| p.id == id) {
                part.name = name;
            }
            self.hair_project.touch(id);
        }
    }

    pub(super) fn set_hair_rigidity(&mut self, id: u64, strands: Vec<(u32, Vec<f32>)>) {
        let Some(part) = self
            .hair_project
            .parts
            .iter_mut()
            .find(|part| part.id == id)
        else {
            return;
        };
        let mut wrote = false;
        for (index, values) in strands {
            let Some(strand) = part.strands.get_mut(&index) else {
                continue;
            };
            if values.len() != strand.points_cm.len() {
                continue;
            }
            strand.rigidity = values;
            wrote = true;
        }
        if !wrote {
            return;
        }
        self.hair_project.touch(id);
        let painted = crate::hair_settings::HAIR_PARAMS
            .iter()
            .find(|param| param.key == "usePaintedRigidity");
        if let Some(param) = painted
            && let Some(part) = self
                .hair_project
                .parts
                .iter_mut()
                .find(|part| part.id == id)
            && part.settings.get(param) < 0.5
        {
            part.settings.set(param, 1.0);
            self.hair_project.touch(id);
        }
    }

    pub(super) fn set_hair_part_style_joints(&mut self, id: u64, on: bool) {
        if let Some(part) = self.hair_project.parts.iter_mut().find(|p| p.id == id)
            && part.style_joints != on
        {
            self.hair_project
                .record(crate::hair_project::HairEdit::StyleJoints);
            if let Some(part) = self.hair_project.parts.iter_mut().find(|p| p.id == id) {
                part.style_joints = on;
            }
            self.hair_project.touch(id);
        }
    }

    pub(super) fn set_hair_scalp_texture(
        &mut self,
        id: u64,
        texture: crate::hair_project::HairScalpTexture,
    ) {
        if self
            .hair_project
            .part(id)
            .is_none_or(|part| part.scalp_texture == texture)
        {
            return;
        }
        self.hair_project
            .record(crate::hair_project::HairEdit::ScalpTexture);
        if let Some(part) = self.hair_project.parts.iter_mut().find(|p| p.id == id) {
            part.scalp_texture = texture;
            self.hair_project.touch(id);
        }
    }

    pub(super) fn copy_hair_settings(&mut self, id: u64) {
        self.hair_settings_clipboard = self
            .hair_project
            .parts
            .iter()
            .find(|part| part.id == id)
            .map(|part| part.settings.clone());
    }

    pub(super) fn paste_hair_settings(&mut self, id: u64) {
        if let Some(settings) = self.hair_settings_clipboard.clone() {
            self.hair_project
                .record(crate::hair_project::HairEdit::SettingsPasted);
            if let Some(part) = self.hair_project.parts.iter_mut().find(|p| p.id == id) {
                let scalp = part.settings.clone();
                part.settings = settings;
                part.settings.carry_scalp_from(&scalp);
                self.hair_project.touch(id);
            }
        }
    }

    pub(super) fn mirror_hair_part(&mut self, id: u64) {
        self.hair_project
            .record(crate::hair_project::HairEdit::PartMirrored);
        let provider = self
            .hair_project
            .part(id)
            .map(|part| part.provider_name.clone());
        if let Some(provider) = provider
            && let Some(scalp) = self.hair_scalps.get(&provider).cloned()
        {
            self.hair_project.mirror_part(id, &scalp);
        }
    }
}

#[cfg(test)]
mod preset_names {
    use crate::state::AppState;

    #[test]
    fn a_layer_is_named_after_the_part_and_nothing_the_folder_already_said() {
        let name = |reference: &str| AppState::preset_part_name(reference);

        assert_eq!(
            name("SELF:/Custom/Hair/Female/AKKEVE/hair/AKKEVE hair051 05.vam"),
            "hair051 05",
            "the creator is on the package, not on the hair",
        );
        assert_eq!(
            name("SELF:/Custom/Hair/Female/Miki/Aphrodite/Aphrodite Back.vam"),
            "Back",
        );
        assert_eq!(
            name("AKKEVE.hair_051.1:/Custom/Hair/Female/AKKEVE/hair/AKKEVE hair051 05.vam"),
            "hair051 05",
            "a package-qualified reference names the creator twice and still must not repeat it",
        );
        assert_eq!(
            name("SELF:/Custom/Hair/Female/ddaamm/ddaamm/long12 scalp.vam"),
            "long12 scalp",
            "nothing in the path repeats the style, so the whole stem stays",
        );
    }

    #[test]
    fn a_token_only_goes_when_it_is_a_whole_word() {
        assert_eq!(
            AppState::preset_part_name("SELF:/Custom/Hair/Female/AKKEVE/hair/hair051 01.vam"),
            "hair051 01",
            "the Hair folder must not eat the style name it happens to prefix",
        );
        assert_eq!(
            AppState::preset_part_name("SELF:/Custom/Hair/Female/Ren/Ren.vam"),
            "Ren",
            "stripping a name down to nothing leaves the layer unnamed, so it stays",
        );
    }
}

#[cfg(test)]
mod selection_tests {
    use crate::hair_project::HairStrand;
    use crate::state::{Action, AppState};

    fn planted(state: &mut AppState, strands: &[u32], points: usize) -> u64 {
        let id = state.hair_project.add_part("Cap");
        if let Some(part) = state.hair_project.parts.iter_mut().find(|p| p.id == id) {
            for root in strands {
                part.strands.insert(
                    *root,
                    HairStrand::new((0..points).map(|s| [0.0, s as f32, 0.0]).collect()),
                );
            }
        }
        id
    }

    #[test]
    fn select_all_takes_the_strands_of_active_parts_and_no_others() {
        let mut state = AppState::default();
        let taken = planted(&mut state, &[1, 2], 4);
        let left_out = planted(&mut state, &[5], 4);
        state.hair_project.activate_part(taken, false);

        state.dispatch(Action::SelectAllHairStrands);

        assert_eq!(
            state.hair_vertex_selection.len(),
            2 * 3,
            "two strands of three movable points each",
        );
        assert!(
            !state
                .hair_vertex_selection
                .iter()
                .any(|(part, _, _)| *part == left_out),
            "a part nobody activated was taken",
        );
        assert!(
            !state
                .hair_vertex_selection
                .iter()
                .any(|(_, _, point)| *point == 0),
            "the root is planted and cannot move, so it must not be held",
        );
    }

    #[test]
    fn growing_reaches_one_segment_each_way_and_stops_at_the_ends() {
        let mut state = AppState::default();
        let part = planted(&mut state, &[1], 5);
        state.hair_vertex_selection.insert((part, 1, 2));

        state.dispatch(Action::GrowHairVertexSelection(1));
        let held: Vec<usize> = state
            .hair_vertex_selection
            .iter()
            .map(|(_, _, point)| *point)
            .collect();
        assert_eq!(held, vec![1, 2, 3], "one each way");

        state.dispatch(Action::GrowHairVertexSelection(1));
        let held: Vec<usize> = state
            .hair_vertex_selection
            .iter()
            .map(|(_, _, point)| *point)
            .collect();
        assert_eq!(held, vec![1, 2, 3, 4], "the root and the tip are the ends");
    }

    #[test]
    fn shrinking_lets_go_of_the_edges_and_never_empties_the_selection() {
        let mut state = AppState::default();
        let part = planted(&mut state, &[1], 6);
        for point in 1..6 {
            state.hair_vertex_selection.insert((part, 1, point));
        }

        state.dispatch(Action::GrowHairVertexSelection(-1));
        let held: Vec<usize> = state
            .hair_vertex_selection
            .iter()
            .map(|(_, _, point)| *point)
            .collect();
        assert_eq!(held, vec![1, 2, 3, 4], "the tip was the only free edge");

        for _ in 0..10 {
            state.dispatch(Action::GrowHairVertexSelection(-1));
        }
        assert!(
            !state.hair_vertex_selection.is_empty(),
            "shrinking emptied the selection, which no tool can recover from",
        );
    }

    #[test]
    fn growing_nothing_stays_nothing() {
        let mut state = AppState::default();
        planted(&mut state, &[1], 4);
        state.dispatch(Action::GrowHairVertexSelection(1));
        assert!(state.hair_vertex_selection.is_empty());
    }
}

#[cfg(test)]
mod settle_tests {
    use crate::hair_project::{HairProject, HairStrand};
    use crate::hair_settle::SettleScope;
    use crate::state::{Action, AppState};

    fn planted(project: &mut HairProject, roots: &[u32]) -> u64 {
        let id = project.add_part("Cap");
        let Some(part) = project.parts.iter_mut().find(|part| part.id == id) else {
            return id;
        };
        for root in roots {
            part.strands.insert(
                *root,
                HairStrand::new((0..4).map(|step| [0.0, step as f32, 0.0]).collect()),
            );
        }
        id
    }

    fn falling(strands: usize, points: usize) -> Vec<Vec<[f32; 3]>> {
        vec![vec![[0.0, -1.0, 0.5]; points]; strands]
    }

    #[test]
    fn keeping_a_shape_turns_the_physics_on_and_waits_for_it() {
        let mut state = AppState::default();
        state.dispatch(Action::KeepSettledHairShape(SettleScope::Everything));
        assert!(state.hair_viewport_physics, "nothing would have fallen");
        assert!(
            state.hair_settle_seconds > 0.0,
            "the read has to wait for the hair to land",
        );
        assert_eq!(
            state.hair_settle_wanted,
            Some(SettleScope::Everything),
            "the ask has to survive until a paint carries it over",
        );
    }

    #[test]
    fn a_shift_moves_every_point_but_the_root() {
        let mut state = AppState::default();
        let id = planted(&mut state.hair_project, &[0, 1]);
        state.hair_settle_wanted = Some(SettleScope::Everything);
        state.dispatch(Action::AdoptSettledHair {
            part_id: id,
            shifts: falling(2, 4),
        });

        let part = state.hair_project.part(id).expect("the part");
        for (root, strand) in &part.strands {
            assert_eq!(
                strand.points_cm[0],
                [0.0, 0.0, 0.0],
                "strand {root} tore off the scalp",
            );
            for step in 1..4 {
                assert_eq!(
                    strand.points_cm[step],
                    [0.0, step as f32 - 1.0, 0.5],
                    "strand {root} point {step} did not move by the shift",
                );
            }
        }
    }

    #[test]
    fn a_readback_that_flings_a_strand_across_the_room_is_refused() {
        let mut state = AppState::default();
        let id = planted(&mut state.hair_project, &[0]);
        state.hair_settle_wanted = Some(SettleScope::Everything);
        state.dispatch(Action::AdoptSettledHair {
            part_id: id,
            shifts: vec![vec![[0.0, -400.0, 0.0]; 4]],
        });

        let part = state.hair_project.part(id).expect("the part");
        assert_eq!(
            part.strands[&0].points_cm[2],
            [0.0, 2.0, 0.0],
            "a solver that came apart moved the hair anyway",
        );
    }

    #[test]
    fn a_readback_that_no_longer_lines_up_is_dropped_rather_than_guessed_at() {
        let mut state = AppState::default();
        let id = planted(&mut state.hair_project, &[0, 1]);
        state.hair_settle_wanted = Some(SettleScope::Everything);

        state.dispatch(Action::AdoptSettledHair {
            part_id: id,
            shifts: falling(1, 4),
        });
        state.dispatch(Action::AdoptSettledHair {
            part_id: id,
            shifts: falling(2, 9),
        });

        let part = state.hair_project.part(id).expect("the part");
        for (root, strand) in &part.strands {
            assert_eq!(
                strand.points_cm[2],
                [0.0, 2.0, 0.0],
                "strand {root} was moved by a stale readback",
            );
        }
    }
}

#[cfg(test)]
mod export_sorting_tests {
    use super::*;
    use crate::hair_project::{HairPartKind, HairProject};

    fn project() -> HairProject {
        HairProject::default()
    }

    fn named(project: &mut HairProject, name: &str, provider: &str, kind: HairPartKind) -> u64 {
        let id = project.add_part(provider);
        if let Some(part) = project.parts.iter_mut().find(|part| part.id == id) {
            part.name = name.to_owned();
            part.kind = kind;
        }
        id
    }

    #[derive(Clone, Copy, Debug)]
    struct AScalp;

    #[test]
    fn a_part_holding_nothing_is_left_out_rather_than_fatal() {
        let mut project = project();
        named(&mut project, "Scalp", "Cap", HairPartKind::Scalp);
        named(&mut project, "Scalp 2", "Nowhere", HairPartKind::Scalp);

        let sorted = sort_parts_for_export(&project.parts, |provider| {
            (provider == "Cap").then_some(AScalp)
        });
        assert_eq!(sorted.carried.len(), 1, "the working cap should be written");
        assert_eq!(sorted.blank, vec!["Scalp 2".to_owned()]);
        assert!(
            sorted.orphaned.is_empty(),
            "an empty part must not stop the export",
        );
    }

    #[test]
    fn a_part_holding_hair_with_no_scalp_still_stops_the_export() {
        let mut project = project();
        let id = named(&mut project, "Bangs", "Nowhere", HairPartKind::Hair);
        if let Some(part) = project.parts.iter_mut().find(|part| part.id == id) {
            part.strands
                .insert(0, crate::hair_project::HairStrand::new(vec![[0.0; 3]; 4]));
        }

        let sorted = sort_parts_for_export(&project.parts, |_| None::<AScalp>);
        assert_eq!(
            sorted.orphaned,
            vec!["Bangs".to_owned()],
            "hair that would be silently dropped has to be named",
        );
        assert!(sorted.carried.is_empty());
    }

    #[test]
    fn a_hair_part_nobody_planted_is_blank_even_when_its_scalp_is_there() {
        let mut project = project();
        named(&mut project, "Empty", "Cap", HairPartKind::Hair);
        let sorted = sort_parts_for_export(&project.parts, |_| Some(AScalp));
        assert_eq!(sorted.blank, vec!["Empty".to_owned()]);
        assert!(sorted.carried.is_empty());
        assert!(sorted.orphaned.is_empty());
    }
}
