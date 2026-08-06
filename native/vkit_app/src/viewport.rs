use std::{sync::Arc, time::Duration};

use egui::{
    Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Layout, PointerButton, Pos2,
    Rect, Response, RichText, Sense, Spinner, Stroke, Ui, UiBuilder, Vec2, pos2, vec2,
};
use vkit_core::{
    formats::Mesh as CoreMesh,
    math::Vec3 as CoreVec3,
    sculpt::ray_triangle,
    symmetry::{MirrorPinOptions, MirrorPinReason, project_mirrored_surface_pin_x},
    vam::transfer_rig_point_idw,
};

use crate::{
    camera::{CameraDelta, ProjectionMode, TurntableCamera},
    camera_control::{
        CameraGesture, ControlMode, DragSample, MiddleDragBinding, interpret_drag,
        middle_drag_gesture,
    },
    hair_renderer::{HairPaintCallback, ScalpPaintCallback},
    i18n::{Locale, TextKey, text},
    lighting::{LightingPreset, MAX_LIGHT_BRIGHTNESS, MIN_LIGHT_BRIGHTNESS, ViewportGrading},
    renderer::{
        MeshPaintCallback, RenderDepthScope, RenderStyle, SkinPaintCallback, SkinVisibilityGroup,
        SkinVisibilityGroups,
    },
    scene::{
        ActivePinDrag, MeshSide, ModelTransform, Ray3, SculptSurfaceGroup, SurfaceEndpoint,
        SurfaceMesh,
    },
    sculpt::{SculptBrush, SculptDab, SculptFalloff, SculptOperation, SculptTarget, SculptTargets},
    shortcuts::Shortcut,
    state::{
        Action, AppState, BaseViewMode, EditSourceMode, EyeGazeMode, StatusMessage, StatusTone,
        Tab, VaMCatalogStatus, ViewportBackgroundMode, ViewportToolPanel,
    },
    texture_project::{TextureSourceMode, TextureTool},
    theme::{
        CAPSULE_RADIUS, COLOR_AXIS_X, COLOR_AXIS_Y, COLOR_AXIS_Z, COLOR_BG, COLOR_BORDER,
        COLOR_DESTRUCTIVE, COLOR_MUTED, COLOR_PRIMARY, COLOR_SUCCESS, COLOR_SURFACE_RAISED,
        COLOR_TEXT, COLOR_TOPBAR, COLOR_VIEWPORT_TOOL, COLOR_WARNING, CONTROL_HEIGHT, FONT_BODY,
        FONT_HEADING, FONT_SM, FONT_XS, SMALL_RADIUS, SPACE_1, SPACE_2, SPACE_3,
    },
    ui::paint_reset_capsule_button,
    ui_components::{
        BRUSH_FALLOFF_COMPACT_WIDTH, BRUSH_FALLOFF_FIELD_HEIGHT, BRUSH_FALLOFF_FIELD_WIDTH,
        FilledNumericSlider, Icon, MINI_HELP_CONTENT_INSET_X, MINI_HELP_CONTENT_INSET_Y,
        MINI_POPUP_CONTENT_INSET_X, MINI_POPUP_CONTENT_INSET_Y, animate_rect,
        animated_segmented_group, brush_falloff_selector, brush_size_gesture_anchor,
        clear_brush_size_gesture, compact_color_picker, control_affordances, ease_in_out_cubic,
        handle_brush_size_gesture, paint_icon, paint_list_row_highlight, paint_texture_pin,
        segment_button, switch, transform_group_editability_icon,
    },
    viewport_chrome::{ChromeAnchor, ViewportChrome},
    viewport_tool_layout::{
        VIEWPORT_TOOL_SIZE, viewport_tool_button_rect, viewport_tool_panel_padding_x,
        viewport_tool_panel_padding_y, viewport_tool_panel_placement,
        viewport_tool_panel_rect_with_height, viewport_tool_rail_rect,
    },
};

#[cfg(test)]
use crate::viewport_tool_layout::VIEWPORT_TOOL_GAP;

const SCAN_SCENE_KEY: u64 = 0x5343_414e;
const TEMPLATE_SCENE_KEY: u64 = 0x4732_544d;
const ALIGNMENT_SCAN_SCENE_KEY: u64 = 0x414c_5343;
const ALIGNMENT_TEMPLATE_SCENE_KEY: u64 = 0x414c_4732;
const RESULT_SCENE_KEY: u64 = 0x5253_4c54;
const RESULT_SKIN_SCENE_KEY: u64 = 0x5253_4b4e;
const RESULT_HAIR_SCENE_KEY: u64 = 0x5248_4149;

const ALIGN_HAIR_SCENE_KEY: u64 = 0x5248_4154;
const RESULT_TEAR_LACRIMAL_SCENE_KEY: u64 = 0x5254_4541;
const RESULT_EYELASH_SCENE_KEY: u64 = 0x5245_5945;
const RESULT_SCULPT_HEAD_SCENE_KEY: u64 = 0x5343_4844;
const RESULT_SCULPT_TEAR_SCENE_KEY: u64 = 0x5343_5452;
const RESULT_SCULPT_LASH_SCENE_KEY: u64 = 0x5343_4c53;
const RESULT_SCULPT_EYES_SCENE_KEY: u64 = 0x5343_4559;
const RESULT_SCULPT_LIPS_SCENE_KEY: u64 = 0x5343_4c50;
const RESULT_SCULPT_LEFT_EYE_SCENE_KEY: u64 = 0x5343_454c;
const RESULT_SCULPT_RIGHT_EYE_SCENE_KEY: u64 = 0x5343_4552;
const RESULT_SCULPT_MOUTH_SCENE_KEY: u64 = 0x5343_4d54;
const RESULT_SCULPT_INNER_MOUTH_SCENE_KEY: u64 = 0x5343_494d;
const SCAN_OVERLAY_SCENE_KEY: u64 = 0x4f56_4c59;
const VIEWPORT_WIRE_KEY_MASK: u64 = 0x5749_5245_0000_0000;
const VIEWPORT_XRAY_KEY_MASK: u64 = 0x5852_4159_0000_0000;
const PIN_HIT_RADIUS: f32 = 13.0;

const PIN_OCCLUSION_TOLERANCE_FACTOR: f64 = 1.0e-4;

const PIN_OCCLUSION_GRAZE_ALLOWANCE: f64 = 0.09;

const PIN_BACKFACE_CUTOFF: f64 = -0.15;
const GIZMO_HIT_RADIUS: f32 = 8.0;
const GIZMO_SCALE_HANDLE_SIZE: f32 = 10.0;
const GIZMO_ARROW_LENGTH: f32 = 11.0;
const GIZMO_ARROW_HALF_WIDTH: f32 = 4.8;
const DETAIL_HUD_HEIGHT: f32 = 34.0;

const DETAIL_HEADER_MARGIN: f32 = 8.0;

const DETAIL_NUMERIC_CONTROL_WIDTH: f32 = 270.0;

const DETAIL_HUD_FULL_NUMERIC_WIDTH: f32 = 250.0;
const DETAIL_HUD_CONTENT_WIDTH: f32 = 966.0;
const DETAIL_HUD_OUTER_WIDTH: f32 = DETAIL_HUD_CONTENT_WIDTH + DETAIL_HUD_INSET_X * 2.0;
const DETAIL_HUD_INSET_X: f32 = 9.0;
const DETAIL_HUD_INSET_Y: f32 = 3.0;
const DETAIL_HUD_TOGGLE_SIZE: f32 = 28.0;
const DETAIL_HUD_EYE_WIDTH: f32 = 52.0;

const DETAIL_HUD_COMPACT_FALLOFF_WIDTH: f32 = BRUSH_FALLOFF_COMPACT_WIDTH;

const DETAIL_HUD_COMPACT_BRUSH_WIDTH: f32 = 48.0;

const DETAIL_HUD_COMPACT_CLUSTER_WIDTH: f32 = DETAIL_HUD_COMPACT_BRUSH_WIDTH
    + DETAIL_HUD_COMPACT_FALLOFF_WIDTH
    + DETAIL_HUD_TOGGLE_SIZE * 3.0
    + DETAIL_HUD_EYE_WIDTH;

const DETAIL_HUD_COMPACT_MIN_WIDTH: f32 = 520.0;

const DETAIL_HUD_COMPACT_MAX_OUTER_WIDTH: f32 = DETAIL_NUMERIC_CONTROL_WIDTH * 2.0
    + DETAIL_HUD_COMPACT_CLUSTER_WIDTH
    + crate::theme::SPACE_2 * 7.0
    + DETAIL_HUD_INSET_X * 2.0;
const DETAIL_HUD_TWO_ROW_ROW_HEIGHT: f32 = 30.0;
const DETAIL_HUD_TWO_ROW_HEIGHT: f32 =
    DETAIL_HUD_INSET_Y * 2.0 + DETAIL_HUD_TWO_ROW_ROW_HEIGHT * 2.0 + crate::theme::SPACE_2;

const DETAIL_HUD_MIN_NUMERIC_WIDTH: f32 = 96.0;

const DETAIL_HUD_TWO_ROW_MIN_WIDTH: f32 =
    DETAIL_HUD_COMPACT_CLUSTER_WIDTH + crate::theme::SPACE_2 * 5.0 + DETAIL_HUD_INSET_X * 2.0;

const DETAIL_HUD_MIN_WIDTH: f32 =
    DETAIL_HUD_MIN_NUMERIC_WIDTH * 2.0 + crate::theme::SPACE_2 + DETAIL_HUD_INSET_X * 2.0;
const DETAIL_HUD_THREE_ROW_HEIGHT: f32 =
    DETAIL_HUD_INSET_Y * 2.0 + DETAIL_HUD_TWO_ROW_ROW_HEIGHT * 3.0 + crate::theme::SPACE_2 * 2.0;
const DETAIL_GROUP_PANEL_WIDTH: f32 = 214.0;
const DETAIL_GROUP_COLLAPSED_WIDTH: f32 = 80.0;
const DETAIL_GROUP_INSET_X: f32 = 9.0;
const DETAIL_GROUP_INSET_Y: f32 = 10.0;

const DETAIL_GROUP_INSET_Y_TOP: f32 = 6.0;
const DETAIL_GROUP_ITEM_GAP: f32 = 4.0;

const SCULPT_BRUSH_FIELD_WIDTH: f32 = 104.0;
const GIZMO_DRAG_ID: &str = "vkit.viewport.alignment.gizmo.drag";
const SCULPT_DRAG_ID: &str = "vkit.viewport.sculpt.stroke";
const SCULPT_BRUSH_SIZE_ID: &str = "vkit.viewport.sculpt.brush_size";
const TEXTURE_TARGET_BRUSH_SIZE_ID: &str = "vkit.viewport.texture.brush_size";
const SCULPT_BRUSH_SIZE_SENSITIVITY: f32 = 0.75;
const TEXTURE_BRUSH_SIZE_SENSITIVITY: f32 = 0.0008;
const SCULPT_DAB_SPACING_RADIUS_FRACTION: f32 = 0.2;
const MAX_SCULPT_INTERPOLATION_STEPS: usize = 4096;
const SCULPT_SMOOTH_DABS_PER_SECOND: f64 = 30.0;
const SCULPT_SMOOTH_DAB_INTERVAL_SECONDS: f64 = 1.0 / SCULPT_SMOOTH_DABS_PER_SECOND;
const MAX_SCULPT_FRAME_DELTA_SECONDS: f64 = 0.1;
const MAX_SCULPT_SMOOTH_DABS_PER_FRAME: usize = 8;
const IMPORT_FOCUS_RELEASE_DURATION: Duration = Duration::from_millis(460);
const EYE_GAZE_GRID_SIZE: f32 = 124.0;
const EYE_GAZE_POPUP_ID: &str = "vkit.viewport.eye-gaze-popup";
const PIN_HELP_ROWS: [(TextKey, TextKey); 14] = [
    (TextKey::HelpPlace, TextKey::ShortcutPlace),
    (TextKey::HelpMove, TextKey::ShortcutMove),
    (TextKey::HelpDelete, TextKey::ShortcutDelete),
    (TextKey::HelpXSymmetry, TextKey::ShortcutXSymmetry),
    (TextKey::HelpOrbit, TextKey::ShortcutOrbit),
    (TextKey::HelpTrackball, TextKey::ShortcutTrackball),
    (TextKey::HelpPan, TextKey::ShortcutPan),
    (TextKey::HelpZoom, TextKey::ShortcutZoom),
    (TextKey::HelpDragZoom, TextKey::ShortcutDragZoom),
    (TextKey::HelpFrameView, TextKey::ShortcutFrameView),
    (TextKey::HelpSnapView, TextKey::ShortcutSnapView),
    (TextKey::HelpStandardViews, TextKey::ShortcutStandardViews),
    (TextKey::HelpLight, TextKey::ShortcutLight),
    (TextKey::HelpUndo, TextKey::ShortcutUndo),
];
const ALIGN_HELP_ROWS: [(TextKey, TextKey); 10] = [
    (TextKey::HelpSnapRotation, TextKey::ShortcutSnapRotation),
    (TextKey::HelpOrbit, TextKey::ShortcutOrbit),
    (TextKey::HelpPan, TextKey::ShortcutPan),
    (TextKey::HelpZoom, TextKey::ShortcutZoom),
    (TextKey::HelpDragZoom, TextKey::ShortcutDragZoom),
    (TextKey::HelpFrameView, TextKey::ShortcutFrameView),
    (TextKey::HelpSnapView, TextKey::ShortcutSnapView),
    (TextKey::HelpStandardViews, TextKey::ShortcutStandardViews),
    (TextKey::HelpLight, TextKey::ShortcutLight),
    (TextKey::HelpUndo, TextKey::ShortcutUndo),
];
const DETAIL_HELP_ROWS: [(TextKey, TextKey); 12] = [
    (TextKey::SculptGrab, TextKey::ShortcutSculptGrab),
    (TextKey::SculptSmooth, TextKey::ShortcutSculptSmooth),
    (TextKey::SculptPushPull, TextKey::ShortcutSculptPushPull),
    (TextKey::SculptXSymmetry, TextKey::ShortcutXSymmetry),
    (TextKey::HelpOrbit, TextKey::ShortcutOrbit),
    (TextKey::HelpPan, TextKey::ShortcutPan),
    (TextKey::HelpZoom, TextKey::ShortcutZoom),
    (TextKey::HelpDragZoom, TextKey::ShortcutDragZoom),
    (TextKey::HelpSnapView, TextKey::ShortcutSnapView),
    (TextKey::HelpStandardViews, TextKey::ShortcutStandardViews),
    (TextKey::HelpLight, TextKey::ShortcutLight),
    (TextKey::HelpUndo, TextKey::ShortcutUndo),
];
const PROJECTION_HELP_ROWS: [(TextKey, TextKey); 6] = [
    (
        TextKey::HelpProjectionPaint,
        TextKey::ShortcutProjectionPaint,
    ),
    (TextKey::HelpProjectionMove, TextKey::ShortcutProjectionMove),
    (
        TextKey::HelpProjectionRotate,
        TextKey::ShortcutProjectionRotate,
    ),
    (TextKey::HelpProjectionZoom, TextKey::ShortcutProjectionZoom),
    (
        TextKey::HelpProjectionBrushSize,
        TextKey::ShortcutProjectionBrushSize,
    ),
    (TextKey::HelpProjectionDone, TextKey::ShortcutProjectionDone),
];
const SAVE_HELP_ROWS: [(TextKey, TextKey); 8] = [
    (TextKey::HelpOrbit, TextKey::ShortcutOrbit),
    (TextKey::HelpPan, TextKey::ShortcutPan),
    (TextKey::HelpZoom, TextKey::ShortcutZoom),
    (TextKey::HelpDragZoom, TextKey::ShortcutDragZoom),
    (TextKey::HelpFrameView, TextKey::ShortcutFrameView),
    (TextKey::HelpSnapView, TextKey::ShortcutSnapView),
    (TextKey::HelpStandardViews, TextKey::ShortcutStandardViews),
    (TextKey::HelpLight, TextKey::ShortcutLight),
];

const BACKGROUND_PANEL: ViewportToolPanel = ViewportToolPanel::BaseView;
const BACKGROUND_MODES: [ViewportBackgroundMode; 3] = [
    ViewportBackgroundMode::Radial,
    ViewportBackgroundMode::Vertical,
    ViewportBackgroundMode::Flat,
];

#[derive(Clone, Copy)]
struct PinMarker {
    pair_index: usize,
    screen: egui::Pos2,
    mismatch: bool,
}

