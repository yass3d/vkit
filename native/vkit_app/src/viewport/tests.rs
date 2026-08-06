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
            (ViewportToolPanel::Hair, BaseViewMode::Texture),
        ] {
            for pass in 0..2 {
                let (desired, available) =
                    panel_desired_vs_available(&mut state, viewport, panel, mode);
                assert!(
                    desired <= available + 0.5,
                    "viewport {height} / {panel:?} / {mode:?} / pass {pass}:                          popover wants {desired} of {available}",
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
fn sculpt_modifier_shortcuts_override_the_selected_brush() {
    for brush in SculptBrush::ALL {
        assert_eq!(
            sculpt_input_mode(
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
                sculpt_input_mode(alt_only, brush),
                SculptInputMode::RestoreFit
            );

            assert_eq!(
                sculpt_input_mode(
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
            assert_eq!(sculpt_input_mode(alt_only, brush), SculptInputMode::Inflate);
        }
    }

    assert_eq!(
        sculpt_input_mode(egui::Modifiers::default(), SculptBrush::Move),
        SculptInputMode::Grab
    );
    assert_eq!(
        sculpt_input_mode(egui::Modifiers::default(), SculptBrush::Smooth),
        SculptInputMode::Smooth
    );
    assert_eq!(
        sculpt_input_mode(egui::Modifiers::default(), SculptBrush::Restore),
        SculptInputMode::Restore
    );

    assert!(SculptInputMode::Smooth.is_paint_style());
    assert!(SculptInputMode::Restore.is_paint_style());
    assert!(!SculptInputMode::Grab.is_paint_style());
    assert!(!SculptInputMode::Inflate.is_paint_style());
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
        (ViewportToolPanel::Hair, BaseViewMode::Texture),
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
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(1120.0, 680.0));
    let plan = detail_hud_plan(viewport).expect("HUD must fit the app minimum");

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
        "the popover's own layer must answer for points inside it, or no list          in it can ever receive the wheel"
    );
}

#[test]
fn the_header_island_never_reaches_under_the_axis_gizmo() {
    for width in [1600.0, 1400.0, 1212.0, 1120.0, 900.0, 700.0, 500.0, 450.0] {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(width, 680.0));
        let (Some(plan), Some(gizmo)) = (
            detail_hud_plan(viewport),
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

    let reserved = {
        let probe = viewport(2000.0);
        2000.0
            - (detail_hud_right_base(probe)
                - detail_hud_left_base(probe)
                - DETAIL_MODE_CAPSULE_WIDTH
                - DETAIL_MODE_CAPSULE_GAP)
    };
    let at = |band: f32| viewport(reserved + band);

    let full = detail_hud_plan(at(DETAIL_HUD_OUTER_WIDTH)).unwrap();
    assert_eq!(full.tier, DetailHudTier::Full);
    assert_eq!(
        full.rect.size(),
        vec2(DETAIL_HUD_OUTER_WIDTH, DETAIL_HUD_HEIGHT)
    );

    let compact = detail_hud_plan(at(DETAIL_HUD_OUTER_WIDTH - 1.0)).unwrap();
    assert_eq!(compact.tier, DetailHudTier::Compact);
    assert_eq!(compact.rect.height(), DETAIL_HUD_HEIGHT);
    assert_eq!(compact.rect.width(), DETAIL_HUD_COMPACT_MAX_OUTER_WIDTH);

    let compact_floor = detail_hud_plan(at(DETAIL_HUD_COMPACT_MIN_WIDTH)).unwrap();
    assert_eq!(compact_floor.tier, DetailHudTier::Compact);
    assert_eq!(compact_floor.rect.width(), DETAIL_HUD_COMPACT_MIN_WIDTH);

    let two_row = detail_hud_plan(at(DETAIL_HUD_COMPACT_MIN_WIDTH - 1.0)).unwrap();
    assert_eq!(two_row.tier, DetailHudTier::TwoRow);
    assert_eq!(two_row.rect.height(), DETAIL_HUD_TWO_ROW_HEIGHT);

    let two_row_floor = detail_hud_plan(at(DETAIL_HUD_TWO_ROW_MIN_WIDTH)).unwrap();
    assert_eq!(two_row_floor.tier, DetailHudTier::TwoRow);
    assert_eq!(two_row_floor.rect.width(), DETAIL_HUD_TWO_ROW_MIN_WIDTH);

    let three_row = detail_hud_plan(at(DETAIL_HUD_TWO_ROW_MIN_WIDTH - 1.0)).unwrap();
    assert_eq!(three_row.tier, DetailHudTier::ThreeRow);
    assert_eq!(three_row.rect.height(), DETAIL_HUD_THREE_ROW_HEIGHT);

    let narrowest = detail_hud_plan(at(DETAIL_HUD_MIN_WIDTH)).unwrap();
    assert_eq!(narrowest.tier, DetailHudTier::ThreeRow);
    assert_eq!(narrowest.rect.width(), DETAIL_HUD_MIN_WIDTH);

    assert!(detail_hud_plan(at(DETAIL_HUD_MIN_WIDTH - 1.0)).is_none());

    for band in [
        DETAIL_HUD_OUTER_WIDTH,
        DETAIL_HUD_OUTER_WIDTH - 1.0,
        DETAIL_HUD_COMPACT_MIN_WIDTH,
        DETAIL_HUD_COMPACT_MIN_WIDTH - 1.0,
        DETAIL_HUD_TWO_ROW_MIN_WIDTH,
        DETAIL_HUD_TWO_ROW_MIN_WIDTH - 1.0,
        DETAIL_HUD_MIN_WIDTH,
    ] {
        let plan = detail_hud_plan(at(band)).unwrap();
        assert!(at(band).contains(plan.rect.min));
        assert!(at(band).contains(plan.rect.max));
    }
}

#[test]
fn header_band_covers_the_two_row_sculpt_hud() {
    let mut state = AppState::default();
    state.active_tab = Tab::Morph;
    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 680.0));
    let plan = detail_hud_plan(viewport).unwrap();
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

    let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(442.0, 680.0));
    let plan = detail_hud_plan(viewport).unwrap();
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
    assert_eq!(PIN_HELP_ROWS.len(), 14);

    for rows in [
        ALIGN_HELP_ROWS.as_slice(),
        PIN_HELP_ROWS.as_slice(),
        DETAIL_HELP_ROWS.as_slice(),
        SAVE_HELP_ROWS.as_slice(),
    ] {
        assert!(rows.contains(&(TextKey::HelpSnapView, TextKey::ShortcutSnapView)));
        assert!(rows.contains(&(TextKey::HelpStandardViews, TextKey::ShortcutStandardViews)));
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
                center_local: [1.0, 2.0, 3.0],
                last_pointer: pos2(10.0, 20.0),
                last_sample_pointer: pos2(10.0, 20.0),
                distance_since_last_sample: 0.0,
                smooth_time_accumulator_seconds: 0.0,
                input_mode: SculptInputMode::Grab,
            },
        );
        data.insert_temp(
            Id::new(SCULPT_BRUSH_SIZE_ID),
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
            .data_mut(
                |data| data.get_temp::<crate::sweep_gesture::Sweep>(Id::new(SCULPT_BRUSH_SIZE_ID))
            )
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
