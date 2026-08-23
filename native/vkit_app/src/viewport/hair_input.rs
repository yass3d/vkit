use super::*;
use crate::hair_project::HairTool;

pub(super) const HAIR_BRUSH_RADIUS_RANGE: std::ops::RangeInclusive<f32> = 8.0..=220.0;
const HAIR_SEGMENTS_SWEEP_SENSITIVITY: f32 = 0.06;
const HAIR_COMB_STROKE_ID: &str = "vkit.viewport.hair.comb";
const HAIR_AUTO_PART_LATCH_ID: &str = "vkit.viewport.hair.auto-part-latch";

#[derive(Clone)]
struct HairCombStroke {
    last_pointer: Pos2,
}

pub(crate) fn clear_hair_pointer_state(context: &egui::Context) {
    context.data_mut(|data| data.remove::<HairCombStroke>(Id::new(HAIR_COMB_STROKE_ID)));
    context.data_mut(|data| data.remove::<Vec<u64>>(Id::new(HAIR_AUTO_PART_LATCH_ID)));
    clear_brush_size_gesture(context, crate::ui_components::BrushSweeps::HAIR.size());
    clear_brush_size_gesture(context, crate::ui_components::BrushSweeps::HAIR.strength());
}

pub(super) fn handle_hair_interaction(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
    input_blocked: bool,
    pane: crate::viewport::ViewPane,
) {
    if state.pending_history_branch {
        return;
    }
    let sweep = crate::ui_components::handle_brush_size_gesture(
        ui,
        crate::ui_components::BrushSweeps::HAIR.size(),
        viewport,
        state.hair_brush_radius_points,
        SCULPT_BRUSH_SIZE_SENSITIVITY,
        HAIR_BRUSH_RADIUS_RANGE,
    );
    if let Some(radius) = sweep.radius {
        state.dispatch(Action::SetHairBrushRadius(radius));
    }
    if sweep.consumed {
        return;
    }

    let shaping = crate::viewport::hair_hud::shapes_existing_hair(state.hair_project.active_tool);
    let strength_sweep = crate::ui_components::handle_brush_strength_gesture(
        ui,
        crate::ui_components::BrushSweeps::HAIR.strength(),
        viewport,
        if shaping {
            state.hair_brush_strength
        } else {
            state
                .hair_project
                .selected_part()
                .map_or(crate::hair_project::DEFAULT_HAIR_SEGMENTS as f32, |part| {
                    part.segments as f32
                })
        },
        if shaping {
            crate::ui_components::BRUSH_STRENGTH_SENSITIVITY
        } else {
            HAIR_SEGMENTS_SWEEP_SENSITIVITY
        },
        if shaping {
            0.05..=1.0
        } else {
            2.0..=crate::hair_project::MAX_HAIR_SEGMENTS as f32
        },
    );
    if let Some(value) = strength_sweep.strength {
        if shaping {
            state.dispatch(Action::SetHairBrushStrength(value));
        } else if let Some(id) = state.hair_project.selected_part_id {
            state.dispatch(Action::SetHairPartSegments {
                id,
                segments: value.round() as usize,
            });
        }
    }
    if strength_sweep.consumed {
        return;
    }

    if input_blocked {
        ui.data_mut(|data| data.remove::<HairCombStroke>(Id::new(HAIR_COMB_STROKE_ID)));
        return;
    }

    if let Some(radius) =
        brush_size_key_step(ui, state.hair_brush_radius_points, HAIR_BRUSH_RADIUS_RANGE)
    {
        state.dispatch(Action::SetHairBrushRadius(radius));
    }
    if crate::shortcuts::Shortcut::HairCombBrush.pressed(ui)
        && state.hair_project.active_tool != HairTool::Comb
    {
        state.dispatch(Action::SetHairTool(HairTool::Comb));
    }

    let tool = state.hair_project.active_tool;
    if tool == HairTool::Pick || ui.input(|input| input.modifiers.command) {
        if response.clicked() {
            pick_part_under_pointer(ui, state, viewport, camera);
        }
        return;
    }
    if state.hair_project.selected_part().is_none() {
        return;
    }
    let radius_points = state.hair_brush_radius_points;
    let active = if state.hair_auto_part && tool != HairTool::Plant {
        auto_part_targets(
            ui,
            state,
            viewport,
            camera,
            radius_points,
            state.hair_mirror_edit,
            tool,
        )
    } else {
        state.hair_project.editable_parts()
    };
    // The wrapped cap, not the stock one: the pointer has to meet the head the
    // person is looking at, and so must the root a plant leaves behind.
    let mut providers: Vec<String> = active
        .iter()
        .chain(state.hair_project.selected_part_id.iter())
        .filter_map(|id| state.hair_project.part(*id))
        .map(|part| part.provider_name.clone())
        .collect();
    providers.sort_unstable();
    providers.dedup();
    for provider in providers {
        if let Some(cap) = crate::viewport::hair_overlays::posed_scalp(ui.ctx(), state, &provider) {
            state.posed_hair_scalps.insert(provider, cap);
        }
    }
    let scalp_of = |state: &AppState, id: u64| {
        state.hair_project.part(id).and_then(|part| {
            state
                .posed_hair_scalps
                .get(&part.provider_name)
                .or_else(|| state.hair_scalps.get(&part.provider_name))
                .cloned()
        })
    };
    let mirror = state.hair_mirror_edit;
    if !matches!(tool, HairTool::Pick)
        && state
            .hair_project
            .selected_part()
            .is_some_and(|part| part.kind.is_scalp())
    {
        return;
    }
    match tool {
        HairTool::Pick => {}
        HairTool::Vertex => {
            crate::viewport::hair_vertex::handle(ui, state, viewport, response, camera);
        }
        HairTool::Plant => {
            let Some(primary) = state
                .hair_project
                .selected_part_id
                .filter(|id| state.hair_project.is_part_editable(*id))
            else {
                return;
            };
            let Some(scalp) = scalp_of(state, primary) else {
                return;
            };
            handle_scalp_brush(
                ui,
                state,
                viewport,
                response,
                camera,
                &scalp,
                primary,
                tool,
                radius_points,
                mirror,
            );
        }
        HairTool::Erase => {
            for part_id in active {
                let Some(scalp) = scalp_of(state, part_id) else {
                    continue;
                };
                handle_scalp_brush(
                    ui,
                    state,
                    viewport,
                    response,
                    camera,
                    &scalp,
                    part_id,
                    tool,
                    radius_points,
                    mirror,
                );
            }
        }
        HairTool::Cut => {
            for part_id in active {
                handle_cut_brush(
                    ui,
                    state,
                    viewport,
                    response,
                    camera,
                    part_id,
                    radius_points,
                    mirror,
                );
            }
        }
        HairTool::Grow => {
            for part_id in active {
                handle_grow_brush(
                    ui,
                    state,
                    viewport,
                    response,
                    camera,
                    part_id,
                    radius_points,
                    mirror,
                );
            }
        }
        HairTool::Comb | HairTool::Pinch | HairTool::Puff => {
            let pressed = ui.input(|input| input.pointer.button_pressed(PointerButton::Primary));
            if pressed && response.hovered() {
                crate::viewport::claim_stroke_pane(ui, HAIR_COMB_STROKE_ID, pane);
            }
            if !ui.input(|input| input.pointer.button_down(PointerButton::Primary)) {
                crate::viewport::release_stroke_pane(ui, HAIR_COMB_STROKE_ID);
            }
            if !crate::viewport::stroke_pane_gate(ui, HAIR_COMB_STROKE_ID, pane) {
                return;
            }
            if active.is_empty() {
                let stroke_id = Id::new(HAIR_COMB_STROKE_ID);
                if !ui.input(|input| input.pointer.button_down(PointerButton::Primary)) {
                    ui.data_mut(|data| data.remove::<HairCombStroke>(stroke_id));
                } else if let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
                    && ((pressed && response.hovered())
                        || ui
                            .data_mut(|data| data.get_temp::<HairCombStroke>(stroke_id))
                            .is_some())
                {
                    ui.data_mut(|data| {
                        data.insert_temp(
                            stroke_id,
                            HairCombStroke {
                                last_pointer: pointer,
                            },
                        );
                    });
                }
            }
            let count = active.len();
            for (index, part_id) in active.into_iter().enumerate() {
                handle_comb_brush(
                    ui,
                    state,
                    viewport,
                    response,
                    camera,
                    part_id,
                    radius_points,
                    index + 1 == count,
                    mirror,
                );
            }
        }
    }

    if ui.input(|input| input.pointer.button_down(PointerButton::Primary)) {
        ui.ctx().request_repaint();
    } else if ui.input(|input| input.pointer.button_released(PointerButton::Primary)) {
        state.dispatch(Action::EndHairStroke);
    }
}