#[derive(Clone, Debug)]
enum AlignmentGizmoDrag {
    Move {
        axis: usize,
        start_pointer: Pos2,
        screen_direction: Vec2,
        world_units_per_point: f64,
        start_value: f64,
    },
    Rotate {
        axis: usize,
        ring: Vec<Pos2>,
        last_ring_parameter: f32,
        accumulated_degrees: f64,
        start_rotation_degrees: [f64; 3],
    },
    Scale {
        start_pointer: Pos2,
        start_scale_xyz: [f64; 3],
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AlignmentGizmoHit {
    Move(usize),
    Rotate(usize),
    Scale,
}

struct AlignmentGizmoGeometry {
    origin: Pos2,
    axis_ends: [Option<Pos2>; 3],
    axis_world_units_per_point: [Option<f64>; 3],
    rings: [Vec<Pos2>; 3],
    scale_handle: Pos2,
    world_center: glam::Vec3,
}

#[derive(Clone, Copy, Debug)]
struct SculptViewportStroke {
    center_local: [f64; 3],
    last_pointer: Pos2,
    last_sample_pointer: Pos2,
    distance_since_last_sample: f32,
    smooth_time_accumulator_seconds: f64,
    input_mode: SculptInputMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SculptInputMode {
    Grab,
    Smooth,
    Inflate,
    Restore,

    RestoreFit,
}

impl SculptInputMode {
    const fn is_paint_style(self) -> bool {
        matches!(self, Self::Smooth | Self::Restore | Self::RestoreFit)
    }
}

#[derive(Clone, Copy)]
struct SculptDabViewport {
    rect: Rect,
    camera: TurntableCamera,
    visible_targets: SculptTargets,
}

fn custom_head_solid_color(state: &AppState) -> Color32 {
    Color32::from_rgb(
        state.custom_head_solid_color_rgb[0],
        state.custom_head_solid_color_rgb[1],
        state.custom_head_solid_color_rgb[2],
    )
}

fn g2_solid_color(state: &AppState) -> Color32 {
    Color32::from_rgb(
        state.g2_solid_color_rgb[0],
        state.g2_solid_color_rgb[1],
        state.g2_solid_color_rgb[2],
    )
}

fn wireframe_color(state: &AppState) -> Color32 {
    Color32::from_rgb(
        state.wireframe_color_rgb[0],
        state.wireframe_color_rgb[1],
        state.wireframe_color_rgb[2],
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RadialBackgroundGeometry {
    center: Pos2,
    radius: f32,
}

fn radial_background_geometry(rect: Rect) -> RadialBackgroundGeometry {
    RadialBackgroundGeometry {
        center: rect.center(),
        radius: rect.size().length() * 0.58,
    }
}

fn paint_viewport_background(ui: &Ui, state: &AppState, rect: Rect) {
    paint_viewport_background_with_opacity(ui, state, rect, 1.0);
}

fn paint_viewport_background_with_opacity(ui: &Ui, state: &AppState, rect: Rect, opacity: f32) {
    let opacity = opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let fade = |color: Color32| color.gamma_multiply(opacity);
    let mut mesh = egui::Mesh::default();

    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
    match state.viewport_background_mode {
        ViewportBackgroundMode::Flat => {
            painter.rect_filled(rect, 0.0, fade(crate::theme::COLOR_VIEWPORT_BG));
        }
        ViewportBackgroundMode::Vertical => {
            let top = fade(crate::theme::COLOR_VIEWPORT_BG_TOP);
            let bottom = fade(crate::theme::COLOR_VIEWPORT_BG_BOTTOM);
            for (position, color) in [
                (rect.left_top(), top),
                (rect.right_top(), top),
                (rect.left_bottom(), bottom),
                (rect.right_bottom(), bottom),
            ] {
                mesh.colored_vertex(position, color);
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(2, 1, 3);
            painter.add(egui::Shape::mesh(mesh));
        }
        ViewportBackgroundMode::Radial => {
            let segments = 40_u32;
            let geometry = radial_background_geometry(rect);
            mesh.colored_vertex(
                geometry.center,
                fade(crate::theme::COLOR_VIEWPORT_BG_CENTER),
            );
            for index in 0..segments {
                let angle = std::f32::consts::TAU * index as f32 / segments as f32;
                mesh.colored_vertex(
                    geometry.center + vec2(angle.cos(), angle.sin()) * geometry.radius,
                    fade(crate::theme::COLOR_VIEWPORT_BG_EDGE),
                );
            }
            for index in 0..segments {
                mesh.add_triangle(0, index + 1, (index + 1) % segments + 1);
            }
            painter.add(egui::Shape::mesh(mesh));
        }
    }
}

const TEMPLATE_FADE_IN_SECS: f32 = 0.20;

const TEMPLATE_FADE_OUT_SECS: f32 = 0.40;

const TEMPLATE_FADE_MAX_FRAME_SECS: f32 = 0.1;

fn template_fade_step(opacity: f32, target: f32, dt: f32) -> f32 {
    let rate = if target > opacity {
        1.0 / TEMPLATE_FADE_IN_SECS
    } else {
        1.0 / TEMPLATE_FADE_OUT_SECS
    };
    let step = dt.max(0.0) * rate;
    if target > opacity {
        (opacity + step).min(target)
    } else {
        (opacity - step).max(target)
    }
}

#[derive(Clone, Copy)]
struct TemplateFadeState {
    opacity: f32,
    updated_at: f64,
    seen_generation: u64,
}

fn draw_template_install_fade(ui: &Ui, state: &AppState, rect: Rect) {
    let id = Id::new("vkit.viewport.template-install-fade");
    let now = ui.input(|input| input.time);
    let generation = state.template_install_generation;
    let load_active = state.template_load_active();
    let target = if load_active { 1.0 } else { 0.0 };
    let mut fade = ui
        .data(|data| data.get_temp::<TemplateFadeState>(id))
        .unwrap_or(TemplateFadeState {
            opacity: 0.0,
            updated_at: now,
            seen_generation: generation,
        });
    if now > fade.updated_at {
        let dt = ((now - fade.updated_at) as f32).min(TEMPLATE_FADE_MAX_FRAME_SECS);
        fade.opacity = template_fade_step(fade.opacity, target, dt);
        fade.updated_at = now;
    }
    if fade.seen_generation != generation {
        fade.seen_generation = generation;
        fade.opacity = 1.0;
    }
    ui.data_mut(|data| data.insert_temp(id, fade));
    if (fade.opacity - target).abs() > f32::EPSILON {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
    paint_viewport_background_with_opacity(ui, state, rect, ease_in_out_cubic(fade.opacity));
}

fn focus_release_progress(elapsed: Duration) -> f32 {
    (elapsed.as_secs_f32() / IMPORT_FOCUS_RELEASE_DURATION.as_secs_f32()).clamp(0.0, 1.0)
}

fn paint_vignette(ui: &Ui, state: &AppState, rect: Rect) {
    let vignette = state.vignette;
    if !vignette.enabled || vignette.intensity <= 0.0 || !rect.is_positive() {
        return;
    }
    const RINGS: usize = 24;
    const SEGMENTS: usize = 64;
    let (center, half) = (rect.center(), rect.size() * 0.5);
    let aspect = half.x / half.y.max(f32::EPSILON);

    let reach = 2.0_f32.sqrt();
    let mut mesh = egui::Mesh::default();
    for ring in 0..=RINGS {
        let span = reach * ring as f32 / RINGS as f32;
        for segment in 0..SEGMENTS {
            let angle = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32;
            let offset = [angle.cos() * span, angle.sin() * span];
            let alpha = (vignette.darkening(offset, aspect) * 255.0)
                .round()
                .clamp(0.0, 255.0);
            mesh.colored_vertex(
                center + vec2(offset[0] * half.x, offset[1] * half.y),
                Color32::from_black_alpha(alpha as u8),
            );
        }
    }
    for ring in 0..RINGS {
        for segment in 0..SEGMENTS {
            let next = (segment + 1) % SEGMENTS;
            let inner = (ring * SEGMENTS + segment) as u32;
            let inner_next = (ring * SEGMENTS + next) as u32;
            let outer = ((ring + 1) * SEGMENTS + segment) as u32;
            let outer_next = ((ring + 1) * SEGMENTS + next) as u32;
            mesh.add_triangle(inner, outer, inner_next);
            mesh.add_triangle(inner_next, outer, outer_next);
        }
    }
    ui.painter()
        .with_clip_rect(rect)
        .add(egui::Shape::mesh(mesh));
}

fn paint_radial_focus_mask(ui: &Ui, rect: Rect, clear_radius: f32, alpha: u8) {
    if alpha == 0 {
        return;
    }
    let segments = 48_u32;
    let outer_radius = rect.size().length() * 0.62;
    let inner_radius = clear_radius.clamp(0.0, outer_radius);
    let mut mesh = egui::Mesh::default();
    for index in 0..segments {
        let angle = std::f32::consts::TAU * index as f32 / segments as f32;
        let direction = vec2(angle.cos(), angle.sin());
        mesh.colored_vertex(
            rect.center() + direction * inner_radius,
            Color32::TRANSPARENT,
        );
        mesh.colored_vertex(
            rect.center() + direction * outer_radius,
            Color32::from_black_alpha(alpha),
        );
    }
    for index in 0..segments {
        let next = (index + 1) % segments;
        let inner = index * 2;
        let outer = inner + 1;
        let next_inner = next * 2;
        let next_outer = next_inner + 1;
        mesh.add_triangle(inner, outer, next_inner);
        mesh.add_triangle(next_inner, outer, next_outer);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

const IMPORT_FOCUS_APPEAR_SECS: f32 = 0.18;

fn draw_import_focus_overlay(ui: &mut Ui, state: &AppState, rect: Rect) {
    let id = Id::new("vkit.viewport.import-focus");
    let appeared_id = id.with("appeared-at");
    if let Some(progress) = state.import_progress {
        let _blocker = ui.interact(rect, id.with("blocker"), Sense::click_and_drag());
        if state.template_load_active() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
            return;
        }

        let now = ui.input(|input| input.time);
        let appeared = ui
            .data(|data| data.get_temp::<f64>(appeared_id))
            .unwrap_or(now);
        ui.data_mut(|data| data.insert_temp(appeared_id, appeared));
        let ramp = ease_in_out_cubic(
            (((now - appeared) as f32) / IMPORT_FOCUS_APPEAR_SECS).clamp(0.0, 1.0),
        );
        ui.painter().rect_filled(
            rect,
            0.0,
            Color32::from_black_alpha((116.0 * ramp).round() as u8),
        );
        paint_radial_focus_mask(
            ui,
            rect,
            rect.width().min(rect.height()) * 0.16,
            (92.0 * ramp).round() as u8,
        );
        if let Some(note) = progress.size_note(state.locale) {
            ui.painter().text(
                rect.center() + vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                note,
                FontId::proportional(FONT_SM),
                COLOR_MUTED.gamma_multiply(ramp),
            );
        }
        ui.painter().text(
            rect.center() - vec2(0.0, 18.0),
            Align2::CENTER_CENTER,
            progress.phase.label(state.locale),
            FontId::proportional(FONT_HEADING),
            COLOR_TEXT,
        );
        let spinner = Rect::from_center_size(rect.center() + vec2(0.0, 20.0), Vec2::splat(24.0));
        ui.put(spinner, Spinner::new().size(22.0).color(COLOR_PRIMARY));
        ui.ctx().request_repaint_after(Duration::from_millis(16));
        return;
    }
    ui.data_mut(|data| data.remove::<f64>(appeared_id));

    let Some(completion) = state.scan_import_completion else {
        return;
    };
    let progress = focus_release_progress(completion.completed_at.elapsed());
    if progress >= 1.0 {
        return;
    }
    let eased = 1.0 - (1.0 - progress).powi(3);
    let outer = rect.size().length() * 0.62;
    paint_radial_focus_mask(
        ui,
        rect,
        outer * eased,
        ((1.0 - progress) * 112.0).round() as u8,
    );
    ui.ctx().request_repaint_after(Duration::from_millis(16));
}

pub fn draw_alignment(ui: &mut Ui, state: &mut AppState, rect: Rect) {
    paint_viewport_background(ui, state, rect);
    let response = ui.interact(
        rect,
        Id::new("vkit.viewport.alignment"),
        Sense::click_and_drag(),
    );
    let mut swept = state.workspace.template_camera;
    let roll_owns_pointer = handle_camera_control_shortcuts(ui, state, rect, &mut swept);
    state.workspace.template_camera = swept;
    let input_blocked = state.import_progress.is_some()
        || roll_owns_pointer
        || camera_mode_owns_pointer(state)
        || viewport_tools_pointer_blocked(ui, state, rect);
    let camera = state.workspace.template_camera;
    let gizmo_captured = !input_blocked && handle_alignment_gizmo(ui, state, rect, camera);
    if !input_blocked && !gizmo_captured {
        let template = if state.edit_source_mode == EditSourceMode::CustomMorph {
            state
                .workspace
                .result
                .clone()
                .or_else(|| state.workspace.template.clone())
        } else {
            state.workspace.template.clone()
        };
        let scan = (state.edit_source_mode == EditSourceMode::ScanHead)
            .then(|| state.workspace.scan.clone())
            .flatten();
        let scan_pose = scan_transform(state);
        let pick = move |ray: Ray3| {
            nearest_visible_world_hit(
                ray,
                &[
                    (template.as_deref(), ModelTransform::default()),
                    (scan.as_deref(), scan_pose),
                ],
            )
        };
        if let Some(moved) = viewport_camera_motion(
            ui,
            &response,
            rect,
            camera,
            state.camera_control,
            &pick,
            false,
        ) {
            state.workspace.template_camera = moved;
            state
                .workspace
                .reconcile_linked_edit_cameras(MeshSide::Template);
            ui.ctx().request_repaint();
        }
        if frame_shortcut_pressed(ui, &response)
            && let Some(bounds) = state.alignment_head_bounds()
        {
            let mut framed = state.workspace.template_camera;
            framed.frame(bounds);
            state.workspace.template_camera = framed;
            state
                .workspace
                .reconcile_linked_edit_cameras(MeshSide::Template);
            ui.ctx().request_repaint();
        }
        handle_light_interaction(ui, state, &response);
    }

    let camera = state.workspace.template_camera;

    let template = if state.edit_source_mode == EditSourceMode::CustomMorph {
        state
            .workspace
            .result
            .clone()
            .or_else(|| state.workspace.template.clone())
    } else {
        state.workspace.template.clone()
    };
    let scan = (state.edit_source_mode == EditSourceMode::ScanHead)
        .then(|| state.workspace.scan.clone())
        .flatten();
    if let Some(template) = template.as_ref() {
        let skin = (state.base_view_mode == BaseViewMode::Texture)
            .then(|| state.active_skin_preview())
            .flatten();
        if let Some(skin) = skin {
            add_skin_callback(
                ui,
                SceneView {
                    rect,
                    camera,
                    transform: ModelTransform::default(),
                },
                ALIGNMENT_TEMPLATE_SCENE_KEY,
                Arc::clone(template),
                skin,
                state,
                SkinDraw {
                    visibility: SkinVisibilityGroups::ALL,
                    show_tear_lacrimals: true,
                    show_eyelashes: true,
                    depth_scope: RenderDepthScope::ResetBeforeDraw,
                },
            );
        } else {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera,
                    transform: ModelTransform::default(),
                },
                ALIGNMENT_TEMPLATE_SCENE_KEY,
                Arc::clone(template),
                state.viewport_grading(),
                state.surface_smooth_passes,
                MeshDraw {
                    color: color_array(
                        g2_solid_color(state),
                        alignment_layer_alpha(state.alignment_g2_opacity),
                    ),
                    style: RenderStyle::Solid,
                    depth_scope: RenderDepthScope::Shared,
                },
            );
        }
    }
    if let Some(scan) = scan.as_ref() {
        add_mesh_callback(
            ui,
            SceneView {
                rect,
                camera,
                transform: scan_transform(state),
            },
            ALIGNMENT_SCAN_SCENE_KEY,
            Arc::clone(scan),
            state.viewport_grading(),
            0,
            MeshDraw {
                color: color_array(
                    custom_head_solid_color(state),
                    alignment_layer_alpha(state.alignment_opacity),
                ),
                style: RenderStyle::Solid,
                depth_scope: RenderDepthScope::ResetBeforeDraw,
            },
        );
    }

    if let Some(template) = template.as_ref() {
        add_hair_try_on(
            ui,
            rect,
            state,
            template,
            ALIGN_HAIR_SCENE_KEY,
            ResultRenderView {
                camera,
                grading: state.viewport_grading(),
                smooth_passes: state.surface_smooth_passes,
            },
        );
    }
    let wire_alpha = overlay_alpha(state.wireframe_opacity);
    if state.wireframe_visible && wire_alpha > 0.0 {
        if let Some(template) = template.as_ref() {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera,
                    transform: ModelTransform::default(),
                },
                ALIGNMENT_TEMPLATE_SCENE_KEY ^ VIEWPORT_WIRE_KEY_MASK,
                Arc::clone(template),
                state.viewport_grading(),
                state.surface_smooth_passes,
                MeshDraw {
                    color: color_array(wireframe_color(state), wire_alpha),
                    style: RenderStyle::Wire,
                    depth_scope: RenderDepthScope::Shared,
                },
            );
        }
        if let Some(scan) = scan.as_ref() {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera,
                    transform: scan_transform(state),
                },
                ALIGNMENT_SCAN_SCENE_KEY ^ VIEWPORT_WIRE_KEY_MASK,
                Arc::clone(scan),
                state.viewport_grading(),
                0,
                MeshDraw {
                    color: color_array(wireframe_color(state), wire_alpha),
                    style: RenderStyle::Wire,
                    depth_scope: RenderDepthScope::Shared,
                },
            );
        }
    }
    let xray_alpha = overlay_alpha(state.xray_opacity);
    if state.xray_visible && xray_alpha > 0.0 {
        let mut reset = true;
        for (scene_key, mesh, transform, smooth_passes, color) in [
            (
                ALIGNMENT_TEMPLATE_SCENE_KEY,
                template.as_ref(),
                ModelTransform::default(),
                state.surface_smooth_passes,
                g2_solid_color(state),
            ),
            (
                ALIGNMENT_SCAN_SCENE_KEY,
                scan.as_ref(),
                scan_transform(state),
                0,
                custom_head_solid_color(state),
            ),
        ] {
            if let Some(mesh) = mesh {
                add_mesh_callback(
                    ui,
                    SceneView {
                        rect,
                        camera,
                        transform,
                    },
                    scene_key ^ VIEWPORT_XRAY_KEY_MASK,
                    Arc::clone(mesh),
                    state.viewport_grading(),
                    smooth_passes,
                    MeshDraw {
                        color: color_array(color, xray_alpha),
                        style: RenderStyle::Xray,
                        depth_scope: if std::mem::take(&mut reset) {
                            RenderDepthScope::ResetBeforeDraw
                        } else {
                            RenderDepthScope::Shared
                        },
                    },
                );
            }
        }
    }
    if state.workspace.scan.is_none() && state.workspace.template.is_none() {
        paint_empty(ui, rect, text(state.locale, TextKey::AlignmentPending));
    }
    draw_template_install_fade(ui, state, rect);

    paint_alignment_gizmo(ui, state, rect, camera);
    paint_viewport_chrome(ui, state, rect, camera);
    draw_viewport_help(ui, state, rect, true);
    paint_vignette(ui, state, rect);
    draw_viewport_tools(ui, state, rect, "alignment");
    draw_import_focus_overlay(ui, state, rect);
}

fn handle_alignment_gizmo(
    ui: &mut Ui,
    state: &mut AppState,
    viewport: Rect,
    camera: TurntableCamera,
) -> bool {
    if state.busy() {
        let removed = ui.data_mut(|data| {
            let id = Id::new(GIZMO_DRAG_ID);
            let removed = data.get_temp::<AlignmentGizmoDrag>(id).is_some();
            data.remove::<AlignmentGizmoDrag>(id);
            removed
        });
        if removed {
            state.dispatch(Action::CancelAlignmentTransform);
        }
        return false;
    }
    let Some(scan) = state.workspace.scan.clone() else {
        let removed = ui.data_mut(|data| {
            let id = Id::new(GIZMO_DRAG_ID);
            let removed = data.get_temp::<AlignmentGizmoDrag>(id).is_some();
            data.remove::<AlignmentGizmoDrag>(id);
            removed
        });
        if removed {
            state.dispatch(Action::CancelAlignmentTransform);
        }
        return false;
    };

    let Some(geometry) = alignment_gizmo_geometry(state, &scan, viewport, camera) else {
        let removed = ui.data_mut(|data| {
            let id = Id::new(GIZMO_DRAG_ID);
            let removed = data.get_temp::<AlignmentGizmoDrag>(id).is_some();
            data.remove::<AlignmentGizmoDrag>(id);
            removed
        });
        if removed {
            state.dispatch(Action::CancelAlignmentTransform);
        }
        return false;
    };
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let primary_pressed = ui.input(|input| input.pointer.button_pressed(PointerButton::Primary));
    let primary_down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));
    let primary_released = ui.input(|input| input.pointer.button_released(PointerButton::Primary));
    let pointer_over_lighting =
        pointer.is_some_and(|pointer| viewport_tools_contains(ui, state, viewport, pointer));
    let drag_id = Id::new(GIZMO_DRAG_ID);
    let mut drag = ui.data_mut(|data| data.get_temp::<AlignmentGizmoDrag>(drag_id));

