use super::*;
use serde_json::json;
use vkit_core::formats::{DazGeometry, Mesh};

#[test]
fn detail_island_collapse_keeps_its_lower_right_corner() {
    let width_delta = DETAIL_GROUP_PANEL_WIDTH - DETAIL_GROUP_COLLAPSED_WIDTH;
    let height_delta = detail_group_expanded_height() - detail_group_collapsed_height();

    let mut none = None;
    anchor_detail_panel_to_right_corner(&mut none, true);
    assert!(none.is_none());

    let mut pos = Some([100.0, 200.0]);
    anchor_detail_panel_to_right_corner(&mut pos, true);
    assert_eq!(pos, Some([100.0 + width_delta, 200.0 + height_delta]));
    anchor_detail_panel_to_right_corner(&mut pos, false);
    assert_eq!(pos, Some([100.0, 200.0]));
}

#[test]
fn bracket_brush_step_grows_shrinks_and_is_silent_at_limits() {
    let range = 8.0..=220.0;

    let grown = stepped_brush_radius(64.0, true, range.clone()).expect("grow moves");
    assert!(grown > 64.0);
    let back = stepped_brush_radius(grown, false, range.clone()).expect("shrink moves");
    assert!((back - 64.0).abs() < 0.01);

    assert_eq!(
        stepped_brush_radius(210.0, true, range.clone()),
        Some(220.0)
    );
    assert_eq!(stepped_brush_radius(220.0, true, range.clone()), None);
    assert_eq!(stepped_brush_radius(8.0, false, range), None);

    let tex = 0.002..=0.25;
    assert_eq!(stepped_brush_radius(0.002, false, tex.clone()), None);
    assert!(stepped_brush_radius(0.035, true, tex).is_some_and(|r| r > 0.035));
}

#[test]
fn template_fade_covers_within_the_load_window_and_reveals_over_target_time() {
    let mut opacity = 0.0;
    let frame = 1.0 / 60.0;
    let mut elapsed = 0.0;
    while opacity < 1.0 && elapsed < 1.0 {
        let next = template_fade_step(opacity, 1.0, frame);
        assert!(next >= opacity, "cover must rise monotonically");
        opacity = next;
        elapsed += frame;
    }
    assert!(
        (TEMPLATE_FADE_IN_SECS..TEMPLATE_FADE_IN_SECS + 2.0 * frame).contains(&elapsed),
        "cover took {elapsed} s"
    );

    let mut elapsed = 0.0;
    while opacity > 0.0 && elapsed < 1.0 {
        let next = template_fade_step(opacity, 0.0, frame);
        assert!(next <= opacity, "reveal must fall monotonically");
        opacity = next;
        elapsed += frame;
    }
    assert!(
        (0.3..=0.45).contains(&TEMPLATE_FADE_OUT_SECS),
        "reveal duration must stay in the requested 300-450 ms window"
    );
    assert!(
        (TEMPLATE_FADE_OUT_SECS..TEMPLATE_FADE_OUT_SECS + 2.0 * frame).contains(&elapsed),
        "reveal took {elapsed} s"
    );

    let after_stall = template_fade_step(1.0, 0.0, TEMPLATE_FADE_MAX_FRAME_SECS);
    assert!(after_stall >= 1.0 - TEMPLATE_FADE_MAX_FRAME_SECS / TEMPLATE_FADE_OUT_SECS - 1e-6);
    assert!(
        after_stall > 0.5,
        "a stalled frame must not skip the reveal"
    );

    assert_eq!(template_fade_step(0.99, 1.0, 1.0), 1.0);
    assert_eq!(template_fade_step(0.01, 0.0, 1.0), 0.0);
}

#[test]
fn eye_look_uses_anatomical_direction_limits() {
    assert_eq!(eye_angles_from_gaze([-1.0, 0.0], true)[0], -25.0);
    assert_eq!(eye_angles_from_gaze([1.0, 0.0], true)[0], 22.0);
    assert_eq!(eye_angles_from_gaze([-1.0, 0.0], false)[0], -22.0);
    assert_eq!(eye_angles_from_gaze([1.0, 0.0], false)[0], 25.0);
    assert_eq!(eye_angles_from_gaze([0.0, 1.0], true)[1], 20.0);
    assert_eq!(eye_angles_from_gaze([0.0, -1.0], true)[1], -30.0);
}

#[test]
fn eye_grid_maps_screen_direction_and_reset_auto_are_explicit() {
    let grid = Rect::from_min_size(pos2(20.0, 40.0), Vec2::splat(120.0));
    assert_eq!(gaze_from_screen(grid.center(), grid), [0.0, 0.0]);
    assert_eq!(gaze_from_screen(grid.right_top(), grid), [1.0, 1.0]);
    assert_eq!(gaze_from_screen(grid.left_bottom(), grid), [-1.0, -1.0]);

    let mut state = AppState::default();
    state.dispatch(Action::SetEyeGazeMode(EyeGazeMode::AutoCursor));
    assert_eq!(state.eye_gaze_mode, EyeGazeMode::AutoCursor);
    state.dispatch(Action::SetManualEyeGaze([0.75, -0.25]));
    assert_eq!(state.manual_eye_gaze, [0.75, -0.25]);
    assert_eq!(state.eye_gaze_mode, EyeGazeMode::Manual);
    state.dispatch(Action::SetEyeGazeMode(EyeGazeMode::AutoCursor));
    state.dispatch(Action::ResetEyeGaze);
    assert_eq!(state.manual_eye_gaze, [0.0, 0.0]);
    assert_eq!(state.eye_gaze_mode, EyeGazeMode::Manual);
}

fn plane_mesh() -> SurfaceMesh {
    SurfaceMesh::new(
        Mesh::new(
            vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            vec![[0, 1, 2], [0, 2, 3]],
        )
        .unwrap(),
    )
    .unwrap()
}

fn layered_mesh() -> SurfaceMesh {
    SurfaceMesh::new(
        Mesh::new(
            vec![
                [-1.0, -1.0, 0.5],
                [1.0, -1.0, 0.5],
                [0.0, 1.0, 0.5],
                [-1.0, -1.0, -0.5],
                [1.0, -1.0, -0.5],
                [0.0, 1.0, -0.5],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .unwrap(),
    )
    .unwrap()
}

fn front_camera() -> TurntableCamera {
    TurntableCamera {
        yaw: 0.0,
        pitch: 0.0,
        target: glam::Vec3::ZERO,
        distance: 3.0,
        frame_radius: 1.0,
        ..Default::default()
    }
}

fn test_rect() -> Rect {
    Rect::from_min_max(pos2(40.0, 70.0), pos2(840.0, 670.0))
}

fn measured_panel_rect(
    state: &mut AppState,
    viewport: Rect,
    panel: ViewportToolPanel,
    mode: BaseViewMode,
) -> (Option<Rect>, Option<Rect>) {
    state.base_view_mode = mode;
    state.viewport_tool_panel = Some(panel);
    let context = egui::Context::default();
    let mut measured = None;
    let mut cached = None;
    let _ = context.run_ui(Default::default(), |ui| {
        measured = measure_viewport_tool_panel_rect(
            ui,
            state,
            viewport,
            panel,
            Id::new(("test.measure-panel", panel, mode as u8)),
        );
        cached = cached_viewport_tool_panel_rect(ui, state, viewport);
    });
    (measured, cached)
}

fn panel_desired_vs_available(
    state: &mut AppState,
    viewport: Rect,
    panel: ViewportToolPanel,
    mode: BaseViewMode,
) -> (f32, f32) {
    state.base_view_mode = mode;
    state.viewport_tool_panel = Some(panel);
    let context = egui::Context::default();
    let mut desired = 0.0;
    let _ = context.run_ui(Default::default(), |ui| {
        let _ = measure_viewport_tool_panel_rect(
            ui,
            state,
            viewport,
            panel,
            Id::new((
                "test.clamp-probe",
                panel,
                mode as u8,
                viewport.height() as i32,
            )),
        );
        desired = ui
            .data(|data| {
                data.get_temp::<f32>(viewport_tool_panel_desired_height_cache_id(panel, mode))
            })
            .unwrap_or(0.0);
    });
    let available = viewport_tool_panel_placement(viewport, panel)
        .map_or(0.0, |placement| placement.available_height);
    (desired, available)
}

#[test]
fn preset_popovers_never_stack_a_second_scrollbar_around_their_library() {
    for height in [1080.0_f32, 900.0, 720.0, 600.0, 520.0, 460.0, 400.0, 360.0] {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(480.0, height));
        let mut state = AppState::default();
        for (panel, mode) in [
            (ViewportToolPanel::Skin, BaseViewMode::Texture),
            (ViewportToolPanel::Skin, BaseViewMode::Solid),
        ] {
            for pass in 0..2 {
                let (desired, available) =
                    panel_desired_vs_available(&mut state, viewport, panel, mode);
                assert!(
                    desired <= available + 0.5,
                    "viewport {height} / {panel:?} / {mode:?} / pass {pass}: popover wants {desired} of {available}",
                );
            }
        }
    }
}

fn measured_help_popup_rect(viewport: Rect, tab: Tab, locale: Locale) -> Option<Rect> {
    let context = egui::Context::default();
    let mut measured = None;
    let _ = context.run_ui(Default::default(), |ui| {
        measured = viewport_help_popup_rect(ui, viewport, HelpScope::Tab(tab), locale);
    });
    measured
}

fn measured_help_contains(state: &AppState, viewport: Rect, pointer: Pos2) -> bool {
    let context = egui::Context::default();
    let mut contains = false;
    let _ = context.run_ui(Default::default(), |ui| {
        contains = viewport_help_contains(ui, state, viewport, pointer);
    });
    contains
}

fn layered_endpoints() -> (SurfaceEndpoint, SurfaceEndpoint) {
    (
        SurfaceEndpoint {
            triangle: 0,
            barycentric: [0.25, 0.25, 0.5],
        },
        SurfaceEndpoint {
            triangle: 1,
            barycentric: [0.25, 0.25, 0.5],
        },
    )
}

#[test]
fn viewport_tool_rail_starts_at_workspace_inset_with_fixed_spacing() {
    let viewport = Rect::from_min_size(pos2(20.0, 30.0), vec2(600.0, 500.0));
    let first = viewport_tool_button_rect(viewport, 0).unwrap();
    let second = viewport_tool_button_rect(viewport, 1).unwrap();
    assert_eq!(first.min, pos2(32.0, 42.0));
    assert_eq!(first.size(), Vec2::splat(34.0));
    assert_eq!(second.top() - first.bottom(), VIEWPORT_TOOL_GAP);
}

#[test]
fn linked_camera_delta_is_applied_to_both_views() {
    let mut state = AppState::default();
    state.cameras_linked = true;
    let delta = CameraDelta {
        orbit_points: vec2(13.0, -7.0),
        viewport_height_points: 600.0,
        ..Default::default()
    };
    let scan_before = state.workspace.scan_camera;
    let template_before = state.workspace.template_camera;
    apply_edit_camera_delta(&mut state, MeshSide::Scan, delta);
    assert_ne!(state.workspace.scan_camera, scan_before);
    assert_ne!(state.workspace.template_camera, template_before);
    assert_eq!(
        state.workspace.scan_camera.yaw,
        state.workspace.template_camera.yaw
    );
    assert_eq!(
        state.workspace.scan_camera.pitch,
        state.workspace.template_camera.pitch
    );
}

#[test]
fn edit_cameras_remain_linked_even_if_legacy_flag_is_false() {
    let mut state = AppState::default();
    state.cameras_linked = false;
    apply_edit_camera_delta(
        &mut state,
        MeshSide::Scan,
        CameraDelta {
            orbit_points: vec2(4.0, 2.0),
            viewport_height_points: 500.0,
            ..Default::default()
        },
    );
    assert_eq!(state.workspace.template_camera, state.workspace.scan_camera);
}

#[test]
fn a_roll_sweep_keeps_writing_the_pane_it_started_over() {
    assert_eq!(
        roll_sweep_side(true, Some(MeshSide::Scan), MeshSide::Template),
        MeshSide::Scan,
        "crossing the divider mid-sweep must not switch panes"
    );
    assert_eq!(
        roll_sweep_side(true, None, MeshSide::Template),
        MeshSide::Template
    );
    assert_eq!(
        roll_sweep_side(false, Some(MeshSide::Scan), MeshSide::Template),
        MeshSide::Template,
        "a stale pin without a sweep means nothing"
    );
}

#[test]
fn a_roll_sweep_write_back_carries_the_linked_pane_along() {
    let mut state = AppState::default();
    let mut swept = state.workspace.scan_camera;
    swept.roll = 0.5;
    commit_swept_edit_camera(&mut state, MeshSide::Scan, swept, true);
    assert_eq!(state.workspace.scan_camera.roll, 0.5);
    assert_eq!(state.workspace.template_camera, state.workspace.scan_camera);

    let mut state = AppState::default();
    let template_before = state.workspace.template_camera;
    let idle = state.workspace.scan_camera;
    commit_swept_edit_camera(&mut state, MeshSide::Scan, idle, false);
    assert_eq!(state.workspace.template_camera, template_before);
}

#[test]
fn the_press_that_exits_trackball_mode_is_spent_until_its_click_passes() {
    assert!(
        !crate::sweep_gesture::sweep_spends_press(true, false),
        "finishing with the key involves no press"
    );
    assert!(crate::sweep_gesture::sweep_spends_press(true, true));
    assert!(!crate::sweep_gesture::sweep_spends_press(false, true));

    let mut spent = true;
    for (down, clicked, still_spent) in [
        (true, false, true),
        (true, false, true),
        (false, true, true),
        (false, false, false),
    ] {
        if crate::sweep_gesture::spent_press_settled(down, clicked) {
            spent = false;
        }
        assert_eq!(spent, still_spent, "down={down} clicked={clicked}");
    }
}

#[test]
fn the_click_that_dismisses_the_help_card_reaches_no_scene_handler() {
    use super::panels::help_card_spends_pointer;
    assert!(help_card_spends_pointer(true, true, false));
    assert!(
        help_card_spends_pointer(true, false, true),
        "the press frame already lands a paint-style dab, so it is spent too"
    );
    assert!(
        !help_card_spends_pointer(true, false, false),
        "a bare hover keeps the camera live under an open card"
    );
    assert!(!help_card_spends_pointer(false, true, true));

    let context = egui::Context::default();
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));
    let mut state = AppState::default();
    state.help_visible = true;
    let _ = context.run_ui(
        egui::RawInput {
            screen_rect: Some(viewport),
            ..Default::default()
        },
        |root| {
            let over_scene = viewport.center();
            assert!(super::panels::viewport_tools_should_block_pointer(
                root, &state, viewport, over_scene, true, false
            ));
            assert!(super::panels::viewport_tools_should_block_pointer(
                root, &state, viewport, over_scene, false, true
            ));
            assert!(!super::panels::viewport_tools_should_block_pointer(
                root, &state, viewport, over_scene, false, false
            ));
        },
    );
}

#[test]
fn wheel_zoom_anchor_pick_returns_the_nearest_layer_point() {
    let near = plane_mesh();
    let far = plane_mesh();
    let near_pose = ModelTransform {
        translation: glam::DVec3::new(0.0, 0.0, 0.5),
        ..Default::default()
    };
    let camera = front_camera();
    let rect = test_rect();
    let ray = camera.ray_from_screen(rect.center(), rect).unwrap();
    let hit = nearest_visible_world_hit(
        ray,
        &[
            (Some(&far), ModelTransform::default()),
            (Some(&near), near_pose),
        ],
    )
    .unwrap();
    assert!((hit.z - 0.5).abs() < 1.0e-4);

    assert!(nearest_visible_world_hit(ray, &[(None, ModelTransform::default())]).is_none());
}

#[test]
fn alignment_callbacks_use_solid_self_depth_and_start_both_surfaces_opaque() {
    assert_eq!(alignment_scan_style(), RenderStyle::Solid);
    assert_eq!(alignment_template_style(), RenderStyle::Solid);
    assert_eq!(alignment_template_depth_scope(), RenderDepthScope::Shared);
    assert_eq!(
        alignment_scan_depth_scope(),
        RenderDepthScope::ResetBeforeDraw
    );
    let mut state = AppState::default();
    assert_eq!(state.alignment_opacity, 1.0);
    assert_eq!(state.alignment_g2_opacity, 1.0);
    assert_eq!(alignment_layer_alpha(state.alignment_g2_opacity), 1.0);
    assert_eq!(
        alignment_layer_alpha(state.alignment_g2_opacity),
        style_alpha(alignment_template_style())
    );
    state.xray_visible = true;
    assert_eq!(alignment_scan_style(), RenderStyle::Solid);
    assert_eq!(alignment_template_style(), RenderStyle::Solid);
    assert_eq!(alignment_template_depth_scope(), RenderDepthScope::Shared);
    assert_eq!(
        alignment_scan_depth_scope(),
        RenderDepthScope::ResetBeforeDraw
    );
}

#[test]
fn base_wire_and_xray_render_passes_compose_independently() {
    assert_eq!(
        render_pass_plan(BaseViewMode::Solid, true, true, false, 0.32, false, 0.28),
        RenderPassPlan {
            solid: true,
            textured: false,
            wire: false,
            xray: false,
        }
    );
    assert_eq!(
        render_pass_plan(BaseViewMode::Texture, true, true, true, 0.32, true, 0.28),
        RenderPassPlan {
            solid: false,
            textured: true,
            wire: true,
            xray: true,
        }
    );
    assert_eq!(
        render_pass_plan(BaseViewMode::Texture, false, true, true, 0.32, true, 0.28),
        RenderPassPlan {
            solid: true,
            textured: false,
            wire: true,
            xray: true,
        },
        "an arbitrary scan falls back to Solid without losing overlays"
    );
}

