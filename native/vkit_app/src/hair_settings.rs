use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum HairParamGroup {
    Performance,
    Physics,
    Stiffness,
    Shape,
    Curl,
    Look,
    Scalp,
}

impl HairParamGroup {
    pub const ALL: [Self; 6] = [
        Self::Performance,
        Self::Physics,
        Self::Stiffness,
        Self::Shape,
        Self::Curl,
        Self::Look,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HairParamKind {
    Color,
    Float { decimals: usize },
    Count,
    Toggle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HairParamTarget {
    Sim,
    ScalpMaterial,
}

#[derive(Clone, Copy, Debug)]
pub struct HairParam {
    pub target: HairParamTarget,
    pub key: &'static str,
    pub group: HairParamGroup,
    pub label: &'static str,
    pub kind: HairParamKind,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

impl HairParam {
    pub fn game_only(&self) -> bool {
        matches!(
            self.key,
            "simulationEnabled"
                | "collisionEnabled"
                | "weight"
                | "drag"
                | "gravityMultiplier"
                | "friction"
                | "bendResistance"
                | "iterations"
                | "cling"
                | "clingRolloff"
                | "snap"
                | "rootRigidity"
                | "mainRigidity"
                | "tipRigidity"
                | "jointRigidity"
                | "rigidityRolloffPower"
        )
    }

    #[must_use]
    pub fn title(&self, locale: crate::i18n::Locale) -> &'static str {
        crate::i18n::hair_param_label(locale, self.key).unwrap_or(self.label)
    }

    #[must_use]
    pub fn hint(&self, locale: crate::i18n::Locale) -> Option<&'static str> {
        crate::i18n::hair_param_hint(locale, self.key)
    }
}

const fn float(
    key: &'static str,
    group: HairParamGroup,
    label: &'static str,
    min: f32,
    max: f32,
    default: f32,
    decimals: usize,
) -> HairParam {
    HairParam {
        target: HairParamTarget::Sim,
        key,
        group,
        label,
        kind: HairParamKind::Float { decimals },
        min,
        max,
        default,
    }
}

const fn count(
    key: &'static str,
    group: HairParamGroup,
    label: &'static str,
    min: f32,
    max: f32,
    default: f32,
) -> HairParam {
    HairParam {
        target: HairParamTarget::Sim,
        key,
        group,
        label,
        kind: HairParamKind::Count,
        min,
        max,
        default,
    }
}

const fn toggle(
    key: &'static str,
    group: HairParamGroup,
    label: &'static str,
    default: bool,
) -> HairParam {
    HairParam {
        target: HairParamTarget::Sim,
        key,
        group,
        label,
        kind: HairParamKind::Toggle,
        min: 0.0,
        max: 1.0,
        default: if default { 1.0 } else { 0.0 },
    }
}

const fn scalp(
    key: &'static str,
    label: &'static str,
    min: f32,
    max: f32,
    default: f32,
    decimals: usize,
) -> HairParam {
    HairParam {
        target: HairParamTarget::ScalpMaterial,
        key,
        group: HairParamGroup::Scalp,
        label,
        kind: HairParamKind::Float { decimals },
        min,
        max,
        default,
    }
}

const fn scalp_color(key: &'static str, label: &'static str, r: u8, g: u8, b: u8) -> HairParam {
    HairParam {
        target: HairParamTarget::ScalpMaterial,
        key,
        group: HairParamGroup::Scalp,
        label,
        kind: HairParamKind::Color,
        min: 0.0,
        max: 255.0,
        default: ((r as u32) << 16 | (g as u32) << 8 | b as u32) as f32,
    }
}

const fn color(key: &'static str, label: &'static str, r: u8, g: u8, b: u8) -> HairParam {
    HairParam {
        target: HairParamTarget::Sim,
        key,
        group: HairParamGroup::Look,
        label,
        kind: HairParamKind::Color,
        min: 0.0,
        max: 255.0,
        default: ((r as u32) << 16 | (g as u32) << 8 | b as u32) as f32,
    }
}

use HairParamGroup::{Curl, Look, Performance, Physics, Shape, Stiffness};

pub const HAIR_PARAMS: [HairParam; 65] = [
    count(
        "hairMultiplier",
        Performance,
        "Hair Multiplier",
        1.0,
        64.0,
        15.0,
    ),
    count(
        "curveDensity",
        Performance,
        "Curve Density",
        2.0,
        64.0,
        12.0,
    ),
    count("iterations", Performance, "Iterations", 1.0, 5.0, 2.0),
    toggle("simulationEnabled", Physics, "Physics", true),
    toggle("collisionEnabled", Physics, "Collision", true),
    float("weight", Physics, "Weight", 0.0, 2.0, 1.0, 3),
    float("drag", Physics, "Drag", 0.0, 1.0, 0.2, 3),
    float(
        "gravityMultiplier",
        Physics,
        "Gravity Multiplier",
        -2.0,
        2.0,
        1.0,
        3,
    ),
    float(
        "collisionRadius",
        Physics,
        "Collision Radius",
        0.001,
        0.1,
        0.006,
        4,
    ),
    float(
        "collisionRadiusRoot",
        Physics,
        "Collision Radius Root",
        0.001,
        0.1,
        0.001,
        4,
    ),
    float("friction", Physics, "Friction", 0.0, 1.0, 0.2, 3),
    float(
        "bendResistance",
        Physics,
        "Bend Resistance",
        0.0,
        1.0,
        0.0,
        3,
    ),
    float(
        "rootRigidity",
        Stiffness,
        "Style Rigidity Root",
        0.0,
        1.0,
        0.3,
        3,
    ),
    float(
        "mainRigidity",
        Stiffness,
        "Style Rigidity Main",
        0.0,
        1.0,
        0.1,
        3,
    ),
    float(
        "tipRigidity",
        Stiffness,
        "Style Rigidity Tip",
        0.0,
        1.0,
        0.025,
        3,
    ),
    float(
        "jointRigidity",
        Stiffness,
        "Joint Rigidity",
        0.0,
        1.0,
        0.5,
        3,
    ),
    float(
        "rigidityRolloffPower",
        Stiffness,
        "Style Rigidity Rolloff",
        0.0,
        16.0,
        6.0,
        2,
    ),
    float("cling", Stiffness, "Style Cling", 0.0, 1.0, 0.5, 3),
    float(
        "clingRolloff",
        Stiffness,
        "Style Cling Rolloff",
        0.0,
        1.0,
        1.0,
        3,
    ),
    float("snap", Stiffness, "Snap", 0.0, 1.0, 0.2, 3),
    toggle(
        "usePaintedRigidity",
        Stiffness,
        "Use Painted Rigidity",
        false,
    ),
    float("width", Shape, "Width", 0.0, 0.001, 0.0001, 5),
    float("length1", Shape, "Length 1", 0.0, 1.0, 1.0, 3),
    float("length2", Shape, "Length 2", 0.0, 1.0, 1.0, 3),
    float("length3", Shape, "Length 3", 0.0, 1.0, 0.95, 3),
    float("maxSpread", Shape, "Max Spread", 0.0, 0.5, 0.025, 4),
    float("spreadRoot", Shape, "Spread Root", 0.0, 1.0, 1.0, 3),
    float("spreadMid", Shape, "Spread Mid", 0.0, 1.0, 1.0, 3),
    float("spreadTip", Shape, "Spread Tip", 0.0, 1.0, 0.7, 3),
    float("spreadMidpoint", Shape, "Spread Midpoint", 0.0, 1.0, 0.5, 3),
    float(
        "spreadCurvePower",
        Shape,
        "Spread Curve Power",
        0.0,
        16.0,
        1.0,
        3,
    ),
    float("curlScale", Curl, "Curl Scale", 0.0, 1.0, 0.0, 3),
    float("curlFrequency", Curl, "Curl Frequency", 0.0, 20.0, 0.2, 3),
    float(
        "curlScaleRandomness",
        Curl,
        "Curl Scale Random",
        0.0,
        2.0,
        1.0,
        3,
    ),
    float(
        "curlFrequencyRandomness",
        Curl,
        "Curl Freq Random",
        0.0,
        2.0,
        0.0,
        3,
    ),
    float("curlRoot", Curl, "Curl Root", 0.0, 1.0, 0.0, 3),
    float("curlMid", Curl, "Curl Mid", 0.0, 1.0, 0.6, 3),
    float("curlTip", Curl, "Curl Tip", 0.0, 1.0, 1.0, 3),
    float("curlMidpoint", Curl, "Curl Midpoint", 0.0, 1.0, 0.5, 3),
    float(
        "curlCurvePower",
        Curl,
        "Curl Curve Power",
        0.0,
        16.0,
        1.0,
        3,
    ),
    float("curlX", Curl, "Curl X", 0.0, 1.0, 0.05, 3),
    float("curlY", Curl, "Curl Y", 0.0, 1.0, 0.04, 3),
    float("curlZ", Curl, "Curl Z", 0.0, 1.0, 0.06, 3),
    float(
        "curlNormalAdjust",
        Curl,
        "Curl Normal Adjust",
        -0.5,
        0.5,
        0.0,
        3,
    ),
    toggle("curlAllowReverse", Curl, "Allow Reverse", false),
    toggle("curlAllowFlipAxis", Curl, "Allow Axis Flip", false),
    scalp("Alpha Adjust", "Scalp Opacity", -1.0, 1.0, -1.0, 3),
    scalp("Gloss", "Scalp Gloss", 0.0, 10.0, 2.0, 2),
    scalp("Specular Intensity", "Scalp Specular", 0.0, 4.0, 0.0, 2),
    scalp(
        "Diffuse Texture Offset",
        "Scalp Texture Offset",
        -1.0,
        3.0,
        0.0,
        2,
    ),
    scalp_color("Diffuse Color", "Scalp Color", 255, 255, 255),
    color("rootColor", "Root Color", 0, 0, 0),
    color("tipColor", "Tip Color", 19, 17, 15),
    color("specularColor", "Specular Color", 99, 66, 58),
    float(
        "colorRolloff",
        Look,
        "Root->Tip Color Rolloff",
        0.0,
        5.0,
        2.0,
        3,
    ),
    float(
        "randomColorPower",
        Look,
        "Random Color Power",
        0.0,
        10.0,
        2.0,
        3,
    ),
    float(
        "randomColorOffset",
        Look,
        "Random Color Offset",
        0.0,
        1.0,
        0.3,
        3,
    ),
    float(
        "primarySpecularSharpness",
        Look,
        "Primary Specular Gloss",
        2.0,
        256.0,
        160.0,
        1,
    ),
    float(
        "secondarySpecularSharpness",
        Look,
        "Secondary Specular Gloss",
        2.0,
        256.0,
        64.0,
        1,
    ),
    float(
        "specularShift",
        Look,
        "Secondary Specular Shift",
        0.0,
        1.0,
        0.4,
        3,
    ),
    float(
        "diffuseSoftness",
        Look,
        "Diffuse Softness",
        0.0,
        1.0,
        0.1,
        3,
    ),
    float("fresnelPower", Look, "Fresnel Power", 0.0, 10.0, 8.0, 3),
    float(
        "fresnelAttenuation",
        Look,
        "Fresnel Attenuation",
        0.0,
        1.0,
        0.2,
        3,
    ),
    float(
        "IBLFactor",
        Look,
        "Global Illumination Factor",
        0.0,
        1.0,
        0.5,
        3,
    ),
    float(
        "normalRandomize",
        Look,
        "Normal Randomize",
        0.0,
        1.0,
        0.0,
        3,
    ),
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HairSettings {
    overrides: BTreeMap<&'static str, f32>,
    stock: Option<&'static ScalpStock>,
}

pub const COLOR_CHANNELS: [&str; 3] = ["r", "g", "b"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScalpStock {
    pub provider_name: &'static str,
    pub scalars: &'static [(&'static str, f32)],
    pub colors: &'static [(&'static str, [u8; 3])],
}

