use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct ViewportCameraInput {
    delta: CameraDelta,

    wheel_points: f32,

    snap_orbit: bool,
}

impl ViewportCameraInput {
    fn has_motion(self) -> bool {
        self.delta.has_motion() || self.wheel_points != 0.0
    }

    fn apply(&mut self, gesture: CameraGesture) {
        match gesture {
            CameraGesture::Orbit(points) => self.delta.orbit_points = points,
            CameraGesture::Pan(points) => self.delta.pan_points = points,
            CameraGesture::Dolly(points) => self.delta.scroll_points = points,
        }
    }
}

pub(super) const ROLL_SWEEP_ID: &str = "vkit.viewport.roll-sweep";

pub(super) const ROLL_SWEEP_SIDE_ID: &str = "vkit.viewport.roll-sweep.side";

pub(super) const CAMERA_KEY_OWNER_ID: &str = "vkit.viewport.camera-key-owner";

pub(super) const SPLIT_PANE_SCENE_SALT: u64 = 0x5350_4c54_5641;
pub(super) const SCENE_SALT_ID: &str = "vkit.viewport.scene-salt";

pub(super) fn scene_salt(ui: &Ui) -> u64 {
    ui.data(|data| data.get_temp::<u64>(Id::new(SCENE_SALT_ID)))
        .unwrap_or_default()
}

pub(super) const fn roll_sweep_side(
    sweep_active: bool,
    pinned: Option<MeshSide>,
    hovered: MeshSide,
) -> MeshSide {
    match (sweep_active, pinned) {
        (true, Some(side)) => side,
        _ => hovered,
    }
}

pub fn resolve_active_pane(ui: &Ui, state: &mut AppState, first: Rect, second: Rect) {
    if let Some(owner) = ui.data(|data| data.get_temp::<ViewPane>(Id::new(CAMERA_KEY_OWNER_ID))) {
        state.active_view_pane = owner;
        return;
    }
    if let Some(point) = ui.input(|input| input.pointer.hover_pos()) {
        if second.contains(point) {
            state.active_view_pane = ViewPane::Split;
        } else if first.contains(point) {
            state.active_view_pane = ViewPane::Primary;
        }
    }
}

pub(super) fn handle_camera_control_shortcuts(
    ui: &Ui,
    state: &mut AppState,
    rect: Rect,
    camera: &mut TurntableCamera,
    pane: ViewPane,
) -> bool {
    if crate::shortcuts::Shortcut::ViewLevelRoll.pressed(ui) {
        camera.level_roll();
    }
    let update = crate::sweep_gesture::handle_sweep(
        ui,
        Id::new(ROLL_SWEEP_ID),
        crate::shortcuts::Shortcut::ViewTrackball,
        rect,
        0.0,
        0.0,
        None,
    );
    if crate::sweep_gesture::sweep_active(ui, Id::new(ROLL_SWEEP_ID)) {
        camera.apply_trackball(ui.input(|input| input.pointer.delta()));
    }
    let armed = crate::sweep_gesture::sweep_active(ui, Id::new(ROLL_SWEEP_ID));
    ui.data_mut(|data| {
        if armed {
            data.insert_temp(Id::new(CAMERA_KEY_OWNER_ID), pane);
        } else {
            data.remove::<ViewPane>(Id::new(CAMERA_KEY_OWNER_ID));
        }
    });
    let mode = if armed {
        ControlMode::Trackball
    } else {
        ControlMode::Orbit
    };
    if state.camera_control != mode {
        state.dispatch(Action::SetCameraControl(mode));
    }

    update.consumed || update.finished
}

pub(super) fn brush_sweep_owns_pointer(ui: &Ui) -> bool {
    crate::ui_components::BrushSweeps::ALL
        .into_iter()
        .any(|sweep| {
            crate::sweep_gesture::sweep_active(ui, sweep.size())
                || crate::sweep_gesture::sweep_active(ui, sweep.strength())
        })
}

pub(super) fn camera_mode_owns_pointer(state: &AppState) -> bool {
    state.camera_control != ControlMode::Orbit
}

pub(super) fn navigation_drag(ui: &Ui, response: &Response) -> Option<MiddleDragBinding> {
    use crate::shortcuts::{Shortcut, Trigger};

    let modifiers = ui.input(|input| input.modifiers);
    [
        (Shortcut::ViewDolly, MiddleDragBinding::Dolly),
        (Shortcut::ViewPan, MiddleDragBinding::Pan),
        (Shortcut::ViewOrbit, MiddleDragBinding::Orbit),
    ]
    .into_iter()
    .find(|(shortcut, _)| {
        let binding = shortcut.binding(ui);
        matches!(binding.trigger, Trigger::Mouse(button) if response.dragged_by(button))
            && binding.modifiers.admits(modifiers)
    })
    .map(|(_, gesture)| gesture)
}