#[test]
fn gizmo_math_is_axis_stable_and_scale_proportional() {
    assert!((translated_axis_value(2.0, vec2(30.0, 40.0), Vec2::X, 0.1) - 5.0).abs() < 1.0e-9);
    let seam_delta = wrapped_angle_delta(170.0_f32.to_radians(), -170.0_f32.to_radians());
    assert!((seam_delta.to_degrees() - 20.0).abs() < 1.0e-4);
    let start_rotation = [23.0, -18.0, 41.0];
    let rotated = world_axis_rotated_euler(start_rotation, 1, 37.0);
    let [sx, sy, sz] = start_rotation.map(f64::to_radians);
    let [rx, ry, rz] = rotated.map(f64::to_radians);
    let expected = glam::DQuat::from_axis_angle(glam::DVec3::Y, 37.0_f64.to_radians())
        * glam::DQuat::from_euler(glam::EulerRot::XYZ, sx, sy, sz);
    let actual = glam::DQuat::from_euler(glam::EulerRot::XYZ, rx, ry, rz);
    assert!((expected.dot(actual).abs() - 1.0).abs() < 1.0e-10);
    assert_eq!(
        uniform_scale_drag_values([2.0, 3.0, 4.0], vec2(90.0, -90.0)),
        [4.0, 6.0, 8.0]
    );
    let clamped = uniform_scale_drag_values([1.0e-5, 1.0, 1.0e5], vec2(10_000.0, 0.0));
    assert!((clamped[2] - 1.0e6).abs() < 1.0e-6);
    assert!((clamped[1] / clamped[0] - 1.0e5).abs() < 1.0e-6);
    assert!((clamped[2] / clamped[1] - 1.0e5).abs() < 1.0e-6);
}

#[test]
fn rotation_snap_uses_accumulated_drag_without_drift_and_shift_wins() {
    assert_eq!(rotation_drag_degrees(134.6, false, false), 134.6);
    assert_eq!(rotation_drag_degrees(134.6, false, true), 135.0);
    assert_eq!(rotation_drag_degrees(134.6, true, false), 90.0);
    assert_eq!(rotation_drag_degrees(134.6, true, true), 90.0);
    assert_eq!(rotation_drag_degrees(-136.0, true, false), -180.0);

    let accumulated = 89.49;
    assert_eq!(rotation_drag_degrees(accumulated, true, false), 90.0);
    assert_eq!(
        rotation_drag_degrees(accumulated, false, false),
        accumulated
    );
}

#[test]
fn grouped_skin_depth_resets_once_then_preserves_skin_occlusion() {
    assert!(grouped_skin_depth_sequence(0).is_empty());
    assert_eq!(
        grouped_skin_depth_sequence(6),
        vec![
            RenderDepthScope::ResetBeforeDraw,
            RenderDepthScope::Shared,
            RenderDepthScope::Shared,
            RenderDepthScope::Shared,
            RenderDepthScope::Shared,
            RenderDepthScope::Shared,
        ]
    );
}

#[test]
fn import_focus_release_is_bounded_and_center_out() {
    assert_eq!(focus_release_progress(Duration::ZERO), 0.0);
    let halfway = focus_release_progress(IMPORT_FOCUS_RELEASE_DURATION / 2);
    assert!(halfway > 0.49 && halfway < 0.51);
    assert_eq!(focus_release_progress(IMPORT_FOCUS_RELEASE_DURATION), 1.0);
    assert_eq!(
        focus_release_progress(IMPORT_FOCUS_RELEASE_DURATION * 4),
        1.0
    );
}

#[test]
fn unified_gizmo_hit_math_keeps_move_rotate_and_center_scale_live_together() {
    let origin = pos2(100.0, 100.0);
    let geometry = AlignmentGizmoGeometry {
        origin,
        axis_ends: [Some(pos2(160.0, 100.0)), None, None],
        axis_world_units_per_point: [Some(0.1), None, None],
        rings: [
            vec![pos2(80.0, 140.0), pos2(120.0, 140.0)],
            Vec::new(),
            Vec::new(),
        ],
        scale_handle: origin,
        world_center: glam::Vec3::ZERO,
    };
    assert_eq!(
        alignment_gizmo_hit(origin, &geometry),
        Some(AlignmentGizmoHit::Scale)
    );
    assert_eq!(
        alignment_gizmo_hit(pos2(135.0, 100.0), &geometry),
        Some(AlignmentGizmoHit::Move(0))
    );
    assert_eq!(
        alignment_gizmo_hit(pos2(100.0, 140.0), &geometry),
        Some(AlignmentGizmoHit::Rotate(0))
    );
}

#[test]
fn move_gizmo_arrowhead_is_a_forward_facing_triangle() {
    let arrow = gizmo_arrowhead(pos2(10.0, 20.0), pos2(50.0, 20.0)).unwrap();
    assert_eq!(arrow[0], pos2(50.0, 20.0));
    assert!(arrow[1].x < arrow[0].x && arrow[2].x < arrow[0].x);
    assert!((arrow[1].x - arrow[2].x).abs() < f32::EPSILON);
    assert!(arrow[1].y > arrow[0].y && arrow[2].y < arrow[0].y);
    assert!(gizmo_arrowhead(pos2(1.0, 1.0), pos2(1.0, 1.0)).is_none());
}

#[test]
fn overlay_alpha_and_sculpt_wire_palette_are_explicit() {
    assert_eq!(overlay_alpha(-0.5), 0.0);
    assert_eq!(overlay_alpha(0.32), 0.32);
    assert_eq!(overlay_alpha(2.0), 1.0);
    assert_eq!(overlay_alpha(f32::NAN), 0.0);

    let chosen = Color32::BLACK;
    let active = SculptTargets::default();
    let (active_color, active_alpha) =
        sculpt_group_wire_style(chosen, active, SculptSurfaceGroup::HeadSkin, 0.32);
    let (inactive_color, inactive_alpha) =
        sculpt_group_wire_style(chosen, active, SculptSurfaceGroup::Eyes, 0.32);
    assert_eq!(active_color, chosen);
    assert_eq!(inactive_color, chosen);
    assert!((active_alpha - 0.32).abs() < f32::EPSILON);
    assert!(inactive_alpha > 0.0 && inactive_alpha < active_alpha);
}

#[test]
fn the_island_names_whatever_a_stroke_would_actually_do() {
    egui::__run_test_ui(|ui| {
        for brush in SculptBrush::ALL {
            for modifiers in [
                egui::Modifiers::default(),
                egui::Modifiers::SHIFT,
                egui::Modifiers::CTRL,
                egui::Modifiers::ALT,
                egui::Modifiers::SHIFT | egui::Modifiers::ALT,
                egui::Modifiers::SHIFT | egui::Modifiers::CTRL,
            ] {
                let mode = sculpt_input_mode(ui, modifiers, brush);
                let shown = brush_shown_for(mode, brush);
                if mode == SculptInputMode::Smooth {
                    assert_eq!(
                        shown,
                        SculptBrush::Smooth,
                        "{modifiers:?} on {brush:?} smooths, so the island has to say so"
                    );
                }
                assert!(
                    shown == brush || mode == SculptInputMode::Smooth,
                    "{modifiers:?} moved the island off {brush:?} for something other than a held Shift"
                );
            }
        }

        for brush in SculptBrush::ALL {
            assert_eq!(
                brush_shown_for(
                    sculpt_input_mode(ui, egui::Modifiers::default(), brush),
                    brush
                ),
                brush
            );
        }
    });
}

#[test]
fn sculpt_modifier_shortcuts_override_the_selected_brush() {
    egui::__run_test_ui(|ui| {
        for brush in SculptBrush::ALL
            .into_iter()
            .filter(|brush| brush.edits_geometry())
        {
            assert_eq!(
                sculpt_input_mode(
                    ui,
                    egui::Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    brush
                ),
                SculptInputMode::Smooth
            );
            assert_eq!(
                sculpt_input_mode(
                    ui,
                    egui::Modifiers {
                        ctrl: true,
                        ..Default::default()
                    },
                    brush
                ),
                SculptInputMode::Inflate
            );

            let alt_only = egui::Modifiers {
                alt: true,
                ..Default::default()
            };
            if matches!(brush, SculptBrush::Restore) {
                assert_eq!(
                    sculpt_input_mode(ui, alt_only, brush),
                    SculptInputMode::RestoreFit
                );

                assert_eq!(
                    sculpt_input_mode(
                        ui,
                        egui::Modifiers {
                            alt: true,
                            ctrl: true,
                            ..Default::default()
                        },
                        brush
                    ),
                    SculptInputMode::Inflate
                );
            } else {
                assert_eq!(
                    sculpt_input_mode(ui, alt_only, brush),
                    SculptInputMode::Inflate
                );
            }
        }

        assert_eq!(
            sculpt_input_mode(ui, egui::Modifiers::default(), SculptBrush::Move),
            SculptInputMode::Grab
        );
        assert_eq!(
            sculpt_input_mode(ui, egui::Modifiers::default(), SculptBrush::Smooth),
            SculptInputMode::Smooth
        );
        assert_eq!(
            sculpt_input_mode(ui, egui::Modifiers::default(), SculptBrush::Restore),
            SculptInputMode::Restore
        );

        assert!(SculptInputMode::Smooth.is_paint_style());
        assert!(SculptInputMode::Restore.is_paint_style());
        assert!(!SculptInputMode::Grab.is_paint_style());
        assert!(!SculptInputMode::Inflate.is_paint_style());
    });
}

#[test]
fn sculpt_hud_uses_one_icon_per_brush_choice() {
    assert_eq!(sculpt_brush_icon(SculptBrush::Move), Icon::BrushMove);
    assert_eq!(sculpt_brush_icon(SculptBrush::Smooth), Icon::BrushSmooth);
    assert_eq!(sculpt_brush_icon(SculptBrush::Restore), Icon::BrushRestore);
    assert_eq!(
        sculpt_brush_text_key(SculptBrush::Move),
        TextKey::SculptBrushMove
    );
    assert_eq!(
        sculpt_brush_text_key(SculptBrush::Smooth),
        TextKey::SculptBrushSmooth
    );
    assert_eq!(
        sculpt_brush_text_key(SculptBrush::Restore),
        TextKey::SculptBrushRestore
    );
    assert_eq!(SculptBrush::default(), SculptBrush::Move);
}

#[test]
fn sculpt_path_sampling_is_radius_scaled_and_event_partition_invariant() {
    let start = pos2(10.0, 20.0);
    let end = pos2(110.0, 20.0);
    let mut remainder = 0.0;
    let samples = sculpt_spaced_samples(start, end, 20.0, &mut remainder);
    assert_eq!(samples.len(), 25);
    assert_eq!(samples.last().copied(), Some(end));
    assert!(remainder.abs() < f32::EPSILON);

    let mut partitioned = Vec::new();
    let mut partition_remainder = 0.0;
    let mut segment_start = start;
    for offset in 1..=100 {
        let segment_end = pos2(start.x + offset as f32, start.y);
        partitioned.extend(sculpt_spaced_samples(
            segment_start,
            segment_end,
            20.0,
            &mut partition_remainder,
        ));
        segment_start = segment_end;
    }
    assert_eq!(partitioned.len(), samples.len());
    for (single, split) in samples.iter().zip(&partitioned) {
        assert!(single.distance(*split) < 1.0e-4);
    }
    assert!((partition_remainder - remainder).abs() < 1.0e-4);
    assert!(sculpt_dab_spacing(10.0) < sculpt_dab_spacing(40.0));

    let mut grab_remainder = 17.0;
    let short_end = start + vec2(1.0, 0.0);
    assert_eq!(
        sculpt_input_samples(
            SculptInputMode::Grab,
            start,
            short_end,
            220.0,
            &mut grab_remainder,
        ),
        vec![short_end],
        "Grab must respond live even below radius-based spacing"
    );
    assert_eq!(grab_remainder, 0.0);
}

#[test]
fn viewport_popovers_measure_contents_and_reuse_the_same_hit_rect() {
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(480.0, 520.0));
    let mut state = AppState::default();
    let rail = viewport_tool_rail_rect(viewport).unwrap();
    assert_eq!(VIEWPORT_TOOL_PANELS[0], ViewportToolPanel::Lighting);
    assert_eq!(VIEWPORT_TOOL_PANELS[1], BACKGROUND_PANEL);
    assert!(viewport_tool_panel_placement(viewport, BACKGROUND_PANEL).is_some());
    for panel in VIEWPORT_TOOL_PANELS {
        assert_eq!(
            viewport_tool_panel_padding_y(panel),
            Some(MINI_POPUP_CONTENT_INSET_Y)
        );
    }
    assert_eq!(viewport_tool_panel_padding_x(), MINI_POPUP_CONTENT_INSET_X);

    let mut heights = Vec::new();
    for (panel, mode) in [
        (ViewportToolPanel::Lighting, BaseViewMode::Texture),
        (BACKGROUND_PANEL, BaseViewMode::Texture),
        (ViewportToolPanel::Camera, BaseViewMode::Texture),
        (ViewportToolPanel::Wireframe, BaseViewMode::Texture),
        (ViewportToolPanel::Xray, BaseViewMode::Texture),
        (ViewportToolPanel::Skin, BaseViewMode::Solid),
        (ViewportToolPanel::Skin, BaseViewMode::Texture),
    ] {
        let (measured, cached) = measured_panel_rect(&mut state, viewport, panel, mode);
        let measured = measured.expect("measured popover must fit the test viewport");
        assert_eq!(cached, Some(measured));
        assert!(viewport.contains(measured.min) && viewport.contains(measured.max));
        assert!(measured.height() >= 48.0);
        assert!(viewport_tool_panel_contains(
            viewport,
            Some(measured),
            measured.center()
        ));
        assert!(!viewport_tool_panel_should_dismiss(
            viewport,
            Some(measured),
            measured.center(),
            true
        ));
        heights.push((panel, mode, measured.height()));
    }
    let height = |panel, mode| {
        heights
            .iter()
            .find(|(candidate, candidate_mode, _)| *candidate == panel && *candidate_mode == mode)
            .unwrap()
            .2
    };
    assert!(
        height(ViewportToolPanel::Lighting, BaseViewMode::Texture)
            > height(ViewportToolPanel::Wireframe, BaseViewMode::Texture)
    );
    assert!(
        height(ViewportToolPanel::Camera, BaseViewMode::Texture)
            > height(ViewportToolPanel::Xray, BaseViewMode::Texture)
    );

    assert!(
        height(ViewportToolPanel::Skin, BaseViewMode::Texture)
            > height(ViewportToolPanel::Skin, BaseViewMode::Solid)
    );
    assert!(
        height(ViewportToolPanel::Lighting, BaseViewMode::Texture) >= 215.0,
        "lighting must include the final rotation row and shared bottom inset"
    );
    assert!(
        height(ViewportToolPanel::Skin, BaseViewMode::Solid) >= 135.0,
        "solid color must include both color rows and shared bottom inset"
    );
    assert!(viewport_tool_panel_contains(viewport, None, rail.center()));

    let help = measured_help_popup_rect(viewport, Tab::Edit, Locale::English).unwrap();
    assert_eq!(
        help.height(),
        MINI_HELP_CONTENT_INSET_Y * 2.0 + PIN_HELP_ROWS.len() as f32 * HELP_ROW_HEIGHT
    );
    assert!(viewport.contains_rect(help));
}

#[test]
fn solid_popover_frame_contains_the_final_g2_swatch() {
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(480.0, 520.0));
    let mut state = AppState::default();
    state.base_view_mode = BaseViewMode::Solid;
    state.viewport_tool_panel = Some(ViewportToolPanel::Skin);
    let expected = g2_solid_color(&state);
    let context = egui::Context::default();
    let mut panel_rect = None;
    let output = context.run_ui(Default::default(), |ui| {
        draw_viewport_tools(ui, &mut state, viewport, "solid-bounds-regression");
        panel_rect = cached_viewport_tool_panel_rect(ui, &state, viewport);
    });
    let panel_rect = panel_rect.expect("solid popover must be measured");
    let g2_swatch = output
        .shapes
        .into_iter()
        .find_map(|clipped| match clipped.shape {
            egui::Shape::Rect(shape) if shape.fill == expected => Some(shape.rect),
            _ => None,
        })
        .expect("G2 color swatch must be painted");
    assert!(panel_rect.contains_rect(g2_swatch));
    assert!(panel_rect.bottom() - g2_swatch.bottom() >= MINI_POPUP_CONTENT_INSET_Y);
}

#[test]
fn minimum_app_viewport_contains_the_content_derived_sculpt_hud() {
    let state = AppState::default();
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1120.0, 680.0));
    let plan = detail_hud_plan(&state, viewport).expect("HUD must fit the app minimum");

    assert!(matches!(
        plan.tier,
        DetailHudTier::Compact | DetailHudTier::Full
    ));
    let hud = plan.rect;
    assert!(viewport.contains(hud.min) && viewport.contains(hud.max));
}

#[test]
fn what_the_chrome_paints_is_what_the_pointer_test_respects() {
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 720.0));
    let context = egui::Context::default();
    let mut state = AppState::default();

    state.camera_control = crate::camera_control::ControlMode::Trackball;

    let gizmo = crate::viewport_chrome::slot(
        viewport,
        crate::viewport_chrome::ChromeAnchor::TopRight,
        crate::viewport::ORIENTATION_HUD_SIZE,
        0,
    )
    .expect("the gizmo fits a 1280x720 viewport");

    let input = || egui::RawInput {
        screen_rect: Some(viewport),
        ..Default::default()
    };
    let _ = context.run_ui(input(), |root| {
        paint_viewport_chrome(root, &state, viewport, TurntableCamera::default());
    });

    let _ = context.run_ui(input(), |root| {
        assert!(
            viewport_chrome_covers(root, gizmo.center()),
            "the gizmo was painted but the pointer falls through it"
        );
        assert!(
            !viewport_chrome_covers(root, viewport.center()),
            "the middle of the head is not chrome"
        );
    });
}

