use super::*;

impl TextureProject {
    pub fn add_source_pin(&mut self, point: [f32; 2]) {
        let Some(layer) = self.selected_layer_mut() else {
            return;
        };
        if let Some(pair) = layer.pins.iter_mut().find(|pair| pair.source.is_none()) {
            pair.source = Some(point);
        } else {
            layer.pins.push(TexturePinPair {
                source: Some(point),
                target: None,
            });
        }
        layer.invalidate_raster();
        self.mark_dirty();
    }

    pub fn stamp_projection(
        &mut self,
        source: &SkinImage,
        triangles: &[vkit_core::texture_bake::ProjectedTriangle],
        brush: vkit_core::texture_bake::ProjectionBrush,
        to_source: impl Fn([f32; 2]) -> Option<[f32; 2]>,
    ) -> usize {
        let edge = self.resolution;

        let view =
            match vkit_core::pixels::RgbaView::new(&source.rgba8, source.width, source.height) {
                Ok(view) => view,
                Err(error) => {
                    self.bake_error = Some(format!("projection source is unusable: {error}"));
                    return 0;
                }
            };
        let Some(layer) = self.selected_layer_mut() else {
            return 0;
        };

        if layer.source_mode != TextureSourceMode::LandmarkPins {
            return 0;
        }

        if brush.erase && layer.painted.is_none() {
            return 0;
        }

        let painted = match layer.painted.as_mut() {
            Some(paint) => {
                let (width, height) = (paint.width, paint.height);
                let rgba8 = Arc::make_mut(&mut paint.rgba8);
                let painted = vkit_core::texture_bake::stamp_projection_onto_g2(
                    rgba8.as_mut_slice(),
                    width,
                    height,
                    view,
                    triangles,
                    brush,
                    to_source,
                );
                if painted > 0 {
                    paint.revision = next_paint_revision();
                }
                painted
            }
            None => {
                let mut rgba8 = vec![0; edge as usize * edge as usize * 4];
                let painted = vkit_core::texture_bake::stamp_projection_onto_g2(
                    rgba8.as_mut_slice(),
                    edge,
                    edge,
                    view,
                    triangles,
                    brush,
                    to_source,
                );
                if painted > 0 {
                    layer.painted = Some(TextureLayerPaint {
                        revision: next_paint_revision(),
                        width: edge,
                        height: edge,
                        rgba8: Arc::new(rgba8),
                    });
                }
                painted
            }
        };
        if painted == 0 {
            return 0;
        }

        layer.painted_regions.clear();

        layer.pins.clear();
        layer.mask_preview = None;
        layer.invalidate_raster();
        self.mark_dirty();
        painted
    }

    pub fn add_target_pin(&mut self, target: TextureTargetPin) {
        let Some(layer) = self.selected_layer_mut() else {
            return;
        };
        if let Some(pair) = layer.pins.iter_mut().find(|pair| pair.target.is_none()) {
            pair.target = Some(target);
        } else {
            layer.pins.push(TexturePinPair {
                source: None,
                target: Some(target),
            });
        }

        layer.painted = None;
        layer.invalidate_raster();
        self.mark_dirty();
    }

    pub fn move_source_pin(&mut self, index: usize, point: [f32; 2]) {
        if let Some(layer) = self.selected_layer_mut()
            && let Some(pair) = layer.pins.get_mut(index)
        {
            pair.source = Some(point);

            layer.painted = None;
            layer.invalidate_raster();
            self.mark_dirty();
        }
    }

    pub fn move_target_pin(&mut self, index: usize, target: TextureTargetPin) {
        if let Some(layer) = self.selected_layer_mut()
            && let Some(pair) = layer.pins.get_mut(index)
        {
            pair.target = Some(target);

            layer.painted = None;
            layer.invalidate_raster();
            self.mark_dirty();
        }
    }

    pub fn remove_pin(&mut self, index: usize) {
        if let Some(layer) = self.selected_layer_mut()
            && index < layer.pins.len()
        {
            layer.pins.remove(index);
            layer.invalidate_raster();
            self.mark_dirty();
        }
    }

    pub fn broadcast_pins_from_selected(&mut self) -> usize {
        let Some(source_id) = self.selected_layer_id else {
            return 0;
        };
        let Some(pins) = self
            .layers
            .iter()
            .find(|layer| layer.id == source_id)
            .map(|layer| layer.pins.clone())
        else {
            return 0;
        };
        if pins.is_empty() {
            return 0;
        }
        let mut copied = 0;
        for layer in &mut self.layers {
            if layer.id == source_id {
                continue;
            }
            layer.pins = pins.clone();

            layer.painted = None;
            layer.invalidate_raster();
            copied += 1;
        }
        if copied > 0 {
            self.mark_dirty();
        }
        copied
    }
}
