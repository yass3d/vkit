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
    camera_control::{CameraGesture, ControlMode, MiddleDragBinding},
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
    ui_components::{
        BRUSH_FALLOFF_COMPACT_WIDTH, BRUSH_STRENGTH_SENSITIVITY, FilledNumericSlider, Icon,
        MINI_HELP_CONTENT_INSET_X, MINI_HELP_CONTENT_INSET_Y, MINI_POPUP_CONTENT_INSET_X,
        MINI_POPUP_CONTENT_INSET_Y, TEXTURE_BRUSH_SIZE_SENSITIVITY, animate_rect,
        animated_segmented_group, brush_falloff_selector, brush_size_gesture_anchor,
        clear_brush_size_gesture, compact_color_picker, control_affordances, ease_in_out_cubic,
        handle_brush_size_gesture, paint_icon, paint_texture_pin, segment_button, switch,
        transform_group_editability_icon,
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

mod alignment_gizmo;
mod backdrop;
mod camera_input;
pub(crate) mod hair_overlays;
mod hair_vertex;
mod help;
mod islands;
mod pins;
mod reference_overlay;
mod reference_panel;
mod render_callbacks;

pub(crate) use alignment_gizmo::*;
use backdrop::*;
use camera_input::*;
use hair_overlays::*;
use help::*;
use islands::*;
use pins::*;
use render_callbacks::*;

const SCAN_SCENE_KEY: u64 = 0x5343_414e;
const TEMPLATE_SCENE_KEY: u64 = 0x4732_544d;
const ALIGNMENT_SCAN_SCENE_KEY: u64 = 0x414c_5343;
const ALIGNMENT_TEMPLATE_SCENE_KEY: u64 = 0x414c_4732;
const RESULT_SCENE_KEY: u64 = 0x5253_4c54;
pub(crate) const RESULT_SKIN_SCENE_KEY: u64 = 0x5253_4b4e;

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
const HAIR_AUTHOR_SCALP_SCENE_KEY: u64 = 0x4841_5343;
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

const GIZMO_DRAG_ID: &str = "vkit.viewport.alignment.gizmo.drag";
const SCULPT_DRAG_ID: &str = "vkit.viewport.sculpt.stroke";
const TEXTURE_BRUSH_UV_SCALE_ID: &str = "vkit.viewport.texture.points_per_uv";
const SCULPT_BRUSH_SIZE_SENSITIVITY: f32 = 0.75;
const SCULPT_DAB_SPACING_RADIUS_FRACTION: f32 = 0.2;
const MAX_SCULPT_INTERPOLATION_STEPS: usize = 4096;
const SCULPT_SMOOTH_DABS_PER_SECOND: f64 = 30.0;
const SCULPT_SMOOTH_DAB_INTERVAL_SECONDS: f64 = 1.0 / SCULPT_SMOOTH_DABS_PER_SECOND;
const MAX_SCULPT_FRAME_DELTA_SECONDS: f64 = 0.1;
const MAX_SCULPT_SMOOTH_DABS_PER_FRAME: usize = 8;
const IMPORT_FOCUS_RELEASE_DURATION: Duration = Duration::from_millis(460);
const EYE_GAZE_GRID_SIZE: f32 = 124.0;
const EYE_GAZE_POPUP_ID: &str = "vkit.viewport.eye-gaze-popup";

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

/// Which handles a gizmo offers, and therefore which ones it draws.
///
/// One flag read by the painter and by the hit test alike, so a handle that is
/// not drawn cannot be grabbed and a handle that is drawn always can.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GizmoHandles {
    rotate: bool,
    scale: bool,
}

impl GizmoHandles {
    /// Move, rotate and scale: what fitting one mesh to another needs.
    const ALL: Self = Self {
        rotate: true,
        scale: true,
    };

    /// Move only. A strand joint has no orientation to turn and no spacing to
    /// stretch — the ring and the centre grip would be two controls that do
    /// nothing anybody asked for.
    const MOVE_ONLY: Self = Self {
        rotate: false,
        scale: false,
    };
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
    mask_step_open: bool,
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

    Mask,
}

