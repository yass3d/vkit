//! A move-and-rotate handle at the middle of the selected strand joints.
//!
//! The same handle the alignment tab uses, standing somewhere else. Dragging a
//! joint directly is fine for one; past that a reader wants an axis to push
//! along and a ring to turn about, and wants the two separated so a nudge does
//! not also twist.
//!
//! No scale. A strand's joints are spaced by the lengths the solver is handed
//! as rest lengths, and pulling them apart uniformly is not an edit anybody
//! reached for — the alignment tab scales because it is fitting one mesh to
//! another, which this is not.

use egui::{Rect, Response, Ui};
use glam::Vec3;

use crate::camera::TurntableCamera;
use crate::state::AppState;
use crate::viewport::{AlignmentGizmoGeometry, AlignmentGizmoHit};

/// What the reader took hold of, held between frames.
const DRAG_ID: &str = "vkit.viewport.hair.vertex-gizmo";

/// How far the handle reaches, as a share of the camera's framing radius.
///
/// A share and not a length: a selection of two joints a centimetre apart still
/// needs a handle big enough to grab, and one across the whole head must not
/// grow one that fills the screen.
const SIZE_SHARE: f32 = 0.10;

#[derive(Clone, Copy, Debug)]
struct Grab {
    hit: AlignmentGizmoHit,
    /// Where the pointer was last frame, in screen points.
    previous: egui::Pos2,
}

/// Where the handle stands, or `None` when nothing is selected.
#[must_use]
pub(super) fn geometry(
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
) -> Option<AlignmentGizmoGeometry> {
    let centre = super::hair_vertex::selection_centre(state)?;
    crate::viewport::gizmo_geometry_at(centre, camera.frame_radius * SIZE_SHARE, viewport, camera)
}

fn held(ui: &Ui) -> Option<Grab> {
    ui.data(|data| data.get_temp::<Grab>(egui::Id::new(DRAG_ID)))
}

/// Take the handle, move what it holds, and let go.
///
/// Returns `true` while the handle has the pointer, so the caller leaves the
/// joints and the camera alone for the rest of the frame.
pub(super) fn handle(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
) -> bool {
    let id = egui::Id::new(DRAG_ID);
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return false;
    };
    let Some(geometry) = geometry(state, viewport, camera) else {
        ui.data_mut(|data| data.remove::<Grab>(id));
        return false;
    };

    if let Some(grab) = held(ui) {
        if !response.dragged() {
            ui.data_mut(|data| data.remove::<Grab>(id));
            state.dispatch(crate::state::Action::EndHairStroke);
            return true;
        }
        apply(state, &geometry, grab, pointer);
        ui.data_mut(|data| {
            data.insert_temp(
                id,
                Grab {
                    previous: pointer,
                    ..grab
                },
            );
        });
        ui.ctx().request_repaint();
        return true;
    }

    if response.drag_started()
        && let Some(hit) = crate::viewport::gizmo_hit(pointer, &geometry, false)
    {
        ui.data_mut(|data| {
            data.insert_temp(
                id,
                Grab {
                    hit,
                    previous: pointer,
                },
            );
        });
        return true;
    }
    false
}

fn apply(state: &mut AppState, geometry: &AlignmentGizmoGeometry, grab: Grab, pointer: egui::Pos2) {
    let joints = super::hair_vertex::selected_joints(state);
    if joints.is_empty() {
        return;
    }
    match grab.hit {
        AlignmentGizmoHit::Move(axis) => {
            let Some(end) = geometry.axis_ends[axis] else {
                return;
            };
            let Some(units) = geometry.axis_world_units_per_point[axis] else {
                return;
            };
            // The pointer's travel along the axis as it appears on screen, in
            // the world units that axis is drawn at. Anything across the axis
            // is the reader's hand wobbling and is not a move.
            let along = (end - geometry.origin).normalized();
            let travel = (pointer - grab.previous).dot(along) * units as f32;
            let direction = [Vec3::X, Vec3::Y, Vec3::Z][axis];
            super::hair_vertex::move_selection(state, &joints, direction * travel);
        }
        AlignmentGizmoHit::Rotate(axis) => {
            let degrees =
                crate::viewport::drag_swept_degrees(grab.previous, pointer, geometry.origin);
            if degrees.abs() < f32::EPSILON {
                return;
            }
            let axis = [Vec3::X, Vec3::Y, Vec3::Z][axis];
            let turn = glam::Quat::from_axis_angle(axis, degrees.to_radians());
            super::hair_vertex::turn_selection(state, &joints, geometry.world_center, turn);
        }
        // Asked for with `false`, so it cannot come back.
        AlignmentGizmoHit::Scale => {}
    }
}

pub(super) fn paint(ui: &Ui, state: &AppState, viewport: Rect, camera: TurntableCamera) {
    let Some(geometry) = geometry(state, viewport, camera) else {
        return;
    };
    crate::viewport::paint_gizmo_geometry(ui, &geometry, false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn geometry_at(centre: Vec3) -> AlignmentGizmoGeometry {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let camera = TurntableCamera {
            target: centre,
            ..TurntableCamera::default()
        };
        crate::viewport::gizmo_geometry_at(
            centre,
            camera.frame_radius * SIZE_SHARE,
            viewport,
            camera,
        )
        .expect("a gizmo in front of the camera")
    }

    /// The centre answers nothing, so a click there is not a scale.
    ///
    /// Strand joints are spaced by the lengths the solver is handed as rest
    /// lengths; pulling them apart uniformly is not an edit anybody reached for.
    #[test]
    fn the_middle_of_this_handle_is_not_a_scale_grip() {
        let geometry = geometry_at(Vec3::ZERO);
        assert!(
            matches!(
                crate::viewport::gizmo_hit(geometry.scale_handle, &geometry, true),
                Some(AlignmentGizmoHit::Scale)
            ),
            "the alignment tab still has one",
        );
        assert!(
            !matches!(
                crate::viewport::gizmo_hit(geometry.scale_handle, &geometry, false),
                Some(AlignmentGizmoHit::Scale)
            ),
            "and this one does not",
        );
    }

    /// It stands where the selection is, and it is the same size wherever that
    /// is — the reach is a share of the framing, not of what is selected.
    #[test]
    fn the_handle_follows_the_selection_without_changing_size() {
        let here = geometry_at(Vec3::ZERO);
        let there = geometry_at(Vec3::new(0.0, 6.0, 0.0));
        assert!(
            (here.world_center - Vec3::ZERO).length() < 1.0e-4,
            "it stands where it was asked to",
        );
        assert!((there.world_center.y - 6.0).abs() < 1.0e-4);

        let reach = |geometry: &AlignmentGizmoGeometry| {
            geometry.axis_ends[1].map(|end| end.distance(geometry.origin))
        };
        if let (Some(near), Some(far)) = (reach(&here), reach(&there)) {
            assert!(
                (near - far).abs() < near * 0.25,
                "the handle grew from {near} to {far} for the same framing",
            );
        }
    }
}