pub(super) fn paint_hair_brush_hud(ui: &Ui, state: &AppState, response: &Response) {
    // Pick takes hold of one part and Vertex of one joint; neither has a radius,
    // so both keep the arrow. Vertex never reaches here — it paints its own
    // handles — and Pick is turned away at this line.
    if state.hair_project.active_tool == crate::hair_project::HairTool::Pick {
        return;
    }
    let hover = response
        .hovered()
        .then(|| ui.input(|input| input.pointer.hover_pos()))
        .flatten();
    let Some(cursor) = crate::ui_components::brush_cursor(
        ui,
        hover,
        crate::ui_components::BrushSweeps::HAIR.size(),
        Some((
            crate::ui_components::BrushSweeps::HAIR.strength(),
            state.hair_brush_strength,
        )),
    ) else {
        return;
    };
    crate::ui_components::paint_brush_cursor(
        ui.painter(),
        cursor,
        state.hair_brush_radius_points.max(1.0),
        crate::theme::COLOR_MUTED,
    );
    crate::ui_components::hide_pointer(ui);
}

fn scalp_brush_gather(
    state: &AppState,
    scalp: &crate::hair_project::ScalpAuthoring,
    part_id: u64,
    centre: [f32; 3],
    radius: f32,
    eye: glam::Vec3,
    want_planted: Option<bool>,
) -> Vec<u32> {
    let Some(part) = state.hair_project.part(part_id) else {
        return Vec::new();
    };
    let radius_sq = radius * radius;
    let mut gathered = Vec::new();
    for (index, vertex) in scalp.vertices_cm.iter().enumerate() {
        let dx = vertex[0] - centre[0];
        let dy = vertex[1] - centre[1];
        let dz = vertex[2] - centre[2];
        if dx * dx + dy * dy + dz * dz > radius_sq {
            continue;
        }
        let world = glam::Vec3::from_array(*vertex);
        let normal = scalp
            .normals
            .get(index)
            .map(|n| glam::Vec3::new(n[0], n[1], n[2]))
            .unwrap_or(glam::Vec3::Z);
        if normal.dot(eye - world) <= 0.0 {
            continue;
        }
        let index = index as u32;
        if want_planted.is_none_or(|want| part.strands.contains_key(&index) == want) {
            gathered.push(index);
        }
    }
    gathered
}