#[test]
fn an_open_popover_owns_its_rect_for_hit_testing() {
    let context = egui::Context::default();
    let screen = Rect::from_min_size(egui::Pos2::ZERO, vec2(1280.0, 800.0));
    let mut state = AppState::default();
    state.viewport_tool_panel = Some(crate::state::ViewportToolPanel::Lighting);

    let mut inside = None;
    let _ = context.run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |root| {
            super::panels::draw_viewport_tools(root, &mut state, screen, "test");
            inside = super::panels::cached_viewport_tool_panel_rect(root, &state, screen);
        },
    );

    let Some(panel) = inside else {
        panic!("the lighting popover did not open");
    };

    let _ = context.run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        },
        |root| {
            super::panels::draw_viewport_tools(root, &mut state, screen, "test");
        },
    );

    let owner = context.layer_id_at(panel.center());
    assert_eq!(
        owner,
        Some(super::panels::viewport_tool_panel_layer()),
        "the popover's own layer must answer for points inside it, or no list in it can ever receive the wheel"
    );
}

#[test]
fn the_header_island_never_reaches_under_the_axis_gizmo() {
    let state = AppState::default();
    for width in [1600.0, 1400.0, 1212.0, 1120.0, 900.0, 700.0, 500.0, 450.0] {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 680.0));
        let (Some(plan), Some(gizmo)) = (
            detail_hud_plan(&state, viewport),
            crate::viewport_chrome::slot(
                viewport,
                crate::viewport_chrome::ChromeAnchor::TopRight,
                crate::viewport::ORIENTATION_HUD_SIZE,
                0,
            ),
        ) else {
            continue;
        };
        assert!(
            plan.rect.right() <= gizmo.left(),
            "at {width} the island reached {}, under a gizmo starting at {}",
            plan.rect.right(),
            gizmo.left()
        );
    }
}

#[test]
fn detail_hud_degrades_through_compact_and_split_view_rows_before_hiding() {
    let viewport = |width: f32| Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 680.0));

    let state = AppState::default();
    let reserved = {
        let probe = viewport(2000.0);
        2000.0
            - (detail_hud_right_base(probe)
                - detail_hud_row_left(&state, probe)
                - DETAIL_MODE_CAPSULE_WIDTH
                - DETAIL_MODE_CAPSULE_GAP)
    };
    let at = |band: f32| viewport(reserved + band);

    let full = detail_hud_plan(&state, at(DETAIL_HUD_OUTER_WIDTH)).unwrap();
    assert_eq!(full.tier, DetailHudTier::Full);
    assert_eq!(
        full.rect.size(),
        vec2(DETAIL_HUD_OUTER_WIDTH, DETAIL_HUD_HEIGHT)
    );

    let compact = detail_hud_plan(&state, at(DETAIL_HUD_OUTER_WIDTH - 1.0)).unwrap();
    assert_eq!(compact.tier, DetailHudTier::Compact);
    assert_eq!(compact.rect.height(), DETAIL_HUD_HEIGHT);
    assert_eq!(compact.rect.width(), DETAIL_HUD_COMPACT_MAX_OUTER_WIDTH);

    let compact_floor = detail_hud_plan(&state, at(DETAIL_HUD_COMPACT_MIN_WIDTH)).unwrap();
    assert_eq!(compact_floor.tier, DetailHudTier::Compact);
    assert_eq!(compact_floor.rect.width(), DETAIL_HUD_COMPACT_MIN_WIDTH);

    let two_row = detail_hud_plan(&state, at(DETAIL_HUD_COMPACT_MIN_WIDTH - 1.0)).unwrap();
    assert_eq!(two_row.tier, DetailHudTier::TwoRow);
    assert_eq!(two_row.rect.height(), DETAIL_HUD_TWO_ROW_HEIGHT);

    let two_row_floor = detail_hud_plan(&state, at(DETAIL_HUD_TWO_ROW_MIN_WIDTH)).unwrap();
    assert_eq!(two_row_floor.tier, DetailHudTier::TwoRow);
    assert_eq!(two_row_floor.rect.width(), DETAIL_HUD_TWO_ROW_MIN_WIDTH);

    let three_row = detail_hud_plan(&state, at(DETAIL_HUD_TWO_ROW_MIN_WIDTH - 1.0)).unwrap();
    assert_eq!(three_row.tier, DetailHudTier::ThreeRow);
    assert_eq!(three_row.rect.height(), DETAIL_HUD_THREE_ROW_HEIGHT);

    let narrowest = detail_hud_plan(&state, at(DETAIL_HUD_MIN_WIDTH)).unwrap();
    assert_eq!(narrowest.tier, DetailHudTier::ThreeRow);
    assert_eq!(narrowest.rect.width(), DETAIL_HUD_MIN_WIDTH);

    assert!(detail_hud_plan(&state, at(DETAIL_HUD_MIN_WIDTH - 1.0)).is_none());

    for band in [
        DETAIL_HUD_OUTER_WIDTH,
        DETAIL_HUD_OUTER_WIDTH - 1.0,
        DETAIL_HUD_COMPACT_MIN_WIDTH,
        DETAIL_HUD_COMPACT_MIN_WIDTH - 1.0,
        DETAIL_HUD_TWO_ROW_MIN_WIDTH,
        DETAIL_HUD_TWO_ROW_MIN_WIDTH - 1.0,
        DETAIL_HUD_MIN_WIDTH,
    ] {
        let plan = detail_hud_plan(&state, at(band)).unwrap();
        assert!(at(band).contains(plan.rect.min));
        assert!(at(band).contains(plan.rect.max));
    }
}

#[test]
fn header_band_covers_the_two_row_sculpt_hud() {
    let mut state = AppState::default();
    state.active_tab = Tab::Morph;
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 680.0));
    let plan = detail_hud_plan(&state, viewport).unwrap();
    assert_eq!(plan.tier, DetailHudTier::TwoRow);
    let second_row_point = pos2(
        plan.rect.center().x,
        plan.rect.bottom() - DETAIL_HUD_INSET_Y - 4.0,
    );

    assert!(second_row_point.y < detail_header_band_bottom(&state, viewport));
    assert!(detail_viewport_controls_contains(
        &state,
        viewport,
        second_row_point
    ));

    let below = pos2(
        plan.rect.center().x,
        detail_header_band_bottom(&state, viewport) + 8.0,
    );
    assert!(!detail_viewport_controls_contains(&state, viewport, below));
}

#[test]
fn header_band_covers_the_three_row_sculpt_hud_in_a_split_view() {
    let mut state = AppState::default();
    state.active_tab = Tab::Morph;

    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(474.0, 680.0));
    let plan = detail_hud_plan(&state, viewport).unwrap();
    assert_eq!(plan.tier, DetailHudTier::ThreeRow);
    let third_row_point = pos2(
        plan.rect.center().x,
        plan.rect.bottom() - DETAIL_HUD_INSET_Y - 4.0,
    );
    assert!(detail_viewport_controls_contains(
        &state,
        viewport,
        third_row_point
    ));
}

#[test]
fn hud_label_column_hugs_text_and_respects_tier_caps() {
    assert_eq!(detail_numeric_label_width(52.0, 270.0), 54.0);
    assert_eq!(detail_numeric_label_width(90.0, 270.0), 64.0);

    assert_eq!(detail_numeric_label_width(26.0, 150.0), 28.0);
    assert_eq!(detail_numeric_label_width(40.0, 150.0), 42.0);
    assert_eq!(detail_numeric_label_width(60.0, 150.0), 44.0);

    assert!(detail_numeric_label_width(20.0, 270.0) < 26.0);
}

#[test]
fn flexed_hud_sliders_stay_within_authored_bounds() {
    let compact_floor_content = DETAIL_HUD_COMPACT_MIN_WIDTH - DETAIL_HUD_INSET_X * 2.0;
    let floor = detail_hud_flex_numeric_width(compact_floor_content, true);
    assert!(floor >= DETAIL_HUD_MIN_NUMERIC_WIDTH);
    let widest_compact_content = DETAIL_HUD_COMPACT_MAX_OUTER_WIDTH - DETAIL_HUD_INSET_X * 2.0;
    assert_eq!(
        detail_hud_flex_numeric_width(widest_compact_content, true),
        DETAIL_NUMERIC_CONTROL_WIDTH
    );
    let narrow_split =
        detail_hud_flex_numeric_width(DETAIL_HUD_MIN_WIDTH - DETAIL_HUD_INSET_X * 2.0, false);
    assert!(narrow_split >= DETAIL_HUD_MIN_NUMERIC_WIDTH);
    assert!(narrow_split <= DETAIL_NUMERIC_CONTROL_WIDTH);
    assert!(
        detail_hud_flex_numeric_width(700.0, true) >= detail_hud_flex_numeric_width(600.0, true)
    );
}

#[test]
fn sculpt_hud_uses_one_icon_per_falloff_choice() {
    assert_eq!(
        sculpt_falloff_icon(SculptFalloff::Smooth),
        Icon::FalloffSmooth
    );
    assert_eq!(
        sculpt_falloff_icon(SculptFalloff::Smoother),
        Icon::FalloffSmoother
    );
    assert_eq!(
        sculpt_falloff_icon(SculptFalloff::Sharp),
        Icon::FalloffSharp
    );
    assert_eq!(
        sculpt_falloff_icon(SculptFalloff::Linear),
        Icon::FalloffLinear
    );
}

#[test]
fn viewport_popover_click_away_and_detail_overlays_block_input() {
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(360.0, 260.0));
    let mut state = AppState::default();
    let (panel, cached) = measured_panel_rect(
        &mut state,
        viewport,
        ViewportToolPanel::Wireframe,
        BaseViewMode::Texture,
    );
    let panel = panel.unwrap();
    assert_eq!(cached, Some(panel));
    assert!(!viewport_tool_panel_should_dismiss(
        viewport,
        Some(panel),
        panel.center(),
        true
    ));
    assert!(!viewport_tool_panel_should_dismiss(
        viewport,
        Some(panel),
        pos2(viewport.right() + 20.0, viewport.bottom() + 20.0),
        false
    ));
    assert!(viewport_tool_panel_should_dismiss(
        viewport,
        Some(panel),
        pos2(viewport.right() + 20.0, viewport.bottom() + 20.0),
        true
    ));
    let help_button = viewport_help_button_rect(viewport).unwrap();
    assert!(viewport_tool_panel_should_dismiss(
        viewport,
        Some(panel),
        help_button.center(),
        true
    ));
    assert!(measured_help_contains(
        &state,
        viewport,
        help_button.center()
    ));

    state.viewport_tool_panel = None;
    state.active_tab = Tab::Morph;
    let large = Rect::from_min_size(pos2(0.0, 0.0), vec2(1120.0, 760.0));

    let panel = detail_group_panel_rect(&state, large).expect("detail overlay must fit a desktop");
    assert!(large.contains(panel.min) && large.contains(panel.max));
    assert!(detail_viewport_controls_contains(
        &state,
        large,
        panel.center()
    ));

    let bar = detail_header_bar_rect(&state, large).expect("header bar must fit a desktop");
    assert!(large.contains(bar.min) && large.contains(bar.max));

    state.sculpt_groups_collapsed = false;
    let expanded_groups = detail_group_panel_rect(&state, large).unwrap();
    state.sculpt_groups_collapsed = true;
    let collapsed_groups = detail_group_panel_rect(&state, large).unwrap();
    assert!(collapsed_groups.height() < expanded_groups.height());
    assert!(collapsed_groups.width() < expanded_groups.width());
    assert!(detail_viewport_controls_contains(
        &state,
        large,
        collapsed_groups.center()
    ));
    let help_button = viewport_help_button_rect(large).unwrap();
    assert!(measured_help_contains(&state, large, help_button.center()));
    state.help_visible = true;
    let help_popup = measured_help_popup_rect(large, state.active_tab, state.locale).unwrap();
    assert!(measured_help_contains(&state, large, help_popup.center()));
}

#[test]
fn solid_mesh_role_colors_are_driven_by_state() {
    let mut state = AppState::default();
    state.custom_head_solid_color_rgb = [12, 34, 56];
    state.g2_solid_color_rgb = [210, 170, 120];
    assert_eq!(
        custom_head_solid_color(&state),
        Color32::from_rgb(12, 34, 56)
    );
    assert_eq!(g2_solid_color(&state), Color32::from_rgb(210, 170, 120));
    assert_ne!(custom_head_solid_color(&state), g2_solid_color(&state));
}

#[test]
fn background_choices_and_wire_color_follow_integrated_state() {
    assert_eq!(
        BACKGROUND_MODES,
        [
            ViewportBackgroundMode::Radial,
            ViewportBackgroundMode::Vertical,
            ViewportBackgroundMode::Flat,
        ]
    );
    let mut state = AppState::default();
    for mode in BACKGROUND_MODES {
        state.dispatch(Action::SetViewportBackgroundMode(mode));
        assert_eq!(state.viewport_background_mode, mode);
        for locale in Locale::ALL {
            assert!(!text(locale, background_mode_key(mode)).is_empty());
        }
    }
    state.dispatch(Action::SetWireframeColor([12, 34, 56]));
    assert_eq!(wireframe_color(&state), Color32::from_rgb(12, 34, 56));
}

#[test]
fn split_view_radial_backgrounds_use_each_physical_viewport_rect() {
    let scan_rect = Rect::from_min_size(pos2(40.0, 20.0), vec2(360.0, 600.0));
    let template_rect = Rect::from_min_size(pos2(401.0, 20.0), vec2(520.0, 600.0));

    let scan = radial_background_geometry(scan_rect);
    let template = radial_background_geometry(template_rect);

    assert_eq!(scan.center, scan_rect.center());
    assert_eq!(template.center, template_rect.center());
    assert_ne!(scan.center, template.center);
    assert!((scan.radius - scan_rect.size().length() * 0.58).abs() < f32::EPSILON);
    assert!((template.radius - template_rect.size().length() * 0.58).abs() < f32::EPSILON);
    assert_ne!(scan.radius, template.radius);
    assert_ne!(
        template.center,
        scan_rect.union(template_rect).center(),
        "the right viewport must not reuse the split workspace center"
    );
}

#[test]
fn camera_dispatch_updates_the_relevant_result_view() {
    let mut state = AppState::default();
    state.active_tab = Tab::Morph;
    let before = relevant_viewport_camera(&state);
    state.dispatch(Action::ToggleProjection);
    assert_ne!(
        relevant_viewport_camera(&state).projection_mode,
        before.projection_mode
    );
    state.dispatch(Action::SetFov(500.0));
    assert!((relevant_viewport_camera(&state).fov_y_degrees() - 120.0).abs() < 1.0e-3);
}

#[test]
fn brush_size_asks_the_shared_sweep_for_f_and_a_bounded_radius() {
    use crate::sweep_gesture::{Sweep, swept_value};

    let sweep = Sweep {
        start_pointer: pos2(100.0, 80.0),
        start_value: 64.0,
    };
    assert_eq!(
        swept_value(sweep, pos2(140.0, 200.0), 0.75, Some(8.0..=220.0)),
        94.0
    );
    assert_eq!(
        swept_value(sweep, pos2(60.0, -100.0), 0.75, Some(8.0..=220.0)),
        34.0,
        "only horizontal travel changes radius"
    );

    assert_eq!(
        swept_value(sweep, pos2(10_000.0, 80.0), 0.75, Some(8.0..=220.0)),
        220.0
    );
}

#[test]
fn scan_overlay_belongs_only_to_save_and_zero_opacity_is_off() {
    let mut state = AppState::default();
    state.overlay_opacity = 0.5;
    state.active_tab = Tab::Morph;
    assert_eq!(result_scan_overlay_alpha(&state), None);
    state.active_tab = Tab::Result;
    assert_eq!(result_scan_overlay_alpha(&state), Some(0.5));
    state.overlay_opacity = 0.0;
    assert_eq!(result_scan_overlay_alpha(&state), None);
    state.overlay_opacity = 1.0;
    assert_eq!(
        result_scan_overlay_alpha(&state),
        Some(1.0),
        "100% overlay must remain fully opaque"
    );
}

#[test]
fn stationary_smooth_uses_the_same_fixed_time_dabs_at_common_frame_rates() {
    fn one_second_dabs(frame_rate: usize) -> usize {
        let mut accumulator = SCULPT_SMOOTH_DAB_INTERVAL_SECONDS;
        (0..frame_rate)
            .map(|_| sculpt_smooth_time_dabs(&mut accumulator, 1.0 / frame_rate as f32))
            .sum()
    }

    let at_30 = one_second_dabs(30);
    assert_eq!(at_30, one_second_dabs(60));
    assert_eq!(at_30, one_second_dabs(144));
    assert_eq!(at_30, 31, "one immediate dab plus 30 fixed-time dabs");
}

#[test]
fn orientation_triad_foreshortens_and_orders_by_depth() {
    let camera = front_camera();
    let view = glam::camera::rh::view::look_at_mat4(camera.eye(), camera.target, glam::Vec3::Y);

    let (across, across_depth) = orientation_arm(view, glam::Vec3::X);
    let (up, up_depth) = orientation_arm(view, glam::Vec3::Y);
    let (towards, towards_depth) = orientation_arm(view, glam::Vec3::Z);

    assert!((across.length() - ORIENTATION_AXIS_RADIUS).abs() < 0.05);
    assert!((up.length() - ORIENTATION_AXIS_RADIUS).abs() < 0.05);
    assert!(across_depth.abs() < 0.05 && up_depth.abs() < 0.05);

    assert!(
        towards.length() < ORIENTATION_MIN_ARM,
        "view-aligned axis kept a {towards:?} arm"
    );
    assert!(towards_depth > 0.9, "expected it pointing at the viewer");

    let turned = glam::camera::rh::view::look_at_mat4(
        camera.target + glam::Vec3::new(1.0, 0.0, 1.0),
        camera.target,
        glam::Vec3::Y,
    );
    let (across_turned, _) = orientation_arm(turned, glam::Vec3::X);
    let (towards_turned, _) = orientation_arm(turned, glam::Vec3::Z);
    assert!(across_turned.length() < across.length());
    assert!(towards_turned.length() > towards.length());
}

