use super::*;

impl AppState {
    pub(super) fn begin_sculpt_stage(&mut self) -> Result<(), String> {
        let output = self
            .workspace
            .result_output
            .as_ref()
            .ok_or_else(|| "result output is unavailable".to_owned())?;
        self.sculpt
            .begin(output)
            .map_err(|error| error.to_string())?;
        self.sync_sculpt_preferences();
        self.install_sculpt_restore_basis();
        Ok(())
    }

    pub(super) fn install_sculpt_restore_basis(&mut self) {
        let basis = self.compare_origin().map(<[[f64; 3]]>::to_vec).or_else(|| {
            self.workspace
                .template_ordered_obj()
                .map(|ordered| ordered.vertices.clone())
        });
        let _ = self.sculpt.set_restore_basis(basis);
    }

    pub(super) fn begin_sculpt_stroke(
        &mut self,
        view_direction_local: Option<[f64; 3]>,
        brush_direction_local: Option<[f64; 3]>,
    ) {
        if !self.is_sculpting() || !self.tab_available(Tab::Morph) {
            return;
        }
        if self.history_branch_needs_asking() {
            self.pending_history_branch = true;
            return;
        }
        let result = if view_direction_local.is_none() && brush_direction_local.is_none() {
            self.sculpt.begin_stroke()
        } else {
            self.sculpt
                .begin_stroke_with_directions(view_direction_local, brush_direction_local)
        };
        if let Err(error) = result {
            self.status = StatusMessage::with_detail(
                TextKey::SculptFailed,
                StatusTone::Error,
                error.to_string(),
            );
        }
    }

    #[cfg(test)]
    pub(super) fn sculpt_dab(&mut self, dab: SculptDab) {
        if !self.is_sculpting() || !self.tab_available(Tab::Morph) {
            return;
        }
        match self.sculpt.dab(dab) {
            Ok(0) => {}
            Ok(_) => self.sync_sculpt_preview(),
            Err(error) => {
                self.status = StatusMessage::with_detail(
                    TextKey::SculptFailed,
                    StatusTone::Error,
                    error.to_string(),
                );
            }
        }
    }

    pub(super) fn sculpt_dab_deferred(&mut self, dab: SculptDab) {
        if !self.is_sculpting() || !self.tab_available(Tab::Morph) {
            return;
        }
        let started = Instant::now();
        match self.sculpt.dab(dab) {
            Ok(changed_vertices) => {
                let serial = self.next_sculpt_timing_serial;
                self.next_sculpt_timing_serial = self.next_sculpt_timing_serial.saturating_add(1);
                self.sculpt_dab_timing = Some(SculptDabTiming {
                    serial,
                    dab_ms: started.elapsed().as_secs_f64() * 1_000.0,
                    changed_vertices,
                });
            }
            Err(error) => {
                self.status = StatusMessage::with_detail(
                    TextKey::SculptFailed,
                    StatusTone::Error,
                    error.to_string(),
                );
            }
        }
    }

    pub(super) fn transform_sculpt_vertices(&mut self, pivot: [f64; 3], basis: [[f64; 3]; 3]) {
        if !self.is_sculpting() || !self.tab_available(Tab::Morph) {
            return;
        }
        let held: Vec<u32> = self.sculpt_vertex_selection.iter().copied().collect();
        if held.len() < 2 {
            return;
        }
        match self
            .sculpt
            .transform_vertices(&held, pivot, basis, [0.0; 3])
        {
            Ok(0) => {}
            Ok(_) => self.sync_sculpt_preview(),
            Err(error) => {
                self.status = StatusMessage::with_detail(
                    TextKey::SculptFailed,
                    StatusTone::Error,
                    error.to_string(),
                );
            }
        }
    }

    pub(super) fn move_sculpt_vertices(&mut self, shift: [f64; 3]) {
        if !self.is_sculpting() || !self.tab_available(Tab::Morph) {
            return;
        }
        let held: Vec<u32> = self.sculpt_vertex_selection.iter().copied().collect();
        if held.is_empty() {
            return;
        }
        match self.sculpt.nudge_vertices(&held, shift) {
            Ok(0) => {}
            Ok(_) => self.sync_sculpt_preview(),
            Err(error) => {
                self.status = StatusMessage::with_detail(
                    TextKey::SculptFailed,
                    StatusTone::Error,
                    error.to_string(),
                );
            }
        }
    }

