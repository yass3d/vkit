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
            bed_markers: None,
            markers: None,
            lines: None,
        },
    );
}

fn settle_capture(state: &AppState, part_id: u64) -> Option<crate::hair_settle::Capture> {
    if state.hair_settle_seconds > 0.0 {
        return None;
    }
    match state.hair_settle_wanted.as_ref()? {
        crate::hair_settle::SettleScope::Everything => Some(crate::hair_settle::Capture {
            part_id,
            sink: state.hair_settle_sink.clone(),
        }),
    }
}

pub(super) fn draw_hair_part_previews(
    ui: &Ui,
    rect: Rect,
    state: &AppState,
    camera: TurntableCamera,
) {
    if state.hair_hide_strands && state.hair_thumbnail.is_none() {
        return;
    }
    warm_builtin_scalp_images(ui, state);
    let head = state.workspace.result.clone();
    let view = ResultRenderView {
        camera,
        grading: state.viewport_grading(),
        smooth_passes: state.drawn_smooth_passes(),
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
            if state.hair_viewport_physics && state.is_hair_editing() {
                crate::hair_physics::HairSimulation::Every
            } else {
                crate::hair_physics::HairSimulation::Off
            },
            settle_capture(state, drawn_part.id),
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

    let scale = ui.ctx().pixels_per_point();
    let mut lines = Vec::new();
    let mut points = Vec::new();
    let mut bed = Vec::new();
    if furniture && state.hair_project.active_tool == crate::hair_project::HairTool::Rigidity {
        stiffness_ribbons(state, &mut lines);
    } else if furniture && state.hair_show_streams {
        hair_streams(state, &mut lines, &mut points, scale, Some((camera, rect)));
    }
    if furniture && state.hair_project.active_tool == crate::hair_project::HairTool::Vertex {
        super::hair_vertex::handles(state, &mut points, scale, Some((camera, rect)));
    }
    if furniture && state.hair_show_points {
        let pitch = scalp_pitch(&scalp);
        for (index, vertex) in scalp.vertices_cm.iter().enumerate() {
            let world = glam::Vec3::new(vertex[0], vertex[1], vertex[2]);
            let normal = normals
                .get(index)
                .map(|n| glam::Vec3::new(n[0], n[1], n[2]))
                .unwrap_or(glam::Vec3::Z);
            if normal.dot(eye - world) <= 0.0 {
                continue;
            }
            let planted = part.strands.contains_key(&(index as u32));
            let color = if planted {
                crate::theme::COLOR_HAIR_POINT_PLANTED
            } else {
                crate::theme::COLOR_HAIR_POINT_OPEN
            };
            let radius = scalp_point_size(planted).points(camera, rect, world, pitch);
            bed.push(crate::renderer::MarkerInstance {
                position: [world.x, world.y, world.z],
                radius: radius * scale,
                fill: rgba(color),
                ring: rgba(color),
                shape: crate::renderer::MarkerInstance::SQUARE,
            });
        }
    }
    draw_hair_part_previews(ui, rect, state, camera);
    place_hair_overlays(ui, rect, state, camera, lines, bed, points);

    if furniture && state.hair_show_colliders {
        draw_body_capsules(ui, painter, rect, state, camera);
    }
}

fn place_hair_overlays(
    ui: &Ui,
    rect: Rect,
    state: &AppState,
    camera: TurntableCamera,
    lines: Vec<crate::renderer::LineInstance>,
    bed: Vec<crate::renderer::MarkerInstance>,
    points: Vec<crate::renderer::MarkerInstance>,
) {
    if lines.is_empty() && points.is_empty() && bed.is_empty() {
        return;
    }
    let Some(head) = overlay_mesh(state) else {
        return;
    };
    super::render_callbacks::add_overlay_callback(
        ui,
        rect,
        HAIR_OVERLAY_SCENE_KEY,
        head,
        ResultRenderView {
            camera,
            grading: state.viewport_grading(),
            smooth_passes: state.drawn_smooth_passes(),
        },
        super::render_callbacks::OverlayGeometry {
            bed: (!bed.is_empty()).then(|| std::sync::Arc::new(bed)),
            lines: (!lines.is_empty()).then(|| std::sync::Arc::new(lines)),
            markers: (!points.is_empty()).then(|| std::sync::Arc::new(points)),
        },
    );
}

pub(super) fn overlay_mesh(state: &AppState) -> Option<Arc<crate::scene::SurfaceMesh>> {
    state.workspace.result.clone().or_else(|| {
        state
            .hair_project
            .selected_part()
            .and_then(|part| state.hair_scalps.get(&part.provider_name))
            .map(|scalp| Arc::clone(&scalp.surface))
    })
}

const HAIR_OVERLAY_SCENE_KEY: u64 = 0x7161_0000_0000_0000;

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
    let cache_id = builtin_scalp_cache_id(provider_name);
    if let Some(cached) = ctx.data(|data| data.get_temp::<Cached>(cache_id)) {
        return cached;
    }
    let images = decode_builtin_scalp_images(state, provider_name);
    ctx.data_mut(|data| data.insert_temp(cache_id, images.clone()));
    images
}

