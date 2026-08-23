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
struct Joint {
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

fn joints_near(
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
    pointer: Pos2,
    reach_points: f32,
) -> Option<Joint> {
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

fn selected_joint(state: &AppState) -> Option<Joint> {
    let (part, strand, point) = state.hair_vertex_selection?;
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
    if response.drag_started() || (response.clicked() && !response.dragged()) {
        state.hair_vertex_selection = joints_near(state, viewport, camera, pointer, GRAB_POINTS)
            .map(|joint| (joint.part, joint.strand, joint.point));
    }
    let Some(joint) = selected_joint(state) else {
        return;
    };
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
    let wanted = joint.at + camera.world_drag_delta_at(joint.at, delta, viewport.height());
    let Some(part) = state.hair_project.part(joint.part) else {
        return;
    };
    let Some(strand) = part.strands.get(&joint.strand) else {
        return;
    };
    let Some(moved) = move_strand_point(&strand.points_cm, joint.point, wanted) else {
        return;
    };
    let mut strands = vec![(joint.strand, moved)];
    if state.hair_mirror_edit
        && let Some(scalp) = state.posed_hair_scalps.get(&part.provider_name)
        && let Some(&pair) = scalp.mirror_pair.get(joint.strand as usize)
        && pair != joint.strand
        && let Some(other) = part.strands.get(&pair)
    {
        let mirrored = Vec3::new(-wanted.x, wanted.y, wanted.z);
        if let Some(moved) = move_strand_point(&other.points_cm, joint.point, mirrored) {
            strands.push((pair, moved));
        }
    }
    state.dispatch(Action::SetHairStrandPoints {
        part_id: joint.part,
        strands,
    });
    ui.ctx().request_repaint();
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
    let hovered = response
        .hovered()
        .then(|| ui.input(|input| input.pointer.hover_pos()))
        .flatten()
        .and_then(|pointer| joints_near(state, viewport, camera, pointer, GRAB_POINTS));

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

    let Some(joint) = selected_joint(state) else {
        return;
    };
    let Some(seen) = camera.project(joint.at, viewport) else {
        return;
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
