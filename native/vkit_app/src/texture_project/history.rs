use super::*;

const TEXTURE_UNDO_STEPS: usize = 512;
pub(super) const TEXTURE_UNDO_BYTES: usize = 1_536 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct TextureUndoSnapshot {
    pub(super) layers: Vec<TextureLayer>,
    pub(super) selected_layer_id: Option<u64>,
    pub(super) resolution: u32,
    pub(super) boundary_feather_pixels: u16,
    pub(super) bake_base: TextureBakeBase,
    pub(super) source_revision: u64,
}

fn snapshot_bytes(snapshot: &TextureUndoSnapshot) -> usize {
    snapshot
        .layers
        .iter()
        .map(|layer| {
            layer.painted.as_ref().map_or(0, |paint| paint.rgba8.len())
                + layer.mask.as_ref().map_or(0, |mask| mask.alpha8.len())
                + layer
                    .edited_image
                    .as_ref()
                    .map_or(0, |image| image.rgba8.len())
        })
        .sum()
}

impl TextureProject {
    fn undo_snapshot(&self) -> TextureUndoSnapshot {
        TextureUndoSnapshot {
            layers: self.layers.clone(),
            selected_layer_id: self.selected_layer_id,
            resolution: self.resolution,
            boundary_feather_pixels: self.boundary_feather_pixels,
            bake_base: self.bake_base,
            source_revision: self.edit_revision,
        }
    }

    pub(crate) fn capture_undo_checkpoint(&self) -> Option<TextureUndoSnapshot> {
        self.undo_transaction
            .is_none()
            .then(|| self.undo_snapshot())
    }

    pub(crate) fn commit_undo_checkpoint(&mut self, checkpoint: Option<TextureUndoSnapshot>) {
        let Some(checkpoint) = checkpoint else {
            return;
        };
        if checkpoint.source_revision != self.edit_revision {
            self.push_undo(checkpoint);
        }
    }

    pub fn begin_undo_transaction(&mut self) {
        if self.undo_transaction.is_none() {
            self.undo_transaction = Some(self.undo_snapshot());
        }
    }

    pub fn end_undo_transaction(&mut self) {
        let checkpoint = self.undo_transaction.take();
        self.commit_undo_checkpoint(checkpoint);

        self.stroke = None;
        self.preview_stroke = None;
    }

    pub const fn edit_transaction_active(&self) -> bool {
        self.undo_transaction.is_some()
    }

    pub fn undo(&mut self) -> bool {
        self.end_undo_transaction();
        if !self.history.can_undo() {
            return false;
        }
        let here = self.undo_snapshot();
        let Some(snapshot) = self.history.undo(here) else {
            return false;
        };
        self.restore(snapshot);
        true
    }

    pub fn redo(&mut self) -> bool {
        self.end_undo_transaction();
        if !self.history.can_redo() {
            return false;
        }
        let here = self.undo_snapshot();
        let Some(snapshot) = self.history.redo(here) else {
            return false;
        };
        self.restore(snapshot);
        true
    }

    #[must_use]
    pub fn history_position(&self) -> (usize, usize) {
        self.history.position()
    }

    fn restore(&mut self, snapshot: TextureUndoSnapshot) {
        self.layers = snapshot.layers;
        self.selected_layer_id = snapshot
            .selected_layer_id
            .filter(|id| self.layers.iter().any(|layer| layer.id == *id))
            .or_else(|| self.layers.first().map(|layer| layer.id));
        self.resolution = normalize_texture_resolution(snapshot.resolution);
        self.boundary_feather_pixels = snapshot.boundary_feather_pixels;
        self.bake_base = snapshot.bake_base;
        self.edit_revision = self.edit_revision.saturating_add(1);
        self.dirty = true;
        self.bake_error = None;
    }

    pub(super) fn push_undo(&mut self, snapshot: TextureUndoSnapshot) {
        self.history.record(snapshot);
        self.history
            .trim(TEXTURE_UNDO_STEPS, TEXTURE_UNDO_BYTES, snapshot_bytes);
    }
}