    if drag.is_none()
        && primary_pressed
        && !pointer_over_lighting
        && let Some(pointer) = pointer
        && viewport.contains(pointer)
    {
        drag = begin_alignment_gizmo_drag(pointer, state, &geometry, camera, viewport);
        if let Some(active) = drag.clone() {
            state.dispatch(Action::BeginAlignmentTransform);
            ui.data_mut(|data| data.insert_temp(drag_id, active));
        }
    }

    if drag.is_none()
        && !pointer_over_lighting
        && pointer.is_some_and(|pointer| alignment_gizmo_hit(pointer, &geometry).is_some())
    {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }

    if primary_down && let (Some(active), Some(pointer)) = (drag.as_mut(), pointer) {
        let modifiers = ui.input(|input| input.modifiers);
        apply_alignment_gizmo_drag(state, active, pointer, modifiers.shift, modifiers.ctrl);
        ui.data_mut(|data| data.insert_temp(drag_id, active.clone()));
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        ui.ctx().request_repaint();
    } else if drag.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::Grab);
    }

    if primary_released || (!primary_down && drag.is_some()) {
        ui.data_mut(|data| data.remove::<AlignmentGizmoDrag>(drag_id));
        state.dispatch(Action::CommitAlignmentTransform);
    }

    drag.is_some()
}

fn begin_alignment_gizmo_drag(
    pointer: Pos2,
    state: &AppState,
    geometry: &AlignmentGizmoGeometry,
    camera: TurntableCamera,
    viewport: Rect,
) -> Option<AlignmentGizmoDrag> {
    match alignment_gizmo_hit(pointer, geometry)? {
        AlignmentGizmoHit::Move(axis) => {
            let end = geometry.axis_ends[axis]?;
            let screen_direction = (end - geometry.origin).normalized();
            (screen_direction != Vec2::ZERO).then_some(AlignmentGizmoDrag::Move {
                axis,
                start_pointer: pointer,
                screen_direction,
                world_units_per_point: geometry.axis_world_units_per_point[axis].unwrap_or_else(
                    || {
                        f64::from(
                            camera
                                .world_units_per_point_at(geometry.world_center, viewport.height()),
                        )
                    },
                ),
                start_value: state.transform.translation_cm[axis],
            })
        }
        AlignmentGizmoHit::Rotate(axis) => {
            let ring = geometry.rings[axis].clone();
            let last_ring_parameter = polyline_parameter(pointer, &ring)?;
            Some(AlignmentGizmoDrag::Rotate {
                axis,
                ring,
                last_ring_parameter,
                accumulated_degrees: 0.0,
                start_rotation_degrees: state.transform.rotation_degrees,
            })
        }
        AlignmentGizmoHit::Scale => Some(AlignmentGizmoDrag::Scale {
            start_pointer: pointer,
            start_scale_xyz: state.transform.scale_xyz,
        }),
    }
}

fn alignment_gizmo_hit(
    pointer: Pos2,
    geometry: &AlignmentGizmoGeometry,
) -> Option<AlignmentGizmoHit> {
    if pointer.distance(geometry.scale_handle) <= GIZMO_HIT_RADIUS * 1.4 {
        return Some(AlignmentGizmoHit::Scale);
    }

    let move_hit = geometry
        .axis_ends
        .iter()
        .enumerate()
        .filter_map(|(axis, end)| {
            let end = (*end)?;
            let distance = point_segment_distance(pointer, geometry.origin, end);
            (distance <= GIZMO_HIT_RADIUS).then_some((distance, axis))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0));
    let rotate_hit = geometry
        .rings
        .iter()
        .enumerate()
        .map(|(axis, ring)| (polyline_distance(pointer, ring), axis))
        .filter(|(distance, _)| *distance <= GIZMO_HIT_RADIUS)
        .min_by(|left, right| left.0.total_cmp(&right.0));

    match (move_hit, rotate_hit) {
        (Some((move_distance, _)), Some((rotate_distance, rotate_axis)))
            if rotate_distance < move_distance =>
        {
            Some(AlignmentGizmoHit::Rotate(rotate_axis))
        }
        (Some((_, axis)), _) => Some(AlignmentGizmoHit::Move(axis)),
        (None, Some((_, axis))) => Some(AlignmentGizmoHit::Rotate(axis)),
        (None, None) => None,
    }
}

fn apply_alignment_gizmo_drag(
    state: &mut AppState,
    drag: &mut AlignmentGizmoDrag,
    pointer: Pos2,
    shift_down: bool,
    ctrl_down: bool,
) {
    match drag {
        AlignmentGizmoDrag::Move {
            axis,
            start_pointer,
            screen_direction,
            world_units_per_point,
            start_value,
        } => state.dispatch(Action::SetPosition {
            axis: *axis,
            value_cm: translated_axis_value(
                *start_value,
                pointer - *start_pointer,
                *screen_direction,
                *world_units_per_point,
            ),
        }),
        AlignmentGizmoDrag::Rotate {
            axis,
            ring,
            last_ring_parameter,
            accumulated_degrees,
            start_rotation_degrees,
        } => {
            let Some(current_parameter) = polyline_parameter(pointer, ring) else {
                return;
            };
            *accumulated_degrees += f64::from(
                wrapped_angle_delta(*last_ring_parameter, current_parameter).to_degrees(),
            );
            *last_ring_parameter = current_parameter;
            let applied_degrees =
                rotation_drag_degrees(*accumulated_degrees, shift_down, ctrl_down);
            let rotation =
                world_axis_rotated_euler(*start_rotation_degrees, *axis, applied_degrees);
            for (component, value_degrees) in rotation.into_iter().enumerate() {
                state.dispatch(Action::SetRotation {
                    axis: component,
                    value_degrees,
                });
            }
        }
        AlignmentGizmoDrag::Scale {
            start_pointer,
            start_scale_xyz,
        } => state.dispatch(Action::SetScale(uniform_scale_drag_values(
            *start_scale_xyz,
            pointer - *start_pointer,
        ))),
    }
}

fn rotation_drag_degrees(accumulated_degrees: f64, shift_down: bool, ctrl_down: bool) -> f64 {
    let step = if shift_down {
        Some(90.0)
    } else if ctrl_down {
        Some(1.0)
    } else {
        None
    };
    step.map_or(accumulated_degrees, |step| {
        (accumulated_degrees / step).round() * step
    })
}

fn paint_alignment_gizmo(ui: &Ui, state: &AppState, viewport: Rect, camera: TurntableCamera) {
    if state.busy() {
        return;
    }
    let Some(scan) = state.workspace.scan.as_deref() else {
        return;
    };
    let Some(geometry) = alignment_gizmo_geometry(state, scan, viewport, camera) else {
        return;
    };

    for (axis, ring) in geometry.rings.iter().enumerate() {
        if ring.len() >= 2 {
            ui.painter().add(egui::Shape::line(
                ring.clone(),
                Stroke::new(1.6, gizmo_axis_color(axis).gamma_multiply(0.82)),
            ));
        }
    }
    for (axis, end) in geometry.axis_ends.iter().copied().enumerate() {
        let Some(end) = end else { continue };
        let color = gizmo_axis_color(axis);
        ui.painter()
            .line_segment([geometry.origin, end], Stroke::new(2.2, color));
        if let Some(points) = gizmo_arrowhead(geometry.origin, end) {
            ui.painter().add(egui::Shape::convex_polygon(
                points.to_vec(),
                color,
                Stroke::NONE,
            ));
        }
    }
    ui.painter().rect_filled(
        Rect::from_center_size(geometry.scale_handle, Vec2::splat(GIZMO_SCALE_HANDLE_SIZE)),
        2.0,
        Color32::WHITE,
    );
}

fn gizmo_arrowhead(origin: Pos2, end: Pos2) -> Option<[Pos2; 3]> {
    let axis = end - origin;
    let length = axis.length();
    if !length.is_finite() || length <= f32::EPSILON {
        return None;
    }
    let direction = axis / length;
    let normal = vec2(-direction.y, direction.x);
    let base = end - direction * GIZMO_ARROW_LENGTH;
    Some([
        end,
        base + normal * GIZMO_ARROW_HALF_WIDTH,
        base - normal * GIZMO_ARROW_HALF_WIDTH,
    ])
}

fn alignment_gizmo_geometry(
    state: &AppState,
    scan: &SurfaceMesh,
    viewport: Rect,
    camera: TurntableCamera,
) -> Option<AlignmentGizmoGeometry> {
    let transform = scan_transform(state);
    let world_bounds = transform.bounds_to_world(scan.facial_focus_bounds());
    let world_center = world_bounds.center();
    let origin = camera.project(world_center, viewport)?.screen;
    let world_size = world_bounds
        .radius()
        .max(camera.frame_radius * 0.08)
        .clamp(camera.frame_radius * 0.08, camera.frame_radius * 0.42);
    let axes = [glam::Vec3::X, glam::Vec3::Y, glam::Vec3::Z];
    let axis_ends = axes.map(|axis| {
        camera
            .project(world_center + axis * world_size, viewport)
            .map(|point| point.screen)
            .filter(|point| point.distance(origin) >= 6.0)
    });
    let axis_world_units_per_point = axis_ends.map(|end| {
        end.and_then(|end| {
            let projected_length = end.distance(origin);
            (projected_length > f32::EPSILON).then_some(f64::from(world_size / projected_length))
        })
    });
    let ring_bases = [
        (glam::Vec3::Y, glam::Vec3::Z),
        (glam::Vec3::Z, glam::Vec3::X),
        (glam::Vec3::X, glam::Vec3::Y),
    ];
    let rings = ring_bases.map(|(first, second)| {
        (0..=48)
            .filter_map(|index| {
                let angle = std::f32::consts::TAU * index as f32 / 48.0;
                let world =
                    world_center + (first * angle.cos() + second * angle.sin()) * world_size * 1.18;
                camera.project(world, viewport).map(|point| point.screen)
            })
            .collect()
    });
    Some(AlignmentGizmoGeometry {
        origin,
        axis_ends,
        axis_world_units_per_point,
        rings,
        scale_handle: origin,
        world_center,
    })
}

fn gizmo_axis_color(axis: usize) -> Color32 {
    match axis {
        0 => COLOR_AXIS_X,
        1 => COLOR_AXIS_Y,
        _ => COLOR_AXIS_Z,
    }
}

fn point_segment_distance(point: Pos2, start: Pos2, end: Pos2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_sq();
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let fraction = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * fraction)
}

fn polyline_distance(point: Pos2, points: &[Pos2]) -> f32 {
    points
        .windows(2)
        .map(|pair| point_segment_distance(point, pair[0], pair[1]))
        .fold(f32::INFINITY, f32::min)
}

fn polyline_parameter(point: Pos2, points: &[Pos2]) -> Option<f32> {
    let segment_count = points.len().checked_sub(1)?;
    if segment_count == 0 {
        return None;
    }
    points
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| {
            let segment = pair[1] - pair[0];
            let length_squared = segment.length_sq();
            if length_squared <= f32::EPSILON {
                return None;
            }
            let fraction = ((point - pair[0]).dot(segment) / length_squared).clamp(0.0, 1.0);
            let closest = pair[0] + segment * fraction;
            let distance_squared = point.distance_sq(closest);
            let parameter =
                std::f32::consts::TAU * (index as f32 + fraction) / segment_count as f32;
            Some((distance_squared, parameter))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, parameter)| parameter)
}

fn wrapped_angle_delta(previous: f32, current: f32) -> f32 {
    (current - previous + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI
}

fn world_axis_rotated_euler(
    start_rotation_degrees: [f64; 3],
    axis: usize,
    delta_degrees: f64,
) -> [f64; 3] {
    let [x, y, z] = start_rotation_degrees.map(f64::to_radians);
    let start = glam::DQuat::from_euler(glam::EulerRot::XYZ, x, y, z);
    let world_axis = [glam::DVec3::X, glam::DVec3::Y, glam::DVec3::Z][axis.min(2)];
    let rotated = glam::DQuat::from_axis_angle(world_axis, delta_degrees.to_radians()) * start;
    let (x, y, z) = rotated.to_euler(glam::EulerRot::XYZ);
    [x.to_degrees(), y.to_degrees(), z.to_degrees()]
}

fn translated_axis_value(
    start_value: f64,
    pointer_delta: Vec2,
    screen_direction: Vec2,
    world_units_per_point: f64,
) -> f64 {
    start_value + f64::from(pointer_delta.dot(screen_direction)) * world_units_per_point.max(0.0)
}

fn uniform_scale_drag_values(start_scale_xyz: [f64; 3], pointer_delta: Vec2) -> [f64; 3] {
    let signed_points = pointer_delta.x - pointer_delta.y;
    let requested_ratio = 2.0_f64.powf(f64::from(signed_points) / 180.0);
    let minimum_ratio = start_scale_xyz
        .into_iter()
        .map(|scale| 1.0e-6 / scale)
        .fold(0.0, f64::max);
    let maximum_ratio = start_scale_xyz
        .into_iter()
        .map(|scale| 1.0e6 / scale)
        .fold(f64::INFINITY, f64::min);
    let ratio = requested_ratio.clamp(minimum_ratio, maximum_ratio);
    start_scale_xyz.map(|scale| scale * ratio)
}

const RESULT_ISLAND_ROW: f32 = 30.0;

const RESULT_ISLAND_INSET: f32 = 10.0;

const RESULT_ISLAND: crate::responsive::Responsive = crate::responsive::Responsive {
    min: 240.0,
    ideal: 360.0,
    fraction: 0.6,
};

fn draw_result_view_island(ui: &mut Ui, state: &mut AppState, viewport: Rect) {
    let overlay = state.workspace.scan.is_some();
    let compare = state.morph_compare_available();
    if !overlay && !compare {
        return;
    }
    let rows = usize::from(overlay) + usize::from(compare);
    let Some(width) = RESULT_ISLAND.resolve(viewport.width()) else {
        return;
    };
    let height = RESULT_ISLAND_INSET * 2.0
        + RESULT_ISLAND_ROW * rows as f32
        + SPACE_2 * (rows.saturating_sub(1)) as f32;
    let island = Rect::from_center_size(
        pos2(
            viewport.center().x,
            viewport.bottom() - RESULT_ISLAND_BOTTOM_GAP - height * 0.5,
        ),
        vec2(width, height),
    );
    let layer = egui::LayerId::new(
        egui::Order::Middle,
        Id::new("vkit.viewport.result-island.layer"),
    );
    let ui = &mut ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(island));
    ui.painter().rect_filled(
        island,
        f32::from(crate::theme::RADIUS_POPOVER),
        COLOR_TOPBAR,
    );
    let _blocker = ui.interact(
        island,
        Id::new("vkit.viewport.result-island"),
        Sense::click_and_drag(),
    );
    let content = island.shrink(RESULT_ISLAND_INSET);
    let mut row_top = content.top();
    let mut next_row = || {
        let row = Rect::from_min_size(
            pos2(content.left(), row_top),
            vec2(content.width(), RESULT_ISLAND_ROW),
        );
        row_top += RESULT_ISLAND_ROW + SPACE_2;
        row
    };
    if overlay {
        let row = next_row();
        let mut opacity = state.overlay_opacity;
        if crate::ui::island_labelled_slider(
            ui,
            row,
            text(state.locale, TextKey::ScanOverlay),
            &mut opacity,
        ) {
            state.dispatch(Action::SetOverlayOpacity(opacity));
        }
    }
    if compare {
        let row = next_row();
        let mut blend = state.morph_compare_blend;
        if crate::ui::island_labelled_slider(
            ui,
            row,
            text(state.locale, TextKey::MorphCompare),
            &mut blend,
        ) {
            state.dispatch(Action::SetMorphCompareBlend(blend));
        }
    }
}

