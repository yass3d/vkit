use super::*;

pub fn draw_alignment(ui: &mut Ui, state: &mut AppState, rect: Rect) {
    paint_viewport_background(ui, state, rect);
    let response = ui.interact(
        rect,
        Id::new("vkit.viewport.alignment"),
        Sense::click_and_drag(),
    );
    let mut swept = state.workspace.template_camera;
    let roll_owns_pointer =
        handle_camera_control_shortcuts(ui, state, rect, &mut swept, ViewPane::Primary);
    commit_swept_edit_camera(state, MeshSide::Template, swept, roll_owns_pointer);
    let input_blocked = state.import_progress.is_some()
        || roll_owns_pointer
        || camera_mode_owns_pointer(state)
        || crate::sweep_gesture::press_spent(ui)
        || viewport_tools_pointer_blocked(ui, state, rect);
    crate::sweep_gesture::settle_press(ui);
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
        if let Some(moved) = viewport_camera_motion(ui, &response, rect, camera, &pick, false) {
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

pub(super) fn handle_alignment_gizmo(
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

pub(super) fn begin_alignment_gizmo_drag(
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

pub(super) fn alignment_gizmo_hit(
    pointer: Pos2,
    geometry: &AlignmentGizmoGeometry,
) -> Option<AlignmentGizmoHit> {
    gizmo_hit(pointer, geometry, GizmoHandles::ALL)
}

pub(super) fn gizmo_hit(
    pointer: Pos2,
    geometry: &AlignmentGizmoGeometry,
    handles: GizmoHandles,
) -> Option<AlignmentGizmoHit> {
    if handles.scale && pointer.distance(geometry.scale_handle) <= GIZMO_HIT_RADIUS * 1.4 {
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
    let rotate_hit = handles
        .rotate
        .then(|| {
            geometry
                .rings
                .iter()
                .enumerate()
                .map(|(axis, ring)| (polyline_distance(pointer, ring), axis))
                .filter(|(distance, _)| *distance <= GIZMO_HIT_RADIUS)
                .min_by(|left, right| left.0.total_cmp(&right.0))
        })
        .flatten();

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

pub(super) fn apply_alignment_gizmo_drag(
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

pub(super) fn rotation_drag_degrees(
    accumulated_degrees: f64,
    shift_down: bool,
    ctrl_down: bool,
) -> f64 {
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

pub(super) fn paint_alignment_gizmo(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
) {
    if state.busy() {
        return;
    }
    let Some(scan) = state.workspace.scan.as_deref() else {
        return;
    };
    let Some(geometry) = alignment_gizmo_geometry(state, scan, viewport, camera) else {
        return;
    };
    paint_gizmo_geometry(ui, &geometry, GizmoHandles::ALL);
}

pub(super) fn paint_gizmo_geometry(
    ui: &Ui,
    geometry: &AlignmentGizmoGeometry,
    handles: GizmoHandles,
) {
    for (axis, ring) in geometry.rings.iter().enumerate() {
        if handles.rotate && ring.len() >= 2 {
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
    if handles.scale {
        ui.painter().rect_filled(
            Rect::from_center_size(geometry.scale_handle, Vec2::splat(GIZMO_SCALE_HANDLE_SIZE)),
            2.0,
            Color32::WHITE,
        );
    }
}

pub(super) fn gizmo_arrowhead(origin: Pos2, end: Pos2) -> Option<[Pos2; 3]> {
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

pub(super) fn alignment_gizmo_geometry(
    state: &AppState,
    scan: &SurfaceMesh,
    viewport: Rect,
    camera: TurntableCamera,
) -> Option<AlignmentGizmoGeometry> {
    let transform = scan_transform(state);
    let world_bounds = transform.bounds_to_world(scan.facial_focus_bounds());
    let world_size = world_bounds
        .radius()
        .max(camera.frame_radius * 0.08)
        .clamp(camera.frame_radius * 0.08, camera.frame_radius * 0.42);
    gizmo_geometry_at(world_bounds.center(), world_size, viewport, camera)
}

pub(super) fn gizmo_geometry_at(
    world_center: glam::Vec3,
    world_size: f32,
    viewport: Rect,
    camera: TurntableCamera,
) -> Option<AlignmentGizmoGeometry> {
    let origin = camera.project(world_center, viewport)?.screen;
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

pub(super) fn gizmo_axis_color(axis: usize) -> Color32 {
    match axis {
        0 => COLOR_AXIS_X,
        1 => COLOR_AXIS_Y,
        _ => COLOR_AXIS_Z,
    }
}

pub(super) fn point_segment_distance(point: Pos2, start: Pos2, end: Pos2) -> f32 {
    let segment = end - start;
    let length_squared = segment.length_sq();
    if length_squared <= f32::EPSILON {
        return point.distance(start);
    }
    let fraction = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance(start + segment * fraction)
}

pub(super) fn polyline_distance(point: Pos2, points: &[Pos2]) -> f32 {
    points
        .windows(2)
        .map(|pair| point_segment_distance(point, pair[0], pair[1]))
        .fold(f32::INFINITY, f32::min)
}

pub(super) fn polyline_parameter(point: Pos2, points: &[Pos2]) -> Option<f32> {
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

pub(super) fn wrapped_angle_delta(previous: f32, current: f32) -> f32 {
    (current - previous + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI
}

pub(super) fn world_axis_rotated_euler(
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

pub(super) fn translated_axis_value(
    start_value: f64,
    pointer_delta: Vec2,
    screen_direction: Vec2,
    world_units_per_point: f64,
) -> f64 {
    start_value + f64::from(pointer_delta.dot(screen_direction)) * world_units_per_point.max(0.0)
}

pub(super) fn uniform_scale_drag_values(
    start_scale_xyz: [f64; 3],
    pointer_delta: Vec2,
) -> [f64; 3] {
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
