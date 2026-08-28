use egui::{Pos2, Rect, Response, Ui};
use glam::Vec3;

use crate::camera::TurntableCamera;
use crate::state::{Action, AppState};

const GRAB_POINTS: f32 = 9.0;

fn marker_size() -> super::marker_size::MarkerSize {
    super::marker_size::MarkerSize::new(0.13, MARKER_SMALLEST..=MARKER_LARGEST)
}

const MARKER_SMALLEST: f32 = 0.35;
const MARKER_LARGEST: f32 = 1.9;

const MARKER_EMPHASIS: f32 = 1.9;

const RING_POINTS: f32 = 0.9;

const MARKER_FLOOR: f32 = 1.1;

struct Seen {
    index: u32,
    at: Pos2,
    depth: f32,

    radius: f32,
}

fn visible_points(ui: &Ui, state: &AppState, viewport: Rect, camera: TurntableCamera) -> Vec<Seen> {
    let Some(vertices) = state.sculpt.display_vertices() else {
        return Vec::new();
    };
    let eye = camera.eye();
    let spacing = vertex_spacing(ui, state);
    let size = marker_size();
    let mut seen = Vec::new();
    for (index, vertex) in vertices.iter().enumerate() {
        let index = index as u32;
        if !state.sculpt.is_vertex_editable(index) {
            continue;
        }
        let world = Vec3::new(vertex[0] as f32, vertex[1] as f32, vertex[2] as f32);
        let Some(projected) = camera.project(world, viewport) else {
            continue;
        };
        if !viewport.contains(projected.screen) {
            continue;
        }
        let distance = (world - eye).length();
        let world_per_point = camera
            .world_units_per_point_at(world, viewport.height())
            .max(1.0e-6);
        seen.push(Seen {
            index,
            at: projected.screen,
            depth: distance,
            radius: size.of_screen_spacing(
                spacing.get(index as usize).copied().unwrap_or(0.0) / world_per_point,
            ),
        });
    }
    seen
}

fn vertex_spacing(ui: &Ui, state: &AppState) -> std::sync::Arc<Vec<f32>> {
    let (Some(mesh), Some(vertices)) =
        (state.sculpt.working_mesh(), state.sculpt.display_vertices())
    else {
        return std::sync::Arc::new(Vec::new());
    };
    type Cached = ((usize, usize), std::sync::Arc<Vec<f32>>);
    let stamp = (mesh.vertices.len(), mesh.faces.len());
    let slot = egui::Id::new("vkit.sculpt.vertex-spacing");
    if let Some((cached, spacing)) = ui.data(|data| data.get_temp::<Cached>(slot))
        && cached == stamp
    {
        return spacing;
    }

    let mut shortest = vec![f32::INFINITY; vertices.len()];
    for face in &mesh.faces {
        let corners = &face.vertex_indices;
        for (step, from) in corners.iter().enumerate() {
            let to = corners[(step + 1) % corners.len()];
            let (Some(a), Some(b)) = (vertices.get(*from as usize), vertices.get(to as usize))
            else {
                continue;
            };
            let span = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let edge = (span[0] * span[0] + span[1] * span[1] + span[2] * span[2]).sqrt() as f32;
            if edge <= 0.0 {
                continue;
            }
            for end in [*from, to] {
                if let Some(held) = shortest.get_mut(end as usize) {
                    *held = held.min(edge);
                }
            }
        }
    }
    for held in &mut shortest {
        if !held.is_finite() {
            *held = 0.0;
        }
    }
    let spacing = std::sync::Arc::new(shortest);
    ui.data_mut(|data| data.insert_temp(slot, (stamp, std::sync::Arc::clone(&spacing))));
    spacing
}

fn point_under(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
    pointer: Pos2,
) -> Option<u32> {
    let points = visible_points(ui, state, viewport, camera);
    nearest_to(state, camera, viewport, &points, pointer)
}