const RESULT_ISLAND_BOTTOM_GAP: f32 = 16.0 + CONTROL_HEIGHT;

const PIN_PROMPT_HEIGHT: f32 = 52.0;

const PIN_PROMPT_INSET: f32 = 22.0;

const PIN_PROMPT_CHEVRON: f32 = 22.0;

const PIN_PROMPT_SWING: f32 = 7.0;

const PIN_PROMPT_SWING_PERIOD: f64 = 2.2;

fn draw_pin_prompt_island(ui: &Ui, state: &AppState, workspace: Rect, split_x: f32) {
    if crate::guidance::next_step(state) != Some(crate::guidance::NextStep::PlacePins) {
        return;
    }
    let galley = ui.painter().layout_no_wrap(
        text(state.locale, TextKey::PinPrompt).to_owned(),
        FontId::proportional(FONT_BODY),
        COLOR_TEXT,
    );
    let width = galley.size().x + (PIN_PROMPT_CHEVRON + PIN_PROMPT_INSET) * 2.0;
    if width > workspace.width() - 32.0 {
        return;
    }
    let island = Rect::from_center_size(
        pos2(split_x, workspace.center().y),
        vec2(width, PIN_PROMPT_HEIGHT),
    );

    ui.painter().rect_filled(
        island,
        PIN_PROMPT_HEIGHT * 0.5,
        COLOR_TOPBAR.gamma_multiply(0.82),
    );
    let text_pos = pos2(
        island.center().x - galley.size().x * 0.5,
        island.center().y - galley.size().y * 0.5,
    );
    ui.painter().galley(text_pos, galley, COLOR_TEXT);

    let time = ui.input(|input| input.time);
    let phase = (time / PIN_PROMPT_SWING_PERIOD).rem_euclid(1.0) as f32;
    let swing = (phase * std::f32::consts::TAU).sin() * PIN_PROMPT_SWING;
    for (icon, side) in [(Icon::ChevronLeft, -1.0_f32), (Icon::ChevronRight, 1.0)] {
        let centre = pos2(
            island.center().x
                + side * (island.width() * 0.5 - PIN_PROMPT_INSET * 0.5 - PIN_PROMPT_CHEVRON * 0.5)
                + side * swing,
            island.center().y,
        );
        paint_icon(
            ui.painter(),
            Rect::from_center_size(centre, Vec2::splat(PIN_PROMPT_CHEVRON)),
            icon,
            COLOR_TEXT,
        );
    }
    ui.ctx().request_repaint();
}

const PIN_ISLAND_HEIGHT: f32 = 40.0;

const PIN_ISLAND_BOTTOM_GAP: f32 = 16.0;

const PIN_ISLAND_INSET: f32 = 8.0;

fn draw_pin_island(ui: &mut Ui, state: &mut AppState, workspace: Rect, split_x: f32) {
    if state.scan_path.is_none() || state.busy() {
        return;
    }
    let undo_label = text(state.locale, TextKey::Undo);
    let reset_label = text(state.locale, TextKey::ResetAllPins);
    let measure = |label: &str| {
        ui.painter()
            .layout_no_wrap(
                label.to_owned(),
                FontId::proportional(crate::theme::FONT_SM),
                COLOR_TEXT,
            )
            .size()
            .x
    };
    let undo_width = measure(undo_label) + 28.0;
    let reset_width = measure(reset_label) + 28.0;
    let width = undo_width + reset_width + PIN_ISLAND_INSET * 3.0;
    if width > workspace.width() - 32.0 {
        return;
    }
    let island = Rect::from_center_size(
        pos2(
            split_x,
            workspace.bottom() - PIN_ISLAND_BOTTOM_GAP - PIN_ISLAND_HEIGHT * 0.5,
        ),
        vec2(width, PIN_ISLAND_HEIGHT),
    );

    let layer = egui::LayerId::new(
        egui::Order::Middle,
        Id::new("vkit.viewport.pin-island.layer"),
    );
    let ui = &mut ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(island));
    ui.painter().rect_filled(
        island,
        f32::from(crate::theme::RADIUS_POPOVER),
        COLOR_TOPBAR,
    );

    let _blocker = ui.interact(
        island,
        Id::new("vkit.viewport.pin-island"),
        Sense::click_and_drag(),
    );
    let content = island.shrink2(vec2(PIN_ISLAND_INSET, 6.0));
    let undo_rect = Rect::from_min_size(content.min, vec2(undo_width, content.height()));
    let reset_rect = Rect::from_min_size(
        pos2(undo_rect.right() + PIN_ISLAND_INSET, content.top()),
        vec2(reset_width, content.height()),
    );
    if crate::ui::island_capsule_button(ui, undo_rect, undo_label, false).clicked() {
        state.dispatch(Action::Undo);
    }

    if crate::ui::island_capsule_button(ui, reset_rect, reset_label, true).clicked() {
        state.dispatch(Action::ResetPins);
    }
}

fn paint_pin_count(ui: &Ui, state: &AppState, pane: Rect) {
    if state.scan_path.is_none() {
        return;
    }
    let label = format!(
        "{} {}",
        text(state.locale, TextKey::PinPairsPlaced),
        state.complete_pairs
    );
    ui.painter().text(
        pos2(pane.center().x, pane.top() + 16.0),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(FONT_XS),
        COLOR_MUTED,
    );
}

pub fn draw_edit(ui: &mut Ui, state: &mut AppState, scan_rect: Rect, template_rect: Rect) {
    let workspace_rect = scan_rect.union(template_rect);

    let side = if ui
        .input(|input| input.pointer.interact_pos())
        .is_some_and(|pointer| template_rect.contains(pointer))
    {
        MeshSide::Template
    } else {
        MeshSide::Scan
    };
    let mut swept = match side {
        MeshSide::Scan => state.workspace.scan_camera,
        MeshSide::Template => state.workspace.template_camera,
    };
    let roll_owns_pointer = handle_camera_control_shortcuts(ui, state, workspace_rect, &mut swept);
    match side {
        MeshSide::Scan => state.workspace.scan_camera = swept,
        MeshSide::Template => state.workspace.template_camera = swept,
    }
    draw_edit_side(
        ui,
        state,
        MeshSide::Scan,
        scan_rect,
        workspace_rect,
        roll_owns_pointer,
    );
    draw_edit_side(
        ui,
        state,
        MeshSide::Template,
        template_rect,
        workspace_rect,
        roll_owns_pointer,
    );

    paint_pin_count(ui, state, template_rect);
    draw_pin_prompt_island(
        ui,
        state,
        workspace_rect,
        (scan_rect.right() + template_rect.left()) * 0.5,
    );
    draw_pin_island(
        ui,
        state,
        workspace_rect,
        (scan_rect.right() + template_rect.left()) * 0.5,
    );
    draw_template_install_fade(ui, state, workspace_rect);

    ui.painter().line_segment(
        [
            pos2(scan_rect.right(), scan_rect.top()),
            pos2(scan_rect.right(), scan_rect.bottom()),
        ],
        Stroke::new(1.0, COLOR_BORDER),
    );

    draw_viewport_help(ui, state, workspace_rect, true);
    paint_vignette(ui, state, workspace_rect);
    draw_viewport_tools(ui, state, workspace_rect, "edit");
    draw_import_focus_overlay(ui, state, workspace_rect);
}

fn apply_result_display_preview(state: &mut AppState, eye_gaze: [f32; 2]) {
    if state.is_detail_editing()
        && state.sculpt_eye_tracking
        && let Some(preview) = sculpt_eye_look_preview(state, eye_gaze)
    {
        let signature = [preview.left_pitch, preview.right_pitch];
        if state.workspace.result_display_preview_matches(signature) {
            return;
        }
        if let Some(vertices) =
            state.sculpt_eye_follow_vertices(preview.left_pitch, preview.right_pitch)
        {
            let _ = state
                .workspace
                .set_result_display_preview_vertices(vertices, signature);
            return;
        }
    }
    if let Some(signature) = state.morph_compare_signature() {
        if state.workspace.result_display_preview_matches(signature) {
            return;
        }
        if let Some(vertices) = state.morph_compare_vertices() {
            let _ = state
                .workspace
                .set_result_display_preview_vertices(vertices, signature);
            return;
        }
    }
    let _ = state.workspace.clear_result_display_preview();
}

pub fn draw_result(ui: &mut Ui, state: &mut AppState, rect: Rect, _title: &str) {
    paint_viewport_background(ui, state, rect);
    let response = ui.interact(
        rect,
        Id::new("vkit.viewport.result"),
        Sense::click_and_drag(),
    );
    let mut swept = state.workspace.result_camera;
    let roll_owns_pointer = handle_camera_control_shortcuts(ui, state, rect, &mut swept);
    state.workspace.result_camera = swept;
    let input_blocked = state.import_progress.is_some()
        || roll_owns_pointer
        || camera_mode_owns_pointer(state)
        || viewport_tools_pointer_blocked(ui, state, rect);
    if !input_blocked {
        let result = state.workspace.result.clone();
        let scan_overlay =
            result_scan_overlay_alpha(state).and_then(|_| state.workspace.scan.clone());
        let scan_pose = scan_transform(state);
        let pick = move |ray: Ray3| {
            nearest_visible_world_hit(
                ray,
                &[
                    (result.as_deref(), ModelTransform::default()),
                    (scan_overlay.as_deref(), scan_pose),
                ],
            )
        };
        if let Some(moved) = viewport_camera_motion(
            ui,
            &response,
            rect,
            state.workspace.result_camera,
            state.camera_control,
            &pick,
            projection_stencil_mode(state),
        ) {
            state.workspace.result_camera = moved;
            ui.ctx().request_repaint();
        }

        if !state.is_detail_editing()
            && frame_shortcut_pressed(ui, &response)
            && let Some(bounds) = state.result_head_bounds()
        {
            let mut framed = state.workspace.result_camera;
            framed.frame(bounds);
            state.workspace.result_camera = framed;
            ui.ctx().request_repaint();
        }

        if !projection_stencil_mode(state) {
            handle_light_interaction(ui, state, &response);
        }
    }

    let camera = state.workspace.result_camera;
    let eye_gaze = viewport_eye_gaze(ui, state, rect);
    apply_result_display_preview(state, eye_gaze);
    let render_view = ResultRenderView {
        camera,
        grading: state.viewport_grading(),
        smooth_passes: state.surface_smooth_passes,
    };

    if !texture_paint_mode(state) && !projection_stencil_mode(state) {
        clear_brush_size_gesture(ui.ctx(), Id::new(TEXTURE_TARGET_BRUSH_SIZE_ID));
    }
    clear_stale_detail_pointer_state(ui, state);

    if projection_stencil_mode(state) {
        handle_projection_stencil(ui, state, rect, &response, camera, input_blocked);
    } else if texture_target_pin_mode(state) {
        handle_texture_target_pin_interaction(ui, state, rect, &response, camera, input_blocked);
    } else if texture_paint_mode(state) {
        handle_texture_paint_interaction(ui, state, rect, &response, camera, input_blocked);
    } else if state.is_sculpting() {
        handle_sculpt_interaction(ui, state, rect, &response, camera, input_blocked);
    }

    let drawn = state.workspace.result.clone();
    if let Some(mesh) = drawn {
        if state.is_detail_editing() {
            add_sculpt_result_callbacks(ui, rect, state, render_view, eye_gaze, Arc::clone(&mesh));
        } else {
            add_result_composed_callbacks(ui, rect, state, render_view, Arc::clone(&mesh));
        }

        if let Some(overlay_alpha) = result_scan_overlay_alpha(state)
            && let Some(scan) = state.workspace.scan.clone()
        {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera,
                    transform: scan_transform(state),
                },
                SCAN_OVERLAY_SCENE_KEY,
                scan,
                state.viewport_grading(),
                0,
                MeshDraw {
                    color: color_array(custom_head_solid_color(state), overlay_alpha),
                    style: if overlay_alpha >= 1.0 {
                        RenderStyle::Solid
                    } else {
                        RenderStyle::Xray
                    },
                    depth_scope: if overlay_alpha >= 1.0 {
                        RenderDepthScope::ResetBeforeDraw
                    } else {
                        RenderDepthScope::Shared
                    },
                },
            );
        }
    } else {
        crate::ui_components::log_once(
            ui.ctx(),
            Id::new("vkit.viewport.result-empty"),
            &format!("{:?}", state.active_tab),
            || {
                let _ = crate::diagnostics::record(
                    crate::diagnostics::Severity::Debug,
                    "viewport",
                    "result_empty_drawn",
                    &format!("tab={:?}", state.active_tab),
                );
            },
        );
        paint_empty(ui, rect, text(state.locale, TextKey::ResultEmpty));
    }
    draw_template_install_fade(ui, state, rect);

    if state.is_sculpting() {
        paint_sculpt_brush_hud(ui, state, &response);
    }

    if projection_stencil_mode(state) {
        if let Some(stencil) = crate::texture_ui::projection_stencil_rect(state, rect) {
            crate::texture_ui::paint_projection_stencil(ui, state, stencil);
            paint_stencil_brush_cursor(ui, state, &response, stencil);
            detail_panels::publish_stencil_projection(ui, state, rect, camera, stencil);
        }

        draw_projection_stencil_island(ui, state, rect);
    } else if texture_target_pin_mode(state) {
        paint_texture_target_pins(ui, state, rect, camera);
    }
    if texture_paint_mode(state) {
        paint_texture_brush_cursor(ui, state, &response, rect);
    }
    paint_viewport_chrome(ui, state, rect, camera);
    if state.active_tab == Tab::Result {
        draw_result_view_island(ui, state, rect);
    }
    draw_viewport_help(ui, state, rect, true);
    paint_vignette(ui, state, rect);

    if state.is_detail_editing() {
        draw_detail_viewport_controls(ui, state, rect);
    }
    draw_viewport_tools(ui, state, rect, "result");
    draw_import_focus_overlay(ui, state, rect);
}

fn draw_edit_side(
    ui: &mut Ui,
    state: &mut AppState,
    side: MeshSide,
    rect: Rect,
    workspace_rect: Rect,
    roll_owns_pointer: bool,
) {
    paint_viewport_background(ui, state, rect);
    let response = ui.interact(
        rect,
        Id::new(match side {
            MeshSide::Scan => "vkit.viewport.scan",
            MeshSide::Template => "vkit.viewport.template",
        }),
        Sense::click_and_drag(),
    );

    let input_blocked = state.import_progress.is_some()
        || roll_owns_pointer
        || camera_mode_owns_pointer(state)
        || viewport_tools_pointer_blocked(ui, state, workspace_rect);
    if !input_blocked {
        let side_mesh = match side {
            MeshSide::Scan => state.workspace.scan.clone(),
            MeshSide::Template => state.workspace.template.clone(),
        };
        let side_pose = match side {
            MeshSide::Scan => scan_transform(state),
            MeshSide::Template => ModelTransform::default(),
        };
        let side_camera = match side {
            MeshSide::Scan => state.workspace.scan_camera,
            MeshSide::Template => state.workspace.template_camera,
        };
        let pick =
            move |ray: Ray3| nearest_visible_world_hit(ray, &[(side_mesh.as_deref(), side_pose)]);
        if let Some(moved) = viewport_camera_motion(
            ui,
            &response,
            rect,
            side_camera,
            state.camera_control,
            &pick,
            false,
        ) {
            set_edit_camera(state, side, moved);
            ui.ctx().request_repaint();
        }
        if frame_shortcut_pressed(ui, &response)
            && let Some(bounds) = state.edit_side_head_bounds(side)
        {
            let mut framed = match side {
                MeshSide::Scan => state.workspace.scan_camera,
                MeshSide::Template => state.workspace.template_camera,
            };
            framed.frame(bounds);
            set_edit_camera(state, side, framed);
            ui.ctx().request_repaint();
        }
        handle_light_interaction(ui, state, &response);
    }

    let mesh = match side {
        MeshSide::Scan => state.workspace.scan.clone(),
        MeshSide::Template => state.workspace.template.clone(),
    };
    let camera = match side {
        MeshSide::Scan => state.workspace.scan_camera,
        MeshSide::Template => state.workspace.template_camera,
    };
    let transform = match side {
        MeshSide::Scan => scan_transform(state),
        MeshSide::Template => ModelTransform::default(),
    };

    if let Some(mesh) = mesh.clone() {
        if !input_blocked {
            handle_pin_interaction(
                ui,
                state,
                side,
                SceneView {
                    rect,
                    camera,
                    transform,
                },
                &response,
                &mesh,
            );
        }
        let color = match side {
            MeshSide::Scan => custom_head_solid_color(state),
            MeshSide::Template => g2_solid_color(state),
        };
        let scene_key = match side {
            MeshSide::Scan => SCAN_SCENE_KEY,
            MeshSide::Template => TEMPLATE_SCENE_KEY,
        };
        let smooth_passes = match side {
            MeshSide::Scan => 0,
            MeshSide::Template => state.surface_smooth_passes,
        };
        let skin = (side == MeshSide::Template && state.base_view_mode == BaseViewMode::Texture)
            .then(|| state.active_skin_preview())
            .flatten();
        if let Some(skin) = skin {
            add_skin_callback(
                ui,
                SceneView {
                    rect,
                    camera,
                    transform,
                },
                scene_key,
                Arc::clone(&mesh),
                skin,
                state,
                SkinDraw {
                    visibility: SkinVisibilityGroups::ALL,
                    show_tear_lacrimals: false,
                    show_eyelashes: false,
                    depth_scope: RenderDepthScope::ResetBeforeDraw,
                },
            );
        } else {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera,
                    transform,
                },
                scene_key,
                Arc::clone(&mesh),
                state.viewport_grading(),
                smooth_passes,
                MeshDraw {
                    color: color_array(color, 1.0),
                    style: RenderStyle::Solid,
                    depth_scope: RenderDepthScope::Shared,
                },
            );
        }
        let wire_alpha = overlay_alpha(state.wireframe_opacity);
        if state.wireframe_visible && wire_alpha > 0.0 {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera,
                    transform,
                },
                scene_key ^ VIEWPORT_WIRE_KEY_MASK,
                Arc::clone(&mesh),
                state.viewport_grading(),
                smooth_passes,
                MeshDraw {
                    color: color_array(wireframe_color(state), wire_alpha),
                    style: RenderStyle::Wire,
                    depth_scope: RenderDepthScope::Shared,
                },
            );
        }
        let xray_alpha = overlay_alpha(state.xray_opacity);
        if state.xray_visible && xray_alpha > 0.0 {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera,
                    transform,
                },
                scene_key ^ VIEWPORT_XRAY_KEY_MASK,
                mesh.clone(),
                state.viewport_grading(),
                smooth_passes,
                MeshDraw {
                    color: color_array(color, xray_alpha),
                    style: RenderStyle::Xray,
                    depth_scope: RenderDepthScope::ResetBeforeDraw,
                },
            );
        }
        paint_pin_markers(ui, state, side, rect, &mesh, camera, transform);
    } else {
        match side {
            MeshSide::Scan => draw_scan_drop_target(ui, state, rect),
            MeshSide::Template => {
                paint_empty(ui, rect, text(state.locale, TextKey::TemplatePending));
            }
        }
    }

    paint_viewport_chrome(ui, state, rect, camera);
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ViewportCameraInput {
    delta: CameraDelta,

    wheel_points: f32,

    snap_orbit: bool,
}

