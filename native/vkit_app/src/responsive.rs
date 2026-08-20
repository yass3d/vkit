#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Responsive {
    pub min: f32,

    pub ideal: f32,

    pub fraction: f32,
}

impl Responsive {
    pub fn resolve(self, available: f32) -> Option<f32> {
        if available.is_nan() || !(self.min.is_finite() && self.ideal.is_finite()) {
            return None;
        }
        let fraction = self.fraction.clamp(0.0, 1.0);

        let want = (available * fraction).min(self.ideal).min(available);
        (want >= self.min).then_some(want)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel() -> Responsive {
        Responsive {
            min: 160.0,
            ideal: 320.0,
            fraction: 0.4,
        }
    }

    #[test]
    fn more_room_stops_helping_once_the_ideal_is_reached() {
        assert_eq!(panel().resolve(4000.0), Some(320.0));
        assert_eq!(panel().resolve(800.0), Some(320.0));

        assert_eq!(panel().resolve(600.0), Some(240.0));
    }

    #[test]
    fn too_little_room_is_answered_with_nothing_rather_than_a_sliver() {
        assert_eq!(
            panel().resolve(300.0),
            None,
            "0.4 of 300 is under the floor"
        );
        assert_eq!(panel().resolve(400.0), Some(160.0));
    }

    #[test]
    fn it_never_asks_for_more_space_than_there_is() {
        let greedy = Responsive {
            min: 10.0,
            ideal: 10_000.0,
            fraction: 1.0,
        };
        assert_eq!(greedy.resolve(500.0), Some(500.0));
        let over = Responsive {
            fraction: 5.0,
            ..greedy
        };
        assert_eq!(over.resolve(500.0), Some(500.0), "a fraction above one");
    }

    #[test]
    fn a_fixed_size_does_not_flex_but_still_refuses_to_overflow() {
        let switch = Responsive {
            min: 36.0,
            ideal: 36.0,
            fraction: 1.0,
        };
        assert_eq!(switch.resolve(4000.0), Some(36.0));
        assert_eq!(switch.resolve(100.0), Some(36.0));
        assert_eq!(switch.resolve(20.0), None);
    }

    #[test]
    fn growing_the_container_never_shrinks_the_panel() {
        let mut previous = 0.0_f32;
        for width in (200..2000).step_by(10) {
            let Some(resolved) = panel().resolve(f32::from(u16::try_from(width).unwrap())) else {
                continue;
            };
            assert!(
                resolved >= previous - 1.0e-4,
                "at {width} it shrank from {previous} to {resolved}"
            );
            previous = resolved;
        }
    }

    #[test]
    fn nothing_infinite_resolves_to_a_size() {
        assert_eq!(panel().resolve(f32::NAN), None);
        assert_eq!(panel().resolve(f32::INFINITY), Some(320.0));
        let broken = Responsive {
            min: f32::NAN,
            ..panel()
        };
        assert_eq!(broken.resolve(800.0), None);
    }
}