    pub(super) fn end_sculpt_stroke(&mut self) {
        if !self.is_sculpting() || !self.tab_available(Tab::Morph) {
            return;
        }
        match self.sculpt.end_stroke() {
            Ok(_) => self.sync_sculpt_preview(),
            Err(error) => {
                self.status = StatusMessage::with_detail(
                    TextKey::SculptFailed,
                    StatusTone::Error,
                    error.to_string(),
                );
            }
        }
    }

    pub(super) fn undo_detail_edit(&mut self) -> bool {
        if !self.tab_available(Tab::Morph) {
            return false;
        }

        #[derive(Clone, Copy)]
        enum DetailUndo {
            MorphValue,
            MorphReset,
            Sculpt,
            MorphMask,
        }

        let newest = [
            self.morph_value_history
                .back()
                .map(|checkpoint| (checkpoint.seq, DetailUndo::MorphValue)),
            self.morph_reset_undo
                .as_ref()
                .map(|checkpoint| (checkpoint.seq, DetailUndo::MorphReset)),
            self.sculpt
                .top_undo_seq()
                .map(|seq| (seq, DetailUndo::Sculpt)),
            self.morph_mask_history
                .back()
                .map(|step| (step.seq, DetailUndo::MorphMask)),
        ]
        .into_iter()
        .flatten()
        .max_by_key(|(seq, _)| *seq);

        match newest {
            Some((_, DetailUndo::MorphValue)) => self.undo_morph_value(),
            Some((_, DetailUndo::MorphReset)) => self.undo_morph_reset(),
            Some((_, DetailUndo::Sculpt)) => self.undo_sculpt(),
            Some((_, DetailUndo::MorphMask)) => self.undo_morph_mask(),
            None => false,
        }
    }

    pub(super) fn open_morph_mask_step(&mut self) {
        let Some(id) = self.appearance_stack.selected_id else {
            return;
        };
        let Some(layer) = self.appearance_stack.layer(id) else {
            return;
        };
        let step = MorphMaskUndo {
            layer_id: id,
            mask: layer.mask.clone(),
            seq: crate::edit_clock::next_seq(),
        };
        self.clear_detail_forward_history();
        if self.morph_mask_history.len() >= MAX_MORPH_VALUE_UNDO {
            self.morph_mask_history.pop_front();
        }
        self.morph_mask_history.push_back(step);
    }

    pub(super) fn paint_morph_mask(
        &mut self,
        vertices: Vec<(u32, f32)>,
        target: f32,
        amount: f32,
        begins_step: bool,
    ) {
        let mut painted = false;
        if begins_step {
            self.open_morph_mask_step();
        }
        if let Some(id) = self.appearance_stack.selected_id
            && let Some(layer) = self.appearance_stack.layer_mut(id)
        {
            for (vertex, weight) in vertices {
                layer.mask.paint(vertex, target, amount * weight);
            }
            painted = true;
        }
        if painted {
            let _ = crate::diagnostics::record(
                crate::diagnostics::Severity::Debug,
                "state",
                "morph_mask_painted",
                &format!(
                    "layers={}; contributing={}; carved={}",
                    self.appearance_stack.layers.len(),
                    self.appearance_stack.contributing().len(),
                    self.appearance_stack
                        .selected_id
                        .and_then(|id| self.appearance_stack.layer(id))
                        .map_or(0, |layer| layer.mask.carved_count()),
                ),
            );
            self.recompose_appearance_blend();
        }
    }

    pub(super) fn undo_morph_mask(&mut self) -> bool {
        let Some(step) = self.morph_mask_history.pop_back() else {
            return false;
        };
        if let Some(forward) = self.swap_layer_mask(&step) {
            self.morph_mask_forward.push_back(forward);
        }
        true
    }

    pub(super) fn redo_morph_mask(&mut self) -> bool {
        let Some(step) = self.morph_mask_forward.pop_back() else {
            return false;
        };
        if let Some(back) = self.swap_layer_mask(&step) {
            self.morph_mask_history.push_back(back);
        }
        true
    }

    fn swap_layer_mask(&mut self, step: &MorphMaskUndo) -> Option<MorphMaskUndo> {
        let layer = self.appearance_stack.layer_mut(step.layer_id)?;
        let replaced = std::mem::replace(&mut layer.mask, step.mask.clone());
        self.recompose_appearance_blend();
        self.status = StatusMessage::new(TextKey::MorphEditUndone, StatusTone::Info);
        Some(MorphMaskUndo {
            layer_id: step.layer_id,
            mask: replaced,
            seq: step.seq,
        })
    }

