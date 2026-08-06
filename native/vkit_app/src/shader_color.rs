macro_rules! color_grading_common_wgsl {
    () => {
        r#"
fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
    let low = color / 12.92;
    let high = pow((color + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(high, low, color <= vec3<f32>(0.04045));
}

fn linear_to_srgb(color: vec3<f32>) -> vec3<f32> {
    let low = color * 12.92;
    let high = 1.055 * pow(color, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(high, low, color <= vec3<f32>(0.0031308));
}

fn sanitize_radiance(color: vec3<f32>) -> vec3<f32> {
    let finite_color = select(
        color,
        vec3<f32>(0.0),
        (color != color) | (abs(color) > vec3<f32>(1.0e20)),
    );
    return max(finite_color, vec3<f32>(0.0));
}

const REINHARD_MIDDLE_GREY_EXPOSURE: f32 = 1.2195122;
const FILMIC_MIDDLE_GREY_EXPOSURE: f32 = 0.723171;

fn tone_curve_reinhard(exposed: vec3<f32>) -> vec3<f32> {
    return exposed / (vec3<f32>(1.0) + exposed);
}

fn tone_curve_filmic(exposed: vec3<f32>) -> vec3<f32> {
    let numerator = exposed * (2.51 * exposed + vec3<f32>(0.03));
    let denominator = exposed * (2.43 * exposed + vec3<f32>(0.59)) + vec3<f32>(0.14);
    return clamp(numerator / denominator, vec3<f32>(0.0), vec3<f32>(1.0));
}

"#
    };
}

macro_rules! color_grading_wgsl {
    () => {
        concat!(
            crate::shader_color::color_grading_common_wgsl!(),
            r#"
fn graded_display(
    linear_color: vec3<f32>,
    exposure: f32,
    filmic: bool,
    target_is_srgb: bool,
) -> vec3<f32> {
    let safe = sanitize_radiance(linear_color);
    let calibration = select(
        REINHARD_MIDDLE_GREY_EXPOSURE,
        FILMIC_MIDDLE_GREY_EXPOSURE,
        filmic,
    );
    let exposed = safe * max(exposure, 0.0) * calibration;
    let mapped = select(
        tone_curve_reinhard(exposed),
        tone_curve_filmic(exposed),
        filmic,
    );
    if (target_is_srgb) { return mapped; }
    return linear_to_srgb(mapped);
}
"#
        )
    };
}

macro_rules! color_grading_hdr_wgsl {
    () => {
        concat!(
            crate::shader_color::color_grading_common_wgsl!(),
            r#"
fn graded_display(
    linear_color: vec3<f32>,
    exposure: f32,
    filmic: bool,
    target_is_srgb: bool,
) -> vec3<f32> {

    _ = exposure;
    _ = filmic;
    _ = target_is_srgb;
    return sanitize_radiance(linear_color);
}
"#
        )
    };
}

pub(crate) use {color_grading_common_wgsl, color_grading_hdr_wgsl, color_grading_wgsl};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ToneMapping {
    #[default]
    Filmic,

    Soft,
}

impl ToneMapping {
    pub const ALL: [Self; 2] = [Self::Filmic, Self::Soft];

    pub const fn shader_flag(self) -> f32 {
        match self {
            Self::Filmic => 1.0,
            Self::Soft => 0.0,
        }
    }

    pub const fn id(self) -> u8 {
        match self {
            Self::Filmic => 0,
            Self::Soft => 1,
        }
    }

    pub const fn from_id(id: u8) -> Self {
        match id {
            1 => Self::Soft,
            _ => Self::Filmic,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preamble_defines_what_every_shader_calls() {
        let preamble = color_grading_wgsl!();
        for symbol in [
            "fn srgb_to_linear(",
            "fn linear_to_srgb(",
            "fn sanitize_radiance(",
            "fn tone_curve_reinhard(",
            "fn tone_curve_filmic(",
            "fn graded_display(",
        ] {
            assert!(preamble.contains(symbol), "preamble lost {symbol}");
            assert!(
                color_grading_hdr_wgsl!().contains(symbol),
                "off-screen preamble lost {symbol}"
            );
        }

        assert!(preamble.contains("tone_curve_filmic(exposed)"));
        assert!(!color_grading_hdr_wgsl!().contains("tone_curve_filmic(exposed)"));
    }

    #[test]
    fn the_two_curves_agree_on_a_neutral_eighteen_percent_surface() {
        const REINHARD: f64 = 1.219_512_2;
        const FILMIC: f64 = 0.723_171;
        let reinhard = |x: f64| x / (1.0 + x);
        let filmic =
            |x: f64| ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0);

        let grey = 0.18;
        let through_reinhard = reinhard(grey * REINHARD);
        let through_filmic = filmic(grey * FILMIC);
        assert!(
            (through_reinhard - through_filmic).abs() < 1.0e-4,
            "{through_reinhard} vs {through_filmic}"
        );

        assert!(filmic(0.05 * FILMIC) < reinhard(0.05 * REINHARD));
        assert!(filmic(1.0 * FILMIC) > reinhard(1.0 * REINHARD));
    }

    #[test]
    fn the_tone_curve_survives_a_round_trip_through_its_stored_id() {
        for curve in ToneMapping::ALL {
            assert_eq!(ToneMapping::from_id(curve.id()), curve);
        }

        assert_eq!(ToneMapping::from_id(200), ToneMapping::Filmic);
    }
}