pub const SCALP_STOCK: [ScalpStock; 5] = [
    ScalpStock {
        provider_name: "UdaneScalp",
        scalars: &[("Gloss", 3.2), ("Global Illumination Filter", 0.5)],
        colors: &[
            ("Diffuse Color", [33, 25, 25]),
            ("Specular Color", [33, 25, 25]),
        ],
    },
    ScalpStock {
        provider_name: "KrayonScalp",
        scalars: &[("Gloss", 2.0), ("Global Illumination Filter", 0.7)],
        colors: &[
            ("Diffuse Color", [209, 209, 209]),
            ("Specular Color", [255, 255, 255]),
        ],
    },
    ScalpStock {
        provider_name: "SoleilScalp",
        scalars: &[
            ("Gloss", 2.0),
            ("Global Illumination Filter", 0.7),
            ("Specular Intensity", 2.5),
        ],
        colors: &[],
    },
    ScalpStock {
        provider_name: "LeytonScalp",
        scalars: &[("Gloss", 2.0), ("Global Illumination Filter", 0.7)],
        colors: &[],
    },
    ScalpStock {
        provider_name: "PantyRegionScalp",
        scalars: &[("Gloss", 6.0), ("Global Illumination Filter", 0.0)],
        colors: &[
            ("Diffuse Color", [255, 255, 255]),
            ("Specular Color", [255, 255, 255]),
        ],
    },
];

