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
        let basis = self
            .workspace
            .template_ordered_obj()
            .map(|ordered| ordered.vertices.clone());
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
        ]
        .into_iter()
        .flatten()
        .max_by_key(|(seq, _)| *seq);

        match newest {
            Some((_, DetailUndo::MorphValue)) => self.undo_morph_value(),
            Some((_, DetailUndo::MorphReset)) => self.undo_morph_reset(),
            Some((_, DetailUndo::Sculpt)) => self.undo_sculpt(),
            None => false,
        }
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
}
