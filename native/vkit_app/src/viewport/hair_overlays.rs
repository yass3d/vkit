use super::*;

pub(super) fn hair_authoring_visible(state: &AppState) -> bool {
    state.shows_authored_hair()
        && state
            .hair_project
            .selected_part()
            .is_some_and(|part| state.hair_scalps.contains_key(&part.provider_name))
}

pub(super) fn add_hair_scalp_guide(
    ui: &Ui,
    rect: Rect,
    camera: TurntableCamera,
    state: &AppState,
    scalp: &crate::hair_project::ScalpAuthoring,
    guide_alpha: f32,
) {
    add_mesh_callback(
        ui,
        SceneView {
            rect,
            camera,
            transform: ModelTransform::default(),
        },
        HAIR_AUTHOR_SCALP_SCENE_KEY,
        Arc::clone(&scalp.surface),
        state.viewport_grading(),
        0,
        MeshDraw {
            color: color_array(crate::theme::COLOR_MUTED, guide_alpha),
            style: RenderStyle::Xray,
            depth_scope: RenderDepthScope::Shared,
        },
    );
}

pub(super) fn draw_hair_part_previews(
    ui: &Ui,
    rect: Rect,
    state: &AppState,
    camera: TurntableCamera,
) {
    if state.hair_hide_strands && state.is_hair_editing() && state.hair_thumbnail.is_none() {
        return;
    }
    let head = state.workspace.result.clone();
    let view = ResultRenderView {
        camera,
        grading: state.viewport_grading(),
        smooth_passes: state.surface_smooth_passes,
    };
    for (index, drawn_part) in state.hair_project.parts.iter().enumerate() {
        let Some(head) = head.as_ref() else {
            break;
        };
        match state.hair_isolated_part() {
            Some(solo) => {
                if solo != drawn_part.id {
                    continue;
                }
            }
            None => {
                if !drawn_part.visible {
                    continue;
                }
            }
        }
        let Some(part_scalp) = state.hair_scalps.get(&drawn_part.provider_name) else {
            continue;
        };
        let Some(preview) = authoring_hair_preview(ui, state, drawn_part, part_scalp) else {
            continue;
        };
        add_hair_callback(
            ui,
            state,
            rect,
            Arc::clone(head),
            preview,
            HAIR_AUTHORING_SCENE_KEY + index as u64,
            view,
            if state.hair_viewport_physics {
                crate::hair_physics::HairSimulation::Every
            } else {
                crate::hair_physics::HairSimulation::Off
            },
        );
    }
}

pub(super) fn add_hair_authoring_overlays(
    ui: &Ui,
    rect: Rect,
    state: &AppState,
    camera: TurntableCamera,
    furniture: bool,
) {
    // The dots mark sockets on the head, so they are read from the wrapped cap.
    let selected = state.hair_project.selected_part().and_then(|part| {
        posed_scalp(ui.ctx(), state, &part.provider_name).map(|scalp| (part, scalp))
    });
    let Some((part, scalp)) = selected else {
        if state.hair_project.parts.is_empty() {
            return;
        }
        draw_hair_part_previews(ui, rect, state, camera);
        return;
    };

    let planted = state
        .hair_project
        .selected_part()
        .map_or(0, |part| part.strands.len());
    let guide_alpha = if planted == 0 {
        0.35
    } else {
        (0.35 - (planted as f32 / 400.0)).max(0.06)
    };
    if furniture {
        add_hair_scalp_guide(ui, rect, camera, state, &scalp, guide_alpha);
    }
    let eye = camera.eye();
    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
    let painter = &painter;
    let normals = scalp.normals();
    draw_hair_part_previews(ui, rect, state, camera);

    if furniture && state.hair_show_streams {
        draw_hair_streams(ui, painter, rect, state, camera);
    }

    if !furniture || !state.hair_show_points {
        return;
    }
    let field = hair_depth_field(ui, state, rect, camera);
    for (index, vertex) in scalp.vertices_cm.iter().enumerate() {
        let world = glam::Vec3::new(vertex[0], vertex[1], vertex[2]);
        let normal = normals
            .get(index)
            .map(|n| glam::Vec3::new(n[0], n[1], n[2]))
            .unwrap_or(glam::Vec3::Z);
        if normal.dot(eye - world) <= 0.0 {
            continue;
        }
        let Some(projected) = camera.project(world, rect) else {
            continue;
        };
        if field.hides(projected.screen, (world - eye).length()) {
            continue;
        }
        let planted = part.strands.contains_key(&(index as u32));
        let (radius, color) = if planted {
            (2.4, crate::theme::COLOR_HAIR_POINT_PLANTED)
        } else {
            (1.8, crate::theme::COLOR_HAIR_POINT_OPEN)
        };
        painter.circle_filled(projected.screen, radius, color);
    }
}

