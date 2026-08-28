use egui::{Pos2, Rect, Response, Ui};
use glam::Vec3;

use crate::camera::TurntableCamera;
use crate::state::{Action, AppState};

const GRAB_POINTS: f32 = 14.0;

fn joint_span(state: &AppState, joint: Joint) -> f32 {
    let Some(strand) = state
        .hair_project
        .part(joint.part)
        .and_then(|part| part.strands.get(&joint.strand))
    else {
        return 1.0;
    };
    let neighbour = if joint.point > 0 { joint.point - 1 } else { 1 };
    strand
        .points_cm
        .get(neighbour)
        .map(|other| (Vec3::from_array(*other) - joint.at).length())
        .unwrap_or(1.0)
}

fn handle_size() -> crate::viewport::marker_size::MarkerSize {
    crate::viewport::marker_size::MarkerSize::new(0.25, 1.4..=5.5)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Joint {
    part: u64,
    strand: u32,
    point: usize,
    at: Vec3,
}

#[must_use]
pub fn move_strand_point(points: &[[f32; 3]], point: usize, wanted: Vec3) -> Option<Vec<[f32; 3]>> {
    if point == 0 || point >= points.len() {
        return None;
    }
    let mut moved: Vec<[f32; 3]> = points.to_vec();
    moved[point] = wanted.to_array();
    Some(moved)
}

fn joints_near(
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
    pointer: Pos2,
    reach_points: f32,
    field: &super::surface_depth::SurfaceDepth,
) -> Option<Joint> {
    let eye = camera.eye();
    let mut best: Option<(f32, Joint)> = None;
    for part_id in state.hair_project.editable_parts() {
        let Some(part) = state.hair_project.part(part_id) else {
            continue;
        };
        for (strand_index, strand) in &part.strands {
            for (point, position) in strand.points_cm.iter().enumerate().skip(1) {
                let at = Vec3::from_array(*position);
                let Some(seen) = camera.project(at, viewport) else {
                    continue;
                };
                let reach = seen.screen.distance(pointer);
                if reach > reach_points || best.is_some_and(|(closest, _)| reach >= closest) {
                    continue;
                }
                if field.hides(seen.screen, (at - eye).length()) {
                    continue;
                }
                best = Some((
                    reach,
                    Joint {
                        part: part_id,
                        strand: *strand_index,
                        point,
                        at,
                    },
                ));
            }
        }
    }
    best.map(|(_, joint)| joint)
}

pub(super) fn selected_joints(state: &AppState) -> Vec<Joint> {
    state
        .hair_vertex_selection
        .iter()
        .filter_map(|&(part, strand, point)| {
            let position = state
                .hair_project
                .part(part)?
                .strands
                .get(&strand)?
                .points_cm
                .get(point)?;
            Some(Joint {
                part,
                strand,
                point,
                at: Vec3::from_array(*position),
            })
        })
        .collect()
}

pub(super) fn selection_centre(state: &AppState) -> Option<Vec3> {
    let joints = selected_joints(state);
    if joints.is_empty() {
        return None;
    }
    let sum: Vec3 = joints.iter().map(|joint| joint.at).sum();
    Some(sum / joints.len() as f32)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PickIntent {
    Replace,
    Toggle,
    Remove,
}

fn apply_pick(
    selection: &mut std::collections::BTreeSet<(u64, u32, usize)>,
    picked: Option<(u64, u32, usize)>,
    intent: PickIntent,
) {
    match (picked, intent) {
        (None, PickIntent::Replace) => selection.clear(),
        (None, _) => {}
        (Some(joint), PickIntent::Replace) => {
            selection.clear();
            selection.insert(joint);
        }
        (Some(joint), PickIntent::Toggle) => {
            if !selection.remove(&joint) {
                selection.insert(joint);
            }
        }
        (Some(joint), PickIntent::Remove) => {
            selection.remove(&joint);
        }
    }
}

pub(super) fn select_connected(state: &mut AppState) {
    let strands: std::collections::BTreeSet<(u64, u32)> = state
        .hair_vertex_selection
        .iter()
        .map(|(part, strand, _)| (*part, *strand))
        .collect();
    if strands.is_empty() {
        return;
    }
    let mut mask: std::collections::BTreeMap<u64, std::collections::BTreeSet<u32>> =
        std::collections::BTreeMap::new();
    for (part_id, strand_id) in strands {
        let Some(points) = state
            .hair_project
            .part(part_id)
            .and_then(|part| part.strands.get(&strand_id))
            .map(|strand| strand.points_cm.len())
        else {
            continue;
        };
        for point in 1..points {
            state
                .hair_vertex_selection
                .insert((part_id, strand_id, point));
        }
        mask.entry(part_id).or_default().insert(strand_id);
    }
    state.hair_strand_mask = mask;
}

pub(super) fn clear_strand_mask(state: &mut AppState) {
    state.hair_strand_mask.clear();
}

pub(super) fn handle(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
) {
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return;
    };
    let editing = response.dragged_by(egui::PointerButton::Primary);
    let field = super::hair_overlays::hair_depth_field(ui, state, viewport, camera);
    if (response.drag_started() && editing) || (response.clicked() && !response.dragged()) {
        let intent = if crate::shortcuts::Shortcut::VertexAddToSelectionHold.held(ui) {
            PickIntent::Toggle
        } else if crate::shortcuts::Shortcut::VertexRemoveFromSelectionHold.held(ui) {
            PickIntent::Remove
        } else {
            PickIntent::Replace
        };
        let picked = joints_near(state, viewport, camera, pointer, GRAB_POINTS, &field)
            .map(|joint| (joint.part, joint.strand, joint.point));
        let dragging_from_a_joint = picked.is_some();
        apply_pick(&mut state.hair_vertex_selection, picked, intent);
        if !dragging_from_a_joint {
            if intent == PickIntent::Replace {
                clear_strand_mask(state);
            }
            return;
        }
    }
    let joints = selected_joints(state);
    if joints.is_empty() {
        return;
    }
    if !editing {
        if response.drag_stopped() {
            state.dispatch(Action::EndHairStroke);
        }
        return;
    }
    let Some(grab) = drag_anchor(ui, state, viewport, camera, response) else {
        return;
    };
    let Some(now) = plane_hit(camera, viewport, pointer, grab) else {
        return;
    };
    let shift = now - grab.at;
    if shift.length_squared() <= 1.0e-12 {
        return;
    }
    move_selection(state, &joints, shift);
    ui.data_mut(|data| data.insert_temp(egui::Id::new(DRAG_ID), Grab { at: now, ..grab }));
    ui.ctx().request_repaint();
}

const DRAG_ID: &str = "vkit.viewport.hair.vertex-drag";

#[derive(Clone, Copy, Debug)]
struct Grab {
    plane_point: Vec3,
    plane_normal: Vec3,
    at: Vec3,
}

fn drag_anchor(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
    response: &Response,
) -> Option<Grab> {
    let id = egui::Id::new(DRAG_ID);
    if response.drag_stopped() {
        ui.data_mut(|data| data.remove::<Grab>(id));
        return None;
    }
    if let Some(grab) = ui.data(|data| data.get_temp::<Grab>(id)) {
        return Some(grab);
    }
    let pointer = ui.input(|input| input.pointer.interact_pos())?;
    let centre = selection_centre(state)?;
    let (forward, _, _) = camera.basis();
    let grab = Grab {
        plane_point: centre,
        plane_normal: forward,
        at: plane_hit_on(camera, viewport, pointer, centre, forward)?,
    };
    ui.data_mut(|data| data.insert_temp(id, grab));
    Some(grab)
}

fn plane_hit(camera: TurntableCamera, viewport: Rect, pointer: Pos2, grab: Grab) -> Option<Vec3> {
    plane_hit_on(
        camera,
        viewport,
        pointer,
        grab.plane_point,
        grab.plane_normal,
    )
}

fn plane_hit_on(
    camera: TurntableCamera,
    viewport: Rect,
    pointer: Pos2,
    plane_point: Vec3,
    plane_normal: Vec3,
) -> Option<Vec3> {
    let ray = camera.ray_from_screen(pointer, viewport)?;
    let origin = Vec3::new(
        ray.origin.x as f32,
        ray.origin.y as f32,
        ray.origin.z as f32,
    );
    let direction = Vec3::new(
        ray.direction.x as f32,
        ray.direction.y as f32,
        ray.direction.z as f32,
    );
    let slope = direction.dot(plane_normal);
    if slope.abs() < 1.0e-6 {
        return None;
    }
    let travel = (plane_point - origin).dot(plane_normal) / slope;
    if !travel.is_finite() {
        return None;
    }
    Some(origin + direction * travel)
}

pub(super) fn move_selection(state: &mut AppState, joints: &[Joint], shift: Vec3) {
    let placed: Vec<(Joint, Vec3)> = joints
        .iter()
        .map(|joint| (*joint, joint.at + shift))
        .collect();
    move_placed(state, &placed);
}

pub(super) fn transform_selection(
    state: &mut AppState,
    joints: &[Joint],
    pivot: Vec3,
    basis: glam::Mat3,
) {
    if joints.len() < 2 {
        return;
    }
    let placed: Vec<(Joint, Vec3)> = joints
        .iter()
        .map(|joint| (*joint, pivot + basis.mul_vec3(joint.at - pivot)))
        .collect();
    move_placed(state, &placed);
}

fn move_placed(state: &mut AppState, placed: &[(Joint, Vec3)]) {
    use std::collections::BTreeMap;

    let mut wanted: BTreeMap<u64, BTreeMap<u32, Vec<(usize, Vec3)>>> = BTreeMap::new();
    for (joint, to) in placed {
        wanted
            .entry(joint.part)
            .or_default()
            .entry(joint.strand)
            .or_default()
            .push((joint.point, *to));
    }

    for (part_id, by_strand) in wanted {
        let Some(part) = state.hair_project.part(part_id) else {
            continue;
        };
        let mirror = state
            .hair_mirror_edit
            .then(|| state.posed_hair_scalps.get(&part.provider_name))
            .flatten();
        let mut strands = Vec::new();
        for (strand_id, points) in by_strand {
            let Some(strand) = part.strands.get(&strand_id) else {
                continue;
            };
            let mut moved = strand.points_cm.clone();
            for (point, to) in &points {
                if let Some(next) = move_strand_point(&moved, *point, *to) {
                    moved = next;
                }
            }
            strands.push((strand_id, moved));

            if let Some(scalp) = mirror
                && let Some(&pair) = scalp.mirror_pair.get(strand_id as usize)
                && pair != strand_id
                && let Some(other) = part.strands.get(&pair)
            {
                let mut mirrored = other.points_cm.clone();
                for (point, to) in &points {
                    let across = Vec3::new(-to.x, to.y, to.z);
                    if let Some(next) = move_strand_point(&mirrored, *point, across) {
                        mirrored = next;
                    }
                }
                strands.push((pair, mirrored));
            }
        }
        if !strands.is_empty() {
            state.dispatch(Action::SetHairStrandPoints { part_id, strands });
        }
    }
}

pub(super) fn handles(
    state: &AppState,
    into: &mut Vec<crate::renderer::MarkerInstance>,
    scale: f32,
    sizing: Option<(TurntableCamera, egui::Rect)>,
) {
    let rgba = |colour: egui::Color32| {
        let [r, g, b, a] = colour.to_array();
        [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            f32::from(a) / 255.0,
        ]
    };
    let size = handle_size();
    for joint in selected_joints(state) {
        let radius = match sizing {
            Some((camera, viewport)) => {
                size.points(camera, viewport, joint.at, joint_span(state, joint))
            }
            None => size.smallest(),
        };
        into.push(crate::renderer::MarkerInstance {
            position: joint.at.to_array(),
            shape: crate::renderer::MarkerInstance::ROUND,
            radius: radius * scale,
            fill: rgba(crate::theme::COLOR_HAIR_POINT_ACTIVE),
            ring: rgba(crate::theme::COLOR_BG.gamma_multiply(0.6)),
        });
    }
}
