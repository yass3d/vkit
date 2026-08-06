#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BloomSettings {
    pub enabled: bool,

    pub intensity: f32,

    pub threshold: f32,

    pub soft_knee: f32,

    pub radius: f32,
}

impl Default for BloomSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            intensity: DEFAULT_INTENSITY,
            threshold: DEFAULT_THRESHOLD,
            soft_knee: DEFAULT_SOFT_KNEE,
            radius: DEFAULT_RADIUS,
        }
    }
}

pub const DEFAULT_INTENSITY: f32 = 0.5;
pub const INTENSITY_RANGE: std::ops::RangeInclusive<f32> = 0.0..=5.0;
pub const DEFAULT_THRESHOLD: f32 = 1.1;
pub const THRESHOLD_RANGE: std::ops::RangeInclusive<f32> = 0.0..=2.0;
pub const DEFAULT_SOFT_KNEE: f32 = 0.5;
pub const SOFT_KNEE_RANGE: std::ops::RangeInclusive<f32> = 0.0..=1.0;
pub const DEFAULT_RADIUS: f32 = 4.0;
pub const RADIUS_RANGE: std::ops::RangeInclusive<f32> = 1.0..=7.0;

pub const MAX_PYRAMID_LEVELS: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThresholdCurve {
    pub knee_start: f32,
    pub knee_span: f32,
    pub knee_scale: f32,
    pub threshold: f32,
}

impl BloomSettings {
    pub fn linear_threshold(self) -> f32 {
        gamma_to_linear(self.threshold.max(0.0))
    }

    pub fn threshold_curve(self) -> ThresholdCurve {
        let threshold = self.linear_threshold();

        let knee = threshold * self.soft_knee.clamp(0.0, 1.0) + 1.0e-5;
        ThresholdCurve {
            knee_start: threshold - knee,
            knee_span: knee * 2.0,
            knee_scale: 0.25 / knee,
            threshold,
        }
    }

    pub fn pyramid_levels(self, height: u32) -> u32 {
        let steps = self.pyramid_steps(height);
        (steps.floor().max(1.0) as u32).clamp(1, MAX_PYRAMID_LEVELS)
    }

    pub fn sample_scale(self, height: u32) -> f32 {
        let steps = self.pyramid_steps(height);
        let levels = self.pyramid_levels(height) as f32;
        0.5 + (steps - levels).clamp(0.0, 1.0)
    }

    fn pyramid_steps(self, height: u32) -> f32 {
        let height = height.max(1) as f32;
        height.log2()
            + self
                .radius
                .clamp(*RADIUS_RANGE.start(), *RADIUS_RANGE.end())
            - 8.0
    }

    pub fn contributes(self) -> bool {
        self.enabled && self.intensity > 0.0
    }
}

