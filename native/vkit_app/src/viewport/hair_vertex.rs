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
pub fn bend_strand(points: &[[f32; 3]], point: usize, wanted: Vec3) -> Option<Vec<[f32; 3]>> {
    if point == 0 || point >= points.len() {
        return None;
    }
    let at = |index: usize| Vec3::from_array(points[index]);
    let parent = at(point - 1);
    let reach = (at(point) - parent).length();
    let direction = (wanted - parent).try_normalize()?;
    let placed = parent + direction * reach;
    let shift = placed - at(point);
    let mut moved: Vec<[f32; 3]> = points.to_vec();
    for (index, slot) in moved.iter_mut().enumerate().skip(point) {
        *slot = (at(index) + shift).to_array();
    }
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
    let Some(moved) = bend_strand(&strand.points_cm, joint.point, wanted) else {
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
        if let Some(moved) = bend_strand(&other.points_cm, joint.point, mirrored) {
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

    #[test]
    fn a_bend_keeps_every_segment_the_length_it_was() {
        let points = straight(6);
        let before = lengths(&points);
        let moved = bend_strand(&points, 2, Vec3::new(9.0, 1.0, -4.0)).expect("a joint bends");
        let after = lengths(&moved);
        assert_eq!(before.len(), after.len());
        for (index, (was, now)) in before.iter().zip(&after).enumerate() {
            assert!(
                (was - now).abs() < 1.0e-4,
                "segment {index} went from {was} to {now}: a drag must bend hair, not stretch it",
            );
        }
    }

    #[test]
    fn everything_below_the_joint_stays_where_it_was() {
        let points = straight(6);
        let moved = bend_strand(&points, 3, Vec3::new(5.0, 3.0, 5.0)).expect("a joint bends");
        for index in 0..3 {
            assert_eq!(
                moved[index], points[index],
                "point {index} is above the joint and must not move",
            );
        }
        assert_ne!(moved[3], points[3]);
    }

    #[test]
    fn the_tail_travels_with_the_joint_rather_than_trailing_behind() {
        let points = straight(6);
        let moved = bend_strand(&points, 2, Vec3::new(4.0, 2.0, 0.0)).expect("a joint bends");
        let shift = Vec3::from_array(moved[2]) - Vec3::from_array(points[2]);
        for index in 3..points.len() {
            let travelled = Vec3::from_array(moved[index]) - Vec3::from_array(points[index]);
            assert!(
                (travelled - shift).length() < 1.0e-4,
                "point {index} moved {travelled:?} where the joint moved {shift:?}",
            );
        }
    }

    #[test]
    fn the_root_is_not_ours_to_move() {
        let points = straight(4);
        assert!(bend_strand(&points, 0, Vec3::X).is_none());
        assert!(bend_strand(&points, 9, Vec3::X).is_none());
    }
}