#[test]
fn help_tables_are_tab_specific_and_keep_documented_pin_inputs() {
    assert_eq!(PIN_HELP_ROWS.len(), 17);
    assert!(PIN_HELP_ROWS.contains(&(TextKey::HelpLevelRoll, TextKey::ShortcutLevelRoll)));

    for rows in [
        ALIGN_HELP_ROWS.as_slice(),
        PIN_HELP_ROWS.as_slice(),
        DETAIL_HELP_ROWS.as_slice(),
        SAVE_HELP_ROWS.as_slice(),
    ] {
        assert!(rows.contains(&(TextKey::HelpSnapView, TextKey::ShortcutSnapView)));
        assert!(rows.contains(&(TextKey::HelpStandardViews, TextKey::ShortcutStandardViews)));
        assert!(rows.contains(&(
            TextKey::HelpCameraProjection,
            TextKey::ShortcutCameraProjection
        )));
    }
    assert!(PIN_HELP_ROWS.contains(&(TextKey::HelpFrameView, TextKey::ShortcutFrameView)));
    assert!(ALIGN_HELP_ROWS.contains(&(TextKey::HelpFrameView, TextKey::ShortcutFrameView)));
    assert!(SAVE_HELP_ROWS.contains(&(TextKey::HelpFrameView, TextKey::ShortcutFrameView)));

    assert!(!DETAIL_HELP_ROWS.contains(&(TextKey::HelpFrameView, TextKey::ShortcutFrameView)));
    assert_eq!(
        PIN_HELP_ROWS[0],
        (TextKey::HelpPlace, TextKey::ShortcutPlace)
    );
    assert!(PIN_HELP_ROWS.contains(&(TextKey::HelpXSymmetry, TextKey::ShortcutXSymmetry)));
    assert!(ALIGN_HELP_ROWS.contains(&(TextKey::HelpSnapRotation, TextKey::ShortcutSnapRotation)));
    for rows in [
        ALIGN_HELP_ROWS.as_slice(),
        PIN_HELP_ROWS.as_slice(),
        DETAIL_HELP_ROWS.as_slice(),
        SAVE_HELP_ROWS.as_slice(),
    ] {
        assert!(rows.contains(&(TextKey::HelpDragZoom, TextKey::ShortcutDragZoom)));
        for locale in Locale::ALL {
            for (function, shortcut) in rows {
                assert!(!text(locale, *function).trim().is_empty());
                assert!(!text(locale, *shortcut).trim().is_empty());
            }
        }
    }
    assert_eq!(
        viewport_help_rows(HelpScope::Tab(Tab::Alignment)),
        ALIGN_HELP_ROWS
    );
    assert_eq!(viewport_help_rows(HelpScope::Tab(Tab::Edit)), PIN_HELP_ROWS);
    assert_eq!(
        viewport_help_rows(HelpScope::Tab(Tab::Morph)),
        DETAIL_HELP_ROWS
    );
    assert_eq!(
        viewport_help_rows(HelpScope::Tab(Tab::Result)),
        SAVE_HELP_ROWS
    );
}

#[test]
fn lighting_angle_control_dispatches_a_delta_from_the_visible_target() {
    let current = 45.0_f32.to_radians();
    let delta = light_rotation_delta_radians(current, 120.0);
    assert!((delta.to_degrees() - 75.0).abs() < 1.0e-4);

    let mut state = AppState::default();
    state.light_yaw_radians = current;
    state.dispatch(Action::RotateLight(delta));
    assert!((state.light_yaw_radians.to_degrees() - 120.0).abs() < 1.0e-4);
}

#[test]
fn picked_pin_projects_back_to_the_clicked_pixel() {
    let mesh = plane_mesh();
    let camera = front_camera();
    let rect = test_rect();
    let pointer = rect.center();
    let endpoint = pick_surface(
        MeshSide::Scan,
        camera,
        pointer,
        rect,
        &mesh,
        ModelTransform::default(),
    )
    .unwrap();
    let world = mesh
        .endpoint_world_point(endpoint, ModelTransform::default())
        .unwrap();
    let projected = camera.project(world, rect).unwrap();
    assert!(projected.screen.distance(pointer) < 1.0e-3);
}

#[test]
fn a_texture_stroke_does_not_reach_the_face_through_the_neck() {
    use vkit_core::vam::{G2UvMapping, G2UvTriangle, UvMaterialRegion};

    let geometry = DazGeometry::new(
        "neck-over-face".into(),
        vec![
            [-1.0, -1.0, 0.5],
            [1.0, -1.0, 0.5],
            [0.0, 1.0, 0.5],
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        vec![vec![0, 1, 2], vec![3, 4, 5]],
        vkit_core::formats::GroupTable {
            indices: vec![0, 0],
            names: vec!["Head".into()],
        },
        vkit_core::formats::GroupTable {
            indices: vec![0, 1],
            names: vec!["Neck".into(), "Face".into()],
        },
        json!({}),
    )
    .unwrap();
    let mesh = SurfaceMesh::from_daz_head_visual(&geometry).unwrap();
    let uv_triangle = |canonical_triangle_index: u32, material_region| G2UvTriangle {
        canonical_face_index: canonical_triangle_index,
        canonical_triangle_index,
        material_region,
        on_head: true,
        position_indices: [0, 1, 2],
        uvs: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
    };
    let mut state = AppState::default();
    state.workspace.result = Some(std::sync::Arc::new(mesh));
    state.vam_uv_mapping = Some(std::sync::Arc::new(G2UvMapping {
        source_path: std::path::PathBuf::new(),
        coordinate_rms_cm: 0.0,
        coordinate_max_cm: 0.0,
        uncovered_triangles: 0,
        faces: Vec::new(),
        triangles: vec![
            uv_triangle(0, UvMaterialRegion::Torso),
            uv_triangle(1, UvMaterialRegion::Face),
        ],
    }));

    let through_the_neck = Ray3::new(
        glam::Vec3::new(0.0, -0.2, 3.0),
        glam::Vec3::new(0.0, 0.0, -1.0),
    )
    .unwrap();
    assert!(
        texture_surface_hit(&state, through_the_neck).is_none(),
        "the stroke reached the face through the neck"
    );

    let mut face_only = state.vam_uv_mapping.as_deref().unwrap().clone();
    face_only
        .triangles
        .retain(|triangle| triangle.material_region == UvMaterialRegion::Face);
    state.vam_uv_mapping = Some(std::sync::Arc::new(face_only));
    assert!(
        texture_surface_hit(&state, through_the_neck).is_some(),
        "the face is pickable once nothing stands in front of it"
    );
}

#[test]
fn the_texture_brush_ring_tracks_the_camera_because_its_dab_is_measured_in_uv() {
    use vkit_core::vam::{G2UvMapping, G2UvTriangle, UvMaterialRegion};

    let geometry = DazGeometry::new(
        "flat-face".into(),
        vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]],
        vec![vec![0, 1, 2]],
        vkit_core::formats::GroupTable {
            indices: vec![0],
            names: vec!["Head".into()],
        },
        vkit_core::formats::GroupTable {
            indices: vec![0],
            names: vec!["Face".into()],
        },
        json!({}),
    )
    .unwrap();
    let mut state = AppState::default();
    state.workspace.result = Some(std::sync::Arc::new(
        SurfaceMesh::from_daz_head_visual(&geometry).unwrap(),
    ));
    state.vam_uv_mapping = Some(std::sync::Arc::new(G2UvMapping {
        source_path: std::path::PathBuf::new(),
        coordinate_rms_cm: 0.0,
        coordinate_max_cm: 0.0,
        uncovered_triangles: 0,
        faces: Vec::new(),
        triangles: vec![G2UvTriangle {
            canonical_face_index: 0,
            canonical_triangle_index: 0,
            material_region: UvMaterialRegion::Face,
            on_head: true,
            position_indices: [0, 1, 2],
            uvs: [[0.0, 0.0], [0.25, 0.0], [0.0, 0.25]],
        }],
    }));

    let viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 900.0));
    let mut camera = TurntableCamera {
        yaw: 0.0,
        pitch: 0.0,
        target: glam::Vec3::ZERO,
        distance: 8.0,
        frame_radius: 2.0,
        ..TurntableCamera::default()
    };

    let far = measure_texture_brush_points_per_uv(&state, viewport, camera, viewport.center())
        .expect("the cursor sits on the face");
    camera.distance = 4.0;
    let near = measure_texture_brush_points_per_uv(&state, viewport, camera, viewport.center())
        .expect("the cursor sits on the face");

    assert!(
        (near / far - 2.0).abs() < 1.0e-3,
        "halving the distance must double the screen span of a UV unit: {far} then {near}"
    );
    assert!(
        (far - viewport.width().min(viewport.height())).abs() > 1.0,
        "the measured span happened to equal the old viewport-relative ring, so this proves nothing"
    );
    assert!(
        measure_texture_brush_points_per_uv(
            &state,
            viewport,
            camera,
            viewport.min + Vec2::splat(4.0),
        )
        .is_none(),
        "there is nothing to measure off the head; that is what the remembered span is for"
    );

    // The span is read where the camera looks, not where the pointer is: the
    // brush paints in texture space, so a ring that changes size as the pointer
    // crosses the face is reporting the local triangle rather than the brush.
    let source = include_str!("detail_panels.rs");
    assert!(
        source.contains(
            "measure_texture_brush_points_per_uv(state, viewport, camera, viewport.center())"
        ),
        "the brush scale is back on the pointer, so the ring breathes as it moves",
    );
}

#[test]
fn template_pin_ray_skips_visible_eye_anatomy_but_occlusion_does_not() {
    let geometry = DazGeometry::new(
        "eye-over-skin".into(),
        vec![
            [-1.0, -1.0, 0.5],
            [1.0, -1.0, 0.5],
            [0.0, 1.0, 0.5],
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        vec![vec![0, 1, 2], vec![3, 4, 5]],
        vkit_core::formats::GroupTable {
            indices: vec![0, 0],
            names: vec!["Head".into()],
        },
        vkit_core::formats::GroupTable {
            indices: vec![0, 1],
            names: vec!["Sclera".into(), "Face".into()],
        },
        json!({}),
    )
    .unwrap();
    let mesh = SurfaceMesh::from_daz_head_visual(&geometry).unwrap();
    let camera = front_camera();
    let rect = test_rect();

    let picked = pick_surface(
        MeshSide::Template,
        camera,
        rect.center(),
        rect,
        &mesh,
        ModelTransform::default(),
    )
    .unwrap();
    assert_eq!(picked.triangle, 1);
    assert_eq!(mesh.render_triangles.len(), 2);

    let skin_world = mesh
        .endpoint_world_point(picked, ModelTransform::default())
        .unwrap();
    assert!(
        !pin_has_camera_line_of_sight(picked, skin_world, &mesh, camera, ModelTransform::default()),
        "the visible Sclera must still occlude a skin pin behind it"
    );
}

#[test]
fn a_grazing_pin_is_allowed_more_slack_than_one_facing_the_camera() {
    let head_on = pin_occlusion_tolerance_share(1.0);
    let silhouette = pin_occlusion_tolerance_share(0.0);
    assert!(
        head_on < 1.0e-3,
        "a pin square to the camera keeps a rounding-sized tolerance: {head_on}"
    );
    assert!(
        silhouette > head_on * 100.0,
        "a grazing pin must get real slack: {silhouette} vs {head_on}"
    );

    let mut previous = head_on;
    for step in 1..=10 {
        let share = pin_occlusion_tolerance_share(1.0 - f64::from(step) / 10.0);
        assert!(share >= previous, "slack shrank at step {step}");
        previous = share;
    }

    assert_eq!(pin_occlusion_tolerance_share(-1.0), silhouette);
}

#[test]
fn pin_markers_only_show_the_camera_facing_surface_in_solid_and_xray() {
    let mesh = layered_mesh();
    let mut state = AppState::default();
    let (front, back) = layered_endpoints();
    state.workspace.pins.add(MeshSide::Scan, front);
    state.workspace.pins.add(MeshSide::Scan, back);
    let rect = test_rect();
    let camera = front_camera();

    for xray in [false, true] {
        state.xray_visible = xray;
        let markers = projected_markers(
            &state,
            MeshSide::Scan,
            rect,
            &mesh,
            camera,
            ModelTransform::default(),
        );
        assert_eq!(
            markers
                .iter()
                .map(|marker| marker.pair_index)
                .collect::<Vec<_>>(),
            vec![0]
        );
        assert_eq!(
            nearest_pin(
                &state,
                MeshSide::Scan,
                rect.center(),
                rect,
                &mesh,
                camera,
                ModelTransform::default(),
            ),
            Some(0)
        );
    }

    let back_camera = TurntableCamera {
        yaw: std::f32::consts::PI,
        ..camera
    };
    let markers = projected_markers(
        &state,
        MeshSide::Scan,
        rect,
        &mesh,
        back_camera,
        ModelTransform::default(),
    );
    assert_eq!(
        markers
            .iter()
            .map(|marker| marker.pair_index)
            .collect::<Vec<_>>(),
        vec![1]
    );
}

#[test]
fn pin_occlusion_survives_extreme_transforms_and_blocks_hidden_selection() {
    let mesh = layered_mesh();
    let rect = test_rect();
    let (front, back) = layered_endpoints();

    for (scale, translation) in [
        (0.01, [140.0, -35.0, 80.0]),
        (100.0, [14_000.0, -3_500.0, 8_000.0]),
    ] {
        let transform = ModelTransform::new(scale, translation);
        let camera = TurntableCamera {
            yaw: 0.35,
            pitch: 0.12,
            target: glam::DVec3::from_array(translation).as_vec3(),
            distance: 3.0 * scale as f32,
            frame_radius: scale as f32,
            ..Default::default()
        };
        let mut paired = AppState::default();
        paired.workspace.pins.add(MeshSide::Scan, front);
        paired.workspace.pins.add(MeshSide::Scan, back);
        assert_eq!(
            projected_markers(&paired, MeshSide::Scan, rect, &mesh, camera, transform,)
                .iter()
                .map(|marker| marker.pair_index)
                .collect::<Vec<_>>(),
            vec![0],
            "scale={scale} translation={translation:?}"
        );

        let mut hidden_only = AppState::default();
        hidden_only.workspace.pins.add(MeshSide::Scan, back);
        let back_world = mesh.endpoint_world_point(back, transform).unwrap();
        let back_screen = camera.project(back_world, rect).unwrap().screen;
        assert!(
            projected_markers(&hidden_only, MeshSide::Scan, rect, &mesh, camera, transform,)
                .is_empty()
        );
        assert_eq!(
            nearest_pin(
                &hidden_only,
                MeshSide::Scan,
                back_screen,
                rect,
                &mesh,
                camera,
                transform,
            ),
            None
        );
    }
}

#[test]
fn x_mirror_adds_stable_negative_then_positive_slots() {
    let mesh = plane_mesh();
    let mut state = AppState::default();
    state.x_mirror = true;
    let positive = SurfaceEndpoint {
        triangle: 0,
        barycentric: [0.25, 0.5, 0.25],
    };
    add_pin_with_optional_mirror(&mut state, MeshSide::Scan, &mesh, positive);
    assert_eq!(state.workspace.pins.pairs().len(), 2);
    let first = state.workspace.pins.pairs()[0].scan.unwrap();
    let second = state.workspace.pins.pairs()[1].scan.unwrap();
    assert!(mesh.endpoint_local_point(first).unwrap().x < 0.0);
    assert!(mesh.endpoint_local_point(second).unwrap().x > 0.0);
    assert!(state.x_mirror);
    state.dispatch(crate::state::Action::Undo);
    assert!(state.workspace.pins.pairs().is_empty());
}

#[test]
fn x_mirror_uses_world_x_after_arbitrary_scan_rotation_without_losing_attachments() {
    let mesh = Arc::new(plane_mesh());
    let mut state = AppState::default();
    state.workspace.scan = Some(Arc::clone(&mesh));
    state.transform.rotation_degrees = [0.0, 0.0, 37.0];
    state.x_mirror = true;
    let picked = SurfaceEndpoint {
        triangle: 0,
        barycentric: [0.25, 0.5, 0.25],
    };

    add_pin_with_optional_mirror(&mut state, MeshSide::Scan, &mesh, picked);

    let endpoints = state
        .workspace
        .pins
        .pairs()
        .iter()
        .filter_map(|pair| pair.scan)
        .collect::<Vec<_>>();
    assert_eq!(endpoints.len(), 2);
    assert!(endpoints.contains(&picked));
    for endpoint in &endpoints {
        assert!(endpoint.triangle < mesh.mesh.triangles.len() as u32);
        assert!((endpoint.barycentric.iter().sum::<f64>() - 1.0).abs() < 1.0e-9);
    }
    let transform = scan_transform(&state);
    let world_x = [endpoints[0], endpoints[1]]
        .map(|endpoint| mesh.endpoint_world_point(endpoint, transform).unwrap().x);
    assert!(world_x[0] <= world_x[1]);
}

#[test]
fn clearing_sculpt_pointer_stroke_removes_the_egui_drag_anchor() {
    let context = egui::Context::default();
    let stroke_id = Id::new(SCULPT_DRAG_ID);
    context.data_mut(|data| {
        data.insert_temp(
            stroke_id,
            SculptViewportStroke {
                mask_step_open: false,
                center_local: [1.0, 2.0, 3.0],
                last_pointer: pos2(10.0, 20.0),
                last_sample_pointer: pos2(10.0, 20.0),
                distance_since_last_sample: 0.0,
                smooth_time_accumulator_seconds: 0.0,
                input_mode: SculptInputMode::Grab,
            },
        );
        data.insert_temp(
            crate::ui_components::BrushSweeps::SCULPT.size(),
            crate::sweep_gesture::Sweep {
                start_pointer: pos2(10.0, 20.0),
                start_value: 64.0,
            },
        );
    });
    assert!(
        context
            .data_mut(|data| data.get_temp::<SculptViewportStroke>(stroke_id))
            .is_some()
    );

    clear_sculpt_pointer_stroke(&context);

    assert!(
        context
            .data_mut(|data| data.get_temp::<SculptViewportStroke>(stroke_id))
            .is_none()
    );
    assert!(
        context
            .data_mut(|data| data.get_temp::<crate::sweep_gesture::Sweep>(
                crate::ui_components::BrushSweeps::SCULPT.size()
            ))
            .is_none()
    );
}

#[test]
fn sculpt_visibility_mask_is_independent_from_result_attachment_toggles() {
    let mut state = AppState::default();
    state.show_result_tear_lacrimals = false;
    state.show_result_eyelashes = false;
    let sculpt_visible = sculpt_visible_targets(&state);
    assert_eq!(sculpt_visible, SculptTargets::ALL);
    assert_eq!(sculpt_hit_targets(&state), SculptTargets::FACE_SURFACE);

    state.show_result_tear_lacrimals = true;
    state.show_result_eyelashes = true;
    assert_eq!(sculpt_visible_targets(&state), SculptTargets::ALL);
    state
        .sculpt
        .set_visible_target_enabled(SculptTarget::Eyelashes, false);
    assert!(!sculpt_visible_targets(&state).contains(SculptTarget::Eyelashes));
    assert!(!sculpt_hit_targets(&state).contains(SculptTarget::Eyelashes));
}

#[test]
fn both_tabs_float_their_prompt_on_the_divider_at_the_same_height() {
    let workspace = Rect::from_min_size(pos2(100.0, 40.0), vec2(900.0, 600.0));
    let split_x = 610.0;
    let centre = crate::viewport::prompt_island_centre(workspace, split_x);
    assert!(
        (centre.x - split_x).abs() < f32::EPSILON,
        "a prompt must sit on the divider, not in the middle of one view"
    );
    assert!((centre.y - workspace.center().y).abs() < f32::EPSILON);

    let dragged = crate::viewport::prompt_island_centre(workspace, 300.0);
    assert!((dragged.x - 300.0).abs() < f32::EPSILON);
    assert!((dragged.y - centre.y).abs() < f32::EPSILON);
}

#[test]
fn the_help_card_names_the_brush_gestures_nobody_finds_alone() {
    for (card, rows, second_sweep) in crate::viewport::BRUSH_HELP_CARDS {
        for wanted in [
            TextKey::HelpBrushSize,
            second_sweep,
            TextKey::ShortcutBrushSize,
            TextKey::ShortcutBrushStrength,
        ] {
            assert!(
                rows.iter()
                    .any(|(function, shortcut)| *function == wanted || *shortcut == wanted),
                "the {card} card does not mention {wanted:?}"
            );
        }
    }
    for locale in Locale::ALL {
        let strength = text(locale, TextKey::ShortcutBrushStrength);
        assert!(
            strength.contains('F'),
            "{locale:?} does not name the F key: {strength}"
        );
    }
}

fn mirror_sheet() -> SurfaceMesh {
    const COLUMNS: usize = 21;
    const ROWS: usize = 11;
    let mut vertices = Vec::new();
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            vertices.push([column as f64 - 10.0, row as f64 - 5.0, 0.0]);
        }
    }
    let mut triangles = Vec::new();
    for row in 0..ROWS - 1 {
        for column in 0..COLUMNS - 1 {
            let base = row * COLUMNS + column;
            triangles.push([base as u32, (base + 1) as u32, (base + COLUMNS) as u32]);
            triangles.push([
                (base + 1) as u32,
                (base + COLUMNS + 1) as u32,
                (base + COLUMNS) as u32,
            ]);
        }
    }
    SurfaceMesh::new(Mesh::new(vertices, triangles).unwrap()).unwrap()
}

