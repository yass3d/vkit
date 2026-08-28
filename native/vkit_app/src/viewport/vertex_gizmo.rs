use egui::{Rect, Response, Ui};
use glam::Vec3;

use crate::camera::TurntableCamera;
use crate::state::AppState;
use crate::viewport::{
    AlignmentGizmoGeometry, AlignmentGizmoHit, GizmoHandles, plane_axes, polyline_parameter,
    wrapped_angle_delta,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VertexOwner {
    Hair,
    Sculpt,
}

impl VertexOwner {
    fn drag_id(self) -> egui::Id {
        egui::Id::new(match self {
            Self::Hair => "vkit.viewport.hair.vertex-gizmo",
            Self::Sculpt => "vkit.viewport.sculpt.vertex-gizmo",
        })
    }

    fn selection_size(self, state: &AppState) -> usize {
        match self {
            Self::Hair => state.hair_vertex_selection.len(),
            Self::Sculpt => state.sculpt_vertex_selection.len(),
        }
    }

    fn centre(self, state: &AppState) -> Option<Vec3> {
        match self {
            Self::Hair => super::hair_vertex::selection_centre(state),
            Self::Sculpt => super::sculpt_vertex::selection_centre(state),
        }
    }

    fn move_by(self, state: &mut AppState, shift: Vec3) {
        match self {
            Self::Hair => {
                let joints = super::hair_vertex::selected_joints(state);
                if !joints.is_empty() {
                    super::hair_vertex::move_selection(state, &joints, shift);
                }
            }
            Self::Sculpt => state.dispatch(crate::state::Action::MoveSculptVertices {
                shift: shift.to_array().map(f64::from),
            }),
        }
    }

    fn transform_by(self, state: &mut AppState, pivot: Vec3, basis: glam::Mat3) {
        match self {
            Self::Hair => {
                let joints = super::hair_vertex::selected_joints(state);
                super::hair_vertex::transform_selection(state, &joints, pivot, basis);
            }
            Self::Sculpt => state.dispatch(crate::state::Action::TransformSculptVertices {
                pivot: pivot.to_array().map(f64::from),
                basis: [basis.row(0), basis.row(1), basis.row(2)]
                    .map(|row| row.to_array().map(f64::from)),
            }),
        }
    }

    fn begin(self, state: &mut AppState) {
        if self == Self::Sculpt {
            state.dispatch(crate::state::Action::BeginSculptStroke {
                view_direction_local: None,
                brush_direction_local: None,
            });
        }
    }

    fn end(self, state: &mut AppState) {
        state.dispatch(match self {
            Self::Hair => crate::state::Action::EndHairStroke,
            Self::Sculpt => crate::state::Action::EndSculptStroke,
        });
    }
}

const SIZE_SHARE: f32 = 0.10;

fn handles(owner: VertexOwner, state: &AppState) -> GizmoHandles {
    if owner.selection_size(state) >= 2 {
        GizmoHandles::ALL
    } else {
        GizmoHandles::MOVE_ONLY
    }
}

#[derive(Clone, Copy, Debug)]
struct Grab {
    hit: AlignmentGizmoHit,
    previous: egui::Pos2,

    ring_parameter: f32,

    reach: f32,
}

#[must_use]
pub(super) fn geometry(
    owner: VertexOwner,
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
) -> Option<AlignmentGizmoGeometry> {
    let centre = owner.centre(state)?;
    crate::viewport::gizmo_geometry_at(centre, camera.frame_radius * SIZE_SHARE, viewport, camera)
}

fn held(ui: &Ui, owner: VertexOwner) -> Option<Grab> {
    ui.data(|data| data.get_temp::<Grab>(owner.drag_id()))
}

fn handle_rotate_mode(
    owner: VertexOwner,
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    camera: TurntableCamera,
) -> bool {
    let id = owner.drag_id().with("rotate-mode");
    let update = crate::sweep_gesture::handle_sweep(
        ui,
        id,
        crate::shortcuts::Shortcut::VertexRotate,
        viewport,
        0.0,
        0.0,
        None,
    );
    let armed = crate::sweep_gesture::sweep_active(ui, id);
    let mode = if armed {
        crate::camera_control::ControlMode::VertexRotate
    } else {
        crate::camera_control::ControlMode::Orbit
    };
    if state.camera_control != mode {
        state.dispatch(crate::state::Action::SetCameraControl(mode));
    }
    if !armed {
        return update.consumed || update.finished;
    }

    let motion = ui.input(|input| input.pointer.delta());
    if motion.x != 0.0
        && let Some(centre) = owner.centre(state)
    {
        let forward = (camera.target - camera.eye()).normalize_or_zero();
        if forward != Vec3::ZERO {
            owner.begin(state);
            owner.transform_by(
                state,
                centre,
                glam::Mat3::from_axis_angle(forward, motion.x * ROTATE_RADIANS_PER_POINT),
            );
            owner.end(state);
        }
    }
    ui.ctx().request_repaint();
    true
}

const ROTATE_RADIANS_PER_POINT: f32 = 0.010;

pub(super) fn handle(
    owner: VertexOwner,
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
) -> bool {
    if handle_rotate_mode(owner, ui, state, viewport, camera) {
        return true;
    }
    let id = owner.drag_id();
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return false;
    };
    let Some(geometry) = geometry(owner, state, viewport, camera) else {
        ui.data_mut(|data| data.remove::<Grab>(id));
        return false;
    };

    let editing = response.dragged_by(egui::PointerButton::Primary);
    if let Some(grab) = held(ui, owner) {
        if !editing {
            ui.data_mut(|data| data.remove::<Grab>(id));
            owner.end(state);
            return true;
        }
        apply(owner, state, &geometry, grab, pointer);
        ui.data_mut(|data| {
            data.insert_temp(
                id,
                Grab {
                    previous: pointer,
                    ring_parameter: match grab.hit {
                        AlignmentGizmoHit::Rotate(axis) => {
                            polyline_parameter(pointer, &geometry.rings[axis])
                                .unwrap_or(grab.ring_parameter)
                        }
                        _ => grab.ring_parameter,
                    },
                    reach: pointer.distance(geometry.origin),
                    ..grab
                },
            );
        });
        ui.ctx().request_repaint();
        return true;
    }

    if response.drag_started_by(egui::PointerButton::Primary)
        && editing
        && let Some(hit) = crate::viewport::gizmo_hit(pointer, &geometry, handles(owner, state))
    {
        ui.data_mut(|data| {
            data.insert_temp(
                id,
                Grab {
                    hit,
                    previous: pointer,
                    ring_parameter: match hit {
                        AlignmentGizmoHit::Rotate(axis) => {
                            polyline_parameter(pointer, &geometry.rings[axis]).unwrap_or(0.0)
                        }
                        _ => 0.0,
                    },
                    reach: pointer.distance(geometry.origin),
                },
            );
        });
        owner.begin(state);
        return true;
    }
    false
}

