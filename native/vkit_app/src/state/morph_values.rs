use super::*;

impl AppState {
    pub(super) fn set_eye_state(&mut self, value: EyeState) {
        let target_value = if value == EyeState::Closed { 1.0 } else { 0.0 };
        if value == EyeState::Closed && self.workspace.eye_morph.is_none() {
            self.status = StatusMessage::new(TextKey::NeedEyeMorph, StatusTone::Warning);
            return;
        }
        let affected = self.morph_affected_pin_pairs().unwrap_or_else(|| {
            self.workspace
                .pins
                .pairs()
                .iter()
                .enumerate()
                .filter_map(|(index, pair)| pair.complete().then_some(index))
                .collect()
        });
        let had_downstream_state = self.has_downstream_geometry();

        let scan_camera = self.workspace.scan_camera;
        let template_camera = self.workspace.template_camera;
        let result_camera = self.workspace.result_camera;
        if let Err(error) = self.workspace.set_template_eye_value(target_value) {
            self.status = StatusMessage::with_detail(
                TextKey::NeedEyeMorph,
                StatusTone::Error,
                error.to_string(),
            );
            return;
        }
        self.workspace.scan_camera = scan_camera;
        self.workspace.template_camera = template_camera;
        self.workspace.result_camera = result_camera;
        self.eye_state = value;
        self.eye_closure = target_value as f32;
        if !affected.is_empty() {
            self.workspace.pins.mark_review_required(affected);
            self.invalidate_geometry(TextKey::ReviewEyePins);
        } else {
            self.invalidate_geometry_if_downstream(TextKey::ResultStale, had_downstream_state);
        }
    }

    fn morph_affected_pin_pairs(&self) -> Option<Vec<usize>> {
        let morph = self.workspace.eye_morph.as_deref()?;
        let template = self.workspace.template.as_deref()?;
        let mut affected = Vec::new();
        for (index, pair) in self.workspace.pins.pairs().iter().enumerate() {
            let Some(endpoint) = pair.template else {
                continue;
            };
            let triangle = template.mesh.triangles.get(endpoint.triangle as usize)?;
            let moves = triangle.iter().any(|&vertex| {
                morph
                    .deltas
                    .get(vertex as usize)
                    .is_some_and(|delta| delta.iter().any(|value| *value != 0.0))
            });
            if moves {
                affected.push(index);
            }
        }
        Some(affected)
    }

    pub(super) fn clear_morph_reset_undo(&mut self) {
        self.morph_reset_undo = None;
    }

    pub(super) fn reset_morphs(&mut self) {
        if !self.tab_available(Tab::Morph) {
            self.status = StatusMessage::new(TextKey::MorphUnavailable, StatusTone::Warning);
            return;
        }

        let snapshot = MorphResetUndo {
            values: self.morph_library.snapshot_values(),
            eye_closure: self.eye_closure,
            seq: crate::edit_clock::next_seq(),
        };
        let default_eye_closure = self.fit_reference_value.unwrap_or(0.0) as f32;
        let controls_changed = self.morph_library.reset();
        let eye_changed = self.eye_closure.to_bits() != default_eye_closure.to_bits();
        if !controls_changed && !eye_changed {
            return;
        }

        self.eye_closure = default_eye_closure;
        if let Err(error) = self.rebuild_morph_preview(default_eye_closure) {
            let _ = self.morph_library.restore_values(&snapshot.values);
            self.eye_closure = snapshot.eye_closure;
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, error);
            return;
        }