pub(super) const SCALP_TEXTURE_MAX_EDGE: u32 = 2048;

pub(super) fn authoring_scalp_layer(
    ctx: &egui::Context,
    state: &AppState,
    part: &crate::hair_project::HairPart,
    scalp: &crate::hair_project::ScalpAuthoring,
    bed: &crate::hair_preview::HeadBed,
) -> Option<crate::hair_preview::AuthoringScalp> {
    if !part.visible {
        return None;
    }
    let (geometry, anchors) = wrapped_cap(ctx, part.provider_name.as_str(), scalp, bed)?;
    let (builtin_diffuse, builtin_alpha) = builtin_scalp_images(ctx, state, &part.provider_name);
    let mut textures = crate::hair_preview::HairScalpTextures {
        material: crate::hair_export::authoring_scalp_material(part),
        authored_material: true,
        diffuse: builtin_diffuse,
        alpha: builtin_alpha,
        ..crate::hair_preview::HairScalpTextures::default()
    };
    if let Some(path) = &part.scalp_texture.diffuse
        && let Some(image) = custom_scalp_image(ctx, path)
    {
        textures.diffuse = Some(image);
    }
    if let Some(path) = &part.scalp_texture.alpha
        && let Some(image) = custom_scalp_image(ctx, path)
    {
        textures.alpha = Some(image);
    }
    Some(crate::hair_preview::AuthoringScalp {
        geometry,
        anchors,
        textures,
    })
}

type WrappedCap = (
    Arc<vkit_core::vam::HairScalpGeometry>,
    Arc<Vec<crate::hair_preview::ScalpAnchor>>,
);

fn wrapped_cap(
    ctx: &egui::Context,
    provider: &str,
    scalp: &crate::hair_project::ScalpAuthoring,
    bed: &crate::hair_preview::HeadBed,
) -> Option<WrappedCap> {
    let cache_id = Id::new(("vkit.hair.wrapped-cap", fxhash(provider)));
    let generation = bed.generation;
    if let Some((cached_generation, geometry, anchors)) = ctx.data(|data| {
        data.get_temp::<(
            u64,
            Arc<vkit_core::vam::HairScalpGeometry>,
            Arc<Vec<crate::hair_preview::ScalpAnchor>>,
        )>(cache_id)
    }) && cached_generation == generation
    {
        return Some((geometry, anchors));
    }
    let geometry = Arc::new(vkit_core::vam::HairScalpGeometry {
        materials: Vec::new(),
        vertices_cm: scalp.vertices_cm.clone(),
        uvs: scalp.uvs.clone(),
        triangles: scalp.triangles.clone(),
    });
    let anchors = Arc::new(crate::hair_preview::wrap_scalp_to_head(&geometry, bed).ok()?);
    ctx.data_mut(|data| {
        data.insert_temp(
            cache_id,
            (generation, Arc::clone(&geometry), Arc::clone(&anchors)),
        );
    });
    Some((geometry, anchors))
}