fn apply(
    owner: VertexOwner,
    state: &mut AppState,
    geometry: &AlignmentGizmoGeometry,
    grab: Grab,
    pointer: egui::Pos2,
) {
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
            owner.move_by(state, direction * travel);
        }
        AlignmentGizmoHit::Plane(normal) => {
            let (first, second) = plane_axes(normal);
            let (Some(first_step), Some(second_step)) = (
                axis_screen_step(geometry, first),
                axis_screen_step(geometry, second),
            ) else {
                return;
            };
            let determinant = first_step.x * second_step.y - first_step.y * second_step.x;
            if determinant.abs() <= f32::EPSILON {
                return;
            }
            let motion = pointer - grab.previous;
            let along_first = (motion.x * second_step.y - motion.y * second_step.x) / determinant;
            let along_second = (first_step.x * motion.y - first_step.y * motion.x) / determinant;
            let axes = [Vec3::X, Vec3::Y, Vec3::Z];
            owner.move_by(
                state,
                axes[first] * along_first + axes[second] * along_second,
            );
        }
        AlignmentGizmoHit::Rotate(axis) => {
            let ring = &geometry.rings[axis];
            let Some(parameter) = polyline_parameter(pointer, ring) else {
                return;
            };
            let turned = wrapped_angle_delta(grab.ring_parameter, parameter);
            if turned == 0.0 {
                return;
            }
            let axis = [Vec3::X, Vec3::Y, Vec3::Z][axis];
            owner.transform_by(
                state,
                geometry.world_center,
                glam::Mat3::from_axis_angle(axis, turned),
            );
        }
        AlignmentGizmoHit::Scale => {
            let reach = pointer.distance(geometry.origin);
            if grab.reach <= SCALE_DEAD_POINTS || reach <= SCALE_DEAD_POINTS {
                return;
            }
            let ratio = (reach / grab.reach).clamp(1.0 / SCALE_STEP_LIMIT, SCALE_STEP_LIMIT);
            owner.transform_by(
                state,
                geometry.world_center,
                glam::Mat3::from_diagonal(Vec3::splat(ratio)),
            );
        }
    }
}

