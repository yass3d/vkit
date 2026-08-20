use super::*;

impl AppState {
    pub(super) fn suggested_alignment(&self) -> Option<ScanTransform> {
        let scan = self.workspace.scan.as_deref()?;
        let template = self.workspace.template.as_deref()?;
        let scan_bounds = scan.facial_focus_bounds();
        let template_bounds = template.facial_focus_bounds();
        let scan_height = f64::from(scan_bounds.max.y - scan_bounds.min.y);
        let template_height = f64::from(template_bounds.max.y - template_bounds.min.y);
        if !scan_height.is_finite()
            || !template_height.is_finite()
            || scan_height <= 1.0e-8
            || template_height <= 1.0e-8
        {
            return None;
        }
        let scale = template_height / scan_height;
        let scan_center = scan_bounds.center().as_dvec3();
        let template_center = template_bounds.center().as_dvec3();

        let translation = template_center - scan_center;
        let prior = SimilarityTransform {
            scale,
            rotation: CoreMat3::IDENTITY,
            translation: CoreVec3::from_array(
                (scan_center + translation - scan_center * scale).to_array(),
            ),
        };
        let template_vertices = template
            .alignment_vertex_positions()
            .into_iter()
            .map(CoreVec3::from_array)
            .collect::<Vec<_>>();
        let refined = suggest_zero_pin_extent_similarity(
            CoreMat3::IDENTITY,
            scan.mesh.as_ref(),
            &template_vertices,
        )
        .unwrap_or(prior);
        let translation = glam::DVec3::from_array(refined.translation.to_array()) - scan_center
            + scan_center * refined.scale;
        Some(ScanTransform {
            scale_xyz: [refined.scale; 3],
            translation_cm: translation.to_array(),
            rotation_degrees: [0.0; 3],
        })
    }

    pub(super) fn suggested_auto_match(&self) -> Option<ScanTransform> {
        let scan = self.workspace.scan.as_deref()?;
        let template = self.workspace.template.as_deref()?;
        let model = self.current_scan_model_transform();
        let scan_bounds = model.bounds_to_world(scan.facial_focus_bounds());
        let template_bounds = template.facial_focus_bounds();
        let scan_height = f64::from(scan_bounds.max.y - scan_bounds.min.y);
        let template_height = f64::from(template_bounds.max.y - template_bounds.min.y);
        if !scan_height.is_finite()
            || !template_height.is_finite()
            || scan_height <= 1.0e-8
            || template_height <= 1.0e-8
        {
            return None;
        }

        let fallback_scale = template_height / scan_height;
        let scan_center = scan_bounds.center().as_dvec3();
        let template_center = template_bounds.center().as_dvec3();
        let prior = SimilarityTransform {
            scale: fallback_scale,
            rotation: CoreMat3::IDENTITY,
            translation: CoreVec3::from_array(
                (template_center - scan_center * fallback_scale).to_array(),
            ),
        };
        let mut front_facing = scan.mesh.as_ref().clone();
        for vertex in &mut front_facing.vertices {
            *vertex = model
                .point_to_world(glam::DVec3::from_array(*vertex))
                .to_array();
        }
        let template_vertices = template
            .alignment_vertex_positions()
            .into_iter()
            .map(CoreVec3::from_array)
            .collect::<Vec<_>>();
        let correction = suggest_zero_pin_extent_similarity(
            CoreMat3::IDENTITY,
            &front_facing,
            &template_vertices,
        )
        .unwrap_or(prior);
        if !correction.scale.is_finite() || correction.scale <= 0.0 {
            return None;
        }

        let pivot_local = model.pivot_local;
        let correction_translation = glam::DVec3::from_array(correction.translation.to_array());
        let translation = correction.scale * (pivot_local + model.translation)
            + correction_translation
            - pivot_local;
        let scale_xyz = self.transform.scale_xyz.map(|axis| axis * correction.scale);
        if !translation.is_finite()
            || scale_xyz
                .into_iter()
                .any(|axis| !axis.is_finite() || axis <= 0.0)
        {
            return None;
        }
        Some(ScanTransform {
            scale_xyz,
            translation_cm: translation.to_array(),
            rotation_degrees: self.transform.rotation_degrees,
        })
    }

    pub(super) fn auto_align(&mut self) {
        if let Some(transform) = self.suggested_auto_match() {
            self.active_tab = Tab::Alignment;
            self.apply_alignment_transform(transform);
        } else {
            self.status = StatusMessage::new(TextKey::AlignmentPending, StatusTone::Warning);
        }
    }

    pub(super) fn reset_placement(&mut self) {
        self.active_tab = Tab::Alignment;
        if let Some(transform) = self.suggested_alignment() {
            self.apply_alignment_transform(transform);
        } else {
            self.status = StatusMessage::new(TextKey::AlignmentPending, StatusTone::Warning);
        }
    }

    pub(super) fn set_scale(&mut self, value: [f64; 3]) {
        self.active_tab = Tab::Alignment;

        let next = ScanTransform {
            scale_xyz: value,
            ..self.transform
        };
        self.apply_alignment_transform(next);
    }

