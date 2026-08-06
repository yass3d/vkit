use egui::{Pos2, Vec2};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraGesture {
    Orbit(Vec2),

    Pan(Vec2),

    Dolly(f32),

    Trackball { orbit: Vec2, roll: f32 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TrackballTurn {
    pub orbit: Vec2,

    pub roll: f32,
}

fn on_sphere(centre: Pos2, radius: f32, point: Pos2) -> [f32; 3] {
    let x = (point.x - centre.x) / radius;

    let y = -(point.y - centre.y) / radius;
    let squared = x.mul_add(x, y * y);
    let z = if squared <= 0.5 {
        (1.0 - squared).sqrt()
    } else {
        0.5 / squared.sqrt()
    };
    let length = z.mul_add(z, squared).sqrt();
    if length > 0.0 {
        [x / length, y / length, z / length]
    } else {
        [0.0, 0.0, 1.0]
    }
}

pub fn trackball_about(centre: Pos2, radius: f32, from: Pos2, to: Pos2) -> TrackballTurn {
    if !(radius.is_finite() && radius > 0.0 && from.is_finite() && to.is_finite()) {
        return TrackballTurn::default();
    }
    let before = on_sphere(centre, radius, from);
    let after = on_sphere(centre, radius, to);

    let cross_z = before[0].mul_add(after[1], -(before[1] * after[0]));
    let dot = before[0].mul_add(after[0], before[1].mul_add(after[1], before[2] * after[2]));
    let angle = dot.clamp(-1.0, 1.0).acos();
    let sine = cross_z.hypot(
        before[1]
            .mul_add(after[2], -(before[2] * after[1]))
            .hypot(before[2].mul_add(after[0], -(before[0] * after[2]))),
    );
    let roll = if sine > 1.0e-9 && angle.is_finite() {
        -angle * (cross_z / sine)
    } else {
        0.0
    };
    TrackballTurn {
        orbit: to - from,
        roll: if roll.is_finite() { roll } else { 0.0 },
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ControlMode {
    #[default]
    Orbit,

    Trackball,
}

impl ControlMode {
    pub const fn needs_an_exit(self) -> bool {
        match self {
            Self::Orbit => false,
            Self::Trackball => true,
        }
    }
}

pub fn unroll_drag(drag: Vec2, roll: f32) -> Vec2 {
    if roll == 0.0 || !roll.is_finite() || !drag.is_finite() {
        return drag;
    }
    let (sin, cos) = (-roll).sin_cos();
    Vec2::new(drag.x * cos - drag.y * sin, drag.x * sin + drag.y * cos)
}

pub const fn middle_drag_gesture(shift_down: bool, ctrl_down: bool) -> MiddleDragBinding {
    if ctrl_down {
        MiddleDragBinding::Dolly
    } else if shift_down {
        MiddleDragBinding::Pan
    } else {
        MiddleDragBinding::Orbit
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MiddleDragBinding {
    Orbit,
    Pan,
    Dolly,
}

impl MiddleDragBinding {
    pub fn gesture(self, motion: egui::Vec2, roll: f32) -> Option<CameraGesture> {
        match self {
            Self::Orbit => {
                let motion = unroll_drag(motion, roll);
                (motion != Vec2::ZERO).then_some(CameraGesture::Orbit(motion))
            }
            Self::Pan => (motion != Vec2::ZERO).then_some(CameraGesture::Pan(motion)),

            Self::Dolly => (motion.y != 0.0).then_some(CameraGesture::Dolly(-motion.y)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragSample {
    pub from: Pos2,
    pub to: Pos2,

    pub pivot: Pos2,

    pub radius: f32,
}

impl DragSample {
    pub fn motion(self) -> Vec2 {
        self.to - self.from
    }
}

pub fn interpret_drag(mode: ControlMode, drag: DragSample, roll: f32) -> Option<CameraGesture> {
    match mode {
        ControlMode::Orbit => {
            let motion = unroll_drag(drag.motion(), roll);
            (motion != Vec2::ZERO).then_some(CameraGesture::Orbit(motion))
        }
        ControlMode::Trackball => {
            let turn = trackball_about(drag.pivot, drag.radius, drag.from, drag.to);
            (turn.orbit != Vec2::ZERO || turn.roll != 0.0).then_some(CameraGesture::Trackball {
                orbit: turn.orbit,
                roll: turn.roll,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f32, y: f32) -> Pos2 {
        Pos2::new(x, y)
    }

    #[test]
    fn a_drag_is_measured_in_the_frame_the_user_sees() {
        use std::f32::consts::FRAC_PI_2;
        let rightward = Vec2::new(10.0, 0.0);
        assert_eq!(unroll_drag(rightward, 0.0), rightward);

        let unrolled = unroll_drag(rightward, FRAC_PI_2);
        assert!((unrolled.x).abs() < 1.0e-5, "{unrolled:?}");
        assert!((unrolled.y + 10.0).abs() < 1.0e-5, "{unrolled:?}");

        for roll in [0.3, 1.0, 2.5, -1.7] {
            let turned = unroll_drag(rightward, roll);
            assert!((turned.length() - rightward.length()).abs() < 1.0e-4);
        }
    }

    #[test]
    fn the_middle_button_means_what_the_modifiers_say() {
        assert_eq!(middle_drag_gesture(false, false), MiddleDragBinding::Orbit);
        assert_eq!(middle_drag_gesture(true, false), MiddleDragBinding::Pan);

        assert_eq!(middle_drag_gesture(true, true), MiddleDragBinding::Dolly);

        let motion = Vec2::new(3.0, -7.0);
        assert_eq!(
            MiddleDragBinding::Orbit.gesture(motion, 0.0),
            Some(CameraGesture::Orbit(motion))
        );
        assert_eq!(
            MiddleDragBinding::Pan.gesture(motion, 0.0),
            Some(CameraGesture::Pan(motion))
        );

        assert_eq!(
            MiddleDragBinding::Dolly.gesture(motion, 0.0),
            Some(CameraGesture::Dolly(7.0))
        );
        for binding in [
            MiddleDragBinding::Orbit,
            MiddleDragBinding::Pan,
            MiddleDragBinding::Dolly,
        ] {
            assert_eq!(binding.gesture(Vec2::ZERO, 0.0), None, "{binding:?}");
        }
    }

    #[test]
    fn a_middle_drag_orbit_is_also_measured_in_the_frame_the_user_sees() {
        use std::f32::consts::FRAC_PI_2;
        let Some(CameraGesture::Orbit(orbit)) =
            MiddleDragBinding::Orbit.gesture(Vec2::new(10.0, 0.0), FRAC_PI_2)
        else {
            panic!("orbit binding must produce an orbit");
        };
        assert!(
            orbit.x.abs() < 1.0e-5 && (orbit.y + 10.0).abs() < 1.0e-5,
            "{orbit:?}"
        );
    }

    #[test]
    fn only_a_mode_that_has_to_be_left_announces_itself() {
        assert!(!ControlMode::default().needs_an_exit());
        assert!(ControlMode::Trackball.needs_an_exit());
    }

    #[test]
    fn one_drag_means_what_the_mode_says_it_means() {
        let drag = DragSample {
            from: point(160.0, 100.0),
            to: point(160.0, 120.0),
            pivot: point(100.0, 100.0),
            radius: 100.0,
        };
        assert_eq!(
            interpret_drag(ControlMode::Orbit, drag, 0.0),
            Some(CameraGesture::Orbit(Vec2::new(0.0, 20.0)))
        );
        let Some(CameraGesture::Trackball { orbit, .. }) =
            interpret_drag(ControlMode::Trackball, drag, 0.0)
        else {
            panic!("trackball mode must produce a trackball turn");
        };
        assert_eq!(orbit, Vec2::new(0.0, 20.0), "the orbit half is the motion");

        let still = DragSample {
            to: drag.from,
            ..drag
        };
        assert_eq!(interpret_drag(ControlMode::Orbit, still, 0.0), None);
        assert_eq!(interpret_drag(ControlMode::Trackball, still, 0.0), None);
    }

    #[test]
    fn a_trackball_rolls_at_the_rim_and_not_through_the_middle() {
        let centre = point(200.0, 200.0);
        let radius = 100.0;

        let through_middle =
            trackball_about(centre, radius, point(180.0, 200.0), point(220.0, 200.0));
        assert!(
            through_middle.roll.abs() < 1.0e-3,
            "a drag across the centre must not tilt the horizon: {}",
            through_middle.roll
        );

        let at_rim = trackball_about(centre, radius, point(360.0, 200.0), point(200.0, 360.0));
        assert!(
            at_rim.roll.abs() > 0.5,
            "circling the rim is what rolling is for: {}",
            at_rim.roll
        );

        let other_way = trackball_about(centre, radius, point(360.0, 200.0), point(200.0, 40.0));
        assert!(at_rim.roll * other_way.roll < 0.0);
    }
}