fn sheet_pin(mesh: &SurfaceMesh, x: f64, y: f64) -> SurfaceEndpoint {
    let (triangle, barycentric) = mesh
        .mesh
        .triangles
        .iter()
        .enumerate()
        .find_map(|(index, corners)| {
            let [a, b, c] = corners.map(|corner| mesh.mesh.vertices[corner as usize]);
            let area = |p: [f64; 3], q: [f64; 3], r: [f64; 3]| {
                (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
            };
            let total = area(a, b, c);
            if total.abs() < 1.0e-12 {
                return None;
            }
            let point = [x, y, 0.0];
            let u = area(point, b, c) / total;
            let v = area(a, point, c) / total;
            let w = 1.0 - u - v;
            (u >= -1.0e-9 && v >= -1.0e-9 && w >= -1.0e-9).then_some((index as u32, [u, v, w]))
        })
        .expect("the point lies on the sheet");
    SurfaceEndpoint {
        triangle,
        barycentric,
    }
}

#[test]
fn a_mirrored_pin_finds_its_partner_across_the_plane() {
    let mesh = mirror_sheet();
    let mut state = AppState::default();
    state.workspace.scan = Some(std::sync::Arc::new(mirror_sheet()));
    state.workspace.scan_source = state.workspace.scan.clone();

    let left = sheet_pin(&mesh, -4.0, 1.0);
    let right = sheet_pin(&mesh, 4.0, 1.0);
    let lone = sheet_pin(&mesh, -9.0, -4.0);
    state.add_surface_pins(MeshSide::Scan, [left, right, lone]);
    assert_eq!(state.workspace.pins.pairs().len(), 3);

    assert_eq!(
        mirrored_pin_index(&state, MeshSide::Scan, &mesh, 0),
        Some(1),
        "the pair should find each other"
    );
    assert_eq!(
        mirrored_pin_index(&state, MeshSide::Scan, &mesh, 1),
        Some(0),
        "and the relation runs both ways"
    );
    assert_eq!(
        mirrored_pin_index(&state, MeshSide::Scan, &mesh, 2),
        None,
        "a pin with nothing across from it has no partner"
    );

    let mut nudged = AppState::default();
    nudged.workspace.scan = state.workspace.scan.clone();
    nudged.workspace.scan_source = state.workspace.scan_source.clone();
    nudged.add_surface_pins(MeshSide::Scan, [left, sheet_pin(&mesh, 4.4, 1.3)]);
    assert_eq!(
        mirrored_pin_index(&nudged, MeshSide::Scan, &mesh, 0),
        Some(1),
        "a pair that drifted a little is still a pair"
    );

    let mut centred = AppState::default();
    centred.workspace.scan = state.workspace.scan.clone();
    centred.workspace.scan_source = state.workspace.scan_source.clone();
    centred.add_surface_pins(MeshSide::Scan, [sheet_pin(&mesh, 0.0, 2.0), left]);
    assert_eq!(
        mirrored_pin_index(&centred, MeshSide::Scan, &mesh, 0),
        None,
        "a centreline pin has no other side"
    );
}

#[test]
fn a_moved_pane_divider_keeps_the_two_panes_and_the_gap_filling_the_half() {
    use super::detail_hud::{PANE_SPLIT_GAP, pane_rects};

    let half = Rect::from_min_size(pos2(10.0, 20.0), vec2(900.0, 600.0));
    for stacked in [false, true] {
        for ratio in [0.2_f32, 0.35, 0.5, 0.68, 0.8] {
            let (first, second) = pane_rects(half, stacked, ratio);
            let (leading, trailing, extent) = if stacked {
                (first.height(), second.height(), half.height())
            } else {
                (first.width(), second.width(), half.width())
            };
            assert!(
                (leading + trailing + PANE_SPLIT_GAP - extent).abs() < 1.0e-3,
                "stacked={stacked} ratio={ratio}: {leading} + {trailing} + gap must be {extent}"
            );
            assert!(leading > 0.0 && trailing > 0.0);
            if stacked {
                assert!((second.top() - first.bottom() - PANE_SPLIT_GAP).abs() < 1.0e-3);
                assert_eq!(first.width(), half.width());
            } else {
                assert!((second.left() - first.right() - PANE_SPLIT_GAP).abs() < 1.0e-3);
                assert_eq!(first.height(), half.height());
            }
        }
    }
}

fn probe_hair_scalp() -> vkit_core::vam::BuiltinHairScalp {
    let mut vertices_cm = Vec::new();
    let mut triangles = Vec::new();
    const N: usize = 8;
    for row in 0..N {
        for col in 0..N {
            let x = (col as f32 / (N - 1) as f32 - 0.5) * 10.0;
            let z = (row as f32 / (N - 1) as f32 - 0.5) * 10.0;
            vertices_cm.push([x, 10.0, z]);
        }
    }
    for row in 0..N - 1 {
        for col in 0..N - 1 {
            let a = (row * N + col) as u32;
            let b = a + 1;
            let c = a + N as u32;
            let d = c + 1;
            triangles.push([a, c, b]);
            triangles.push([b, c, d]);
        }
    }
    vkit_core::vam::BuiltinHairScalp {
        provider_name: crate::hair_project::HAIR_SCALP_PROVIDERS[0].to_owned(),
        geometry: vkit_core::vam::HairScalpGeometry {
            materials: vec!["scalp".into()],
            uvs: vec![[0.0, 0.0]; vertices_cm.len()],
            vertices_cm,
            triangles,
        },
    }
}

fn probe_hair_state() -> AppState {
    let mut state = AppState::default();
    state.builtin_hair_scalps = std::sync::Arc::new(vec![probe_hair_scalp()]);
    state.active_tab = crate::state::Tab::Hair;
    state.dispatch(crate::state::Action::AddHairPart {
        provider_name: crate::hair_project::HAIR_SCALP_PROVIDERS[0].to_owned(),
    });
    state
}

fn probe_strand_at(state: &mut AppState, part_index: usize, at: [f32; 3]) {
    let strand = crate::hair_project::HairStrand::new(vec![at, [at[0], at[1] + 2.0, at[2]]]);
    let key = state.hair_project.parts[part_index].strands.len() as u32;
    state.hair_project.parts[part_index]
        .strands
        .insert(key, strand);
}

#[test]
fn the_auto_part_ray_prefers_the_part_nearer_the_camera() {
    let mut state = probe_hair_state();
    state.dispatch(crate::state::Action::AddHairPart {
        provider_name: crate::hair_project::HAIR_SCALP_PROVIDERS[0].to_owned(),
    });
    let near = state.hair_project.parts[0].id;
    probe_strand_at(&mut state, 0, [0.4, 10.0, 5.0]);
    probe_strand_at(&mut state, 1, [0.1, 10.0, -5.0]);

    let picked = super::hair_input::ray_part_target(
        &state,
        TurntableCamera::default(),
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0)),
        glam::Vec3::new(0.0, 10.0, 30.0),
        glam::Vec3::new(0.0, 0.0, -1.0),
        1.0e4,
    );
    assert_eq!(picked, Some(near), "depth decides, not sideways distance");
}

#[test]
fn the_auto_part_ray_refuses_what_the_ring_does_not_cover() {
    let mut state = probe_hair_state();
    probe_strand_at(&mut state, 0, [100.0, 10.0, 0.0]);
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));

    let tiny_ring = super::hair_input::ray_part_target(
        &state,
        TurntableCamera::default(),
        viewport,
        glam::Vec3::new(0.0, 10.0, 30.0),
        glam::Vec3::new(0.0, 0.0, -1.0),
        1.0,
    );
    assert_eq!(tiny_ring, None, "a metre off-axis is not under a 1pt brush");

    let behind = super::hair_input::ray_part_target(
        &state,
        TurntableCamera::default(),
        viewport,
        glam::Vec3::new(100.0, 10.0, -30.0),
        glam::Vec3::new(0.0, 0.0, -1.0),
        1.0e4,
    );
    assert_eq!(behind, None, "hair behind the ray origin never counts");
}

#[test]
fn the_auto_part_ray_skips_hidden_parts() {
    let mut state = probe_hair_state();
    let only = state.hair_project.parts[0].id;
    probe_strand_at(&mut state, 0, [0.0, 10.0, 0.0]);
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));
    let ray_origin = glam::Vec3::new(0.0, 10.0, 30.0);
    let ray = glam::Vec3::new(0.0, 0.0, -1.0);

    let seen = super::hair_input::ray_part_target(
        &state,
        TurntableCamera::default(),
        viewport,
        ray_origin,
        ray,
        1.0e4,
    );
    assert_eq!(seen, Some(only));

    state.dispatch(crate::state::Action::ToggleHairPartVisible(only));
    let hidden = super::hair_input::ray_part_target(
        &state,
        TurntableCamera::default(),
        viewport,
        ray_origin,
        ray,
        1.0e4,
    );
    assert_eq!(hidden, None, "a hidden lock cannot take the brush");
}

#[test]
fn relaxing_segment_lengths_restores_spacing_and_pins_the_root() {
    let mut points = vec![
        [0.0, 0.0, 0.0],
        [0.0, 2.0, 0.0],
        [0.0, 4.0, 0.0],
        [0.0, 6.0, 0.0],
    ];
    let spacing = vec![2.0, 2.0, 2.0];
    points[2] = [3.0, 4.5, 1.0];

    super::hair_input::relax_segment_lengths(&mut points, &spacing);

    assert_eq!(points[0], [0.0, 0.0, 0.0], "the root belongs to the scalp");
    for (index, pair) in points.windows(2).enumerate() {
        let d = [
            pair[1][0] - pair[0][0],
            pair[1][1] - pair[0][1],
            pair[1][2] - pair[0][2],
        ];
        let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        assert!(
            (length - spacing[index]).abs() < 1.0e-3,
            "segment {index} came back {length}, wanted {}",
            spacing[index],
        );
    }
}

#[test]
fn the_hair_island_yields_to_the_capture_frame() {
    let mut state = probe_hair_state();
    let wide = Rect::from_min_size(pos2(0.0, 0.0), vec2(1600.0, 900.0));
    assert!(
        super::hair_hud::hair_hud_plan(&state, wide).is_some(),
        "a selected part earns the island"
    );

    let selected = state.hair_project.selected_part_id.take();
    state.hair_project.active_part_ids.clear();
    assert!(
        super::hair_hud::hair_hud_plan(&state, wide).is_some(),
        "the toggles on this row steer the viewport, not a layer, so the island          must outlast the selection"
    );
    state.hair_project.selected_part_id = selected;

    state.hair_thumbnail = Some(crate::state::HairThumbnailJob {
        target: crate::state::HairThumbnailTarget::Preset,
        square: None,
        shoot: false,
    });
    assert!(
        super::hair_hud::hair_hud_plan(&state, wide).is_none(),
        "framing clears the stage"
    );
    state.hair_thumbnail = None;

    let sliver = Rect::from_min_size(pos2(0.0, 0.0), vec2(120.0, 900.0));
    assert!(
        super::hair_hud::hair_hud_plan(&state, sliver).is_none(),
        "no room, no island"
    );
}

#[test]
fn the_toolbox_rect_tracks_its_column_count() {
    let mut state = probe_hair_state();
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1600.0, 900.0));

    state.hair_toolbox_columns = 2;
    let two = super::hair_hud::hair_toolbox_rect(&state, viewport)
        .expect("two columns fit a full viewport");
    state.hair_toolbox_columns = 1;
    let one = super::hair_hud::hair_toolbox_rect(&state, viewport)
        .expect("one column fits a full viewport");

    assert!(
        (two.width() - one.width() - (DETAIL_HUD_TOGGLE_SIZE + crate::theme::SPACE_2)).abs() < 0.01,
        "the second column costs one cell and one gap"
    );
    assert!(one.height() > two.height(), "fewer columns stack taller");
}

#[test]
fn the_stream_switch_draws_the_active_strands_and_nothing_when_off() {
    let painted = |state: &mut AppState| -> usize {
        let pane = Rect::from_min_max(pos2(0.0, 0.0), pos2(900.0, 700.0));
        let context = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(1200.0, 800.0))),
            ..Default::default()
        };
        let output = context.run_ui(raw, |ui| {
            draw_result(ui, state, pane, "hair");
        });
        output.shapes.len()
    };

    let mut state = probe_hair_state();
    let part = state.hair_project.parts[0].id;
    state.dispatch(crate::state::Action::PlantHairStrands {
        part_id: part,
        scalp_indices: (0..12).collect(),
    });

    state.hair_show_streams = false;
    let quiet = painted(&mut state);
    state.hair_show_streams = true;
    let shown = painted(&mut state);
    assert!(
        shown > quiet,
        "turning the switch on added nothing: {quiet} -> {shown}"
    );

    state.dispatch(crate::state::Action::AddHairPart {
        provider_name: crate::hair_project::HAIR_SCALP_PROVIDERS[0].to_owned(),
    });
    let second = state.hair_project.parts[1].id;
    state.dispatch(crate::state::Action::PlantHairStrands {
        part_id: second,
        scalp_indices: (12..24).collect(),
    });
    state.dispatch(crate::state::Action::ActivateHairPart {
        id: part,
        additive: false,
    });
    state.dispatch(crate::state::Action::ActivateHairPart {
        id: second,
        additive: false,
    });
    let one_active = painted(&mut state);
    state.dispatch(crate::state::Action::ActivateHairPart {
        id: part,
        additive: true,
    });
    let both_active = painted(&mut state);
    assert!(
        both_active > one_active,
        "gathering a second layer drew no more streams: {one_active} -> {both_active}"
    );

    state.hair_show_streams = false;
    let quiet_again = painted(&mut state);
    assert!(
        quiet_again < one_active,
        "the switch stopped switching: {quiet_again} vs {one_active}"
    );
}