impl SculptInputMode {
    const fn is_paint_style(self) -> bool {
        matches!(
            self,
            Self::Smooth | Self::Restore | Self::RestoreFit | Self::Mask
        )
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

pub fn draw_edit(ui: &mut Ui, state: &mut AppState, scan_rect: Rect, template_rect: Rect) {
    crate::sweep_gesture::set_commit_style(ui, state.brush_sweep_commit);
    let workspace_rect = scan_rect.union(template_rect);

    let hovered = if ui
        .input(|input| input.pointer.interact_pos())
        .is_some_and(|pointer| template_rect.contains(pointer))
    {
        MeshSide::Template
    } else {
        MeshSide::Scan
    };
    let side = roll_sweep_side(
        crate::sweep_gesture::sweep_active(ui, Id::new(ROLL_SWEEP_ID)),
        ui.data(|data| data.get_temp::<MeshSide>(Id::new(ROLL_SWEEP_SIDE_ID))),
        hovered,
    );
    let mut swept = match side {
        MeshSide::Scan => state.workspace.scan_camera,
        MeshSide::Template => state.workspace.template_camera,
    };
    let roll_owns_pointer =
        handle_camera_control_shortcuts(ui, state, workspace_rect, &mut swept, ViewPane::Primary);
    commit_swept_edit_camera(state, side, swept, roll_owns_pointer);
    if crate::sweep_gesture::sweep_active(ui, Id::new(ROLL_SWEEP_ID)) {
        ui.data_mut(|data| data.insert_temp(Id::new(ROLL_SWEEP_SIDE_ID), side));
    } else {
        ui.data_mut(|data| data.remove::<MeshSide>(Id::new(ROLL_SWEEP_SIDE_ID)));
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
    work_nav::draw_work_nav(ui, state, workspace_rect);
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ViewPane {
    #[default]
    Primary,
    Split,
}

impl ViewPane {
    const fn interaction_id(self) -> &'static str {
        match self {
            Self::Primary => "vkit.viewport.result",
            Self::Split => "vkit.viewport.result.split",
        }
    }
}

pub fn draw_result_pane(
    ui: &mut Ui,
    state: &mut AppState,
    rect: Rect,
    title: &str,
    pane: ViewPane,
    chrome: Rect,
) {
    let camera_keys = state.active_view_pane == pane;
    if pane == ViewPane::Primary {
        draw_result_in(ui, state, rect, title, pane, camera_keys, Some(chrome));
        return;
    }
    std::mem::swap(
        &mut state.workspace.result_camera,
        &mut state.workspace.result_camera_split,
    );
    ui.data_mut(|data| data.insert_temp(Id::new(SCENE_SALT_ID), SPLIT_PANE_SCENE_SALT));
    draw_result_in(ui, state, rect, title, pane, camera_keys, Some(chrome));
    ui.data_mut(|data| data.remove::<u64>(Id::new(SCENE_SALT_ID)));
    std::mem::swap(
        &mut state.workspace.result_camera,
        &mut state.workspace.result_camera_split,
    );
}

pub fn draw_result(ui: &mut Ui, state: &mut AppState, rect: Rect, title: &str) {
    draw_result_in(ui, state, rect, title, ViewPane::Primary, true, None);
}

fn draw_result_in(
    ui: &mut Ui,
    state: &mut AppState,
    rect: Rect,
    _title: &str,
    pane: ViewPane,
    camera_keys: bool,
    chrome: Option<Rect>,
) {
    crate::sweep_gesture::set_commit_style(ui, state.brush_sweep_commit);
    paint_viewport_background(ui, state, rect);
    reference_overlay::note_reference_aspects(ui, state);
    reference_overlay::paint_reference_images(ui, state, rect);
    let response = ui.interact(
        rect,
        Id::new(pane.interaction_id()),
        Sense::click_and_drag(),
    );
    let mut swept = state.workspace.result_camera;
    let roll_owns_pointer =
        camera_keys && handle_camera_control_shortcuts(ui, state, rect, &mut swept, pane);
    state.workspace.result_camera = swept;
    let pointer_taken = state.import_progress.is_some()
        || roll_owns_pointer
        || camera_mode_owns_pointer(state)
        || crate::sweep_gesture::press_spent(ui)
        || viewport_tools_pointer_blocked(ui, state, chrome.unwrap_or(rect));
    crate::sweep_gesture::settle_press(ui);

    // Asked before the camera and the brushes, and answered only for a press
    // that missed the model: a reference is behind the head, so it takes a
    // click nothing in front of it wanted, and nothing else.
    let pointer_position = ui.input(|input| input.pointer.interact_pos());
    let model_under_pointer = {
        let camera = state.workspace.result_camera;
        let scan_pose = scan_transform(state);
        let scan_overlay =
            result_scan_overlay_alpha(state).and_then(|_| state.workspace.scan.clone());
        let result = state.workspace.result.clone();
        move || {
            let Some(pointer) = pointer_position else {
                return false;
            };
            let Some(ray) = camera.ray_from_screen(pointer, rect) else {
                return false;
            };
            nearest_visible_world_hit(
                ray,
                &[
                    (result.as_deref(), ModelTransform::default()),
                    (scan_overlay.as_deref(), scan_pose),
                ],
            )
            .is_some()
        }
    };
    let reference_owns_pointer = !pointer_taken
        && reference_overlay::handle_reference_drag(ui, state, rect, model_under_pointer);

    let pointer_taken = pointer_taken || reference_owns_pointer;
    let camera_blocked = pointer_taken || brush_sweep_owns_pointer(ui);
    let input_blocked = pointer_taken;
    if !camera_blocked {
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
            &pick,
            projection_stencil_mode(state),
        ) {
            state.workspace.result_camera = moved;
            ui.ctx().request_repaint();
        }

        if !state.is_detail_editing()
            && !state.is_hair_editing()
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
        clear_brush_size_gesture(
            ui.ctx(),
            crate::ui_components::BrushSweeps::TEXTURE_SURFACE.size(),
        );
        clear_brush_size_gesture(
            ui.ctx(),
            crate::ui_components::BrushSweeps::TEXTURE_SURFACE.strength(),
        );
    }
    clear_stale_detail_pointer_state(ui, state);

    if projection_stencil_mode(state) {
        handle_projection_stencil(ui, state, rect, &response, camera, input_blocked);
    } else if texture_target_pin_mode(state) {
        handle_texture_target_pin_interaction(ui, state, rect, &response, camera, input_blocked);
    } else if texture_paint_mode(state) {
        handle_texture_paint_interaction(ui, state, rect, &response, camera, input_blocked);
    } else if state.is_sculpting() {
        handle_sculpt_interaction(ui, state, rect, &response, camera, input_blocked, pane);
    } else if state.is_hair_editing() {
        hair_input::handle_hair_interaction(
            ui,
            state,
            rect,
            &response,
            camera,
            input_blocked || state.hair_thumbnail.is_some(),
            pane,
        );
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
    } else if !hair_authoring_visible(state) && !state.busy() {
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

    if state.shows_authored_hair() {
        let framing = state.hair_thumbnail.is_some();
        add_hair_authoring_overlays(ui, rect, state, camera, state.is_hair_editing() && !framing);
    }
    draw_template_install_fade(ui, state, rect);

    if state.is_sculpting() {
        paint_sculpt_brush_hud(ui, state, &response);
    }

    if state.is_hair_editing() && state.hair_thumbnail.is_none() {
        if state.hair_project.active_tool == crate::hair_project::HairTool::Vertex {
            hair_vertex::paint(ui, state, rect, &response, swept);
            vertex_gizmo::paint(ui, state, rect, swept);
        } else {
            hair_input::paint_hair_brush_hud(ui, state, &response);
        }
    }

    if projection_stencil_mode(state) {
        if let Some(stencil) = crate::texture_ui::projection_stencil_rect(state, rect) {
            crate::texture_ui::paint_projection_stencil(ui, state, stencil);
            paint_stencil_brush_cursor(ui, state, &response, stencil);
        }
    } else if texture_target_pin_mode(state) {
        paint_texture_target_pins(ui, state, rect, camera);
    }
    if texture_paint_mode(state) {
        paint_texture_brush_cursor(ui, state, &response, rect, camera);
        paint_clone_anchor_on_surface(ui, state, rect, camera);
    }
    paint_viewport_chrome(ui, state, rect, camera);
    paint_vignette(ui, state, rect);
    if chrome.is_none() {
        draw_result_floating_chrome(ui, state, rect);
    }
}

pub fn draw_result_floating_chrome(ui: &mut Ui, state: &mut AppState, rect: Rect) {
    if state.active_tab == Tab::Result {
        draw_result_view_island(ui, state, rect);
    }
    draw_viewport_help(ui, state, rect, true);
    if state.is_detail_editing() {
        draw_detail_viewport_controls(ui, state, rect);
    }
    work_nav::draw_work_nav(ui, state, rect);
    if projection_stencil_mode(state) {
        draw_projection_stencil_island(ui, state, rect);
    }
    if state.is_hair_editing() {
        super::viewport::hair_hud::draw_hair_toolbox(ui, state, rect);
    }
    sculpt_toolbox::draw_sculpt_toolbox(ui, state, rect);
    texture_toolbox::draw_texture_toolbox(ui, state, rect);
    if state.shows_authored_hair() {
        if state.hair_thumbnail.is_some() {
            draw_thumbnail_frame(ui, state, rect);
        }
        draw_thumbnail_shot_pulse(ui, state, rect);
    }
    draw_viewport_tools(ui, state, rect, "result");
    draw_import_focus_overlay(ui, state, rect);
}

pub(super) fn stroke_pane_gate(ui: &Ui, key: &'static str, pane: ViewPane) -> bool {
    ui.data(|data| data.get_temp::<ViewPane>(Id::new((key, "stroke-owner"))))
        .is_none_or(|owner| owner == pane)
}

pub(super) fn claim_stroke_pane(ui: &Ui, key: &'static str, pane: ViewPane) {
    ui.data_mut(|data| data.insert_temp(Id::new((key, "stroke-owner")), pane));
}

pub(super) fn release_stroke_pane(ui: &Ui, key: &'static str) {
    ui.data_mut(|data| data.remove::<ViewPane>(Id::new((key, "stroke-owner"))));
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
        || crate::sweep_gesture::press_spent(ui)
        || viewport_tools_pointer_blocked(ui, state, workspace_rect);
    crate::sweep_gesture::settle_press(ui);
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
        if let Some(moved) = viewport_camera_motion(ui, &response, rect, side_camera, &pick, false)
        {
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

mod panels;

pub(crate) use panels::panel_list_budget;
use panels::*;

mod detail_hud;

use detail_hud::*;
pub(crate) use detail_hud::{detail_header_band_bottom, draw_detail_workspace};

fn sculpt_brush_icon(brush: SculptBrush) -> Icon {
    match brush {
        SculptBrush::Move => Icon::BrushMove,
        SculptBrush::Smooth => Icon::BrushSmooth,
        SculptBrush::Restore => Icon::BrushRestore,
        SculptBrush::Mask => Icon::Eraser,
    }
}

fn sculpt_brush_text_key(brush: SculptBrush) -> TextKey {
    match brush {
        SculptBrush::Move => TextKey::SculptBrushMove,
        SculptBrush::Smooth => TextKey::SculptBrushSmooth,
        SculptBrush::Restore => TextKey::SculptBrushRestore,
        SculptBrush::Mask => TextKey::SculptBrushMask,
    }
}

fn sculpt_brush_tooltip_key(brush: SculptBrush) -> TextKey {
    match brush {
        SculptBrush::Move => TextKey::SculptBrushMoveTooltip,
        SculptBrush::Smooth => TextKey::SculptBrushSmoothTooltip,
        SculptBrush::Restore => TextKey::SculptBrushRestoreTooltip,
        SculptBrush::Mask => TextKey::SculptBrushMaskTooltip,
    }
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

mod hair_hud;
mod hair_input;
pub(crate) use hair_hud::draw_hair_workspace;
pub(crate) use hair_input::clear_hair_pointer_state;
mod sculpt_input;
mod sculpt_toolbox;
mod texture_toolbox;
mod toolbox;
mod vertex_gizmo;
mod work_nav;

pub(crate) use sculpt_input::clear_sculpt_pointer_stroke;
use sculpt_input::*;

#[derive(Clone, Copy, Debug)]
struct SceneView {
    rect: Rect,
    camera: TurntableCamera,
    transform: ModelTransform,
}

#[derive(Clone, Copy, Debug)]
struct MeshDraw {
    color: [f32; 4],
    style: RenderStyle,
    depth_scope: RenderDepthScope,
}

#[derive(Clone, Copy, Debug)]
struct SkinDraw {
    visibility: SkinVisibilityGroups,
    show_tear_lacrimals: bool,
    show_eyelashes: bool,
    depth_scope: RenderDepthScope,
}

pub(crate) const HAIR_SCALP_SCENE_KEY: u64 = 0x5343_4C50_0000_0000;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RenderPassPlan {
    solid: bool,
    textured: bool,
    wire: bool,
    xray: bool,
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

pub(crate) const HAIR_AUTHORING_SCENE_KEY: u64 = 0x4841_4952_0000_0000;

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

pub(crate) use hair_overlays::authoring_hair_preview as hair_portrait_preview;