pub(super) fn viewport_camera_input(
    ui: &Ui,
    response: &Response,
    rect: Rect,
    roll: f32,
) -> ViewportCameraInput {
    let modifiers = ui.input(|input| input.modifiers);

    let pointer_delta = ui.input(|input| input.pointer.delta());
    let mut camera_input = ViewportCameraInput {
        delta: CameraDelta {
            viewport_height_points: rect.height(),
            ..Default::default()
        },
        wheel_points: 0.0,
        snap_orbit: false,
    };

    if let Some(binding) = navigation_drag(ui, response) {
        if let Some(gesture) = binding.gesture(pointer_delta, roll) {
            camera_input.apply(gesture);
        }
        camera_input.snap_orbit = binding == MiddleDragBinding::Orbit && modifiers.alt;
    }
    if response.hovered() {
        camera_input.wheel_points = ui.input(|input| input.smooth_scroll_delta.y);
    }
    camera_input
}

pub(super) fn viewport_camera_motion(
    ui: &Ui,
    response: &Response,
    rect: Rect,
    mut camera: TurntableCamera,
    pick_world_point: &dyn Fn(Ray3) -> Option<glam::Vec3>,
    wheel_claimed: bool,
) -> Option<TurntableCamera> {
    let mut camera_input = viewport_camera_input(ui, response, rect, camera.roll);

    if wheel_claimed {
        camera_input.wheel_points = 0.0;
    }
    if !camera_input.has_motion() {
        return None;
    }
    if camera_input.delta.has_motion() {
        let free_orbit = camera_input.delta.orbit_points != Vec2::ZERO && !camera_input.snap_orbit;
        if free_orbit {
            camera.commit_snap();
        }
        camera.apply_delta(camera_input.delta);
        if camera_input.snap_orbit {
            camera.snap_view = true;
        }
    }
    if camera_input.wheel_points != 0.0 {
        let cursor = ui
            .input(|input| input.pointer.hover_pos())
            .filter(|pointer| rect.contains(*pointer));
        match cursor {
            Some(cursor) => {
                let anchor = camera
                    .ray_from_screen(cursor, rect)
                    .and_then(pick_world_point);
                camera.zoom_about_screen_point(camera_input.wheel_points, cursor, rect, anchor);
            }
            None => camera.apply_delta(CameraDelta {
                scroll_points: camera_input.wheel_points,
                viewport_height_points: rect.height(),
                ..Default::default()
            }),
        }
    }
    Some(camera)
}

pub(super) fn frame_shortcut_pressed(ui: &Ui, response: &Response) -> bool {
    response.hovered() && crate::shortcuts::Shortcut::FrameSelected.pressed(ui)
}

pub(super) fn nearest_visible_world_hit(
    ray: Ray3,
    layers: &[(Option<&SurfaceMesh>, ModelTransform)],
) -> Option<glam::Vec3> {
    let mut best: Option<(f64, glam::DVec3)> = None;
    for (mesh, transform) in layers {
        let Some(mesh) = mesh else {
            continue;
        };
        let Some(hit) = mesh.pick_visible_surface(ray, *transform) else {
            continue;
        };
        let world = transform.point_to_world(hit.local_point);
        let along = (world - ray.origin).dot(ray.direction);
        if !along.is_finite() || along <= 0.0 {
            continue;
        }
        if best.is_none_or(|(current, _)| along < current) {
            best = Some((along, world));
        }
    }
    best.map(|(_, world)| world.as_vec3())
}

pub(super) fn handle_light_interaction(ui: &Ui, state: &mut AppState, response: &Response) {
    let rotate = response.hovered() && crate::shortcuts::Shortcut::LightRotate.held(ui);
    if rotate {
        let delta = ui.input(|input| input.pointer.delta().x);
        if delta != 0.0 {
            state.dispatch(crate::state::Action::RotateLight(delta * 0.012));
            ui.ctx().request_repaint();
        }
    }
}

pub(super) fn set_edit_camera(state: &mut AppState, side: MeshSide, camera: TurntableCamera) {
    match side {
        MeshSide::Scan => state.workspace.scan_camera = camera,
        MeshSide::Template => state.workspace.template_camera = camera,
    }
    state.workspace.reconcile_linked_edit_cameras(side);
}

pub(super) fn commit_swept_edit_camera(
    state: &mut AppState,
    side: MeshSide,
    swept: TurntableCamera,
    sweep_owns_camera: bool,
) {
    match side {
        MeshSide::Scan => state.workspace.scan_camera = swept,
        MeshSide::Template => state.workspace.template_camera = swept,
    }
    if sweep_owns_camera {
        state.workspace.reconcile_linked_edit_cameras(side);
    }
}

#[cfg(test)]
pub(super) fn apply_edit_camera_delta(state: &mut AppState, side: MeshSide, delta: CameraDelta) {
    let mut camera = match side {
        MeshSide::Scan => state.workspace.scan_camera,
        MeshSide::Template => state.workspace.template_camera,
    };
    camera.apply_delta(delta);
    set_edit_camera(state, side, camera);
}
