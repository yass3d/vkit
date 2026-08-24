use egui::{Pos2, Rect, Response, Ui};
use glam::Vec3;

use crate::camera::TurntableCamera;
use crate::state::{Action, AppState};

const GRAB_POINTS: f32 = 14.0;

const MARKER_POINTS: f32 = 3.0;

const SELECTED_MARKER_POINTS: f32 = 5.0;

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
    field: &super::hair_overlays::HairDepthField,
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
    let delta = ui.input(|input| input.pointer.delta());
    if delta == egui::Vec2::ZERO {
        return;
    }
    let Some(centre) = selection_centre(state) else {
        return;
    };
    let shift = camera.world_drag_delta_at(centre, delta, viewport.height());
    move_selection(state, &joints, shift);
    ui.ctx().request_repaint();
}

pub(super) fn move_selection(state: &mut AppState, joints: &[Joint], shift: Vec3) {
    let placed: Vec<(Joint, Vec3)> = joints
        .iter()
        .map(|joint| (*joint, joint.at + shift))
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

pub(super) fn paint(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
) {
    let painter = ui
        .painter()
        .with_clip_rect(ui.clip_rect().intersect(viewport));
    let field = super::hair_overlays::hair_depth_field(ui, state, viewport, camera);
    let hovered = response
        .hovered()
        .then(|| ui.input(|input| input.pointer.hover_pos()))
        .flatten()
        .and_then(|pointer| joints_near(state, viewport, camera, pointer, GRAB_POINTS, &field));

    if let Some(joint) = hovered
        && let Some(seen) = camera.project(joint.at, viewport)
    {
        painter.rect_stroke(
            Rect::from_center_size(seen.screen, egui::Vec2::splat(MARKER_POINTS * 2.0)),
            1.0,
            egui::Stroke::new(1.5, crate::theme::COLOR_TEXT),
            egui::StrokeKind::Middle,
        );
    }

    for joint in selected_joints(state) {
        let Some(seen) = camera.project(joint.at, viewport) else {
            continue;
        };
        let marker =
            Rect::from_center_size(seen.screen, egui::Vec2::splat(SELECTED_MARKER_POINTS * 2.0));
        painter.rect_filled(marker, 1.0, crate::theme::COLOR_HAIR_POINT_ACTIVE);
        for (from, to) in [
            (marker.left_center(), marker.right_center()),
            (marker.center_top(), marker.center_bottom()),
        ] {
            painter.line_segment(
                [
                    from - (to - from).normalized() * 6.0,
                    to + (to - from).normalized() * 6.0,
                ],
                egui::Stroke::new(1.0, crate::theme::COLOR_HAIR_POINT_ACTIVE),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight(count: usize) -> Vec<[f32; 3]> {
        (0..count).map(|step| [0.0, step as f32, 0.0]).collect()
    }

    fn lengths(points: &[[f32; 3]]) -> Vec<f32> {
        points
            .windows(2)
            .map(|pair| (Vec3::from_array(pair[1]) - Vec3::from_array(pair[0])).length())
            .collect()
    }

    #[test]
    fn the_three_ways_a_click_changes_a_selection() {
        use std::collections::BTreeSet;

        let one = (1_u64, 2_u32, 3_usize);
        let two = (1, 2, 4);
        let mut selection = BTreeSet::new();

        apply_pick(&mut selection, Some(one), PickIntent::Replace);
        assert_eq!(selection, BTreeSet::from([one]));

        apply_pick(&mut selection, Some(two), PickIntent::Toggle);
        assert_eq!(selection, BTreeSet::from([one, two]), "shift adds");

        apply_pick(&mut selection, Some(two), PickIntent::Toggle);
        assert_eq!(selection, BTreeSet::from([one]), "and shift takes back");

        apply_pick(&mut selection, Some(two), PickIntent::Remove);
        assert_eq!(
            selection,
            BTreeSet::from([one]),
            "alt on an outsider is nothing"
        );

        apply_pick(&mut selection, Some(one), PickIntent::Remove);
        assert!(selection.is_empty(), "alt takes out what is in");

        apply_pick(&mut selection, Some(one), PickIntent::Replace);
        apply_pick(&mut selection, Some(two), PickIntent::Replace);
        assert_eq!(selection, BTreeSet::from([two]), "a plain click replaces");
    }

    #[test]
    fn an_empty_click_clears_only_when_it_is_a_plain_one() {
        use std::collections::BTreeSet;

        let held = BTreeSet::from([(1_u64, 2_u32, 3_usize)]);

        let mut selection = held.clone();
        apply_pick(&mut selection, None, PickIntent::Replace);
        assert!(selection.is_empty());

        for intent in [PickIntent::Toggle, PickIntent::Remove] {
            let mut selection = held.clone();
            apply_pick(&mut selection, None, intent);
            assert_eq!(selection, held, "{intent:?} on empty space keeps it");
        }
    }

    #[test]
    fn a_drag_moves_the_joint_it_took_hold_of_and_nothing_else() {
        let points = straight(6);
        let wanted = Vec3::new(9.0, 1.0, -4.0);
        let moved = move_strand_point(&points, 2, wanted).expect("a joint moves");

        assert_eq!(moved.len(), points.len());
        assert_eq!(moved[2], wanted.to_array(), "it goes where it was dragged");
        for index in (0..points.len()).filter(|index| *index != 2) {
            assert_eq!(
                moved[index], points[index],
                "point {index} is not the one that was grabbed",
            );
        }
    }

    #[test]
    fn the_tail_stays_where_it_was() {
        let points = straight(6);
        let moved = move_strand_point(&points, 2, Vec3::new(4.0, 2.0, 0.0)).expect("moves");
        for index in 3..points.len() {
            assert_eq!(moved[index], points[index], "point {index} followed along");
        }
    }

    #[test]
    fn the_segments_either_side_stretch_to_reach() {
        let points = straight(6);
        let before = lengths(&points);
        let moved = move_strand_point(&points, 2, Vec3::new(0.0, 40.0, 0.0)).expect("moves");
        let after = lengths(&moved);
        assert!(after[1] > before[1] * 2.0, "the segment above it grew");
        assert!(after[2] > before[2] * 2.0, "and so did the one below");
        for index in [0, 3, 4] {
            assert!(
                (before[index] - after[index]).abs() < 1.0e-4,
                "segment {index} touches neither side of the moved joint",
            );
        }
    }

    #[test]
    fn the_root_is_not_ours_to_move() {
        let points = straight(4);
        assert!(move_strand_point(&points, 0, Vec3::X).is_none());
        assert!(move_strand_point(&points, 9, Vec3::X).is_none());
    }
}
