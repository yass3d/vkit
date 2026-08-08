use std::{path::PathBuf, sync::Arc};

use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, FontTweak,
    Stroke, TextStyle, Theme, Vec2, Visuals, style::ScrollStyle,
};

use crate::i18n::Locale;

pub const COLOR_BG: Color32 = Color32::from_rgb(0x0b, 0x0b, 0x0b);
pub const COLOR_SURFACE: Color32 = Color32::from_rgb(0x18, 0x18, 0x18);
pub const COLOR_SURFACE_RAISED: Color32 = Color32::from_rgb(0x23, 0x23, 0x23);
pub const COLOR_SURFACE_HOVER: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
pub const COLOR_TEXT: Color32 = Color32::from_rgb(0xea, 0xea, 0xea);
pub const COLOR_MUTED: Color32 = Color32::from_rgb(0xa0, 0xa0, 0xa0);
pub const COLOR_BORDER: Color32 = Color32::from_rgb(0x3b, 0x3b, 0x3b);
pub const COLOR_AXIS_X: Color32 = Color32::from_rgb(0xe0, 0x5a, 0x5a);
pub const COLOR_AXIS_Y: Color32 = Color32::from_rgb(0x63, 0xbd, 0x73);
pub const COLOR_AXIS_Z: Color32 = Color32::from_rgb(0x5d, 0x98, 0xdc);
pub const COLOR_PRIMARY: Color32 = Color32::from_rgb(0xf4, 0xf4, 0xf4);

pub const COLOR_FOCUS: Color32 = Color32::from_rgb(0x70, 0x70, 0x70);
pub const COLOR_WARNING: Color32 = Color32::from_rgb(0xe6, 0xb7, 0x45);

pub const COLOR_WARNING_ACTIVE_BG: Color32 = Color32::from_rgb(0x8a, 0x72, 0x2a);

/// The one colour the guidance pulse breathes in. Everything else in the
/// interface is grey, so hue is the only channel a pulse can use that will not
/// be mistaken for a hover or a selection — which is why this is the only
/// saturated colour a background is allowed to take.
pub const COLOR_EMPHASIS: Color32 = Color32::from_rgb(0xf5, 0xb3, 0x0a);

pub const COLOR_TEXTURE_PIN: Color32 = Color32::from_rgb(42, 60, 78);
pub const COLOR_DESTRUCTIVE: Color32 = Color32::from_rgb(0xde, 0x5c, 0x63);

pub const COLOR_CLOSE_HOVER: Color32 = Color32::from_rgb(0xc4, 0x2b, 0x1c);
pub const COLOR_CLOSE_PRESSED: Color32 = Color32::from_rgb(0x9f, 0x22, 0x16);
pub const COLOR_SUCCESS: Color32 = Color32::from_rgb(0xd2, 0xd2, 0xd2);
pub const COLOR_TOPBAR: Color32 = Color32::from_rgb(0x10, 0x10, 0x10);
pub const COLOR_FIELD: Color32 = Color32::from_rgb(0x11, 0x11, 0x11);

pub const COLOR_TITLE_FIELD: Color32 = Color32::from_rgb(0x05, 0x05, 0x06);
pub const COLOR_VIEWPORT_TOOL: Color32 = Color32::from_rgb(0x25, 0x25, 0x25);

pub const COLOR_TRACK: Color32 = Color32::from_gray(54);

pub const COLOR_TRACK_FILL: Color32 = Color32::from_gray(238);

pub const COLOR_ICON: Color32 = Color32::from_gray(208);

pub const COLOR_ACTIVE_BG: Color32 = Color32::from_gray(232);
pub const COLOR_ACTIVE_INK: Color32 = Color32::from_gray(18);

pub const COLOR_RAIL_IDLE: Color32 = Color32::from_gray(20);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ControlState {
    pub hovered: bool,
    pub pressed: bool,

    pub active: bool,
}

pub fn control_fill(idle: Color32, state: ControlState) -> Color32 {
    if state.active {
        return COLOR_ACTIVE_BG;
    }
    if state.pressed {
        return hover_fill(hover_fill(idle));
    }
    if state.hovered {
        return hover_fill(idle);
    }
    idle
}