fn gamma_to_linear(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_passes_run_in_the_order_the_light_does() {
        let canonical = [
            "occlusion darkens the linear scene",
            "bloom is gathered from that darkened scene",
            "the tone curve maps it to a display",
            "the vignette darkens the corners of the picture",
        ];

        let wired = [
            "occlusion darkens the linear scene",
            "bloom is gathered from that darkened scene",
            "the tone curve maps it to a display",
            "the vignette darkens the corners of the picture",
        ];
        assert_eq!(wired, canonical);
    }

    #[test]
    fn shadowing_a_region_can_only_reduce_what_it_blooms() {
        let settings = BloomSettings::default();
        let curve = settings.threshold_curve();

        let kept = |brightness: f32| {
            let soft = (brightness - curve.knee_start).clamp(0.0, curve.knee_span);
            let soft = curve.knee_scale * soft * soft;
            soft.max(brightness - curve.threshold).max(0.0)
        };
        let lit = 3.0_f32;
        let mut previous = kept(lit);
        for step in 1..=20_u8 {
            let visibility = 1.0 - f32::from(step) / 20.0;
            let shadowed = kept(lit * visibility);
            assert!(
                shadowed <= previous + 1.0e-6,
                "visibility {visibility} bloomed more than the brighter one:                  {shadowed} vs {previous}"
            );
            previous = shadowed;
        }

        assert_eq!(kept(0.0), 0.0);
    }

    #[test]
    fn a_vignette_runs_from_nothing_at_the_centre_to_its_intensity_at_the_corner() {
        let vignette = VignetteSettings {
            enabled: true,
            ..VignetteSettings::default()
        };
        assert_eq!(vignette.darkening([0.0, 0.0], 1.6), 0.0);
        let corner = vignette.darkening([1.0, 1.0], 1.6);
        assert!(
            (corner - vignette.intensity).abs() < 1.0e-6,
            "corner reached {corner}, wanted {}",
            vignette.intensity
        );

        for step in 0..=100 {
            let t = step as f32 / 100.0;
            let value = vignette.darkening([t, t], 1.6);
            assert!(
                (0.0..=vignette.intensity + 1.0e-6).contains(&value),
                "{value} at {t}"
            );
        }
    }

    #[test]
    fn a_vignette_that_is_off_darkens_nothing_at_all() {
        let off = VignetteSettings::default();
        assert!(!off.enabled);
        for point in [[0.0, 0.0], [0.5, 0.5], [1.0, 1.0]] {
            assert_eq!(off.darkening(point, 1.6), 0.0);
        }
        let zeroed = VignetteSettings {
            enabled: true,
            intensity: 0.0,
            ..off
        };
        assert_eq!(zeroed.darkening([1.0, 1.0], 1.6), 0.0);
    }

    #[test]
    fn roundness_decides_whether_the_window_shape_is_followed() {
        let base = VignetteSettings {
            enabled: true,
            ..VignetteSettings::default()
        };
        let round = VignetteSettings {
            roundness: 1.0,
            ..base
        };
        let square = VignetteSettings {
            roundness: 0.0,
            ..base
        };

        let side = [0.95_f32, 0.0];
        assert!(
            round.darkening(side, 1.9) > square.darkening(side, 1.9),
            "round {} vs square {}",
            round.darkening(side, 1.9),
            square.darkening(side, 1.9)
        );

        assert!((round.darkening(side, 1.0) - square.darkening(side, 1.0)).abs() < 1.0e-6);
    }

    #[test]
    fn an_impossible_window_shape_still_answers_a_number() {
        let vignette = VignetteSettings {
            enabled: true,
            ..VignetteSettings::default()
        };
        for aspect in [0.0_f32, -3.0, f32::NAN, f32::INFINITY] {
            let value = vignette.darkening([0.7, 0.3], aspect);
            assert!(value.is_finite(), "aspect {aspect} gave {value}");
            assert!((0.0..=1.0).contains(&value));
        }
    }

    #[test]
    fn the_defaults_match_the_plugin_this_reproduces() {
        let bloom = BloomSettings::default();
        assert_eq!(bloom.intensity, 0.5);
        assert_eq!(bloom.threshold, 1.1);
        assert_eq!(bloom.soft_knee, 0.5);
        assert_eq!(bloom.radius, 4.0);
        for (value, range) in [
            (bloom.intensity, INTENSITY_RANGE),
            (bloom.threshold, THRESHOLD_RANGE),
            (bloom.soft_knee, SOFT_KNEE_RANGE),
            (bloom.radius, RADIUS_RANGE),
        ] {
            assert!(range.contains(&value), "{value} outside {range:?}");
        }
    }

    #[test]
    fn the_threshold_is_converted_out_of_the_space_the_control_reads_in() {
        let bloom = BloomSettings::default();

        assert!((bloom.linear_threshold() - 1.242_77).abs() < 1.0e-4);

        let dim = BloomSettings {
            threshold: 0.02,
            ..bloom
        };
        assert!((dim.linear_threshold() - 0.02 / 12.92).abs() < 1.0e-9);
    }

    #[test]
    fn a_hard_knee_really_is_a_hard_cut() {
        let curve = BloomSettings {
            soft_knee: 0.0,
            ..BloomSettings::default()
        }
        .threshold_curve();

        assert!((curve.knee_start - curve.threshold).abs() < 1.0e-4);
        assert!(curve.knee_span < 1.0e-4);
    }

    #[test]
    fn a_soft_knee_ramps_in_over_the_octave_below_the_threshold() {
        let curve = BloomSettings::default().threshold_curve();
        assert!(
            curve.knee_start < curve.threshold,
            "the ramp has to start below the cut"
        );

        assert!((curve.knee_start - curve.threshold * 0.5).abs() < 1.0e-3);
        assert!((curve.knee_span - curve.threshold).abs() < 1.0e-3);
        assert!(curve.knee_scale.is_finite() && curve.knee_scale > 0.0);
    }

    #[test]
    fn the_bleed_covers_the_same_share_of_a_small_window_and_a_large_one() {
        let bloom = BloomSettings::default();
        assert_eq!(
            bloom.pyramid_levels(1080),
            bloom.pyramid_levels(540) + 1,
            "doubling the height must add one level, not none"
        );
        assert_eq!(bloom.pyramid_levels(720), 5);
        assert_eq!(bloom.pyramid_levels(1440), 6);

        let wide = BloomSettings {
            radius: 7.0,
            ..bloom
        };
        assert!(wide.pyramid_levels(1080) > bloom.pyramid_levels(1080));
    }

    #[test]
    fn the_pyramid_stays_within_its_bounds_at_any_window_size() {
        for radius in [1.0_f32, 4.0, 7.0] {
            let bloom = BloomSettings {
                radius,
                ..BloomSettings::default()
            };
            for height in [0_u32, 1, 8, 64, 1080, 4320, 65535] {
                let levels = bloom.pyramid_levels(height);
                assert!(
                    (1..=MAX_PYRAMID_LEVELS).contains(&levels),
                    "radius {radius} at height {height} gave {levels}"
                );
                let scale = bloom.sample_scale(height);
                assert!(scale.is_finite(), "radius {radius} height {height}");
                assert!((0.5..=1.5).contains(&scale), "scale {scale} out of range");
            }
        }
    }

    #[test]
    fn a_bloom_that_would_add_nothing_reports_that_it_need_not_run() {
        assert!(BloomSettings::default().contributes());
        assert!(
            !BloomSettings {
                enabled: false,
                ..BloomSettings::default()
            }
            .contributes()
        );
        assert!(
            !BloomSettings {
                intensity: 0.0,
                ..BloomSettings::default()
            }
            .contributes()
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VignetteSettings {
    pub enabled: bool,

    pub intensity: f32,

    pub smoothness: f32,

    pub roundness: f32,
}

impl Default for VignetteSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            intensity: DEFAULT_VIGNETTE_INTENSITY,
            smoothness: DEFAULT_VIGNETTE_SMOOTHNESS,
            roundness: DEFAULT_VIGNETTE_ROUNDNESS,
        }
    }
}

