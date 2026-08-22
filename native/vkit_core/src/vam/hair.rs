use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::skin::{AssetLocator, SkinSex, list_var_entries};
use super::{Result, VaMError, catalog::VaMRoot, io_error};

const MAX_HAIR_PRESET_BYTES: usize = 16 * 1024 * 1024;
const MAX_HAIR_VAB_BYTES: usize = 128 * 1024 * 1024;

const SCALP_GROUP_NAMES: usize = 4;
const MAX_SCALP_VERTICES: usize = 1_000_000;
const MAX_SCALP_MATERIALS: usize = 1_024;
const MAX_SCALP_FACES: usize = 1_000_000;
const MAX_SCALP_FACE_CORNERS: usize = 64;
const MAX_SCALP_TEXTURE_BYTES: usize = 64 * 1024 * 1024;

const MAX_SCALP_PROBE_BYTES: usize = 128 * 1024 * 1024;
const MAX_WALK_FILES: usize = 2_000_000;
const MAX_ARCHIVES: usize = 100_000;
const MAX_STRAND_SLOTS: usize = 100_000;
const MAX_HAIR_POINTS: usize = 10_000_000;
const MAX_RENDER_INDICES: usize = 30_000_000;
const MAX_SEGMENTS: usize = 1_024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HairShaderType {
    Fast,
    #[default]
    Quality,
    QualityThicken,
    QualityThickenMore,
    NonStandard,
}