#[must_use]
pub fn scalp_stock(provider_name: &str) -> Option<&'static ScalpStock> {
    SCALP_STOCK
        .iter()
        .find(|stock| stock.provider_name == provider_name)
}

impl HairSettings {
    #[must_use]
    pub fn stock_of(&self, param: &HairParam) -> f32 {
        self.stock
            .and_then(|stock| {
                stock
                    .scalars
                    .iter()
                    .find(|(key, _)| *key == param.key)
                    .map(|(_, value)| *value)
            })
            .unwrap_or(param.default)
    }

    #[must_use]
    pub fn stock_channel(&self, param: &HairParam, channel: usize) -> f32 {
        if let Some(rgb) = self.stock.and_then(|stock| {
            stock
                .colors
                .iter()
                .find(|(key, _)| *key == param.key)
                .map(|(_, rgb)| *rgb)
        }) {
            return f32::from(rgb[channel.min(2)]);
        }
        let packed = param.default as u32;
        f32::from(((packed >> (16 - 8 * channel)) & 0xff) as u8)
    }

    pub fn wear(&mut self, provider_name: &str) {
        self.stock = scalp_stock(provider_name);
    }

    pub fn color_channel(&self, param: &HairParam, channel: usize) -> f32 {
        self.overrides
            .get(color_key(param.key, channel))
            .copied()
            .unwrap_or_else(|| self.stock_channel(param, channel))
    }