pub(super) fn builtin_scalp_images(
    ctx: &egui::Context,
    state: &AppState,
    provider_name: &str,
) -> (
    Option<Arc<crate::skin_preview::SkinImage>>,
    Option<Arc<crate::skin_preview::SkinImage>>,
) {
    type Cached = (
        Option<Arc<crate::skin_preview::SkinImage>>,
        Option<Arc<crate::skin_preview::SkinImage>>,
    );
    let cache_id = Id::new(("vkit.hair.builtin-scalp-images", provider_name));
    if let Some(cached) = ctx.data(|data| data.get_temp::<Cached>(cache_id)) {
        return cached;
    }
    let set = state
        .builtin_scalp_textures
        .iter()
        .find(|set| set.provider_name == provider_name);
    let decode = |reference: Option<&vkit_core::vam::BuiltinTextureRef>, slot: u64| {
        let reference = reference?;
        let decoded =
            vkit_core::vam::load_builtin_texture_rgba(reference, SCALP_TEXTURE_MAX_EDGE).ok()?;
        let revision = crate::skin_preview::revision_domain::BUILTIN_SCALP
            | (fxhash(provider_name) << 8)
            | slot;
        crate::skin_preview::SkinImage::new(revision, decoded.width, decoded.height, decoded.rgba8)
            .ok()
            .map(Arc::new)
    };
    let images: Cached = match set {
        Some(set) => (
            decode(set.diffuse.as_ref(), 1),
            decode(set.alpha.as_ref(), 2),
        ),
        None => (None, None),
    };
    ctx.data_mut(|data| data.insert_temp(cache_id, images.clone()));
    images
}

pub(super) fn custom_scalp_image(
    ctx: &egui::Context,
    path: &std::path::Path,
) -> Option<Arc<crate::skin_preview::SkinImage>> {
    let identity = std::fs::metadata(path)
        .map(|meta| {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |duration| duration.as_secs());
            (meta.len(), mtime)
        })
        .unwrap_or((0, 0));
    type Cached = ((u64, u64), Option<Arc<crate::skin_preview::SkinImage>>);
    let cache_id = Id::new(("vkit.hair.custom-scalp-image", path));
    if let Some((cached_identity, image)) = ctx.data(|data| data.get_temp::<Cached>(cache_id))
        && cached_identity == identity
    {
        return image;
    }
    let revision = crate::skin_preview::revision_domain::CUSTOM_SCALP
        | (fxhash(&path.to_string_lossy()) ^ identity.0 ^ identity.1.rotate_left(17)) >> 8;
    let image = std::fs::read(path)
        .ok()
        .and_then(|bytes| {
            crate::skin_preview::SkinImage::decode_bounded(revision, &bytes, SCALP_TEXTURE_MAX_EDGE)
                .ok()
        })
        .map(Arc::new);
    ctx.data_mut(|data| data.insert_temp(cache_id, (identity, image.clone())));
    image
}

pub(super) fn fxhash(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash & 0x000f_ffff_ffff_ffff
}

type CachedPreview = (u64, bool, bool, f64, Arc<crate::hair_preview::HairPreview>);

/// The cap where it actually stands, for anything the pointer touches.
///
/// The stock cap is the provider's shape. Everything the person sees is that
/// shape wrapped onto the head, so picking, brushing and the guide dots have to
/// read the wrapped one or the click lands where the head used to be.
pub(crate) fn posed_scalp(
    ctx: &egui::Context,
    state: &AppState,
    provider: &str,
) -> Option<Arc<crate::hair_project::ScalpAuthoring>> {
    let stock = state.hair_scalps.get(provider).map(Arc::clone)?;
    // Before a head is loaded there is nothing to wrap to, and the tab still
    // has to draw and still has to be clickable. The stock cap stands in.
    let Some(head) = state.workspace.result.as_ref().map(Arc::clone) else {
        return Some(stock);
    };
    let Some(bed) = head_bed(ctx, state) else {
        return Some(stock);
    };
    let cache_id = Id::new(("vkit.hair.posed-scalp", fxhash(provider)));
    if let Some((generation, posed)) =
        ctx.data(|data| data.get_temp::<(u64, Arc<crate::hair_project::ScalpAuthoring>)>(cache_id))
        && generation == bed.generation
    {
        return Some(posed);
    }
    let Some((_, anchors)) = wrapped_cap(ctx, provider, &stock, &bed) else {
        return Some(stock);
    };
    let vertices: Vec<[f32; 3]> = anchors
        .iter()
        .map(|anchor| crate::hair_renderer::anchored_position(&head, anchor).to_array())
        .collect();
    let Some(posed) = stock.posed(vertices).map(Arc::new) else {
        return Some(stock);
    };
    ctx.data_mut(|data| data.insert_temp(cache_id, (bed.generation, Arc::clone(&posed))));
    Some(posed)
}

