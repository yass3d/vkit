use super::*;

#[derive(Clone)]
struct ProjectionCanvasTextureCache {
    revision: u64,
    width: u32,
    height: u32,
    handle: TextureHandle,
}

pub(super) fn projection_canvas_texture_handle(
    ui: &Ui,
    layer: &TextureLayer,
    paint: &crate::texture_project::TextureLayerPaint,
) -> TextureHandle {
    let size = [paint.width as usize, paint.height as usize];
    let id = Id::new(("vkit.texture.projection-canvas", layer.id));
    if let Some(mut cache) = ui.data(|data| data.get_temp::<ProjectionCanvasTextureCache>(id)) {
        if cache.revision != paint.revision
            || cache.width != paint.width
            || cache.height != paint.height
        {
            let patch = (cache.width == paint.width && cache.height == paint.height)
                .then(|| paint_atlas_region_since(layer, cache.revision, paint.revision))
                .flatten();
            match patch {
                Some([min_x, min_y, max_x, max_y]) => cache.handle.set_partial(
                    [min_x as usize, min_y as usize],
                    rgba_region_color_image(
                        &paint.rgba8,
                        paint.width,
                        [min_x, min_y, max_x, max_y],
                    ),
                    TextureOptions::LINEAR,
                ),
                None => cache.handle.set(
                    egui::ColorImage::from_rgba_unmultiplied(size, &paint.rgba8),
                    TextureOptions::LINEAR,
                ),
            }
            cache.revision = paint.revision;
            cache.width = paint.width;
            cache.height = paint.height;
            ui.data_mut(|data| data.insert_temp(id, cache.clone()));
        }
        return cache.handle;
    }
    let handle = ui.ctx().load_texture(
        format!("vkit-projection-canvas-{}", layer.id),
        egui::ColorImage::from_rgba_unmultiplied(size, &paint.rgba8),
        TextureOptions::LINEAR,
    );
    let cache = ProjectionCanvasTextureCache {
        revision: paint.revision,
        width: paint.width,
        height: paint.height,
        handle: handle.clone(),
    };
    ui.data_mut(|data| data.insert_temp(id, cache));
    handle
}

pub(super) fn paint_atlas_region_since(
    layer: &TextureLayer,
    from: u64,
    to: u64,
) -> Option<[u32; 4]> {
    region_union_since(&layer.painted_regions, from, to)
}

pub(super) fn mask_preview_texture_handle(
    ui: &Ui,
    layer_id: u64,
    image: &SkinImage,
) -> TextureHandle {
    crate::ui_components::stamped_texture(
        ui,
        "vkit.texture.mask-preview",
        layer_id,
        (image.revision, image.width, image.height),
        TextureOptions::LINEAR,
        || {
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width as usize, image.height as usize],
                &image.rgba8,
            )
        },
    )
}

pub(crate) fn projection_stencil_rect(state: &AppState, viewport: Rect) -> Option<Rect> {
    let layer = state.texture_project.selected_layer()?;
    let image = layer.edited_image.as_ref().or(layer.image.as_ref())?;
    if image.width == 0 || image.height == 0 {
        return None;
    }

    let bounds = viewport.shrink(24.0);
    if bounds.width() <= 1.0 || bounds.height() <= 1.0 {
        return None;
    }
    let scale = (bounds.width() / image.width as f32).min(bounds.height() / image.height as f32);
    let size = vec2(image.width as f32 * scale, image.height as f32 * scale);
    Some(Rect::from_center_size(bounds.center(), size))
}

pub(crate) fn paint_projection_stencil(ui: &Ui, state: &AppState, rect: Rect) {
    let Some(layer) = state.texture_project.selected_layer() else {
        return;
    };
    let Some(image) = layer.edited_image.as_ref().or(layer.image.as_ref()) else {
        return;
    };
    let texture = adjusted_source_texture_handle(ui, layer, image);
    let alpha = state.texture_project.projection_opacity.clamp(0.0, 1.0);
    let corners = projection_stencil_corners(state, rect);
    let tint = Color32::WHITE.gamma_multiply(alpha);

    ui.painter()
        .add(egui::Shape::mesh(stencil_quad(texture.id(), corners, tint)));

    for index in 0..4 {
        ui.painter().line_segment(
            [corners[index], corners[(index + 1) % 4]],
            Stroke::new(1.0, COLOR_PRIMARY),
        );
    }
}