fn brush_world_radius(
    viewport: Rect,
    camera: TurntableCamera,
    at: [f32; 3],
    radius_points: f32,
) -> f32 {
    camera.world_units_per_point_at(glam::Vec3::from_array(at), viewport.height())
        * radius_points.max(1.0)
}

fn brush_hit(
    ui: &Ui,
    viewport: Rect,
    camera: TurntableCamera,
    scalp: &crate::hair_project::ScalpAuthoring,
    radius_points: f32,
) -> Option<([f32; 3], f32)> {
    let pointer = ui.input(|input| input.pointer.interact_pos())?;
    let ray = camera.ray_from_screen(pointer, viewport)?;
    let hit = scalp
        .surface
        .pick_visible_surface(ray, ModelTransform::default())?;
    let center = [
        hit.local_point.x as f32,
        hit.local_point.y as f32,
        hit.local_point.z as f32,
    ];
    Some((
        center,
        brush_world_radius(viewport, camera, center, radius_points),
    ))
}

fn strand_cloud_hit(
    ui: &Ui,
    viewport: Rect,
    camera: TurntableCamera,
    part: &crate::hair_project::HairPart,
    radius_points: f32,
) -> Option<([f32; 3], f32, Pos2)> {
    let pointer = ui.input(|input| input.pointer.interact_pos())?;
    let (origin, direction) = pointer_ray(ui, viewport, camera, false)?;
    let (center, radius) =
        strand_cloud_hit_along(viewport, camera, part, radius_points, origin, direction)?;
    Some((center, radius, pointer))
}

fn mirrored_strand_cloud_hit(
    ui: &Ui,
    viewport: Rect,
    camera: TurntableCamera,
    part: &crate::hair_project::HairPart,
    radius_points: f32,
) -> Option<([f32; 3], f32, Pos2)> {
    let pointer = ui.input(|input| input.pointer.interact_pos())?;
    let (origin, direction) = pointer_ray(ui, viewport, camera, true)?;
    let (center, radius) =
        strand_cloud_hit_along(viewport, camera, part, radius_points, origin, direction)?;
    Some(([-center[0], center[1], center[2]], radius, pointer))
}

fn pointer_ray(
    ui: &Ui,
    viewport: Rect,
    camera: TurntableCamera,
    mirrored: bool,
) -> Option<(glam::Vec3, glam::Vec3)> {
    let pointer = ui.input(|input| input.pointer.interact_pos())?;
    let ray = camera.ray_from_screen(pointer, viewport)?;
    let flip = if mirrored { -1.0 } else { 1.0 };
    let origin = glam::Vec3::new(
        ray.origin.x as f32 * flip,
        ray.origin.y as f32,
        ray.origin.z as f32,
    );
    let direction = glam::Vec3::new(
        ray.direction.x as f32 * flip,
        ray.direction.y as f32,
        ray.direction.z as f32,
    )
    .normalize_or_zero();
    Some((origin, direction))
}

fn strand_cloud_hit_along(
    viewport: Rect,
    camera: TurntableCamera,
    part: &crate::hair_project::HairPart,
    radius_points: f32,
    origin: glam::Vec3,
    direction: glam::Vec3,
) -> Option<([f32; 3], f32)> {
    let mut best: Option<([f32; 3], f32)> = None;
    for strand in part.strands.values() {
        for point in &strand.points_cm {
            let world = glam::Vec3::from_array(*point);
            let along = (world - origin).dot(direction);
            if along <= 0.0 {
                continue;
            }
            let distance = (world - (origin + direction * along)).length();
            if best.is_none_or(|(_, best_distance)| distance < best_distance) {
                best = Some((*point, distance));
            }
        }
    }
    let (center, distance) = best?;
    let radius = brush_world_radius(viewport, camera, center, radius_points);
    if distance > radius * 1.5 {
        return None;
    }
    Some((center, radius))
}