    pub(super) fn undo_sculpt(&mut self) -> bool {
        if !self.is_sculpting() || !self.tab_available(Tab::Morph) {
            return false;
        }
        match self.sculpt.undo() {
            Ok(true) => {
                self.sync_sculpt_preview();
                true
            }
            Ok(false) => false,
            Err(error) => {
                self.status = StatusMessage::with_detail(
                    TextKey::SculptFailed,
                    StatusTone::Error,
                    error.to_string(),
                );
                true
            }
        }
    }

    pub(super) fn redo_detail_edit(&mut self) -> bool {
        if !self.tab_available(Tab::Morph) {
            return false;
        }

        #[derive(Clone, Copy)]
        enum DetailRedo {
            MorphValue,
            MorphReset,
            Sculpt,
            MorphMask,
        }

        let oldest = [
            self.morph_value_forward
                .back()
                .map(|checkpoint| (checkpoint.seq, DetailRedo::MorphValue)),
            self.morph_reset_forward
                .as_ref()
                .map(|checkpoint| (checkpoint.seq, DetailRedo::MorphReset)),
            self.sculpt
                .top_redo_seq()
                .map(|seq| (seq, DetailRedo::Sculpt)),
            self.morph_mask_forward
                .back()
                .map(|step| (step.seq, DetailRedo::MorphMask)),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(seq, _)| *seq);

        match oldest {
            Some((_, DetailRedo::MorphValue)) => self.redo_morph_value(),
            Some((_, DetailRedo::MorphReset)) => self.redo_morph_reset(),
            Some((_, DetailRedo::Sculpt)) => self.redo_sculpt(),
            Some((_, DetailRedo::MorphMask)) => self.redo_morph_mask(),
            None => false,
        }
    }

    pub(super) fn clear_detail_forward_history(&mut self) {
        self.morph_mask_forward.clear();
        self.morph_value_forward.clear();
        self.morph_reset_forward = None;
        self.sculpt.clear_forward_history();
    }

    pub(super) fn redo_sculpt(&mut self) -> bool {
        if !self.is_sculpting() || !self.tab_available(Tab::Morph) {
            return false;
        }
        match self.sculpt.redo() {
            Ok(true) => {
                self.sync_sculpt_preview();
                true
            }
            Ok(false) => false,
            Err(error) => {
                self.status = StatusMessage::with_detail(
                    TextKey::SculptFailed,
                    StatusTone::Error,
                    error.to_string(),
                );
                true
            }
        }
    }

    pub(super) fn reset_sculpt(&mut self) {
        if !self.is_sculpting() || !self.tab_available(Tab::Morph) {
            return;
        }
        match self.sculpt.reset() {
            Ok(true) => self.sync_sculpt_preview(),
            Ok(false) => {}
            Err(error) => {
                self.status = StatusMessage::with_detail(
                    TextKey::SculptFailed,
                    StatusTone::Error,
                    error.to_string(),
                );
            }
        }
    }

    #[cfg(test)]
    pub(super) fn apply_sculpt(&mut self) {
        if !self.is_sculpting() || !self.tab_available(Tab::Morph) {
            return;
        }
        if self.morph_library.has_loading_controls() {
            self.status = StatusMessage::new(TextKey::MorphLoading, StatusTone::Warning);
            return;
        }

        match self.compose_current_morphs(ResultPreviewPhase::Morph) {
            Ok(()) => {
                self.active_tab = Tab::Morph;
                self.status = StatusMessage::new(TextKey::SculptApplied, StatusTone::Success);
            }
            Err(detail) => {
                self.status = StatusMessage::with_detail(
                    TextKey::GenerationFailed,
                    StatusTone::Error,
                    detail,
                );
            }
        }
    }

    pub(super) fn sync_sculpt_preview(&mut self) {
        self.morph_preview_dirty = true;
        if let Err(error) = self.compose_current_morphs(ResultPreviewPhase::Morph) {
            self.status =
                StatusMessage::with_detail(TextKey::SculptFailed, StatusTone::Error, error);
        }
    }

