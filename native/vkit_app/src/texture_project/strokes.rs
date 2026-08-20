use super::*;

const EDITED_REGION_HISTORY: usize = 96;

impl TextureProject {
    pub fn add_mask_dab(
        &mut self,
        layer_id: u64,
        uv: [f32; 2],
        source: Option<[f32; 2]>,
        subtract: bool,
    ) {
        if !uv
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            || source.is_some_and(|point| {
                !point
                    .into_iter()
                    .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            })
        {
            return;
        }
        let radius = self.mask_brush_radius.clamp(0.002, 0.25);
        let falloff = self.mask_brush_falloff;
        let opacity = self.mask_brush_opacity.clamp(0.01, 1.0);
        let mask_edge = self.resolution;
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == layer_id) else {
            return;
        };
        let coverage_size = layer.mask.as_ref().map_or(
            RasterSize {
                width: mask_edge,
                height: mask_edge,
            },
            |mask| RasterSize {
                width: mask.width,
                height: mask.height,
            },
        );
        let dab = TextureMaskDab {
            uv,
            radius,
            falloff,
            opacity,
            add: !subtract,
            source,
        };

        let coverage = stroke_coverage(
            &mut self.stroke,
            layer_id,
            TextureTool::MaskBrush,
            coverage_size,
        );
        if source.is_some() {
            apply_mask_preview_dab(layer, dab, &mut self.preview_stroke);
        }
        apply_layer_mask_dab(layer, mask_edge, dab, coverage);
        self.mark_dirty();
    }

    pub fn clear_mask(&mut self, layer_id: u64) {
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == layer_id) else {
            return;
        };
        if layer.mask.take().is_some() {
            reset_mask_preview(layer);
            self.mark_dirty();
        }
    }

    pub fn end_clone_stroke(&mut self) {
        self.clone_offset = None;
    }

    pub fn set_clone_sample_on_surface(
        &mut self,
        point: [f32; 2],
        triangle_index: u32,
        barycentric: [f64; 3],
    ) {
        self.set_clone_sample(point);
        if self.clone_sample.is_some() {
            self.clone_sample_surface = Some((triangle_index, barycentric));
        }
    }

    pub fn set_clone_sample(&mut self, point: [f32; 2]) {
        if point
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        {
            self.clone_sample = Some(point);
            self.clone_offset = None;
            self.clone_sample_surface = None;
        }
    }

    pub fn apply_retouch_dab(
        &mut self,
        layer_id: u64,
        tool: TextureTool,
        point: [f32; 2],
        reverse: bool,
    ) {
        if !matches!(
            tool,
            TextureTool::CloneStamp | TextureTool::DodgeBurn | TextureTool::Sponge
        ) || !point
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        {
            return;
        }
        let radius = self.mask_brush_radius.clamp(0.002, 0.25);
        let falloff = self.mask_brush_falloff;
        let opacity = self.mask_brush_opacity.clamp(0.01, 1.0);

        let source_offset = matches!(tool, TextureTool::CloneStamp)
            .then(|| {
                let offset = self.clone_offset.or_else(|| {
                    self.clone_sample
                        .map(|sample| [point[0] - sample[0], point[1] - sample[1]])
                })?;
                self.clone_offset = Some(offset);
                Some(offset)
            })
            .flatten();
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == layer_id) else {
            return;
        };

        if let Some(paint) = layer.painted.as_mut() {
            let size = RasterSize {
                width: paint.width,
                height: paint.height,
            };
            let coverage = stroke_coverage(&mut self.stroke, layer_id, tool, size);
            let rgba8 = Arc::make_mut(&mut paint.rgba8);
            let touched = apply_retouch_pixels(
                rgba8,
                size,
                RetouchStroke {
                    tool,
                    point,
                    clone_offset: source_offset,
                    reverse,
                },
                BrushDab {
                    radius,
                    falloff,
                    opacity,
                },
                coverage,
            );
            paint.revision = next_paint_revision();
            let revision = paint.revision;
            if let Some(touched) = touched {
                layer.painted_regions.push_back((revision, touched));
                while layer.painted_regions.len() > EDITED_REGION_HISTORY {
                    layer.painted_regions.pop_front();
                }
            } else {
                layer.painted_regions.clear();
            }
            layer.invalidate_raster();
            self.mark_dirty();
            return;
        }
        if layer.image.is_none() {
            return;
        }
        if layer.edited_image.is_none() {
            layer.edited_image = layer.image.as_ref().map(|image| {
                Arc::new(SkinImage {
                    revision: image.revision.wrapping_add(1),
                    width: image.width,
                    height: image.height,
                    rgba8: Arc::new(image.rgba8.as_ref().clone()),
                    uv_orientation: image.uv_orientation,
                })
            });
        }
        let Some(image) = layer.edited_image.as_mut() else {
            return;
        };
        let image = Arc::make_mut(image);
        let size = RasterSize {
            width: image.width,
            height: image.height,
        };
        let coverage = stroke_coverage(&mut self.stroke, layer_id, tool, size);
        let rgba8 = Arc::make_mut(&mut image.rgba8);
        let touched = apply_retouch_pixels(
            rgba8,
            size,
            RetouchStroke {
                tool,
                point,
                clone_offset: source_offset,
                reverse,
            },
            BrushDab {
                radius,
                falloff,
                opacity,
            },
            coverage,
        );
        image.revision = image.revision.wrapping_add(1);
        let revision = image.revision;
        if let Some(touched) = touched {
            layer.edited_regions.push_back((revision, touched));
            while layer.edited_regions.len() > EDITED_REGION_HISTORY {
                layer.edited_regions.pop_front();
            }
        } else {
            layer.edited_regions.clear();
        }
        layer.invalidate_raster();
        self.mark_dirty();
    }
}