    pub fn set_color_channel(&mut self, param: &HairParam, channel: usize, value: f32) {
        if !value.is_finite() || channel >= COLOR_CHANNELS.len() {
            return;
        }
        let key = color_key(param.key, channel);
        let value = value.clamp(0.0, 255.0).round();
        let default = self.stock_channel(param, channel);
        if (value - default).abs() < f32::EPSILON {
            self.overrides.remove(key);
        } else {
            self.overrides.insert(key, value);
        }
    }

    pub fn color_hsv(&self, param: &HairParam) -> [f32; 3] {
        let rgb = [
            self.color_channel(param, 0) / 255.0,
            self.color_channel(param, 1) / 255.0,
            self.color_channel(param, 2) / 255.0,
        ];
        rgb_to_hsv(rgb)
    }

    pub fn get(&self, param: &HairParam) -> f32 {
        self.overrides
            .get(param.key)
            .copied()
            .unwrap_or_else(|| self.stock_of(param))
    }

    pub fn set(&mut self, param: &HairParam, value: f32) {
        if !value.is_finite() {
            return;
        }
        let value = value.clamp(param.min, param.max);
        if (value - self.stock_of(param)).abs() < f32::EPSILON {
            self.overrides.remove(param.key);
        } else {
            self.overrides.insert(param.key, value);
        }
    }

    #[must_use]
    pub fn for_base_part() -> Self {
        let mut settings = Self::default();
        if let Some(param) = param_by_key(SCALP_OPACITY_KEY) {
            settings.set(param, param.max);
        }
        if let Some(cap) = param_by_key(SCALP_COLOR_KEY) {
            for channel in 0..COLOR_CHANNELS.len() {
                settings.set_color_channel(cap, channel, VISIBLE_SCALP_COLOR[channel.min(2)]);
            }
        }
        settings
    }

    pub fn absorb_storables(
        &mut self,
        sim: Option<&serde_json::Value>,
        scalp_material: Option<&serde_json::Value>,
    ) {
        for param in &HAIR_PARAMS {
            let source = match param.target {
                HairParamTarget::Sim => sim,
                HairParamTarget::ScalpMaterial => scalp_material,
            };
            let Some(entry) = source.and_then(|storable| storable.get(param.key)) else {
                continue;
            };
            match param.kind {
                HairParamKind::Color => {
                    let Some(rgb) = hsv_entry_to_rgb(entry) else {
                        continue;
                    };
                    for (channel, value) in rgb.into_iter().enumerate() {
                        self.set_color_channel(param, channel, value);
                    }
                }
                HairParamKind::Toggle => {
                    let Some(on) = storable_bool(entry) else {
                        continue;
                    };
                    self.set(param, f32::from(u8::from(on)));
                }
                HairParamKind::Count | HairParamKind::Float { .. } => {
                    let Some(value) = storable_number(entry) else {
                        continue;
                    };
                    self.set(param, value);
                }
            }
        }
    }

    #[must_use]
    pub fn shows_scalp_cap(&self) -> bool {
        param_by_key(SCALP_OPACITY_KEY).is_some_and(|param| self.get(param) > param.min)
    }

    pub fn carry_scalp_from(&mut self, source: &Self) {
        self.reset_group(HairParamGroup::Scalp);
        for param in HAIR_PARAMS
            .iter()
            .filter(|param| param.group == HairParamGroup::Scalp)
        {
            if param.kind == HairParamKind::Color {
                for channel in 0..COLOR_CHANNELS.len() {
                    let key = color_key(param.key, channel);
                    if let Some(value) = source.overrides.get(key) {
                        self.overrides.insert(key, *value);
                    }
                }
            } else if let Some(value) = source.overrides.get(param.key) {
                self.overrides.insert(param.key, *value);
            }
        }
    }

    pub fn hide_scalp_cap(&mut self) {
        if let Some(param) = param_by_key(SCALP_OPACITY_KEY) {
            self.set(param, self.stock_of(param));
        }
    }

    pub fn is_default(&self, param: &HairParam) -> bool {
        !self.overrides.contains_key(param.key)
    }

    pub fn reset_group(&mut self, group: HairParamGroup) {
        for param in HAIR_PARAMS.iter().filter(|param| param.group == group) {
            self.overrides.remove(param.key);
            if param.kind == HairParamKind::Color {
                for channel in 0..COLOR_CHANNELS.len() {
                    self.overrides.remove(color_key(param.key, channel));
                }
            }
        }
    }