fn head_bed(ctx: &egui::Context, state: &AppState) -> Option<Arc<crate::hair_preview::HeadBed>> {
    let template = state.workspace.template_geometry.as_ref().map(Arc::clone)?;
    // Hair is planted on the head the person is looking at, not on the one the
    // figure shipped with. A morph moves the scalp, so the bed is rebuilt with
    // it; without this the guides bind to an unmorphed head and are then
    // pushed about by a collider built from the morphed one.
    let head = state.workspace.result.as_ref().map(Arc::clone)?;
    let revision = head.revision;
    let cache_id = Id::new("vkit.hair.head-bed");
    type Cached = (
        Arc<vkit_core::formats::DazGeometry>,
        u64,
        Arc<crate::hair_preview::HeadBed>,
    );
    if let Some((cached, cached_revision, bed)) = ctx.data(|data| data.get_temp::<Cached>(cache_id))
        && Arc::ptr_eq(&cached, &template)
        && cached_revision == revision
    {
        return Some(bed);
    }
    let bed =
        Arc::new(crate::hair_preview::HeadBed::build_on(&template, (*head.mesh).clone()).ok()?);
    ctx.data_mut(|data| data.insert_temp(cache_id, (template, revision, Arc::clone(&bed))));
    Some(bed)
}

pub(crate) fn authoring_hair_preview(
    ctx: &egui::Context,
    state: &AppState,
    part: &crate::hair_project::HairPart,
    scalp: &crate::hair_project::ScalpAuthoring,
) -> Option<Arc<crate::hair_preview::HairPreview>> {
    let cache_id = Id::new(("vkit.hair.authoring-preview", part.id));
    let revision = part.revision;
    let tinted = part_tint_active(state);
    let lit = state.hair_project.is_part_active(part.id)
        && state.hair_thumbnail.is_none()
        && state.is_hair_editing();
    let now = ctx.input(|input| input.time);
    if let Some((cached_revision, cached_tint, cached_lit, built_at, preview)) =
        ctx.data(|data| data.get_temp::<CachedPreview>(cache_id))
        && cached_tint == tinted
        && cached_lit == lit
        && (cached_revision == revision
            || (state.hair_project.stroke_open()
                && state.hair_thumbnail.is_none()
                && now - built_at < HAIR_STROKE_REBUILD_SECONDS))
    {
        return Some(preview);
    }
    let bed_started = std::time::Instant::now();
    let bed = head_bed(ctx, state)?;
    let bed_ms = bed_started.elapsed().as_secs_f64() * 1000.0;
    let geometry_started = std::time::Instant::now();
    let geometry = crate::hair_export::authoring_guide_geometry(part, scalp, false)?;
    let geometry_ms = geometry_started.elapsed().as_secs_f64() * 1000.0;
    let mut look = crate::hair_export::authoring_look(part);
    if tinted {
        let tint = part_tint_color(part.id);
        let share = if lit {
            TINT_SHARE_ACTIVE
        } else {
            TINT_SHARE_RESTING
        };
        let blend = |base: Option<[f32; 3]>| {
            let base = base.unwrap_or([0.1, 0.1, 0.1]);
            let keep = 1.0 - share;
            Some([
                base[0] * keep + tint[0] * share,
                base[1] * keep + tint[1] * share,
                base[2] * keep + tint[2] * share,
            ])
        };
        look.root_color = blend(look.root_color);
        look.tip_color = blend(look.tip_color);
        if !lit {
            look.root_color = Some(rest_layer(look.root_color));
            look.tip_color = Some(rest_layer(look.tip_color));
        }
    }
    if lit {
        look.root_color = Some(lift_active_layer(look.root_color));
        look.tip_color = Some(lift_active_layer(look.tip_color));
    }
    let scalp_started = std::time::Instant::now();
    let scalp_layer = authoring_scalp_layer(ctx, state, part, scalp, &bed);
    let scalp_ms = scalp_started.elapsed().as_secs_f64() * 1000.0;
    let preview_started = std::time::Instant::now();
    let preview = crate::hair_preview::authoring_hair_preview(
        Arc::new(geometry),
        look,
        crate::hair_export::authoring_physics(part),
        &bed,
        HAIR_PREVIEW_CHILD_BUDGET,
        scalp_layer,
    )
    .ok()
    .map(Arc::new)?;
    let _ = crate::diagnostics::record(
        crate::diagnostics::Severity::Info,
        "hair",
        "preview_built",
        &format!(
            "part={}; bed_ms={bed_ms:.1}; guides_ms={geometry_ms:.1}; scalp_ms={scalp_ms:.1}; preview_ms={:.1}",
            part.id,
            preview_started.elapsed().as_secs_f64() * 1000.0,
        ),
    );
    ctx.data_mut(|data| {
        data.insert_temp(cache_id, (revision, tinted, lit, now, Arc::clone(&preview)));
    });
    Some(preview)
}

