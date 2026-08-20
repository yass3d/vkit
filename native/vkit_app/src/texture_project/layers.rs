use super::*;

impl TextureProject {
    pub fn add_image_layer(&mut self, path: PathBuf, source_mode: TextureSourceMode) -> u64 {
        let id = self.take_layer_id();
        self.layers
            .insert(0, TextureLayer::image(id, path, source_mode));
        self.selected_layer_id = Some(id);
        self.set_active_tool(self.active_tool);
        self.mark_dirty();
        id
    }

    pub fn ensure_scan_layer(&mut self, name: String) -> Option<u64> {
        if let Some(existing) = self
            .layers
            .iter()
            .find(|layer| layer.source_mode == TextureSourceMode::ScanMesh)
        {
            let id = existing.id;
            self.invalidate_scan_projection();
            return Some(id);
        }
        let id = self.take_layer_id();
        let mut layer = TextureLayer::scan(id);
        layer.name = name;
        self.layers.insert(0, layer);
        self.selected_layer_id.get_or_insert(id);
        self.mark_dirty();
        Some(id)
    }

    pub(super) fn adopt_scan_atlases(
        &mut self,
        rasters: &BTreeMap<u64, CachedTextureLayerRaster>,
        unmirrored: &BTreeMap<u64, Arc<SkinImage>>,
    ) {
        for layer in &mut self.layers {
            if layer.source_mode != TextureSourceMode::ScanMesh || layer.image.is_some() {
                continue;
            }
            let Some(raster) = rasters.get(&layer.id) else {
                continue;
            };
            if raster.raster_revision != layer.raster_revision {
                continue;
            }
            let atlas = unmirrored
                .get(&layer.id)
                .cloned()
                .unwrap_or_else(|| Arc::clone(&raster.image));
            layer.image = Some(atlas);
        }
    }

    pub fn invalidate_scan_projection(&mut self) {
        let mut touched = false;
        for layer in &mut self.layers {
            if layer.source_mode == TextureSourceMode::ScanMesh {
                layer.invalidate_raster();

                layer.image = None;
                layer.edited_image = None;
                touched = true;
            }
        }
        if touched {
            self.mark_dirty();
        }
    }

    pub fn remove_layer(&mut self, id: u64) -> bool {
        let Some(index) = self.layers.iter().position(|layer| layer.id == id) else {
            return false;
        };
        self.layers.remove(index);
        if self.selected_layer_id == Some(id) {
            self.selected_layer_id = self
                .layers
                .get(index.min(self.layers.len().saturating_sub(1)))
                .map(|layer| layer.id);
        }

        self.set_active_tool(self.active_tool);
        if !self.has_editable_layer() && self.active_tool == TextureTool::Projection {
            self.set_active_tool(TextureTool::PinPair);
        }

        if self.active_tool == TextureTool::PinPair
            && self
                .selected_layer()
                .is_some_and(|layer| layer.painted.is_some())
        {
            self.set_active_tool(TextureTool::Projection);
        }
        self.mark_dirty();
        true
    }

    pub fn select_layer(&mut self, id: u64) {
        if self.layers.iter().any(|layer| layer.id == id) {
            self.selected_layer_id = Some(id);

            self.set_active_tool(self.active_tool);

            if self.active_tool == TextureTool::PinPair
                && self
                    .selected_layer()
                    .is_some_and(|layer| layer.painted.is_some())
            {
                self.set_active_tool(TextureTool::Projection);
            }
        }
    }

    pub fn move_layer_to(&mut self, id: u64, insertion_index: usize) {
        let Some(index) = self.layers.iter().position(|layer| layer.id == id) else {
            return;
        };
        let layer = self.layers.remove(index);
        let adjusted = insertion_index
            .saturating_sub(usize::from(index < insertion_index))
            .min(self.layers.len());
        self.layers.insert(adjusted, layer);
        if adjusted != index {
            self.mark_dirty();
        }
    }

    pub fn set_source_view(&mut self, id: u64, zoom: f32, center: [f32; 2]) {
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == id) else {
            return;
        };
        if !zoom.is_finite() || !center.into_iter().all(f32::is_finite) {
            return;
        }
        layer.source_view_zoom = zoom.clamp(1.0, 32.0);
        layer.source_view_center = center.map(|value| value.clamp(0.0, 1.0));
    }

    pub fn reset_layer(&mut self, layer_id: u64) {
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == layer_id) else {
            return;
        };
        layer.edited_image = None;
        layer.painted = None;
        layer.mask = None;
        layer.mask_preview = None;
        layer.pins.clear();
        layer.adjustments = TextureColorAdjustments::default();
        layer.opacity = 1.0;
        layer.blend_mode = TextureBlendMode::Normal;
        layer.normal_strength = 1.0;
        layer.scalar_invert = false;
        layer.invalidate_raster();
        self.mark_dirty();
    }

    fn take_layer_id(&mut self) -> u64 {
        let id = self.next_layer_id;
        self.next_layer_id = self.next_layer_id.saturating_add(1);
        id
    }
}