    pub fn is_color_default(&self, param: &HairParam) -> bool {
        (0..COLOR_CHANNELS.len())
            .all(|channel| !self.overrides.contains_key(color_key(param.key, channel)))
    }

    pub fn color_entries(&self) -> Vec<(&'static str, [f32; 3])> {
        HAIR_PARAMS
            .iter()
            .filter(|param| {
                param.kind == HairParamKind::Color && param.target == HairParamTarget::Sim
            })
            .map(|param| (param.key, self.color_hsv(param)))
            .collect()
    }

    pub fn storable_value(&self, param: &HairParam) -> String {
        let value = self.get(param);
        match param.kind {
            HairParamKind::Color => String::new(),
            HairParamKind::Toggle => if value >= 0.5 { "true" } else { "false" }.to_owned(),
            HairParamKind::Count => format!("{}", value.round() as i64),
            HairParamKind::Float { decimals } => {
                let text = format!("{value:.decimals$}");
                let text = text.trim_end_matches('0').trim_end_matches('.');
                if text.is_empty() || text == "-" {
                    "0".to_owned()
                } else {
                    text.to_owned()
                }
            }
        }
    }

    pub fn storable_entries(&self) -> Vec<(&'static str, String)> {
        HAIR_PARAMS
            .iter()
            .filter(|param| {
                param.kind != HairParamKind::Color && param.target == HairParamTarget::Sim
            })
            .map(|param| (param.key, self.storable_value(param)))
            .collect()
    }
}

fn storable_number(entry: &serde_json::Value) -> Option<f32> {
    entry.as_f64().map_or_else(
        || {
            entry
                .as_str()
                .and_then(|text| text.trim().parse::<f32>().ok())
        },
        |value| Some(value as f32),
    )
}

fn storable_bool(entry: &serde_json::Value) -> Option<bool> {
    entry.as_bool().or_else(|| match entry.as_str()?.trim() {
        "true" | "True" | "1" => Some(true),
        "false" | "False" | "0" => Some(false),
        _ => None,
    })
}

fn hsv_entry_to_rgb(entry: &serde_json::Value) -> Option<[f32; 3]> {
    let channel = |name: &str| entry.get(name).and_then(storable_number);
    let hsv = [channel("h")?, channel("s")?, channel("v")?];
    let rgb = hsv_to_rgb(hsv);
    Some([rgb[0] * 255.0, rgb[1] * 255.0, rgb[2] * 255.0])
}

