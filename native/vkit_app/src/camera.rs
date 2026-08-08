use egui::{Pos2, Rect, Vec2};
use glam::{DVec3, Mat4, Vec3, Vec4};

use crate::scene::{Bounds3, Ray3};

pub const MIN_FOV_Y_DEGREES: f32 = 10.0;
pub const MAX_FOV_Y_DEGREES: f32 = 120.0;
pub const DEFAULT_FOV_Y_DEGREES: f32 = 38.0;
pub const DEFAULT_VIEW_YAW_RADIANS: f32 = 15.0_f32.to_radians();
pub const DEFAULT_VIEW_PITCH_RADIANS: f32 = 15.0_f32.to_radians();

const DEFAULT_FOV_Y_RADIANS: f32 = DEFAULT_FOV_Y_DEGREES.to_radians();

const ORBIT_RADIANS_PER_POINT: f32 = 0.0035;

const PITCH_LIMIT_RADIANS: f32 = 1.553_343;

const VIEW_SNAP_RADIANS: f32 = std::f32::consts::FRAC_PI_4;

const PAN_POINTS_SCALE: f32 = 0.5;
const ZOOM_PER_SCROLL_POINT: f32 = 0.0025;
const FRAME_MARGIN: f32 = 1.18;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProjectionMode {
    #[default]
    Perspective,
    Orthographic,
}

/// The views the numpad jumps to.
///
/// A face is worked from the front, so the four diagonals are quarter turns off
/// the front rather than free orbits, and each sits on the numpad key that
/// already points that way: 7 and 9 above, 1 and 3 below.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardView {
    LeftSide,
    RightSide,

    Top,

    Bottom,

    FrontUpperLeft,
    FrontUpperRight,
    FrontLowerLeft,
    FrontLowerRight,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TurntableCamera {
    pub yaw: f32,
    pub pitch: f32,

    pub roll: f32,
    pub target: Vec3,
    pub distance: f32,
    pub frame_radius: f32,

    /// The box the camera was last framed against, kept so the fit can use the extent the camera
    /// actually looks along instead of the bounding sphere. `frame_radius` must go on meaning
    /// "scene scale" for lighting, ambient occlusion and near/far, so the tighter fit needs its
    /// own input.
    pub frame_bounds: Option<Bounds3>,
    pub fov_y_radians: f32,
    pub projection_mode: ProjectionMode,

    pub orthographic_scale: f32,

    pub snap_view: bool,
}

