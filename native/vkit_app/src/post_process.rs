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