impl HairShaderType {
    pub const fn id(self) -> u32 {
        match self {
            Self::Fast => 0,
            Self::Quality => 1,
            Self::QualityThicken => 2,
            Self::QualityThickenMore => 3,
            Self::NonStandard => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HairScalpMaterialSettings {
    pub diffuse_color: [f32; 3],
    pub specular_color: [f32; 3],
    pub specular_intensity: f32,
    pub glossiness: f32,
    pub specular_fresnel: f32,
    pub alpha_adjust: f32,
    pub diffuse_offset: f32,
}

impl Default for HairScalpMaterialSettings {
    fn default() -> Self {
        Self {
            diffuse_color: [1.0; 3],
            specular_color: [0.15; 3],
            specular_intensity: 0.0,
            glossiness: 3.2,
            specular_fresnel: 0.5,
            alpha_adjust: 0.0,
            diffuse_offset: 0.0,
        }
    }
}

impl HairScalpMaterialSettings {
    pub fn roughness(self) -> f32 {
        (1.0 / (1.0 + self.glossiness.max(0.0) * 0.35)).clamp(0.08, 0.95)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HairOpticalSettings {
    pub shader_type: HairShaderType,
    pub color_rolloff: f32,
    pub specular_color: [f32; 3],
    pub diffuse_softness: f32,
    pub primary_specular_sharpness: f32,
    pub secondary_specular_sharpness: f32,
    pub specular_shift: f32,
    pub fresnel_power: f32,
    pub fresnel_attenuation: f32,
    pub random_color_power: f32,
    pub random_color_offset: f32,
    pub ibl_factor: f32,
    pub normal_randomize: f32,

    pub child_lengths: [f32; 3],
}

impl Default for HairOpticalSettings {
    fn default() -> Self {
        Self {
            shader_type: HairShaderType::Quality,
            color_rolloff: 1.0,
            specular_color: [0.15; 3],
            diffuse_softness: 0.75,
            primary_specular_sharpness: 50.0,
            secondary_specular_sharpness: 50.0,
            specular_shift: 0.01,
            fresnel_power: 10.0,
            fresnel_attenuation: 1.0,
            random_color_power: 1.0,
            random_color_offset: 0.3,
            ibl_factor: 0.5,
            normal_randomize: 0.0,
            child_lengths: [1.0; 3],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HairLookPatch {
    pub shader_type: Option<HairShaderType>,
    pub root_color: Option<[f32; 3]>,
    pub tip_color: Option<[f32; 3]>,
    pub specular_color: Option<[f32; 3]>,
    pub color_rolloff: Option<f32>,
    pub diffuse_softness: Option<f32>,
    pub primary_specular_sharpness: Option<f32>,
    pub secondary_specular_sharpness: Option<f32>,
    pub specular_shift: Option<f32>,
    pub fresnel_power: Option<f32>,
    pub fresnel_attenuation: Option<f32>,
    pub random_color_power: Option<f32>,
    pub random_color_offset: Option<f32>,
    pub ibl_factor: Option<f32>,
    pub normal_randomize: Option<f32>,
    pub width_m: Option<f32>,
    pub length: [Option<f32>; 3],
    pub curve_density: Option<u32>,
    pub hair_multiplier: Option<u32>,
    pub max_spread_m: Option<f32>,
    pub curl: [Option<f32>; 3],
    pub curl_scale: Option<f32>,
    pub curl_frequency: Option<f32>,
    pub curl_scale_randomness: Option<f32>,
    pub curl_frequency_randomness: Option<f32>,
    pub curl_allow_reverse: Option<bool>,
    pub curl_allow_flip_axis: Option<bool>,
    pub curl_root: Option<f32>,
    pub curl_mid: Option<f32>,
    pub curl_tip: Option<f32>,
    pub curl_midpoint: Option<f32>,
    pub curl_normal_adjust: Option<f32>,
    pub curl_curve_power: Option<f32>,

    pub spread_root: Option<f32>,
    pub spread_mid: Option<f32>,
    pub spread_tip: Option<f32>,
    pub spread_midpoint: Option<f32>,
    pub spread_curve_power: Option<f32>,

    pub collision_radius_m: Option<f32>,

    pub collision_radius_root_m: Option<f32>,

    pub scalp_diffuse: Option<String>,
    pub scalp_specular: Option<String>,
    pub scalp_gloss: Option<String>,
    pub scalp_normal: Option<String>,
    pub scalp_alpha: Option<String>,
    pub scalp_diffuse_color: Option<[f32; 3]>,
    pub scalp_specular_color: Option<[f32; 3]>,
    pub scalp_specular_intensity: Option<f32>,
    pub scalp_glossiness: Option<f32>,
    pub scalp_specular_fresnel: Option<f32>,
    pub scalp_alpha_adjust: Option<f32>,
    pub scalp_diffuse_offset: Option<f32>,
}

impl HairLookPatch {
    pub fn apply(&mut self, overlay: &Self) {
        macro_rules! replace {
            ($field:ident) => {
                if overlay.$field.is_some() {
                    self.$field = overlay.$field;
                }
            };
        }
        replace!(shader_type);
        replace!(root_color);
        replace!(tip_color);
        replace!(specular_color);
        replace!(color_rolloff);
        replace!(diffuse_softness);
        replace!(primary_specular_sharpness);
        replace!(secondary_specular_sharpness);
        replace!(specular_shift);
        replace!(fresnel_power);
        replace!(fresnel_attenuation);
        replace!(random_color_power);
        replace!(random_color_offset);
        replace!(ibl_factor);
        replace!(normal_randomize);
        replace!(width_m);
        replace!(curve_density);
        replace!(hair_multiplier);
        replace!(max_spread_m);
        replace!(curl_scale);
        replace!(curl_frequency);
        replace!(curl_scale_randomness);
        replace!(curl_frequency_randomness);
        replace!(curl_allow_reverse);
        replace!(curl_allow_flip_axis);
        replace!(curl_root);
        replace!(curl_mid);
        replace!(curl_tip);
        replace!(curl_midpoint);
        replace!(curl_curve_power);
        replace!(spread_root);
        replace!(spread_mid);
        replace!(spread_tip);
        replace!(spread_midpoint);
        replace!(spread_curve_power);
        replace!(collision_radius_m);
        replace!(collision_radius_root_m);
        replace!(scalp_diffuse_color);
        replace!(scalp_specular_color);
        replace!(scalp_specular_intensity);
        replace!(scalp_glossiness);
        replace!(scalp_specular_fresnel);
        replace!(scalp_alpha_adjust);
        replace!(scalp_diffuse_offset);
        if overlay.scalp_diffuse.is_some() {
            self.scalp_diffuse.clone_from(&overlay.scalp_diffuse);
        }
        if overlay.scalp_specular.is_some() {
            self.scalp_specular.clone_from(&overlay.scalp_specular);
        }
        if overlay.scalp_gloss.is_some() {
            self.scalp_gloss.clone_from(&overlay.scalp_gloss);
        }
        if overlay.scalp_normal.is_some() {
            self.scalp_normal.clone_from(&overlay.scalp_normal);
        }
        if overlay.scalp_alpha.is_some() {
            self.scalp_alpha.clone_from(&overlay.scalp_alpha);
        }
        for index in 0..3 {
            if overlay.length[index].is_some() {
                self.length[index] = overlay.length[index];
            }
            if overlay.curl[index].is_some() {
                self.curl[index] = overlay.curl[index];
            }
        }
    }

    pub fn optical_settings(&self) -> HairOpticalSettings {
        let defaults = HairOpticalSettings::default();
        HairOpticalSettings {
            shader_type: self.shader_type.unwrap_or(defaults.shader_type),
            color_rolloff: self
                .color_rolloff
                .unwrap_or(defaults.color_rolloff)
                .clamp(0.0, 5.0),
            specular_color: self.specular_color.unwrap_or(defaults.specular_color),
            diffuse_softness: self
                .diffuse_softness
                .unwrap_or(defaults.diffuse_softness)
                .clamp(0.0, 1.0),
            primary_specular_sharpness: self
                .primary_specular_sharpness
                .unwrap_or(defaults.primary_specular_sharpness)
                .clamp(1.0, 1024.0),
            secondary_specular_sharpness: self
                .secondary_specular_sharpness
                .unwrap_or(defaults.secondary_specular_sharpness)
                .clamp(1.0, 1024.0),
            specular_shift: self
                .specular_shift
                .unwrap_or(defaults.specular_shift)
                .clamp(-1.0, 1.0),
            fresnel_power: self
                .fresnel_power
                .unwrap_or(defaults.fresnel_power)
                .clamp(0.25, 32.0),
            fresnel_attenuation: self
                .fresnel_attenuation
                .unwrap_or(defaults.fresnel_attenuation)
                .clamp(0.0, 4.0),
            random_color_power: self
                .random_color_power
                .unwrap_or(defaults.random_color_power)
                .clamp(0.0, 10.0),
            random_color_offset: self
                .random_color_offset
                .unwrap_or(defaults.random_color_offset)
                .clamp(-1.0, 1.0),
            ibl_factor: self
                .ibl_factor
                .unwrap_or(defaults.ibl_factor)
                .clamp(0.0, 4.0),
            normal_randomize: self
                .normal_randomize
                .unwrap_or(defaults.normal_randomize)
                .clamp(0.0, 1.0),
            // length1/2/3 are registered 0-1, and zero is a value creators
            // ship: with L = 0 the game's trunc puts every domain point on
            // tessellation point 0, so that tier collapses and draws nothing.
            // A floor of 0.05 turned it into a visible stub instead.
            child_lengths: std::array::from_fn(|index| {
                self.length[index]
                    .unwrap_or(defaults.child_lengths[index])
                    .clamp(0.0, 1.0)
            }),
        }
    }

    pub fn scalp_material_settings(&self) -> HairScalpMaterialSettings {
        let defaults = HairScalpMaterialSettings::default();
        HairScalpMaterialSettings {
            diffuse_color: self.scalp_diffuse_color.unwrap_or(defaults.diffuse_color),
            specular_color: self.scalp_specular_color.unwrap_or(defaults.specular_color),
            specular_intensity: self
                .scalp_specular_intensity
                .unwrap_or(defaults.specular_intensity)
                .clamp(0.0, 4.0),
            glossiness: self
                .scalp_glossiness
                .unwrap_or(defaults.glossiness)
                .clamp(0.0, 32.0),
            specular_fresnel: self
                .scalp_specular_fresnel
                .unwrap_or(defaults.specular_fresnel)
                .clamp(0.0, 2.0),
            alpha_adjust: self
                .scalp_alpha_adjust
                .unwrap_or(defaults.alpha_adjust)
                .clamp(-1.0, 1.0),
            diffuse_offset: self
                .scalp_diffuse_offset
                .unwrap_or(defaults.diffuse_offset)
                .clamp(-1.0, 3.0),
        }
    }

    pub fn spread_settings(&self) -> HairSpreadSettings {
        let defaults = HairSpreadSettings::default();
        HairSpreadSettings {
            max_spread_m: self
                .max_spread_m
                .unwrap_or(defaults.max_spread_m)
                .clamp(0.0, 0.5),
            root: self.spread_root.unwrap_or(defaults.root).clamp(0.0, 1.0),
            mid: self.spread_mid.unwrap_or(defaults.mid).clamp(0.0, 1.0),
            tip: self.spread_tip.unwrap_or(defaults.tip).clamp(0.0, 1.0),
            midpoint: self
                .spread_midpoint
                .unwrap_or(defaults.midpoint)
                .clamp(0.001, 1.0),
            curve_power: self
                .spread_curve_power
                .unwrap_or(defaults.curve_power)
                .clamp(0.0, 16.0),
        }
    }

    pub fn waviness_settings(&self) -> HairWavinessSettings {
        let defaults = HairWavinessSettings::default();
        HairWavinessSettings {
            vector_m: [
                self.curl[0].unwrap_or(defaults.vector_m[0]).clamp(0.0, 1.0),
                self.curl[1].unwrap_or(defaults.vector_m[1]).clamp(0.0, 1.0),
                self.curl[2].unwrap_or(defaults.vector_m[2]).clamp(0.0, 1.0),
            ],
            scale: self.curl_scale.unwrap_or(defaults.scale).clamp(0.0, 1.0),
            frequency: self
                .curl_frequency
                .unwrap_or(defaults.frequency)
                .clamp(0.0, 20.0),
            scale_randomness: self
                .curl_scale_randomness
                .unwrap_or(defaults.scale_randomness)
                .clamp(0.0, 2.0),
            frequency_randomness: self
                .curl_frequency_randomness
                .unwrap_or(defaults.frequency_randomness)
                .clamp(0.0, 2.0),
            allow_reverse: self.curl_allow_reverse.unwrap_or(defaults.allow_reverse),
            allow_flip_axis: self
                .curl_allow_flip_axis
                .unwrap_or(defaults.allow_flip_axis),
            root: self.curl_root.unwrap_or(defaults.root).clamp(0.0, 1.0),
            mid: self.curl_mid.unwrap_or(defaults.mid).clamp(0.0, 1.0),
            tip: self.curl_tip.unwrap_or(defaults.tip).clamp(0.0, 1.0),
            midpoint: self
                .curl_midpoint
                .unwrap_or(defaults.midpoint)
                .clamp(0.001, 1.0),
            normal_adjust: self
                .curl_normal_adjust
                .unwrap_or(defaults.normal_adjust)
                .clamp(-0.5, 0.5),
            curve_power: self
                .curl_curve_power
                .unwrap_or(defaults.curve_power)
                .clamp(0.0, 16.0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HairSpreadSettings {
    pub max_spread_m: f32,
    pub root: f32,
    pub mid: f32,
    pub tip: f32,
    pub midpoint: f32,
    pub curve_power: f32,
}

impl Default for HairSpreadSettings {
    fn default() -> Self {
        Self {
            max_spread_m: 0.0,
            root: 1.0,
            mid: 1.0,
            tip: 1.0,
            midpoint: 0.5,
            curve_power: 1.0,
        }
    }
}

impl HairSpreadSettings {
    pub fn deviation(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        let followed = if t <= self.midpoint {
            let eased = (1.0 - t / self.midpoint).powf(self.curve_power);
            self.mid + (self.root - self.mid) * eased
        } else {
            let eased =
                ((t - self.midpoint) / (1.0 - self.midpoint).max(0.001)).powf(self.curve_power);
            self.mid + (self.tip - self.mid) * eased
        };
        (1.0 - followed).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HairWavinessSettings {
    pub vector_m: [f32; 3],
    pub normal_adjust: f32,
    pub scale: f32,
    pub frequency: f32,
    pub scale_randomness: f32,
    pub frequency_randomness: f32,
    pub allow_reverse: bool,
    pub allow_flip_axis: bool,
    pub root: f32,
    pub mid: f32,
    pub tip: f32,
    pub midpoint: f32,
    pub curve_power: f32,
}

impl Default for HairWavinessSettings {
    fn default() -> Self {
        Self {
            normal_adjust: 0.0,
            vector_m: [0.1, 0.0, 0.1],
            scale: 0.0,
            frequency: 0.0,
            scale_randomness: 1.0,
            frequency_randomness: 1.0,
            allow_reverse: false,
            allow_flip_axis: false,
            root: 1.0,
            mid: 1.0,
            tip: 1.0,
            midpoint: 0.5,
            curve_power: 1.0,
        }
    }
}

impl HairWavinessSettings {
    pub fn envelope(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if t <= self.midpoint {
            let eased = (1.0 - t / self.midpoint).powf(self.curve_power);
            self.mid + (self.root - self.mid) * eased
        } else {
            let eased =
                ((t - self.midpoint) / (1.0 - self.midpoint).max(0.001)).powf(self.curve_power);
            self.mid + (self.tip - self.mid) * eased
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HairScalpTextureBytes {
    pub diffuse: Option<Vec<u8>>,
    pub specular: Option<Vec<u8>>,
    pub gloss: Option<Vec<u8>>,
    pub normal: Option<Vec<u8>>,
    pub alpha: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HairPhysicsPatch {
    pub simulation_enabled: Option<bool>,
    pub collision_enabled: Option<bool>,
    pub use_painted_rigidity: Option<bool>,
    pub collision_radius_m: Option<f32>,
    pub collision_radius_root_m: Option<f32>,
    pub drag: Option<f32>,
    pub friction: Option<f32>,
    pub gravity_multiplier: Option<f32>,
    pub weight: Option<f32>,
    pub iterations: Option<u32>,
    pub root_rigidity: Option<f32>,
    pub main_rigidity: Option<f32>,
    pub tip_rigidity: Option<f32>,
    pub rigidity_rolloff_power: Option<f32>,
    pub cling: Option<f32>,
    pub cling_rolloff: Option<f32>,
    pub snap: Option<f32>,
    pub bend_resistance: Option<f32>,
    pub wind: Option<[f32; 3]>,
}

impl HairPhysicsPatch {
    pub fn apply(&mut self, overlay: &Self) {
        macro_rules! replace {
            ($field:ident) => {
                if overlay.$field.is_some() {
                    self.$field = overlay.$field;
                }
            };
        }
        replace!(simulation_enabled);
        replace!(collision_enabled);
        replace!(use_painted_rigidity);
        replace!(collision_radius_m);
        replace!(collision_radius_root_m);
        replace!(drag);
        replace!(friction);
        replace!(gravity_multiplier);
        replace!(weight);
        replace!(iterations);
        replace!(root_rigidity);
        replace!(main_rigidity);
        replace!(tip_rigidity);
        replace!(rigidity_rolloff_power);
        replace!(cling);
        replace!(cling_rolloff);
        replace!(snap);
        replace!(bend_resistance);
        replace!(wind);
    }

    pub fn resolve(&self) -> HairPhysicsSettings {
        let defaults = HairPhysicsSettings::default();
        HairPhysicsSettings {
            simulation_enabled: self
                .simulation_enabled
                .unwrap_or(defaults.simulation_enabled),
            collision_enabled: self.collision_enabled.unwrap_or(defaults.collision_enabled),
            use_painted_rigidity: self
                .use_painted_rigidity
                .unwrap_or(defaults.use_painted_rigidity),
            collision_radius_m: self
                .collision_radius_m
                .unwrap_or(defaults.collision_radius_m),
            collision_radius_root_m: self
                .collision_radius_root_m
                .unwrap_or(defaults.collision_radius_root_m),
            drag: self.drag.unwrap_or(defaults.drag),
            friction: self.friction.unwrap_or(defaults.friction),
            gravity_multiplier: self
                .gravity_multiplier
                .unwrap_or(defaults.gravity_multiplier),
            weight: self.weight.unwrap_or(defaults.weight),
            iterations: self.iterations.unwrap_or(defaults.iterations),
            root_rigidity: self.root_rigidity.unwrap_or(defaults.root_rigidity),
            main_rigidity: self.main_rigidity.unwrap_or(defaults.main_rigidity),
            tip_rigidity: self.tip_rigidity.unwrap_or(defaults.tip_rigidity),
            rigidity_rolloff_power: self
                .rigidity_rolloff_power
                .unwrap_or(defaults.rigidity_rolloff_power),
            cling: self.cling.unwrap_or(defaults.cling),
            cling_rolloff: self.cling_rolloff.unwrap_or(defaults.cling_rolloff),
            snap: self.snap.unwrap_or(defaults.snap),
            bend_resistance: self.bend_resistance.unwrap_or(defaults.bend_resistance),
            wind: self.wind.unwrap_or(defaults.wind),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HairPhysicsSettings {
    pub simulation_enabled: bool,
    pub collision_enabled: bool,
    pub use_painted_rigidity: bool,
    pub collision_radius_m: f32,
    pub collision_radius_root_m: f32,
    pub drag: f32,
    pub friction: f32,
    pub gravity_multiplier: f32,
    pub weight: f32,
    pub iterations: u32,
    pub root_rigidity: f32,
    pub main_rigidity: f32,
    pub tip_rigidity: f32,
    pub rigidity_rolloff_power: f32,
    pub cling: f32,
    pub cling_rolloff: f32,
    pub snap: f32,
    pub bend_resistance: f32,
    pub wind: [f32; 3],
}

impl Default for HairPhysicsSettings {
    fn default() -> Self {
        Self {
            simulation_enabled: true,
            collision_enabled: true,
            use_painted_rigidity: false,
            collision_radius_m: 0.01,
            collision_radius_root_m: 0.01,
            drag: 0.0,
            friction: 0.8,
            gravity_multiplier: 1.0,
            weight: 1.5,
            iterations: 1,
            root_rigidity: 0.1,
            main_rigidity: 0.02,
            tip_rigidity: 0.0,
            rigidity_rolloff_power: 8.0,
            cling: 1.0,
            cling_rolloff: 0.0,
            snap: 0.2,
            bend_resistance: 0.2,
            wind: [0.0; 3],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HairPartReference {
    pub reference: String,
    pub internal_id: String,
    pub geometry: AssetLocator,
    pub appearance: Option<AssetLocator>,
    pub preset_look: HairLookPatch,
    #[serde(default)]
    pub preset_physics: HairPhysicsPatch,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HairPreset {
    pub stable_id: String,
    pub label: String,
    pub source: AssetLocator,
    pub thumbnail: Option<AssetLocator>,
    pub sex: SkinSex,
    pub parts: Vec<HairPartReference>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HairPresetScanDiagnostics {
    pub skipped_archives: usize,
    pub skipped_presets: usize,
    pub warnings: Vec<String>,
}

impl HairPresetScanDiagnostics {
    fn warning(&mut self, message: String) {
        if self.warnings.len() < 64 {
            self.warnings.push(message);
            self.warnings.sort();
            self.warnings.dedup();
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HairPresetScanReport {
    pub presets: Vec<HairPreset>,

    pub shared_scalp: Option<HairPartReference>,
    pub diagnostics: HairPresetScanDiagnostics,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HairGuide {
    pub scalp_index: u32,
    pub points_cm: Vec<[f32; 3]>,

    pub rigidity: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HairScalpGeometry {
    pub materials: Vec<String>,

    pub vertices_cm: Vec<[f32; 3]>,

    pub uvs: Vec<[f32; 2]>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BuiltinHairScalp {
    pub provider_name: String,
    pub geometry: HairScalpGeometry,
}

pub fn load_builtin_hair_scalps(root: &VaMRoot) -> Result<Vec<BuiltinHairScalp>> {
    let bundle_path = root
        .path()
        .join("VaM_Data")
        .join("StreamingAssets")
        .join("a_per");
    let bytes = fs::read(&bundle_path).map_err(|error| io_error(&bundle_path, error))?;
    super::unity_base::extract_daz_scalp_provider_meshes(&bytes)?
        .into_iter()
        .map(|provider| {
            let mut triangles = Vec::new();
            for polygon in provider.polygons {
                if polygon.len() < 3 {
                    continue;
                }
                for corner in 1..polygon.len() - 1 {
                    triangles.push([polygon[0], polygon[corner], polygon[corner + 1]]);
                }
            }
            if triangles.is_empty() {
                return Err(invalid_geometry(
                    &provider.object_name,
                    "built-in scalp has no renderable polygons",
                ));
            }
            Ok(BuiltinHairScalp {
                provider_name: provider.object_name,
                geometry: HairScalpGeometry {
                    materials: provider.material_names,
                    vertices_cm: provider
                        .vertices_m
                        .into_iter()
                        .map(|point| [point[0] * 100.0, point[1] * 100.0, point[2] * 100.0])
                        .collect(),
                    uvs: provider.uvs,
                    triangles,
                },
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct HairGuideGeometry {
    pub provider_name: String,
    pub segments: usize,
    pub segment_length_cm: f32,
    pub scalp_vertex_count: usize,
    pub guides: Vec<HairGuide>,
    pub guide_triangles: Vec<[u32; 3]>,
    pub root_map: Vec<u32>,

    pub nearby_joints: Vec<HairNearbyJoint>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HairNearbyJoint {
    pub a: [u32; 2],

    pub b: [u32; 2],
    pub elasticity: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HairPartAsset {
    pub geometry: HairGuideGeometry,
    pub look: HairLookPatch,
    pub physics: HairPhysicsSettings,
}

#[derive(Clone, Copy, Debug)]
enum PresetContainer {
    Local,
    Archive(usize),
    Expanded(usize),
}

#[derive(Clone, Debug)]
struct HairArchive {
    path: PathBuf,
    uid: String,
    entries: Vec<String>,
    lookup: BTreeMap<String, usize>,
}

#[derive(Clone, Debug)]
struct ExpandedPackage {
    path: PathBuf,
    uid: String,
}

#[derive(Clone, Debug, Default)]
struct HairPackageIndex {
    archives: Vec<HairArchive>,
    archive_by_uid: BTreeMap<String, Vec<usize>>,
    expanded: Vec<ExpandedPackage>,
    expanded_by_uid: BTreeMap<String, Vec<usize>>,
}

impl HairPackageIndex {
    fn scan(root: &VaMRoot, diagnostics: &mut HairPresetScanDiagnostics) -> Result<Self> {
        let addon_root = root.addon_packages_path();
        let mut archive_paths = walk_files_if_exists(&addon_root)?
            .into_iter()
            .filter(|path| has_extension(path, "var"))
            .collect::<Vec<_>>();
        archive_paths.sort_by_key(|path| path_sort_key(path));
        archive_paths.dedup();
        if archive_paths.len() > MAX_ARCHIVES {
            return Err(VaMError::InvalidHairPreset {
                locator: addon_root.display().to_string(),
                message: format!("archive count exceeds {MAX_ARCHIVES}"),
            });
        }

        let mut archives = Vec::with_capacity(archive_paths.len());
        for path in archive_paths {
            match list_var_entries(&path) {
                Ok(entries) => {
                    let lookup = entries
                        .iter()
                        .enumerate()
                        .map(|(index, entry)| (normalized_archive_path(entry), index))
                        .collect();
                    archives.push(HairArchive {
                        uid: path
                            .file_stem()
                            .and_then(|value| value.to_str())
                            .unwrap_or_default()
                            .to_owned(),
                        path,
                        entries,
                        lookup,
                    });
                }
                Err(error) => {
                    diagnostics.skipped_archives += 1;
                    diagnostics.warning(format!("archive {}: {error}", path.display()));
                }
            }
        }

        let mut expanded = Vec::new();
        if addon_root.is_dir() {
            let mut entries = fs::read_dir(&addon_root)
                .map_err(|error| io_error(&addon_root, error))?
                .collect::<std::io::Result<Vec<_>>>()
                .map_err(|error| io_error(&addon_root, error))?;
            entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
            for entry in entries {
                if entry
                    .file_type()
                    .map_err(|error| io_error(entry.path(), error))?
                    .is_dir()
                {
                    expanded.push(ExpandedPackage {
                        uid: entry.file_name().to_string_lossy().into_owned(),
                        path: entry.path(),
                    });
                }
            }
        }

        let mut archive_by_uid = BTreeMap::<String, Vec<usize>>::new();
        for (index, archive) in archives.iter().enumerate() {
            archive_by_uid
                .entry(archive.uid.to_ascii_lowercase())
                .or_default()
                .push(index);
        }
        let mut expanded_by_uid = BTreeMap::<String, Vec<usize>>::new();
        for (index, package) in expanded.iter().enumerate() {
            expanded_by_uid
                .entry(package.uid.to_ascii_lowercase())
                .or_default()
                .push(index);
        }
        Ok(Self {
            archives,
            archive_by_uid,
            expanded,
            expanded_by_uid,
        })
    }

    fn expanded_owner(&self, path: &Path) -> Option<usize> {
        self.expanded
            .iter()
            .position(|package| path.starts_with(&package.path))
    }

    fn resolve(&self, raw: &str, owner: PresetContainer, root: &VaMRoot) -> Option<AssetLocator> {
        let reference = raw.trim().replace('\\', "/");
        if reference
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("self:/"))
        {
            return self.resolve_owned(&reference[6..], owner, root);
        }
        if let Some((uid, entry)) = reference.split_once(":/") {
            return self.resolve_package(uid, entry);
        }
        self.resolve_owned(reference.trim_start_matches('/'), owner, root)
    }

    fn resolve_owned(
        &self,
        entry: &str,
        owner: PresetContainer,
        root: &VaMRoot,
    ) -> Option<AssetLocator> {
        match owner {
            PresetContainer::Archive(index) => self.archives.get(index)?.locator(entry),
            PresetContainer::Expanded(index) => {
                let path = self
                    .expanded
                    .get(index)?
                    .path
                    .join(entry.replace('/', "\\"));
                path.is_file().then_some(AssetLocator::File(path))
            }
            PresetContainer::Local => {
                let path = root.path().join(entry.replace('/', "\\"));
                path.is_file().then_some(AssetLocator::File(path))
            }
        }
    }

    fn resolve_package(&self, uid: &str, entry: &str) -> Option<AssetLocator> {
        let key = uid.to_ascii_lowercase();
        if let Some(index) = select_package_index(&key, &self.expanded_by_uid, |index| {
            &self.expanded[index].uid
        }) {
            let path = self.expanded[index].path.join(entry.replace('/', "\\"));
            if path.is_file() {
                return Some(AssetLocator::File(path));
            }
        }
        select_package_index(&key, &self.archive_by_uid, |index| {
            &self.archives[index].uid
        })
        .and_then(|index| self.archives[index].locator(entry))
    }

    fn sibling(&self, locator: &AssetLocator, extension: &str) -> Option<AssetLocator> {
        match locator {
            AssetLocator::File(path) => {
                let sibling = path.with_extension(extension);
                sibling.is_file().then_some(AssetLocator::File(sibling))
            }
            AssetLocator::VarEntry { archive, entry } => {
                let sibling = replace_extension(entry, extension);
                self.archives
                    .iter()
                    .find(|candidate| candidate.path == *archive)?
                    .locator(&sibling)
            }
            AssetLocator::UnresolvedVaMUrl(_) | AssetLocator::BuiltinTexture(_) => None,
        }
    }
}

impl HairArchive {
    fn locator(&self, entry: &str) -> Option<AssetLocator> {
        let normalized = normalized_archive_path(entry);
        let index = *self.lookup.get(&normalized)?;
        Some(AssetLocator::VarEntry {
            archive: self.path.clone(),
            entry: self.entries[index].clone(),
        })
    }
}

pub fn scan_hair_presets(root: &VaMRoot) -> Result<Vec<HairPreset>> {
    Ok(scan_hair_presets_with_report(root)?.presets)
}

pub fn scan_hair_presets_with_report(root: &VaMRoot) -> Result<HairPresetScanReport> {
    let mut diagnostics = HairPresetScanDiagnostics::default();
    let packages = HairPackageIndex::scan(root, &mut diagnostics)?;
    let mut presets = BTreeMap::new();

    let local_root = root.person_assets_path().join("Hair");
    for path in walk_files_if_exists(&local_root)? {
        if !has_extension(&path, "vap") {
            continue;
        }
        let source = AssetLocator::File(path.clone());
        let attempt = fs::read(&path)
            .map_err(|error| io_error(&path, error))
            .and_then(|bytes| {
                if bytes.len() > MAX_HAIR_PRESET_BYTES {
                    return Err(invalid_preset(&source, "preset exceeds the byte limit"));
                }
                parse_hair_preset(&bytes, source, PresetContainer::Local, root, &packages)
            });
        record_preset_attempt(
            attempt,
            &path.display().to_string(),
            &mut presets,
            &mut diagnostics,
        );
    }

    for package in &packages.expanded {
        for path in walk_files_if_exists(&package.path)? {
            if !has_extension(&path, "vap") || !is_person_hair_path(&path_to_forward(&path)) {
                continue;
            }
            let source = AssetLocator::File(path.clone());
            let owner = packages
                .expanded_owner(&path)
                .map(PresetContainer::Expanded)
                .unwrap_or(PresetContainer::Local);
            let attempt = fs::read(&path)
                .map_err(|error| io_error(&path, error))
                .and_then(|bytes| {
                    if bytes.len() > MAX_HAIR_PRESET_BYTES {
                        return Err(invalid_preset(&source, "preset exceeds the byte limit"));
                    }
                    parse_hair_preset(&bytes, source, owner, root, &packages)
                });
            record_preset_attempt(
                attempt,
                &path.display().to_string(),
                &mut presets,
                &mut diagnostics,
            );
        }
    }

    for (archive_index, archive) in packages.archives.iter().enumerate() {
        for entry in &archive.entries {
            if !entry.to_ascii_lowercase().ends_with(".vap") || !is_person_hair_path(entry) {
                continue;
            }
            let source = AssetLocator::VarEntry {
                archive: archive.path.clone(),
                entry: entry.clone(),
            };
            let attempt = source.read_bytes(MAX_HAIR_PRESET_BYTES).and_then(|bytes| {
                parse_hair_preset(
                    &bytes,
                    source,
                    PresetContainer::Archive(archive_index),
                    root,
                    &packages,
                )
            });
            record_preset_attempt(
                attempt,
                &format!("{}::{entry}", archive.path.display()),
                &mut presets,
                &mut diagnostics,
            );
        }
    }

    let presets: Vec<HairPreset> = presets.into_values().collect();
    let shared_scalp = find_shared_scalp(&presets);
    Ok(HairPresetScanReport {
        presets,
        shared_scalp,
        diagnostics,
    })
}

fn record_preset_attempt(
    attempt: Result<Option<HairPreset>>,
    locator: &str,
    presets: &mut BTreeMap<String, HairPreset>,
    diagnostics: &mut HairPresetScanDiagnostics,
) {
    match attempt {
        Ok(Some(preset)) => {
            presets.insert(preset.stable_id.clone(), preset);
        }
        Ok(None) => {}
        Err(error) => {
            diagnostics.skipped_presets += 1;
            diagnostics.warning(format!("preset {locator}: {error}"));
        }
    }
}

fn parse_hair_preset(
    bytes: &[u8],
    source: AssetLocator,
    owner: PresetContainer,
    root: &VaMRoot,
    packages: &HairPackageIndex,
) -> Result<Option<HairPreset>> {
    let document: Value = crate::vam::simple_json::parse_document(bytes).map_err(|error| {
        invalid_preset(
            &source,
            format!("preset JSON could not be decoded: {error}"),
        )
    })?;
    let storables = document
        .get("storables")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid_preset(&source, "preset has no storables array"))?;
    let Some(hair_items) = storables.iter().find_map(|storable| {
        (storable.get("id").and_then(Value::as_str) == Some("geometry"))
            .then(|| storable.get("hair").and_then(Value::as_array))
            .flatten()
    }) else {
        return Ok(None);
    };

    let mut parts = Vec::new();
    let mut sex = SkinSex::Unknown;
    for item in hair_items {
        let Some(item) = item.as_object() else {
            continue;
        };
        if !json_bool(item.get("enabled"), true) {
            continue;
        }
        let Some(reference) = item.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !reference.to_ascii_lowercase().ends_with(".vam") {
            continue;
        }
        let Some(vam) = packages.resolve(reference, owner, root) else {
            return Err(invalid_preset(
                &source,
                format!("hair part reference could not be resolved: {reference}"),
            ));
        };
        let Some(geometry) = packages.sibling(&vam, "vab") else {
            return Err(invalid_preset(
                &source,
                format!("hair part has no matching VAB geometry: {reference}"),
            ));
        };
        let internal_id = item
            .get("internalId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        let sim_storable = find_sim_storable(storables, &internal_id);
        let mut preset_look = sim_storable.map(parse_hair_look).unwrap_or_default();
        preset_look.apply(&parse_scalp_material(storables, &internal_id));
        let preset_physics = sim_storable.map(parse_hair_physics).unwrap_or_default();
        sex = merge_sex(sex, infer_hair_sex(reference));
        parts.push(HairPartReference {
            reference: reference.to_owned(),
            internal_id,
            geometry,
            appearance: packages.sibling(&vam, "vaj"),
            preset_look,
            preset_physics,
        });
    }
    if parts.is_empty() {
        return Ok(None);
    }

    let label = source_label(&source);
    let thumbnail = packages
        .sibling(&source, "jpg")
        .or_else(|| packages.sibling(&source, "png"));
    Ok(Some(HairPreset {
        stable_id: source.display_key().to_ascii_lowercase(),
        label,
        source,
        thumbnail,
        sex,
        parts,
    }))
}

fn find_sim_storable<'a>(storables: &'a [Value], internal_id: &str) -> Option<&'a Value> {
    let exact = (!internal_id.is_empty()).then(|| format!("{internal_id}Sim"));
    storables.iter().find(|storable| {
        let Some(id) = storable.get("id").and_then(Value::as_str) else {
            return false;
        };
        exact
            .as_deref()
            .is_some_and(|expected| id.eq_ignore_ascii_case(expected))
    })
}

pub fn hair_look_from_storable(value: &Value) -> HairLookPatch {
    parse_hair_look(value)
}

pub fn hair_physics_from_storable(value: &Value) -> HairPhysicsPatch {
    parse_hair_physics(value)
}

fn parse_hair_look(value: &Value) -> HairLookPatch {
    HairLookPatch {
        shader_type: value
            .get("shaderType")
            .and_then(Value::as_str)
            .and_then(parse_hair_shader_type),
        root_color: value.get("rootColor").and_then(parse_hsv),
        tip_color: value.get("tipColor").and_then(parse_hsv),
        specular_color: value.get("specularColor").and_then(parse_hsv),
        color_rolloff: json_f32(value.get("colorRolloff")),
        diffuse_softness: json_f32(value.get("diffuseSoftness")),
        primary_specular_sharpness: json_f32(value.get("primarySpecularSharpness")),
        secondary_specular_sharpness: json_f32(value.get("secondarySpecularSharpness")),
        specular_shift: json_f32(value.get("specularShift")),
        fresnel_power: json_f32(value.get("fresnelPower")),
        fresnel_attenuation: json_f32(value.get("fresnelAttenuation")),
        random_color_power: json_f32(value.get("randomColorPower")),
        random_color_offset: json_f32(value.get("randomColorOffset")),
        ibl_factor: json_f32(value.get("IBLFactor")),
        normal_randomize: json_f32(value.get("normalRandomize")),
        width_m: json_f32(value.get("width")),
        length: [
            json_f32(value.get("length1")),
            json_f32(value.get("length2")),
            json_f32(value.get("length3")),
        ],
        curve_density: json_u32(value.get("curveDensity")),
        hair_multiplier: json_u32(value.get("hairMultiplier")),
        max_spread_m: json_f32(value.get("maxSpread")),
        curl: [
            json_f32(value.get("curlX")),
            json_f32(value.get("curlY")),
            json_f32(value.get("curlZ")),
        ],
        curl_normal_adjust: json_f32(value.get("curlNormalAdjust")),
        curl_scale: json_f32(value.get("curlScale")),
        curl_frequency: json_f32(value.get("curlFrequency")),
        curl_scale_randomness: json_f32(value.get("curlScaleRandomness")),
        curl_frequency_randomness: json_f32(value.get("curlFrequencyRandomness")),
        curl_allow_reverse: json_optional_bool(value.get("curlAllowReverse")),
        curl_allow_flip_axis: json_optional_bool(value.get("curlAllowFlipAxis")),
        curl_root: json_f32(value.get("curlRoot")),
        curl_mid: json_f32(value.get("curlMid")),
        curl_tip: json_f32(value.get("curlTip")),
        curl_midpoint: json_f32(value.get("curlMidpoint")),
        curl_curve_power: json_f32(value.get("curlCurvePower")),
        spread_root: json_f32(value.get("spreadRoot")),
        spread_mid: json_f32(value.get("spreadMid")),
        spread_tip: json_f32(value.get("spreadTip")),
        spread_midpoint: json_f32(value.get("spreadMidpoint")),
        spread_curve_power: json_f32(value.get("spreadCurvePower")),
        collision_radius_m: json_f32(value.get("collisionRadius")),
        collision_radius_root_m: json_f32(value.get("collisionRadiusRoot")),
        scalp_diffuse: None,
        scalp_specular: None,
        scalp_gloss: None,
        scalp_normal: None,
        scalp_alpha: None,
        scalp_diffuse_color: None,
        scalp_specular_color: None,
        scalp_specular_intensity: None,
        scalp_glossiness: None,
        scalp_specular_fresnel: None,
        scalp_alpha_adjust: None,
        scalp_diffuse_offset: None,
    }
}

fn parse_hair_shader_type(value: &str) -> Option<HairShaderType> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fast" => Some(HairShaderType::Fast),
        "quality" => Some(HairShaderType::Quality),
        "qualitythicken" => Some(HairShaderType::QualityThicken),
        "qualitythickenmore" => Some(HairShaderType::QualityThickenMore),
        "nonstandard" => Some(HairShaderType::NonStandard),
        _ => None,
    }
}

fn parse_hair_physics(value: &Value) -> HairPhysicsPatch {
    HairPhysicsPatch {
        simulation_enabled: json_optional_bool(value.get("simulationEnabled")),
        collision_enabled: json_optional_bool(value.get("collisionEnabled")),
        use_painted_rigidity: json_optional_bool(value.get("usePaintedRigidity")),
        collision_radius_m: json_f32(value.get("collisionRadius")),
        collision_radius_root_m: json_f32(value.get("collisionRadiusRoot")),
        drag: json_f32(value.get("drag")),
        friction: json_f32(value.get("friction")),
        gravity_multiplier: json_f32(value.get("gravityMultiplier")),
        weight: json_f32(value.get("weight")),
        iterations: json_u32(value.get("iterations")),
        root_rigidity: json_f32(value.get("rootRigidity")),
        main_rigidity: json_f32(value.get("mainRigidity")),
        tip_rigidity: json_f32(value.get("tipRigidity")),
        rigidity_rolloff_power: json_f32(value.get("rigidityRolloffPower")),
        cling: json_f32(value.get("cling")),
        cling_rolloff: json_f32(value.get("clingRolloff")),
        snap: json_f32(value.get("snap")),
        bend_resistance: json_f32(value.get("bendResistance")),
        wind: json_vec3(value.get("wind")),
    }
}

pub fn hair_scalp_material_from_storables(storables: &[Value], internal_id: &str) -> HairLookPatch {
    parse_scalp_material(storables, internal_id)
}

fn parse_scalp_material(storables: &[Value], internal_id: &str) -> HairLookPatch {
    let material = hair_scalp_material_storable(storables, internal_id);
    let texture = |material: &Value, key: &str| {
        material
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let Some(material) = material else {
        return HairLookPatch::default();
    };
    HairLookPatch {
        scalp_diffuse: texture(material, "customTexture_MainTex"),
        scalp_specular: texture(material, "customTexture_SpecTex"),
        scalp_gloss: texture(material, "customTexture_GlossTex"),
        scalp_normal: texture(material, "customTexture_BumpMap"),
        scalp_alpha: texture(material, "customTexture_AlphaTex"),
        scalp_diffuse_color: material.get("Diffuse Color").and_then(parse_hsv),
        scalp_specular_color: material.get("Specular Color").and_then(parse_hsv),
        scalp_specular_intensity: json_f32(material.get("Specular Intensity")),
        scalp_glossiness: json_f32(material.get("Gloss")),
        scalp_specular_fresnel: json_f32(material.get("Specular Fresnel")),
        scalp_alpha_adjust: json_f32(material.get("Alpha Adjust")),
        scalp_diffuse_offset: json_f32(material.get("Diffuse Texture Offset")),
        ..Default::default()
    }
}

fn parse_hsv(value: &Value) -> Option<[f32; 3]> {
    let h = json_f32(value.get("h"))?.rem_euclid(1.0);
    let s = json_f32(value.get("s"))?.clamp(0.0, 1.0);
    let v = json_f32(value.get("v"))?.clamp(0.0, 1.0);
    let sector = h * 6.0;
    let index = sector.floor() as u32;
    let fraction = sector - index as f32;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * fraction);
    let t = v * (1.0 - s * (1.0 - fraction));
    Some(match index % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    })
}

fn json_f32(value: Option<&Value>) -> Option<f32> {
    let value = value?;
    let parsed = value
        .as_f64()
        .map(|number| number as f32)
        .or_else(|| value.as_str()?.parse::<f32>().ok())?;
    parsed.is_finite().then_some(parsed)
}

fn json_u32(value: Option<&Value>) -> Option<u32> {
    let parsed = json_f32(value)?;
    (parsed >= 0.0 && parsed <= u32::MAX as f32).then_some(parsed.round() as u32)
}

fn json_optional_bool(value: Option<&Value>) -> Option<bool> {
    let value = value?;
    value.as_bool().or_else(
        || match value.as_str()?.trim().to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
    )
}

fn json_vec3(value: Option<&Value>) -> Option<[f32; 3]> {
    let values = value?.as_array()?;
    if values.len() < 3 {
        return None;
    }
    Some([
        json_f32(values.first())?,
        json_f32(values.get(1))?,
        json_f32(values.get(2))?,
    ])
}

fn json_bool(value: Option<&Value>, default: bool) -> bool {
    let Some(value) = value else {
        return default;
    };
    value
        .as_bool()
        .or_else(
            || match value.as_str()?.trim().to_ascii_lowercase().as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
        )
        .unwrap_or(default)
}

fn source_label(source: &AssetLocator) -> String {
    let raw = match source {
        AssetLocator::File(path) => path.file_stem().and_then(|value| value.to_str()),
        AssetLocator::VarEntry { entry, .. } => Path::new(entry)
            .file_stem()
            .and_then(|value| value.to_str()),
        AssetLocator::UnresolvedVaMUrl(url) => {
            Path::new(url).file_stem().and_then(|value| value.to_str())
        }
        AssetLocator::BuiltinTexture(_) => None,
    }
    .unwrap_or("Hair");
    raw.strip_prefix("Preset_").unwrap_or(raw).to_owned()
}

fn infer_hair_sex(reference: &str) -> SkinSex {
    let normalized = format!("/{}/", reference.replace('\\', "/").to_ascii_lowercase());
    if normalized.contains("/custom/hair/female/") {
        SkinSex::Female
    } else if normalized.contains("/custom/hair/male/") {
        SkinSex::Male
    } else {
        SkinSex::Unknown
    }
}

fn merge_sex(current: SkinSex, next: SkinSex) -> SkinSex {
    match (current, next) {
        (SkinSex::Unknown, value) => value,
        (value, SkinSex::Unknown) => value,
        (left, right) if left == right => left,
        _ => SkinSex::Unknown,
    }
}

pub fn load_hair_part_geometry(part: &HairPartReference) -> Result<HairGuideGeometry> {
    let bytes = part.geometry.read_bytes(MAX_HAIR_VAB_BYTES)?;
    parse_hair_vab(&bytes, &part.geometry.display_key())
}

fn find_shared_scalp(presets: &[HairPreset]) -> Option<HairPartReference> {
    presets
        .iter()
        .flat_map(|preset| preset.parts.iter())
        .find(|part| {
            part.geometry
                .read_bytes(MAX_SCALP_PROBE_BYTES)
                .is_ok_and(|bytes| is_bare_scalp_container(&bytes))
                && {
                    let look = load_hair_part_look(part).unwrap_or_default();
                    load_hair_scalp_textures(part, &look).alpha.is_some()
                }
        })
        .cloned()
}

fn is_bare_scalp_container(bytes: &[u8]) -> bool {
    let mut reader = HairCursor::new(bytes, "");
    let opens_with_mesh = reader
        .expect_string("DynamicStore", "container type")
        .is_ok()
        && reader.expect_string("1.0", "DynamicStore version").is_ok()
        && reader.peek_string().is_some_and(|name| name == "DAZMesh");
    opens_with_mesh && parse_hair_vab(bytes, "").is_err()
}

pub fn load_hair_scalp_textures(
    part: &HairPartReference,
    look: &HairLookPatch,
) -> HairScalpTextureBytes {
    let read = |reference: Option<&String>| {
        let locator = sibling_locator(&part.geometry, reference?)?;
        locator.read_bytes(MAX_SCALP_TEXTURE_BYTES).ok()
    };
    HairScalpTextureBytes {
        diffuse: read(look.scalp_diffuse.as_ref()),
        specular: read(look.scalp_specular.as_ref()),
        gloss: read(look.scalp_gloss.as_ref()),
        normal: read(look.scalp_normal.as_ref()),
        alpha: read(look.scalp_alpha.as_ref()),
    }
}

fn find_var_by_reference(addon_root: &std::path::Path, package: &str) -> Option<PathBuf> {
    let (base, wanted_version) = match package.rsplit_once('.') {
        Some((base, version)) if version.eq_ignore_ascii_case("latest") => (base, None),
        Some((base, version)) => match version.parse::<u32>() {
            Ok(version) => (base, Some(version)),
            Err(_) => (package, None),
        },
        None => (package, None),
    };
    let mut best: Option<(u32, PathBuf)> = None;
    let mut pending = vec![(addon_root.to_path_buf(), 0_u8)];
    while let Some((folder, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth < 4 {
                    pending.push((path, depth + 1));
                }
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(stem) = name.strip_suffix(".var") else {
                continue;
            };
            let Some((candidate_base, candidate_version)) = stem.rsplit_once('.') else {
                continue;
            };
            if !candidate_base.eq_ignore_ascii_case(base) {
                continue;
            }
            let Ok(candidate_version) = candidate_version.parse::<u32>() else {
                continue;
            };
            if wanted_version == Some(candidate_version) {
                return Some(path);
            }
            if best
                .as_ref()
                .is_none_or(|(version, _)| candidate_version > *version)
            {
                best = Some((candidate_version, path));
            }
        }
    }
    best.map(|(_, path)| path)
}

fn sibling_locator(anchor: &AssetLocator, reference: &str) -> Option<AssetLocator> {
    let reference = reference.trim();
    if reference.is_empty() {
        return None;
    }
    if let Some((package, entry)) = reference.split_once(":/")
        && !package.eq_ignore_ascii_case("SELF")
        && !package.is_empty()
        && !entry.is_empty()
    {
        let archive_folder = match anchor {
            AssetLocator::VarEntry { archive, .. } => archive.parent()?.to_path_buf(),
            AssetLocator::File(path) => {
                if let Some(addon_packages) = path.ancestors().find(|candidate| {
                    candidate
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("AddonPackages"))
                }) {
                    addon_packages.to_path_buf()
                } else {
                    let custom = path.ancestors().find(|candidate| {
                        candidate
                            .file_name()
                            .is_some_and(|name| name.eq_ignore_ascii_case("Custom"))
                    })?;
                    custom.parent()?.join("AddonPackages")
                }
            }
            _ => return None,
        };

        let archive = archive_folder.join(format!("{package}.var"));
        if archive.is_file() {
            return Some(AssetLocator::VarEntry {
                archive,
                entry: entry.trim_start_matches('/').to_owned(),
            });
        }
        let addon_root = if archive_folder
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("AddonPackages"))
        {
            Some(archive_folder.clone())
        } else {
            archive_folder
                .ancestors()
                .find(|candidate| {
                    candidate
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("AddonPackages"))
                })
                .map(std::path::Path::to_path_buf)
        };
        if let Some(addon_root) = addon_root
            && let Some(archive) = find_var_by_reference(&addon_root, package)
        {
            return Some(AssetLocator::VarEntry {
                archive,
                entry: entry.trim_start_matches('/').to_owned(),
            });
        }
    }
    let rooted = reference
        .strip_prefix("SELF:/")
        .or_else(|| reference.strip_prefix("SELF:"))
        .map(|entry| entry.trim_start_matches('/'));
    let relative = rooted
        .is_none()
        .then(|| reference.trim_start_matches("./").trim_start_matches('/'))
        .filter(|value| !value.is_empty());
    let rooted = rooted.filter(|value| !value.is_empty());
    match anchor {
        AssetLocator::VarEntry { archive, entry } => {
            let resolved = match (rooted, relative) {
                (Some(rooted), _) => rooted.to_owned(),
                (None, Some(relative)) => {
                    let folder = entry.rsplit_once('/').map_or("", |(folder, _)| folder);
                    if folder.is_empty() {
                        relative.to_owned()
                    } else {
                        format!("{folder}/{relative}")
                    }
                }
                _ => return None,
            };
            (!resolved.is_empty()).then(|| AssetLocator::VarEntry {
                archive: archive.clone(),
                entry: resolved,
            })
        }
        AssetLocator::File(path) => match (rooted, relative) {
            (Some(rooted), _) => {
                let custom = path.ancestors().find(|candidate| {
                    candidate
                        .file_name()
                        .is_some_and(|name| name.eq_ignore_ascii_case("Custom"))
                })?;
                Some(AssetLocator::File(custom.parent()?.join(rooted)))
            }
            (None, Some(relative)) => Some(AssetLocator::File(path.parent()?.join(relative))),
            _ => None,
        },
        _ => None,
    }
}

pub fn load_hair_part_scalp(part: &HairPartReference) -> Result<HairScalpGeometry> {
    let bytes = part.geometry.read_bytes(MAX_HAIR_VAB_BYTES)?;
    parse_hair_scalp_vab(&bytes, &part.geometry.display_key())
}

pub fn load_hair_part_asset(part: &HairPartReference) -> Result<HairPartAsset> {
    let geometry = load_hair_part_geometry(part)?;
    let (look, physics) = load_hair_part_settings(part)?;
    Ok(HairPartAsset {
        geometry,
        look,
        physics,
    })
}

pub fn read_hair_storables(locator: &AssetLocator) -> Result<Vec<Value>> {
    let bytes = locator.read_bytes(MAX_HAIR_PRESET_BYTES)?;
    let document = crate::vam::simple_json::parse_document(&bytes)
        .map_err(|error| invalid_preset(locator, error.to_string()))?;
    Ok(document
        .get("storables")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

pub fn hair_sim_storable<'a>(storables: &'a [Value], internal_id: &str) -> Option<&'a Value> {
    find_sim_storable(storables, internal_id).or_else(|| {
        storables.iter().find(|candidate| {
            candidate
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| id.ends_with("Sim"))
        })
    })
}

pub fn hair_scalp_material_storable<'a>(
    storables: &'a [Value],
    internal_id: &str,
) -> Option<&'a Value> {
    if internal_id.is_empty() {
        let mut lone = storables.iter().filter(|storable| {
            storable
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(is_scalp_material_id)
        });
        let first = lone.next();
        return if lone.next().is_none() { first } else { None };
    }
    storables.iter().find(|storable| {
        storable
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| scalp_material_id_matches(id, internal_id))
    })
}

fn is_scalp_material_id(id: &str) -> bool {
    id.to_ascii_lowercase().contains("scalpmaterial")
}

fn scalp_material_id_matches(id: &str, internal_id: &str) -> bool {
    if !is_scalp_material_id(id) {
        return false;
    }
    fn unqualify(value: &str) -> &str {
        value.split_once(':').map_or(value, |(_, rest)| rest)
    }
    id.starts_with(internal_id) || unqualify(id).starts_with(unqualify(internal_id))
}

pub fn load_hair_part_look(part: &HairPartReference) -> Result<HairLookPatch> {
    load_hair_part_settings(part).map(|(look, _)| look)
}

pub fn load_hair_part_settings(
    part: &HairPartReference,
) -> Result<(HairLookPatch, HairPhysicsSettings)> {
    let mut look = HairLookPatch::default();
    let mut physics = HairPhysicsPatch::default();

    if let Some(locator) = &part.appearance
        && let Ok(bytes) = locator.read_bytes(MAX_HAIR_PRESET_BYTES)
        && let Ok(document) = crate::vam::simple_json::parse_document(&bytes)
        && let Some(storables) = document.get("storables").and_then(Value::as_array)
    {
        {
            let storable = find_sim_storable(storables, &part.internal_id).or_else(|| {
                storables.iter().find(|candidate| {
                    candidate
                        .get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| id.ends_with("Sim"))
                })
            });
            if let Some(storable) = storable {
                look = parse_hair_look(storable);
                physics = parse_hair_physics(storable);
            }
            look.apply(&parse_scalp_material(storables, &part.internal_id));
        }
    }
    look.apply(&part.preset_look);
    physics.apply(&part.preset_physics);
    Ok((look, physics.resolve()))
}

fn walk_component_blocks(reader: &mut HairCursor<'_>, locator: &str) -> Result<()> {
    while let Some(name) = reader.peek_string() {
        match name.as_str() {
            "DAZMesh" => {
                read_daz_mesh_block(reader, locator)?;
            }
            "DAZSkinWrap" => {
                skip_daz_skin_wrap_block(reader)?;
            }
            "MaterialOptions" => {
                skip_material_options_block(reader)?;
            }
            other => {
                return Err(unsupported_geometry(
                    locator,
                    format!(
                        "component block {other:?} sits before the item payload and this reader cannot measure it"
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn skip_material_options_block(reader: &mut HairCursor<'_>) -> Result<()> {
    reader.expect_string("MaterialOptions", "container type")?;
    reader.expect_string("1.0", "MaterialOptions version")?;
    reader.read_string("material options override id")?;
    let count = reader.read_count("material slot count", MAX_SCALP_MATERIALS)?;
    for _ in 0..count {
        reader.read_i32("material slot")?;
    }
    Ok(())
}

fn skip_daz_skin_wrap_block(reader: &mut HairCursor<'_>) -> Result<()> {
    reader.expect_string("DAZSkinWrap", "container type")?;
    reader.expect_string("1.0", "DAZSkinWrap version")?;
    reader.read_string("skin wrap name")?;
    reader.expect_string("DAZSkinWrapStore", "skin wrap store type")?;
    reader.expect_string("1.0", "skin wrap store version")?;
    let count = reader.read_count("skin wrap vertex count", MAX_SCALP_VERTICES)?;
    for _ in 0..count {
        for _ in 0..4 {
            reader.read_i32("skin wrap binding")?;
        }
        for _ in 0..6 {
            reader.read_f32("skin wrap projection")?;
        }
    }
    Ok(())
}

fn read_daz_mesh_block(reader: &mut HairCursor<'_>, locator: &str) -> Result<HairScalpGeometry> {
    reader.expect_string("DAZMesh", "container type")?;
    reader.expect_string("1.0", "DAZMesh version")?;
    for index in 0..SCALP_GROUP_NAMES {
        reader.read_string(&format!("group name {index}"))?;
    }

    let vertex_count = reader.read_count("scalp vertex count", MAX_SCALP_VERTICES)?;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        vertices.push([
            reader.read_f32("scalp vertex x")? * 100.0,
            reader.read_f32("scalp vertex y")? * 100.0,
            reader.read_f32("scalp vertex z")? * 100.0,
        ]);
    }

    let material_count = reader.read_count("scalp material count", MAX_SCALP_MATERIALS)?;
    let mut materials = Vec::with_capacity(material_count);
    for index in 0..material_count {
        materials.push(reader.read_string(&format!("scalp material {index}"))?);
    }

    let face_count = reader.read_count("scalp face count", MAX_SCALP_FACES)?;
    let mut triangles = Vec::with_capacity(face_count * 2);
    for _ in 0..face_count {
        let _material = reader.read_i32("scalp face material")?;
        let corners = reader.read_count("scalp face corner count", MAX_SCALP_FACE_CORNERS)?;
        let mut face = Vec::with_capacity(corners);
        for _ in 0..corners {
            let index = reader.read_nonnegative_u32("scalp face corner")?;
            if index as usize >= vertex_count {
                return Err(invalid_geometry(
                    locator,
                    format!("scalp face corner {index} is outside {vertex_count} vertices"),
                ));
            }
            face.push(index);
        }
        for corner in 1..corners.saturating_sub(1) {
            triangles.push([face[0], face[corner], face[corner + 1]]);
        }
    }
    for _ in 0..face_count {
        reader.read_i32("scalp uv face material")?;
        let corners = reader.read_count("scalp uv face corner count", MAX_SCALP_FACE_CORNERS)?;
        for _ in 0..corners {
            reader.read_nonnegative_u32("scalp uv face corner")?;
        }
    }

    let uv_count = reader.read_count("scalp uv count", MAX_SCALP_VERTICES)?;
    let mut uvs = Vec::with_capacity(uv_count);
    for _ in 0..uv_count {
        uvs.push([
            reader.read_f32("scalp uv u")?,
            reader.read_f32("scalp uv v")?,
        ]);
    }
    if uvs.len() != vertex_count {
        uvs.clear();
    }

    let map_count = reader.read_count("scalp uv vertex map count", MAX_SCALP_VERTICES)?;
    for _ in 0..map_count {
        reader.read_i32("uv vertex map from")?;
        reader.read_i32("uv vertex map to")?;
        reader.read_i32("uv vertex map poly")?;
    }

    Ok(HairScalpGeometry {
        materials,
        vertices_cm: vertices,
        uvs,
        triangles,
    })
}

pub fn parse_hair_scalp_vab(bytes: &[u8], locator: &str) -> Result<HairScalpGeometry> {
    let mut reader = HairCursor::new(bytes, locator);
    reader.expect_string("DynamicStore", "container type")?;
    reader.expect_string("1.0", "DynamicStore version")?;
    let geometry = read_daz_mesh_block(&mut reader, locator)?;
    if geometry.triangles.is_empty() {
        return Err(invalid_geometry(locator, "scalp mesh has no faces"));
    }
    Ok(geometry)
}

pub fn parse_hair_vab(bytes: &[u8], locator: &str) -> Result<HairGuideGeometry> {
    let mut reader = HairCursor::new(bytes, locator);
    reader.expect_string("DynamicStore", "container type")?;
    reader.expect_string("1.0", "DynamicStore version")?;

    walk_component_blocks(&mut reader, locator)?;
    if !reader.read_bool("hair geometry flag")? {
        return Err(invalid_geometry(locator, "VAB contains no hair geometry"));
    }
    let creator = reader.read_string("geometry creator")?;
    if creator != "RuntimeHairGeometryCreator" {
        return Err(unsupported_geometry(
            locator,
            format!("hair geometry creator {creator:?} is not simulated strands"),
        ));
    }
    let schema = reader.read_string("runtime hair schema")?;
    if schema != "1.0" && schema != "1.1" {
        return Err(invalid_geometry(
            locator,
            format!("unsupported RuntimeHairGeometryCreator schema {schema:?}"),
        ));
    }
    let provider_name = reader.read_string("provider name")?;
    let segments = reader.read_count("segment count", MAX_SEGMENTS)?;
    if segments == 0 {
        return Err(invalid_geometry(locator, "segment count is zero"));
    }
    let segment_length_m = reader.read_f32("segment length")?;
    if segment_length_m <= 0.0 {
        return Err(invalid_geometry(locator, "segment length must be positive"));
    }
    let _mask_name = reader.read_string("scalp mask name")?;

    let scalp_vertex_count = reader.read_count("scalp mask length", MAX_STRAND_SLOTS)?;
    for _ in 0..scalp_vertex_count {
        reader.read_bool("scalp mask value")?;
    }

    let strand_slots = reader.read_count("strand slot count", MAX_STRAND_SLOTS)?;
    if strand_slots != scalp_vertex_count {
        return Err(invalid_geometry(
            locator,
            format!(
                "scalp mask length {scalp_vertex_count} does not match strand slot count {strand_slots}"
            ),
        ));
    }
    let mut guides = Vec::new();
    let mut retained_rigidity_indices = Vec::new();

    let mut flat_point_map = std::collections::HashMap::new();
    let mut total_points = 0usize;
    for _ in 0..strand_slots {
        let scalp_index = reader.read_nonnegative_u32("scalp vertex index")?;
        let point_count = reader.read_count("guide point count", MAX_HAIR_POINTS)?;
        let flattened_start = total_points;
        total_points = total_points
            .checked_add(point_count)
            .filter(|count| *count <= MAX_HAIR_POINTS)
            .ok_or_else(|| {
                invalid_geometry(locator, "total guide point count exceeds the limit")
            })?;

        let mut points_cm = Vec::with_capacity(point_count);
        let mut retained = Vec::with_capacity(point_count);
        for point_index in 0..point_count {
            let x = reader.read_raw_f32("guide point x")?;
            let y = reader.read_raw_f32("guide point y")?;
            let z = reader.read_raw_f32("guide point z")?;
            if x.is_finite() && y.is_finite() && z.is_finite() {
                points_cm.push([x * 100.0, y * 100.0, z * 100.0]);
                retained.push(flattened_start + point_index);
            }
        }
        if !points_cm.is_empty() {
            for (kept_position, flat_index) in retained.iter().enumerate() {
                flat_point_map.insert(*flat_index, [guides.len() as u32, kept_position as u32]);
            }
            guides.push(HairGuide {
                scalp_index,
                points_cm,
                rigidity: Vec::new(),
            });
            retained_rigidity_indices.push(retained);
        }
    }

    let index_count = reader.read_count("guide index count", MAX_RENDER_INDICES)?;
    if index_count % 3 != 0 {
        return Err(invalid_geometry(
            locator,
            "guide index count is not divisible by three",
        ));
    }
    let mut flat_indices = Vec::with_capacity(index_count);
    for _ in 0..index_count {
        flat_indices.push(reader.read_nonnegative_u32("guide triangle index")?);
    }
    if flat_indices
        .iter()
        .any(|index| *index as usize >= guides.len())
    {
        return Err(invalid_geometry(
            locator,
            "guide triangle references a missing active guide",
        ));
    }
    let guide_triangles = flat_indices
        .chunks_exact(3)
        .map(|triangle| [triangle[0], triangle[1], triangle[2]])
        .collect();

    let vertex_count = reader.read_count("flattened guide vertex count", MAX_HAIR_POINTS)?;
    if vertex_count != total_points {
        return Err(invalid_geometry(
            locator,
            format!(
                "flattened guide vertex count {vertex_count} does not match strand points {total_points}"
            ),
        ));
    }

    for _ in 0..vertex_count {
        reader.read_raw_f32("flattened guide vertex x")?;
        reader.read_raw_f32("flattened guide vertex y")?;
        reader.read_raw_f32("flattened guide vertex z")?;
    }

    let mut rigidity_values = Vec::new();
    if schema == "1.1" {
        let rigidity_count = reader.read_count("rigidity count", MAX_HAIR_POINTS)?;
        rigidity_values.reserve(rigidity_count);
        for _ in 0..rigidity_count {
            rigidity_values.push(reader.read_f32("rigidity")?);
        }
    }
    // No painted rigidity is a state, not a value. RuntimeHairGeometryCreator
    // sets `rigidities = null` for a version-1.0 file and for a 1.1 file that
    // stores a count of zero, and BuildPointJoints then falls through to the
    // root/main/tip ramp. Substituting 1.0 instead pinned every particle to its
    // rest pose — the solver's own `Rigidity >= 1` snap — so thirteen shipped
    // items rendered with no physics at all: no gravity, no swing, no collision
    // response, and nothing to say why.
    if !rigidity_values.is_empty() {
        for (guide, retained) in guides.iter_mut().zip(retained_rigidity_indices) {
            guide.rigidity = retained
                .into_iter()
                .map(|index| rigidity_values.get(index).copied().unwrap_or(1.0))
                .collect();
        }
    }

    let root_map_count = reader.read_count("root map count", MAX_STRAND_SLOTS)?;
    if root_map_count != guides.len() {
        return Err(invalid_geometry(
            locator,
            format!(
                "root map count {root_map_count} does not match active guide count {}",
                guides.len()
            ),
        ));
    }
    let mut root_map = Vec::with_capacity(root_map_count);
    for _ in 0..root_map_count {
        root_map.push(reader.read_nonnegative_u32("root map index")?);
    }

    let mut nearby_joints = Vec::new();
    if !reader.is_exhausted() {
        let point_ratio = |guide: u32, point: u32| -> f32 {
            let count = guides[guide as usize].points_cm.len().max(2);
            point as f32 / (count - 1) as f32
        };
        let group_count = reader.read_count("nearby group count", MAX_STRAND_SLOTS)?;
        for _ in 0..group_count {
            let joint_count = reader.read_count("nearby joint count", MAX_HAIR_POINTS * 4)?;
            for _ in 0..joint_count {
                let a_flat = reader.read_f32("nearby joint a")?;
                let b_flat = reader.read_f32("nearby joint b")?;
                let _rest_distance_m = reader.read_raw_f32("nearby joint distance")?;
                let _distance_ratio = reader.read_raw_f32("nearby joint ratio")?;
                if !(a_flat.is_finite() && b_flat.is_finite()) || a_flat < 0.0 || b_flat < 0.0 {
                    continue;
                }

                let (Some(a), Some(b)) = (
                    flat_point_map.get(&(a_flat as usize)).copied(),
                    flat_point_map.get(&(b_flat as usize)).copied(),
                ) else {
                    continue;
                };

                let elasticity =
                    ((1.0 - point_ratio(a[0], a[1])) + (1.0 - point_ratio(b[0], b[1]))) * 0.5;
                nearby_joints.push(HairNearbyJoint { a, b, elasticity });
            }
        }
    }

    Ok(HairGuideGeometry {
        provider_name,
        segments,
        segment_length_cm: segment_length_m * 100.0,
        scalp_vertex_count,
        guides,
        guide_triangles,
        root_map,
        nearby_joints,
    })
}

struct HairCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
    locator: &'a str,
}

impl<'a> HairCursor<'a> {
    fn new(bytes: &'a [u8], locator: &'a str) -> Self {
        Self {
            bytes,
            offset: 0,
            locator,
        }
    }

    fn take(&mut self, count: usize, field: &str) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| invalid_geometry(self.locator, format!("{field} is truncated")))?;
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn is_exhausted(&self) -> bool {
        self.offset >= self.bytes.len()
    }

    fn peek_string(&self) -> Option<String> {
        let length = usize::from(*self.bytes.get(self.offset)?);
        if length < 2 {
            return None;
        }
        let start = self.offset + 1;
        let text = self.bytes.get(start..start + length)?;
        text.iter()
            .all(|byte| byte.is_ascii_alphanumeric())
            .then(|| String::from_utf8_lossy(text).into_owned())
    }

    fn read_bool(&mut self, field: &str) -> Result<bool> {
        match self.take(1, field)?[0] {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(invalid_geometry(
                self.locator,
                format!("{field} has invalid boolean byte {value}"),
            )),
        }
    }

    fn read_i32(&mut self, field: &str) -> Result<i32> {
        Ok(i32::from_le_bytes(
            self.take(4, field)?.try_into().expect("four bytes"),
        ))
    }

    fn read_nonnegative_u32(&mut self, field: &str) -> Result<u32> {
        let value = self.read_i32(field)?;
        u32::try_from(value)
            .map_err(|_| invalid_geometry(self.locator, format!("{field} is negative ({value})")))
    }

    fn read_count(&mut self, field: &str, maximum: usize) -> Result<usize> {
        let value = self.read_i32(field)?;
        let value = usize::try_from(value).map_err(|_| {
            invalid_geometry(self.locator, format!("{field} is negative ({value})"))
        })?;
        if value > maximum {
            return Err(invalid_geometry(
                self.locator,
                format!("{field} {value} exceeds {maximum}"),
            ));
        }
        Ok(value)
    }

    fn read_f32(&mut self, field: &str) -> Result<f32> {
        let value = self.read_raw_f32(field)?;
        value
            .is_finite()
            .then_some(value)
            .ok_or_else(|| invalid_geometry(self.locator, format!("{field} is not finite")))
    }

    fn read_raw_f32(&mut self, field: &str) -> Result<f32> {
        Ok(f32::from_le_bytes(
            self.take(4, field)?.try_into().expect("four bytes"),
        ))
    }

    fn read_string(&mut self, field: &str) -> Result<String> {
        let mut length = 0usize;
        let mut shift = 0u32;
        for _ in 0..5 {
            let byte = self.take(1, field)?[0];
            length |= usize::from(byte & 0x7f)
                .checked_shl(shift)
                .ok_or_else(|| invalid_geometry(self.locator, format!("{field} is too long")))?;
            if byte & 0x80 == 0 {
                let bytes = self.take(length, field)?;
                return std::str::from_utf8(bytes)
                    .map(str::to_owned)
                    .map_err(|error| {
                        invalid_geometry(self.locator, format!("{field} is not UTF-8: {error}"))
                    });
            }
            shift += 7;
        }
        Err(invalid_geometry(
            self.locator,
            format!("{field} has an invalid .NET string length"),
        ))
    }

    fn expect_string(&mut self, expected: &str, field: &str) -> Result<()> {
        let actual = self.read_string(field)?;
        if actual == expected {
            Ok(())
        } else {
            Err(invalid_geometry(
                self.locator,
                format!("{field} is {actual:?}, expected {expected:?}"),
            ))
        }
    }
}

fn walk_files_if_exists(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut stack = vec![root.to_path_buf()];
    let mut result = Vec::new();
    while let Some(folder) = stack.pop() {
        let mut entries = fs::read_dir(&folder)
            .map_err(|error| io_error(&folder, error))?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|error| io_error(&folder, error))?;
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
        for entry in entries.into_iter().rev() {
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| io_error(&path, error))?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                result.push(path);
                if result.len() > MAX_WALK_FILES {
                    return Err(VaMError::InvalidHairPreset {
                        locator: root.display().to_string(),
                        message: format!("file count exceeds {MAX_WALK_FILES}"),
                    });
                }
            }
        }
    }
    result.sort_by_key(|path| path_sort_key(path));
    Ok(result)
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

fn path_sort_key(path: &Path) -> String {
    path.to_string_lossy().to_ascii_lowercase()
}

fn normalized_archive_path(path: &str) -> String {
    path.trim_start_matches(['/', '\\'])
        .replace('\\', "/")
        .to_ascii_lowercase()
}

fn path_to_forward(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn is_person_hair_path(path: &str) -> bool {
    let normalized = format!("/{}/", normalized_archive_path(path));
    normalized.contains("/custom/atom/person/hair/")
}

fn replace_extension(path: &str, extension: &str) -> String {
    match path.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}.{extension}"),
        None => format!("{path}.{extension}"),
    }
}

fn select_package_index<T>(
    key: &str,
    exact: &BTreeMap<String, Vec<usize>>,
    uid: impl Fn(usize) -> T,
) -> Option<usize>
where
    T: AsRef<str>,
{
    if let Some(index) = exact.get(key).and_then(|values| values.first()) {
        return Some(*index);
    }
    let base = key.strip_suffix(".latest")?;
    exact
        .iter()
        .filter(|(candidate, _)| candidate.starts_with(&format!("{base}.")))
        .flat_map(|(_, values)| values.iter().copied())
        .max_by(|left, right| {
            version_key(uid(*left).as_ref()).cmp(&version_key(uid(*right).as_ref()))
        })
}

fn version_key(uid: &str) -> Vec<(bool, u64, String)> {
    uid.split('.')
        .map(|part| match part.parse::<u64>() {
            Ok(number) => (true, number, String::new()),
            Err(_) => (false, 0, part.to_ascii_lowercase()),
        })
        .collect()
}

fn invalid_preset(locator: &AssetLocator, message: impl Into<String>) -> VaMError {
    VaMError::InvalidHairPreset {
        locator: locator.display_key(),
        message: message.into(),
    }
}

fn invalid_geometry(locator: &str, message: impl Into<String>) -> VaMError {
    VaMError::InvalidHairGeometry {
        locator: locator.to_owned(),
        message: message.into(),
    }
}

fn unsupported_geometry(locator: &str, message: impl Into<String>) -> VaMError {
    VaMError::UnsupportedHairGeometry {
        locator: locator.to_owned(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_creator_qualified_scalp_material_still_finds_its_textures() {
        let storables = serde_json::json!([
            {"id": "AKKEVE:AKKEVE hair014 01UdaneScalpMaterial",
             "customTexture_MainTex": "SELF:/Custom/Hair/Female/AKKEVE/Udane scalp/6.jpg",
             "customTexture_AlphaTex": "./scalp/alpha.png"},
        ]);
        let storables = storables.as_array().unwrap();

        let patch = parse_scalp_material(storables, "AKKEVE hair014 01");
        assert_eq!(
            patch.scalp_diffuse.as_deref(),
            Some("SELF:/Custom/Hair/Female/AKKEVE/Udane scalp/6.jpg"),
        );
        assert_eq!(patch.scalp_alpha.as_deref(), Some("./scalp/alpha.png"));

        let loose = serde_json::json!([
            {"id": "MohawkV3UdaneScalpMaterial", "customTexture_MainTex": "./main.png"},
        ]);
        let loose = parse_scalp_material(loose.as_array().unwrap(), "MohawkV3");
        assert_eq!(loose.scalp_diffuse.as_deref(), Some("./main.png"));

        let other = parse_scalp_material(storables, "RenVR Lexi Long");
        assert!(
            other.scalp_diffuse.is_none(),
            "matched a different hair's scalp"
        );
    }

    #[test]
    fn a_preset_hands_each_part_its_own_scalp_material() {
        let storables = serde_json::json!([
            {"id": "AKKEVE:AKKEVE hair051 04UdaneScalpMaterial",
             "customTexture_AlphaTex": "SELF:/Custom/Hair/Female/AKKEVE/Udane scalp/12.jpg"},
            {"id": "AKKEVE:AKKEVE hair051 05UdaneScalpMaterial",
             "customTexture_MainTex": "",
             "customTexture_AlphaTex": "SELF:/Custom/Hair/Female/AKKEVE/Udane scalp/34.jpg",
             "Alpha Adjust": "-0.09438676"},
            {"id": "AKKEVE:AKKEVE hair051 06KrayonScalpMaterial",
             "customTexture_AlphaTex": "SELF:/Custom/Hair/Female/AKKEVE/Krayon scalp/9.jpg"},
        ]);
        let storables = storables.as_array().unwrap();

        let patch = parse_scalp_material(storables, "AKKEVE:AKKEVE hair051 05");
        assert_eq!(
            patch.scalp_alpha.as_deref(),
            Some("SELF:/Custom/Hair/Female/AKKEVE/Udane scalp/34.jpg"),
            "a creator-qualified internalId must find its own part, which is the \
             only shape VaM writes",
        );
        assert!(
            patch.scalp_diffuse.is_none(),
            "an empty slot stays empty rather than borrowing a neighbour's sheet",
        );

        let sibling = parse_scalp_material(storables, "AKKEVE:AKKEVE hair051 06");
        assert_eq!(
            sibling.scalp_alpha.as_deref(),
            Some("SELF:/Custom/Hair/Female/AKKEVE/Krayon scalp/9.jpg"),
        );

        let stranger = parse_scalp_material(storables, "AKKEVE:AKKEVE hair051 01");
        assert!(stranger.scalp_alpha.is_none(), "matched a neighbour's cap");
    }

    #[test]
    fn a_nameless_part_only_borrows_a_scalp_material_when_there_is_exactly_one() {
        let lone = serde_json::json!([
            {"id": "MohawkV3UdaneScalpMaterial", "customTexture_AlphaTex": "./a.png"},
        ]);
        assert_eq!(
            parse_scalp_material(lone.as_array().unwrap(), "")
                .scalp_alpha
                .as_deref(),
            Some("./a.png"),
        );

        let crowd = serde_json::json!([
            {"id": "OneUdaneScalpMaterial", "customTexture_AlphaTex": "./a.png"},
            {"id": "TwoUdaneScalpMaterial", "customTexture_AlphaTex": "./b.png"},
        ]);
        assert!(
            parse_scalp_material(crowd.as_array().unwrap(), "")
                .scalp_alpha
                .is_none(),
            "with no name to go on, guessing between caps is worse than nothing",
        );
    }

    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn the_curl_family_parses_and_resolves_with_vam_defaults() {
        let storable: Value = serde_json::from_str(
            r#"{
                "id": "HairSim",
                "curlX": "0.4", "curlY": "0.0", "curlZ": "0.8",
                "curlScale": "0.02", "curlFrequency": "6",
                "curlScaleRandomness": "0.5", "curlFrequencyRandomness": "0.25",
                "curlAllowReverse": "true", "curlAllowFlipAxis": "false",
                "curlRoot": "0.0", "curlMid": "1.0", "curlTip": "0.2",
                "curlMidpoint": "0.5", "curlCurvePower": "2.0"
            }"#,
        )
        .unwrap();
        let waviness = parse_hair_look(&storable).waviness_settings();
        assert_eq!(waviness.vector_m, [0.4, 0.0, 0.8]);
        assert_eq!(waviness.scale, 0.02);
        assert_eq!(waviness.frequency, 6.0);
        assert!(waviness.allow_reverse);
        assert!(!waviness.allow_flip_axis);

        assert!((waviness.envelope(0.0) - 0.0).abs() < 1.0e-6);
        assert!((waviness.envelope(0.5) - 1.0).abs() < 1.0e-6);
        assert!((waviness.envelope(1.0) - 0.2).abs() < 1.0e-6);

        assert!((waviness.envelope(0.25) - 0.75).abs() < 1.0e-6);

        let silent = parse_hair_look(&serde_json::from_str::<Value>("{}").unwrap());
        let defaults = silent.waviness_settings();
        assert_eq!(defaults.scale, 0.0);
        assert_eq!(defaults.vector_m, [0.1, 0.0, 0.1]);
    }

    fn write_string(bytes: &mut Vec<u8>, value: &str) {
        let mut length = value.len();
        while length >= 0x80 {
            bytes.push((length as u8) | 0x80);
            length >>= 7;
        }
        bytes.push(length as u8);
        bytes.extend_from_slice(value.as_bytes());
    }

    fn write_i32(bytes: &mut Vec<u8>, value: i32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn write_f32(bytes: &mut Vec<u8>, value: f32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn direct_vab(schema: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_string(&mut bytes, "DynamicStore");
        write_string(&mut bytes, "1.0");
        bytes.push(1);
        write_string(&mut bytes, "RuntimeHairGeometryCreator");
        write_string(&mut bytes, schema);
        write_string(&mut bytes, "FemaleCustom");
        write_i32(&mut bytes, 2);
        write_f32(&mut bytes, 0.01);
        write_string(&mut bytes, "mask");
        write_i32(&mut bytes, 2);
        bytes.extend_from_slice(&[1, 1]);
        write_i32(&mut bytes, 2);
        for (scalp, x) in [(10, 0.1), (20, 0.2)] {
            write_i32(&mut bytes, scalp);
            write_i32(&mut bytes, 2);
            for point in [[x, 1.0, 2.0], [x, 1.1, 2.0]] {
                for value in point {
                    write_f32(&mut bytes, value);
                }
            }
        }
        write_i32(&mut bytes, 3);
        for index in [0, 1, 0] {
            write_i32(&mut bytes, index);
        }
        write_i32(&mut bytes, 4);
        for _ in 0..4 {
            for value in [0.0, 0.0, 0.0] {
                write_f32(&mut bytes, value);
            }
        }
        if schema == "1.1" {
            write_i32(&mut bytes, 4);
            for _ in 0..4 {
                write_f32(&mut bytes, 0.5);
            }
        }
        write_i32(&mut bytes, 2);
        write_i32(&mut bytes, 10);
        write_i32(&mut bytes, 20);
        bytes
    }

    #[test]
    fn parses_direct_runtime_hair_in_metres_as_centimetres() {
        for schema in ["1.0", "1.1"] {
            let geometry = parse_hair_vab(&direct_vab(schema), "test.vab").unwrap();
            assert_eq!(geometry.provider_name, "FemaleCustom");
            assert_eq!(geometry.segments, 2);
            assert_eq!(geometry.segment_length_cm, 1.0);
            assert_eq!(geometry.guides.len(), 2);
            assert_eq!(geometry.guides[0].points_cm[0], [10.0, 100.0, 200.0]);
            // A 1.0 file paints no rigidity at all, and the absence has to
            // survive as an absence: RuntimeHairGeometryCreator nulls the
            // array and BuildPointJoints falls through to the root/main/tip
            // ramp. Filling it with 1.0 instead welds every particle to rest.
            assert_eq!(
                geometry.guides[0].rigidity,
                if schema == "1.1" {
                    vec![0.5; 2]
                } else {
                    Vec::new()
                },
            );
            assert_eq!(geometry.guide_triangles, vec![[0, 1, 0]]);
            assert_eq!(geometry.root_map, vec![10, 20]);
        }
    }

    fn daz_mesh_block() -> Vec<u8> {
        let mut bytes = Vec::new();
        write_string(&mut bytes, "DAZMesh");
        write_string(&mut bytes, "1.0");
        for name in ["untitled", "untitled-2", "untitled-1", "untitled-3"] {
            write_string(&mut bytes, name);
        }
        write_i32(&mut bytes, 4);
        for point in [
            [0.0_f32, 1.8, 0.0],
            [0.1, 1.8, 0.0],
            [0.1, 1.8, 0.1],
            [0.0, 1.8, 0.1],
        ] {
            for axis in point {
                write_f32(&mut bytes, axis);
            }
        }
        write_i32(&mut bytes, 1);
        write_string(&mut bytes, "Material");
        write_i32(&mut bytes, 1);
        write_i32(&mut bytes, 0);
        write_i32(&mut bytes, 4);
        for corner in [0, 1, 2, 3] {
            write_i32(&mut bytes, corner);
        }
        write_i32(&mut bytes, 0);
        write_i32(&mut bytes, 4);
        for corner in [0, 1, 2, 3] {
            write_i32(&mut bytes, corner);
        }
        write_i32(&mut bytes, 4);
        for uv in [[0.0_f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
            write_f32(&mut bytes, uv[0]);
            write_f32(&mut bytes, uv[1]);
        }
        write_i32(&mut bytes, 1);
        for value in [0, 0, 0] {
            write_i32(&mut bytes, value);
        }
        bytes
    }

    #[test]
    fn a_component_block_before_the_hair_is_walked_past_not_refused() {
        for prefix in [
            daz_mesh_block(),
            material_options_block(),
            skin_wrap_block(),
        ] {
            let mut bytes = Vec::new();
            write_string(&mut bytes, "DynamicStore");
            write_string(&mut bytes, "1.0");
            bytes.extend_from_slice(&prefix);
            let plain = direct_vab("1.0");
            let header = {
                let mut header = Vec::new();
                write_string(&mut header, "DynamicStore");
                write_string(&mut header, "1.0");
                header
            };
            bytes.extend_from_slice(&plain[header.len()..]);

            let geometry = parse_hair_vab(&bytes, "wrapped.vab").expect("composite container");
            assert_eq!(geometry.provider_name, "FemaleCustom");
            assert_eq!(geometry.guides.len(), 2);
            assert_eq!(geometry.root_map, vec![10, 20]);
        }
    }

    fn material_options_block() -> Vec<u8> {
        let mut bytes = Vec::new();
        write_string(&mut bytes, "MaterialOptions");
        write_string(&mut bytes, "1.0");
        write_string(&mut bytes, "override");
        write_i32(&mut bytes, 2);
        write_i32(&mut bytes, 0);
        write_i32(&mut bytes, 1);
        bytes
    }

    fn skin_wrap_block() -> Vec<u8> {
        let mut bytes = Vec::new();
        write_string(&mut bytes, "DAZSkinWrap");
        write_string(&mut bytes, "1.0");
        write_string(&mut bytes, "wrap");
        write_string(&mut bytes, "DAZSkinWrapStore");
        write_string(&mut bytes, "1.0");
        write_i32(&mut bytes, 1);
        for value in [7, 1, 2, 3] {
            write_i32(&mut bytes, value);
        }
        for _ in 0..6 {
            write_f32(&mut bytes, 0.25);
        }
        bytes
    }

    #[test]
    fn an_unmeasurable_component_block_is_named_rather_than_guessed_at() {
        let mut bytes = Vec::new();
        write_string(&mut bytes, "DynamicStore");
        write_string(&mut bytes, "1.0");
        write_string(&mut bytes, "SomeFutureComponent");
        let error = parse_hair_vab(&bytes, "future.vab").unwrap_err();
        assert!(
            matches!(error, VaMError::UnsupportedHairGeometry { .. }),
            "{error}"
        );
        assert!(error.to_string().contains("SomeFutureComponent"), "{error}");
    }

    #[test]
    fn a_creator_that_is_not_the_hair_one_is_unsupported_and_a_truncated_one_is_corrupt() {
        let mut other = Vec::new();
        write_string(&mut other, "DynamicStore");
        write_string(&mut other, "1.0");
        other.push(1);
        write_string(&mut other, "SomeOtherCreator");
        let error = parse_hair_vab(&other, "other.vab").unwrap_err();
        assert!(
            matches!(error, VaMError::UnsupportedHairGeometry { .. }),
            "{error}"
        );

        let mut broken = Vec::new();
        write_string(&mut broken, "DynamicStore");
        write_string(&mut broken, "1.0");
        broken.push(1);
        write_string(&mut broken, "RuntimeHairGeometryCreator");
        let error = parse_hair_vab(&broken, "broken.vab").unwrap_err();
        assert!(
            matches!(error, VaMError::InvalidHairGeometry { .. }),
            "{error}"
        );
    }

    #[test]
    fn scalp_vab_decodes_into_a_triangulated_cap() {
        let mut bytes = Vec::new();
        write_string(&mut bytes, "DynamicStore");
        write_string(&mut bytes, "1.0");
        bytes.extend_from_slice(&daz_mesh_block());

        let scalp = parse_hair_scalp_vab(&bytes, "cap.vab").unwrap();
        assert_eq!(
            scalp.uvs,
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
        );
        assert_eq!(scalp.materials, vec!["Material".to_owned()]);
        assert_eq!(scalp.vertices_cm.len(), 4);
        assert_eq!(scalp.triangles, vec![[0, 1, 2], [0, 2, 3]]);
        assert_eq!(scalp.vertices_cm[1], [10.0, 180.0, 0.0]);
    }

    #[test]
    fn texture_references_resolve_both_of_vams_forms() {
        let anchor = AssetLocator::VarEntry {
            archive: PathBuf::from("pack.var"),
            entry: "Custom/Hair/Female/VL_13/Scalp/v1/Scalp_v1_high.vaj".to_owned(),
        };
        assert_eq!(
            sibling_locator(&anchor, "SELF:/Custom/Hair/main3.png"),
            Some(AssetLocator::VarEntry {
                archive: PathBuf::from("pack.var"),
                entry: "Custom/Hair/main3.png".to_owned(),
            })
        );
        assert_eq!(
            sibling_locator(&anchor, "./texture/NorikoBase_alpha.png"),
            Some(AssetLocator::VarEntry {
                archive: PathBuf::from("pack.var"),
                entry: "Custom/Hair/Female/VL_13/Scalp/v1/texture/NorikoBase_alpha.png".to_owned(),
            })
        );
        assert_eq!(sibling_locator(&anchor, "   "), None);
    }

    #[test]
    fn explicit_package_texture_reference_switches_to_the_named_archive() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "vkit-hair-texture-package-{}-{nonce}",
            std::process::id()
        ));
        let packages = root.join("AddonPackages");
        fs::create_dir_all(&packages).unwrap();
        let source = packages.join("author.style.1.var");
        let texture_package = packages.join("author.scalp.3.var");
        fs::write(&source, []).unwrap();
        fs::write(&texture_package, []).unwrap();
        let anchor = AssetLocator::VarEntry {
            archive: source,
            entry: "Custom/Hair/Female/style.vab".to_owned(),
        };
        assert_eq!(
            sibling_locator(&anchor, "author.scalp.3:/Custom/Hair/Female/scalp/spec.png"),
            Some(AssetLocator::VarEntry {
                archive: texture_package,
                entry: "Custom/Hair/Female/scalp/spec.png".to_owned(),
            })
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_non_finite_guide_point_is_dropped_not_fatal() {
        let mut bytes = Vec::new();
        write_string(&mut bytes, "DynamicStore");
        write_string(&mut bytes, "1.0");
        bytes.push(1);
        write_string(&mut bytes, "RuntimeHairGeometryCreator");
        write_string(&mut bytes, "1.0");
        write_string(&mut bytes, "provider");
        write_i32(&mut bytes, 2);
        write_f32(&mut bytes, 0.01);
        write_string(&mut bytes, "mask");
        write_i32(&mut bytes, 1);
        bytes.push(1);
        write_i32(&mut bytes, 1);

        write_i32(&mut bytes, 0);
        write_i32(&mut bytes, 3);
        for point in [
            [0.0_f32, 1.0, 0.0],
            [f32::NAN, f32::NAN, f32::NAN],
            [0.0, 1.2, 0.0],
        ] {
            for value in point {
                write_f32(&mut bytes, value);
            }
        }

        write_i32(&mut bytes, 3);
        for index in [0, 0, 0] {
            write_i32(&mut bytes, index);
        }

        write_i32(&mut bytes, 3);
        for _ in 0..3 {
            for value in [f32::NAN, 0.0, 0.0] {
                write_f32(&mut bytes, value);
            }
        }

        write_i32(&mut bytes, 1);
        write_i32(&mut bytes, 0);

        let geometry = parse_hair_vab(&bytes, "nan.vab").unwrap();
        assert_eq!(geometry.guides.len(), 1);

        assert_eq!(geometry.guides[0].points_cm.len(), 2);
        assert!(
            geometry.guides[0]
                .points_cm
                .iter()
                .flatten()
                .all(|c| c.is_finite())
        );
    }

    #[test]
    fn staggered_child_lengths_survive_into_the_optical_settings() {
        let look = HairLookPatch {
            length: [Some(1.0), Some(0.4710246), Some(0.0)],
            ..Default::default()
        };
        let optics = look.optical_settings();
        assert_eq!(optics.child_lengths[0], 1.0);
        assert_eq!(optics.child_lengths[1], 0.4710246);

        // A zeroed tier reaches the shader as zero and draws nothing, which
        // is what the game does with it.
        assert_eq!(optics.child_lengths[2], 0.0);
    }

    #[test]
    fn catalog_keeps_multi_part_vap_as_one_user_facing_preset() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root_path =
            std::env::temp_dir().join(format!("vkit-hair-catalog-{}-{nonce}", std::process::id()));
        let preset_dir = root_path.join("Custom/Atom/Person/Hair");
        let part_dir = root_path.join("Custom/Hair/Female/Test");
        fs::create_dir_all(&preset_dir).unwrap();
        fs::create_dir_all(&part_dir).unwrap();
        for name in ["Front", "Back"] {
            fs::write(part_dir.join(format!("{name}.vam")), b"{}").unwrap();
            fs::write(part_dir.join(format!("{name}.vab")), direct_vab("1.0")).unwrap();
        }
        fs::write(
            part_dir.join("Front.vaj"),
            serde_json::to_vec(&serde_json::json!({
                "storables": [{
                    "id": "frontSim",
                    "drag": "0.25",
                    "gravityMultiplier": "1.75",
                    "iterations": "2"
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        let vap = serde_json::json!({
            "storables": [
                {
                    "id": "geometry",
                    "hair": [
                        {"id": "Custom/Hair/Female/Test/Front.vam", "internalId": "front", "enabled": "true"},
                        {"id": "Custom/Hair/Female/Test/Back.vam", "internalId": "back", "enabled": "true"}
                    ]
                },
                {
                    "id": "frontSim",
                    "width": "0.001",
                    "shaderType": "QualityThicken",
                    "colorRolloff": "1.7",
                    "diffuseSoftness": "0.2",
                    "primarySpecularSharpness": "256",
                    "secondarySpecularSharpness": "128",
                    "specularShift": "-0.04",
                    "fresnelPower": "8",
                    "fresnelAttenuation": "0.2",
                    "randomColorPower": "2.5",
                    "randomColorOffset": "0.3",
                    "IBLFactor": "0.65",
                    "normalRandomize": "0.4",
                    "simulationEnabled": "true",
                    "gravityMultiplier": "-0.5",
                    "iterations": "4",
                    "wind": ["1", "2", "3"]
                },
                {
                    "id": "frontCustomScalpMaterialCombined",
                    "Specular Intensity": "1.25",
                    "Gloss": "3.2",
                    "Specular Fresnel": "0.6",
                    "Alpha Adjust": "-0.1",
                    "Diffuse Color": {"h": "0", "s": "0", "v": "0.8"},
                    "Specular Color": {"h": "0", "s": "0", "v": "0.3"},
                    "customTexture_MainTex": "SELF:/Custom/Hair/face.png",
                    "customTexture_SpecTex": "SELF:/Custom/Hair/spec.png",
                    "customTexture_GlossTex": "SELF:/Custom/Hair/gloss.png",
                    "customTexture_BumpMap": "SELF:/Custom/Hair/normal.png",
                    "customTexture_AlphaTex": "SELF:/Custom/Hair/alpha.png"
                },
                {"id": "backSim", "width": "0.002"}
            ]
        });
        fs::write(
            preset_dir.join("Preset_Full_Set.vap"),
            serde_json::to_vec(&vap).unwrap(),
        )
        .unwrap();

        let root = VaMRoot::open(&root_path).unwrap();
        let presets = scan_hair_presets(&root).unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].label, "Full_Set");
        assert_eq!(presets[0].parts.len(), 2);
        assert_eq!(presets[0].parts[0].preset_look.width_m, Some(0.001));
        let optics = presets[0].parts[0].preset_look.optical_settings();
        assert_eq!(optics.shader_type, HairShaderType::QualityThicken);
        assert_eq!(optics.color_rolloff, 1.7);
        assert_eq!(optics.diffuse_softness, 0.2);
        assert_eq!(optics.primary_specular_sharpness, 256.0);
        assert_eq!(optics.secondary_specular_sharpness, 128.0);
        assert_eq!(optics.specular_shift, -0.04);
        assert_eq!(optics.fresnel_power, 8.0);
        assert_eq!(optics.fresnel_attenuation, 0.2);
        assert_eq!(optics.random_color_power, 2.5);
        assert_eq!(optics.random_color_offset, 0.3);
        assert_eq!(optics.ibl_factor, 0.65);
        assert_eq!(optics.normal_randomize, 0.4);

        assert_eq!(optics.child_lengths, [1.0; 3]);
        let scalp = presets[0].parts[0].preset_look.scalp_material_settings();
        assert_eq!(scalp.specular_intensity, 1.25);
        assert_eq!(scalp.glossiness, 3.2);
        assert_eq!(scalp.specular_fresnel, 0.6);
        assert_eq!(scalp.alpha_adjust, -0.1);
        assert_eq!(scalp.diffuse_color, [0.8; 3]);
        assert_eq!(scalp.specular_color, [0.3; 3]);
        assert!(scalp.roughness() < 0.5);
        assert_eq!(
            presets[0].parts[0].preset_look.scalp_normal.as_deref(),
            Some("SELF:/Custom/Hair/normal.png")
        );
        assert_eq!(
            presets[0].parts[0].preset_physics.gravity_multiplier,
            Some(-0.5)
        );
        assert_eq!(presets[0].parts[0].preset_physics.iterations, Some(4));
        assert_eq!(
            presets[0].parts[0].preset_physics.wind,
            Some([1.0, 2.0, 3.0])
        );
        let (_, physics) = load_hair_part_settings(&presets[0].parts[0]).unwrap();
        assert_eq!(physics.drag, 0.25);
        assert_eq!(physics.gravity_multiplier, -0.5);
        assert_eq!(physics.iterations, 4);
        assert_eq!(physics.wind, [1.0, 2.0, 3.0]);

        fs::remove_dir_all(root_path).unwrap();
    }
}