fn axis_screen_step(geometry: &AlignmentGizmoGeometry, axis: usize) -> Option<egui::Vec2> {
    let end = geometry.axis_ends[axis]?;
    let units = geometry.axis_world_units_per_point[axis]?;
    let run = end - geometry.origin;
    let length = run.length();
    (length > f32::EPSILON && units > 0.0).then(|| run / length / units as f32)
}

const SCALE_DEAD_POINTS: f32 = 8.0;

const SCALE_STEP_LIMIT: f32 = 1.5;

pub(super) fn paint(
    owner: VertexOwner,
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
) {
    let Some(geometry) = geometry(owner, state, viewport, camera) else {
        return;
    };
    crate::viewport::paint_gizmo_geometry(ui, &geometry, handles(owner, state));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    const ONE: GizmoHandles = GizmoHandles::MOVE_ONLY;

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
    fn a_single_point_offers_neither_a_ring_nor_a_scale_grip() {
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
                crate::viewport::gizmo_hit(geometry.scale_handle, &geometry, ONE),
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
            crate::viewport::gizmo_hit(on_a_ring, &geometry, ONE).is_none(),
            "one point has no centre to turn about that is not the point",
        );

        assert!(
            matches!(
                crate::viewport::gizmo_hit(
                    on_a_ring,
                    &geometry,
                    crate::viewport::GizmoHandles::ALL
                ),
                Some(AlignmentGizmoHit::Rotate(_))
            ),
            "a selection of two must be able to turn",
        );
    }

    #[test]
    fn a_plane_is_reachable_and_loses_to_the_arrow_it_sits_beside() {
        let geometry = geometry_at(Vec3::ZERO);
        let quad = geometry
            .plane_quads
            .iter()
            .enumerate()
            .find(|(_, quad)| quad.len() == 4);
        let Some((normal, quad)) = quad else {
            return;
        };
        let centre = quad
            .iter()
            .fold(egui::Vec2::ZERO, |sum, point| sum + point.to_vec2())
            / quad.len() as f32;
        assert_eq!(
            crate::viewport::gizmo_hit(centre.to_pos2(), &geometry, ONE),
            Some(AlignmentGizmoHit::Plane(normal)),
            "the middle of a plane quad has to be that plane",
        );

        for (axis, end) in geometry.axis_ends.iter().copied().enumerate() {
            let Some(end) = end else { continue };
            let along = geometry.origin + (end - geometry.origin) * 0.5;
            assert_eq!(
                crate::viewport::gizmo_hit(along, &geometry, ONE),
                Some(AlignmentGizmoHit::Move(axis)),
                "a plane swallowed the arrow running through it",
            );
        }
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