impl ViewportCameraInput {
    fn has_motion(self) -> bool {
        self.delta.has_motion() || self.wheel_points != 0.0
    }

    fn apply(&mut self, gesture: CameraGesture) {
        match gesture {
            CameraGesture::Orbit(points) => self.delta.orbit_points = points,
            CameraGesture::Pan(points) => self.delta.pan_points = points,
            CameraGesture::Dolly(points) => self.delta.scroll_points = points,
            CameraGesture::Trackball { orbit, roll } => {
                self.delta.orbit_points += orbit;
                self.delta.roll_radians += roll;
            }
        }
    }
}

const ROLL_SWEEP_ID: &str = "vkit.viewport.roll-sweep";

const ROLL_RADIANS_PER_POINT: f32 = 0.004;

fn handle_camera_control_shortcuts(
    ui: &Ui,
    state: &mut AppState,
    rect: Rect,
    camera: &mut TurntableCamera,
) -> bool {
    let update = crate::sweep_gesture::handle_sweep(
        ui,
        Id::new(ROLL_SWEEP_ID),
        crate::shortcuts::Shortcut::ViewTrackball,
        rect,
        camera.roll,
        ROLL_RADIANS_PER_POINT,
        None,
    );
    if let Some(roll) = update.value
        && roll.is_finite()
    {
        camera.roll = roll.rem_euclid(std::f32::consts::TAU);
    }
    let armed = crate::sweep_gesture::sweep_active(ui, Id::new(ROLL_SWEEP_ID));
    let mode = if armed {
        ControlMode::Trackball
    } else {
        ControlMode::Orbit
    };
    if state.camera_control != mode {
        state.dispatch(Action::SetCameraControl(mode));
    }

    update.consumed || update.finished
}

fn camera_mode_owns_pointer(state: &AppState) -> bool {
    state.camera_control != ControlMode::Orbit
}

fn viewport_camera_input(
    ui: &Ui,
    response: &Response,
    rect: Rect,
    mode: ControlMode,
    roll: f32,
) -> ViewportCameraInput {
    let modifiers = ui.input(|input| input.modifiers);

    let pointer_delta = ui.input(|input| input.pointer.delta());
    let mut camera_input = ViewportCameraInput {
        delta: CameraDelta {
            viewport_height_points: rect.height(),
            ..Default::default()
        },
        wheel_points: 0.0,
        snap_orbit: false,
    };

    if mode != ControlMode::Orbit
        && response.dragged_by(PointerButton::Primary)
        && let Some(origin) = response.interact_pointer_pos()
    {
        let to = origin;
        let from = to - pointer_delta;
        if let Some(gesture) = interpret_drag(
            mode,
            DragSample {
                from,
                to,
                pivot: rect.center(),
                radius: rect.width().min(rect.height()) * 0.5,
            },
            roll,
        ) {
            camera_input.apply(gesture);
        }
    }
    if response.dragged_by(PointerButton::Middle) {
        let binding = middle_drag_gesture(modifiers.shift, modifiers.ctrl);
        if let Some(gesture) = binding.gesture(pointer_delta, roll) {
            camera_input.apply(gesture);
        }
        camera_input.snap_orbit = binding == MiddleDragBinding::Orbit && modifiers.alt;
    }
    if response.hovered() {
        camera_input.wheel_points = ui.input(|input| input.smooth_scroll_delta.y);
    }
    camera_input
}

fn viewport_camera_motion(
    ui: &Ui,
    response: &Response,
    rect: Rect,
    mut camera: TurntableCamera,
    mode: ControlMode,
    pick_world_point: &dyn Fn(Ray3) -> Option<glam::Vec3>,
    wheel_claimed: bool,
) -> Option<TurntableCamera> {
    let mut camera_input = viewport_camera_input(ui, response, rect, mode, camera.roll);

    if wheel_claimed {
        camera_input.wheel_points = 0.0;
    }
    if !camera_input.has_motion() {
        return None;
    }
    if camera_input.delta.has_motion() {
        let free_orbit = camera_input.delta.orbit_points != Vec2::ZERO && !camera_input.snap_orbit;
        if free_orbit {
            camera.commit_snap();
        }
        camera.apply_delta(camera_input.delta);
        if camera_input.snap_orbit {
            camera.snap_view = true;
        }
    }
    if camera_input.wheel_points != 0.0 {
        let cursor = ui
            .input(|input| input.pointer.hover_pos())
            .filter(|pointer| rect.contains(*pointer));
        match cursor {
            Some(cursor) => {
                let anchor = camera
                    .ray_from_screen(cursor, rect)
                    .and_then(pick_world_point);
                camera.zoom_about_screen_point(camera_input.wheel_points, cursor, rect, anchor);
            }
            None => camera.apply_delta(CameraDelta {
                scroll_points: camera_input.wheel_points,
                viewport_height_points: rect.height(),
                ..Default::default()
            }),
        }
    }
    Some(camera)
}

fn frame_shortcut_pressed(ui: &Ui, response: &Response) -> bool {
    response.hovered() && crate::shortcuts::Shortcut::FrameSelected.pressed(ui)
}

fn nearest_visible_world_hit(
    ray: Ray3,
    layers: &[(Option<&SurfaceMesh>, ModelTransform)],
) -> Option<glam::Vec3> {
    let mut best: Option<(f64, glam::DVec3)> = None;
    for (mesh, transform) in layers {
        let Some(mesh) = mesh else {
            continue;
        };
        let Some(hit) = mesh.pick_visible_surface(ray, *transform) else {
            continue;
        };
        let world = transform.point_to_world(hit.local_point);
        let along = (world - ray.origin).dot(ray.direction);
        if !along.is_finite() || along <= 0.0 {
            continue;
        }
        if best.is_none_or(|(current, _)| along < current) {
            best = Some((along, world));
        }
    }
    best.map(|(_, world)| world.as_vec3())
}

fn handle_light_interaction(ui: &Ui, state: &mut AppState, response: &Response) {
    let rotate = ui.input(|input| {
        input.modifiers.shift
            && input.pointer.button_down(PointerButton::Secondary)
            && response.hovered()
    });
    if rotate {
        let delta = ui.input(|input| input.pointer.delta().x);
        if delta != 0.0 {
            state.dispatch(crate::state::Action::RotateLight(delta * 0.012));
            ui.ctx().request_repaint();
        }
    }
}

mod panels;

pub(crate) use panels::panel_list_budget;
use panels::*;

mod detail_hud;

use detail_hud::*;
pub(crate) use detail_hud::{detail_header_band_bottom, draw_detail_workspace};
pub(crate) use detail_panels::{forget_stencil_projection, stencil_projection};

fn sculpt_brush_icon(brush: SculptBrush) -> Icon {
    match brush {
        SculptBrush::Move => Icon::BrushMove,
        SculptBrush::Smooth => Icon::BrushSmooth,
        SculptBrush::Restore => Icon::BrushRestore,
    }
}

fn sculpt_brush_text_key(brush: SculptBrush) -> TextKey {
    match brush {
        SculptBrush::Move => TextKey::SculptBrushMove,
        SculptBrush::Smooth => TextKey::SculptBrushSmooth,
        SculptBrush::Restore => TextKey::SculptBrushRestore,
    }
}

fn sculpt_brush_tooltip_key(brush: SculptBrush) -> TextKey {
    match brush {
        SculptBrush::Move => TextKey::SculptBrushMoveTooltip,
        SculptBrush::Smooth => TextKey::SculptBrushSmoothTooltip,
        SculptBrush::Restore => TextKey::SculptBrushRestoreTooltip,
    }
}

fn sculpt_brush_selector(
    ui: &mut Ui,
    locale: Locale,
    current: SculptBrush,
    compact: bool,
) -> Option<SculptBrush> {
    let popup_id = Id::new("vkit.viewport.sculpt.brush");
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let trigger_width = if compact {
        DETAIL_HUD_COMPACT_BRUSH_WIDTH
    } else {
        SCULPT_BRUSH_FIELD_WIDTH
    };
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(trigger_width, BRUSH_FALLOFF_FIELD_HEIGHT),
        Sense::click(),
    );

    let response = response.on_hover_text(text(locale, sculpt_brush_tooltip_key(current)));
    let fill = if open || response.hovered() {
        crate::theme::hover_fill(crate::theme::COLOR_FIELD)
    } else {
        crate::theme::COLOR_FIELD
    };
    ui.painter().rect_filled(rect, SMALL_RADIUS, fill);
    ui.painter().rect_stroke(
        rect,
        SMALL_RADIUS,
        Stroke::new(
            1.0,
            if open {
                crate::theme::COLOR_HAIRLINE
            } else {
                COLOR_BORDER
            },
        ),
        egui::StrokeKind::Inside,
    );
    control_affordances(ui, &response, rect, f32::from(SMALL_RADIUS));

    let icon_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 7.0, rect.center().y - 8.0),
        Vec2::splat(16.0),
    );
    paint_icon(
        ui.painter(),
        icon_rect,
        sculpt_brush_icon(current),
        COLOR_TEXT,
    );
    if !compact {
        ui.painter().text(
            Pos2::new(icon_rect.right() + 7.0, rect.center().y),
            Align2::LEFT_CENTER,
            text(locale, sculpt_brush_text_key(current)),
            FontId::proportional(FONT_SM),
            COLOR_TEXT,
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            Pos2::new(rect.right() - 10.0, rect.center().y),
            Vec2::splat(12.0),
        ),
        Icon::ChevronDown,
        COLOR_MUTED,
    );

    let mut selected = None;
    egui::Popup::menu(&response)
        .id(popup_id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(SCULPT_BRUSH_FIELD_WIDTH);
            ui.spacing_mut().item_spacing.y = 3.0;
            for brush in SculptBrush::ALL {
                let row = selector_menu_row_with_hint(
                    ui,
                    sculpt_brush_icon(brush),
                    text(locale, sculpt_brush_text_key(brush)),
                    Some(sculpt_brush_hint(brush)),
                    brush == current,
                )
                .on_hover_text(text(locale, sculpt_brush_tooltip_key(brush)));
                if row.clicked() {
                    selected = Some(brush);
                    ui.close();
                }
            }
        });
    selected
}

fn sculpt_falloff_selector(
    ui: &mut Ui,
    locale: Locale,
    current: SculptFalloff,
    compact: bool,
) -> Option<SculptFalloff> {
    brush_falloff_selector(
        ui,
        Id::new("vkit.viewport.sculpt.falloff"),
        locale,
        current,
        compact,
    )
}

#[cfg(test)]
fn sculpt_falloff_icon(falloff: SculptFalloff) -> Icon {
    crate::ui_components::brush_falloff_icon(falloff)
}

fn selector_menu_row_with_hint(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    shortcut: Option<&str>,
    selected: bool,
) -> Response {
    let (row, response) =
        ui.allocate_exact_size(Vec2::new(BRUSH_FALLOFF_FIELD_WIDTH, 26.0), Sense::click());
    paint_list_row_highlight(ui, row, selected, response.hovered());
    let icon_rect = Rect::from_min_size(
        Pos2::new(row.left() + 7.0, row.center().y - 8.0),
        Vec2::splat(16.0),
    );
    paint_icon(ui.painter(), icon_rect, icon, COLOR_TEXT);
    ui.painter().text(
        Pos2::new(icon_rect.right() + 7.0, row.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(FONT_SM),
        COLOR_TEXT,
    );
    if let Some(shortcut) = shortcut {
        ui.painter().text(
            Pos2::new(row.right() - 8.0, row.center().y),
            Align2::RIGHT_CENTER,
            shortcut,
            FontId::proportional(FONT_SM),
            COLOR_MUTED,
        );
    }
    control_affordances(ui, &response, row, f32::from(SMALL_RADIUS));
    response
}

fn draw_detail_separator(ui: &mut Ui) {
    ui.add_space(SPACE_1);
    let (divider, _) = ui.allocate_exact_size(vec2(1.0, 18.0), Sense::hover());
    ui.painter().line_segment(
        [
            pos2(divider.center().x, divider.top()),
            pos2(divider.center().x, divider.bottom()),
        ],
        Stroke::new(1.0, COLOR_BORDER),
    );
    ui.add_space(SPACE_1);
}

mod detail_panels;
use detail_panels::*;

mod sculpt_input;

pub(crate) use sculpt_input::clear_sculpt_pointer_stroke;
use sculpt_input::*;

fn set_edit_camera(state: &mut AppState, side: MeshSide, camera: TurntableCamera) {
    match side {
        MeshSide::Scan => state.workspace.scan_camera = camera,
        MeshSide::Template => state.workspace.template_camera = camera,
    }
    state.workspace.reconcile_linked_edit_cameras(side);
}

#[cfg(test)]
fn apply_edit_camera_delta(state: &mut AppState, side: MeshSide, delta: CameraDelta) {
    let mut camera = match side {
        MeshSide::Scan => state.workspace.scan_camera,
        MeshSide::Template => state.workspace.template_camera,
    };
    camera.apply_delta(delta);
    set_edit_camera(state, side, camera);
}

#[derive(Clone, Copy, Debug)]
struct SceneView {
    rect: Rect,
    camera: TurntableCamera,
    transform: ModelTransform,
}

fn handle_pin_interaction(
    ui: &Ui,
    state: &mut AppState,
    side: MeshSide,
    view: SceneView,
    response: &Response,
    mesh: &SurfaceMesh,
) {
    let SceneView {
        rect,
        camera,
        transform,
    } = view;
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let modifiers = ui.input(|input| input.modifiers);
    let primary_pressed = ui.input(|input| input.pointer.button_pressed(PointerButton::Primary));
    if primary_pressed
        && modifiers.alt
        && response.hovered()
        && let Some(pointer) = pointer
        && let Some(index) = nearest_pin(state, side, pointer, rect, mesh, camera, transform)
        && state.begin_surface_pin_drag(side, index)
    {
        state.workspace.active_drag = Some(ActivePinDrag {
            side,
            pair_index: index,
        });
    }

    let active = state.workspace.active_drag;
    let primary_down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));
    if primary_down
        && let Some(active) = active
        && active.side == side
        && let Some(pointer) = pointer
        && let Some(hit) = pick_surface(side, camera, pointer, rect, mesh, transform)
    {
        state.move_surface_pin(side, active.pair_index, hit);
        ui.ctx().request_repaint();
    }

    if ui.input(|input| input.pointer.button_released(PointerButton::Primary))
        && state
            .workspace
            .active_drag
            .is_some_and(|drag| drag.side == side)
    {
        state.workspace.active_drag = None;
    }

    if response.clicked_by(PointerButton::Primary)
        && !modifiers.alt
        && let Some(pointer) = pointer
        && let Some(endpoint) = pick_surface(side, camera, pointer, rect, mesh, transform)
    {
        add_pin_with_optional_mirror(state, side, mesh, endpoint);
    }

    if response.clicked_by(PointerButton::Secondary)
        && !modifiers.shift
        && let Some(pointer) = pointer
        && let Some(index) = nearest_pin(state, side, pointer, rect, mesh, camera, transform)
    {
        state.delete_surface_pin(side, index);
    }
}

