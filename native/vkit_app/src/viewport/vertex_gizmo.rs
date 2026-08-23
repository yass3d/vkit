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
use crate::viewport::{AlignmentGizmoGeometry, AlignmentGizmoHit, GizmoHandles};

/// What the reader took hold of, held between frames.
const DRAG_ID: &str = "vkit.viewport.hair.vertex-gizmo";

/// How far the handle reaches, as a share of the camera's framing radius.
///
/// A share and not a length: a selection of two joints a centimetre apart still
/// needs a handle big enough to grab, and one across the whole head must not
/// grow one that fills the screen.
const SIZE_SHARE: f32 = 0.10;

/// Move, and nothing else.
///
/// A strand joint is a point. It has no orientation to turn about and no
/// spacing to stretch, so a ring and a centre grip would be two controls that
/// answer questions nobody asked.
const HANDLES: GizmoHandles = GizmoHandles::MOVE_ONLY;

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

    let editing = response.dragged_by(egui::PointerButton::Primary);
    if let Some(grab) = held(ui) {
        if !editing {
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
        && editing
        && let Some(hit) = crate::viewport::gizmo_hit(pointer, &geometry, HANDLES)
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
        // `HANDLES` offers neither, so neither is drawn and neither can be
        // grabbed. They stay in the match because the enum is the alignment
        // tab's too, and that tab has both.
        AlignmentGizmoHit::Rotate(_) | AlignmentGizmoHit::Scale => {}
    }
}

pub(super) fn paint(ui: &Ui, state: &AppState, viewport: Rect, camera: TurntableCamera) {
    let Some(geometry) = geometry(state, viewport, camera) else {
        return;
    };
    crate::viewport::paint_gizmo_geometry(ui, &geometry, HANDLES);
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

    /// This handle moves and does nothing else.
    ///
    /// A strand joint is a point: no orientation to turn about, no spacing to
    /// stretch. A ring and a centre grip would be two controls that answer
    /// questions nobody asked, so neither is drawn and neither answers — one
    /// flag decides both, which is why they cannot disagree.
    #[test]
    fn this_handle_offers_neither_a_ring_nor_a_scale_grip() {
        let geometry = geometry_at(Vec3::ZERO);

        assert!(
            matches!(
                crate::viewport::gizmo_hit(
                    geometry.scale_handle,
                    &geometry,
                    crate::viewport::GizmoHandles::ALL
                ),
                Some(AlignmentGizmoHit::Scale)
            ),
            "the alignment tab still has a scale grip",
        );
        assert!(
            !matches!(
                crate::viewport::gizmo_hit(geometry.scale_handle, &geometry, HANDLES),
                Some(AlignmentGizmoHit::Scale)
            ),
            "and this one has no scale grip — the centre is just where the axes meet",
        );

        // Somewhere on a ring and clear of every axis.
        let on_a_ring = geometry.rings[0]
            .iter()
            .copied()
            .find(|point| {
                geometry
                    .axis_ends
                    .iter()
                    .flatten()
                    .all(|end| point.distance(*end) > 24.0)
                    && point.distance(geometry.origin) > 24.0
            })
            .expect("a ring point away from the axes");
        assert!(
            matches!(
                crate::viewport::gizmo_hit(
                    on_a_ring,
                    &geometry,
                    crate::viewport::GizmoHandles::ALL
                ),
                Some(AlignmentGizmoHit::Rotate(_))
            ),
            "the alignment tab still turns",
        );
        assert!(
            crate::viewport::gizmo_hit(on_a_ring, &geometry, HANDLES).is_none(),
            "and this one does not, because it does not draw a ring to turn",
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