impl Default for TurntableCamera {
    fn default() -> Self {
        Self {
            yaw: DEFAULT_VIEW_YAW_RADIANS,
            pitch: DEFAULT_VIEW_PITCH_RADIANS,
            roll: 0.0,
            target: Vec3::ZERO,
            distance: 3.0,
            frame_radius: 1.0,
            frame_bounds: None,
            fov_y_radians: DEFAULT_FOV_Y_RADIANS,
            projection_mode: ProjectionMode::Perspective,
            orthographic_scale: 2.36,
            snap_view: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CameraDelta {
    pub orbit_points: Vec2,
    pub pan_points: Vec2,

    pub roll_radians: f32,
    pub scroll_points: f32,
    pub viewport_height_points: f32,
}

impl CameraDelta {
    pub fn has_motion(self) -> bool {
        self.orbit_points != Vec2::ZERO
            || self.pan_points != Vec2::ZERO
            || self.roll_radians != 0.0
            || self.scroll_points != 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectedPoint {
    pub screen: Pos2,
    pub depth: f32,
}

impl TurntableCamera {
    pub fn frame(&mut self, bounds: Bounds3) {
        self.target = bounds.center();
        self.frame_radius = bounds.radius().max(1.0e-4);
        self.frame_bounds = Some(bounds);
        self.frame_current_target();
    }

    pub fn reset_view(&mut self) {
        self.yaw = DEFAULT_VIEW_YAW_RADIANS;
        self.pitch = DEFAULT_VIEW_PITCH_RADIANS;

        self.roll = 0.0;
        self.frame_current_target();
    }

    pub fn reset_view_with_default_fov(&mut self, bounds: Option<Bounds3>) {
        self.fov_y_radians = DEFAULT_FOV_Y_RADIANS;
        if let Some(bounds) = bounds {
            self.target = bounds.center();
            self.frame_radius = bounds.radius().max(1.0e-4);
            self.frame_bounds = Some(bounds);
        }
        self.reset_view();
    }

    pub fn set_projection_mode(&mut self, mode: ProjectionMode) {
        if self.projection_mode == mode {
            return;
        }
        match mode {
            ProjectionMode::Perspective => {
                self.distance = (self.orthographic_scale * 0.5 / self.half_fov_tan())
                    .clamp(self.minimum_distance(), self.maximum_distance());
            }
            ProjectionMode::Orthographic => {
                self.orthographic_scale =
                    (2.0 * self.distance * self.half_fov_tan()).max(self.minimum_ortho_scale());
            }
        }
        self.projection_mode = mode;
    }

    pub fn fov_y_degrees(&self) -> f32 {
        self.fov_y_radians.to_degrees()
    }

    pub fn set_fov_y_degrees(&mut self, degrees: f32) -> f32 {
        let degrees = if degrees.is_finite() {
            degrees.clamp(MIN_FOV_Y_DEGREES, MAX_FOV_Y_DEGREES)
        } else {
            DEFAULT_FOV_Y_DEGREES
        };
        self.fov_y_radians = degrees.to_radians();
        degrees
    }

    pub fn set_fov_y_degrees_with_dolly_compensation(&mut self, degrees: f32) -> f32 {
        let previous_half_fov_tan = self.half_fov_tan();
        let accepted = self.set_fov_y_degrees(degrees);
        if self.projection_mode == ProjectionMode::Perspective {
            self.distance = (self.distance * previous_half_fov_tan / self.half_fov_tan())
                .clamp(self.minimum_distance(), self.maximum_distance());
        }
        accepted
    }

    #[cfg(test)]
    pub fn set_orthographic_scale(&mut self, scale: f32) -> f32 {
        let scale = if scale.is_finite() && scale > 0.0 {
            scale.clamp(self.minimum_ortho_scale(), self.maximum_ortho_scale())
        } else {
            (self.frame_radius * 2.0 * FRAME_MARGIN).max(self.minimum_ortho_scale())
        };
        self.orthographic_scale = scale;
        scale
    }

    pub fn apply_delta(&mut self, delta: CameraDelta) {
        self.apply_orbit_angles(delta.orbit_points);
        if delta.roll_radians != 0.0 && delta.roll_radians.is_finite() {
            self.roll = (self.roll + delta.roll_radians).rem_euclid(std::f32::consts::TAU);
        }

        if delta.pan_points != Vec2::ZERO {
            let height = delta.viewport_height_points.max(1.0);
            let world_per_point = match self.projection_mode {
                ProjectionMode::Perspective => 2.0 * self.distance * self.half_fov_tan() / height,
                ProjectionMode::Orthographic => self.orthographic_scale / height,
            };
            let (_, right, up) = self.basis();
            let scaled = world_per_point * PAN_POINTS_SCALE;
            self.target +=
                right * (-delta.pan_points.x * scaled) + up * (delta.pan_points.y * scaled);
        }

        if delta.scroll_points != 0.0 {
            let factor = (-delta.scroll_points * ZOOM_PER_SCROLL_POINT).exp();
            match self.projection_mode {
                ProjectionMode::Perspective => {
                    self.distance = (self.distance * factor)
                        .clamp(self.minimum_distance(), self.maximum_distance());
                }
                ProjectionMode::Orthographic => {
                    self.orthographic_scale = (self.orthographic_scale * factor)
                        .clamp(self.minimum_ortho_scale(), self.maximum_ortho_scale());
                }
            }
        }
    }

    pub fn zoom_about_screen_point(
        &mut self,
        scroll_points: f32,
        cursor: Pos2,
        viewport: Rect,
        anchor: Option<Vec3>,
    ) {
        if scroll_points == 0.0 {
            return;
        }

        let anchor = anchor
            .filter(|point| point.is_finite())
            .map(Vec3::as_dvec3)
            .or_else(|| {
                self.cursor_world_point_at_depth(cursor, viewport, f64::from(self.distance))
            });
        self.apply_delta(CameraDelta {
            scroll_points,
            viewport_height_points: viewport.height(),
            ..Default::default()
        });
        let Some(anchor) = anchor else {
            return;
        };
        let depth = (anchor - self.eye().as_dvec3()).dot(self.view_forward());
        if !(depth.is_finite() && depth > 1.0e-9) {
            return;
        }

        let Some(reprojected) = self.cursor_world_point_at_depth(cursor, viewport, depth) else {
            return;
        };
        let correction = (anchor - reprojected).as_vec3();
        if correction.is_finite() {
            self.target += correction;
        }
    }

    pub fn eye(&self) -> Vec3 {
        let (forward_from_target, _, _) = self.basis();
        self.target + forward_from_target * self.distance
    }

    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        let aspect = aspect.max(1.0e-4);
        let near = (self.distance * 0.001).max(self.frame_radius * 0.000_1);
        let far = (self.distance + self.frame_radius * 20.0).max(near + 1.0);
        let projection = match self.projection_mode {
            ProjectionMode::Perspective => glam::camera::rh::proj::directx::perspective(
                self.safe_fov_y_radians(),
                aspect,
                near,
                far,
            ),
            ProjectionMode::Orthographic => {
                let half_height = self.orthographic_scale.max(self.minimum_ortho_scale()) * 0.5;
                let half_width = half_height * aspect;
                glam::camera::rh::proj::directx::orthographic(
                    -half_width,
                    half_width,
                    -half_height,
                    half_height,
                    near,
                    far,
                )
            }
        };
        let (_, _, up) = self.basis();
        projection * glam::camera::rh::view::look_at_mat4(self.eye(), self.target, up)
    }

    pub fn ray_from_screen(&self, screen: Pos2, viewport: Rect) -> Option<Ray3> {
        if viewport.width() <= 0.0 || viewport.height() <= 0.0 || !viewport.contains(screen) {
            return None;
        }
        let x = ((screen.x - viewport.left()) / viewport.width()) * 2.0 - 1.0;
        let y = 1.0 - ((screen.y - viewport.top()) / viewport.height()) * 2.0;
        let inverse = self
            .view_projection(viewport.width() / viewport.height())
            .inverse();
        if !inverse.is_finite() {
            return None;
        }
        let near = unproject(inverse, Vec3::new(x, y, 0.0))?;
        let far = unproject(inverse, Vec3::new(x, y, 1.0))?;
        Ray3::new(near, far - near)
    }

    pub fn project(&self, point: Vec3, viewport: Rect) -> Option<ProjectedPoint> {
        if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
            return None;
        }
        let clip = self.view_projection(viewport.width() / viewport.height()) * point.extend(1.0);
        if !clip.is_finite() || clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if !(0.0..=1.0).contains(&ndc.z) {
            return None;
        }
        let screen = Pos2::new(
            viewport.left() + (ndc.x + 1.0) * 0.5 * viewport.width(),
            viewport.top() + (1.0 - ndc.y) * 0.5 * viewport.height(),
        );
        Some(ProjectedPoint {
            screen,
            depth: ndc.z,
        })
    }

    pub fn world_units_per_point_at(&self, point: Vec3, viewport_height_points: f32) -> f32 {
        let height = viewport_height_points.max(1.0);
        match self.projection_mode {
            ProjectionMode::Perspective => {
                let eye = self.eye();
                let view_forward = (self.target - eye).normalize_or_zero();
                let depth = (point - eye)
                    .dot(view_forward)
                    .abs()
                    .max(self.frame_radius * 1.0e-4)
                    .max(1.0e-6);
                2.0 * depth * self.half_fov_tan() / height
            }
            ProjectionMode::Orthographic => self.orthographic_scale / height,
        }
    }

    pub fn world_drag_delta_at(
        &self,
        point: Vec3,
        delta_points: Vec2,
        viewport_height_points: f32,
    ) -> Vec3 {
        let scale = self.world_units_per_point_at(point, viewport_height_points);
        let (_, right, up) = self.basis();
        right * (delta_points.x * scale) + up * (-delta_points.y * scale)
    }

    pub fn look_from_standard_view(&mut self, view: StandardView) {
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
        // The diagonals lift by a quarter turn rather than the pitch limit: at
        // the limit the camera is overhead and the face is edge on, which is
        // exactly the framing these keys exist to avoid.
        const DIAGONAL_PITCH: f32 = FRAC_PI_4 * 0.65;
        let (yaw, pitch) = match view {
            StandardView::LeftSide => (-FRAC_PI_2, 0.0),
            StandardView::RightSide => (FRAC_PI_2, 0.0),
            StandardView::Top => (0.0, PITCH_LIMIT_RADIANS),
            StandardView::Bottom => (0.0, -PITCH_LIMIT_RADIANS),
            StandardView::FrontUpperLeft => (-FRAC_PI_4, DIAGONAL_PITCH),
            StandardView::FrontUpperRight => (FRAC_PI_4, DIAGONAL_PITCH),
            StandardView::FrontLowerLeft => (-FRAC_PI_4, -DIAGONAL_PITCH),
            StandardView::FrontLowerRight => (FRAC_PI_4, -DIAGONAL_PITCH),
        };
        self.yaw = yaw;
        self.pitch = pitch;

        self.roll = 0.0;
        self.snap_view = false;
    }

    fn snapped_angles(&self) -> (f32, f32) {
        let yaw = (self.yaw / VIEW_SNAP_RADIANS).round() * VIEW_SNAP_RADIANS;
        let pitch = ((self.pitch / VIEW_SNAP_RADIANS).round() * VIEW_SNAP_RADIANS)
            .clamp(-PITCH_LIMIT_RADIANS, PITCH_LIMIT_RADIANS);
        (yaw, pitch)
    }

    pub fn commit_snap(&mut self) {
        if self.snap_view {
            let (yaw, pitch) = self.snapped_angles();
            self.yaw = yaw;
            self.pitch = pitch;
            self.snap_view = false;
        }
    }

    fn effective_angles(&self) -> (f32, f32) {
        if self.snap_view {
            self.snapped_angles()
        } else {
            (self.yaw, self.pitch)
        }
    }

    fn apply_orbit_angles(&mut self, orbit_points: Vec2) {
        self.yaw -= orbit_points.x * ORBIT_RADIANS_PER_POINT;
        self.pitch = (self.pitch + orbit_points.y * ORBIT_RADIANS_PER_POINT)
            .clamp(-PITCH_LIMIT_RADIANS, PITCH_LIMIT_RADIANS);
    }

    fn view_forward(&self) -> DVec3 {
        let (forward_from_target, _, _) = self.basis();
        (-forward_from_target).as_dvec3()
    }

    fn cursor_world_point_at_depth(
        &self,
        cursor: Pos2,
        viewport: Rect,
        depth: f64,
    ) -> Option<DVec3> {
        if viewport.width() <= 0.0 || viewport.height() <= 0.0 || !viewport.contains(cursor) {
            return None;
        }
        let ndc_x = f64::from((cursor.x - viewport.left()) / viewport.width()) * 2.0 - 1.0;
        let ndc_y = 1.0 - f64::from((cursor.y - viewport.top()) / viewport.height()) * 2.0;
        let aspect = f64::from(viewport.width() / viewport.height().max(1.0e-4));
        let (forward_from_target, right, up) = self.basis();
        let (lateral_x, lateral_y) = match self.projection_mode {
            ProjectionMode::Perspective => {
                let half_fov_tan = f64::from(self.half_fov_tan());
                (
                    ndc_x * half_fov_tan * aspect * depth,
                    ndc_y * half_fov_tan * depth,
                )
            }
            ProjectionMode::Orthographic => {
                let half_height =
                    f64::from(self.orthographic_scale.max(self.minimum_ortho_scale())) * 0.5;
                (ndc_x * half_height * aspect, ndc_y * half_height)
            }
        };
        let point = self.eye().as_dvec3()
            + (-forward_from_target).as_dvec3() * depth
            + right.as_dvec3() * lateral_x
            + up.as_dvec3() * lateral_y;
        point.is_finite().then_some(point)
    }

    fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let (yaw, pitch) = self.effective_angles();
        let cos_pitch = pitch.cos();
        let forward_from_target =
            Vec3::new(yaw.sin() * cos_pitch, pitch.sin(), yaw.cos() * cos_pitch)
                .normalize_or_zero();
        let right = Vec3::Y.cross(forward_from_target).normalize_or_zero();
        let up = forward_from_target.cross(right).normalize_or_zero();
        let roll = self.effective_roll();
        if roll == 0.0 {
            return (forward_from_target, right, up);
        }

        let (sin, cos) = roll.sin_cos();
        let rolled_right = (right * cos + up * sin).normalize_or_zero();
        let rolled_up = (up * cos - right * sin).normalize_or_zero();
        (forward_from_target, rolled_right, rolled_up)
    }

    fn effective_roll(&self) -> f32 {
        if self.roll.is_finite() {
            self.roll.rem_euclid(std::f32::consts::TAU)
        } else {
            0.0
        }
    }

    fn frame_current_target(&mut self) {
        self.frame_radius = if self.frame_radius.is_finite() {
            self.frame_radius.max(1.0e-4)
        } else {
            1.0
        };
        let half_height = self.framed_half_height();
        self.distance =
            (half_height / self.half_fov_tan() * FRAME_MARGIN).max(self.frame_radius * 1.25);
        self.orthographic_scale = half_height * 2.0 * FRAME_MARGIN;
    }

    /// How much of the framed box has to fit vertically, measured along the axes the camera is
    /// actually looking down. Falls back to the bounding-sphere radius when no box is on record,
    /// and is clamped to it so the fit is never looser than it used to be.
    fn framed_half_height(&self) -> f32 {
        let Some(bounds) = self.frame_bounds else {
            return self.frame_radius;
        };
        let half = (bounds.max - bounds.min) * 0.5;
        if !half.is_finite() {
            return self.frame_radius;
        }
        let (_, right, up) = self.basis();
        let along =
            |axis: Vec3| half.x * axis.x.abs() + half.y * axis.y.abs() + half.z * axis.z.abs();
        along(up)
            .max(along(right))
            .max(self.frame_radius * 1.0e-3)
            .min(self.frame_radius)
    }

    fn safe_fov_y_radians(&self) -> f32 {
        let degrees = self.fov_y_radians.to_degrees();
        if degrees.is_finite() {
            degrees
                .clamp(MIN_FOV_Y_DEGREES, MAX_FOV_Y_DEGREES)
                .to_radians()
        } else {
            DEFAULT_FOV_Y_RADIANS
        }
    }

    fn half_fov_tan(&self) -> f32 {
        (self.safe_fov_y_radians() * 0.5).tan().max(1.0e-4)
    }

    fn minimum_distance(&self) -> f32 {
        (self.frame_radius * 0.015).max(1.0e-5)
    }

    fn maximum_distance(&self) -> f32 {
        (self.frame_radius * 10_000.0).max(10.0)
    }

    fn minimum_ortho_scale(&self) -> f32 {
        (self.frame_radius * 0.001).max(1.0e-6)
    }

    fn maximum_ortho_scale(&self) -> f32 {
        (self.frame_radius * 20_000.0).max(20.0)
    }
}

fn unproject(inverse_view_projection: Mat4, point: Vec3) -> Option<Vec3> {
    let homogeneous: Vec4 = inverse_view_projection * point.extend(1.0);
    if !homogeneous.is_finite() || homogeneous.w.abs() <= 1.0e-8 {
        return None;
    }
    Some(homogeneous.truncate() / homogeneous.w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_views_look_along_their_axes() {
        let axis = |view| {
            let mut camera = TurntableCamera::default();
            camera.look_from_standard_view(view);
            let (forward_from_target, _, _) = camera.basis();
            forward_from_target
        };
        assert!(axis(StandardView::LeftSide).x < -0.99);
        assert!(axis(StandardView::RightSide).x > 0.99);
        assert!(axis(StandardView::Top).y > 0.99);
        assert!(axis(StandardView::Bottom).y < -0.99);

        // The diagonals are checked by sign rather than by angle, because a
        // wrong yaw sign is the mistake that reads correctly in the source and
        // wrong on screen: the view swings to the far side of the face.
        for (view, side, height) in [
            (StandardView::FrontUpperLeft, -1.0, 1.0),
            (StandardView::FrontUpperRight, 1.0, 1.0),
            (StandardView::FrontLowerLeft, -1.0, -1.0),
            (StandardView::FrontLowerRight, 1.0, -1.0),
        ] {
            let forward = axis(view);
            assert!(
                forward.x * f32::from(side as i8) > 0.2,
                "{view:?} must sit on its own side of the face"
            );
            assert!(
                forward.y * f32::from(height as i8) > 0.2,
                "{view:?} must sit above or below as its key suggests"
            );
            assert!(
                forward.z > 0.2,
                "{view:?} is a front diagonal and must stay in front of the face"
            );
        }

        let (_, right, up) = {
            let mut camera = TurntableCamera::default();
            camera.look_from_standard_view(StandardView::Top);
            camera.basis()
        };
        assert!(right.length() > 0.5 && up.length() > 0.5);
    }

    #[test]
    fn alt_orbit_snaps_the_view_without_freezing_the_drag() {
        let mut camera = TurntableCamera {
            yaw: 0.0,
            pitch: 0.0,
            ..Default::default()
        };

        for _ in 0..5 {
            camera.apply_delta(CameraDelta {
                orbit_points: Vec2::new(20.0, 0.0),
                viewport_height_points: 720.0,
                ..Default::default()
            });
            camera.snap_view = true;
        }
        assert!(camera.yaw.abs() > 0.1, "the drag kept accumulating");
        let (rendered_yaw, _) = camera.effective_angles();
        assert!(
            (rendered_yaw % VIEW_SNAP_RADIANS).abs() < 1.0e-4,
            "rendered yaw {rendered_yaw} is off the 45-degree grid"
        );

        camera.commit_snap();
        assert!(!camera.snap_view);
        assert!((camera.yaw % VIEW_SNAP_RADIANS).abs() < 1.0e-4);
    }

    #[test]
    fn viewport_size_alone_is_not_camera_motion() {
        let delta = CameraDelta {
            viewport_height_points: 720.0,
            ..Default::default()
        };
        assert!(!delta.has_motion());
    }

    fn viewport() -> Rect {
        Rect::from_min_max(Pos2::new(20.0, 30.0), Pos2::new(820.0, 630.0))
    }

    #[test]
    fn target_projects_to_viewport_center() {
        let camera = TurntableCamera::default();
        let projected = camera.project(camera.target, viewport()).unwrap();
        assert!((projected.screen.x - viewport().center().x).abs() < 1.0e-3);
        assert!((projected.screen.y - viewport().center().y).abs() < 1.0e-3);
    }

    #[test]
    fn default_and_reset_use_natural_fifteen_degree_orientation() {
        let mut camera = TurntableCamera::default();
        assert!((camera.yaw.to_degrees() - 15.0).abs() < 1.0e-5);
        assert!((camera.pitch.to_degrees() - 15.0).abs() < 1.0e-5);

        camera.yaw = -1.1;
        camera.pitch = 0.8;
        camera.target = Vec3::new(4.0, 5.0, 6.0);
        camera.frame_radius = 2.5;
        camera.distance = 999.0;
        camera.orthographic_scale = 999.0;
        camera.reset_view();

        assert!((camera.yaw.to_degrees() - 15.0).abs() < 1.0e-5);
        assert!((camera.pitch.to_degrees() - 15.0).abs() < 1.0e-5);
        assert_eq!(camera.target, Vec3::new(4.0, 5.0, 6.0));
        assert_eq!(camera.frame_radius, 2.5);
        assert!(camera.distance > camera.frame_radius);
        assert!((camera.orthographic_scale - 2.0 * 2.5 * FRAME_MARGIN).abs() < 1.0e-5);
    }

    #[test]
    fn fov_setter_rejects_non_finite_values_and_clamps_extremes() {
        let mut camera = TurntableCamera::default();
        assert_eq!(camera.set_fov_y_degrees(-500.0), MIN_FOV_Y_DEGREES);
        assert!((camera.fov_y_degrees() - MIN_FOV_Y_DEGREES).abs() < 1.0e-5);
        assert_eq!(camera.set_fov_y_degrees(500.0), MAX_FOV_Y_DEGREES);
        assert!((camera.fov_y_degrees() - MAX_FOV_Y_DEGREES).abs() < 1.0e-5);
        assert_eq!(camera.set_fov_y_degrees(f32::NAN), DEFAULT_FOV_Y_DEGREES);
        assert!(camera.view_projection(1.0).is_finite());
    }

    #[test]
    fn dolly_compensated_fov_keeps_projected_size_at_the_pivot() {
        for (from, to) in [
            (38.0_f32, 70.0_f32),
            (70.0, 24.0),
            (24.0, 110.0),
            (110.0, 10.0),
        ] {
            let mut camera = TurntableCamera {
                distance: 3.2,
                ..Default::default()
            };
            camera.set_fov_y_degrees(from);
            let framed_span = camera.distance * camera.half_fov_tan();
            let (_, right, up) = camera.basis();

            let probe = camera.target + right * 0.25 - up * 0.4;
            let before = camera.project(probe, viewport()).unwrap();
            camera.set_fov_y_degrees_with_dolly_compensation(to);
            let after = camera.project(probe, viewport()).unwrap();
            assert!((camera.fov_y_degrees() - to).abs() < 1.0e-4);
            assert!(
                (camera.distance * camera.half_fov_tan() - framed_span).abs() < 1.0e-5,
                "fov {from} -> {to} must keep distance * tan(fov/2) constant"
            );
            assert!(
                before.screen.distance(after.screen) < 0.05,
                "fov {from} -> {to} moved the probe by {} points",
                before.screen.distance(after.screen)
            );
        }
    }

    #[test]
    fn dolly_compensation_leaves_orthographic_framing_untouched() {
        let mut camera = TurntableCamera::default();
        camera.set_projection_mode(ProjectionMode::Orthographic);
        let distance = camera.distance;
        let scale = camera.orthographic_scale;
        camera.set_fov_y_degrees_with_dolly_compensation(95.0);
        assert_eq!(camera.distance, distance);
        assert_eq!(camera.orthographic_scale, scale);
        assert!((camera.fov_y_degrees() - 95.0).abs() < 1.0e-4);
    }

    #[test]
    fn reset_with_default_fov_is_identical_from_any_fov_and_zoom_history() {
        let bounds = Bounds3::from_points(&[Vec3::new(-0.4, -0.5, -0.3), Vec3::new(0.5, 0.6, 0.4)]);
        let mut reference = TurntableCamera::default();
        reference.reset_view_with_default_fov(Some(bounds));

        let mut framed = TurntableCamera::default();
        framed.frame(bounds);
        assert_eq!(reference.distance, framed.distance);
        assert_eq!(reference.target, framed.target);
        assert!((reference.fov_y_degrees() - DEFAULT_FOV_Y_DEGREES).abs() < 1.0e-5);

        for start_fov in [MAX_FOV_Y_DEGREES, MIN_FOV_Y_DEGREES, 77.0] {
            let mut camera = TurntableCamera::default();
            camera.set_fov_y_degrees_with_dolly_compensation(start_fov);
            camera.apply_delta(CameraDelta {
                orbit_points: Vec2::new(120.0, -60.0),
                pan_points: Vec2::new(40.0, -25.0),
                roll_radians: 0.0,
                scroll_points: 300.0,
                viewport_height_points: 600.0,
            });
            camera.zoom_about_screen_point(-90.0, viewport().center(), viewport(), None);
            camera.reset_view_with_default_fov(Some(bounds));
            assert_eq!(
                camera, reference,
                "reset from fov {start_fov} must restore the canonical camera"
            );
        }
    }

    #[test]
    fn cursor_zoom_keeps_the_anchor_point_under_the_cursor() {
        let mut camera = TurntableCamera::default();
        let (_, right, up) = camera.basis();
        let anchor = camera.target + right * 0.6 - up * 0.35;
        let cursor = camera.project(anchor, viewport()).unwrap().screen;
        let distance = camera.distance;
        camera.zoom_about_screen_point(240.0, cursor, viewport(), Some(anchor));
        let expected = distance * (-240.0 * ZOOM_PER_SCROLL_POINT).exp();
        assert!((camera.distance - expected).abs() < 1.0e-4);
        let after = camera.project(anchor, viewport()).unwrap().screen;
        assert!(
            cursor.distance(after) < 0.05,
            "anchor drifted {} points on screen",
            cursor.distance(after)
        );
    }

    #[test]
    fn cursor_zoom_without_a_hit_anchors_the_pivot_depth_plane() {
        let mut camera = TurntableCamera::default();
        let (_, right, up) = camera.basis();

        let probe = camera.target - right * 0.4 + up * 0.22;
        let cursor = camera.project(probe, viewport()).unwrap().screen;
        camera.zoom_about_screen_point(-180.0, cursor, viewport(), None);
        let after = camera.project(probe, viewport()).unwrap().screen;
        assert!(cursor.distance(after) < 0.05);
    }

    #[test]
    fn cursor_zoom_at_the_viewport_center_stays_a_centered_zoom() {
        let mut camera = TurntableCamera::default();
        let target = camera.target;
        let distance = camera.distance;
        camera.zoom_about_screen_point(120.0, viewport().center(), viewport(), None);
        assert!(camera.target.abs_diff_eq(target, 1.0e-4));
        assert!(camera.distance < distance);
    }

    #[test]
    fn cursor_zoom_anchors_in_orthographic_mode_too() {
        let mut camera = TurntableCamera::default();
        camera.set_projection_mode(ProjectionMode::Orthographic);
        camera.set_orthographic_scale(4.0);
        let (_, right, up) = camera.basis();
        let anchor = camera.target + right * 0.8 - up * 0.5;
        let cursor = camera.project(anchor, viewport()).unwrap().screen;
        let scale = camera.orthographic_scale;
        camera.zoom_about_screen_point(200.0, cursor, viewport(), Some(anchor));
        assert!(camera.orthographic_scale < scale);
        let after = camera.project(anchor, viewport()).unwrap().screen;
        assert!(cursor.distance(after) < 0.05);
    }

    #[test]
    fn center_ray_points_through_target() {
        let camera = TurntableCamera::default();
        let ray = camera
            .ray_from_screen(viewport().center(), viewport())
            .unwrap();
        let to_target = (camera.target.as_dvec3() - ray.origin).normalize();
        assert!(ray.direction.dot(to_target) > 0.999_99);
    }

    #[test]
    fn perspective_projection_has_depth_dependent_screen_scale() {
        let camera = TurntableCamera::default();
        let (_, right, _) = camera.basis();
        let toward_eye = (camera.eye() - camera.target).normalize();
        let at_target = camera.project(camera.target + right, viewport()).unwrap();
        let nearer = camera
            .project(camera.target + right + toward_eye, viewport())
            .unwrap();
        let center_x = viewport().center().x;
        assert!((nearer.screen.x - center_x).abs() > (at_target.screen.x - center_x).abs());
    }

    #[test]
    fn orthographic_projection_rays_and_world_scale_are_depth_independent() {
        let mut camera = TurntableCamera::default();
        camera.set_projection_mode(ProjectionMode::Orthographic);
        camera.set_orthographic_scale(4.0);

        let center = camera.project(camera.target, viewport()).unwrap();
        assert!((center.screen.x - viewport().center().x).abs() < 1.0e-3);
        assert!((center.screen.y - viewport().center().y).abs() < 1.0e-3);

        let left = camera
            .ray_from_screen(
                Pos2::new(viewport().left() + 100.0, viewport().center().y),
                viewport(),
            )
            .unwrap();
        let right = camera
            .ray_from_screen(
                Pos2::new(viewport().right() - 100.0, viewport().center().y),
                viewport(),
            )
            .unwrap();
        assert!(left.direction.dot(right.direction) > 0.999_999);
        assert!((left.origin - right.origin).length() > 0.1);

        let (_, _, up) = camera.basis();
        let at_target = camera.world_units_per_point_at(camera.target, viewport().height());
        let far_away =
            camera.world_units_per_point_at(camera.target + up * 100.0, viewport().height());
        assert!((at_target - 4.0 / viewport().height()).abs() < 1.0e-6);
        assert_eq!(at_target, far_away);
    }

    #[test]
    fn changing_projection_preserves_visible_vertical_span() {
        let mut camera = TurntableCamera {
            distance: 7.25,
            ..Default::default()
        };
        let perspective_span = 2.0 * camera.distance * camera.half_fov_tan();
        camera.set_projection_mode(ProjectionMode::Orthographic);
        assert!((camera.orthographic_scale - perspective_span).abs() < 1.0e-5);
        camera.set_projection_mode(ProjectionMode::Perspective);
        assert!((camera.distance - 7.25).abs() < 1.0e-4);
    }

    #[test]
    fn rolling_turns_the_view_without_moving_the_camera() {
        let mut camera = TurntableCamera::default();
        let eye_before = camera.eye();
        let target_before = camera.target;
        camera.apply_delta(CameraDelta {
            roll_radians: 0.7,
            ..Default::default()
        });
        assert!((camera.eye() - eye_before).length() < 1.0e-5);
        assert_eq!(camera.target, target_before);
        assert!((camera.roll - 0.7).abs() < 1.0e-6);
    }

    #[test]
    fn screen_up_turns_by_exactly_the_angle_rolled() {
        use std::f32::consts::FRAC_PI_2;
        let upright = TurntableCamera::default();
        let (_, right_before, up_before) = upright.basis();
        let mut rolled = TurntableCamera::default();
        rolled.apply_delta(CameraDelta {
            roll_radians: FRAC_PI_2,
            ..Default::default()
        });
        let (_, right_after, up_after) = rolled.basis();

        assert!((up_after + right_before).length() < 1.0e-5, "{up_after:?}");
        assert!(
            (right_after - up_before).length() < 1.0e-5,
            "{right_after:?}"
        );

        assert!((right_after.dot(up_after)).abs() < 1.0e-5);
        assert!((right_after.length() - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn rolling_all_the_way_round_returns_to_where_it_started() {
        use std::f32::consts::TAU;
        let upright = TurntableCamera::default();
        let mut camera = TurntableCamera::default();
        for _ in 0..40 {
            camera.apply_delta(CameraDelta {
                roll_radians: TAU / 8.0,
                ..Default::default()
            });
        }
        assert!(camera.roll < TAU, "roll wound up to {}", camera.roll);
        let (_, right, up) = camera.basis();
        let (_, expected_right, expected_up) = upright.basis();
        assert!((right - expected_right).length() < 1.0e-4);
        assert!((up - expected_up).length() < 1.0e-4);
    }

    #[test]
    fn the_view_matrix_uses_the_rolled_up_and_not_world_up() {
        let mut camera = TurntableCamera::default();
        camera.apply_delta(CameraDelta {
            roll_radians: 0.9,
            ..Default::default()
        });
        let view_projection = camera.view_projection(1.5);
        let (_, _, up) = camera.basis();

        let above = camera.project(camera.target + up * 0.2, viewport());
        let centre = camera.project(camera.target, viewport());
        let (Some(above), Some(centre)) = (above, centre) else {
            panic!("both points are in front of the camera");
        };
        assert!(
            above.screen.y < centre.screen.y,
            "rolled up did not project upward: {above:?} vs {centre:?}"
        );
        assert!(
            (above.screen.x - centre.screen.x).abs() < 0.5,
            "rolled up drifted sideways"
        );
        assert!(view_projection.is_finite());
    }

    #[test]
    fn resetting_and_naming_a_view_both_put_the_horizon_back() {
        let mut camera = TurntableCamera::default();
        camera.apply_delta(CameraDelta {
            roll_radians: 1.3,
            ..Default::default()
        });
        camera.reset_view();
        assert_eq!(camera.roll, 0.0);

        camera.apply_delta(CameraDelta {
            roll_radians: 1.3,
            ..Default::default()
        });
        camera.look_from_standard_view(StandardView::RightSide);
        assert_eq!(camera.roll, 0.0, "a named view kept a tilt");
    }

    #[test]
    fn rolling_at_the_pole_still_gives_an_orthonormal_frame() {
        for view in [StandardView::Top, StandardView::Bottom] {
            let mut camera = TurntableCamera::default();
            camera.look_from_standard_view(view);
            camera.apply_delta(CameraDelta {
                roll_radians: 0.6,
                ..Default::default()
            });
            let (forward, right, up) = camera.basis();
            for axis in [forward, right, up] {
                assert!(
                    (axis.length() - 1.0).abs() < 1.0e-4,
                    "{view:?}: {axis:?} is not a unit vector"
                );
            }
            assert!(
                right.dot(up).abs() < 1.0e-4,
                "{view:?}: frame is not square"
            );
            assert!(camera.view_projection(1.5).is_finite());
        }
    }
    #[test]
    fn camera_delta_is_deterministic_and_pitch_is_bounded() {
        let delta = CameraDelta {
            orbit_points: Vec2::new(17.0, 50_000.0),
            pan_points: Vec2::new(-4.0, 9.0),
            roll_radians: 0.0,
            scroll_points: 120.0,
            viewport_height_points: 600.0,
        };
        let mut first = TurntableCamera::default();
        let mut second = first;
        first.apply_delta(delta);
        second.apply_delta(delta);
        assert_eq!(first, second);
        assert!(first.pitch < std::f32::consts::FRAC_PI_2);
        assert!(first.distance > 0.0);
    }

    #[test]
    fn framing_fits_the_box_the_camera_sees_not_its_bounding_sphere() {
        let bounds =
            Bounds3::from_points(&[Vec3::new(-8.0, -13.5, -12.0), Vec3::new(8.0, 13.5, 12.0)]);
        let mut tight = TurntableCamera::default();
        tight.frame(bounds);

        // The old fit, reproduced: no box on record, so the bounding sphere is all there is.
        let mut sphere = TurntableCamera {
            target: bounds.center(),
            frame_radius: bounds.radius(),
            ..Default::default()
        };
        sphere.reset_view();

        assert_eq!(
            tight.frame_radius, sphere.frame_radius,
            "frame_radius must go on meaning scene scale for lighting and near/far"
        );
        assert!(
            tight.distance < sphere.distance,
            "a head-shaped box framed as a sphere sits {:.0}% too far back",
            (sphere.distance / tight.distance - 1.0) * 100.0
        );
        assert!(tight.orthographic_scale < sphere.orthographic_scale);
        assert!(tight.distance > tight.frame_radius);
    }

    #[test]
    fn frame_keeps_nonzero_camera_for_flat_mesh() {
        let mut camera = TurntableCamera::default();
        camera.frame(Bounds3::from_points(&[
            Vec3::new(-1.0, -2.0, 0.0),
            Vec3::new(1.0, 2.0, 0.0),
        ]));
        assert_eq!(camera.target, Vec3::ZERO);
        assert!(camera.frame_radius > 0.0);
        assert!(camera.distance > camera.frame_radius);
    }

    #[test]
    fn screen_drag_translation_tracks_camera_right_and_up() {
        let camera = TurntableCamera::default();
        let point = camera.target;
        let right = camera.world_drag_delta_at(point, Vec2::new(10.0, 0.0), 600.0);
        let up = camera.world_drag_delta_at(point, Vec2::new(0.0, -10.0), 600.0);
        assert!(right.length() > 0.0 && up.length() > 0.0);
        assert!(right.dot(up).abs() < 1.0e-5);
    }
}