fn add_pin_with_optional_mirror(
    state: &mut AppState,
    side: MeshSide,
    mesh: &SurfaceMesh,
    endpoint: SurfaceEndpoint,
) {
    if !state.x_mirror {
        state.add_surface_pin(side, endpoint);
        return;
    }
    let Some(point) = mesh.endpoint_local_point(endpoint) else {
        state.add_surface_pin(side, endpoint);
        state.x_mirror = false;
        state.status = StatusMessage::with_detail(
            TextKey::XMirrorRejected,
            StatusTone::Warning,
            "invalid picked surface attachment",
        );
        return;
    };
    let transform = match side {
        MeshSide::Scan => scan_transform(state),
        MeshSide::Template => ModelTransform::default(),
    };
    let world_point = transform.point_to_world(point);
    let world_mesh = mesh_in_world_space(&mesh.mesh, transform);
    let center_x = mesh_center_x(&world_mesh);
    let projection = project_mirrored_surface_pin_x(
        &world_mesh,
        world_point.to_array(),
        MirrorPinOptions {
            center_x,
            ..Default::default()
        },
    );
    if projection.reason == MirrorPinReason::CenterlineSingle {
        state.add_surface_pin(side, endpoint);
        return;
    }
    let mirrored =
        projection
            .triangle_id
            .zip(projection.barycentric)
            .map(|(triangle, barycentric)| SurfaceEndpoint {
                triangle,
                barycentric,
            });
    if projection.accepted
        && projection.create_counterpart
        && let Some(mirrored) = mirrored
    {
        if !mesh.is_editable_triangle(mirrored.triangle) {
            state.add_surface_pin(side, endpoint);
            state.x_mirror = false;
            state.status = StatusMessage::with_detail(
                TextKey::XMirrorRejected,
                StatusTone::Warning,
                "mirrored point is outside the editable surface",
            );
            return;
        }

        if world_point.x <= center_x {
            state.add_surface_pins(side, [endpoint, mirrored]);
        } else {
            state.add_surface_pins(side, [mirrored, endpoint]);
        }
        return;
    }

    state.add_surface_pin(side, endpoint);
    state.x_mirror = false;
    state.status = StatusMessage::with_detail(
        TextKey::XMirrorRejected,
        StatusTone::Warning,
        format!("{:?}", projection.reason),
    );
}

fn mesh_in_world_space(mesh: &CoreMesh, transform: ModelTransform) -> CoreMesh {
    CoreMesh {
        vertices: mesh
            .vertices
            .iter()
            .map(|vertex| {
                transform
                    .point_to_world(glam::DVec3::from_array(*vertex))
                    .to_array()
            })
            .collect(),
        triangles: mesh.triangles.clone(),
    }
}

fn mesh_center_x(mesh: &CoreMesh) -> f64 {
    let (min_x, max_x) = mesh
        .vertices
        .iter()
        .map(|vertex| vertex[0])
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(min_x, max_x), x| {
            (min_x.min(x), max_x.max(x))
        });
    if min_x.is_finite() && max_x.is_finite() {
        (min_x + max_x) * 0.5
    } else {
        0.0
    }
}

fn pick_surface(
    side: MeshSide,
    camera: TurntableCamera,
    pointer: egui::Pos2,
    rect: Rect,
    mesh: &SurfaceMesh,
    transform: ModelTransform,
) -> Option<SurfaceEndpoint> {
    let ray = camera.ray_from_screen(pointer, rect)?;
    match side {
        MeshSide::Scan => mesh.pick_visible_surface(ray, transform),
        MeshSide::Template => mesh.pick_editable_surface(ray, transform),
    }
    .map(SurfaceEndpoint::from)
}

fn nearest_pin(
    state: &AppState,
    side: MeshSide,
    pointer: egui::Pos2,
    rect: Rect,
    mesh: &SurfaceMesh,
    camera: TurntableCamera,
    transform: ModelTransform,
) -> Option<usize> {
    projected_markers(state, side, rect, mesh, camera, transform)
        .into_iter()
        .filter_map(|marker| {
            let distance = marker.screen.distance(pointer);
            (distance <= PIN_HIT_RADIUS).then_some((distance, marker.pair_index))
        })
        .min_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
        })
        .map(|(_, index)| index)
}

fn projected_markers(
    state: &AppState,
    side: MeshSide,
    rect: Rect,
    mesh: &SurfaceMesh,
    camera: TurntableCamera,
    transform: ModelTransform,
) -> Vec<PinMarker> {
    let mismatch_start = state.workspace.pins.mismatch_start();
    state
        .workspace
        .pins
        .pairs()
        .iter()
        .enumerate()
        .filter_map(|(pair_index, pair)| {
            let endpoint = pair.endpoint(side)?;
            let world = mesh.endpoint_world_point(endpoint, transform)?;
            let projected = camera.project(world, rect)?;
            if !rect.contains(projected.screen)
                || !pin_has_camera_line_of_sight(endpoint, world, mesh, camera, transform)
            {
                return None;
            }
            Some(PinMarker {
                pair_index,
                screen: projected.screen,
                mismatch: mismatch_start.is_some_and(|start| pair_index >= start)
                    || state.workspace.pins.requires_review(pair_index),
            })
        })
        .collect()
}

fn pin_surface_facing(
    endpoint: SurfaceEndpoint,
    world: glam::Vec3,
    mesh: &SurfaceMesh,
    camera: TurntableCamera,
    transform: ModelTransform,
) -> Option<f64> {
    let triangle = mesh.mesh.triangles.get(endpoint.triangle as usize)?;

    let [a, b, c] = triangle.map(|index| {
        transform.point_to_world(glam::DVec3::from_array(
            *mesh.mesh.vertices.get(index as usize).unwrap_or(&[0.0; 3]),
        ))
    });
    let world_normal = (b - a).cross(c - a).normalize_or_zero();
    if world_normal == glam::DVec3::ZERO {
        return None;
    }

    let centre = transform.point_to_world(mesh.visible_bounds.center().as_dvec3());
    let outward = if world_normal.dot(world.as_dvec3() - centre) < 0.0 {
        -world_normal
    } else {
        world_normal
    };
    let toward_eye = (camera.eye().as_dvec3() - world.as_dvec3()).normalize_or_zero();
    (toward_eye != glam::DVec3::ZERO).then(|| outward.dot(toward_eye))
}

fn pin_occlusion_tolerance_share(facing: f64) -> f64 {
    let graze = 1.0 - facing.clamp(0.0, 1.0);
    PIN_OCCLUSION_TOLERANCE_FACTOR + PIN_OCCLUSION_GRAZE_ALLOWANCE * graze
}

fn pin_has_camera_line_of_sight(
    endpoint: SurfaceEndpoint,
    world: glam::Vec3,
    mesh: &SurfaceMesh,
    camera: TurntableCamera,
    transform: ModelTransform,
) -> bool {
    let facing = pin_surface_facing(endpoint, world, mesh, camera, transform);
    if facing.is_some_and(|facing| facing < PIN_BACKFACE_CUTOFF) {
        return false;
    }
    let eye = camera.eye();
    let Some(ray) = Ray3::new(eye, world - eye) else {
        return false;
    };
    let endpoint_distance = (world.as_dvec3() - ray.origin).dot(ray.direction);
    if !endpoint_distance.is_finite() || endpoint_distance < 0.0 {
        return false;
    }

    let Some(first_hit) = mesh.pick_visible_surface(ray, transform) else {
        return facing.is_some();
    };
    let first_world = transform.point_to_world(first_hit.local_point);
    let first_distance = (first_world - ray.origin).dot(ray.direction);
    let surface_radius =
        f64::from(mesh.visible_bounds.radius()) * transform.scale_xyz.abs().max_element();
    let coordinate_magnitude = world
        .as_dvec3()
        .abs()
        .max_element()
        .max(first_world.abs().max_element());
    let f32_roundoff = coordinate_magnitude * f64::from(f32::EPSILON) * 4.0;
    let share = pin_occlusion_tolerance_share(facing.unwrap_or(1.0));
    let tolerance = (surface_radius * share).max(f32_roundoff).max(1.0e-6);
    endpoint_distance <= first_distance + tolerance
}