type ScalpSheets = (
    Option<Arc<crate::skin_preview::SkinImage>>,
    Option<Arc<crate::skin_preview::SkinImage>>,
);

fn builtin_scalp_cache_id(provider_name: &str) -> Id {
    Id::new(("vkit.hair.builtin-scalp-images", provider_name))
}

fn decode_builtin_scalp_images(state: &AppState, provider_name: &str) -> ScalpSheets {
    let Some(set) = state
        .builtin_scalp_textures
        .iter()
        .find(|set| set.provider_name == provider_name)
    else {
        return (None, None);
    };
    decode_scalp_sheets(provider_name, set.diffuse.as_ref(), set.alpha.as_ref())
}

fn decode_scalp_sheets(
    provider_name: &str,
    diffuse: Option<&vkit_core::vam::BuiltinTextureRef>,
    alpha: Option<&vkit_core::vam::BuiltinTextureRef>,
) -> ScalpSheets {
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
    rayon::join(|| decode(diffuse, 1), || decode(alpha, 2))
}

pub(super) fn warm_builtin_scalp_images(ctx: &egui::Context, state: &AppState) {
    use rayon::prelude::*;

    let wanted: Vec<String> = state
        .hair_project
        .parts
        .iter()
        .filter(|part| part.visible)
        .map(|part| part.provider_name.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|provider| {
            ctx.data(|data| data.get_temp::<ScalpSheets>(builtin_scalp_cache_id(provider)))
                .is_none()
        })
        .collect();
    if wanted.is_empty() {
        return;
    }
    let work: Vec<(String, Option<_>, Option<_>)> = wanted
        .into_iter()
        .filter_map(|provider| {
            let set = state
                .builtin_scalp_textures
                .iter()
                .find(|set| set.provider_name == provider)?;
            Some((provider, set.diffuse.clone(), set.alpha.clone()))
        })
        .collect();
    let decoded: Vec<(String, ScalpSheets)> = work
        .into_par_iter()
        .map(|(provider, diffuse, alpha)| {
            let images = decode_scalp_sheets(&provider, diffuse.as_ref(), alpha.as_ref());
            (provider, images)
        })
        .collect();
    ctx.data_mut(|data| {
        for (provider, images) in decoded {
            data.insert_temp(builtin_scalp_cache_id(&provider), images);
        }
    });
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

pub(crate) fn posed_scalp(
    ctx: &egui::Context,
    state: &AppState,
    provider: &str,
) -> Option<Arc<crate::hair_project::ScalpAuthoring>> {
    let stock = state.hair_scalps.get(provider).map(Arc::clone)?;
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
            || ((state.hair_project.stroke_open() || state.hair_project.control_open())
                && state.hair_thumbnail.is_none()
                && now - built_at < HAIR_STROKE_REBUILD_SECONDS))
    {
        return Some(preview);
    }
    let bed_started = std::time::Instant::now();
    let bed = head_bed(ctx, state)?;
    let bed_ms = bed_started.elapsed().as_secs_f64() * 1000.0;
    let geometry_started = std::time::Instant::now();
    let geometry = crate::hair_export::preview_guide_geometry(part, scalp)?;
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
            "part={}; rev={revision}; stroke_open={}; tint={tinted}; lit={lit}; bed_ms={bed_ms:.1}; guides_ms={geometry_ms:.1}; scalp_ms={scalp_ms:.1}; preview_ms={:.1}",
            part.id,
            state.hair_project.stroke_open(),
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

use super::surface_depth::SurfaceDepth;

type CachedField = (u64, Arc<SurfaceDepth>);

pub(super) fn hair_depth_field(
    ui: &Ui,
    state: &AppState,
    rect: Rect,
    camera: TurntableCamera,
) -> Arc<SurfaceDepth> {
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

    let scale = head.as_deref().map_or(0.0, |head| {
        camera.world_units_per_point_at(head.visible_bounds.center(), rect.height())
    });
    let mut field = SurfaceDepth::new(rect, scale);
    if let Some(head) = head.as_deref() {
        let projected: Vec<Option<(Pos2, f32)>> = head
            .mesh
            .vertices
            .iter()
            .map(|vertex| {
                let world = glam::Vec3::new(vertex[0] as f32, vertex[1] as f32, vertex[2] as f32);
                camera
                    .project(world, rect)
                    .map(|seen| (seen.screen, (world - eye).length()))
            })
            .collect();
        for triangle in head.render_triangles.iter() {
            let [Some(a), Some(b), Some(c)] =
                triangle.map(|index| projected.get(index as usize).copied().flatten())
            else {
                continue;
            };
            field.fill_triangle(a, b, c);
        }
    }
    if false {
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

pub(super) fn stiffness_ribbons(state: &AppState, into: &mut Vec<crate::renderer::LineInstance>) {
    for part_id in state.hair_project.editable_parts() {
        let Some(part) = state.hair_project.part(part_id) else {
            continue;
        };
        let physics = crate::hair_export::authoring_physics(part);
        for strand in part.strands.values() {
            let points = strand.points_cm.len();
            let stiffness =
                |index: usize| -> f32 {
                    strand.rigidity.get(index).copied().unwrap_or_else(|| {
                        vkit_core::vam::rolloff_rigidity(&physics, index, points)
                    })
                };
            let along = |index: usize| index as f32 / (points.saturating_sub(1).max(1)) as f32;
            for index in 1..points {
                let (Some(from), Some(to)) =
                    (strand.points_cm.get(index - 1), strand.points_cm.get(index))
                else {
                    continue;
                };
                into.push(crate::renderer::LineInstance {
                    from_position: *from,
                    from_width: crate::hair_rigidity::half_width(along(index - 1)),
                    to_position: *to,
                    to_width: crate::hair_rigidity::half_width(along(index)),
                    from_colour: rgba(crate::hair_rigidity::ink(stiffness(index - 1))),
                    to_colour: rgba(crate::hair_rigidity::ink(stiffness(index))),
                });
            }
        }
    }
}

fn rgba(colour: egui::Color32) -> [f32; 4] {
    let [r, g, b, a] = colour.to_array();
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        f32::from(a) / 255.0,
    ]
}

fn draw_body_capsules(
    ui: &Ui,
    painter: &egui::Painter,
    rect: Rect,
    state: &AppState,
    camera: TurntableCamera,
) {
    let Some(bed) = head_bed(ui.ctx(), state) else {
        return;
    };
    let ink = crate::theme::COLOR_MUTED.gamma_multiply(0.55);
    let stroke = egui::Stroke::new(1.0, ink);
    for capsule in &bed.body_capsules {
        let a = glam::Vec3::from_array(capsule.a);
        let b = glam::Vec3::from_array(capsule.b);
        let axis = b - a;
        let Some(along) = axis.try_normalize() else {
            continue;
        };
        let seed = if along.dot(glam::Vec3::Y).abs() < 0.9 {
            glam::Vec3::Y
        } else {
            glam::Vec3::X
        };
        let across = along.cross(seed).normalize_or(glam::Vec3::X);
        let through = along.cross(across).normalize_or(glam::Vec3::Z);
        let ring = |centre: glam::Vec3| -> Vec<Pos2> {
            (0..=24)
                .filter_map(|step| {
                    let angle = std::f32::consts::TAU * step as f32 / 24.0;
                    let at =
                        centre + (across * angle.cos() + through * angle.sin()) * capsule.radius;
                    Some(camera.project(at, rect)?.screen)
                })
                .collect()
        };
        for centre in [a, b] {
            let points = ring(centre);
            if points.len() > 2 {
                painter.add(egui::Shape::line(points, stroke));
            }
        }
        for turn in 0..4 {
            let angle = std::f32::consts::TAU * turn as f32 / 4.0;
            let offset = (across * angle.cos() + through * angle.sin()) * capsule.radius;
            let (Some(from), Some(to)) = (
                camera.project(a + offset, rect),
                camera.project(b + offset, rect),
            ) else {
                continue;
            };
            painter.line_segment([from.screen, to.screen], stroke);
        }
    }
}

fn stream_point_size() -> super::marker_size::MarkerSize {
    super::marker_size::MarkerSize::new(0.13, 0.25..=3.0)
}

fn stream_point_active_size() -> super::marker_size::MarkerSize {
    super::marker_size::MarkerSize::new(0.22, 1.2..=5.0)
}

pub(super) fn hair_streams(
    state: &AppState,
    lines: &mut Vec<crate::renderer::LineInstance>,
    markers: &mut Vec<crate::renderer::MarkerInstance>,
    scale: f32,
    sizing: Option<(TurntableCamera, Rect)>,
) {
    let tinted = part_tint_active(state);
    let mut picked = Vec::new();
    for part_id in state.hair_project.editable_parts() {
        let Some(part) = state.hair_project.part(part_id) else {
            continue;
        };
        let colour = if tinted {
            part_tint_ink(part_id)
        } else {
            crate::theme::COLOR_PRIMARY
        };
        let ink = dimmed(rgba(colour), STREAM_INK);
        let picked_ink = rgba(crate::theme::COLOR_HAIR_POINT_ACTIVE);
        let picked_fill = rgba(crate::theme::COLOR_HAIR_POINT_SELECTED);
        let neighbouring_strands = state
            .hair_scalps
            .get(&part.provider_name)
            .map_or(f32::INFINITY, |scalp| scalp_pitch(scalp));
        for (strand_id, strand) in &part.strands {
            picked.clear();
            picked.extend((0..strand.points_cm.len()).map(|index| {
                state
                    .hair_vertex_selection
                    .contains(&(part_id, *strand_id, index))
            }));

            for (index, pair) in strand.points_cm.windows(2).enumerate() {
                let joined = picked[index] && picked[index + 1];
                let ink = if joined { picked_ink } else { ink };
                lines.push(crate::renderer::LineInstance {
                    from_position: pair[0],
                    from_width: STREAM_WIDTH * scale,
                    to_position: pair[1],
                    to_width: STREAM_WIDTH * scale,
                    from_colour: ink,
                    to_colour: ink,
                });
            }
            for (index, point) in strand.points_cm.iter().enumerate() {
                let editing = picked[index];
                let neighbour = if index > 0 { index - 1 } else { 1 };
                let span = strand
                    .points_cm
                    .get(neighbour)
                    .map(|other| {
                        (glam::Vec3::from_array(*other) - glam::Vec3::from_array(*point)).length()
                    })
                    .unwrap_or(1.0)
                    .min(neighbouring_strands);
                let size = if editing {
                    stream_point_active_size()
                } else {
                    stream_point_size()
                };
                let radius = match sizing {
                    Some((camera, viewport)) => {
                        size.points(camera, viewport, glam::Vec3::from_array(*point), span)
                    }
                    None => size.smallest(),
                };
                markers.push(crate::renderer::MarkerInstance {
                    position: *point,
                    radius: radius * scale,
                    shape: crate::renderer::MarkerInstance::ROUND,
                    fill: if editing {
                        picked_fill
                    } else {
                        rgba(crate::theme::COLOR_HAIR_POINT)
                    },
                    ring: if editing {
                        picked_fill
                    } else {
                        rgba(crate::theme::COLOR_HAIR_POINT)
                    },
                });
            }
        }
    }
}

const STREAM_WIDTH: f32 = 3.0;

const STREAM_INK: f32 = 0.55;

fn dimmed(colour: [f32; 4], amount: f32) -> [f32; 4] {
    [
        colour[0] * amount,
        colour[1] * amount,
        colour[2] * amount,
        colour[3],
    ]
}

fn scalp_point_size(planted: bool) -> super::marker_size::MarkerSize {
    super::marker_size::MarkerSize::new(if planted { 0.20 } else { 0.16 }, 0.3..=3.6)
}

fn scalp_pitch(scalp: &crate::hair_project::ScalpAuthoring) -> f32 {
    let points = &scalp.vertices_cm;
    if points.len() < 2 {
        return 1.0;
    }
    let step = (points.len() / 64).max(1);
    let mut total = 0.0;
    let mut counted = 0.0;
    for index in (0..points.len()).step_by(step) {
        let here = glam::Vec3::from_array(points[index]);
        let mut nearest = f32::INFINITY;
        for other in points.iter().step_by(step) {
            let gap = (glam::Vec3::from_array(*other) - here).length();
            if gap > 1.0e-4 {
                nearest = nearest.min(gap);
            }
        }
        if nearest.is_finite() {
            total += nearest;
            counted += 1.0;
        }
    }
    if counted > 0.0 { total / counted } else { 1.0 }
}
