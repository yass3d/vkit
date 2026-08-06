#[derive(Clone, Debug, PartialEq)]
pub struct EyelidLookTarget {
    pub per_side: String,

    pub shared: &'static str,
    pub weight: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EyelidLookWeights {
    pub top_down: f64,

    pub top_up: f64,

    pub bottom_down: f64,

    pub bottom_up: f64,
}

impl EyelidLookWeights {
    #[must_use]
    pub fn named(&self, suffix: &str) -> [EyelidLookTarget; 4] {
        [
            EyelidLookTarget {
                per_side: format!("PHMEyelidsTopDown{suffix}"),
                shared: "CTRLEyeLidsTopDown",
                weight: self.top_down,
            },
            EyelidLookTarget {
                per_side: format!("PHMEyeLidsTopUp{suffix}"),
                shared: "CTRLEyeLidsTopUp",
                weight: self.top_up,
            },
            EyelidLookTarget {
                per_side: format!("PHMEyeLidsBottomDown{suffix}"),
                shared: "CTRLEyeLidsBottomDown",
                weight: self.bottom_down,
            },
            EyelidLookTarget {
                per_side: format!("PHMEyeLidsBottomUp{suffix}"),
                shared: "CTRLEyeLidsBottomUp",
                weight: self.bottom_up,
            },
        ]
    }

    #[must_use]
    pub fn is_rest(&self) -> bool {
        self.top_down <= 0.0
            && self.top_up <= 0.0
            && self.bottom_down <= 0.0
            && self.bottom_up <= 0.0
    }
}

const LOOK_UP_TOP_FACTOR: f64 = 3.0;
const LOOK_UP_BOTTOM_FACTOR: f64 = 1.0;
const LOOK_DOWN_TOP_FACTOR: f64 = 1.5;
const LOOK_DOWN_BOTTOM_FACTOR: f64 = 4.0;

#[must_use]
pub fn eyelid_look_weights(pitch_radians: f64) -> EyelidLookWeights {
    if !pitch_radians.is_finite() {
        return EyelidLookWeights::default();
    }
    if pitch_radians > 0.0 {
        EyelidLookWeights {
            top_down: (pitch_radians * LOOK_DOWN_TOP_FACTOR).clamp(0.0, 1.0),
            bottom_down: (pitch_radians * LOOK_DOWN_BOTTOM_FACTOR).clamp(0.0, 1.0),
            ..EyelidLookWeights::default()
        }
    } else {
        let up = -pitch_radians;
        EyelidLookWeights {
            top_up: (up * LOOK_UP_TOP_FACTOR).clamp(0.0, 1.0),
            bottom_up: (up * LOOK_UP_BOTTOM_FACTOR).clamp(0.0, 1.0),
            ..EyelidLookWeights::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EyelidLookRole {
    TopDown,
    TopUp,
    BottomDown,
    BottomUp,
}

#[must_use]
pub fn eyelid_look_role(internal_name: &str) -> Option<EyelidLookRole> {
    let lower = internal_name.to_ascii_lowercase();
    let lower = lower
        .strip_suffix('l')
        .filter(|_| lower.starts_with("phm"))
        .or_else(|| lower.strip_suffix('r').filter(|_| lower.starts_with("phm")))
        .unwrap_or(&lower);
    match lower {
        "phmeyelidstopdown" | "ctrleyelidstopdown" => Some(EyelidLookRole::TopDown),
        "phmeyelidstopup" | "ctrleyelidstopup" => Some(EyelidLookRole::TopUp),
        "phmeyelidsbottomdown" | "ctrleyelidsbottomdown" => Some(EyelidLookRole::BottomDown),
        "phmeyelidsbottomup" | "ctrleyelidsbottomup" => Some(EyelidLookRole::BottomUp),
        _ => None,
    }
}

#[must_use]
pub fn gaze_pitch_for_lid_weight(role: EyelidLookRole, weight: f64) -> f64 {
    let weight = weight.clamp(0.0, 1.0);
    match role {
        EyelidLookRole::TopDown => weight / LOOK_DOWN_TOP_FACTOR,
        EyelidLookRole::BottomDown => weight / LOOK_DOWN_BOTTOM_FACTOR,
        EyelidLookRole::TopUp => -(weight / LOOK_UP_TOP_FACTOR),
        EyelidLookRole::BottomUp => -(weight / LOOK_UP_BOTTOM_FACTOR),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_gaze_moves_no_lid() {
        assert!(eyelid_look_weights(0.0).is_rest());
        assert!(eyelid_look_weights(f64::NAN).is_rest());
        assert_eq!(eyelid_look_weights(0.0), EyelidLookWeights::default());
    }

    #[test]
    fn looking_up_and_looking_down_drive_opposite_pairs() {
        let up = eyelid_look_weights(-0.2);
        assert!(up.top_up > 0.0 && up.bottom_up > 0.0);
        assert_eq!((up.top_down, up.bottom_down), (0.0, 0.0));

        let down = eyelid_look_weights(0.2);
        assert!(down.top_down > 0.0 && down.bottom_down > 0.0);
        assert_eq!((down.top_up, down.bottom_up), (0.0, 0.0));
    }

    #[test]
    fn the_lid_nearest_the_gaze_moves_furthest() {
        let up = eyelid_look_weights(-0.2);
        assert!(
            up.top_up > up.bottom_up * 2.5,
            "an eye rolling up lifts its upper lid much further: {up:?}"
        );
        let down = eyelid_look_weights(0.2);
        assert!(
            down.bottom_down > down.top_down * 2.0,
            "and looking down drags the lower lid: {down:?}"
        );
    }

    #[test]
    fn an_extreme_gaze_saturates_rather_than_overshoots() {
        let down = eyelid_look_weights(0.6);
        assert_eq!(down.bottom_down, 1.0);
        assert!(down.top_down < 1.0, "and not everything saturates at once");
    }

    #[test]
    fn the_control_names_match_the_banks_own_spelling() {
        let names: Vec<_> = eyelid_look_weights(-0.1)
            .named("L")
            .into_iter()
            .map(|target| target.per_side)
            .collect();
        assert_eq!(
            names,
            vec![
                "PHMEyelidsTopDownL",
                "PHMEyeLidsTopUpL",
                "PHMEyeLidsBottomDownL",
                "PHMEyeLidsBottomUpL",
            ]
        );

        for target in eyelid_look_weights(0.1).named("R") {
            for name in [target.per_side.as_str(), target.shared] {
                assert!(
                    crate::vam::is_eyelid_look_control(name),
                    "{name} is not one of the controls the pack carries"
                );
            }
        }
    }
}