fn auto_part_targets(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
    radius_points: f32,
    mirror: bool,
    tool: HairTool,
) -> Vec<u64> {
    let latch_id = Id::new(HAIR_AUTO_PART_LATCH_ID);
    let stroking = matches!(tool, HairTool::Comb | HairTool::Pinch | HairTool::Puff);
    let down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));
    if stroking
        && down
        && let Some(latched) = ui.data_mut(|data| data.get_temp::<Vec<u64>>(latch_id))
    {
        return latched
            .into_iter()
            .filter(|id| state.hair_project.is_part_editable(*id))
            .collect();
    }
    if !down {
        ui.data_mut(|data| data.remove::<Vec<u64>>(latch_id));
    }

    let mut targets = Vec::new();
    if let Some((origin, direction)) = pointer_ray(ui, viewport, camera, false)
        && let Some(id) = ray_part_target(state, camera, viewport, origin, direction, radius_points)
    {
        targets.push(id);
    }
    if mirror
        && let Some((origin, direction)) = pointer_ray(ui, viewport, camera, true)
        && let Some(id) = ray_part_target(state, camera, viewport, origin, direction, radius_points)
        && !targets.contains(&id)
    {
        targets.push(id);
    }
    if stroking && down && !targets.is_empty() {
        ui.data_mut(|data| data.insert_temp(latch_id, targets.clone()));
    }
    targets
}

fn pick_part_under_pointer(ui: &Ui, state: &mut AppState, viewport: Rect, camera: TurntableCamera) {
    let Some((origin, direction)) = pointer_ray(ui, viewport, camera, false) else {
        return;
    };
    let Some(part_id) = ray_part_target(
        state,
        camera,
        viewport,
        origin,
        direction,
        PART_PICK_REACH_POINTS,
    ) else {
        return;
    };
    let additive = crate::shortcuts::Shortcut::ListAddToSelectionHold.held(ui);
    state.dispatch(Action::ActivateHairPart {
        id: part_id,
        additive,
    });
}

pub(super) const PART_PICK_REACH_POINTS: f32 = 3.0;

fn ray_segment_approach(
    origin: glam::Vec3,
    direction: glam::Vec3,
    from: glam::Vec3,
    to: glam::Vec3,
) -> Option<(glam::Vec3, f32)> {
    let span = to - from;
    let offset = from - origin;
    let along_span = direction.dot(span);
    let span_length = span.length_squared();
    let along_offset = direction.dot(offset);
    let across_offset = span.dot(offset);

    let denominator = span_length - along_span * along_span;
    let travel = if denominator.abs() <= f32::EPSILON || span_length <= f32::EPSILON {
        0.0
    } else {
        ((along_offset * along_span - across_offset) / denominator).clamp(0.0, 1.0)
    };
    let closest = from + span * travel;
    let depth = (closest - origin).dot(direction);
    (depth > 0.0).then_some((closest, depth))
}

pub(super) fn ray_part_target(
    state: &AppState,
    camera: TurntableCamera,
    viewport: Rect,
    origin: glam::Vec3,
    direction: glam::Vec3,
    tolerance_points: f32,
) -> Option<u64> {
    let accept = tolerance_points.max(1.0);
    let mut best: Option<(u64, f32)> = None;
    for part in &state.hair_project.parts {
        if !part.visible {
            continue;
        }
        let mut nearest: Option<(f32, f32)> = None;
        for strand in part.strands.values() {
            for pair in strand.points_cm.windows(2) {
                let from = glam::Vec3::from_array(pair[0]);
                let to = glam::Vec3::from_array(pair[1]);
                let Some((closest, depth)) = ray_segment_approach(origin, direction, from, to)
                else {
                    continue;
                };
                let per_point = camera.world_units_per_point_at(closest, viewport.height());
                if per_point <= 0.0 {
                    continue;
                }
                let reach = (closest - (origin + direction * depth)).length() / per_point;
                if reach > accept {
                    continue;
                }
                if nearest.is_none_or(|(_, seen)| depth < seen) {
                    nearest = Some((reach, depth));
                }
            }
        }
        if let Some((_, depth)) = nearest
            && best.is_none_or(|(_, best_depth)| depth < best_depth)
        {
            best = Some((part.id, depth));
        }
    }
    best.map(|(id, _)| id)
}