fn color_key(key: &'static str, channel: usize) -> &'static str {
    static KEYS: std::sync::OnceLock<std::sync::Mutex<Vec<&'static str>>> =
        std::sync::OnceLock::new();
    let wanted = format!("{key}.{}", COLOR_CHANNELS[channel]);
    let table = KEYS.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    let mut table = table.lock().expect("colour key table");
    if let Some(found) = table.iter().find(|entry| **entry == wanted) {
        return found;
    }
    let leaked: &'static str = Box::leak(wanted.into_boxed_str());
    table.push(leaked);
    leaked
}

pub fn hsv_to_rgb(hsv: [f32; 3]) -> [f32; 3] {
    let [h, s, v] = hsv;
    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s.clamp(0.0, 1.0);
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

pub fn rgb_to_hsv(rgb: [f32; 3]) -> [f32; 3] {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let delta = max - min;
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if (max - rgb[0]).abs() < f32::EPSILON {
        ((rgb[1] - rgb[2]) / delta).rem_euclid(6.0)
    } else if (max - rgb[1]).abs() < f32::EPSILON {
        (rgb[2] - rgb[0]) / delta + 2.0
    } else {
        (rgb[0] - rgb[1]) / delta + 4.0
    } / 6.0;
    let saturation = if max <= f32::EPSILON {
        0.0
    } else {
        delta / max
    };
    [hue, saturation, max]
}

impl HairSettings {
    pub fn scalp_material_entries(&self) -> Vec<(&'static str, String)> {
        HAIR_PARAMS
            .iter()
            .filter(|param| {
                param.target == HairParamTarget::ScalpMaterial && param.kind != HairParamKind::Color
            })
            .map(|param| (param.key, self.storable_value(param)))
            .collect()
    }
}

pub const SCALP_OPACITY_KEY: &str = "Alpha Adjust";

pub const SCALP_COLOR_KEY: &str = "Diffuse Color";

pub const VISIBLE_SCALP_COLOR: [f32; 3] = [108.0, 74.0, 44.0];

#[must_use]
pub fn param_by_key(key: &str) -> Option<&'static HairParam> {
    HAIR_PARAMS.iter().find(|param| param.key == key)
}

pub fn params_in(group: HairParamGroup) -> impl Iterator<Item = &'static HairParam> {
    HAIR_PARAMS.iter().filter(move |param| param.group == group)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_range_matches_the_storable_vam_registers() {
        const VAM: &[(&str, f32, f32)] = &[
            ("collisionRadius", 0.001, 0.1),
            ("collisionRadiusRoot", 0.001, 0.1),
            ("drag", 0.0, 1.0),
            ("rootRigidity", 0.0, 1.0),
            ("mainRigidity", 0.0, 1.0),
            ("tipRigidity", 0.0, 1.0),
            ("rigidityRolloffPower", 0.0, 16.0),
            ("jointRigidity", 0.0, 1.0),
            ("friction", 0.0, 1.0),
            ("gravityMultiplier", -2.0, 2.0),
            ("weight", 0.0, 2.0),
            ("iterations", 1.0, 5.0),
            ("cling", 0.0, 1.0),
            ("clingRolloff", 0.0, 1.0),
            ("snap", 0.0, 1.0),
            ("bendResistance", 0.0, 1.0),
            ("colorRolloff", 0.0, 5.0),
            ("diffuseSoftness", 0.0, 1.0),
            ("primarySpecularSharpness", 2.0, 256.0),
            ("secondarySpecularSharpness", 2.0, 256.0),
            ("specularShift", 0.0, 1.0),
            ("fresnelPower", 0.0, 10.0),
            ("fresnelAttenuation", 0.0, 1.0),
            ("randomColorPower", 0.0, 10.0),
            ("randomColorOffset", 0.0, 1.0),
            ("IBLFactor", 0.0, 1.0),
            ("normalRandomize", 0.0, 1.0),
            ("curlX", 0.0, 1.0),
            ("curlY", 0.0, 1.0),
            ("curlZ", 0.0, 1.0),
            ("curlScale", 0.0, 1.0),
            ("curlScaleRandomness", 0.0, 2.0),
            ("curlFrequency", 0.0, 20.0),
            ("curlFrequencyRandomness", 0.0, 2.0),
            ("curlNormalAdjust", -0.5, 0.5),
            ("curlRoot", 0.0, 1.0),
            ("curlMid", 0.0, 1.0),
            ("curlTip", 0.0, 1.0),
            ("curlMidpoint", 0.0, 1.0),
            ("curlCurvePower", 0.0, 16.0),
            ("length1", 0.0, 1.0),
            ("length2", 0.0, 1.0),
            ("length3", 0.0, 1.0),
            ("width", 0.0, 0.001),
            ("curveDensity", 2.0, 64.0),
            ("hairMultiplier", 1.0, 64.0),
            ("maxSpread", 0.0, 0.5),
            ("spreadRoot", 0.0, 1.0),
            ("spreadMid", 0.0, 1.0),
            ("spreadTip", 0.0, 1.0),
            ("spreadMidpoint", 0.0, 1.0),
            ("spreadCurvePower", 0.0, 16.0),
        ];
        let mut wrong = Vec::new();
        for (key, low, high) in VAM {
            let Some(param) = HAIR_PARAMS.iter().find(|param| param.key == *key) else {
                wrong.push(format!("{key}: missing from HAIR_PARAMS"));
                continue;
            };
            let (ours_low, ours_high) = (param.min, param.max);
            if (ours_low - low).abs() > 1.0e-9 || (ours_high - high).abs() > 1.0e-9 {
                wrong.push(format!(
                    "{key}: ours {ours_low}..{ours_high}, VaM {low}..{high}",
                ));
            }
        }
        assert!(
            wrong.is_empty(),
            "ranges VaM would clamp:
  {}",
            wrong.join(
                "
  "
            )
        );
    }

    #[test]
    fn every_parameter_names_a_storable_once_and_defaults_inside_its_own_range() {
        let mut seen = std::collections::HashSet::new();
        for param in &HAIR_PARAMS {
            assert!(seen.insert(param.key), "duplicate storable {}", param.key);
            assert!(
                param.min <= param.max,
                "{} has an inverted range",
                param.key
            );
            if param.kind == HairParamKind::Color {
                for channel in 0..COLOR_CHANNELS.len() {
                    let value = HairSettings::default().color_channel(param, channel);
                    assert!(
                        (0.0..=255.0).contains(&value),
                        "{} channel {channel} defaults to {value}",
                        param.key,
                    );
                }
                continue;
            }
            assert!(
                (param.min..=param.max).contains(&param.default),
                "{} defaults to {} outside {}..={}",
                param.key,
                param.default,
                param.min,
                param.max,
            );
        }
    }

    #[test]
    fn a_value_put_back_to_its_default_stops_being_an_override() {
        let param = HAIR_PARAMS
            .iter()
            .find(|param| param.key == "hairMultiplier")
            .unwrap();
        let mut settings = HairSettings::default();
        assert!(settings.is_default(param));
        settings.set(param, 40.0);
        assert!(!settings.is_default(param));
        assert_eq!(settings.get(param), 40.0);
        settings.set(param, param.default);
        assert!(settings.is_default(param));
    }

    #[test]
    fn values_are_clamped_and_non_finite_input_is_refused() {
        let param = HAIR_PARAMS
            .iter()
            .find(|param| param.key == "hairMultiplier")
            .unwrap();
        let mut settings = HairSettings::default();
        settings.set(param, 1000.0);
        assert_eq!(settings.get(param), 64.0);
        settings.set(param, f32::NAN);
        assert_eq!(settings.get(param), 64.0);
    }

    #[test]
    fn the_curl_defaults_leave_curl_reachable_from_its_own_slider() {
        let settings = HairSettings::default();
        let value = |key: &str| {
            let param = HAIR_PARAMS.iter().find(|param| param.key == key).unwrap();
            settings.get(param)
        };

        assert_eq!(value("curlScale"), 0.0, "hair should start straight");

        assert!(value("curlMid") > 0.0, "the envelope is dead at the middle");
        assert!(value("curlTip") > 0.0, "the envelope is dead at the tip");
        assert!(value("curlFrequency") > 0.0, "no turns to curl through");
        let curl = [value("curlX"), value("curlY"), value("curlZ")];
        assert!(
            curl.iter().any(|component| component.abs() > 0.0),
            "no radius to curl by: {curl:?}",
        );
        assert_eq!(value("curlRoot"), 0.0);
    }

    #[test]
    fn storables_are_written_the_way_vam_writes_them() {
        let settings = HairSettings::default();
        let entries: BTreeMap<_, _> = settings.storable_entries().into_iter().collect();
        assert_eq!(entries["hairMultiplier"], "15");
        assert_eq!(entries["iterations"], "2");
        assert_eq!(entries["simulationEnabled"], "true");
        assert_eq!(entries["usePaintedRigidity"], "false");
        assert_eq!(entries["weight"], "1");
        assert_eq!(entries["tipRigidity"], "0.025");
        assert_eq!(entries["maxSpread"], "0.025");
        assert_eq!(entries["width"], "0.0001");
    }
}

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn a_colour_edits_in_rgb_and_stores_as_the_hsv_vam_reads() {
        let param = HAIR_PARAMS
            .iter()
            .find(|param| param.key == "tipColor")
            .unwrap();
        let mut settings = HairSettings::default();
        assert_eq!(settings.color_channel(param, 0), 19.0);
        assert!(settings.is_color_default(param));

        settings.set_color_channel(param, 0, 255.0);
        settings.set_color_channel(param, 1, 0.0);
        settings.set_color_channel(param, 2, 0.0);
        assert!(!settings.is_color_default(param));
        let hsv = settings.color_hsv(param);
        assert!(hsv[0].abs() < 1e-4, "hue {hsv:?}");
        assert!((hsv[1] - 1.0).abs() < 1e-4, "saturation {hsv:?}");
        assert!((hsv[2] - 1.0).abs() < 1e-4, "value {hsv:?}");

        settings.set_color_channel(param, 0, 0.0);
        settings.set_color_channel(param, 1, 255.0);
        let hsv = settings.color_hsv(param);
        assert!((hsv[0] - 1.0 / 3.0).abs() < 1e-4, "hue {hsv:?}");

        settings.set_color_channel(param, 1, 0.0);
        let hsv = settings.color_hsv(param);
        assert_eq!(hsv, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn resetting_the_look_group_returns_every_colour_channel() {
        let param = HAIR_PARAMS
            .iter()
            .find(|param| param.key == "rootColor")
            .unwrap();
        let mut settings = HairSettings::default();
        settings.set_color_channel(param, 2, 200.0);
        assert!(!settings.is_color_default(param));
        settings.reset_group(HairParamGroup::Look);
        assert!(settings.is_color_default(param));
        assert_eq!(settings.color_channel(param, 2), 0.0);
    }

    #[test]
    fn every_parameter_is_named_and_explained_in_every_language() {
        for locale in crate::i18n::Locale::ALL {
            if locale == crate::i18n::Locale::English {
                continue;
            }
            for param in &HAIR_PARAMS {
                assert!(
                    crate::i18n::hair_param_label(locale, param.key).is_some(),
                    "{locale:?} has no name for {}",
                    param.key,
                );
                assert!(
                    crate::i18n::hair_param_hint(locale, param.key).is_some(),
                    "{locale:?} has no explanation for {}",
                    param.key,
                );
            }
        }

        for param in &HAIR_PARAMS {
            assert_eq!(
                param.title(crate::i18n::Locale::English),
                param.label,
                "{} must keep VaM's wording in English",
                param.key,
            );
            assert!(
                param.hint(crate::i18n::Locale::English).is_some(),
                "{} has no English explanation, so no language has one",
                param.key,
            );
        }
    }

    #[test]
    fn settings_survive_the_trip_out_to_storables_and_back() {
        let mut written = HairSettings::default();
        let mut expected: Vec<(&HairParam, f32)> = Vec::new();
        for param in &HAIR_PARAMS {
            let value = match param.kind {
                HairParamKind::Color => continue,
                HairParamKind::Toggle => {
                    if param.default >= 0.5 {
                        0.0
                    } else {
                        1.0
                    }
                }
                HairParamKind::Count => (param.default + 1.0).min(param.max),
                HairParamKind::Float { .. } => {
                    let span = param.max - param.min;
                    (param.min + span * 0.37).clamp(param.min, param.max)
                }
            };
            written.set(param, value);
            expected.push((param, written.get(param)));
        }

        let mut sim = serde_json::Map::new();
        let mut scalp = serde_json::Map::new();
        for param in &HAIR_PARAMS {
            if matches!(param.kind, HairParamKind::Color) {
                continue;
            }
            let slot = match param.target {
                HairParamTarget::Sim => &mut sim,
                HairParamTarget::ScalpMaterial => &mut scalp,
            };
            slot.insert(
                param.key.to_owned(),
                serde_json::Value::String(written.storable_value(param)),
            );
        }

        let mut read = HairSettings::default();
        read.absorb_storables(
            Some(&serde_json::Value::Object(sim)),
            Some(&serde_json::Value::Object(scalp)),
        );
        for (param, value) in expected {
            let back = read.get(param);
            assert!(
                (back - value).abs() <= 10.0_f32.powi(-(decimals_of(param) as i32)) + f32::EPSILON,
                "{} went out as {value} and came back as {back}",
                param.key
            );
        }
    }

    const fn decimals_of(param: &HairParam) -> usize {
        match param.kind {
            HairParamKind::Float { decimals } => decimals,
            _ => 0,
        }
    }

    #[test]
    fn a_colour_survives_the_hsv_capsule_it_is_written_as() {
        let cap = param_by_key(SCALP_COLOR_KEY).expect("the cap has a colour");
        let mut written = HairSettings::default();
        for (channel, value) in [37.0_f32, 154.0, 219.0].into_iter().enumerate() {
            written.set_color_channel(cap, channel, value);
        }
        let hsv = written.color_hsv(cap);
        let entry = serde_json::json!({
            "h": format!("{:.5}", hsv[0]),
            "s": format!("{:.5}", hsv[1]),
            "v": format!("{:.5}", hsv[2]),
        });
        let mut slot = serde_json::Map::new();
        slot.insert(cap.key.to_owned(), entry);

        let mut read = HairSettings::default();
        read.absorb_storables(None, Some(&serde_json::Value::Object(slot)));
        for (channel, value) in [37.0_f32, 154.0, 219.0].into_iter().enumerate() {
            let back = read.color_channel(cap, channel);
            assert!(
                (back - value).abs() <= 1.0,
                "channel {channel} went out as {value} and came back as {back}"
            );
        }
    }

    #[test]
    fn each_scalp_mesh_answers_with_the_values_vam_ships_it_with() {
        let gloss = param_by_key("Gloss").expect("gloss");
        let specular = param_by_key("Specular Intensity").expect("specular");

        let mut udane = HairSettings::default();
        udane.wear("UdaneScalp");
        assert_eq!(
            udane.get(gloss),
            3.2,
            "every one of the 304 Udane scalps in a full library ships 3.2",
        );

        let mut soleil = HairSettings::default();
        soleil.wear("SoleilScalp");
        assert_eq!(soleil.get(gloss), 2.0);
        assert_eq!(
            soleil.get(specular),
            2.5,
            "Soleil stands at 2.5, which the slider used to stop short of",
        );
        assert!(
            specular.max >= 2.5,
            "a range that cannot reach a stock value silently rewrites it",
        );

        let mut pubic = HairSettings::default();
        pubic.wear("PantyRegionScalp");
        assert_eq!(pubic.get(gloss), 6.0);

        assert_eq!(
            HairSettings::default().get(gloss),
            gloss.default,
            "a layer wearing no known mesh keeps the table value",
        );
    }

    #[test]
    fn a_value_equal_to_the_meshs_own_stock_is_not_recorded_as_a_change() {
        let gloss = param_by_key("Gloss").expect("gloss");
        let mut settings = HairSettings::default();
        settings.wear("UdaneScalp");

        settings.set(gloss, 3.2);
        assert!(
            settings.is_default(gloss),
            "typing the stock value back must clear the override, not store it",
        );
        settings.set(gloss, 2.0);
        assert!(
            !settings.is_default(gloss),
            "2.0 is the table value but not Udane s, so it is a real change",
        );
    }
}