const TINT_SHARE_ACTIVE: f32 = 0.72;
const TINT_SHARE_RESTING: f32 = 0.45;
const ACTIVE_LAYER_LIFT: f32 = 0.38;
const ACTIVE_LAYER_SATURATION: f32 = 0.65;
const RESTING_LAYER_FADE: f32 = 0.78;
const RESTING_LAYER_DIM: f32 = 0.82;

pub(super) fn lift_active_layer(colour: Option<[f32; 3]>) -> [f32; 3] {
    let base = colour.unwrap_or([0.1, 0.1, 0.1]);
    let [hue, saturation, value] = crate::hair_settings::rgb_to_hsv(base);
    crate::hair_settings::hsv_to_rgb([
        hue,
        (saturation * (1.0 + ACTIVE_LAYER_SATURATION)).min(1.0),
        value + (1.0 - value) * ACTIVE_LAYER_LIFT,
    ])
}

pub(super) fn rest_layer(colour: Option<[f32; 3]>) -> [f32; 3] {
    let base = colour.unwrap_or([0.1, 0.1, 0.1]);
    let [hue, saturation, value] = crate::hair_settings::rgb_to_hsv(base);
    crate::hair_settings::hsv_to_rgb([
        hue,
        saturation * RESTING_LAYER_FADE,
        value * RESTING_LAYER_DIM,
    ])
}

pub(super) fn part_tint_active(state: &AppState) -> bool {
    state.hair_part_tint && state.hair_thumbnail.is_none() && state.is_hair_editing()
}

pub(super) fn part_tint_ink(part_id: u64) -> egui::Color32 {
    let [r, g, b] = part_tint_color(part_id);
    let channel = |value: f32| (value * 255.0).round().clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgb(channel(r), channel(g), channel(b))
}

pub(super) fn part_tint_color(part_id: u64) -> [f32; 3] {
    let hue = (part_id as f32 * 0.618_034).fract();
    let h = hue * 6.0;
    let x = 1.0 - ((h % 2.0) - 1.0).abs();
    let (r, g, b) = match h as u32 % 6 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [r * 0.7 + 0.3, g * 0.7 + 0.3, b * 0.7 + 0.3]
}