    pub(super) fn restore_sculpt_preview(&mut self) -> Result<(), String> {
        let vertices = self
            .sculpt
            .working_mesh()
            .map(|mesh| mesh.vertices.clone())
            .ok_or_else(|| "sculpt working mesh is unavailable".to_owned())?;
        self.sculpt.clear_presentation_vertices();
        self.refresh_vam_full_preview(&vertices)?;
        self.workspace
            .update_result_preview_vertices(vertices)
            .map_err(|error| error.to_string())?;
        self.result_preview_phase = ResultPreviewPhase::Sculpt;
        Ok(())
    }

    pub(super) fn refresh_vam_full_preview(
        &mut self,
        canonical_vertices: &[[f64; 3]],
    ) -> Result<(), String> {
        let has_genital_controls = self
            .morph_library
            .controls()
            .iter()
            .any(|control| control.genital_target.is_some());
        if !has_genital_controls {
            self.vam_full_preview = None;
            return Ok(());
        }
        let provider = self
            .vam_geometry_provider
            .as_ref()
            .map(Arc::clone)
            .ok_or_else(|| {
                "Genital morph preview requires its validated user-owned VaM geometry provider"
                    .to_owned()
            })?;
        let expected_sex = figure_sex_to_geometry_sex(self.figure_sex);
        if provider.sex() != expected_sex {
            return Err(format!(
                "Genital morph preview provider is {:?}, but the current figure is {:?}",
                provider.sex(),
                expected_sex
            ));
        }
        if canonical_vertices.len() != provider.daz_anchor().vertices.len() {
            return Err(format!(
                "Genital morph preview requires {} canonical vertices, got {}",
                provider.daz_anchor().vertices.len(),
                canonical_vertices.len()
            ));
        }
        let mut target = provider.daz_anchor().clone();
        target.vertices.clone_from_slice(canonical_vertices);
        target.validate().map_err(|error| error.to_string())?;
        let mut full = provider
            .compose_vam_output(&target)
            .map_err(|error| error.to_string())?;
        let applications = self
            .morph_library
            .genital_applications()
            .map_err(|error| error.to_string())?;
        apply_genital_applications_to_vam_mesh(&provider, &mut full, &applications)
            .map_err(|error| error.to_string())?;
        self.vam_full_preview = Some(Arc::new(full));
        Ok(())
    }

    pub(super) fn apply_neck_ear_restore_to_result(&mut self) -> Result<(), String> {
        let Some(fit) = self.workspace.fitted_result.clone() else {
            return Ok(());
        };
        let vertices = if self.restore_neck_ears {
            let Some(weights) = self.workspace.neck_ear_restore_weights() else {
                return Ok(());
            };
            let Some(template) = self.workspace.template_ordered_obj() else {
                return Ok(());
            };
            if template.vertices.len() != fit.vertices.len() {
                return Err("template and fit vertex counts disagree".to_owned());
            }
            vkit_core::restore_region::blend_toward_base(
                &template.vertices,
                &fit.vertices,
                &weights,
            )
        } else {
            fit.vertices.clone()
        };
        self.workspace
            .update_result_preview_vertices(vertices)
            .map_err(|error| error.to_string())
    }

    pub(super) fn compose_current_morphs(
        &mut self,
        phase: ResultPreviewPhase,
    ) -> Result<(), String> {
        let morph_available = matches!(
            self.result,
            ResultState::Ready {
                morph_available: true,
                ..
            }
        );
        if morph_available {
            self.rebuild_morph_preview(self.eye_closure)?;
        } else {
            self.restore_sculpt_preview()?;
        }
        self.morph_preview_dirty = false;
        self.result_preview_phase = phase;
        Ok(())
    }

    pub(super) fn commit_save_preview(&mut self) -> Result<(), String> {
        if self.morph_library.has_loading_controls() {
            return Err("morph controls are still loading".to_owned());
        }

        self.sculpt_eye_tracking = false;
        self.manual_eye_gaze = [0.0; 2];
        self.eye_gaze_mode = EyeGazeMode::Manual;

        self.morph_compare_blend = 1.0;

        if !self.texture_project.baked_preview_enabled || self.texture_project.hide_vam_skin_preview
        {
            self.texture_project.baked_preview_enabled = true;
            self.texture_project.hide_vam_skin_preview = false;
            self.texture_project.mark_dirty();
        }
        self.compose_current_morphs(ResultPreviewPhase::Save)?;
        self.sculpt.mark_applied();
        Ok(())
    }
}
