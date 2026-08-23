//! One joint of one strand, taken hold of and moved.
//!
//! The brushes shape a crowd of strands at once, which is the wrong instrument
//! for a wave: a wave is a small number of deliberate bends. This picks a
//! single point joint, marks it, and lets it be dragged in the plane facing the
//! camera. The strand's segment lengths are kept, so a drag bends the hair
//! rather than stretching it.

use egui::{Pos2, Rect, Response, Ui};
use glam::Vec3;

use crate::camera::TurntableCamera;
use crate::state::{Action, AppState};

/// How near the pointer has to be, in points, to take hold of a joint.
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

/// Move one joint and carry the rest of the strand with it, keeping every
/// segment the length it already was.
///
/// The joint is first pulled back onto the sphere its parent segment allows, so
/// the segment entering the joint cannot stretch; then everything beyond the
/// joint moves by whatever the joint actually moved.
#[must_use]
/// Put one joint where the reader dragged it, and leave the rest alone.
///
/// It used to carry every joint below it by the same shift, holding the whole
/// tail rigid and keeping each segment the length it was. That is a pose tool,
/// not a vertex tool: taking hold of a joint halfway down a strand swung the
/// entire end of it, and there was no way to move one joint by itself. Every
/// other editor in the world moves the vertex you grabbed.
///
/// The segments either side stretch to reach, which is fine and is why the
/// length constraint went with the tail drag: the authored positions ARE the
/// rest lengths the solver is given, so a segment set longer stays longer
/// rather than being pulled back.
///
/// The root stays out of it — it is bound to the scalp.
pub fn move_strand_point(points: &[[f32; 3]], point: usize, wanted: Vec3) -> Option<Vec<[f32; 3]>> {
    if point == 0 || point >= points.len() {
        return None;
    }
    let mut moved: Vec<[f32; 3]> = points.to_vec();
    moved[point] = wanted.to_array();
    Some(moved)
}

/// The joint nearest the pointer that the reader can actually see.
///
/// `field` is the same far-side mask every hair overlay asks. Without it a
/// click in empty space could land on a joint behind the skull that happened to
/// project under the cursor — invisible, and then dragged. What is hidden is
/// not pickable, which is the only rule that matches what is on screen.
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
            // The root is bound to the scalp and is not ours to move.
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

/// Where every selected joint is right now.
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

/// The middle of the selection, which is where the gizmo stands.
pub(super) fn selection_centre(state: &AppState) -> Option<Vec3> {
    let joints = selected_joints(state);
    if joints.is_empty() {
        return None;
    }
    let sum: Vec3 = joints.iter().map(|joint| joint.at).sum();
    Some(sum / joints.len() as f32)
}

/// What a click does to the selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PickIntent {
    /// Plain click: this joint and nothing else.
    Replace,
    /// Shift: add it, or take it back out if it was already in.
    Toggle,
    /// Alt: take it out and leave the rest.
    Remove,
}

/// Fold one pick into a selection.
///
/// Split out from the input so it can be checked without a pointer: the rules
/// are small and they are the part a reader notices when they are wrong.
fn apply_pick(
    selection: &mut std::collections::BTreeSet<(u64, u32, usize)>,
    picked: Option<(u64, u32, usize)>,
    intent: PickIntent,
) {
    match (picked, intent) {
        // Clicking nothing clears, the way it does everywhere. Holding a
        // modifier means "adjust what I have", so an empty click keeps it.
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
    let field = super::hair_overlays::hair_depth_field(ui, state, viewport, camera);
    if response.drag_started() || (response.clicked() && !response.dragged()) {
        let intent = if crate::shortcuts::Shortcut::VertexAddToSelectionHold.held(ui) {
            PickIntent::Toggle
        } else if crate::shortcuts::Shortcut::VertexRemoveFromSelectionHold.held(ui) {
            PickIntent::Remove
        } else {
            PickIntent::Replace
        };
        let picked = joints_near(state, viewport, camera, pointer, GRAB_POINTS, &field)
            .map(|joint| (joint.part, joint.strand, joint.point));
        // A modifier click that lands on nothing must not start a drag either,
        // or the reader would haul the whole selection about by empty space.
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
    if !response.dragged() {
        if response.drag_stopped() {
            state.dispatch(Action::EndHairStroke);
        }
        return;
    }
    let delta = ui.input(|input| input.pointer.delta());
    if delta == egui::Vec2::ZERO {
        return;
    }
    // One screen delta, turned into world at the middle of the selection, so
    // every joint travels together rather than each by its own depth.
    let Some(centre) = selection_centre(state) else {
        return;
    };
    let shift = camera.world_drag_delta_at(centre, delta, viewport.height());
    move_selection(state, &joints, shift);
    ui.ctx().request_repaint();
}

/// Turn every selected joint about a point.
pub(super) fn turn_selection(
    state: &mut AppState,
    joints: &[Joint],
    about: Vec3,
    turn: glam::Quat,
) {
    let placed: Vec<(Joint, Vec3)> = joints
        .iter()
        .map(|joint| (*joint, about + turn * (joint.at - about)))
        .collect();
    move_placed(state, &placed);
}

/// Move every selected joint by one world shift, mirrors included.
pub(super) fn move_selection(state: &mut AppState, joints: &[Joint], shift: Vec3) {
    let placed: Vec<(Joint, Vec3)> = joints
        .iter()
        .map(|joint| (*joint, joint.at + shift))
        .collect();
    move_placed(state, &placed);
}

/// Put each named joint at the place given for it, mirrors included.
///
/// One writer for every way the joints move — a drag, an axis, a ring — so the
/// mirroring and the root rule are decided once rather than three times.
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
            // Through `move_strand_point` rather than writing the slot here:
            // that is where "one joint, and never the root" is decided, and it
            // should be decided once.
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

/// Mark what can be taken hold of, and what already is.
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
        // A short cross so the anchor reads as a handle rather than a dot.
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

    /// Plain click replaces, Shift adds and takes back, Alt only removes.
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

    /// Clicking empty space clears — but only when no modifier is down.
    ///
    /// A reader holding shift is saying "adjust what I have"; losing the lot
    /// because they missed a joint by two pixels is the opposite of that.
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

    /// Only the joint under the hand moves.
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

    /// The tail used to travel with the joint, which is a pose tool and not a
    /// vertex tool. Taking hold of a joint halfway down swung the whole end of
    /// the strand and there was no way to move one joint by itself.
    #[test]
    fn the_tail_stays_where_it_was() {
        let points = straight(6);
        let moved = move_strand_point(&points, 2, Vec3::new(4.0, 2.0, 0.0)).expect("moves");
        for index in 3..points.len() {
            assert_eq!(moved[index], points[index], "point {index} followed along");
        }
    }

    /// Segments stretch to reach, and that is deliberate: the authored
    /// positions are the rest lengths the solver is handed, so a segment set
    /// longer stays longer instead of being pulled back.
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
