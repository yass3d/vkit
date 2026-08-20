use super::*;

pub(super) fn handle_pin_interaction(
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
            mirror_index: state
                .x_mirror
                .then(|| mirrored_pin_index(state, side, mesh, index))
                .flatten(),
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
        if let Some(mirror) = active.mirror_index
            && let Some(mirrored) = mirrored_endpoint(state, side, mesh, hit)
        {
            state.move_surface_pin(side, mirror, mirrored);
        }
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
        let mirror = state
            .x_mirror
            .then(|| mirrored_pin_index(state, side, mesh, index))
            .flatten();
        match mirror {
            Some(mirror) if mirror > index => {
                state.delete_surface_pin(side, mirror);
                state.delete_surface_pin(side, index);
            }
            Some(mirror) => {
                state.delete_surface_pin(side, index);
                state.delete_surface_pin(side, mirror);
            }
            None => {
                state.delete_surface_pin(side, index);
            }
        }
    }
}

pub(super) fn mirrored_endpoint(
    state: &AppState,
    side: MeshSide,
    mesh: &SurfaceMesh,
    endpoint: SurfaceEndpoint,
) -> Option<SurfaceEndpoint> {
    let point = mesh.endpoint_local_point(endpoint)?;
    let transform = match side {
        MeshSide::Scan => scan_transform(state),
        MeshSide::Template => ModelTransform::default(),
    };
    let world_mesh = mesh_in_world_space(&mesh.mesh, transform);
    let projection = project_mirrored_surface_pin_x(
        &world_mesh,
        transform.point_to_world(point).to_array(),
        MirrorPinOptions {
            center_x: mesh_center_x(&world_mesh),
            ..Default::default()
        },
    );
    if !projection.accepted || !projection.create_counterpart {
        return None;
    }
    projection
        .triangle_id
        .zip(projection.barycentric)
        .map(|(triangle, barycentric)| SurfaceEndpoint {
            triangle,
            barycentric,
        })
}

pub(super) fn mirrored_pin_index(
    state: &AppState,
    side: MeshSide,
    mesh: &SurfaceMesh,
    index: usize,
) -> Option<usize> {
    let pairs = state.workspace.pins.pairs();
    let endpoint = pairs.get(index)?.endpoint(side)?;
    let transform = match side {
        MeshSide::Scan => scan_transform(state),
        MeshSide::Template => ModelTransform::default(),
    };
    let world_of = |endpoint: SurfaceEndpoint| {
        mesh.endpoint_local_point(endpoint)
            .map(|point| transform.point_to_world(point))
    };
    let here = world_of(endpoint)?;
    let world_mesh = mesh_in_world_space(&mesh.mesh, transform);
    let center_x = mesh_center_x(&world_mesh);
    let scale = mesh_characteristic_size(&world_mesh);

    if (here.x - center_x).abs() <= scale * PIN_CENTRELINE_RATIO {
        return None;
    }
    let reflected = glam::DVec3::new(2.0 * center_x - here.x, here.y, here.z);
    let tolerance = scale * PIN_COUNTERPART_RATIO;

    let mut best: Option<(f64, usize)> = None;
    for (candidate, pair) in pairs.iter().enumerate() {
        if candidate == index {
            continue;
        }
        let Some(point) = pair.endpoint(side).and_then(world_of) else {
            continue;
        };
        let distance = point.distance(reflected);
        if distance <= tolerance && best.is_none_or(|(closest, _)| distance < closest) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, candidate)| candidate)
}

pub(super) const PIN_COUNTERPART_RATIO: f64 = 0.06;
pub(super) const PIN_CENTRELINE_RATIO: f64 = 0.004;

pub(super) fn mesh_characteristic_size(mesh: &vkit_core::formats::Mesh) -> f64 {
    let mut minimum = [f64::INFINITY; 3];
    let mut maximum = [f64::NEG_INFINITY; 3];
    for point in &mesh.vertices {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(point[axis]);
            maximum[axis] = maximum[axis].max(point[axis]);
        }
    }
    (0..3)
        .map(|axis| (maximum[axis] - minimum[axis]).powi(2))
        .sum::<f64>()
        .sqrt()
        .max(f64::EPSILON)
}

pub(super) fn add_pin_with_optional_mirror(
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

pub(super) fn mesh_in_world_space(mesh: &CoreMesh, transform: ModelTransform) -> CoreMesh {
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

pub(super) fn mesh_center_x(mesh: &CoreMesh) -> f64 {
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

pub(super) fn pick_surface(
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

pub(super) fn nearest_pin(
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

pub(super) fn projected_markers(
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

pub(super) fn pin_surface_facing(
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

pub(super) fn pin_occlusion_tolerance_share(facing: f64) -> f64 {
    let graze = 1.0 - facing.clamp(0.0, 1.0);
    PIN_OCCLUSION_TOLERANCE_FACTOR + PIN_OCCLUSION_GRAZE_ALLOWANCE * graze
}

pub(super) fn pin_has_camera_line_of_sight(
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

pub(super) fn paint_pin_markers(
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
