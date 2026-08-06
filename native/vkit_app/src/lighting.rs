#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
#[repr(u32)]
#[serde(rename_all = "lowercase")]
pub enum LightingPreset {
    #[default]
    Studio = 0,
    Soft = 1,
    Daylight = 2,
    Warm = 3,
    Gloss = 4,

    Portrait = 5,
}

pub const MAX_PUNCTUAL_LIGHTS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PunctualLight {
    pub position: [f32; 3],

    pub direction: [f32; 3],

    pub radiance: [f32; 3],

    pub range: f32,

    pub cos_inner: f32,
    pub cos_outer: f32,
}

impl PunctualLight {
    pub const DARK: Self = Self {
        position: [0.0; 3],
        direction: [0.0, 0.0, 1.0],
        radiance: [0.0; 3],
        range: 1.0,
        cos_inner: 1.0,
        cos_outer: 1.0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightingProfile {
    pub key_radiance: [f32; 3],
    pub fill_radiance: [f32; 3],
    pub environment_top: [f32; 3],
    pub environment_bottom: [f32; 3],
    pub specular_strength: f32,
    pub grazing_strength: f32,

    pub punctual: [PunctualLight; MAX_PUNCTUAL_LIGHTS],
    pub punctual_count: u32,
}

impl LightingPreset {
    pub const ALL: [Self; 6] = [
        Self::Studio,
        Self::Soft,
        Self::Daylight,
        Self::Warm,
        Self::Gloss,
        Self::Portrait,
    ];

    pub const fn id(self) -> u32 {
        self as u32
    }

    pub const fn label_ko(self) -> &'static str {
        match self {
            Self::Studio => "스튜디오",
            Self::Soft => "부드럽게",
            Self::Daylight => "주광",
            Self::Warm => "따뜻하게",
            Self::Gloss => "광택 확인",
            Self::Portrait => "인물 조명",
        }
    }

    pub const fn label_en(self) -> &'static str {
        match self {
            Self::Studio => "Studio",
            Self::Soft => "Soft",
            Self::Daylight => "Daylight",
            Self::Warm => "Warm",
            Self::Gloss => "Gloss Check",
            Self::Portrait => "Portrait",
        }
    }

    pub const fn label_ja(self) -> &'static str {
        match self {
            Self::Studio => "スタジオ",
            Self::Soft => "ソフト",
            Self::Daylight => "昼光",
            Self::Warm => "ウォーム",
            Self::Gloss => "光沢チェック",
            Self::Portrait => "ポートレート",
        }
    }

    pub const fn swatch(self) -> ([u8; 3], [u8; 3]) {
        match self {
            Self::Studio => ([255, 218, 190], [128, 170, 218]),
            Self::Soft => ([236, 231, 224], [190, 205, 220]),
            Self::Daylight => ([222, 238, 255], [128, 174, 236]),
            Self::Warm => ([255, 191, 132], [190, 116, 92]),
            Self::Gloss => ([245, 247, 255], [86, 115, 165]),
            Self::Portrait => ([255, 226, 196], [96, 138, 176]),
        }
    }

    pub const fn profile(self) -> LightingProfile {
        match self {
            Self::Studio => LightingProfile {
                key_radiance: [2.35, 2.22, 2.10],
                fill_radiance: [0.60, 0.72, 0.84],
                environment_top: [0.22, 0.24, 0.28],
                environment_bottom: [0.095, 0.085, 0.075],
                specular_strength: 0.90,
                grazing_strength: 0.30,
                punctual: [PunctualLight::DARK; MAX_PUNCTUAL_LIGHTS],
                punctual_count: 0,
            },
            Self::Soft => LightingProfile {
                key_radiance: [1.55, 1.50, 1.44],
                fill_radiance: [0.95, 0.98, 1.02],
                environment_top: [0.35, 0.36, 0.38],
                environment_bottom: [0.24, 0.22, 0.20],
                specular_strength: 0.55,
                grazing_strength: 0.14,
                punctual: [PunctualLight::DARK; MAX_PUNCTUAL_LIGHTS],
                punctual_count: 0,
            },
            Self::Daylight => LightingProfile {
                key_radiance: [2.05, 2.25, 2.55],
                fill_radiance: [0.62, 0.80, 1.08],
                environment_top: [0.28, 0.36, 0.52],
                environment_bottom: [0.12, 0.11, 0.10],
                specular_strength: 0.90,
                grazing_strength: 0.26,
                punctual: [PunctualLight::DARK; MAX_PUNCTUAL_LIGHTS],
                punctual_count: 0,
            },
            Self::Warm => LightingProfile {
                key_radiance: [2.70, 2.10, 1.55],
                fill_radiance: [0.82, 0.58, 0.44],
                environment_top: [0.28, 0.20, 0.15],
                environment_bottom: [0.15, 0.085, 0.060],
                specular_strength: 0.76,
                grazing_strength: 0.22,
                punctual: [PunctualLight::DARK; MAX_PUNCTUAL_LIGHTS],
                punctual_count: 0,
            },
            Self::Gloss => LightingProfile {
                key_radiance: [3.10, 3.05, 3.00],
                fill_radiance: [0.34, 0.46, 0.70],
                environment_top: [0.13, 0.15, 0.20],
                environment_bottom: [0.045, 0.050, 0.065],
                specular_strength: 1.80,
                grazing_strength: 0.50,
                punctual: [PunctualLight::DARK; MAX_PUNCTUAL_LIGHTS],
                punctual_count: 0,
            },

            Self::Portrait => LightingProfile {
                key_radiance: [0.30, 0.29, 0.28],
                fill_radiance: [0.22, 0.25, 0.30],
                environment_top: [0.17, 0.185, 0.215],
                environment_bottom: [0.075, 0.070, 0.065],
                specular_strength: 0.95,
                grazing_strength: 0.36,
                punctual: [
                    PunctualLight {
                        position: [-1.55, 1.45, 2.35],
                        direction: [0.46, -0.42, -0.78],
                        radiance: [17.6, 16.1, 13.7],
                        range: 9.0,
                        cos_inner: 0.83,
                        cos_outer: 0.42,
                    },
                    PunctualLight {
                        position: [2.05, 0.85, 1.95],
                        direction: [-0.63, -0.24, -0.74],
                        radiance: [6.3, 6.6, 6.6],
                        range: 10.0,
                        cos_inner: 0.86,
                        cos_outer: 0.50,
                    },
                    PunctualLight {
                        position: [0.10, 0.35, 3.15],
                        direction: [-0.03, -0.10, -0.99],
                        radiance: [3.7, 4.1, 4.1],
                        range: 13.0,
                        cos_inner: 0.55,
                        cos_outer: 0.06,
                    },
                    PunctualLight {
                        position: [-1.75, 1.95, -2.15],
                        direction: [0.52, -0.55, 0.65],
                        radiance: [8.6, 7.4, 5.5],
                        range: 8.5,
                        cos_inner: 0.80,
                        cos_outer: 0.34,
                    },
                ],
                punctual_count: 4,
            },
        }
    }
}