#[allow(clippy::too_many_arguments)]
fn handle_scalp_brush(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
    scalp: &std::sync::Arc<crate::hair_project::ScalpAuthoring>,
    part_id: u64,
    tool: HairTool,
    radius_points: f32,
    mirror: bool,
) {
    if !ui.input(|input| input.pointer.button_down(PointerButton::Primary)) {
        return;
    }
    if !response.hovered() && !response.dragged() {
        return;
    }
    let Some((centre, radius)) = brush_hit(ui, viewport, camera, scalp, radius_points) else {
        return;
    };
    let want_planted = tool == HairTool::Erase;
    let raw = scalp_brush_gather(state, scalp, part_id, centre, radius, camera.eye(), None);
    let Some(part) = state.hair_project.part(part_id) else {
        return;
    };
    let mut gathered: Vec<u32> = raw
        .iter()
        .copied()
        .filter(|index| part.strands.contains_key(index) == want_planted)
        .collect();
    if mirror {
        for &index in &raw {
            let Some(&pair) = scalp.mirror_pair.get(index as usize) else {
                continue;
            };
            if pair != index && part.strands.contains_key(&pair) == want_planted {
                gathered.push(pair);
            }
        }
        gathered.sort_unstable();
        gathered.dedup();
    }
    if gathered.is_empty() {
        return;
    }
    match tool {
        HairTool::Erase => state.dispatch(Action::UnplantHairStrands {
            part_id,
            scalp_indices: gathered,
        }),
        _ => state.dispatch(Action::PlantHairStrands {
            part_id,
            scalp_indices: gathered,
        }),
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "one brush call per active part, mirror riding along"
)]
fn handle_cut_brush(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
    part_id: u64,
    radius_points: f32,
    mirror: bool,
) {
    if !ui.input(|input| input.pointer.button_down(PointerButton::Primary)) {
        return;
    }
    if !response.hovered() && !response.dragged() {
        return;
    }
    let Some(part) = state.hair_project.part(part_id) else {
        return;
    };
    let Some((center, radius, _pointer)) =
        strand_cloud_hit(ui, viewport, camera, part, radius_points).or_else(|| {
            mirror
                .then(|| mirrored_strand_cloud_hit(ui, viewport, camera, part, radius_points))
                .flatten()
        })
    else {
        return;
    };
    let dt = ui.input(|input| input.stable_dt).clamp(0.0, 0.1);
    let strength = state.hair_brush_strength.clamp(0.05, 1.0);
    let radius_sq = radius * radius;
    let mut centers = vec![center];
    if mirror {
        centers.push([-center[0], center[1], center[2]]);
    }
    let mut cut = Vec::new();
    let mut cut_indices = std::collections::BTreeSet::new();
    for center in centers {
        for (&scalp_index, strand) in &part.strands {
            if cut_indices.contains(&scalp_index) {
                continue;
            }
            let Some(root) = strand.points_cm.first().copied() else {
                continue;
            };
            let mut nearest = None;
            for (index, point) in strand.points_cm.iter().enumerate() {
                let dx = point[0] - center[0];
                let dy = point[1] - center[1];
                let dz = point[2] - center[2];
                let distance = dx * dx + dy * dy + dz * dz;
                if distance <= radius_sq && nearest.is_none_or(|(_, best)| distance < best) {
                    nearest = Some((index, distance));
                }
            }
            let Some((index, _)) = nearest else {
                continue;
            };
            let total: f32 = strand
                .points_cm
                .windows(2)
                .map(|pair| {
                    let dx = pair[1][0] - pair[0][0];
                    let dy = pair[1][1] - pair[0][1];
                    let dz = pair[1][2] - pair[0][2];
                    (dx * dx + dy * dy + dz * dz).sqrt()
                })
                .sum();
            let last = strand.points_cm.len().saturating_sub(1).max(1) as f32;
            let mut keep = index as f32 / last;
            let floor = part.minimum_strand_length_cm();
            if total > 1.0e-4 {
                keep = keep.max((floor / total).min(1.0));
            }
            if keep >= 0.999 {
                continue;
            }
            let bite = (HAIR_CUT_RATE * strength * dt).min(1.0);
            let keep = 1.0 - (1.0 - keep) * bite;
            if keep >= 0.999 {
                continue;
            }
            let points = strand
                .points_cm
                .iter()
                .map(|point| {
                    [
                        root[0] + (point[0] - root[0]) * keep,
                        root[1] + (point[1] - root[1]) * keep,
                        root[2] + (point[2] - root[2]) * keep,
                    ]
                })
                .collect();
            cut.push((scalp_index, points));
            cut_indices.insert(scalp_index);
        }
    }
    if cut.is_empty() {
        return;
    }
    state.dispatch(Action::SetHairStrandPoints {
        part_id,
        strands: cut,
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "one brush call per active part, mirror riding along"
)]
fn handle_grow_brush(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
    part_id: u64,
    radius_points: f32,
    mirror: bool,
) {
    if !ui.input(|input| input.pointer.button_down(PointerButton::Primary)) {
        return;
    }
    if !response.hovered() && !response.dragged() {
        return;
    }
    let Some(part) = state.hair_project.part(part_id) else {
        return;
    };
    let Some((center, radius, _pointer)) =
        strand_cloud_hit(ui, viewport, camera, part, radius_points).or_else(|| {
            mirror
                .then(|| mirrored_strand_cloud_hit(ui, viewport, camera, part, radius_points))
                .flatten()
        })
    else {
        return;
    };
    let radius_sq = radius * radius;
    let mut centers = vec![center];
    if mirror {
        centers.push([-center[0], center[1], center[2]]);
    }
    let gathered: Vec<u32> = part
        .strands
        .iter()
        .filter(|(_, strand)| {
            strand.points_cm.iter().any(|point| {
                centers.iter().any(|center| {
                    let dx = point[0] - center[0];
                    let dy = point[1] - center[1];
                    let dz = point[2] - center[2];
                    dx * dx + dy * dy + dz * dz <= radius_sq
                })
            })
        })
        .map(|(index, _)| *index)
        .collect();
    if gathered.is_empty() {
        return;
    }
    let dt = ui.input(|input| input.stable_dt).clamp(0.0, 0.1);
    let shrink = crate::shortcuts::Shortcut::HairInvertHold.held(ui);
    let strength = state.hair_brush_strength.clamp(0.05, 1.0);
    let rate = (HAIR_GROW_RATE * strength * dt).exp();
    let factor = if shrink { 1.0 / rate } else { rate };
    state.dispatch(Action::ScaleHairStrands {
        part_id,
        scalp_indices: gathered,
        factor,
    });
}

const HAIR_GROW_RATE: f32 = 1.8;

const HAIR_CUT_RATE: f32 = 60.0;

const HAIR_SMOOTH_RATE: f32 = 6.0;

const HAIR_PINCH_RATE: f32 = 9.0;

const HAIR_PINCH_TIP_BIAS: i32 = 3;

const HAIR_PUFF_RATE: f32 = 14.0;
const HAIR_SMOOTH_PASSES: usize = 3;

pub(super) fn relax_bending(points: &mut [[f32; 3]], weights: &[f32], spacing: &[f32], rate: f32) {
    if points.len() < 3 {
        return;
    }
    let unit = |from: [f32; 3], to: [f32; 3]| -> Option<([f32; 3], f32)> {
        let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        let length = delta.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
        (length > 1.0e-6).then(|| {
            (
                [delta[0] / length, delta[1] / length, delta[2] / length],
                length,
            )
        })
    };

    let mut directions = Vec::with_capacity(points.len() - 1);
    let mut lengths = Vec::with_capacity(points.len() - 1);
    for index in 0..points.len() - 1 {
        match unit(points[index], points[index + 1]) {
            Some((direction, length)) => {
                directions.push(direction);
                lengths.push(length);
            }
            None => {
                directions.push([0.0, 1.0, 0.0]);
                lengths.push(0.0);
            }
        }
    }

    let before = directions.clone();
    let last = before.len() - 1;
    for (index, direction) in directions.iter_mut().enumerate() {
        let pull = rate * weights.get(index + 1).copied().unwrap_or(0.0);
        if pull <= 0.0 {
            continue;
        }
        let prev = before[index.saturating_sub(1)];
        let next = before[(index + 1).min(last)];
        let mut blended = [0.0_f32; 3];
        for (axis, blended) in blended.iter_mut().enumerate() {
            let average = (prev[axis] + before[index][axis] + next[axis]) / 3.0;
            *blended = before[index][axis] + (average - before[index][axis]) * pull;
        }
        let length = blended.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
        if length > 1.0e-6 {
            *direction = blended.map(|axis| axis / length);
        }
    }

    for index in 0..directions.len() {
        let step = spacing.get(index).copied().unwrap_or(lengths[index]);
        for axis in 0..3 {
            points[index + 1][axis] = points[index][axis] + directions[index][axis] * step;
        }
    }
}

const HAIR_COMB_RELAX_PASSES: usize = 24;

pub(super) fn relax_segment_lengths(points: &mut [[f32; 3]], spacing: &[f32]) {
    if points.len() < 2 {
        return;
    }
    relax_towards_rest(points, spacing);
    for index in 1..points.len() {
        let rest = spacing.get(index - 1).copied().unwrap_or(0.0);
        let delta = [
            points[index][0] - points[index - 1][0],
            points[index][1] - points[index - 1][1],
            points[index][2] - points[index - 1][2],
        ];
        let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
        if length <= 1e-6 {
            continue;
        }
        for axis in 0..3 {
            points[index][axis] = points[index - 1][axis] + delta[axis] / length * rest;
        }
    }
}

fn relax_towards_rest(points: &mut [[f32; 3]], spacing: &[f32]) {
    if points.len() < 2 {
        return;
    }
    for _ in 0..HAIR_COMB_RELAX_PASSES {
        for index in 1..points.len() {
            let rest = spacing.get(index - 1).copied().unwrap_or(0.0);
            let delta = [
                points[index][0] - points[index - 1][0],
                points[index][1] - points[index - 1][1],
                points[index][2] - points[index - 1][2],
            ];
            let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
            if length <= 1e-6 {
                points[index][1] += rest;
                continue;
            }
            let correction = (length - rest) / length;
            let (near, far) = if index == 1 { (0.0, 1.0) } else { (0.5, 0.5) };
            for axis in 0..3 {
                let shift = delta[axis] * correction;
                points[index - 1][axis] += shift * near;
                points[index][axis] -= shift * far;
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "one brush call per active part, sharing one stroke"
)]
fn handle_comb_brush(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
    part_id: u64,
    radius_points: f32,
    advance_stroke: bool,
    mirror: bool,
) {
    let stroke_id = Id::new(HAIR_COMB_STROKE_ID);
    let pressed = ui.input(|input| input.pointer.button_pressed(PointerButton::Primary));
    let down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));

    if pressed
        && response.hovered()
        && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
    {
        ui.data_mut(|data| {
            data.insert_temp(
                stroke_id,
                HairCombStroke {
                    last_pointer: pointer,
                },
            );
        });
    }

    if !down {
        ui.data_mut(|data| data.remove::<HairCombStroke>(stroke_id));
        return;
    }
    let Some(stroke) = ui.data_mut(|data| data.get_temp::<HairCombStroke>(stroke_id)) else {
        return;
    };
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return;
    };

    let Some(part) = state.hair_project.part(part_id) else {
        return;
    };
    let Some((center, radius, _)) = strand_cloud_hit(ui, viewport, camera, part, radius_points)
        .or_else(|| {
            mirror
                .then(|| mirrored_strand_cloud_hit(ui, viewport, camera, part, radius_points))
                .flatten()
        })
    else {
        if advance_stroke {
            let mut stroke = stroke;
            stroke.last_pointer = pointer;
            ui.data_mut(|data| data.insert_temp(stroke_id, stroke));
        }
        return;
    };
    let mirrored_center = [-center[0], center[1], center[2]];
    let falloff = |point: &[f32; 3], center: &[f32; 3]| -> f32 {
        let dx = point[0] - center[0];
        let dy = point[1] - center[1];
        let dz = point[2] - center[2];
        let distance = (dx * dx + dy * dy + dz * dz).sqrt();
        let inside = (1.0 - (distance / radius).min(1.0)).clamp(0.0, 1.0);
        inside * inside * (3.0 - 2.0 * inside)
    };
    let reach = radius * radius;
    let within = |point: &[f32; 3], center: &[f32; 3]| -> bool {
        let dx = point[0] - center[0];
        let dy = point[1] - center[1];
        let dz = point[2] - center[2];
        dx * dx + dy * dy + dz * dz <= reach
    };
    let mut captured = Vec::new();
    for (&scalp_index, strand) in &part.strands {
        let touched = strand
            .points_cm
            .iter()
            .skip(1)
            .any(|point| within(point, &center) || (mirror && within(point, &mirrored_center)));
        if !touched {
            continue;
        }
        let mut weights = Vec::with_capacity(strand.points_cm.len());
        let mut mirror_weights = Vec::with_capacity(strand.points_cm.len());
        let mut any = false;
        for (position, point) in strand.points_cm.iter().enumerate() {
            let (weight, mirrored) = if position == 0 {
                (0.0, 0.0)
            } else {
                (
                    falloff(point, &center),
                    if mirror {
                        falloff(point, &mirrored_center)
                    } else {
                        0.0
                    },
                )
            };
            if weight > 1e-4 || mirrored > 1e-4 {
                any = true;
            }
            weights.push(weight);
            mirror_weights.push(mirrored);
        }
        if !any {
            continue;
        }
        let spacing: Vec<f32> = strand
            .points_cm
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
        captured.push((
            scalp_index,
            strand.points_cm.clone(),
            weights,
            mirror_weights,
            spacing,
        ));
    }
    if captured.is_empty() {
        if advance_stroke {
            let mut stroke = stroke;
            stroke.last_pointer = pointer;
            ui.data_mut(|data| data.insert_temp(stroke_id, stroke));
        }
        return;
    }
    let center_world = center;

    let strength = state.hair_brush_strength.clamp(0.05, 1.0) * 2.0;
    let delta_points = pointer - stroke.last_pointer;

    let smoothing = crate::shortcuts::Shortcut::HairSmoothHold.held(ui);
    let inverting = crate::shortcuts::Shortcut::HairInvertHold.held(ui);
    let pinching = matches!(state.hair_project.active_tool, HairTool::Pinch) && !smoothing;
    let puffing = matches!(state.hair_project.active_tool, HairTool::Puff) && !smoothing;
    let flattening = puffing && inverting;
    let puff_rate = if puffing {
        let dt = ui.input(|input| input.stable_dt).clamp(0.0, 0.1);
        HAIR_PUFF_RATE * strength * dt * if flattening { -1.0 } else { 1.0 }
    } else {
        0.0
    };
    let spreading = pinching && inverting;
    let pinch_rate = if pinching {
        let dt = ui.input(|input| input.stable_dt).clamp(0.0, 0.1);
        (HAIR_PINCH_RATE * strength * dt).min(1.0) * if spreading { -1.0 } else { 1.0 }
    } else {
        0.0
    };
    let collider = if puffing { state.head_collider() } else { None };
    let mut strands = Vec::with_capacity(captured.len());
    for (scalp_index, captured, weights, mirror_weights, spacing) in &captured {
        let Some(points) = state
            .hair_project
            .part(part_id)
            .and_then(|part| part.strands.get(scalp_index))
            .map(|strand| strand.points_cm.clone())
        else {
            continue;
        };
        let base = captured;
        let mut points = points;
        if points.len() != base.len() {
            continue;
        }
        if puffing {
            for (index, point) in points.iter_mut().enumerate().skip(1) {
                let weight = weights
                    .get(index)
                    .copied()
                    .unwrap_or(0.0)
                    .max(mirror_weights.get(index).copied().unwrap_or(0.0));
                if weight <= 0.0 {
                    continue;
                }
                let Some(normal) = collider
                    .as_ref()
                    .and_then(|collider| collider.surface_normal(*point))
                else {
                    continue;
                };
                for (axis, coordinate) in point.iter_mut().enumerate() {
                    *coordinate += normal[axis] * puff_rate * weight;
                }
            }
        } else if pinching {
            let last = points.len().saturating_sub(1).max(1) as f32;
            for index in 0..points.len() {
                let along_strand = index as f32 / last;
                let bias = along_strand.powi(HAIR_PINCH_TIP_BIAS);
                let pull = pinch_rate * weights.get(index).copied().unwrap_or(0.0) * bias;
                let mirror_pull =
                    pinch_rate * mirror_weights.get(index).copied().unwrap_or(0.0) * bias;
                if pull == 0.0 && mirror_pull == 0.0 {
                    continue;
                }
                let ahead = points[(index + 1).min(points.len() - 1)];
                let behind = points[index.saturating_sub(1)];
                let direction = {
                    let raw = [
                        ahead[0] - behind[0],
                        ahead[1] - behind[1],
                        ahead[2] - behind[2],
                    ];
                    let length = raw.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
                    (length > 1.0e-6).then(|| raw.map(|axis| axis / length))
                };
                let mut combined = [0.0_f32; 3];
                for (target, pull) in [(center_world, pull), (mirrored_center, mirror_pull)] {
                    if pull == 0.0 {
                        continue;
                    }
                    let mut delta = [0.0_f32; 3];
                    for (axis, delta) in delta.iter_mut().enumerate() {
                        *delta = target[axis] - points[index][axis];
                    }
                    if let Some(direction) = direction {
                        let along: f32 = delta
                            .iter()
                            .zip(direction)
                            .map(|(delta, direction)| delta * direction)
                            .sum();
                        for (axis, delta) in delta.iter_mut().enumerate() {
                            *delta -= along * direction[axis];
                        }
                    }
                    for (axis, combined) in combined.iter_mut().enumerate() {
                        *combined += delta[axis] * pull;
                    }
                }
                let total = pull.abs() + mirror_pull.abs();
                if total <= 1.0e-6 {
                    continue;
                }
                let scale = pull.abs().max(mirror_pull.abs()) / total;
                for (axis, point) in points[index].iter_mut().enumerate() {
                    *point += combined[axis] * scale;
                }
            }
        } else if smoothing {
            let dt = ui.input(|input| input.stable_dt).clamp(0.0, 0.1);
            let rate = (HAIR_SMOOTH_RATE * strength * dt).min(1.0);
            let blended: Vec<f32> = weights
                .iter()
                .zip(mirror_weights)
                .map(|(weight, mirrored)| weight.max(*mirrored))
                .collect();
            for _ in 0..HAIR_SMOOTH_PASSES {
                relax_bending(&mut points, &blended, spacing, rate);
            }
            strands.push((*scalp_index, points));
            continue;
        } else {
            for index in 1..points.len() {
                let weight = weights[index];
                let mirrored = mirror_weights[index];
                if weight <= 0.0 && mirrored <= 0.0 {
                    continue;
                }
                let step = camera.world_drag_delta_at(
                    glam::Vec3::from_array(points[index]),
                    delta_points,
                    viewport.height(),
                ) * strength;
                let total = weight + mirrored;
                let peak = weight.max(mirrored);
                let balance = (weight - mirrored) / total;
                points[index] = [
                    points[index][0] + step.x * peak * balance,
                    points[index][1] + step.y * peak,
                    points[index][2] + step.z * peak,
                ];
            }
        }
        relax_segment_lengths(&mut points, spacing);
        strands.push((*scalp_index, points));
    }
    if advance_stroke {
        let mut stroke = stroke;
        stroke.last_pointer = pointer;
        ui.data_mut(|data| data.insert_temp(stroke_id, stroke));
    }
    if !strands.is_empty() {
        state.dispatch(Action::SetHairStrandPoints { part_id, strands });
    }
}