pub(super) fn stencil_quad(
    texture: egui::TextureId,
    corners: [Pos2; 4],
    tint: Color32,
) -> egui::epaint::Mesh {
    let mut mesh = egui::epaint::Mesh::with_texture(texture);

    for (corner, uv) in corners
        .into_iter()
        .zip([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
    {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: corner,
            uv: pos2(uv[0], uv[1]),
            color: tint,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    mesh
}

pub(super) fn projection_stencil_corners(state: &AppState, rect: Rect) -> [Pos2; 4] {
    let placement = state.texture_project.projection_placement;
    let centre = rect.center() + egui::vec2(placement.offset[0], placement.offset[1]);
    let half = egui::vec2(
        rect.width() * placement.scale * 0.5,
        rect.height() * placement.scale * 0.5,
    );
    let (sine, cosine) = placement.rotation.sin_cos();
    [
        egui::vec2(-half.x, -half.y),
        egui::vec2(half.x, -half.y),
        egui::vec2(half.x, half.y),
        egui::vec2(-half.x, half.y),
    ]
    .map(|offset| {
        centre
            + egui::vec2(
                offset.x * cosine - offset.y * sine,
                offset.x * sine + offset.y * cosine,
            )
    })
}

#[derive(Clone)]
struct AdjustedTextureCache {
    revision: u64,
    width: u32,
    height: u32,
    adjustments: TextureColorAdjustments,
    handle: TextureHandle,
}

pub(super) fn adjusted_source_texture_handle(
    ui: &Ui,
    layer: &TextureLayer,
    image: &SkinImage,
) -> TextureHandle {
    let adjustments = if layer.channel.is_color() {
        layer.adjustments
    } else {
        TextureColorAdjustments::default()
    };
    let build = || {
        let size = [image.width as usize, image.height as usize];
        if adjustments == TextureColorAdjustments::default() {
            egui::ColorImage::from_rgba_unmultiplied(size, &image.rgba8)
        } else {
            let mut rgba = image.rgba8.as_ref().clone();
            apply_color_adjustments(&mut rgba, adjustments);
            egui::ColorImage::from_rgba_unmultiplied(size, &rgba)
        }
    };
    let id = Id::new(("vkit.texture.source-view", layer.id));
    if let Some(mut cache) = ui.data(|data| data.get_temp::<AdjustedTextureCache>(id)) {
        if cache.revision != image.revision
            || cache.width != image.width
            || cache.height != image.height
            || cache.adjustments != adjustments
        {
            let patch = (cache.width == image.width
                && cache.height == image.height
                && cache.adjustments == adjustments)
                .then(|| painted_region_since(layer, cache.revision, image.revision))
                .flatten();
            match patch {
                Some([min_x, min_y, max_x, max_y]) => {
                    cache.handle.set_partial(
                        [min_x as usize, min_y as usize],
                        region_color_image(image, [min_x, min_y, max_x, max_y], adjustments),
                        TextureOptions::LINEAR,
                    );
                }
                None => cache.handle.set(build(), TextureOptions::LINEAR),
            }
            cache.revision = image.revision;
            cache.width = image.width;
            cache.height = image.height;
            cache.adjustments = adjustments;
            ui.data_mut(|data| data.insert_temp(id, cache.clone()));
        }
        return cache.handle;
    }
    let handle = ui.ctx().load_texture(
        format!("vkit-texture-source-{}", layer.id),
        build(),
        TextureOptions::LINEAR,
    );
    ui.data_mut(|data| {
        data.insert_temp(
            id,
            AdjustedTextureCache {
                revision: image.revision,
                width: image.width,
                height: image.height,
                adjustments,
                handle: handle.clone(),
            },
        )
    });
    handle
}

pub(super) fn painted_region_since(layer: &TextureLayer, from: u64, to: u64) -> Option<[u32; 4]> {
    region_union_since(&layer.edited_regions, from, to)
}

pub(super) fn region_union_since(
    regions: &std::collections::VecDeque<(u64, [u32; 4])>,
    from: u64,
    to: u64,
) -> Option<[u32; 4]> {
    if to <= from {
        return None;
    }
    let mut expected = from.wrapping_add(1);
    let mut bounds: Option<[u32; 4]> = None;
    for (revision, region) in regions.iter().skip_while(|(revision, _)| *revision <= from) {
        if *revision != expected {
            return None;
        }
        expected = revision.wrapping_add(1);
        bounds = Some(match bounds {
            Some(current) => [
                current[0].min(region[0]),
                current[1].min(region[1]),
                current[2].max(region[2]),
                current[3].max(region[3]),
            ],
            None => *region,
        });
    }
    (expected == to.wrapping_add(1)).then_some(bounds).flatten()
}

pub(super) fn region_color_image(
    image: &SkinImage,
    region: [u32; 4],
    adjustments: TextureColorAdjustments,
) -> egui::ColorImage {
    let (size, mut rgba) = crop_region(&image.rgba8, image.width, region);
    if adjustments != TextureColorAdjustments::default() {
        apply_color_adjustments(&mut rgba, adjustments);
    }
    egui::ColorImage::from_rgba_unmultiplied(size, &rgba)
}

pub(super) fn rgba_region_color_image(
    rgba8: &[u8],
    width: u32,
    region: [u32; 4],
) -> egui::ColorImage {
    let (size, rgba) = crop_region(rgba8, width, region);
    egui::ColorImage::from_rgba_unmultiplied(size, &rgba)
}

pub(super) fn crop_region(
    rgba8: &[u8],
    source_width: u32,
    [min_x, min_y, max_x, max_y]: [u32; 4],
) -> ([usize; 2], Vec<u8>) {
    let width = (max_x - min_x + 1) as usize;
    let height = (max_y - min_y + 1) as usize;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in min_y..=max_y {
        let row = (y as usize * source_width as usize + min_x as usize) * 4;
        rgba.extend_from_slice(&rgba8[row..row + width * 4]);
    }
    ([width, height], rgba)
}

const PROJECTION_THUMBNAIL_EDGE: u32 = 128;

const PROJECTION_THUMBNAIL_MIN_INTERVAL: f64 = 0.25;

#[derive(Clone)]
struct ProjectionThumbnailCache {
    revision: u64,
    refreshed_at: f64,
    handle: TextureHandle,
}

pub(super) fn paint_projection_thumbnail(
    ui: &Ui,
    rect: Rect,
    layer_id: u64,
    paint: &crate::texture_project::TextureLayerPaint,
    stroke_in_progress: bool,
) {
    ui.painter().rect_filled(rect, CONTROL_RADIUS, COLOR_BG);
    let texture = projection_thumbnail_handle(ui, layer_id, paint, stroke_in_progress);
    ui.painter().image(
        texture.id(),
        rect,
        Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    ui.painter().rect_stroke(
        rect,
        CONTROL_RADIUS,
        Stroke::new(1.0, COLOR_BORDER),
        egui::StrokeKind::Inside,
    );
}

pub(super) fn projection_thumbnail_handle(
    ui: &Ui,
    layer_id: u64,
    paint: &crate::texture_project::TextureLayerPaint,
    stroke_in_progress: bool,
) -> TextureHandle {
    let id = Id::new(("vkit.texture.projection-thumbnail", layer_id));
    let now = ui.input(|input| input.time);
    let cached = ui.data(|data| data.get_temp::<ProjectionThumbnailCache>(id));
    if let Some(cache) = &cached {
        if cache.revision == paint.revision {
            return cache.handle.clone();
        }
        if stroke_in_progress {
            return cache.handle.clone();
        }
        let held_for = now - cache.refreshed_at;
        if held_for < PROJECTION_THUMBNAIL_MIN_INTERVAL {
            ui.ctx().request_repaint_after(Duration::from_secs_f64(
                PROJECTION_THUMBNAIL_MIN_INTERVAL - held_for,
            ));
            return cache.handle.clone();
        }
    }
    let image = projection_thumbnail_image(paint);
    match cached {
        Some(mut cache) => {
            cache.handle.set(image, TextureOptions::LINEAR);
            cache.revision = paint.revision;
            cache.refreshed_at = now;
            let handle = cache.handle.clone();
            ui.data_mut(|data| data.insert_temp(id, cache));
            handle
        }
        None => {
            let handle = ui.ctx().load_texture(
                format!("vkit-projection-thumbnail-{layer_id}"),
                image,
                TextureOptions::LINEAR,
            );
            ui.data_mut(|data| {
                data.insert_temp(
                    id,
                    ProjectionThumbnailCache {
                        revision: paint.revision,
                        refreshed_at: now,
                        handle: handle.clone(),
                    },
                )
            });
            handle
        }
    }
}

pub(super) fn projection_thumbnail_image(
    paint: &crate::texture_project::TextureLayerPaint,
) -> egui::ColorImage {
    let edge = PROJECTION_THUMBNAIL_EDGE.min(paint.width).min(paint.height);
    if edge == 0 || paint.rgba8.len() != paint.width as usize * paint.height as usize * 4 {
        return egui::ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 0]);
    }
    let mut rgba = Vec::with_capacity(edge as usize * edge as usize * 4);
    for y in 0..edge {
        let row = (u64::from(y) * u64::from(paint.height) / u64::from(edge)) as usize
            * paint.width as usize
            * 4;
        for x in 0..edge {
            let offset =
                row + (u64::from(x) * u64::from(paint.width) / u64::from(edge)) as usize * 4;
            rgba.extend_from_slice(&paint.rgba8[offset..offset + 4]);
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([edge as usize, edge as usize], &rgba)
}

pub(super) fn paint_thumbnail(ui: &Ui, rect: Rect, layer_id: u64, image: Option<&SkinImage>) {
    ui.painter().rect_filled(rect, CONTROL_RADIUS, COLOR_BG);
    if let Some(image) = image {
        crate::ui_components::paint_thumbnail_image(
            ui,
            rect,
            TEXTURE_THUMBNAIL_NS,
            layer_id,
            image,
        );
    } else {
        paint_icon(
            ui.painter(),
            rect.shrink(8.0),
            Icon::HeadTexture,
            COLOR_MUTED,
        );
    }
    ui.painter().rect_stroke(
        rect,
        CONTROL_RADIUS,
        Stroke::new(1.0, COLOR_BORDER),
        egui::StrokeKind::Inside,
    );
}