        self.morph_value_history.clear();
        self.morph_edit_open = None;
        self.morph_reset_undo = Some(snapshot);
        self.morph_preview_dirty = false;
        self.result_preview_phase = ResultPreviewPhase::Morph;
        self.active_tab = Tab::Morph;
        self.status = StatusMessage::new(TextKey::MorphsReset, StatusTone::Info);
    }

    pub(super) fn undo_morph_value(&mut self) -> bool {
        if !self.tab_available(Tab::Morph) {
            return false;
        }
        let Some(snapshot) = self.morph_value_history.pop_back() else {
            return false;
        };
        let _ = self.morph_library.restore_values(&snapshot.values);
        self.eye_closure = snapshot.eye_closure;
        if let Err(error) = self.rebuild_morph_preview(snapshot.eye_closure) {
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, error);
            return true;
        }
        self.morph_edit_open = None;
        self.morph_preview_dirty = false;
        self.result_preview_phase = ResultPreviewPhase::Morph;
        self.active_tab = Tab::Morph;
        self.status = StatusMessage::new(TextKey::MorphEditUndone, StatusTone::Info);
        true
    }

    pub(super) fn undo_morph_reset(&mut self) -> bool {
        let Some(snapshot) = self.morph_reset_undo.clone() else {
            return false;
        };
        if !self.tab_available(Tab::Morph) {
            return false;
        }

        let _ = self.morph_library.restore_values(&snapshot.values);
        self.eye_closure = snapshot.eye_closure;
        if let Err(error) = self.rebuild_morph_preview(snapshot.eye_closure) {
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, error);
            return true;
        }
        self.clear_morph_reset_undo();
        self.morph_preview_dirty = false;
        self.result_preview_phase = ResultPreviewPhase::Morph;
        self.active_tab = Tab::Morph;
        self.status = StatusMessage::new(TextKey::MorphsResetUndone, StatusTone::Info);
        true
    }

    pub(super) fn set_eye_closure(&mut self, value: f32) {
        let value = value.clamp(0.0, 1.0);
        if !self.tab_available(Tab::Morph) {
            let _ = crate::diagnostics::record(
                crate::diagnostics::Severity::Warning,
                "state",
                "eye_closure_rejected",
                "morph stage is unavailable",
            );
            self.status = StatusMessage::new(TextKey::MorphUnavailable, StatusTone::Warning);
            return;
        }
        let Some(_morph) = self.workspace.eye_morph.as_ref().map(Arc::clone) else {
            let _ = crate::diagnostics::record(
                crate::diagnostics::Severity::Warning,
                "state",
                "eye_closure_rejected",
                &format!(
                    "workspace eye morph is absent (template={:?}; provider={:?}; reference={:?})",
                    self.template_path.as_ref().and_then(|p| p.file_name()),
                    self.vam_geometry_base_provenance.map(|p| p.tier),
                    self.fit_reference_value,
                ),
            );
            self.status = StatusMessage::new(TextKey::MorphUnavailable, StatusTone::Warning);
            return;
        };

        let previous = self.eye_closure;
        self.eye_closure = value;
        if let Err(error) = self.rebuild_morph_preview(value) {
            self.eye_closure = previous;
            let _ = crate::diagnostics::record(
                crate::diagnostics::Severity::Error,
                "state",
                "eye_closure_rebuild_failed",
                &error,
            );
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, error);
            return;
        }
        let changed = previous.to_bits() != value.to_bits();
        if changed {
            self.clear_morph_reset_undo();
        }
        self.morph_preview_dirty = false;
        self.result_preview_phase = ResultPreviewPhase::Morph;
        self.active_tab = Tab::Morph;

        self.status = StatusMessage::default();
    }

    pub(super) fn set_face_morph(&mut self, id: &str, value: f32) {
        if !self.tab_available(Tab::Morph) {
            self.status = StatusMessage::new(TextKey::SculptNeedsBaseHead, StatusTone::Warning);
            return;
        }

        self.active_tab = Tab::Morph;

        if self.morph_edit_open.as_deref() != Some(id) {
            if self.morph_value_history.len() >= MAX_MORPH_VALUE_UNDO {
                self.morph_value_history.pop_front();
            }
            self.morph_value_history.push_back(MorphResetUndo {
                values: self.morph_library.snapshot_values(),
                eye_closure: self.eye_closure,
                seq: crate::edit_clock::next_seq(),
            });
            self.morph_edit_open = Some(id.to_owned());
        }
        if self.morph_library.needs_resolution(id) {
            if self.morph_library.begin_resolution(id, value) {
                self.clear_morph_reset_undo();
                self.morph_preview_dirty = true;
                let request: Result<VaMWorkRequest, String> = (|| {
                    let descriptor = self.vam_cached_morphs.get(id).cloned().ok_or_else(|| {
                        format!("local morph cache metadata is unavailable for {id}")
                    })?;
                    let geometry = self
                        .workspace
                        .template_geometry
                        .as_deref()
                        .ok_or_else(|| "G2 template is unavailable".to_owned())?;
                    let canonical_faces = self
                        .vam_morph_faces
                        .as_ref()
                        .map(Arc::clone)
                        .ok_or_else(|| "cached morph topology receipt is unavailable".to_owned())?;
                    Ok(VaMWorkRequest::ResolveCachedMorph {
                        catalog_revision: self.vam_catalog_revision,
                        geometry_revision: self.revision,
                        control_id: id.to_owned(),
                        descriptor: Box::new(descriptor),
                        vertex_count: geometry.vertices.len(),
                        canonical_faces,
                    })
                })();
                match request {
                    Ok(request) => self.pending_vam_work.push_back(request),
                    Err(detail) => {
                        let _ = self.morph_library.mark_resolution_failed(id);
                        self.status = StatusMessage::with_detail(
                            TextKey::MorphLoadFailed,
                            StatusTone::Error,
                            detail,
                        );
                        return;
                    }
                }
            }
            self.status = StatusMessage::new(TextKey::MorphLoading, StatusTone::Info);
            return;
        }
        let previous = self
            .morph_library
            .controls()
            .iter()
            .find(|control| control.id == id)
            .map(|control| control.value);

        if !self.morph_library.set_value(id, value) {
            self.status = StatusMessage::new(TextKey::MorphNotInLibrary, StatusTone::Warning);
            return;
        }
        if let Err(error) = self.rebuild_morph_preview(self.eye_closure) {
            if let Some(previous) = previous {
                let _ = self.morph_library.set_value(id, previous);
            }
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, error);
            return;
        }
        self.clear_morph_reset_undo();
        self.morph_preview_dirty = false;
        self.result_preview_phase = ResultPreviewPhase::Morph;
        self.status = StatusMessage::default();
    }

    fn rotate_globes_to_gaze(&self, vertices: &mut [[f64; 3]], gaze: [f32; 2]) {
        let Some(template) = self.workspace.template_geometry.as_deref() else {
            return;
        };
        let Some(groups) = vkit_core::formats::collect_canonical_head_groups(template) else {
            return;
        };
        for (faces, bone, fallback, left_eye) in [
            (&groups.left_eye, "lEye", [3.13, 168.03, 5.828_937], true),
            (&groups.right_eye, "rEye", [-3.13, 168.03, 5.828_937], false),
        ] {
            let base_center = template
                .bone(bone)
                .map(|bone| vkit_core::math::Vec3::from(bone.center_point))
                .unwrap_or_else(|| vkit_core::math::Vec3::from(fallback));
            let Ok(center) =
                vkit_core::vam::transfer_rig_point_idw(base_center, &template.vertices, vertices)
            else {
                continue;
            };
            let [yaw, pitch] = crate::state::eye_angles_from_gaze(gaze, left_eye);
            let transform = crate::scene::ModelTransform::from_components_with_pivot(
                1.0,
                [0.0; 3],
                [-pitch, yaw, 0.0],
                [center.x, center.y, center.z],
            );

            let mut rotated = vec![false; vertices.len()];
            for &face_index in faces {
                let Some(face) = template.faces.get(face_index as usize) else {
                    continue;
                };
                for &vertex_id in face {
                    let index = vertex_id as usize;
                    if index >= vertices.len() || rotated[index] {
                        continue;
                    }
                    rotated[index] = true;
                    let point = glam::DVec3::from_array(vertices[index]);
                    vertices[index] = transform.point_to_world(point).to_array();
                }
            }
        }
    }

    pub(super) fn rebuild_morph_preview(&mut self, eye_value: f32) -> Result<(), String> {
        let total_started = Instant::now();

        let sculpt_base = self
            .sculpt
            .working_mesh()
            .ok_or_else(|| "sculpt working mesh is unavailable".to_owned())?;
        let eye_morph = self
            .workspace
            .eye_morph
            .as_ref()
            .ok_or_else(|| "closed-eye morph is unavailable".to_owned())?;
        let reference_value = self
            .fit_reference_value
            .ok_or_else(|| "fit eye reference is unavailable".to_owned())?;
        let eye_value = f64::from(eye_value.clamp(0.0, 1.0));
        let mut preview_vertices = sculpt_base.vertices.clone();
        eye_morph
            .apply_to_bound_vertices(&mut preview_vertices, eye_value, reference_value)
            .map_err(|error| error.to_string())?;
        self.morph_library
            .apply_to_bound_vertices(&mut preview_vertices, sculpt_base.faces.len())
            .map_err(|error| error.to_string())?;

        if let Some(gaze) = self.frozen_eye_gaze {
            self.rotate_globes_to_gaze(&mut preview_vertices, gaze);
        }
        let compose_finished = Instant::now();

        if eye_value != reference_value {
            let template = self
                .workspace
                .template_geometry
                .as_ref()
                .map(Arc::clone)
                .ok_or_else(|| "G2 template is unavailable".to_owned())?;
            let canonical = self.eyelid_conformation_plan.is_some()
                || matches_canonical_g2_topology(template.vertices.len(), &template.faces)
                    .map_err(|error| error.to_string())?;
            if canonical {
                let conformation_plan = match self.eyelid_conformation_plan.as_ref() {
                    Some(plan) => Some(Arc::clone(plan)),
                    None => EyelidConformationPlan::build(&template).ok().map(Arc::new),
                };
                if let Some(conformation_plan) = conformation_plan {
                    self.eyelid_conformation_plan = Some(Arc::clone(&conformation_plan));
                    self.morph_preview_reference_arrays.clear();
                    self.morph_preview_reference_arrays
                        .extend_from_slice(&template.vertices);
                    eye_morph
                        .apply_to_bound_vertices(
                            &mut self.morph_preview_reference_arrays,
                            eye_value,
                            eye_morph.default,
                        )
                        .map_err(|error| error.to_string())?;
                    self.morph_library
                        .apply_to_bound_vertices(
                            &mut self.morph_preview_reference_arrays,
                            template.faces.len(),
                        )
                        .map_err(|error| error.to_string())?;
                    self.morph_preview_reference_scratch.clear();
                    self.morph_preview_reference_scratch.extend(
                        self.morph_preview_reference_arrays
                            .iter()
                            .copied()
                            .map(CoreVec3::from),
                    );
                    self.morph_preview_conform_scratch.clear();
                    self.morph_preview_conform_scratch
                        .extend(preview_vertices.iter().copied().map(CoreVec3::from));

                    match conformation_plan.conform_bound_vertices(
                        &self.morph_preview_reference_scratch,
                        &mut self.morph_preview_conform_scratch,
                    ) {
                        Ok(_) => {
                            for (vertex, conformed) in preview_vertices
                                .iter_mut()
                                .zip(&self.morph_preview_conform_scratch)
                            {
                                *vertex = conformed.to_array();
                            }
                        }
                        Err(error) => {
                            if !self.eyelid_conformation_skipped {
                                self.eyelid_conformation_skipped = true;
                                let _ = crate::diagnostics::record(
                                    crate::diagnostics::Severity::Info,
                                    "state",
                                    "eyelid_conformation_skipped",
                                    &format!("eye_value={eye_value:.3}; {error}"),
                                );
                            }
                        }
                    }
                }
            }
        }
        let eyelid_finished = Instant::now();
        self.refresh_vam_full_preview(&preview_vertices)?;
        self.sculpt
            .set_presentation_vertices(&preview_vertices)
            .map_err(|error| error.to_string())?;
        self.workspace
            .update_result_preview_vertices(preview_vertices)
            .map_err(|error| error.to_string())?;
        let scene_finished = Instant::now();
        let serial = self.next_morph_timing_serial;
        self.next_morph_timing_serial = self.next_morph_timing_serial.saturating_add(1);
        self.morph_preview_timing = Some(MorphPreviewTiming {
            serial,
            compose_ms: (compose_finished - total_started).as_secs_f64() * 1_000.0,
            eyelid_ms: (eyelid_finished - compose_finished).as_secs_f64() * 1_000.0,
            scene_ms: (scene_finished - eyelid_finished).as_secs_f64() * 1_000.0,
            total_ms: (scene_finished - total_started).as_secs_f64() * 1_000.0,
        });
        Ok(())
    }
}

impl AppState {
    pub(crate) fn set_look_browser_open(&mut self, open: bool) {
        if open {
            if self.morphs_before_browsing.is_none() {
                self.morphs_before_browsing = Some(self.morph_library.snapshot_values());
            }
        } else {
            self.morphs_before_browsing = None;
        }
        self.morph_look_find_open = open;
    }
}