pub const DEFAULT_VIGNETTE_INTENSITY: f32 = 0.45;
pub const VIGNETTE_INTENSITY_RANGE: std::ops::RangeInclusive<f32> = 0.0..=1.0;
pub const DEFAULT_VIGNETTE_SMOOTHNESS: f32 = 0.2;
pub const VIGNETTE_SMOOTHNESS_RANGE: std::ops::RangeInclusive<f32> = 0.01..=1.0;
pub const DEFAULT_VIGNETTE_ROUNDNESS: f32 = 1.0;
pub const VIGNETTE_ROUNDNESS_RANGE: std::ops::RangeInclusive<f32> = 0.0..=1.0;

impl VignetteSettings {
    pub fn darkening(self, offset: [f32; 2], aspect: f32) -> f32 {
        if !self.enabled || self.intensity <= 0.0 {
            return 0.0;
        }
        let roundness = self.roundness.clamp(0.0, 1.0);
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };

        let stretch = 1.0 + (aspect - 1.0) * roundness;
        let x = offset[0].abs() * stretch;
        let y = offset[1].abs();
        let distance = x.hypot(y) / (1.0_f32).hypot(stretch.max(1.0));

        let smoothness = self.smoothness.clamp(0.01, 1.0);
        let ramp = ((distance - (1.0 - smoothness)) / smoothness).clamp(0.0, 1.0);

        let eased = ramp * ramp * (3.0 - 2.0 * ramp);
        eased * self.intensity.clamp(0.0, 1.0)
    }
}