pub fn control_ink(state: ControlState) -> Color32 {
    if state.active {
        COLOR_ACTIVE_INK
    } else if state.hovered || state.pressed {
        COLOR_TEXT
    } else {
        COLOR_ICON
    }
}

pub const COLOR_HAIRLINE: Color32 = Color32::from_gray(82);
pub const COLOR_HAIRLINE_STRONG: Color32 = Color32::from_gray(214);

pub const COLOR_VIEWPORT_BG: Color32 = Color32::from_rgb(31, 33, 38);
pub const COLOR_VIEWPORT_BG_TOP: Color32 = Color32::from_rgb(48, 51, 58);
pub const COLOR_VIEWPORT_BG_BOTTOM: Color32 = Color32::from_rgb(17, 19, 23);
pub const COLOR_VIEWPORT_BG_CENTER: Color32 = Color32::from_rgb(53, 56, 64);
pub const COLOR_VIEWPORT_BG_EDGE: Color32 = Color32::from_rgb(15, 17, 21);

pub const SPACE_1: f32 = 2.0;
pub const SPACE_2: f32 = 4.0;
pub const SPACE_3: f32 = 8.0;
pub const SPACE_4: f32 = 12.0;

pub const RADIUS_S: u8 = 4;
pub const RADIUS_M: u8 = 8;
pub const RADIUS_POPOVER: u8 = 12;

pub const FONT_XS: f32 = 11.0;
pub const FONT_SM: f32 = 12.0;
pub const FONT_BODY: f32 = 13.0;
pub const FONT_HEADING: f32 = 15.0;

pub const CONTROL_H_COMPACT: f32 = 24.0;
pub const CONTROL_H_DENSE: f32 = 28.0;
pub const CONTROL_H: f32 = 32.0;
pub const CONTROL_H_PRIMARY: f32 = 42.0;

pub const TOP_BAR_HEIGHT: f32 = 44.0;
pub const STATUS_BAR_HEIGHT: f32 = 24.0;
pub const PROGRESS_HEIGHT: f32 = 3.0;
pub const INSPECTOR_DEFAULT_WIDTH: f32 = 432.0;
pub const INSPECTOR_MIN_WIDTH: f32 = 384.0;
pub const INSPECTOR_MAX_WIDTH: f32 = 640.0;
pub const INSPECTOR_RESIZE_GRAB_RADIUS: f32 = 4.0;
pub const INSPECTOR_ACTIVE_DIVIDER_WIDTH: f32 = 2.0;
pub const PANEL_INSET: f32 = 12.0;

pub const CONTROL_HEIGHT: f32 = CONTROL_H;

pub const SKIN_STAR_SIZE: f32 = 18.0;

pub const TITLE_VAM_TAB_GAP: f32 = 28.0;

pub const PLACE_HEAD_HEIGHT: f32 = 42.0;

pub const TITLE_SETTINGS_SIZE: f32 = 28.0;
pub const SECTION_GAP: f32 = 12.0;

pub const BODY_FONT_SIZE: f32 = FONT_BODY;

pub const TOOLTIP_DELAY_SECS: f32 = 0.5;

const TOOLTIP_GRACE_SECS: f32 = 0.2;

const TOOLTIP_SUPPRESSED_DELAY_SECS: f32 = 3600.0;

pub const SECTION_LABEL_FONT_SIZE: f32 = BODY_FONT_SIZE + 2.0;
pub const CONTROL_RADIUS: u8 = 8;
pub const CAPSULE_RADIUS: u8 = 16;

pub const ACTION_RADIUS: u8 = CAPSULE_RADIUS;

pub const SMALL_RADIUS: u8 = CONTROL_RADIUS;

pub const DISABLED_ALPHA: f32 = 0.48;

pub fn disabled(color: Color32) -> Color32 {
    color.gamma_multiply(DISABLED_ALPHA)
}

const HOVER_LIFT: u8 = 0x30 - 0x23;

pub fn hover_fill(base: Color32) -> Color32 {
    Color32::from_rgba_premultiplied(
        base.r().saturating_add(HOVER_LIFT),
        base.g().saturating_add(HOVER_LIFT),
        base.b().saturating_add(HOVER_LIFT),
        base.a(),
    )
}

pub fn focus_ring() -> Stroke {
    Stroke::new(1.0, COLOR_FOCUS)
}