fn paint_pin_markers(
    ui: &Ui,
    state: &AppState,
    side: MeshSide,
    rect: Rect,
    mesh: &SurfaceMesh,
    camera: TurntableCamera,
    transform: ModelTransform,
) {
    let active = state.workspace.active_drag;
    for marker in projected_markers(state, side, rect, mesh, camera, transform) {
        let selected =
            active.is_some_and(|drag| drag.side == side && drag.pair_index == marker.pair_index);
        let fill = if marker.mismatch {
            COLOR_DESTRUCTIVE
        } else {
            match side {
                MeshSide::Scan => custom_head_solid_color(state),
                MeshSide::Template => g2_solid_color(state),
            }
        };
        let radius = if selected { 6.5 } else { 5.0 };
        ui.painter().circle_filled(marker.screen, radius, fill);
        ui.painter().circle_stroke(
            marker.screen,
            radius,
            Stroke::new(if selected { 2.0 } else { 1.0 }, COLOR_BG),
        );
        if state.pin_numbers {
            ui.painter().text(
                marker.screen + vec2(radius + 3.0, -radius - 1.0),
                Align2::LEFT_BOTTOM,
                format!("{:02}", marker.pair_index + 1),
                FontId::proportional(FONT_XS),
                if marker.mismatch {
                    COLOR_DESTRUCTIVE
                } else {
                    COLOR_TEXT
                },
            );
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct MeshDraw {
    color: [f32; 4],
    style: RenderStyle,
    depth_scope: RenderDepthScope,
}

fn add_mesh_callback(
    ui: &Ui,
    view: SceneView,
    scene_key: u64,
    mesh: Arc<SurfaceMesh>,
    grading: ViewportGrading,
    smooth_passes: u8,
    draw: MeshDraw,
) {
    let SceneView {
        rect,
        camera,
        transform,
    } = view;
    let MeshDraw {
        color,
        style,
        depth_scope,
    } = draw;
    let callback = MeshPaintCallback {
        frame_radius: camera.frame_radius,
        scene_key,
        mesh,
        view_projection: camera.view_projection(rect.width() / rect.height().max(1.0)),
        model: transform.matrix(),
        eye: camera.eye(),
        light_yaw_radians: grading.light_yaw_radians,
        light_preset: grading.preset,
        light_brightness: grading.brightness,
        tone_mapping: grading.tone_mapping,
        smooth_passes,
        color,
        style,
        depth_scope,
    };
    ui.painter().add(callback.paint_callback(rect));
}

#[derive(Clone, Copy, Debug)]
struct SkinDraw {
    visibility: SkinVisibilityGroups,
    show_tear_lacrimals: bool,
    show_eyelashes: bool,
    depth_scope: RenderDepthScope,
}

fn add_skin_callback(
    ui: &Ui,
    view: SceneView,
    scene_key: u64,
    mesh: Arc<SurfaceMesh>,
    skin: Arc<crate::skin_preview::SkinPreview>,
    state: &AppState,
    draw: SkinDraw,
) {
    let SceneView {
        rect,
        camera,
        transform,
    } = view;
    let SkinDraw {
        visibility: skin_visibility,
        show_tear_lacrimals,
        show_eyelashes,
        depth_scope,
    } = draw;
    let callback = SkinPaintCallback {
        frame_radius: camera.frame_radius,
        scene_key,
        mesh,
        skin,
        view_projection: camera.view_projection(rect.width() / rect.height().max(1.0)),
        model: transform.matrix(),
        eye: camera.eye(),
        light_yaw_radians: state.light_yaw_radians,
        light_preset: state.lighting_preset,
        light_brightness: state.light_brightness,
        tone_mapping: state.tone_mapping,
        depth_scope,
        skin_visibility,
        show_tear_lacrimals,
        show_eyelashes,
        smooth_passes: state.surface_smooth_passes,
    };
    ui.painter().add(callback.paint_callback(rect));
}

const HAIR_SCALP_SCENE_KEY: u64 = 0x5343_4C50_0000_0000;

fn begin_bloom(ui: &Ui, state: &AppState, rect: Rect) -> bool {
    if !(state.bloom.contributes() || state.ambient_occlusion.contributes()) || !rect.is_positive()
    {
        return false;
    }
    ui.painter()
        .add(crate::bloom::BloomBeginCallback.paint_callback(rect));
    true
}

fn end_bloom(ui: &Ui, state: &AppState, rect: Rect, camera: TurntableCamera, opened: bool) {
    if !opened {
        return;
    }
    let view_projection = camera.view_projection(rect.width() / rect.height().max(1.0));
    ui.painter().add(
        crate::bloom::BloomOverlayCallback {
            settings: state.bloom,
            occlusion: state.ambient_occlusion,
            view: crate::ambient_occlusion::AoView {
                view_projection: view_projection.to_cols_array_2d(),
                inverse_view_projection: view_projection.inverse().to_cols_array_2d(),
                eye: camera.eye().to_array(),
                frame_radius: camera.frame_radius,

                exposure: 1.0,
                filmic: state.tone_mapping.shader_flag(),
            },
            rect,

            exposure: 1.0,
            filmic: state.tone_mapping.shader_flag(),
        }
        .paint_callback(rect),
    );
}

fn add_hair_try_on(
    ui: &Ui,
    rect: Rect,
    state: &AppState,
    head: &Arc<SurfaceMesh>,
    scene_key: u64,
    view: ResultRenderView,
) {
    if !state.hair_visible {
        return;
    }
    let Some(hair) = state.hair_preview.as_ref().map(Arc::clone) else {
        return;
    };
    for (index, scalp) in hair.scalps.iter().enumerate() {
        let callback = ScalpPaintCallback {
            scene_key: scene_key ^ (HAIR_SCALP_SCENE_KEY + index as u64),
            head: Arc::clone(head),
            part: scalp.clone(),
            view_projection: view
                .camera
                .view_projection(rect.width() / rect.height().max(1.0)),
            model: ModelTransform::default().matrix(),
            eye: view.camera.eye(),
            light_yaw_radians: view.grading.light_yaw_radians,
            light_preset: view.grading.preset,
            light_brightness: view.grading.brightness,
            tone_mapping: view.grading.tone_mapping,
        };
        ui.painter().add(callback.paint_callback(rect));
    }
    add_hair_callback(ui, state, rect, Arc::clone(head), hair, scene_key, view);
}

fn add_hair_callback(
    ui: &Ui,
    state: &AppState,
    rect: Rect,
    mesh: Arc<SurfaceMesh>,
    preview: Arc<crate::hair_preview::HairPreview>,
    scene_key: u64,
    view: ResultRenderView,
) {
    let settling = state.hair_settle_seconds > 0.0;

    let head_changed_recently = {
        let probe = egui::Id::new(("hair-head-revision", scene_key));
        let now = ui.input(|input| input.time);
        ui.ctx().data_mut(|data| {
            let previous: Option<(u64, f64)> = data.get_temp(probe);
            let stamp = match previous {
                Some((revision, stamp)) if revision == mesh.revision => stamp,
                _ => now,
            };
            data.insert_temp(probe, (mesh.revision, stamp));
            now - stamp
                < crate::hair_physics::SHAPE_QUIET_SECONDS
                    + f64::from(crate::hair_physics::SHAPE_CHANGE_SETTLE_SECONDS)
                    + 0.25
        })
    };
    let pump_frames = settling || head_changed_recently;
    let callback = HairPaintCallback {
        scene_key,
        mesh,
        preview,
        view_projection: view
            .camera
            .view_projection(rect.width() / rect.height().max(1.0)),
        model: ModelTransform::default().matrix(),
        eye: view.camera.eye(),
        light_yaw_radians: view.grading.light_yaw_radians,
        light_preset: view.grading.preset,
        light_brightness: view.grading.brightness,
        tone_mapping: view.grading.tone_mapping,
        time_seconds: ui.input(|input| input.time),
        settle_gravity: if settling {
            crate::state::HAIR_SETTLE_GRAVITY
        } else {
            0.0
        },

        viewport_pixels: {
            let scale = ui.ctx().pixels_per_point();
            [rect.width() * scale, rect.height() * scale]
        },
    };
    ui.painter().add(callback.paint_callback(rect));
    if pump_frames {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

#[derive(Clone, Copy)]
struct ResultRenderView {
    camera: TurntableCamera,
    grading: ViewportGrading,
    smooth_passes: u8,
}

#[derive(Clone, Copy)]
struct EyeLookPreview {
    left: ModelTransform,
    right: ModelTransform,
    left_pitch: f64,
    right_pitch: f64,
}

fn sculpt_eye_look_preview(state: &AppState, gaze: [f32; 2]) -> Option<EyeLookPreview> {
    if !state.sculpt_eye_tracking {
        return None;
    }
    let template = state.workspace.template_geometry.as_deref()?;
    let result = state.workspace.result_output.as_deref()?;
    let eye_transform = |id: &str, fallback: [f64; 3], left_eye: bool| {
        let base_center = template
            .bone(id)
            .map(|bone| CoreVec3::from(bone.center_point))
            .unwrap_or_else(|| CoreVec3::from(fallback));
        let center =
            transfer_rig_point_idw(base_center, &template.vertices, &result.vertices).ok()?;
        let [yaw, pitch] = eye_angles_from_gaze(gaze, left_eye);
        Some((
            ModelTransform::from_components_with_pivot(
                1.0,
                [0.0; 3],
                [-pitch, yaw, 0.0],
                [center.x, center.y, center.z],
            ),
            pitch,
        ))
    };
    let (left, left_pitch) = eye_transform("lEye", [3.13, 168.03, 5.828_937], true)?;
    let (right, right_pitch) = eye_transform("rEye", [-3.13, 168.03, 5.828_937], false)?;
    Some(EyeLookPreview {
        left,
        right,
        left_pitch,
        right_pitch,
    })
}

fn gaze_from_screen(pointer: Pos2, rect: Rect) -> [f32; 2] {
    let half_width = (rect.width() * 0.5).max(1.0);
    let half_height = (rect.height() * 0.5).max(1.0);
    [
        ((pointer.x - rect.center().x) / half_width).clamp(-1.0, 1.0),
        ((rect.center().y - pointer.y) / half_height).clamp(-1.0, 1.0),
    ]
}

fn viewport_eye_gaze(ui: &Ui, state: &AppState, viewport: Rect) -> [f32; 2] {
    if state.eye_gaze_mode == EyeGazeMode::Manual {
        return state.manual_eye_gaze;
    }
    let cache_id = Id::new("vkit.viewport.eye-gaze.auto");
    if let Some(pointer) = ui.input(|input| input.pointer.hover_pos())
        && viewport.contains(pointer)
    {
        let gaze = gaze_from_screen(pointer, viewport);
        ui.data_mut(|data| data.insert_temp(cache_id, gaze));
        return gaze;
    }
    ui.data(|data| data.get_temp::<[f32; 2]>(cache_id))
        .unwrap_or(state.manual_eye_gaze)
}

fn eye_angles_from_gaze(gaze: [f32; 2], left_eye: bool) -> [f64; 2] {
    crate::state::eye_angles_from_gaze(gaze, left_eye)
}

fn grouped_skin_depth_sequence(callback_count: usize) -> Vec<RenderDepthScope> {
    (0..callback_count)
        .map(|index| {
            if index == 0 {
                RenderDepthScope::ResetBeforeDraw
            } else {
                RenderDepthScope::Shared
            }
        })
        .collect()
}

fn add_sculpt_result_callbacks(
    ui: &Ui,
    rect: Rect,
    state: &AppState,
    view: ResultRenderView,
    eye_gaze: [f32; 2],
    result_mesh: Arc<SurfaceMesh>,
) {
    let bloomed = begin_bloom(ui, state, rect);
    let visible = sculpt_visible_targets(state);
    let active = state.sculpt.editable_targets();
    let mut surfaces = state.workspace.result_sculpt_surfaces().collect::<Vec<_>>();
    surfaces.retain(|surface| visible.contains(sculpt_group_target(surface.group)));
    let eye_preview =
        sculpt_eye_look_preview(state, eye_gaze).zip(state.workspace.result_eye_surfaces());
    if eye_preview.is_some() {
        surfaces.retain(|surface| surface.group != SculptSurfaceGroup::Eyes);
    }
    let has_grouped_surfaces = !surfaces.is_empty() || eye_preview.is_some();

    let skin = (state.base_view_mode == BaseViewMode::Texture)
        .then(|| state.active_skin_preview())
        .flatten();

    if let Some(skin) = skin {
        if has_grouped_surfaces {
            let eye_callback_count =
                usize::from(visible.contains(SculptTarget::Eyes) && eye_preview.is_some()) * 2;
            let mut depth_sequence =
                grouped_skin_depth_sequence(surfaces.len() + eye_callback_count).into_iter();
            for surface in &surfaces {
                if sculpt_group_needs_solid(&skin, surface.group) {
                    let _ = depth_sequence.next();
                    add_mesh_callback(
                        ui,
                        SceneView {
                            rect,
                            camera: view.camera,
                            transform: ModelTransform::default(),
                        },
                        sculpt_group_scene_key(surface.group),
                        Arc::clone(&surface.mesh),
                        view.grading,
                        view.smooth_passes,
                        MeshDraw {
                            color: color_array(g2_solid_color(state), 1.0),
                            style: RenderStyle::Solid,
                            depth_scope: RenderDepthScope::Shared,
                        },
                    );
                    continue;
                }
                add_skin_callback(
                    ui,
                    SceneView {
                        rect,
                        camera: view.camera,
                        transform: ModelTransform::default(),
                    },
                    sculpt_group_scene_key(surface.group),
                    Arc::clone(&surface.mesh),
                    Arc::clone(&skin),
                    state,
                    SkinDraw {
                        visibility: sculpt_skin_visibility(visible),
                        show_tear_lacrimals: true,
                        show_eyelashes: true,
                        depth_scope: depth_sequence
                            .next()
                            .expect("grouped skin depth sequence covers every surface"),
                    },
                );
            }
            if visible.contains(SculptTarget::Eyes)
                && let Some((eye_preview, (left_eye, right_eye))) = &eye_preview
            {
                for (scene_key, mesh, transform) in [
                    (
                        RESULT_SCULPT_LEFT_EYE_SCENE_KEY,
                        Arc::clone(left_eye),
                        eye_preview.left,
                    ),
                    (
                        RESULT_SCULPT_RIGHT_EYE_SCENE_KEY,
                        Arc::clone(right_eye),
                        eye_preview.right,
                    ),
                ] {
                    add_skin_callback(
                        ui,
                        SceneView {
                            rect,
                            camera: view.camera,
                            transform,
                        },
                        scene_key,
                        mesh,
                        Arc::clone(&skin),
                        state,
                        SkinDraw {
                            visibility: sculpt_skin_visibility(visible),
                            show_tear_lacrimals: true,
                            show_eyelashes: true,
                            depth_scope: depth_sequence
                                .next()
                                .expect("grouped skin depth sequence covers both eyes"),
                        },
                    );
                }
            }
        } else {
            add_skin_callback(
                ui,
                SceneView {
                    rect,
                    camera: view.camera,
                    transform: ModelTransform::default(),
                },
                RESULT_SKIN_SCENE_KEY,
                Arc::clone(&result_mesh),
                skin,
                state,
                SkinDraw {
                    visibility: sculpt_skin_visibility(visible),
                    show_tear_lacrimals: true,
                    show_eyelashes: true,
                    depth_scope: RenderDepthScope::ResetBeforeDraw,
                },
            );
        }
    } else if !has_grouped_surfaces {
        if visible.contains(SculptTarget::HeadSkin) {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera: view.camera,
                    transform: ModelTransform::default(),
                },
                RESULT_SCENE_KEY,
                Arc::clone(&result_mesh),
                view.grading,
                view.smooth_passes,
                MeshDraw {
                    color: color_array(g2_solid_color(state), 1.0),
                    style: RenderStyle::Solid,
                    depth_scope: RenderDepthScope::Shared,
                },
            );
        }
    } else {
        for surface in &surfaces {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera: view.camera,
                    transform: ModelTransform::default(),
                },
                sculpt_group_scene_key(surface.group),
                Arc::clone(&surface.mesh),
                view.grading,
                view.smooth_passes,
                MeshDraw {
                    color: color_array(g2_solid_color(state), 1.0),
                    style: RenderStyle::Solid,
                    depth_scope: RenderDepthScope::Shared,
                },
            );
        }
        if visible.contains(SculptTarget::Eyes)
            && let Some((eye_preview, (left_eye, right_eye))) = &eye_preview
        {
            for (scene_key, mesh, transform) in [
                (
                    RESULT_SCULPT_LEFT_EYE_SCENE_KEY,
                    Arc::clone(left_eye),
                    eye_preview.left,
                ),
                (
                    RESULT_SCULPT_RIGHT_EYE_SCENE_KEY,
                    Arc::clone(right_eye),
                    eye_preview.right,
                ),
            ] {
                add_mesh_callback(
                    ui,
                    SceneView {
                        rect,
                        camera: view.camera,
                        transform,
                    },
                    scene_key,
                    mesh,
                    view.grading,
                    view.smooth_passes,
                    MeshDraw {
                        color: color_array(g2_solid_color(state), 1.0),
                        style: RenderStyle::Solid,
                        depth_scope: RenderDepthScope::Shared,
                    },
                );
            }
        }
    }

    add_hair_try_on(ui, rect, state, &result_mesh, RESULT_HAIR_SCENE_KEY, view);
    end_bloom(ui, state, rect, view.camera, bloomed);

    let wire_alpha = overlay_alpha(state.wireframe_opacity);
    if state.wireframe_visible && wire_alpha > 0.0 {
        if !has_grouped_surfaces {
            if !visible.contains(SculptTarget::HeadSkin) {
                return;
            }
            let (color, alpha) = sculpt_group_wire_style(
                wireframe_color(state),
                active,
                SculptSurfaceGroup::HeadSkin,
                wire_alpha,
            );
            add_colored_result_wire_callback(
                ui,
                rect,
                RESULT_SCENE_KEY,
                Arc::clone(&result_mesh),
                view,
                color,
                alpha,
            );
        } else {
            for surface in &surfaces {
                let (color, alpha) = sculpt_group_wire_style(
                    wireframe_color(state),
                    active,
                    surface.group,
                    wire_alpha,
                );
                add_colored_result_wire_callback(
                    ui,
                    rect,
                    sculpt_group_scene_key(surface.group),
                    Arc::clone(&surface.mesh),
                    view,
                    color,
                    alpha,
                );
            }
        }
    }

    let xray_alpha = overlay_alpha(state.xray_opacity);
    if state.xray_visible && xray_alpha > 0.0 {
        if !has_grouped_surfaces {
            if !visible.contains(SculptTarget::HeadSkin) {
                return;
            }
            add_result_xray_callback(
                ui,
                rect,
                RESULT_SCENE_KEY,
                result_mesh,
                view,
                XrayDraw {
                    color: g2_solid_color(state),
                    alpha: xray_alpha,
                    depth_scope: RenderDepthScope::ResetBeforeDraw,
                },
            );
        } else {
            for (index, surface) in surfaces.iter().enumerate() {
                add_result_xray_callback(
                    ui,
                    rect,
                    sculpt_group_scene_key(surface.group),
                    Arc::clone(&surface.mesh),
                    view,
                    XrayDraw {
                        color: g2_solid_color(state),
                        alpha: xray_alpha,
                        depth_scope: if index == 0 {
                            RenderDepthScope::ResetBeforeDraw
                        } else {
                            RenderDepthScope::Shared
                        },
                    },
                );
            }
        }
    }
}

const fn sculpt_group_target(group: SculptSurfaceGroup) -> SculptTarget {
    match group {
        SculptSurfaceGroup::HeadSkin => SculptTarget::HeadSkin,
        SculptSurfaceGroup::TearLacrimal => SculptTarget::Tear,
        SculptSurfaceGroup::Eyelashes => SculptTarget::Eyelashes,
        SculptSurfaceGroup::Eyes => SculptTarget::Eyes,
        SculptSurfaceGroup::Lips => SculptTarget::Lips,
        SculptSurfaceGroup::InnerMouth => SculptTarget::InnerMouth,
        SculptSurfaceGroup::TeethTongue => SculptTarget::TeethTongue,
    }
}

fn skin_channel_is_visible(image: &crate::skin_preview::SkinImage) -> bool {
    image
        .rgba8
        .chunks_exact(4)
        .any(|pixel| pixel[3] > SKIN_CHANNEL_VISIBLE_ALPHA)
}

const SKIN_CHANNEL_VISIBLE_ALPHA: u8 = 8;

fn sculpt_groups_without_skin_coverage(skin: &crate::skin_preview::SkinPreview) -> [bool; 2] {
    [
        !skin_channel_is_visible(&skin.lacrimal),
        !skin_channel_is_visible(&skin.eyelashes),
    ]
}

fn sculpt_group_needs_solid(
    skin: &crate::skin_preview::SkinPreview,
    group: SculptSurfaceGroup,
) -> bool {
    let [tear_blank, lashes_blank] = sculpt_groups_without_skin_coverage(skin);
    match group {
        SculptSurfaceGroup::TearLacrimal => tear_blank,
        SculptSurfaceGroup::Eyelashes => lashes_blank,
        _ => false,
    }
}

fn sculpt_skin_visibility(visible: SculptTargets) -> SkinVisibilityGroups {
    let mut groups = SkinVisibilityGroups::NONE;
    for (target, group) in [
        (SculptTarget::HeadSkin, SkinVisibilityGroup::HeadSkin),
        (SculptTarget::Lips, SkinVisibilityGroup::HeadSkin),
        (SculptTarget::Tear, SkinVisibilityGroup::Tear),
        (SculptTarget::Eyelashes, SkinVisibilityGroup::Eyelashes),
        (SculptTarget::Eyes, SkinVisibilityGroup::Eyes),
        (SculptTarget::TeethTongue, SkinVisibilityGroup::TeethTongue),
        (SculptTarget::InnerMouth, SkinVisibilityGroup::InnerMouth),
    ] {
        if visible.contains(target) {
            groups = groups.with(group);
        }
    }
    groups
}

const fn sculpt_group_scene_key(group: SculptSurfaceGroup) -> u64 {
    match group {
        SculptSurfaceGroup::HeadSkin => RESULT_SCULPT_HEAD_SCENE_KEY,
        SculptSurfaceGroup::TearLacrimal => RESULT_SCULPT_TEAR_SCENE_KEY,
        SculptSurfaceGroup::Eyelashes => RESULT_SCULPT_LASH_SCENE_KEY,
        SculptSurfaceGroup::Eyes => RESULT_SCULPT_EYES_SCENE_KEY,
        SculptSurfaceGroup::Lips => RESULT_SCULPT_LIPS_SCENE_KEY,
        SculptSurfaceGroup::InnerMouth => RESULT_SCULPT_INNER_MOUTH_SCENE_KEY,
        SculptSurfaceGroup::TeethTongue => RESULT_SCULPT_MOUTH_SCENE_KEY,
    }
}

fn overlay_alpha(opacity: f32) -> f32 {
    if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn result_scan_overlay_alpha(state: &AppState) -> Option<f32> {
    let alpha = overlay_alpha(state.overlay_opacity);
    (state.active_tab == Tab::Result && alpha > 0.0).then_some(alpha)
}

const fn sculpt_group_wire_style(
    wire_color: Color32,
    active: SculptTargets,
    group: SculptSurfaceGroup,
    opacity: f32,
) -> (Color32, f32) {
    if active.contains(sculpt_group_target(group)) {
        (wire_color, opacity)
    } else {
        (wire_color, opacity * SCULPT_INACTIVE_WIRE_FADE)
    }
}

const SCULPT_INACTIVE_WIRE_FADE: f32 = 0.42;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RenderPassPlan {
    solid: bool,
    textured: bool,
    wire: bool,
    xray: bool,
}

fn render_pass_plan(
    base_mode: BaseViewMode,
    texture_compatible: bool,
    texture_available: bool,
    wire_visible: bool,
    wire_opacity: f32,
    xray_visible: bool,
    xray_opacity: f32,
) -> RenderPassPlan {
    let textured = base_mode == BaseViewMode::Texture && texture_compatible && texture_available;
    RenderPassPlan {
        solid: !textured,
        textured,
        wire: wire_visible && overlay_alpha(wire_opacity) > 0.0,
        xray: xray_visible && overlay_alpha(xray_opacity) > 0.0,
    }
}

fn add_result_composed_callbacks(
    ui: &Ui,
    rect: Rect,
    state: &AppState,
    view: ResultRenderView,
    result_mesh: Arc<SurfaceMesh>,
) {
    let bloomed = begin_bloom(ui, state, rect);
    let mut meshes = vec![(RESULT_SCENE_KEY, Arc::clone(&result_mesh))];
    for (scene_key, enabled, part) in [
        (
            RESULT_TEAR_LACRIMAL_SCENE_KEY,
            state.show_result_tear_lacrimals,
            state.workspace.result_tear_lacrimals.clone(),
        ),
        (
            RESULT_EYELASH_SCENE_KEY,
            state.show_result_eyelashes,
            state.workspace.result_eyelashes.clone(),
        ),
    ] {
        if enabled && let Some(part) = part {
            meshes.push((scene_key, part));
        }
    }
    let skin = state.active_skin_preview();
    let plan = render_pass_plan(
        state.base_view_mode,
        true,
        skin.is_some(),
        state.wireframe_visible,
        state.wireframe_opacity,
        state.xray_visible,
        state.xray_opacity,
    );
    if plan.textured {
        add_skin_callback(
            ui,
            SceneView {
                rect,
                camera: view.camera,
                transform: ModelTransform::default(),
            },
            RESULT_SKIN_SCENE_KEY,
            Arc::clone(&result_mesh),
            skin.expect("textured render plan requires an available skin"),
            state,
            SkinDraw {
                visibility: SkinVisibilityGroups::ALL,
                show_tear_lacrimals: state.show_result_tear_lacrimals,
                show_eyelashes: state.show_result_eyelashes,
                depth_scope: RenderDepthScope::ResetBeforeDraw,
            },
        );
    } else if plan.solid {
        for (scene_key, mesh) in &meshes {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera: view.camera,
                    transform: ModelTransform::default(),
                },
                *scene_key,
                Arc::clone(mesh),
                view.grading,
                view.smooth_passes,
                MeshDraw {
                    color: color_array(g2_solid_color(state), 1.0),
                    style: RenderStyle::Solid,
                    depth_scope: RenderDepthScope::Shared,
                },
            );
        }
    }
    add_hair_try_on(ui, rect, state, &result_mesh, RESULT_HAIR_SCENE_KEY, view);
    end_bloom(ui, state, rect, view.camera, bloomed);
    if plan.wire {
        let alpha = overlay_alpha(state.wireframe_opacity);
        for (scene_key, mesh) in &meshes {
            add_colored_result_wire_callback(
                ui,
                rect,
                *scene_key,
                Arc::clone(mesh),
                view,
                wireframe_color(state),
                alpha,
            );
        }
    }
    if plan.xray {
        let alpha = overlay_alpha(state.xray_opacity);
        for (index, (scene_key, mesh)) in meshes.iter().enumerate() {
            add_result_xray_callback(
                ui,
                rect,
                *scene_key,
                Arc::clone(mesh),
                view,
                XrayDraw {
                    color: g2_solid_color(state),
                    alpha,
                    depth_scope: if index == 0 {
                        RenderDepthScope::ResetBeforeDraw
                    } else {
                        RenderDepthScope::Shared
                    },
                },
            );
        }
    }
}

fn add_colored_result_wire_callback(
    ui: &Ui,
    rect: Rect,
    scene_key: u64,
    mesh: Arc<SurfaceMesh>,
    view: ResultRenderView,
    color: Color32,
    alpha: f32,
) {
    add_mesh_callback(
        ui,
        SceneView {
            rect,
            camera: view.camera,
            transform: ModelTransform::default(),
        },
        scene_key ^ VIEWPORT_WIRE_KEY_MASK,
        mesh,
        view.grading,
        view.smooth_passes,
        MeshDraw {
            color: color_array(color, alpha),
            style: RenderStyle::Wire,
            depth_scope: RenderDepthScope::Shared,
        },
    );
}

#[derive(Clone, Copy, Debug)]
struct XrayDraw {
    color: Color32,
    alpha: f32,
    depth_scope: RenderDepthScope,
}