    pub(super) fn scale_xyz_with_axis(&self, axis: usize, value: f64) -> Option<[f64; 3]> {
        let mut scale_xyz = self.transform.scale_xyz;
        if self.scale_linked {
            let current = scale_xyz[axis];
            if !current.is_finite() || current <= 0.0 {
                return None;
            }
            let factor = value / current;
            if !factor.is_finite() || factor <= 0.0 {
                return None;
            }
            for axis_scale in &mut scale_xyz {
                *axis_scale *= factor;
            }
            if scale_xyz
                .iter()
                .any(|axis_scale| !axis_scale.is_finite() || *axis_scale <= 0.0)
            {
                return None;
            }
        } else {
            scale_xyz[axis] = value;
        }
        Some(scale_xyz)
    }

    pub(super) fn set_position(&mut self, axis: usize, value_cm: f64) {
        self.active_tab = Tab::Alignment;
        let mut translation_cm = self.transform.translation_cm;
        translation_cm[axis] = value_cm;
        let next = ScanTransform {
            translation_cm,
            ..self.transform
        };
        self.apply_alignment_transform(next);
    }

    pub(super) fn set_rotation(&mut self, axis: usize, value_degrees: f64) {
        self.active_tab = Tab::Alignment;
        let mut rotation_degrees = self.transform.rotation_degrees;
        rotation_degrees[axis] = value_degrees;
        let next = ScanTransform {
            rotation_degrees,
            ..self.transform
        };
        self.apply_alignment_transform(next);
    }

    pub(super) fn current_scan_model_transform(&self) -> ModelTransform {
        let pivot_local = self
            .workspace
            .scan
            .as_deref()
            .map(|scan| scan.facial_focus_bounds().center().as_dvec3().to_array())
            .unwrap_or([0.0; 3]);
        ModelTransform::from_components_xyz_with_pivot(
            self.transform.scale_xyz,
            self.transform.translation_cm,
            self.transform.rotation_degrees,
            pivot_local,
        )
    }

    fn scan_world_head_bounds(&self) -> Option<Bounds3> {
        let scan = self.workspace.scan.as_deref()?;
        Some(
            self.current_scan_model_transform()
                .bounds_to_world(scan.facial_focus_bounds()),
        )
    }

    fn template_head_bounds(&self) -> Option<Bounds3> {
        Some(self.workspace.template.as_deref()?.facial_focus_bounds())
    }

    pub(crate) fn alignment_head_bounds(&self) -> Option<Bounds3> {
        union_head_bounds(self.template_head_bounds(), self.scan_world_head_bounds())
    }

    pub(crate) fn edit_side_head_bounds(&self, side: MeshSide) -> Option<Bounds3> {
        match side {
            MeshSide::Scan => self.scan_world_head_bounds(),
            MeshSide::Template => self.template_head_bounds(),
        }
    }

    pub(crate) fn result_head_bounds(&self) -> Option<Bounds3> {
        let result = Some(self.workspace.result.as_deref()?.facial_focus_bounds());
        let overlay = (self.active_tab == Tab::Result
            && self.overlay_opacity.is_finite()
            && self.overlay_opacity > 0.0)
            .then(|| self.scan_world_head_bounds())
            .flatten();
        union_head_bounds(result, overlay)
    }

    fn active_scan_symmetry_mode(&self) -> SymmetryMode {
        match self.scan_symmetry {
            ScanSymmetry::Original => SymmetryMode::Off,
            ScanSymmetry::PositiveX => SymmetryMode::PositiveX,
            ScanSymmetry::NegativeX => SymmetryMode::NegativeX,
        }
    }

    pub(super) fn refresh_active_scan_symmetry(&mut self) {
        self.texture_project.invalidate_scan_projection();
        let transform = self.current_scan_model_transform();
        let mode = self.active_scan_symmetry_mode();
        if let Err(error) = self
            .workspace
            .set_scan_symmetry_with_transform(mode, transform)
        {
            self.status = StatusMessage::with_detail(
                TextKey::GenerationFailed,
                StatusTone::Error,
                error.to_string(),
            );
        }
    }

    pub(super) fn apply_alignment_transform(&mut self, next: ScanTransform) -> bool {
        if self.transform == next {
            return false;
        }
        let had_downstream_state = self.has_downstream_geometry();
        let refresh_now = self.alignment_transform_baseline.is_none();
        if refresh_now {
            self.push_alignment_undo(self.transform);
        }
        self.transform = next;
        if refresh_now {
            self.refresh_active_scan_symmetry();
        }
        self.invalidate_geometry_if_downstream(TextKey::ResultStale, had_downstream_state);
        true
    }

    pub(super) fn push_alignment_undo(&mut self, transform: ScanTransform) {
        if self.alignment_undo_history.len() >= MAX_ALIGNMENT_UNDO_HISTORY {
            self.alignment_undo_history.pop_front();
        }
        self.alignment_undo_history.push_back(transform);
    }

    pub(super) fn clear_alignment_history(&mut self) {
        self.alignment_undo_history.clear();
        self.alignment_transform_baseline = None;
    }
}