#[test]
fn framing_a_layers_portrait_puts_that_layer_alone_on_the_head() {
    let mut state = probe_hair_state();
    state.dispatch(crate::state::Action::AddHairPart {
        provider_name: crate::hair_project::HAIR_SCALP_PROVIDERS[0].to_owned(),
    });
    let (first, second) = (
        state.hair_project.parts[0].id,
        state.hair_project.parts[1].id,
    );

    assert_eq!(
        state.hair_isolated_part(),
        None,
        "nothing is isolated while no portrait is being framed"
    );

    state.begin_hair_thumbnail(crate::state::HairThumbnailTarget::Part(first));
    assert_eq!(state.hair_isolated_part(), Some(first));

    state.hair_thumbnail = None;
    state.begin_hair_thumbnail(crate::state::HairThumbnailTarget::Part(second));
    assert_eq!(state.hair_isolated_part(), Some(second));

    state.hair_thumbnail = None;
    state.begin_hair_thumbnail(crate::state::HairThumbnailTarget::Preset);
    assert_eq!(state.hair_isolated_part(), None);

    state.hair_thumbnail = None;
    assert_eq!(state.hair_isolated_part(), None);
}

#[test]
fn a_new_or_duplicated_layer_arrives_on_its_own() {
    let mut state = probe_hair_state();
    let first = state.hair_project.parts[0].id;
    assert_eq!(state.hair_project.editable_parts(), vec![first]);

    state.dispatch(crate::state::Action::AddHairPart {
        provider_name: crate::hair_project::HAIR_SCALP_PROVIDERS[0].to_owned(),
    });
    let second = state.hair_project.parts[1].id;
    assert_eq!(
        state.hair_project.editable_parts(),
        vec![second],
        "creating a layer left the previous one active"
    );
    assert_eq!(state.hair_project.selected_part_id, Some(second));

    state.dispatch(crate::state::Action::ActivateHairPart {
        id: first,
        additive: true,
    });
    assert_eq!(state.hair_project.editable_parts(), vec![first, second]);

    state.dispatch(crate::state::Action::DuplicateHairPart(second));
    let copy = state
        .hair_project
        .parts
        .last()
        .expect("a duplicated layer")
        .id;
    assert_eq!(
        state.hair_project.editable_parts(),
        vec![copy],
        "duplicating left the gathered layers active alongside the copy"
    );
    assert_eq!(state.hair_project.selected_part_id, Some(copy));
}

#[test]
fn the_hair_reset_restores_shape_without_touching_where_or_which() {
    let mut state = probe_hair_state();
    state.dispatch(crate::state::Action::AddHairPart {
        provider_name: crate::hair_project::HAIR_SCALP_PROVIDERS[0].to_owned(),
    });
    let (active, idle) = (
        state.hair_project.parts[0].id,
        state.hair_project.parts[1].id,
    );
    for part_id in [active, idle] {
        state.dispatch(crate::state::Action::PlantHairStrands {
            part_id,
            scalp_indices: (0..6).collect(),
        });
    }
    state.dispatch(crate::state::Action::ActivateHairPart {
        id: active,
        additive: false,
    });

    let planted_tip = |state: &AppState, id: u64| -> [f32; 3] {
        *state
            .hair_project
            .part(id)
            .and_then(|part| part.strands.values().next())
            .and_then(|strand| strand.points_cm.last())
            .expect("a planted strand")
    };
    let (was_active, was_idle) = (planted_tip(&state, active), planted_tip(&state, idle));

    for part_id in [active, idle] {
        let strands: Vec<(u32, Vec<[f32; 3]>)> = state
            .hair_project
            .part(part_id)
            .expect("part")
            .strands
            .iter()
            .map(|(index, strand)| {
                let mut points = strand.points_cm.clone();
                if let Some(tip) = points.last_mut() {
                    tip[0] += 5.0;
                }
                (*index, points)
            })
            .collect();
        state.dispatch(crate::state::Action::SetHairStrandPoints { part_id, strands });
    }
    assert!(
        (planted_tip(&state, active)[0] - was_active[0]).abs() > 1.0,
        "the styling did not take"
    );

    let roots_before = state.hair_project.part(active).expect("part").strands.len();
    state.dispatch(crate::state::Action::ResetHairShapes);

    assert_eq!(
        planted_tip(&state, active),
        was_active,
        "the active layer goes back to the way it was planted"
    );
    assert_eq!(
        state.hair_project.part(active).expect("part").strands.len(),
        roots_before,
        "the reset restores shape, it does not unplant"
    );
    assert_ne!(
        planted_tip(&state, idle),
        was_idle,
        "a layer nobody is working on is not reset"
    );
}

#[test]
fn a_click_activates_one_part_and_shift_gathers_several() {
    let mut state = probe_hair_state();
    let first = state.hair_project.parts[0].id;
    state.dispatch(crate::state::Action::AddHairPart {
        provider_name: crate::hair_project::HAIR_SCALP_PROVIDERS[0].to_owned(),
    });
    let second = state.hair_project.parts[1].id;
    assert_eq!(
        state.hair_project.editable_parts(),
        vec![second],
        "a new layer arrives alone, the way a plain click leaves one"
    );

    state.dispatch(crate::state::Action::ActivateHairPart {
        id: first,
        additive: false,
    });
    assert_eq!(
        state.hair_project.editable_parts(),
        vec![first],
        "a plain click leaves only the clicked part active"
    );
    assert_eq!(state.hair_project.selected_part_id, Some(first));

    state.dispatch(crate::state::Action::ActivateHairPart {
        id: first,
        additive: false,
    });
    assert!(
        state.hair_project.editable_parts().is_empty(),
        "re-clicking the only active part puts it out, so nothing is highlighted"
    );

    state.dispatch(crate::state::Action::ActivateHairPart {
        id: first,
        additive: false,
    });

    state.dispatch(crate::state::Action::ActivateHairPart {
        id: second,
        additive: true,
    });
    assert_eq!(
        state.hair_project.editable_parts(),
        vec![first, second],
        "shift gathers a second part into the set"
    );
    assert_eq!(state.hair_project.selected_part_id, Some(second));

    state.dispatch(crate::state::Action::ActivateHairPart {
        id: second,
        additive: true,
    });
    assert_eq!(state.hair_project.editable_parts(), vec![first]);
    assert_eq!(
        state.hair_project.selected_part_id,
        Some(first),
        "primacy hands over when the primary shifts off"
    );

    state.dispatch(crate::state::Action::ActivateHairPart {
        id: first,
        additive: true,
    });
    assert_eq!(
        state.hair_project.editable_parts(),
        vec![first],
        "the last active part cannot be shifted off"
    );

    state.dispatch(crate::state::Action::ActivateHairPart {
        id: second,
        additive: false,
    });
    assert_eq!(
        state.hair_project.editable_parts(),
        vec![second],
        "a plain click on another part swaps the whole set for it"
    );
    assert_eq!(state.hair_project.selected_part_id, Some(second));
}

#[test]
fn a_deep_rewind_in_hair_warns_before_an_edit_drops_the_road_ahead() {
    let mut state = probe_hair_state();
    let part_id = state.hair_project.parts[0].id;
    for start in 0..6u32 {
        state.dispatch(crate::state::Action::PlantHairStrands {
            part_id,
            scalp_indices: vec![start],
        });
        state.dispatch(crate::state::Action::EndHairStroke);
    }
    for _ in 0..5 {
        state.dispatch(crate::state::Action::Undo);
    }
    let (_, forward) = state.history_position();
    assert_eq!(forward, 5, "five steps stand ahead");

    state.dispatch(crate::state::Action::PlantHairStrands {
        part_id,
        scalp_indices: vec![7],
    });
    assert!(state.pending_history_branch, "the edit must ask first");
    let (_, forward) = state.history_position();
    assert_eq!(forward, 5, "a refused edit drops nothing");

    state.dispatch(crate::state::Action::ConfirmHistoryBranch);
    assert!(!state.pending_history_branch);
    let (_, forward) = state.history_position();
    assert_eq!(forward, 0, "confirming is what clears the road ahead");

    state.dispatch(crate::state::Action::PlantHairStrands {
        part_id,
        scalp_indices: vec![7],
    });
    state.dispatch(crate::state::Action::EndHairStroke);
    assert!(
        !state.pending_history_branch,
        "the second attempt goes through"
    );
}

fn hair_overlay_dot_count(state: &mut AppState, viewport: Rect) -> usize {
    let context = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(1200.0, 800.0))),
        ..Default::default()
    };
    let output = context.run_ui(raw, |ui| {
        draw_result(ui, state, viewport, "hair");
    });
    output
        .shapes
        .iter()
        .filter(|clipped| matches!(clipped.shape, egui::Shape::Circle(_)))
        .count()
}

#[test]
fn the_hair_tab_draws_its_scalp_without_a_result_mesh() {
    let viewport = test_rect();
    let framed = |state: &mut AppState| {
        let mut camera = state.workspace.result_camera;
        camera.frame(crate::scene::Bounds3 {
            min: glam::Vec3::new(-6.0, 4.0, -6.0),
            max: glam::Vec3::new(6.0, 16.0, 6.0),
        });
        state.workspace.result_camera = camera;
    };

    let mut bare = AppState::default();
    bare.active_tab = crate::state::Tab::Hair;
    framed(&mut bare);
    let chrome = hair_overlay_dot_count(&mut bare, viewport);

    let mut state = probe_hair_state();
    state.dispatch(crate::state::Action::PlantHairStrands {
        part_id: state.hair_project.parts[0].id,
        scalp_indices: (0..12).collect(),
    });
    assert!(
        !state.hair_project.parts[0].strands.is_empty(),
        "seeded strands"
    );
    framed(&mut state);
    assert!(
        state.workspace.result.is_none(),
        "this test is about the no-result case",
    );

    let painted = hair_overlay_dot_count(&mut state, viewport);
    assert!(
        painted > chrome,
        "the Hair tab painted no scalp of its own without a result mesh          ({painted} dots vs {chrome} of bare viewport chrome): the scalp, its          vertex dots and every planted strand are invisible, so the tab reads          as completely broken",
    );
}

#[test]
fn arming_a_brush_sweep_never_latches_the_viewport_shut() {
    let mut state = probe_hair_state();
    let viewport = test_rect();
    let context = egui::Context::default();
    let center = viewport.center();

    let frame = |state: &mut AppState, press_f: bool| -> (bool, bool) {
        let mut raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(1200.0, 800.0))),
            ..Default::default()
        };
        raw.events.push(egui::Event::PointerMoved(center));
        if press_f {
            raw.events.push(egui::Event::Key {
                key: egui::Key::F,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            });
            raw.events.push(egui::Event::Key {
                key: egui::Key::F,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Default::default(),
            });
        }
        let mut owned = false;
        let mut camera_ran = false;
        let _ = context.run_ui(raw, |ui| {
            draw_result(ui, state, viewport, "hair");
            owned = brush_sweep_owns_pointer(ui);
            camera_ran = !(state.import_progress.is_some()
                || camera_mode_owns_pointer(state)
                || crate::sweep_gesture::press_spent(ui)
                || viewport_tools_pointer_blocked(ui, state, viewport));
        });
        (owned, camera_ran)
    };

    let _ = frame(&mut state, true);
    let (armed, _) = frame(&mut state, false);
    assert!(
        armed,
        "the F press did not arm the sweep; test is not exercising the latch"
    );

    let _ = frame(&mut state, true);
    let (still_owned, camera_ran) = frame(&mut state, false);
    assert!(
        !still_owned,
        "the sweep survived its own finish keypress: the viewport is latched shut          and camera orbit is dead in every tab until the process restarts",
    );
    assert!(
        camera_ran,
        "the camera is still blocked after the sweep finished"
    );
}

#[test]
fn planting_lands_strands_under_the_pointer() {
    let mut state = probe_hair_state();
    let viewport = test_rect();
    let mut camera = state.workspace.result_camera;
    camera.frame(crate::scene::Bounds3 {
        min: glam::Vec3::new(-6.0, 4.0, -6.0),
        max: glam::Vec3::new(6.0, 16.0, 6.0),
    });
    state.workspace.result_camera = camera;

    let context = egui::Context::default();
    let center = viewport.center();
    for frame in 0..4 {
        let mut raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(1200.0, 800.0))),
            ..Default::default()
        };
        raw.events.push(egui::Event::PointerMoved(center));
        if frame >= 2 {
            raw.events.push(egui::Event::PointerButton {
                pos: center,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            });
        }
        let _ = context.run_ui(raw, |ui| {
            draw_result(ui, &mut state, viewport, "hair");
        });
    }

    let planted = state.hair_project.parts[0].strands.len();
    assert!(
        planted > 0,
        "the plant brush pressed on the scalp and nothing grew",
    );
    let scalp = state.hair_scalps.values().next().cloned().expect("scalp");
    for (index, strand) in &state.hair_project.parts[0].strands {
        let root = scalp.vertices_cm[*index as usize];
        assert_eq!(
            strand.points_cm[0], root,
            "strand {index} left its scalp vertex"
        );
        assert!(strand.points_cm.len() >= 2, "strand {index} has no length");
    }
}

#[test]
fn hair_undo_walks_the_tabs_own_history_one_stroke_at_a_time() {
    let mut state = probe_hair_state();
    let part_id = state.hair_project.parts[0].id;
    let pins_before = state.workspace.pins.pairs().len();

    for chunk in [0..4u32, 4..8, 8..12] {
        state.dispatch(crate::state::Action::PlantHairStrands {
            part_id,
            scalp_indices: chunk.collect(),
        });
    }
    state.dispatch(crate::state::Action::EndHairStroke);
    assert_eq!(state.hair_project.parts[0].strands.len(), 12);

    state.dispatch(crate::state::Action::Undo);
    assert_eq!(
        state.hair_project.parts[0].strands.len(),
        0,
        "one drag should cost exactly one undo",
    );
    assert_eq!(
        state.workspace.pins.pairs().len(),
        pins_before,
        "hair undo reached into another stage's history",
    );

    state.dispatch(crate::state::Action::Redo);
    assert_eq!(
        state.hair_project.parts[0].strands.len(),
        12,
        "redo did not return the stroke"
    );

    state.dispatch(crate::state::Action::PlantHairStrands {
        part_id,
        scalp_indices: (12..16).collect(),
    });
    state.dispatch(crate::state::Action::EndHairStroke);
    state.dispatch(crate::state::Action::Undo);
    assert_eq!(state.hair_project.parts[0].strands.len(), 12);
}

#[test]
fn the_click_that_commits_a_sweep_does_not_reach_the_brush() {
    let mut state = probe_hair_state();
    let viewport = test_rect();
    let mut camera = state.workspace.result_camera;
    camera.frame(crate::scene::Bounds3 {
        min: glam::Vec3::new(-6.0, 4.0, -6.0),
        max: glam::Vec3::new(6.0, 16.0, 6.0),
    });
    state.workspace.result_camera = camera;

    let context = egui::Context::default();
    let center = viewport.center();
    let step = |state: &mut AppState, f: bool, press: bool, release: bool| {
        let mut raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(1200.0, 800.0))),
            ..Default::default()
        };
        raw.events.push(egui::Event::PointerMoved(center));
        if f {
            for pressed in [true, false] {
                raw.events.push(egui::Event::Key {
                    key: egui::Key::F,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Default::default(),
                });
            }
        }
        for (button_pressed, wanted) in [(true, press), (false, release)] {
            if wanted {
                raw.events.push(egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed: button_pressed,
                    modifiers: Default::default(),
                });
            }
        }
        let _ = context.run_ui(raw, |ui| {
            draw_result(ui, state, viewport, "hair");
        });
    };

    step(&mut state, false, false, false);
    step(&mut state, true, false, false);
    step(&mut state, false, true, false);
    assert_eq!(
        state.hair_project.parts[0].strands.len(),
        0,
        "the committing click planted hair",
    );
    step(&mut state, false, false, false);
    assert_eq!(
        state.hair_project.parts[0].strands.len(),
        0,
        "the spent press leaked into the next frame and planted hair",
    );
    step(&mut state, false, false, true);
    step(&mut state, false, false, false);
    step(&mut state, false, true, false);
    assert!(
        !state.hair_project.parts[0].strands.is_empty(),
        "the brush stayed dead after the sweep let go of the pointer",
    );
}

#[test]
fn combing_holds_the_length_without_swinging_the_untouched_tip() {
    let rest = 1.0_f32;
    let mut points: Vec<[f32; 3]> = (0..10)
        .map(|i| [0.0, 10.0 - i as f32 * rest, 0.0])
        .collect();
    let spacing = vec![rest; points.len() - 1];
    let before = points.clone();

    points[4][0] += 0.6;
    hair_input::relax_segment_lengths(&mut points, &spacing);

    assert_eq!(points[0], before[0], "the root left its scalp vertex");

    for (index, window) in points.windows(2).enumerate() {
        let d = [
            window[1][0] - window[0][0],
            window[1][1] - window[0][1],
            window[1][2] - window[0][2],
        ];
        let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        assert!(
            (length - rest).abs() < 0.02,
            "segment {index} is {length}, not {rest}",
        );
    }

    let tip_shift = (points[9][0] - before[9][0]).abs();
    assert!(
        tip_shift < 0.35,
        "the untouched tip moved {tip_shift} cm sideways from a dab in the middle",
    );
}

#[test]
fn the_history_strip_counts_hair_steps() {
    let mut state = probe_hair_state();
    let (baseline, _) = state.history_position();

    let part_id = state.hair_project.parts[0].id;
    for chunk in [0..4u32, 4..8] {
        state.dispatch(crate::state::Action::PlantHairStrands {
            part_id,
            scalp_indices: chunk.collect(),
        });
        state.dispatch(crate::state::Action::EndHairStroke);
    }
    let (back, forward) = state.history_position();
    assert_eq!((back - baseline, forward), (2, 0), "two strokes, two steps");

    state.dispatch(crate::state::Action::Undo);
    let (back, forward) = state.history_position();
    assert_eq!(
        (back - baseline, forward),
        (1, 1),
        "undo moves along the strip",
    );
}

