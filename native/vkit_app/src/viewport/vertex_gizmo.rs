use egui::{Rect, Response, Ui};
use glam::Vec3;

use crate::camera::TurntableCamera;
use crate::state::AppState;
use crate::viewport::{AlignmentGizmoGeometry, AlignmentGizmoHit, GizmoHandles};

const DRAG_ID: &str = "vkit.viewport.hair.vertex-gizmo";

const SIZE_SHARE: f32 = 0.10;

const HANDLES: GizmoHandles = GizmoHandles::MOVE_ONLY;

#[derive(Clone, Copy, Debug)]
struct Grab {
    hit: AlignmentGizmoHit,
    previous: egui::Pos2,
}

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
            let along = (end - geometry.origin).normalized();
            let travel = (pointer - grab.previous).dot(along) * units as f32;
            let direction = [Vec3::X, Vec3::Y, Vec3::Z][axis];
            super::hair_vertex::move_selection(state, &joints, direction * travel);
        }
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