#[derive(Clone, Debug)]
pub struct InstalledFont {
    pub path: PathBuf,
    pub y_offset_factor: f32,
}

#[derive(Clone, Debug)]
pub struct FontReport {
    pub fonts: Vec<InstalledFont>,

    pub korean_ready: bool,

    pub locale_ready: bool,
}

pub fn configure_context(context: &Context, locale: Locale) -> FontReport {
    let report = install_locale_fonts(context, locale);
    context.set_theme(Theme::Dark);
    context.set_visuals(vkit_visuals());
    context.style_mut_of(Theme::Dark, |style| {
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(10.0, 5.0);
        style.spacing.interact_size = Vec2::new(30.0, 30.0);
        style.spacing.combo_width = 112.0;
        style.spacing.text_edit_width = 180.0;

        style.spacing.scroll = ScrollStyle::solid();
        style.interaction.resize_grab_radius_side = INSPECTOR_RESIZE_GRAB_RADIUS;
        style.interaction.tooltip_delay = TOOLTIP_DELAY_SECS;
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(SECTION_LABEL_FONT_SIZE, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Body,
            FontId::new(BODY_FONT_SIZE, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(BODY_FONT_SIZE, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(12.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(12.0, FontFamily::Monospace),
        );
    });
    report
}

pub fn set_tooltips_enabled(context: &Context, enabled: bool) {
    let delay = if enabled {
        TOOLTIP_DELAY_SECS
    } else {
        TOOLTIP_SUPPRESSED_DELAY_SECS
    };
    if context.style_of(Theme::Dark).interaction.tooltip_delay == delay {
        return;
    }
    context.style_mut_of(Theme::Dark, |style| {
        style.interaction.tooltip_delay = delay;

        style.interaction.tooltip_grace_time = if enabled { TOOLTIP_GRACE_SECS } else { 0.0 };
    });
}

pub fn clamp_inspector_width(width: f32) -> f32 {
    if width.is_finite() {
        width.clamp(INSPECTOR_MIN_WIDTH, INSPECTOR_MAX_WIDTH)
    } else {
        INSPECTOR_DEFAULT_WIDTH
    }
}

#[derive(Clone, Copy, Debug)]
struct FontCandidate {
    file: &'static str,
    ttc_index: u32,
    y_offset_factor: f32,
}

const fn ttf(file: &'static str) -> FontCandidate {
    FontCandidate {
        file,
        ttc_index: 0,
        y_offset_factor: 0.0,
    }
}

const fn tweaked(file: &'static str, y_offset_factor: f32) -> FontCandidate {
    FontCandidate {
        file,
        ttc_index: 0,
        y_offset_factor,
    }
}

const fn ttc(file: &'static str, ttc_index: u32, y_offset_factor: f32) -> FontCandidate {
    FontCandidate {
        file,
        ttc_index,
        y_offset_factor,
    }
}

static KOREAN_FONTS: &[FontCandidate] = &[ttf("malgun.ttf"), ttf("malgunsl.ttf")];

static LATIN_FONTS: &[FontCandidate] = &[ttf("segoeui.ttf"), ttf("arial.ttf")];

static JAPANESE_FONTS: &[FontCandidate] = &[
    ttc("YuGothM.ttc", 0, 0.344),
    ttc("meiryo.ttc", 0, 0.029),
    ttc("msgothic.ttc", 0, 0.064),
];

static CHINESE_SIMPLIFIED_FONTS: &[FontCandidate] = &[
    ttc("msyh.ttc", 0, 0.0),
    ttc("msyhbd.ttc", 0, 0.0),
    ttf("simsun.ttc"),
];

static CHINESE_TRADITIONAL_FONTS: &[FontCandidate] =
    &[ttc("msjh.ttc", 0, -0.038), ttf("mingliu.ttc")];

static THAI_FONTS: &[FontCandidate] = &[tweaked("leelawui.ttf", -0.115), ttf("tahoma.ttf")];

static INDIC_FONTS: &[FontCandidate] = &[
    ttc("Nirmala.ttc", 0, 0.0),
    ttf("mangal.ttf"),
    ttf("vrinda.ttf"),
];

fn locale_font_groups(locale: Locale) -> Vec<&'static [FontCandidate]> {
    let mut groups: Vec<&'static [FontCandidate]> = Vec::with_capacity(7);

    match locale {
        Locale::Korean | Locale::English => {}
        Locale::Japanese => groups.push(JAPANESE_FONTS),
        Locale::ZhHans => groups.push(CHINESE_SIMPLIFIED_FONTS),
        Locale::ZhHant => groups.push(CHINESE_TRADITIONAL_FONTS),
        Locale::Thai => groups.push(THAI_FONTS),
        Locale::Hindi | Locale::Bengali => groups.push(INDIC_FONTS),

        Locale::Spanish
        | Locale::Portuguese
        | Locale::French
        | Locale::German
        | Locale::Russian
        | Locale::Indonesian
        | Locale::Vietnamese => {}
    }
    groups.push(KOREAN_FONTS);
    groups.push(LATIN_FONTS);

    for group in [
        JAPANESE_FONTS,
        CHINESE_SIMPLIFIED_FONTS,
        CHINESE_TRADITIONAL_FONTS,
        THAI_FONTS,
        INDIC_FONTS,
    ] {
        if !groups.iter().any(|existing| std::ptr::eq(*existing, group)) {
            groups.push(group);
        }
    }
    groups
}

pub fn install_locale_fonts(context: &Context, locale: Locale) -> FontReport {
    let windir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let fonts_dir = windir.join("Fonts");

    let groups = locale_font_groups(locale);
    let mut definitions = FontDefinitions::empty();
    let mut chain = Vec::new();
    let mut fonts = Vec::new();
    let mut korean_ready = false;
    let mut locale_ready = false;
    for (group_index, group) in groups.iter().enumerate() {
        for candidate in *group {
            let path = fonts_dir.join(candidate.file);
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            definitions.font_data.insert(
                candidate.file.to_owned(),
                Arc::new(FontData {
                    index: candidate.ttc_index,
                    tweak: FontTweak {
                        y_offset_factor: candidate.y_offset_factor,
                        ..FontTweak::default()
                    },
                    ..FontData::from_owned(bytes)
                }),
            );
            chain.push(candidate.file.to_owned());
            fonts.push(InstalledFont {
                path,
                y_offset_factor: candidate.y_offset_factor,
            });
            if std::ptr::eq(*group, KOREAN_FONTS) {
                korean_ready = true;
            }
            if group_index == 0 {
                locale_ready = true;
            }
            break;
        }
    }

    if chain.is_empty() {
        return FontReport {
            fonts,
            korean_ready: false,
            locale_ready: false,
        };
    }

    definitions
        .families
        .insert(FontFamily::Proportional, chain.clone());
    definitions.families.insert(FontFamily::Monospace, chain);
    context.set_fonts(definitions);
    FontReport {
        fonts,
        korean_ready,
        locale_ready,
    }
}

pub fn glyph_font_sources(context: &Context, character: char) -> Vec<String> {
    context.fonts_mut(|fonts| {
        fonts
            .fonts
            .font(&FontFamily::Proportional)
            .characters()
            .get(&character)
            .cloned()
            .unwrap_or_default()
    })
}

fn vkit_visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.override_text_color = Some(COLOR_TEXT);
    visuals.weak_text_color = Some(COLOR_MUTED);
    visuals.panel_fill = COLOR_SURFACE;
    visuals.window_fill = COLOR_SURFACE;
    visuals.window_stroke = Stroke::NONE;
    visuals.window_corner_radius = CornerRadius::same(CONTROL_RADIUS);
    visuals.menu_corner_radius = CornerRadius::same(CONTROL_RADIUS);
    visuals.extreme_bg_color = COLOR_FIELD;
    visuals.text_edit_bg_color = Some(COLOR_FIELD);
    visuals.faint_bg_color = COLOR_TOPBAR;
    visuals.code_bg_color = COLOR_FIELD;
    visuals.warn_fg_color = COLOR_WARNING;
    visuals.error_fg_color = COLOR_DESTRUCTIVE;
    visuals.selection.bg_fill = COLOR_FOCUS;
    visuals.selection.stroke = Stroke::new(1.0, COLOR_TEXT);
    visuals.hyperlink_color = COLOR_PRIMARY;
    visuals.text_cursor.stroke = Stroke::new(1.5, COLOR_PRIMARY);
    visuals.ime_composition.active_underline_stroke = Stroke::new(1.5, COLOR_PRIMARY);
    visuals.ime_composition.inactive_underline_stroke = Stroke::new(1.0, COLOR_FOCUS);
    visuals.slider_trailing_fill = true;
    visuals.disabled_alpha = 0.48;
    visuals.window_shadow = Default::default();
    visuals.popup_shadow = Default::default();

    visuals.widgets.noninteractive.bg_fill = COLOR_SURFACE;
    visuals.widgets.noninteractive.weak_bg_fill = COLOR_SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, COLOR_TEXT);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(CONTROL_RADIUS);

    visuals.widgets.inactive.bg_fill = COLOR_SURFACE_RAISED;
    visuals.widgets.inactive.weak_bg_fill = COLOR_SURFACE_RAISED;
    visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, COLOR_MUTED);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(CONTROL_RADIUS);

    visuals.widgets.hovered.bg_fill = COLOR_SURFACE_HOVER;
    visuals.widgets.hovered.weak_bg_fill = COLOR_SURFACE_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::NONE;
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, COLOR_TEXT);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(CONTROL_RADIUS);

    visuals.widgets.active.bg_fill = COLOR_SURFACE_HOVER;
    visuals.widgets.active.weak_bg_fill = COLOR_SURFACE_HOVER;
    visuals.widgets.active.bg_stroke = Stroke::NONE;
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, COLOR_PRIMARY);
    visuals.widgets.active.corner_radius = CornerRadius::same(CONTROL_RADIUS);

    visuals.widgets.open = visuals.widgets.active;
    visuals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_emphasis_colour_carries_a_hue_no_surface_can_imitate() {
        // The pulse used to breathe in near-white, and against surfaces that
        // are themselves grey it read as a hover rather than as a summons —
        // which is how it came to be missed. Its whole job now rests on being
        // the one saturated thing on screen, so a slide back toward grey has
        // to fail here rather than in front of someone using the app.
        let chroma_of = |color: Color32| {
            let [red, green, blue, _] = color.to_array().map(i16::from);
            red.max(green).max(blue) - red.min(green).min(blue)
        };
        let chroma = chroma_of(COLOR_EMPHASIS);
        assert!(
            chroma >= 0x60,
            "the emphasis pulse is only {chroma} away from grey; it has to be unmistakable"
        );

        for surface in [
            COLOR_PRIMARY,
            COLOR_SURFACE,
            COLOR_SURFACE_RAISED,
            COLOR_SURFACE_HOVER,
        ] {
            assert!(
                chroma_of(surface) <= 0x08,
                "a surface grew a hue, so the pulse no longer stands alone against it"
            );
        }
    }

    #[test]
    fn native_chrome_uses_the_borderless_token_contract() {
        assert_eq!(TOP_BAR_HEIGHT, 44.0);
        assert_eq!(STATUS_BAR_HEIGHT, 24.0);
        assert_eq!(CONTROL_HEIGHT, 32.0);
        assert_eq!(CONTROL_RADIUS, 8);
        assert_eq!(CAPSULE_RADIUS, 16);
        assert_eq!([SPACE_1, SPACE_2, SPACE_3, SPACE_4], [2.0, 4.0, 8.0, 12.0]);
        assert_eq!([RADIUS_S, RADIUS_M, RADIUS_POPOVER], [4, 8, 12]);
        assert_eq!(RADIUS_M, CONTROL_RADIUS);
        assert_eq!([FONT_XS, FONT_SM, FONT_BODY], [11.0, 12.0, 13.0]);
        assert_eq!(FONT_HEADING, 15.0);
        assert_eq!(FONT_BODY, BODY_FONT_SIZE);
        assert_eq!(FONT_HEADING, SECTION_LABEL_FONT_SIZE);
        assert_eq!(
            [
                CONTROL_H_COMPACT,
                CONTROL_H_DENSE,
                CONTROL_H,
                CONTROL_H_PRIMARY
            ],
            [24.0, 28.0, 32.0, 42.0]
        );
        assert_eq!(CONTROL_H, CONTROL_HEIGHT);
        assert_eq!(ACTION_RADIUS, CAPSULE_RADIUS);
        assert_eq!(PANEL_INSET, 12.0);
        assert_eq!(SECTION_LABEL_FONT_SIZE, BODY_FONT_SIZE + 2.0);
        assert_eq!(INSPECTOR_DEFAULT_WIDTH, 432.0);
        assert_eq!(INSPECTOR_MIN_WIDTH, 384.0);
        assert_eq!(INSPECTOR_MAX_WIDTH, 640.0);
        assert_eq!(TOOLTIP_DELAY_SECS, 0.5);
        let visuals = vkit_visuals();
        assert_eq!(visuals.window_stroke, Stroke::NONE);
        assert_eq!(visuals.widgets.inactive.bg_stroke, Stroke::NONE);
        assert_eq!(visuals.widgets.hovered.bg_stroke, Stroke::NONE);
        assert_eq!(visuals.widgets.active.bg_stroke, Stroke::NONE);
        assert_eq!(visuals.selection.bg_fill, COLOR_FOCUS);
        assert_eq!(visuals.selection.stroke.color, COLOR_TEXT);
        assert_eq!(visuals.text_cursor.stroke.color, COLOR_PRIMARY);
        assert_eq!(visuals.widgets.inactive.bg_fill, COLOR_SURFACE_RAISED);
        assert_eq!(visuals.widgets.hovered.bg_fill, COLOR_SURFACE_HOVER);
    }

    #[test]
    fn general_ui_palette_is_achromatic() {
        fn is_achromatic(color: Color32) -> bool {
            color.r() == color.g() && color.g() == color.b()
        }

        for color in [
            COLOR_BG,
            COLOR_SURFACE,
            COLOR_SURFACE_RAISED,
            COLOR_SURFACE_HOVER,
            COLOR_TEXT,
            COLOR_MUTED,
            COLOR_BORDER,
            COLOR_PRIMARY,
            COLOR_FOCUS,
            COLOR_SUCCESS,
            COLOR_TOPBAR,
            COLOR_FIELD,
            COLOR_VIEWPORT_TOOL,
            COLOR_TRACK,
            COLOR_ACTIVE_BG,
            COLOR_ACTIVE_INK,
            COLOR_RAIL_IDLE,
            COLOR_TRACK_FILL,
            COLOR_ICON,
            COLOR_HAIRLINE,
            COLOR_HAIRLINE_STRONG,
        ] {
            assert!(is_achromatic(color), "{color:?} reintroduces a UI hue");
        }
        assert!(!is_achromatic(COLOR_WARNING));
        assert!(!is_achromatic(COLOR_DESTRUCTIVE));
        assert_ne!(COLOR_AXIS_X, COLOR_AXIS_Y);
        assert_ne!(COLOR_AXIS_Y, COLOR_AXIS_Z);

        for color in [
            COLOR_VIEWPORT_BG,
            COLOR_VIEWPORT_BG_TOP,
            COLOR_VIEWPORT_BG_BOTTOM,
            COLOR_VIEWPORT_BG_CENTER,
            COLOR_VIEWPORT_BG_EDGE,
        ] {
            let channels = [color.r(), color.g(), color.b()];
            let spread = channels.iter().max().unwrap() - channels.iter().min().unwrap();
            assert!(
                spread <= 16,
                "{color:?} exceeds the near-neutral viewport exemption"
            );
        }

        assert!(!is_achromatic(COLOR_WARNING_ACTIVE_BG));
    }

    #[test]
    fn state_recipes_are_single_sourced() {
        assert_eq!(DISABLED_ALPHA, vkit_visuals().disabled_alpha);
        let faded = disabled(COLOR_TEXT);
        assert!(faded.r() < COLOR_TEXT.r());
        assert_eq!(faded.r(), faded.g());
        assert_eq!(faded.g(), faded.b());
        assert_eq!(hover_fill(COLOR_SURFACE_RAISED), COLOR_SURFACE_HOVER);
        let lifted = hover_fill(COLOR_VIEWPORT_TOOL);
        assert_eq!(
            lifted.r() - COLOR_VIEWPORT_TOOL.r(),
            COLOR_SURFACE_HOVER.r() - COLOR_SURFACE_RAISED.r()
        );
        assert_eq!(focus_ring().width, 1.0);
        assert_eq!(focus_ring().color, COLOR_FOCUS);
    }

    #[test]
    fn locale_font_chains_lead_with_the_locale_script_and_keep_korean_and_latin() {
        for locale in Locale::ALL {
            let groups = locale_font_groups(locale);
            assert!(
                groups
                    .iter()
                    .any(|group| std::ptr::eq(*group, KOREAN_FONTS)),
                "{locale:?} chain must keep Hangul coverage"
            );
            assert!(
                groups.iter().any(|group| std::ptr::eq(*group, LATIN_FONTS)),
                "{locale:?} chain must keep a Latin safety net"
            );
            let expected_head = match locale {
                Locale::Korean | Locale::English => KOREAN_FONTS,
                Locale::Japanese => JAPANESE_FONTS,
                Locale::ZhHans => CHINESE_SIMPLIFIED_FONTS,
                Locale::ZhHant => CHINESE_TRADITIONAL_FONTS,
                Locale::Thai => THAI_FONTS,
                Locale::Hindi | Locale::Bengali => INDIC_FONTS,

                Locale::Spanish
                | Locale::Portuguese
                | Locale::French
                | Locale::German
                | Locale::Russian
                | Locale::Indonesian
                | Locale::Vietnamese => KOREAN_FONTS,
            };
            assert!(
                std::ptr::eq(groups[0], expected_head),
                "{locale:?} chain must lead with its primary script"
            );
        }
    }

    #[test]
    fn japanese_font_chain_resolves_kana_and_keeps_hangul_coverage() {
        let context = Context::default();
        let report = configure_context(&context, crate::i18n::Locale::Japanese);
        if report.fonts.is_empty() {
            eprintln!("skipping: no Windows system fonts available");
            return;
        }

        let _ = context.run_ui(egui::RawInput::default(), |_| {});
        if report.locale_ready {
            let kana_sources = glyph_font_sources(&context, 'あ');
            assert!(
                !kana_sources.is_empty(),
                "kana must resolve to a loaded font from {:?}",
                report.fonts
            );
            eprintln!("kana 'あ' renders with: {}", kana_sources.join(", "));
        }
        if report.korean_ready {
            let hangul_sources = glyph_font_sources(&context, '한');
            assert!(
                !hangul_sources.is_empty(),
                "hangul must keep resolving under the Japanese chain"
            );
            eprintln!("hangul '한' renders with: {}", hangul_sources.join(", "));
        }
    }

    fn glyph_center_in_control(locale: Locale, character: char) -> Option<f32> {
        let context = Context::default();
        let report = configure_context(&context, locale);
        if report.fonts.is_empty() {
            return None;
        }
        let _ = context.run_ui(egui::RawInput::default(), |_| {});
        let galley = context.fonts_mut(|fonts| {
            fonts.layout_no_wrap(
                character.to_string(),
                FontId::proportional(FONT_BODY),
                COLOR_TEXT,
            )
        });
        let placed = galley.rows.first()?;
        let glyph = placed.row.glyphs.first()?;
        if glyph.uv_rect.size.y <= 0.0 {
            return None;
        }
        let top = placed.pos.y + glyph.pos.y + glyph.uv_rect.offset.y;
        Some((CONTROL_H - galley.size().y) * 0.5 + top + glyph.uv_rect.size.y * 0.5)
    }

    #[test]
    fn every_script_centres_in_controls_like_korean_text() {
        let Some(reference) = glyph_center_in_control(Locale::Korean, '한') else {
            eprintln!("skipping: no Windows system fonts available");
            return;
        };

        for (locale, sample, name) in [
            (Locale::Japanese, 'あ', "Japanese kana"),
            (Locale::Japanese, '한', "Hangul under the Japanese chain"),
            (Locale::ZhHans, '简', "Simplified Chinese"),
            (Locale::ZhHant, '繁', "Traditional Chinese"),
            (Locale::Hindi, 'क', "Devanagari"),
            (Locale::Bengali, 'ক', "Bengali"),
            (Locale::Thai, 'ก', "Thai"),
            (Locale::Russian, 'Я', "Cyrillic"),
            (Locale::Vietnamese, 'ế', "Vietnamese stacked diacritics"),
        ] {
            let Some(centre) = glyph_center_in_control(locale, sample) else {
                eprintln!("skipping {name}: no font for {sample:?}");
                continue;
            };
            let delta = centre - reference;
            eprintln!("{name}: {centre:.2}px ({delta:+.2}px from Hangul)");
            assert!(
                delta.abs() <= 1.5,
                "{name} sits {delta:+.2}px from the Hangul reference; \
                 its font needs a y_offset_factor, measured against this same row"
            );
        }

        for candidate in KOREAN_FONTS.iter().chain(LATIN_FONTS.iter()) {
            assert_eq!(
                candidate.y_offset_factor, 0.0,
                "{} must stay untweaked",
                candidate.file
            );
        }
    }

    #[test]
    fn every_language_names_itself_in_a_font_that_exists() {
        for locale in Locale::ALL {
            let sample = locale
                .selector_label()
                .chars()
                .find(|character| !character.is_ascii())
                .unwrap_or('A');
            let context = Context::default();

            if configure_context(&context, Locale::Korean).fonts.is_empty() {
                eprintln!("skipping: no Windows system fonts available");
                return;
            }
            let _ = context.run_ui(egui::RawInput::default(), |_| {});
            let sources = glyph_font_sources(&context, sample);
            assert!(
                !sources.is_empty(),
                "{:?} names itself {:?}, and {sample:?} has no font in the chain -- \
                 the picker would draw it as a box",
                locale,
                locale.selector_label()
            );
        }
    }

    #[test]
    fn inspector_width_contract_clamps_settings_and_live_drag_values() {
        assert_eq!(clamp_inspector_width(120.0), INSPECTOR_MIN_WIDTH);
        assert_eq!(clamp_inspector_width(512.0), 512.0);
        assert_eq!(clamp_inspector_width(2_000.0), INSPECTOR_MAX_WIDTH);
        assert_eq!(clamp_inspector_width(f32::NAN), INSPECTOR_DEFAULT_WIDTH);
        assert_eq!(
            clamp_inspector_width(f32::INFINITY),
            INSPECTOR_DEFAULT_WIDTH
        );
    }

    #[test]
    fn minimum_inspector_keeps_a_useful_borderless_content_column() {
        let inner_width = INSPECTOR_MIN_WIDTH - PANEL_INSET * 2.0;
        assert_eq!(inner_width, 360.0);
        assert_eq!(INSPECTOR_RESIZE_GRAB_RADIUS * 2.0, 8.0);
        assert_eq!(INSPECTOR_ACTIVE_DIVIDER_WIDTH, 2.0);
    }

    fn numeric_after(line: &str, needle: &str) -> usize {
        let mut count = 0;
        let mut rest = line;
        while let Some(position) = rest.find(needle) {
            rest = &rest[position + needle.len()..];
            if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                count += 1;
            }
        }
        count
    }

    #[test]
    fn ui_sources_do_not_reintroduce_raw_style_literals() {
        let sources = [
            ("ui.rs", include_str!("ui.rs"), 0_usize),
            ("ui_components.rs", include_str!("ui_components.rs"), 2),
            ("viewport.rs", include_str!("viewport.rs"), 7),
            (
                "viewport_tool_layout.rs",
                include_str!("viewport_tool_layout.rs"),
                0,
            ),
        ];
        for (name, source, rgb_allowance) in sources {
            let mut from_gray = 0_usize;
            let mut from_rgb = 0_usize;
            let mut sized_text = 0_usize;
            let mut spaced = 0_usize;
            let mut font_points = 0_usize;
            for line in source.lines() {
                from_gray += line.matches("from_gray(").count();
                from_rgb += line.matches("from_rgb(").count();
                if !line.contains("Spinner") {
                    sized_text += numeric_after(line, ".size(");
                }
                spaced += numeric_after(line, "add_space(");
                font_points += numeric_after(line, "FontId::proportional(");
            }
            assert_eq!(
                from_gray, 0,
                "{name}: new from_gray literal; use a theme color token"
            );
            assert!(
                from_rgb <= rgb_allowance,
                "{name}: {from_rgb} from_rgb call sites exceed the audited \
                 baseline of {rgb_allowance}; use a theme color token"
            );
            assert_eq!(
                sized_text, 0,
                "{name}: raw text size; use the FONT_XS..FONT_HEADING scale"
            );
            assert_eq!(
                spaced, 0,
                "{name}: raw add_space pixels; use the SPACE_1..SPACE_4 scale"
            );
            assert_eq!(
                font_points, 0,
                "{name}: raw FontId size; use the FONT_XS..FONT_HEADING scale"
            );
        }
    }
}