pub const DEFAULT_LIGHT_BRIGHTNESS: f32 = 1.0;
pub const MIN_LIGHT_BRIGHTNESS: f32 = 0.35;
pub const MAX_LIGHT_BRIGHTNESS: f32 = 2.0;

pub fn sanitize_brightness(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(MIN_LIGHT_BRIGHTNESS, MAX_LIGHT_BRIGHTNESS)
    } else {
        DEFAULT_LIGHT_BRIGHTNESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_ids_are_stable_and_brightness_is_bounded() {
        for (index, preset) in LightingPreset::ALL.into_iter().enumerate() {
            assert_eq!(preset.id(), index as u32);
            assert!(!preset.label_ko().is_empty());
            assert!(!preset.label_en().is_empty());
            assert!(!preset.label_ja().is_empty());
            let profile = preset.profile();
            for value in profile
                .key_radiance
                .into_iter()
                .chain(profile.fill_radiance)
                .chain(profile.environment_top)
                .chain(profile.environment_bottom)
                .chain([profile.specular_strength, profile.grazing_strength])
            {
                assert!(value.is_finite() && value >= 0.0, "{preset:?}: {value}");
            }
        }
        assert!(
            LightingPreset::Gloss.profile().specular_strength
                > LightingPreset::Studio.profile().specular_strength
        );
        assert!(
            LightingPreset::Soft.profile().environment_top[0]
                > LightingPreset::Gloss.profile().environment_top[0]
        );
        assert_eq!(sanitize_brightness(f32::NAN), DEFAULT_LIGHT_BRIGHTNESS);
        assert_eq!(sanitize_brightness(0.0), MIN_LIGHT_BRIGHTNESS);
        assert_eq!(sanitize_brightness(8.0), MAX_LIGHT_BRIGHTNESS);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportGrading {
    pub light_yaw_radians: f32,
    pub preset: LightingPreset,
    pub brightness: f32,
    pub tone_mapping: crate::shader_color::ToneMapping,
}

#[cfg(test)]
mod grading_tests {
    use super::*;

    #[test]
    fn an_unusable_brightness_is_brought_back_into_range() {
        assert_eq!(sanitize_brightness(f32::NAN), sanitize_brightness(1.0));
        assert!(sanitize_brightness(1.0e9) <= 2.0);
        assert!(sanitize_brightness(-5.0) >= 0.35);
    }
}