fn add_result_xray_callback(
    ui: &Ui,
    rect: Rect,
    scene_key: u64,
    mesh: Arc<SurfaceMesh>,
    view: ResultRenderView,
    draw: XrayDraw,
) {
    let XrayDraw {
        color,
        alpha,
        depth_scope,
    } = draw;
    add_mesh_callback(
        ui,
        SceneView {
            rect,
            camera: view.camera,
            transform: ModelTransform::default(),
        },
        scene_key ^ VIEWPORT_XRAY_KEY_MASK,
        mesh,
        view.grading,
        view.smooth_passes,
        MeshDraw {
            color: color_array(color, alpha),
            style: RenderStyle::Xray,
            depth_scope,
        },
    );
}

fn scan_transform(state: &AppState) -> ModelTransform {
    let pivot_local = state
        .workspace
        .scan
        .as_deref()
        .map(|scan| scan.facial_focus_bounds().center().as_dvec3().to_array())
        .unwrap_or([0.0; 3]);
    ModelTransform::from_components_xyz_with_pivot(
        state.transform.scale_xyz,
        state.transform.translation_cm,
        state.transform.rotation_degrees,
        pivot_local,
    )
}

#[cfg(test)]
const fn alignment_scan_style() -> RenderStyle {
    RenderStyle::Solid
}

#[cfg(test)]
const fn alignment_template_style() -> RenderStyle {
    RenderStyle::Solid
}

#[cfg(test)]
const fn alignment_template_depth_scope() -> RenderDepthScope {
    RenderDepthScope::Shared
}

#[cfg(test)]
const fn alignment_scan_depth_scope() -> RenderDepthScope {
    RenderDepthScope::ResetBeforeDraw
}

fn alignment_layer_alpha(opacity: f32) -> f32 {
    opacity.clamp(0.0, 1.0)
}

const DROP_TARGET_INSET: f32 = 40.0;

const DROP_TARGET_MAX: Vec2 = Vec2::new(320.0, 240.0);

const DROP_TARGET_RADIUS: f32 = 18.0;

const DROP_DASH: f32 = 9.0;
const DROP_GAP: f32 = 7.0;

const DROP_PLUS_ARM: f32 = 13.0;

const DROP_LABEL_GAP: f32 = 26.0;

fn draw_scan_drop_target(ui: &mut Ui, state: &mut AppState, rect: Rect) {
    let available = rect.shrink(DROP_TARGET_INSET);
    let frame = Rect::from_center_size(
        available.center(),
        vec2(
            available.width().min(DROP_TARGET_MAX.x),
            available.height().min(DROP_TARGET_MAX.y),
        ),
    );
    if frame.width() < 80.0 || frame.height() < 80.0 {
        paint_empty(ui, rect, text(state.locale, TextKey::AddHeadFile));
        return;
    }

    let response = ui.interact(frame, Id::new("vkit.viewport.scan.drop"), Sense::click());

    let carrying = ui.input(|input| !input.raw.hovered_files.is_empty());
    let lit = carrying || response.hovered();

    let calling = crate::ui_components::attention_progress(
        ui,
        crate::ui::attention_target_id(crate::state::AttentionTarget::CustomHeadLoad),
    );
    let color = match calling {
        Some(_) => COLOR_DESTRUCTIVE,
        None if lit => COLOR_TEXT,
        None => COLOR_MUTED,
    };
    if calling.is_some() {
        ui.ctx().request_repaint();
    }

    if carrying {
        ui.painter().rect_filled(
            frame,
            crate::theme::CONTROL_RADIUS,
            crate::theme::COLOR_SURFACE_HOVER,
        );
    }
    let weight = if lit || calling.is_some() { 2.0 } else { 1.0 };
    paint_dashed_rect(ui, frame, Stroke::new(weight, color));

    let center = pos2(frame.center().x, frame.center().y - DROP_LABEL_GAP * 0.5);
    let stroke = Stroke::new(2.0, color);
    ui.painter().line_segment(
        [
            pos2(center.x - DROP_PLUS_ARM, center.y),
            pos2(center.x + DROP_PLUS_ARM, center.y),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            pos2(center.x, center.y - DROP_PLUS_ARM),
            pos2(center.x, center.y + DROP_PLUS_ARM),
        ],
        stroke,
    );
    ui.painter().text(
        pos2(center.x, center.y + DROP_LABEL_GAP + DROP_PLUS_ARM),
        Align2::CENTER_CENTER,
        text(state.locale, TextKey::AddHeadFile),
        FontId::proportional(FONT_BODY),
        color,
    );
    ui.painter().text(
        pos2(
            center.x,
            center.y + DROP_LABEL_GAP + DROP_PLUS_ARM + FONT_BODY + 6.0,
        ),
        Align2::CENTER_CENTER,
        text(state.locale, TextKey::AddHeadFileHint),
        FontId::proportional(FONT_XS),
        COLOR_MUTED,
    );

    if response.clicked() {
        state.dispatch(Action::RequestOpenScan);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

fn paint_dashed_rect(ui: &Ui, rect: Rect, stroke: Stroke) {
    let painter = ui.painter();
    let radius = DROP_TARGET_RADIUS
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5);

    for (centre, start_angle) in [
        (
            pos2(rect.right() - radius, rect.top() + radius),
            -std::f32::consts::FRAC_PI_2,
        ),
        (pos2(rect.right() - radius, rect.bottom() - radius), 0.0),
        (
            pos2(rect.left() + radius, rect.bottom() - radius),
            std::f32::consts::FRAC_PI_2,
        ),
        (
            pos2(rect.left() + radius, rect.top() + radius),
            std::f32::consts::PI,
        ),
    ] {
        let arc_length = radius * std::f32::consts::FRAC_PI_2;
        let step = DROP_DASH + DROP_GAP;
        let count = (arc_length / step).round().max(1.0);
        let stride = arc_length / count;
        let dash = (stride * DROP_DASH / step).min(stride);
        for index in 0..count as usize {
            let from = index as f32 * stride;
            let point_at = |along: f32| {
                let angle = start_angle + along / radius.max(1.0e-3);
                pos2(
                    centre.x + radius * angle.cos(),
                    centre.y + radius * angle.sin(),
                )
            };

            let mid = point_at(from + dash * 0.5);
            painter.line_segment([point_at(from), mid], stroke);
            painter.line_segment([mid, point_at(from + dash)], stroke);
        }
    }
    let corners = [
        (
            pos2(rect.left() + radius, rect.top()),
            pos2(rect.right() - radius, rect.top()),
        ),
        (
            pos2(rect.right(), rect.top() + radius),
            pos2(rect.right(), rect.bottom() - radius),
        ),
        (
            pos2(rect.right() - radius, rect.bottom()),
            pos2(rect.left() + radius, rect.bottom()),
        ),
        (
            pos2(rect.left(), rect.bottom() - radius),
            pos2(rect.left(), rect.top() + radius),
        ),
    ];
    for (from, to) in corners {
        let span = to - from;
        let length = span.length();
        if length <= 0.0 {
            continue;
        }
        let step = DROP_DASH + DROP_GAP;

        let count = (length / step).round().max(1.0);
        let stride = length / count;
        let dash = (stride * DROP_DASH / step).min(stride);
        let direction = span / length;
        for index in 0..count as usize {
            let start = index as f32 * stride;
            painter.line_segment(
                [from + direction * start, from + direction * (start + dash)],
                stroke,
            );
        }
    }
}

fn paint_empty(ui: &Ui, rect: Rect, message: &str) {
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        message,
        FontId::proportional(FONT_BODY),
        COLOR_MUTED,
    );
}

pub(crate) const ORIENTATION_HUD_SIZE: Vec2 = vec2(52.0, 60.0);

const ORIENTATION_AXIS_RADIUS: f32 = 18.0;

const ORIENTATION_MIN_ARM: f32 = 1.5;

const ORIENTATION_DEPTH_FADE: f32 = 0.45;

fn paint_camera_mode_badge(ui: &Ui, state: &AppState, chrome: &mut ViewportChrome) {
    if !state.camera_control.needs_an_exit() {
        return;
    }
    let label = text(state.locale, TextKey::CameraTrackballArmed);
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), FontId::proportional(FONT_XS), COLOR_TEXT);
    let padding = vec2(12.0, 6.0);
    let Some(badge) = chrome.claim(ChromeAnchor::BottomCentre, galley.size() + padding * 2.0)
    else {
        return;
    };
    ui.painter()
        .rect_filled(badge, CAPSULE_RADIUS, COLOR_SURFACE_RAISED);
    ui.painter().galley(badge.min + padding, galley, COLOR_TEXT);
}

fn paint_viewport_chrome(ui: &Ui, state: &AppState, rect: Rect, camera: TurntableCamera) {
    let mut chrome = ViewportChrome::new(rect);
    paint_orientation_hud(ui, &mut chrome, camera);
    paint_camera_mode_badge(ui, state, &mut chrome);
    publish_chrome_claims(ui, &chrome);
}

const CHROME_CLAIMS_ID: &str = "vkit.viewport.chrome.claims";

fn publish_chrome_claims(ui: &Ui, chrome: &ViewportChrome) {
    let claims: Vec<Rect> = chrome.claimed().collect();
    let frame = ui.ctx().cumulative_pass_nr();
    ui.data_mut(|data| {
        let id = Id::new(CHROME_CLAIMS_ID);
        let entry = data.get_temp_mut_or_default::<(u64, Vec<Rect>)>(id);

        if entry.0 != frame {
            *entry = (frame, Vec::new());
        }
        entry.1.extend(claims);
    });
}

pub(crate) fn viewport_chrome_covers(ui: &Ui, pointer: Pos2) -> bool {
    ui.data(|data| {
        data.get_temp::<(u64, Vec<Rect>)>(Id::new(CHROME_CLAIMS_ID))
            .is_some_and(|(_, claims)| claims.iter().any(|rect| rect.contains(pointer)))
    })
}

fn paint_orientation_hud(ui: &Ui, chrome: &mut ViewportChrome, camera: TurntableCamera) {
    let Some(hud) = chrome.claim(ChromeAnchor::TopRight, ORIENTATION_HUD_SIZE) else {
        return;
    };
    let origin = pos2(hud.center().x, hud.center().y + 7.0);

    let view = glam::camera::rh::view::look_at_mat4(camera.eye(), camera.target, glam::Vec3::Y);
    let mut arms = [
        (glam::Vec3::X, "X", COLOR_AXIS_X),
        (glam::Vec3::Y, "Y", COLOR_AXIS_Y),
        (glam::Vec3::Z, "Z", COLOR_AXIS_Z),
    ]
    .map(|(axis, label, color)| {
        let (offset, depth) = orientation_arm(view, axis);
        (offset, depth, label, color)
    });

    arms.sort_by(|left, right| left.1.total_cmp(&right.1));

    for (offset, depth, label, color) in arms {
        let fade = ORIENTATION_DEPTH_FADE
            + (1.0 - ORIENTATION_DEPTH_FADE) * depth.mul_add(0.5, 0.5).clamp(0.0, 1.0);
        let color = color.gamma_multiply(fade);
        let end = origin + offset;
        if offset.length() >= ORIENTATION_MIN_ARM {
            ui.painter()
                .line_segment([origin, end], Stroke::new(1.6, color));
        }
        ui.painter().circle_filled(end, 1.8, color);
        ui.painter().text(
            end + offset.normalized() * 5.0,
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(FONT_XS),
            color,
        );
    }
}

fn orientation_arm(view: glam::Mat4, axis: glam::Vec3) -> (Vec2, f32) {
    let direction = view.transform_vector3(axis).normalize_or_zero();
    (
        Vec2::new(direction.x, -direction.y) * ORIENTATION_AXIS_RADIUS,
        direction.z,
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum HelpScope {
    Tab(Tab),

    Projection,
}

fn help_scope(state: &AppState) -> HelpScope {
    if state.is_texturing()
        && state.texture_project.active_tool == crate::texture_project::TextureTool::Projection
    {
        return HelpScope::Projection;
    }
    HelpScope::Tab(state.active_tab)
}

fn viewport_help_rows(scope: HelpScope) -> &'static [(TextKey, TextKey)] {
    match scope {
        HelpScope::Tab(Tab::Alignment) => &ALIGN_HELP_ROWS,
        HelpScope::Tab(Tab::Edit) => &PIN_HELP_ROWS,
        HelpScope::Tab(Tab::Morph | Tab::Texture) => &DETAIL_HELP_ROWS,
        HelpScope::Tab(Tab::Result) => &SAVE_HELP_ROWS,
        HelpScope::Projection => &PROJECTION_HELP_ROWS,
    }
}

fn viewport_help_button_rect(viewport: Rect) -> Option<Rect> {
    crate::viewport_chrome::slot(viewport, ChromeAnchor::BottomLeft, Vec2::splat(28.0), 0)
}

const HELP_FONT: f32 = FONT_BODY;

const HELP_ROW_HEIGHT: f32 = 22.0;

fn viewport_help_column_widths(ui: &Ui, scope: HelpScope, locale: Locale) -> (f32, f32) {
    let measure = |key| {
        ui.painter()
            .layout_no_wrap(
                text(locale, key).to_owned(),
                FontId::proportional(HELP_FONT),
                Color32::WHITE,
            )
            .size()
            .x
    };
    let rows = viewport_help_rows(scope);
    let function_width = rows
        .iter()
        .map(|(function, _)| measure(*function))
        .fold(0.0, f32::max);
    let shortcut_width = rows
        .iter()
        .map(|(_, shortcut)| measure(*shortcut))
        .fold(0.0, f32::max);
    (function_width, shortcut_width)
}

fn viewport_help_popup_rect(
    ui: &Ui,
    viewport: Rect,
    scope: HelpScope,
    locale: Locale,
) -> Option<Rect> {
    let button = viewport_help_button_rect(viewport)?;
    let rows = viewport_help_rows(scope);
    let (function_width, shortcut_width) = viewport_help_column_widths(ui, scope, locale);
    let intrinsic_width = MINI_HELP_CONTENT_INSET_X * 2.0 + function_width + 24.0 + shortcut_width;
    let width = intrinsic_width
        .max(220.0)
        .min((viewport.width() - 32.0).max(0.0));
    let height = MINI_HELP_CONTENT_INSET_Y * 2.0 + rows.len() as f32 * HELP_ROW_HEIGHT;
    let rect = Rect::from_min_size(
        pos2(viewport.left() + 16.0, button.top() - 8.0 - height),
        vec2(width, height),
    );
    (width >= 220.0 && viewport.contains_rect(rect)).then_some(rect)
}

fn viewport_help_contains(ui: &Ui, state: &AppState, viewport: Rect, pointer: Pos2) -> bool {
    viewport_help_button_rect(viewport).is_some_and(|rect| rect.contains(pointer))
        || (state.help_visible
            && viewport_help_popup_rect(ui, viewport, help_scope(state), state.locale)
                .is_some_and(|rect| rect.contains(pointer)))
}

fn draw_viewport_help(ui: &mut Ui, state: &mut AppState, viewport: Rect, draw_button: bool) {
    let id = Id::new("vkit.viewport.help");
    if state.help_visible
        && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
        && ui.input(|input| input.pointer.any_click())
        && !viewport_help_contains(ui, state, viewport, pointer)
    {
        state.dispatch(Action::ToggleHelp);
    }
    if state.help_visible
        && let Some(panel_rect) =
            viewport_help_popup_rect(ui, viewport, help_scope(state), state.locale)
    {
        ui.painter().rect_filled(
            panel_rect,
            SMALL_RADIUS as f32,
            COLOR_TOPBAR.gamma_multiply(0.94),
        );
        let _blocker = ui.interact(
            panel_rect,
            id.with("panel-blocker"),
            Sense::click_and_drag(),
        );
        let painter = ui.painter().with_clip_rect(panel_rect);
        let (function_width, _) = viewport_help_column_widths(ui, help_scope(state), state.locale);
        let shortcut_x = panel_rect.left() + MINI_HELP_CONTENT_INSET_X + function_width + 24.0;
        for (index, (function, shortcut)) in viewport_help_rows(help_scope(state))
            .iter()
            .copied()
            .enumerate()
        {
            let y = panel_rect.top()
                + MINI_HELP_CONTENT_INSET_Y
                + HELP_ROW_HEIGHT * 0.5
                + index as f32 * HELP_ROW_HEIGHT;
            painter.text(
                pos2(panel_rect.left() + MINI_HELP_CONTENT_INSET_X, y),
                Align2::LEFT_CENTER,
                text(state.locale, function),
                FontId::proportional(HELP_FONT),
                COLOR_TEXT,
            );
            painter.text(
                pos2(shortcut_x, y),
                Align2::LEFT_CENTER,
                text(state.locale, shortcut),
                FontId::proportional(HELP_FONT),
                COLOR_MUTED,
            );
        }
    }

    if !draw_button {
        return;
    }
    let Some(button_rect) = viewport_help_button_rect(viewport) else {
        return;
    };
    let response = ui.interact(button_rect, id.with("button"), Sense::click());
    ui.painter().circle_filled(
        button_rect.center(),
        14.0,
        if state.help_visible || response.hovered() {
            crate::theme::hover_fill(COLOR_VIEWPORT_TOOL)
        } else {
            COLOR_VIEWPORT_TOOL
        },
    );
    ui.painter().text(
        button_rect.center(),
        Align2::CENTER_CENTER,
        "?",
        FontId::proportional(FONT_HEADING),
        COLOR_TEXT,
    );
    control_affordances(ui, &response, button_rect, button_rect.height() * 0.5);
    if response.clicked() {
        state.dispatch(Action::ToggleHelp);
    }
}

fn color_array(color: Color32, alpha: f32) -> [f32; 4] {
    [
        f32::from(color.r()) / 255.0,
        f32::from(color.g()) / 255.0,
        f32::from(color.b()) / 255.0,
        alpha,
    ]
}

#[cfg(test)]
const fn style_alpha(style: RenderStyle) -> f32 {
    match style {
        RenderStyle::Solid | RenderStyle::Wire => 1.0,
        RenderStyle::Xray => 0.28,
    }
}

#[cfg(test)]
mod tests;