/// How many strands one layer's preview may draw.
///
/// The game draws `hairMultiplier` children on every guide triangle and does
/// not budget at all: one layer of `AKKEVE hair051` alone asks for some
/// twenty-three thousand. At six thousand the preview drew about a quarter of
/// the hair the game does, which is most of why it looked so sparse beside it.
/// The children are enumerated once per rebuild and live in a GPU buffer, so
/// the cost is memory rather than frame time; the cap is here for the styles
/// that would ask for hundreds of thousands.
pub(super) const HAIR_PREVIEW_CHILD_BUDGET: usize = 32_000;
pub(super) const HAIR_STROKE_REBUILD_SECONDS: f64 = 0.045;

pub(super) fn draw_thumbnail_frame(ui: &Ui, state: &mut AppState, rect: Rect) {
    const CONTROL_STRIP: f32 = 64.0;
    const TOP_CLEAR: f32 = 84.0;
    const SIDE_CLEAR: f32 = 24.0;
    let stage = Rect::from_min_max(
        egui::pos2(rect.min.x + SIDE_CLEAR, rect.min.y + TOP_CLEAR),
        egui::pos2(rect.max.x - SIDE_CLEAR, rect.max.y - CONTROL_STRIP),
    );
    let side = stage.width().min(stage.height()).max(64.0);
    let square = Rect::from_center_size(stage.center(), egui::vec2(side, side));
    if let Some(job) = state.hair_thumbnail.as_mut() {
        job.square = Some([square.min.x, square.min.y, square.max.x, square.max.y]);
    }

    let painter = ui.painter_at(rect);
    let dim = Color32::from_black_alpha(140);
    for outside in [
        Rect::from_min_max(rect.min, egui::pos2(rect.max.x, square.min.y)),
        Rect::from_min_max(egui::pos2(rect.min.x, square.max.y), rect.max),
        Rect::from_min_max(
            egui::pos2(rect.min.x, square.min.y),
            egui::pos2(square.min.x, square.max.y),
        ),
        Rect::from_min_max(
            egui::pos2(square.max.x, square.min.y),
            egui::pos2(rect.max.x, square.max.y),
        ),
    ] {
        if outside.width() > 0.0 && outside.height() > 0.0 {
            painter.rect_filled(outside, 0.0, dim);
        }
    }
    painter.rect_stroke(
        square.expand(1.0),
        2.0,
        egui::Stroke::new(2.0, crate::theme::COLOR_TEXT),
        egui::StrokeKind::Outside,
    );

    let shutter = ui
        .interact(
            square,
            Id::new("vkit.hair.thumbnail-shutter"),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::Crosshair);
    if shutter.clicked()
        && let Some(job) = state.hair_thumbnail.as_mut()
    {
        job.shoot = true;
    }

    let mut close = false;
    egui::Area::new(Id::new("vkit.hair.thumbnail-close"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(square.max.x - 2.0, square.min.y - 6.0))
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .show(ui.ctx(), |ui| {
            close = crate::ui_components::icon_button(
                ui,
                crate::ui_components::Icon::Cross,
                crate::i18n::text(state.locale, crate::i18n::TextKey::HairThumbnailSkip),
            )
            .clicked();
        });
    if close {
        state.hair_thumbnail = None;
    }

    egui::Area::new(Id::new("vkit.hair.thumbnail-frame"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(rect.center().x, square.max.y + 8.0))
        .pivot(egui::Align2::CENTER_TOP)
        .show(ui.ctx(), |ui| {
            ui.label(
                egui::RichText::new(crate::i18n::text(
                    state.locale,
                    crate::i18n::TextKey::HairThumbnailPrompt,
                ))
                .color(crate::theme::COLOR_TEXT),
            );
        });
}

pub(super) fn draw_thumbnail_shot_pulse(ui: &Ui, state: &mut AppState, rect: Rect) {
    const PULSE_SECONDS: f64 = 0.28;
    let Some(flash) = state.hair_shot_flash else {
        return;
    };
    let age = ui.input(|input| input.time) - flash.at;
    if age > 1.0 {
        state.hair_shot_flash = None;
        return;
    }
    ui.ctx().request_repaint();
    if age > PULSE_SECONDS {
        return;
    }
    let square = Rect::from_min_max(
        egui::pos2(flash.square[0], flash.square[1]),
        egui::pos2(flash.square[2], flash.square[3]),
    );
    let phase = (age / PULSE_SECONDS) as f32;
    let depth = if phase < 0.35 {
        phase / 0.35
    } else {
        1.0 - (phase - 0.35) / 0.65
    };
    let inset = 10.0 * depth;
    ui.painter_at(rect).rect_stroke(
        square.shrink(inset),
        2.0,
        egui::Stroke::new(2.0 + depth * 1.5, crate::theme::COLOR_TEXT),
        egui::StrokeKind::Outside,
    );
}

const DEPTH_FIELD_CELL: f32 = 4.0;
const DEPTH_FIELD_SLACK_CM: f32 = 2.0;

pub(super) struct HairDepthField {
    rect: Rect,
    cols: usize,
    rows: usize,
    nearest: Vec<f32>,
}

impl HairDepthField {
    fn new(rect: Rect) -> Self {
        let cols = ((rect.width() / DEPTH_FIELD_CELL).ceil() as usize).max(1);
        let rows = ((rect.height() / DEPTH_FIELD_CELL).ceil() as usize).max(1);
        Self {
            rect,
            cols,
            rows,
            nearest: vec![f32::INFINITY; cols * rows],
        }
    }

    fn coords(&self, screen: Pos2) -> Option<(usize, usize)> {
        let x = ((screen.x - self.rect.left()) / DEPTH_FIELD_CELL).floor();
        let y = ((screen.y - self.rect.top()) / DEPTH_FIELD_CELL).floor();
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let (x, y) = (x as usize, y as usize);
        (x < self.cols && y < self.rows).then_some((x, y))
    }

    fn mark(&mut self, screen: Pos2, distance: f32) {
        if let Some((x, y)) = self.coords(screen) {
            let cell = &mut self.nearest[y * self.cols + x];
            if distance < *cell {
                *cell = distance;
            }
        }
    }

    fn draw_span(&mut self, from: (Pos2, f32), to: (Pos2, f32)) {
        let steps = ((from.0 - to.0).length() / DEPTH_FIELD_CELL)
            .ceil()
            .max(1.0);
        let steps = (steps as usize).min(512);
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            self.mark(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        }
    }

    #[cfg(test)]
    pub(super) fn probe(rect: Rect) -> Self {
        Self::new(rect)
    }

    #[cfg(test)]
    pub(super) fn probe_mark(&mut self, screen: Pos2, distance: f32) {
        self.mark(screen, distance);
    }

    #[must_use]
    pub(super) fn hides(&self, screen: Pos2, distance: f32) -> bool {
        let Some((x, y)) = self.coords(screen) else {
            return false;
        };
        let mut nearest = f32::INFINITY;
        for row in y.saturating_sub(1)..=(y + 1).min(self.rows - 1) {
            for col in x.saturating_sub(1)..=(x + 1).min(self.cols - 1) {
                nearest = nearest.min(self.nearest[row * self.cols + col]);
            }
        }
        distance > nearest + DEPTH_FIELD_SLACK_CM
    }
}

type CachedField = (u64, Arc<HairDepthField>);

pub(super) fn hair_depth_field(
    ui: &Ui,
    state: &AppState,
    rect: Rect,
    camera: TurntableCamera,
) -> Arc<HairDepthField> {
    use std::hash::{Hash, Hasher};

    let eye = camera.eye();
    let head = state.workspace.result.clone();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    state.hair_project.edit_revision.hash(&mut hasher);
    state.hair_hide_strands.hash(&mut hasher);
    head.as_deref()
        .map_or(0, |head| head.revision)
        .hash(&mut hasher);
    for bits in [
        eye.x.to_bits(),
        eye.y.to_bits(),
        eye.z.to_bits(),
        rect.width().to_bits(),
        rect.height().to_bits(),
    ] {
        bits.hash(&mut hasher);
    }
    let stamp = hasher.finish();

    let slot = Id::new("vkit.hair.depth-field");
    if let Some((cached_stamp, field)) = ui.data(|data| data.get_temp::<CachedField>(slot))
        && cached_stamp == stamp
    {
        return field;
    }

    let mut field = HairDepthField::new(rect);
    if let Some(head) = head.as_deref() {
        for vertex in &head.mesh.vertices {
            let world = glam::Vec3::new(vertex[0] as f32, vertex[1] as f32, vertex[2] as f32);
            if let Some(projected) = camera.project(world, rect) {
                field.mark(projected.screen, (world - eye).length());
            }
        }
    }
    if !state.hair_hide_strands {
        let solo = state.hair_isolated_part();
        for part in &state.hair_project.parts {
            match solo {
                Some(id) if id != part.id => continue,
                None if !part.visible => continue,
                _ => {}
            }
            for strand in part.strands.values() {
                let mut previous: Option<(Pos2, f32)> = None;
                for point in &strand.points_cm {
                    let world = glam::Vec3::from_array(*point);
                    let Some(projected) = camera.project(world, rect) else {
                        previous = None;
                        continue;
                    };
                    let step = (projected.screen, (world - eye).length());
                    match previous {
                        Some(from) => field.draw_span(from, step),
                        None => field.mark(step.0, step.1),
                    }
                    previous = Some(step);
                }
            }
        }
    }
    let field = Arc::new(field);
    ui.data_mut(|data| data.insert_temp(slot, (stamp, Arc::clone(&field))));
    field
}

/// How big a strand joint is drawn, and how big the one under the hand is.
const STREAM_POINT_RADIUS: f32 = 1.6;
const STREAM_POINT_ACTIVE_RADIUS: f32 = 3.0;

fn draw_hair_streams(
    ui: &Ui,
    painter: &egui::Painter,
    rect: Rect,
    state: &AppState,
    camera: TurntableCamera,
) {
    let tinted = part_tint_active(state);
    let field = hair_depth_field(ui, state, rect, camera);
    let eye = camera.eye();
    for part_id in state.hair_project.editable_parts() {
        let Some(part) = state.hair_project.part(part_id) else {
            continue;
        };
        let ink = if tinted {
            part_tint_ink(part_id)
        } else {
            crate::theme::COLOR_PRIMARY
        };
        for (strand_id, strand) in &part.strands {
            let mut run: Vec<Pos2> = Vec::new();
            let flush = |run: &mut Vec<Pos2>| {
                if run.len() >= 2 {
                    painter.add(egui::Shape::line(
                        std::mem::take(run),
                        egui::Stroke::new(1.0, ink.gamma_multiply(0.75)),
                    ));
                } else {
                    run.clear();
                }
            };
            for (index, point) in strand.points_cm.iter().enumerate() {
                let world = glam::Vec3::from_array(*point);
                match camera.project(world, rect) {
                    Some(projected) if !field.hides(projected.screen, (world - eye).length()) => {
                        run.push(projected.screen);
                        // Every point, not only the one being dragged. You
                        // cannot correct a strand you cannot see the joints of.
                        let editing =
                            state.hair_vertex_selection == Some((part_id, *strand_id, index));
                        painter.circle_filled(
                            projected.screen,
                            if editing {
                                STREAM_POINT_ACTIVE_RADIUS
                            } else {
                                STREAM_POINT_RADIUS
                            },
                            if editing {
                                crate::theme::COLOR_HAIR_POINT_ACTIVE
                            } else {
                                crate::theme::COLOR_HAIR_POINT
                            },
                        );
                    }
                    _ => flush(&mut run),
                }
            }
            flush(&mut run);
        }
    }
}