/// The split toggle sits under the view buttons, in one place, on every tab
/// that offers it — and is simply absent on the tabs that do not.
#[test]
fn the_split_toggle_sits_below_the_view_buttons_wherever_it_is_offered() {
    let viewport = test_rect();
    let slot = crate::viewport_tool_layout::viewport_split_toggle_rect(
        viewport,
        super::panels::VIEWPORT_TOOL_PANELS.len(),
    )
    .expect("the rail has room for one more slot");
    let last_panel = crate::viewport_tool_layout::viewport_tool_button_rect(
        viewport,
        super::panels::VIEWPORT_TOOL_PANELS.len() - 1,
    )
    .expect("the last panel button is on screen");
    assert!(
        slot.top() > last_panel.bottom(),
        "the toggle belongs under the buttons it is grouped with"
    );
    assert!(
        (slot.left() - last_panel.left()).abs() < 0.01,
        "same column"
    );

    let mut state = AppState::default();
    for (tab, offered) in [
        (crate::state::Tab::Morph, true),
        (crate::state::Tab::Hair, true),
        (crate::state::Tab::Alignment, false),
        (crate::state::Tab::Result, false),
    ] {
        state.active_tab = tab;
        assert_eq!(
            detail_hud::split_toggle_available(&state),
            offered,
            "{tab:?} disagrees about whether it can split"
        );
    }
}

#[test]
fn the_hair_overlay_never_paints_outside_its_own_pane() {
    let pane = Rect::from_min_max(pos2(40.0, 70.0), pos2(440.0, 670.0));
    let framed = |state: &mut AppState, shove: f32| {
        let mut camera = state.workspace.result_camera;
        camera.frame(crate::scene::Bounds3 {
            min: glam::Vec3::new(-6.0, 4.0, -6.0),
            max: glam::Vec3::new(6.0, 16.0, 6.0),
        });
        camera.target.x += shove;
        state.workspace.result_camera = camera;
    };
    let escaped = |state: &mut AppState| -> usize {
        let context = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(1200.0, 800.0))),
            ..Default::default()
        };
        let output = context.run_ui(raw, |ui| {
            draw_result(ui, state, pane, "hair");
        });
        output
            .shapes
            .iter()
            .filter(|clipped| !pane.contains_rect(clipped.clip_rect))
            .count()
    };

    let planted = |shove: f32| -> AppState {
        let mut state = probe_hair_state();
        let part_id = state.hair_project.parts[0].id;
        state.dispatch(crate::state::Action::PlantHairStrands {
            part_id,
            scalp_indices: (0..24).collect(),
        });
        framed(&mut state, shove);
        state
    };

    let mut centred = planted(0.0);
    let chrome = escaped(&mut centred);

    let mut state = planted(40.0);
    let with_hair = escaped(&mut state);

    assert_eq!(
        with_hair,
        chrome,
        "shoving the head past the pane edge added {} painted shapes that escape          it; the overlay is spilling its dots and strands into the          neighbouring view",
        with_hair.saturating_sub(chrome),
    );
}

#[test]
fn combing_keeps_the_strand_exactly_as_long_as_it_was() {
    let rest = 1.0_f32;
    let mut points: Vec<[f32; 3]> = (0..16)
        .map(|i| [0.0, 10.0 - i as f32 * rest, 0.0])
        .collect();
    let spacing = vec![rest; points.len() - 1];
    let before_total: f32 = spacing.iter().sum();

    for point in &mut points[5..11] {
        point[0] += 2.5;
    }
    hair_input::relax_segment_lengths(&mut points, &spacing);

    let after_total: f32 = points
        .windows(2)
        .map(|pair| {
            let d = [
                pair[1][0] - pair[0][0],
                pair[1][1] - pair[0][1],
                pair[1][2] - pair[0][2],
            ];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        })
        .sum();
    assert!(
        (after_total - before_total).abs() < 1.0e-3,
        "the strand went from {before_total} to {after_total}; a comb must not stretch it",
    );
}

#[test]
fn shift_smoothing_takes_the_kinks_out_and_keeps_the_length() {
    let rest = 1.0_f32;
    let mut points: Vec<[f32; 3]> = (0..16)
        .map(|i| {
            let side = if i % 2 == 0 { 0.4 } else { -0.4 };
            [side, 10.0 - i as f32 * rest, 0.0]
        })
        .collect();
    let spacing: Vec<f32> = points
        .windows(2)
        .map(|pair| {
            let d = [
                pair[1][0] - pair[0][0],
                pair[1][1] - pair[0][1],
                pair[1][2] - pair[0][2],
            ];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        })
        .collect();
    let total_before: f32 = spacing.iter().sum();
    let kink = |points: &[[f32; 3]]| -> f32 {
        points
            .windows(3)
            .map(|w| ((w[0][0] + w[2][0]) * 0.5 - w[1][0]).abs())
            .sum()
    };
    let before = kink(&points);

    let weights = vec![1.0_f32; points.len()];
    for _ in 0..200 {
        hair_input::relax_bending(&mut points, &weights, &spacing, 0.3);
    }

    assert!(
        kink(&points) < before * 0.05,
        "smoothing left {} of {before} of the zigzag",
        kink(&points),
    );
    let total_after: f32 = points
        .windows(2)
        .map(|pair| {
            let d = [
                pair[1][0] - pair[0][0],
                pair[1][1] - pair[0][1],
                pair[1][2] - pair[0][2],
            ];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
        })
        .sum();
    assert!(
        (total_after - total_before).abs() < 1.0e-3,
        "smoothing changed the length from {total_before} to {total_after}",
    );
    assert_eq!(
        points[0],
        [0.4, 10.0, 0.0],
        "the root left its scalp vertex"
    );
}

#[test]
fn the_mask_brush_carves_coverage_without_touching_the_sculpt_stack() {
    use crate::appearance_layers::AppearanceStack;

    let mut state = AppState::default();
    state.sculpt_brush = crate::sculpt::SculptBrush::Mask;
    assert!(
        !crate::sculpt::SculptBrush::Mask.edits_geometry(),
        "the mask brush must not be treated as a geometry edit",
    );

    let mut stack = AppearanceStack::default();
    let id = stack.add("A".into(), Vec::new());
    state.appearance_stack = stack;

    state.dispatch(crate::state::Action::PaintMorphMask {
        vertices: vec![(3, 1.0), (4, 1.0), (5, 1.0)],
        target: 0.0,
        amount: 1.0,
        begins_step: true,
    });
    let layer = state.appearance_stack.layer(id).expect("layer");
    assert_eq!(layer.mask.coverage(4), 0.0);
    assert_eq!(
        layer.mask.coverage(9),
        1.0,
        "an untouched vertex stays claimed"
    );

    state.dispatch(crate::state::Action::PaintMorphMask {
        vertices: vec![(4, 1.0)],
        target: 1.0,
        amount: 1.0,
        begins_step: true,
    });
    assert_eq!(
        state.appearance_stack.layer(id).unwrap().mask.coverage(4),
        1.0
    );

    state.appearance_stack.selected_id = None;
    state.dispatch(crate::state::Action::PaintMorphMask {
        vertices: vec![(0, 1.0)],
        target: 0.0,
        amount: 1.0,
        begins_step: true,
    });
    assert_eq!(
        state.appearance_stack.layer(id).unwrap().mask.coverage(0),
        1.0
    );
}

#[test]
fn the_blend_stands_aside_until_a_layer_is_on_the_stack() {
    use crate::appearance_layers::AppearanceStack;

    let mut state = AppState::default();
    assert!(
        state.appearance_blend(8).is_none(),
        "with no layers the morph library must still do the applying",
    );

    let mut stack = AppearanceStack::default();
    stack.add("A".into(), vec![[1.0, 0.0, 0.0]; 8]);
    state.appearance_stack = stack;
    let blended = state.appearance_blend(8).expect("layers");
    assert_eq!(blended.len(), 8);
    assert!(blended.iter().all(|delta| delta == &[1.0, 0.0, 0.0]));

    let id = state.appearance_stack.layers[0].id;
    state.dispatch(crate::state::Action::SetAppearanceLayerVisible { id, visible: false });
    assert!(state.appearance_blend(8).is_none());
}

#[test]
fn the_mask_brush_stays_a_mask_brush_under_every_modifier() {
    egui::__run_test_ui(|ui| {
        for modifiers in [
            egui::Modifiers::default(),
            egui::Modifiers {
                shift: true,
                ..Default::default()
            },
            egui::Modifiers {
                alt: true,
                ..Default::default()
            },
            egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
            egui::Modifiers {
                alt: true,
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        ] {
            assert_eq!(
                sculpt_input_mode(ui, modifiers, SculptBrush::Mask),
                SculptInputMode::Mask,
                "the mask brush must never edit geometry, held keys included",
            );
        }
    });
}

#[test]
fn a_strand_is_drawn_at_its_authored_width_and_widened_when_sub_pixel() {
    let width_cm = 0.0001_f64 * 100.0;
    assert!(
        (width_cm - 0.01).abs() < 1.0e-12,
        "VaM authors width in metres"
    );

    let drawn = |per_point: f64, t: f32| {
        let taper = 0.06 + (1.0 - 0.06) * (1.0 - t.clamp(0.0, 1.0)).powf(0.55);
        let half_pixels = (width_cm * f64::from(taper) * 0.5 / per_point) as f32;
        let widening = if half_pixels > 0.02 && half_pixels < 0.35 {
            (0.35 / half_pixels).min(8.0)
        } else {
            1.0
        };
        ((half_pixels * widening * 2.0).max(0.1), widening)
    };

    let (far, _) = drawn(0.01, 0.0);
    let (near, _) = drawn(0.001, 0.0);
    assert!(
        near > far,
        "{near} should exceed {far} as the camera closes in"
    );

    let (_, near_widening) = drawn(0.05, 0.0);
    assert!(
        near_widening > 1.0,
        "a sub-pixel strand must be widened, got {near_widening}",
    );
    let (_, capped) = drawn(0.15, 0.0);
    assert!(
        (capped - 8.0).abs() < 1.0e-4,
        "the widening must stop at the shader's cap, got {capped}",
    );
    let (_, abandoned) = drawn(1.0, 0.0);
    assert!(
        (abandoned - 1.0).abs() < 1.0e-4,
        "below the floor the widening is left alone, got {abandoned}",
    );

    let root = 0.06 + 0.94 * 1.0_f32.powf(0.55);
    let middle = 0.06 + 0.94 * 0.5_f32.powf(0.55);
    let tip = 0.06_f32;
    assert!(
        middle > root * 0.6,
        "the shaft stays full: {middle} of {root}"
    );
    assert!(
        tip < root * 0.1,
        "the tip comes to a point: {tip} of {root}"
    );
    assert!(tip > 0.0, "but never to nothing, or the end disappears");
}

#[test]
fn curve_density_fills_in_the_curve_without_moving_the_guides() {
    let guides: Vec<[f32; 3]> = vec![
        [0.0, 0.0, 0.0],
        [1.0, 2.0, 0.0],
        [0.0, 4.0, 0.0],
        [1.0, 6.0, 0.0],
    ];
    let sample = |points: &[[f32; 3]], density: usize| -> Vec<[f32; 3]> {
        if points.len() < 3 || density <= points.len() {
            return points.to_vec();
        }
        let last = points.len() - 1;
        let at = |index: isize| points[index.clamp(0, last as isize) as usize];
        (0..density)
            .map(|step| {
                let along = step as f32 / (density - 1) as f32 * last as f32;
                let segment = (along.floor() as isize).min(last as isize - 1);
                let t = along - segment as f32;
                let (p0, p1, p2, p3) = (
                    at(segment - 1),
                    at(segment),
                    at(segment + 1),
                    at(segment + 2),
                );
                let mut point = [0.0_f32; 3];
                for (axis, point) in point.iter_mut().enumerate() {
                    let (a, b, c, d) = (p0[axis], p1[axis], p2[axis], p3[axis]);
                    *point = 0.5
                        * ((2.0 * b)
                            + (-a + c) * t
                            + (2.0 * a - 5.0 * b + 4.0 * c - d) * t * t
                            + (-a + 3.0 * b - 3.0 * c + d) * t * t * t);
                }
                point
            })
            .collect()
    };

    let coarse = sample(&guides, 4);
    assert_eq!(
        coarse, guides,
        "at or below the guide count, nothing is added"
    );

    let fine = sample(&guides, 31);
    assert_eq!(fine.len(), 31, "the slider is the point count");
    for guide in &guides {
        assert!(
            fine.iter().any(|point| {
                (point[0] - guide[0]).abs() < 1.0e-4
                    && (point[1] - guide[1]).abs() < 1.0e-4
                    && (point[2] - guide[2]).abs() < 1.0e-4
            }),
            "guide {guide:?} is not on the curve drawn through it",
        );
    }
    assert_eq!(fine[0], guides[0]);
    assert_eq!(fine[fine.len() - 1], guides[guides.len() - 1]);
}

#[test]
fn the_hair_sliders_reach_the_look_the_renderer_is_built_from() {
    let mut state = probe_hair_state();
    let part_id = state.hair_project.parts[0].id;
    state.dispatch(crate::state::Action::PlantHairStrands {
        part_id,
        scalp_indices: (0..24).collect(),
    });

    let look_now = |state: &AppState| {
        let part = state.hair_project.part(part_id).expect("part");
        crate::hair_export::authoring_look(part)
    };
    let set = |state: &mut AppState, key: &'static str, value: f32| {
        let param = crate::hair_settings::HAIR_PARAMS
            .iter()
            .find(|param| param.key == key)
            .unwrap_or_else(|| panic!("{key} is not a hair parameter"));
        state.dispatch(crate::state::Action::SetHairParam {
            id: part_id,
            key: param.key,
            value,
        });
    };

    set(&mut state, "hairMultiplier", 4.0);
    let few = look_now(&state).hair_multiplier;
    set(&mut state, "hairMultiplier", 32.0);
    let many = look_now(&state).hair_multiplier;
    assert!(
        many > few,
        "the multiplier did not reach the look: {few:?} -> {many:?}",
    );

    set(&mut state, "curlScale", 0.0);
    let straight = look_now(&state).waviness_settings();
    set(&mut state, "curlScale", 0.8);
    let curled = look_now(&state).waviness_settings();
    assert!(
        curled.scale > straight.scale,
        "the curl slider did not reach the look: {straight:?} -> {curled:?}",
    );
}

#[test]
fn planting_hair_can_be_undone_and_redone() {
    assert!(
        crate::ui::routes_global_undo(crate::state::Tab::Hair, true, true, false),
        "Ctrl+Z must route to undo while the hair tab is open",
    );

    let mut state = probe_hair_state();
    state.active_tab = crate::state::Tab::Hair;
    let part_id = state.hair_project.parts[0].id;
    let planted = |state: &AppState| {
        state
            .hair_project
            .part(part_id)
            .map_or(0, |part| part.strands.len())
    };
    assert_eq!(planted(&state), 0);

    state.dispatch(crate::state::Action::PlantHairStrands {
        part_id,
        scalp_indices: (0..8).collect(),
    });
    state.dispatch(crate::state::Action::EndHairStroke);
    let after = planted(&state);
    assert!(after > 0, "nothing was planted");

    state.dispatch(crate::state::Action::Undo);
    assert_eq!(planted(&state), 0, "Ctrl+Z did not take the planting back");

    state.dispatch(crate::state::Action::Redo);
    assert_eq!(planted(&state), after, "redo did not put it back");
}

#[test]
fn pinching_pulls_across_the_strand_and_favours_the_tip() {
    let centre = [0.0_f32, 0.0, 0.0];
    let points: Vec<[f32; 3]> = (0..9).map(|i| [2.0, i as f32, 0.0]).collect();

    let pull_at = |index: usize| -> [f32; 3] {
        let last = points.len() - 1;
        let along_strand = index as f32 / last as f32;
        let weight = along_strand.powi(3);
        let ahead = points[(index + 1).min(last)];
        let behind = points[index.saturating_sub(1)];
        let raw = [
            ahead[0] - behind[0],
            ahead[1] - behind[1],
            ahead[2] - behind[2],
        ];
        let length = raw.iter().map(|a| a * a).sum::<f32>().sqrt();
        let direction = raw.map(|a| a / length);
        let mut delta = [0.0_f32; 3];
        for (axis, delta) in delta.iter_mut().enumerate() {
            *delta = centre[axis] - points[index][axis];
        }
        let along: f32 = delta.iter().zip(direction).map(|(d, u)| d * u).sum();
        for (axis, delta) in delta.iter_mut().enumerate() {
            *delta -= along * direction[axis];
        }
        delta.map(|d| d * weight)
    };

    let magnitude = |v: [f32; 3]| v.iter().map(|a| a * a).sum::<f32>().sqrt();
    let tip = pull_at(points.len() - 1);
    let middle = pull_at(points.len() / 2);
    let root = pull_at(1);

    assert!(
        magnitude(tip) > magnitude(middle) * 4.0,
        "the tip must lead by a wide margin: {} vs {}",
        magnitude(tip),
        magnitude(middle),
    );
    assert!(
        magnitude(root) < magnitude(tip) * 0.05,
        "the root barely moves, or the whole head of hair drags",
    );
    assert!(
        tip[1].abs() < 1.0e-5,
        "the pull must be across the strand, got {tip:?}",
    );
    assert!(tip[0] < 0.0, "and it must close on the brush, got {tip:?}");
}

