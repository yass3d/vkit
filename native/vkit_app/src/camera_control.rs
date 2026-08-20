use egui::Vec2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraGesture {
    Orbit(Vec2),

    Pan(Vec2),

    Dolly(f32),
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

#[cfg(test)]
mod tests {
    use super::*;

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
        use crate::shortcuts::{Shortcut, Trigger};
        for shortcut in [Shortcut::ViewOrbit, Shortcut::ViewPan, Shortcut::ViewDolly] {
            assert_eq!(
                shortcut.default_binding().trigger,
                Trigger::Mouse(egui::PointerButton::Middle),
                "{shortcut:?} starts on the middle button"
            );
        }
        assert_eq!(Shortcut::ViewOrbit.default_binding().label(), "Wheel click");
        assert_eq!(
            Shortcut::ViewPan.default_binding().label(),
            "Shift+Wheel click"
        );
        assert_eq!(
            Shortcut::ViewDolly.default_binding().label(),
            "Ctrl+Wheel click"
        );

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
}