fn nearest_to(
    state: &AppState,
    camera: TurntableCamera,
    viewport: Rect,
    points: &[Seen],
    pointer: Pos2,
) -> Option<u32> {
    let reach = GRAB_POINTS * GRAB_POINTS;
    let mut candidates: Vec<&Seen> = points
        .iter()
        .filter(|seen| seen.at.distance_sq(pointer) <= reach)
        .collect();
    if candidates.is_empty() {
        return None;
    }
    candidates.sort_by(|left, right| {
        left.depth.total_cmp(&right.depth).then_with(|| {
            left.at
                .distance_sq(pointer)
                .total_cmp(&right.at.distance_sq(pointer))
        })
    });

    let slack = camera
        .world_units_per_point_at(camera.target, viewport.height())
        .max(1.0e-4)
        * GRAB_POINTS;
    for seen in candidates {
        let Some(ray) = camera.ray_from_screen(seen.at, viewport) else {
            continue;
        };
        let blocked = state
            .sculpt
            .raycast_visible(
                ray.origin.to_array(),
                ray.direction.to_array(),
                state.sculpt.visible_targets(),
            )
            .is_some_and(|hit| {
                let at = glam::DVec3::from_array(hit.point_local).as_vec3();
                (at - camera.eye()).length() + slack < seen.depth
            });
        if !blocked {
            return Some(seen.index);
        }
    }
    None
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PickIntent {
    Replace,
    Toggle,
    Remove,
}

fn apply_pick(
    selection: &mut std::collections::BTreeSet<u32>,
    picked: Option<u32>,
    intent: PickIntent,
) {
    match (picked, intent) {
        (None, PickIntent::Replace) => selection.clear(),
        (None, _) => {}
        (Some(point), PickIntent::Replace) => {
            selection.clear();
            selection.insert(point);
        }
        (Some(point), PickIntent::Toggle) => {
            if !selection.remove(&point) {
                selection.insert(point);
            }
        }
        (Some(point), PickIntent::Remove) => {
            selection.remove(&point);
        }
    }
}

#[must_use]
pub(super) fn selection_centre(state: &AppState) -> Option<Vec3> {
    let vertices = state.sculpt.display_vertices()?;
    let mut total = Vec3::ZERO;
    let mut counted = 0.0_f32;
    for index in &state.sculpt_vertex_selection {
        let Some(vertex) = vertices.get(*index as usize) else {
            continue;
        };
        total += Vec3::new(vertex[0] as f32, vertex[1] as f32, vertex[2] as f32);
        counted += 1.0;
    }
    (counted > 0.0).then(|| total / counted)
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
    if (response.drag_started_by(egui::PointerButton::Primary) && editing)
        || (response.clicked() && !response.dragged())
    {
        let intent = if crate::shortcuts::Shortcut::VertexAddToSelectionHold.held(ui) {
            PickIntent::Toggle
        } else if crate::shortcuts::Shortcut::VertexRemoveFromSelectionHold.held(ui) {
            PickIntent::Remove
        } else {
            PickIntent::Replace
        };
        let picked = point_under(ui, state, viewport, camera, pointer);
        let took_one = picked.is_some();
        apply_pick(&mut state.sculpt_vertex_selection, picked, intent);
        if !took_one {
            return;
        }
        if editing {
            state.dispatch(Action::BeginSculptStroke {
                view_direction_local: None,
                brush_direction_local: None,
            });
        }
    }
    if state.sculpt_vertex_selection.is_empty() {
        return;
    }
    if !editing {
        if response.drag_stopped() {
            ui.data_mut(|data| data.remove::<Grab>(egui::Id::new(DRAG_ID)));
            state.dispatch(Action::EndSculptStroke);
        }
        return;
    }
    let Some(grab) = drag_anchor(ui, state, viewport, camera) else {
        return;
    };
    let Some(now) = plane_hit_on(
        camera,
        viewport,
        pointer,
        grab.plane_point,
        grab.plane_normal,
    ) else {
        return;
    };
    let shift = now - grab.at;
    if shift.length_squared() <= 1.0e-12 {
        return;
    }
    state.dispatch(Action::MoveSculptVertices {
        shift: shift.to_array().map(f64::from),
    });
    ui.data_mut(|data| data.insert_temp(egui::Id::new(DRAG_ID), Grab { at: now, ..grab }));
    ui.ctx().request_repaint();
}

const DRAG_ID: &str = "vkit.viewport.sculpt.vertex-drag";

#[derive(Clone, Copy, Debug)]
struct Grab {
    plane_point: Vec3,
    plane_normal: Vec3,
    at: Vec3,
}

fn drag_anchor(ui: &Ui, state: &AppState, viewport: Rect, camera: TurntableCamera) -> Option<Grab> {
    let id = egui::Id::new(DRAG_ID);
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
    travel.is_finite().then(|| origin + direction * travel)
}

#[must_use]
pub(super) fn markers(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
) -> Option<std::sync::Arc<Vec<crate::renderer::MarkerInstance>>> {
    if !state.editing_the_cage() {
        return None;
    }
    let points = visible_points(ui, state, viewport, camera);
    if points.is_empty() {
        return None;
    }
    let hover = ui
        .input(|input| input.pointer.hover_pos())
        .filter(|pointer| viewport.contains(*pointer));
    let nearest = hover.and_then(|pointer| nearest_to(state, camera, viewport, &points, pointer));

    let wire = super::wireframe_color(state);
    let ring = crate::ui_components::readable_ink(wire);
    let scale = ui.ctx().pixels_per_point();
    let of = |colour: egui::Color32| {
        let [r, g, b, a] = colour.to_array();
        [
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            f32::from(a) / 255.0,
        ]
    };

    let vertices = state.sculpt.display_vertices()?;
    let mut instances = Vec::with_capacity(points.len());
    for seen in &points {
        let held = state.sculpt_vertex_selection.contains(&seen.index);
        let under = nearest == Some(seen.index);
        if !held && !under && seen.radius < MARKER_FLOOR {
            continue;
        }
        let Some(vertex) = vertices.get(seen.index as usize) else {
            continue;
        };
        let radius = if held || under {
            (seen.radius * MARKER_EMPHASIS).max(marker_size().smallest() * MARKER_EMPHASIS)
        } else {
            seen.radius
        };
        let interesting = held || under;
        instances.push(crate::renderer::MarkerInstance {
            position: [vertex[0] as f32, vertex[1] as f32, vertex[2] as f32],
            shape: crate::renderer::MarkerInstance::ROUND,
            radius: if interesting {
                (radius + RING_POINTS) * scale
            } else {
                radius * scale
            },
            fill: of(if held {
                crate::theme::COLOR_PRIMARY
            } else {
                wire
            }),
            ring: of(if interesting { ring } else { wire }),
        });
    }
    (!instances.is_empty()).then(|| std::sync::Arc::new(instances))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection(of: &[u32]) -> std::collections::BTreeSet<u32> {
        of.iter().copied().collect()
    }

    #[test]
    fn the_wireframe_comes_up_with_the_tool_and_goes_away_with_it() {
        let mut state = crate::state::AppState::default();
        state.active_tab = crate::state::Tab::Morph;
        state.result_preview_phase = crate::state::ResultPreviewPhase::Sculpt;
        assert!(!state.wireframe_shown(), "a fresh sculpt starts without it");

        state.sculpt_brush = crate::sculpt::SculptBrush::Vertex;
        assert!(state.wireframe_shown(), "the vertex tool needs the edges");
        assert!(
            !state.wireframe_visible,
            "the tool changed the reader's own setting instead of asking",
        );

        state.sculpt_brush = crate::sculpt::SculptBrush::Move;
        assert!(!state.wireframe_shown(), "the wire was left on");
    }

    #[test]
    fn clicking_a_point_takes_it_and_clicking_past_them_all_lets_go() {
        let mut held = selection(&[3, 4]);
        apply_pick(&mut held, Some(9), PickIntent::Replace);
        assert_eq!(held, selection(&[9]), "a plain click starts over");

        apply_pick(&mut held, None, PickIntent::Replace);
        assert!(
            held.is_empty(),
            "clicking past every point kept a selection"
        );
    }

    #[test]
    fn the_add_and_remove_holds_leave_the_rest_of_the_selection_alone() {
        let mut held = selection(&[1, 2]);
        apply_pick(&mut held, Some(3), PickIntent::Toggle);
        assert_eq!(held, selection(&[1, 2, 3]));

        apply_pick(&mut held, Some(2), PickIntent::Toggle);
        assert_eq!(held, selection(&[1, 3]), "toggling a held point drops it");

        apply_pick(&mut held, Some(1), PickIntent::Remove);
        assert_eq!(held, selection(&[3]));

        apply_pick(&mut held, Some(1), PickIntent::Remove);
        assert_eq!(
            held,
            selection(&[3]),
            "removing what is not there changed it"
        );

        apply_pick(&mut held, None, PickIntent::Toggle);
        apply_pick(&mut held, None, PickIntent::Remove);
        assert_eq!(
            held,
            selection(&[3]),
            "a miss while holding a modifier must not clear",
        );
    }

    const _: () = {
        assert!(
            GRAB_POINTS > MARKER_LARGEST * MARKER_EMPHASIS,
            "the grab radius has to be bigger than the dot, or it is a fight",
        );
        assert!(
            MARKER_FLOOR >= MARKER_SMALLEST,
            "a marker that never reaches the floor can never be dropped",
        );
        assert!(
            MARKER_EMPHASIS > 1.0,
            "a held point has to stand out from the rest",
        );
    };

    fn radius_of(edge: f32, world_per_point: f32) -> f32 {
        marker_size().of_screen_spacing(edge / world_per_point)
    }

    #[test]
    fn a_marker_shrinks_as_the_mesh_gets_further_away() {
        assert!(
            radius_of(0.4, 0.05) > radius_of(0.4, 0.2),
            "pulling back did not shrink the marker",
        );
        assert!(
            radius_of(0.4, 1.0e-9) <= MARKER_LARGEST,
            "a bead got through"
        );
        assert!(
            radius_of(0.4, 1.0e9) >= MARKER_SMALLEST,
            "a marker vanished",
        );
    }

    #[test]
    fn a_vertex_in_a_crowd_gets_a_smaller_dot_than_one_on_its_own() {
        let close = radius_of(0.05, 0.02);
        let spread = radius_of(0.5, 0.02);
        assert!(
            close < spread,
            "a crowded vertex drew as large as a lonely one: {close} against {spread}",
        );
    }
}