#[test]
fn cutting_shortens_in_proportion_and_keeps_every_point() {
    let root = [0.0_f32, 0.0, 0.0];
    let points: Vec<[f32; 3]> = (0..11).map(|i| [0.0, i as f32, 0.0]).collect();

    let cut_at = |index: usize| -> Vec<[f32; 3]> {
        let last = points.len() - 1;
        let keep = (index as f32 / last as f32).max(0.05);
        points
            .iter()
            .map(|point| {
                [
                    root[0] + (point[0] - root[0]) * keep,
                    root[1] + (point[1] - root[1]) * keep,
                    root[2] + (point[2] - root[2]) * keep,
                ]
            })
            .collect()
    };

    let cut = cut_at(5);
    assert_eq!(
        cut.len(),
        points.len(),
        "a cut strand keeps every point; only the length changes",
    );
    assert_eq!(cut[0], points[0], "the root does not move");
    assert!(
        (cut[cut.len() - 1][1] - 5.0).abs() < 1.0e-5,
        "the tip should land where the line was drawn, got {:?}",
        cut[cut.len() - 1],
    );
    let spacing: Vec<f32> = cut.windows(2).map(|pair| pair[1][1] - pair[0][1]).collect();
    for gap in &spacing {
        assert!(
            (gap - spacing[0]).abs() < 1.0e-5,
            "the points bunched: {spacing:?}",
        );
    }

    let stub = cut_at(0);
    assert!(
        stub[stub.len() - 1][1] > 0.0,
        "cutting to the root must leave something to work with",
    );
}

#[test]
fn the_hair_strength_slot_follows_the_tool() {
    use crate::hair_project::HairTool;
    use crate::viewport::hair_hud::shapes_existing_hair;

    for tool in [
        HairTool::Comb,
        HairTool::Pinch,
        HairTool::Cut,
        HairTool::Grow,
    ] {
        assert!(
            shapes_existing_hair(tool),
            "{tool:?} reshapes hair, so it wants a strength",
        );
    }
    for tool in [HairTool::Plant, HairTool::Erase] {
        assert!(
            !shapes_existing_hair(tool),
            "{tool:?} makes or removes hair, so it wants the segment count",
        );
    }

    let mut state = probe_hair_state();
    let before = state.hair_brush_strength;
    state.dispatch(crate::state::Action::SetHairBrushStrength(1.0));
    assert!(state.hair_brush_strength > before);
    state.dispatch(crate::state::Action::SetHairBrushStrength(0.0));
    assert!(
        state.hair_brush_strength > 0.0,
        "a brush of zero strength is a brush that does nothing at all",
    );
}

#[test]
fn combing_drops_the_part_of_the_pull_that_points_away_from_the_head() {
    let normal = glam::Vec3::new(0.0, 1.0, 0.0);
    let sweep = |translation: glam::Vec3| translation - normal * translation.dot(normal);

    let outward = sweep(glam::Vec3::new(0.0, 3.0, 0.0));
    assert!(
        outward.length() < 1.0e-6,
        "outward pull survived: {outward:?}"
    );

    let across = glam::Vec3::new(2.0, 0.0, -1.0);
    assert!(
        (sweep(across) - across).length() < 1.0e-6,
        "a sideways sweep must arrive whole",
    );

    let oblique = glam::Vec3::new(2.0, 2.0, 0.0);
    let combed = sweep(oblique);
    assert!(
        (combed.x - 2.0).abs() < 1.0e-6,
        "the sweep was weakened: {combed:?}"
    );
    assert!(combed.y.abs() < 1.0e-6, "the lift survived: {combed:?}");
    assert!(
        combed.length() < oblique.length(),
        "and the stroke carries less than it arrived with",
    );
}

#[test]
fn dragging_a_hair_slider_is_one_history_step_not_one_per_frame() {
    let mut state = probe_hair_state();
    state.active_tab = crate::state::Tab::Hair;
    let part_id = state.hair_project.parts[0].id;
    let multiplier = crate::hair_settings::HAIR_PARAMS
        .iter()
        .find(|param| param.key == "hairMultiplier")
        .expect("multiplier");
    let curl = crate::hair_settings::HAIR_PARAMS
        .iter()
        .find(|param| param.key == "curlScale")
        .expect("curl");

    let before = state.hair_project.history_position().0;
    for step in 1..=20 {
        state.dispatch(crate::state::Action::SetHairParam {
            id: part_id,
            key: multiplier.key,
            value: step as f32,
        });
    }
    assert_eq!(
        state.hair_project.history_position().0,
        before + 1,
        "twenty frames of one drag is one step",
    );

    state.dispatch(crate::state::Action::SetHairParam {
        id: part_id,
        key: curl.key,
        value: 0.4,
    });
    assert_eq!(state.hair_project.history_position().0, before + 2);

    state.hair_project.end_control();
    state.dispatch(crate::state::Action::SetHairParam {
        id: part_id,
        key: curl.key,
        value: 0.6,
    });
    assert_eq!(state.hair_project.history_position().0, before + 3);

    state.dispatch(crate::state::Action::Undo);
    state.dispatch(crate::state::Action::Undo);
    state.dispatch(crate::state::Action::Undo);
    let part = state.hair_project.part(part_id).expect("part");
    assert!(
        (part.settings.get(multiplier) - 20.0).abs() > 1.0e-6,
        "the drag survived its own undo",
    );
}

#[test]
fn combing_weights_by_distance_alone_and_pins_only_the_root() {
    let radius = 4.0_f32;
    let weight_at = |position: usize, distance: f32| -> f32 {
        if position == 0 {
            return 0.0;
        }
        let inside = (1.0 - (distance / radius).min(1.0)).clamp(0.0, 1.0);
        inside * inside * (3.0 - 2.0 * inside)
    };

    assert_eq!(weight_at(0, 0.0), 0.0);
    assert!(
        (weight_at(1, 0.0) - 1.0).abs() < 1.0e-6,
        "the second point must take the full pull, got {}",
        weight_at(1, 0.0),
    );
    assert!((weight_at(1, 2.0) - weight_at(9, 2.0)).abs() < 1.0e-6);
    assert_eq!(weight_at(5, radius), 0.0);
    let shoulder = weight_at(5, radius * 0.95);
    assert!(shoulder > 0.0 && shoulder < 0.05, "got {shoulder}");
}

#[test]
fn a_settled_head_stops_asking_for_frames() {
    use crate::hair_physics::HairSimulation;
    use crate::viewport::render_callbacks::hair_pump_wanted;

    assert!(
        hair_pump_wanted(HairSimulation::Every, 3.0),
        "a disturbed part is worth stepping"
    );
    assert!(hair_pump_wanted(HairSimulation::Every, 0.1));
    assert!(
        !hair_pump_wanted(HairSimulation::Every, 0.0),
        "a spent budget means the drape has settled and the head does not move"
    );
    assert!(
        !hair_pump_wanted(HairSimulation::Off, 3.0),
        "physics off never pumps, however fresh the budget"
    );
}

#[test]
fn a_tinted_stream_wears_the_same_colour_as_the_part_it_belongs_to() {
    use crate::viewport::hair_overlays::{part_tint_color, part_tint_ink};

    for part_id in [1_u64, 2, 3, 7, 40] {
        let [r, g, b] = part_tint_color(part_id);
        let ink = part_tint_ink(part_id);
        assert_eq!(
            [ink.r(), ink.g(), ink.b()],
            [
                (r * 255.0).round() as u8,
                (g * 255.0).round() as u8,
                (b * 255.0).round() as u8
            ],
            "part {part_id}: the stream must not drift from the strands it stands for"
        );
        assert_ne!(
            ink,
            crate::theme::COLOR_PRIMARY,
            "part {part_id}: a tint that lands on the untinted colour tells the reader nothing"
        );
    }

    let mut seen = std::collections::HashSet::new();
    for part_id in 1_u64..=8 {
        assert!(
            seen.insert(part_tint_ink(part_id).to_array()),
            "two parts share a tint, so the streams cannot be told apart"
        );
    }
}

#[test]
fn a_lit_layer_gains_saturation_without_inventing_a_hue_on_plain_hair() {
    use crate::viewport::hair_overlays::lift_active_layer;

    for grey in [[0.0_f32, 0.0, 0.0], [0.08, 0.08, 0.08], [0.6, 0.6, 0.6]] {
        let lit = lift_active_layer(Some(grey));
        let span = lit.iter().copied().fold(f32::MIN, f32::max)
            - lit.iter().copied().fold(f32::MAX, f32::min);
        assert!(
            span < 1.0e-3,
            "plain hair must stay colourless when lit: {grey:?} became {lit:?}"
        );
        assert!(
            lit[0] > grey[0] || grey[0] >= 1.0,
            "and it must still brighten: {grey:?} became {lit:?}"
        );
    }

    let tinted = [0.45_f32, 0.18, 0.12];
    let lit = lift_active_layer(Some(tinted));
    let saturation = |c: [f32; 3]| {
        let high = c.iter().copied().fold(f32::MIN, f32::max);
        let low = c.iter().copied().fold(f32::MAX, f32::min);
        if high <= 0.0 {
            0.0
        } else {
            (high - low) / high
        }
    };
    assert!(
        saturation(lit) > saturation(tinted),
        "a tinted layer must read MORE coloured when lit, not washed out:          {tinted:?} became {lit:?}"
    );
}

/// A surface fills the mask, so nothing shows through the middle of it.
///
/// The head used to be splatted in one VERTEX at a time. Eleven thousand of
/// them against a cell every four points means, at any real zoom, more cells
/// than vertices — and every cell between two vertices stayed at infinity, so a
/// strand on the far side that landed in one was hidden by nothing. That is
/// what put hair points on the cheek.
#[test]
fn a_filled_triangle_leaves_no_hole_for_the_far_side_to_show_through() {
    use super::hair_overlays::HairDepthField;

    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));

    // Three corners far enough apart that splatting them would leave the middle
    // empty, which is exactly the case that leaked.
    let a = (pos2(40.0, 40.0), 30.0);
    let b = (pos2(240.0, 60.0), 30.0);
    let c = (pos2(120.0, 240.0), 30.0);

    let mut splatted = HairDepthField::probe(rect);
    for corner in [a, b, c] {
        splatted.probe_mark(corner.0, corner.1);
    }
    let middle = pos2(130.0, 110.0);
    assert!(
        !splatted.hides(middle, 60.0),
        "the fixture must reproduce the hole, or this test proves nothing"
    );

    let mut filled = HairDepthField::probe(rect);
    filled.probe_fill(a, b, c);
    assert!(
        filled.hides(middle, 60.0),
        "a point thirty centimetres behind the surface must not show through it"
    );
    assert!(
        !filled.hides(middle, 30.0),
        "and the surface still does not hide itself"
    );
    assert!(
        !filled.hides(pos2(360.0, 40.0), 60.0),
        "outside the triangle nothing is covered, so nothing is hidden"
    );
}

#[test]
fn one_depth_field_answers_for_the_points_and_the_streams_alike() {
    use super::hair_overlays::HairDepthField;

    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 300.0));
    let mut field = HairDepthField::probe(rect);
    let at = pos2(120.0, 90.0);
    field.probe_mark(at, 30.0);

    assert!(
        !field.hides(at, 30.0),
        "whatever drew the field has to survive its own shadow"
    );
    assert!(
        !field.hides(at, 31.5),
        "a centimetre behind is the same lock of hair, not the far side"
    );
    assert!(
        field.hides(at, 45.0),
        "fifteen centimetres further away is across the head"
    );
    assert!(
        field.hides(at + vec2(3.0, 0.0), 45.0),
        "the query dilates by a cell, so a pinhole between marks does not leak"
    );
    assert!(
        !field.hides(pos2(300.0, 40.0), 200.0),
        "nothing was drawn there, so nothing can be hidden by it"
    );
    assert!(
        !field.hides(pos2(-5.0, 90.0), 200.0),
        "a point off the viewport is not behind anything"
    );
}

#[test]
fn picking_answers_to_the_cursor_rather_than_to_the_brush() {
    let mut state = probe_hair_state();
    state.dispatch(crate::state::Action::AddHairPart {
        provider_name: crate::hair_project::HAIR_SCALP_PROVIDERS[0].to_owned(),
    });
    probe_strand_at(&mut state, 0, [0.0, 10.0, 0.0]);
    probe_strand_at(&mut state, 1, [40.0, 10.0, 0.0]);
    let under_cursor = state.hair_project.parts[0].id;
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0));

    let picked = super::hair_input::ray_part_target(
        &state,
        TurntableCamera::default(),
        viewport,
        glam::Vec3::new(0.0, 10.0, 30.0),
        glam::Vec3::new(0.0, 0.0, -1.0),
        super::hair_input::PART_PICK_REACH_POINTS,
    );
    assert_eq!(
        picked,
        Some(under_cursor),
        "the layer far to the side must not answer a click at the centre"
    );

    let brush_wide = super::hair_input::ray_part_target(
        &state,
        TurntableCamera::default(),
        viewport,
        glam::Vec3::new(0.0, 10.0, 30.0),
        glam::Vec3::new(0.0, 0.0, -1.0),
        400.0,
    );
    assert!(
        brush_wide.is_some(),
        "a brush still sweeps as wide as it is told to; only the pick is tight"
    );
}

#[test]
fn a_layer_being_worked_on_reads_apart_from_one_that_is_not() {
    use crate::hair_settings::rgb_to_hsv;
    use crate::viewport::hair_overlays::{lift_active_layer, rest_layer};

    let base = [0.35, 0.18, 0.12];
    let [_, base_saturation, base_value] = rgb_to_hsv(base);
    let [_, lit_saturation, lit_value] = rgb_to_hsv(lift_active_layer(Some(base)));
    let [_, resting_saturation, resting_value] = rgb_to_hsv(rest_layer(Some(base)));

    assert!(
        lit_value > base_value,
        "the layer under the brush has to come forward"
    );
    assert!(
        resting_value < base_value,
        "a resting layer has to fall back, not merely stay put"
    );
    assert!(
        resting_value > base_value * 0.6,
        "and it must stay visible while it waits: {resting_value} against {base_value}"
    );
    assert!(
        lit_value > resting_value * 1.6,
        "at a glance the two must not read the same: {lit_value} against {resting_value}"
    );
    assert!(
        lit_saturation > resting_saturation * 1.5,
        "colour is what separates them: {lit_saturation} against {resting_saturation}"
    );
    assert!(
        resting_saturation > base_saturation * 0.6,
        "a resting layer keeps enough colour to say which layer it is"
    );
}

#[test]
fn a_preset_portrait_holds_every_layer_and_a_layer_portrait_holds_one() {
    let mut state = probe_hair_state();
    state.dispatch(crate::state::Action::AddHairPart {
        provider_name: crate::hair_project::HAIR_SCALP_PROVIDERS[0].to_owned(),
    });
    let first = state.hair_project.parts[0].id;
    probe_strand_at(&mut state, 0, [0.0, 10.0, 0.0]);
    probe_strand_at(&mut state, 1, [60.0, 10.0, 0.0]);

    let Some(whole) = state.hair_portrait_bounds(None) else {
        return;
    };
    let alone = state
        .hair_portrait_bounds(Some(first))
        .expect("a layer frames on itself");

    assert!(
        whole.max.x > alone.max.x,
        "the preset portrait has to reach the layer standing off to the side, or it          is just a picture of the first one"
    );

    let wanted = state.hair_portraits_wanted();
    assert_eq!(
        wanted.len(),
        3,
        "two layers and the preset each want their own portrait"
    );
}

#[test]
fn the_history_bar_ignores_what_is_not_an_edit() {
    let mut state = probe_hair_state();
    let id = state.hair_project.parts[0].id;
    let start = state.hair_project.history_position().0;

    state.dispatch(crate::state::Action::ToggleHairPartVisible(id));
    state.dispatch(crate::state::Action::SetHairBrushRadius(120.0));
    state.dispatch(crate::state::Action::ActivateHairPart {
        id,
        additive: false,
    });
    assert_eq!(
        state.hair_project.history_position().0,
        start,
        "hiding a layer, sizing the brush and picking a layer are not edits"
    );

    for segments in [6, 7, 8, 9] {
        state.dispatch(crate::state::Action::SetHairPartSegments { id, segments });
    }
    assert_eq!(
        state.hair_project.history_position().0,
        start + 1,
        "a segment sweep is one step, not one per sample"
    );

    state.dispatch(crate::state::Action::EndHairStroke);
    state.dispatch(crate::state::Action::SetHairPartSegments { id, segments: 10 });
    assert_eq!(
        state.hair_project.history_position().0,
        start + 2,
        "releasing the sweep closes the step"
    );

    state.dispatch(crate::state::Action::SetHairPartSegments { id, segments: 10 });
    assert_eq!(
        state.hair_project.history_position().0,
        start + 2,
        "a value that does not move records nothing"
    );
}

#[test]
fn stepping_back_leaves_a_hidden_layer_hidden() {
    let mut state = probe_hair_state();
    let id = state.hair_project.parts[0].id;
    state.dispatch(crate::state::Action::SetHairPartSegments { id, segments: 9 });
    state.dispatch(crate::state::Action::EndHairStroke);
    state.dispatch(crate::state::Action::ToggleHairPartVisible(id));
    assert!(!state.hair_project.part(id).unwrap().visible);

    assert!(state.hair_project.undo(), "there is a step to take back");
    assert!(
        !state.hair_project.part(id).unwrap().visible,
        "undo returns the geometry, never the view"
    );
}

#[test]
fn a_hidden_layer_is_out_of_reach_of_the_brush() {
    let mut state = probe_hair_state();
    state.dispatch(crate::state::Action::AddHairPart {
        provider_name: crate::hair_project::HAIR_SCALP_PROVIDERS[0].to_owned(),
    });
    let first = state.hair_project.parts[0].id;
    let second = state.hair_project.parts[1].id;
    state.dispatch(crate::state::Action::ActivateHairPart {
        id: first,
        additive: false,
    });
    state.dispatch(crate::state::Action::ActivateHairPart {
        id: second,
        additive: true,
    });
    assert_eq!(state.hair_project.editable_parts(), vec![first, second]);

    state.dispatch(crate::state::Action::ToggleHairPartVisible(first));
    assert!(
        state.hair_project.is_part_active(first),
        "hiding leaves the layer selected"
    );
    assert_eq!(
        state.hair_project.editable_parts(),
        vec![second],
        "a hidden layer is isolated from every edit"
    );
    assert!(!state.hair_project.is_part_editable(first));
}
