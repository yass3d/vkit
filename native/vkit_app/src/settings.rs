use egui::{Align, Frame, Id, Layout, Margin, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::{
    i18n::{Locale, TextKey, text},
    lighting::LightingPreset,
    shader_color::ToneMapping,
    shortcuts::{Binding, ModifierPolicy, Shortcut, Trigger},
    state::{Action, AppState, MorphNameDisplay, ViewportBackgroundMode},
    theme::{
        COLOR_BORDER, COLOR_FIELD, COLOR_HAIRLINE, COLOR_MUTED, COLOR_SURFACE_HOVER,
        COLOR_SURFACE_RAISED, COLOR_TEXT, CONTROL_H_DENSE, CONTROL_RADIUS, FONT_HEADING, FONT_SM,
        FONT_XS, SPACE_1, SPACE_2, SPACE_3, SPACE_4, hover_fill,
    },
    ui_components::{FilledNumericSlider, Icon, paint_icon, paint_list_row_highlight, tooltip},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SettingsSection {
    #[default]
    Graphics,
    Viewport,
    General,
    Shortcuts,

    About,
}

impl SettingsSection {
    pub const ALL: [Self; 5] = [
        Self::Graphics,
        Self::Viewport,
        Self::General,
        Self::Shortcuts,
        Self::About,
    ];

    const fn label(self) -> TextKey {
        match self {
            Self::Graphics => TextKey::SettingsGraphics,
            Self::Viewport => TextKey::SettingsViewport,
            Self::General => TextKey::SettingsGeneral,
            Self::Shortcuts => TextKey::SettingsShortcuts,
            Self::About => TextKey::SettingsAbout,
        }
    }
}

const PAGE_MAX_WIDTH: f32 = 760.0;
const PAGE_MAX_HEIGHT: f32 = 560.0;

const SECTION_COLUMN_WIDTH: f32 = 148.0;
const SECTION_ROW_HEIGHT: f32 = 32.0;

const CONTROL_COLUMN_WIDTH: f32 = 220.0;
const ROW_HEIGHT: f32 = 30.0;

const COMBO_LABEL_ALLOWANCE: f32 = 44.0;

const SUPPORT_MARK_SIZE: f32 = 54.0;
const SUPPORT_MARK_SLOT: f32 = 64.0;
const SOURCE_MARK_SIZE: f32 = 30.0;
const SOURCE_MARK_SLOT: f32 = 40.0;

const MARK_GROW: f32 = 0.16;
const MARK_GROW_SECONDS: f32 = 0.12;

const ABOUT_ROW_HEIGHT: f32 = 26.0;

pub fn draw_settings_page(root: &mut Ui, state: &mut AppState) {
    if !state.settings_open {
        return;
    }
    let content = root.ctx().content_rect().size();
    let width = (content.x - 96.0).clamp(420.0, PAGE_MAX_WIDTH);
    let height = (content.y - 96.0).clamp(300.0, PAGE_MAX_HEIGHT);

    let mut section = state.settings_section;
    let modal = egui::Modal::new(Id::new("vkit.settings.modal"))
        .frame(
            Frame::new()
                .fill(COLOR_SURFACE_RAISED)
                .stroke(Stroke::NONE)
                .corner_radius(CONTROL_RADIUS)
                .inner_margin(Margin::same(0)),
        )
        .show(root.ctx(), |ui| {
            ui.set_width(width);
            ui.set_height(height);
            let full = ui.max_rect();
            let column = Rect::from_min_size(full.min, vec2(SECTION_COLUMN_WIDTH, full.height()));
            let pane = Rect::from_min_max(pos2(column.right() + 1.0, full.top()), full.max);

            ui.painter().vline(
                column.right(),
                full.top()..=full.bottom(),
                Stroke::new(1.0, COLOR_BORDER),
            );

            draw_section_column(ui, state.locale, column, &mut section);
            draw_section_pane(ui, state, pane, section);
        });

    if section != state.settings_section {
        state.settings_section = section;
    }
    if modal.should_close() {
        state.dispatch(Action::CloseSettings);
    }
}

fn draw_section_column(ui: &mut Ui, locale: Locale, rect: Rect, section: &mut SettingsSection) {
    let mut column = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(SPACE_2, SPACE_3)))
            .layout(Layout::top_down(Align::Min)),
    );
    for candidate in SettingsSection::ALL {
        let (row, response) = column.allocate_exact_size(
            vec2(column.available_width(), SECTION_ROW_HEIGHT),
            Sense::click(),
        );
        let selected = *section == candidate;
        paint_list_row_highlight(&column, row, selected, response.hovered());
        column.painter().text(
            pos2(row.left() + SPACE_3, row.center().y),
            egui::Align2::LEFT_CENTER,
            text(locale, candidate.label()),
            egui::FontId::proportional(FONT_SM),
            if selected { COLOR_TEXT } else { COLOR_MUTED },
        );
        if response.clicked() {
            *section = candidate;
        }
    }
}

/// How much room the settings content keeps down each side.
///
/// Wider than the vertical margin on purpose: a row is a label on the left and
/// a control on the right, and with the pane's own edge close to both of them
/// the row reads as though it is falling off.
const PANE_SIDE_MARGIN: i8 = 24;

fn draw_section_pane(ui: &mut Ui, state: &mut AppState, rect: Rect, section: SettingsSection) {
    // The scroll area takes the whole pane so its bar rides at the pane's own
    // right edge, where a bar belongs; the margin lives inside it, on the
    // content. Insetting the scroll area instead dragged the bar inward with
    // the text, and what a reader wants room around is the content.
    let mut pane = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(0.0, SPACE_4)))
            .layout(Layout::top_down(Align::Min)),
    );
    pane.spacing_mut().item_spacing.y = SPACE_2;
    egui::ScrollArea::vertical()
        .id_salt("vkit.settings.pane")
        .auto_shrink([false, false])
        .show(&mut pane, |ui| {
            Frame::new()
                .inner_margin(Margin::symmetric(PANE_SIDE_MARGIN, 0))
                .show(ui, |ui| draw_section_body(ui, state, section));
        });
}

fn draw_section_body(ui: &mut Ui, state: &mut AppState, section: SettingsSection) {
    match section {
        SettingsSection::Graphics => draw_graphics_settings(ui, state),
        SettingsSection::Viewport => draw_viewport_settings(ui, state),
        SettingsSection::General => draw_general_settings(ui, state),
        SettingsSection::Shortcuts => draw_shortcut_settings(ui, state),
        SettingsSection::About => draw_about_settings(ui, state),
    }
}

fn setting_row<R>(
    ui: &mut Ui,
    locale: Locale,
    label: TextKey,
    hint: Option<TextKey>,
    control: impl FnOnce(&mut Ui) -> R,
) -> R {
    setting_row_sized(ui, locale, label, hint, CONTROL_COLUMN_WIDTH, control)
}

fn setting_row_sized<R>(
    ui: &mut Ui,
    locale: Locale,
    label: TextKey,
    hint: Option<TextKey>,
    control_width: f32,
    control: impl FnOnce(&mut Ui) -> R,
) -> R {
    let (row, _) = ui.allocate_exact_size(
        vec2(ui.available_width().max(0.0), ROW_HEIGHT),
        Sense::hover(),
    );

    let control_width = control_width
        .max(CONTROL_COLUMN_WIDTH)
        .min(row.width().max(0.0));
    let control_rect = Rect::from_min_size(
        pos2(row.right() - control_width, row.top()),
        vec2(control_width, row.height()),
    );
    ui.painter().with_clip_rect(row).text(
        pos2(row.left(), row.center().y),
        egui::Align2::LEFT_CENTER,
        crate::ui_components::ellipsize_to_width(
            ui,
            text(locale, label),
            (control_rect.left() - row.left() - SPACE_2).max(0.0),
            egui::FontId::proportional(FONT_SM),
        ),
        egui::FontId::proportional(FONT_SM),
        COLOR_TEXT,
    );
    let mut cell = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(control_rect)
            .layout(Layout::right_to_left(Align::Center)),
    );
    let value = control(&mut cell);
    if let Some(hint) = hint {
        ui.painter().text(
            pos2(row.left(), row.bottom() + 1.0),
            egui::Align2::LEFT_TOP,
            text(locale, hint),
            egui::FontId::proportional(FONT_XS),
            COLOR_MUTED,
        );
        ui.add_space(FONT_XS + SPACE_2);
    }
    value
}

fn effect_switch(
    ui: &mut Ui,
    locale: Locale,
    enabled: &mut bool,
    description: TextKey,
) -> egui::Response {
    let row =
        crate::ui_components::switch_row(ui, enabled, text(locale, TextKey::SettingsEffectEnabled));
    tooltip(
        row,
        text(locale, description),
        crate::ui_components::NO_SHORTCUT,
    )
}

/// A section label with a hairline running off the end of it.
///
/// One shape for every section in the page, including the shortcut list and the
/// about pane — the about pane drew a full-width rule *above* its headings,
/// which reads as the end of what came before rather than the start of what
/// follows.
fn group_heading(ui: &mut Ui, locale: Locale, key: TextKey) {
    ui.add_space(SPACE_3);
    let label = egui::RichText::new(text(locale, key))
        .size(FONT_XS)
        .color(COLOR_MUTED);
    let galley = ui.painter().layout_no_wrap(
        text(locale, key).to_owned(),
        egui::FontId::proportional(FONT_XS),
        COLOR_MUTED,
    );
    let row = galley.size().y.max(FONT_XS);
    ui.allocate_ui_with_layout(
        vec2(ui.available_width().max(0.0), row),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.label(label);
            let rest = ui.available_width() - SPACE_2;
            if rest > SPACE_2 {
                ui.add_space(SPACE_2);
                let (rect, _) = ui.allocate_exact_size(vec2(rest, row), Sense::hover());
                ui.painter().rect_filled(
                    Rect::from_min_max(
                        pos2(rect.left(), rect.center().y - 0.5),
                        pos2(rect.right(), rect.center().y + 0.5),
                    ),
                    0.0,
                    COLOR_HAIRLINE,
                );
            }
        },
    );
    ui.add_space(SPACE_2);
}

/// Quality, then effects, then lighting, down one page.
///
/// They were three segments of a picker, which asked the reader to remember
/// which of three places a setting was in before they could look for it. There
/// are eleven controls between them; a picker is for pages that do not fit
/// beside each other, and these do.
fn draw_graphics_settings(ui: &mut Ui, state: &mut AppState) {
    draw_quality_settings(ui, state);
    ui.add_space(SPACE_4);
    draw_effect_settings(ui, state);
    ui.add_space(SPACE_4);
    draw_lighting_settings(ui, state);
}

fn msaa_label(samples: u32) -> String {
    if samples <= 1 {
        "Off".to_owned()
    } else {
        format!("{samples}x")
    }
}

fn draw_quality_settings(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;

    // Smoothing passes decide how the surface is shaded, which is the same
    // question antialiasing asks about its edges. It sat under the viewport,
    // beside the background colour.
    group_heading(ui, locale, TextKey::SettingsGraphicsQuality);
    let mut passes = f32::from(state.surface_smooth_passes);
    let changed = setting_row(
        ui,
        locale,
        TextKey::SettingsSmoothPasses,
        Some(TextKey::SettingsSmoothPassesHint),
        |ui| {
            ui.add(
                FilledNumericSlider::new(&mut passes, 0.0..=4.0)
                    .decimals(0)
                    .min_width(CONTROL_COLUMN_WIDTH),
            )
            .changed()
        },
    );
    if changed {
        state.dispatch(Action::SetSurfaceSmoothPasses(passes.round() as u8));
    }

    let mut wanted = state.msaa_samples;
    let offered = crate::renderer::supported_msaa_samples();
    let row = setting_row(ui, locale, TextKey::SettingsMsaa, None, |ui| {
        egui::ComboBox::from_id_salt("vkit.settings.msaa")
            .width(CONTROL_COLUMN_WIDTH)
            .selected_text(msaa_label(wanted))
            .show_ui(ui, |ui| {
                for candidate in offered {
                    ui.selectable_value(&mut wanted, *candidate, msaa_label(*candidate));
                }
            })
            .response
    });
    tooltip(
        row,
        text(locale, TextKey::SettingsMsaaTooltip),
        crate::ui_components::NO_SHORTCUT,
    );
    if wanted != state.msaa_samples {
        state.dispatch(Action::SetMsaaSamples(wanted));
    }
}

fn draw_lighting_settings(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;

    group_heading(ui, locale, TextKey::SettingsGraphicsLighting);
    let mut preset = state.lighting_preset;
    setting_row(ui, locale, TextKey::SettingsLightingPreset, None, |ui| {
        egui::ComboBox::from_id_salt("vkit.settings.lighting")
            .width(CONTROL_COLUMN_WIDTH)
            .selected_text(lighting_preset_label(preset, locale))
            .show_ui(ui, |ui| {
                for candidate in LightingPreset::ALL {
                    ui.selectable_value(
                        &mut preset,
                        candidate,
                        lighting_preset_label(candidate, locale),
                    );
                }
            });
    });
    if preset != state.lighting_preset {
        state.dispatch(Action::SetLightingPreset(preset));
    }

    let mut brightness = state.light_brightness;
    let changed = setting_row(ui, locale, TextKey::SettingsExposure, None, |ui| {
        ui.add(
            FilledNumericSlider::new(&mut brightness, 0.35..=2.0)
                .decimals(2)
                .min_width(CONTROL_COLUMN_WIDTH),
        )
        .changed()
    });
    if changed {
        state.dispatch(Action::SetLightBrightness(brightness));
    }

    let mut curve = state.tone_mapping;
    let row = setting_row(ui, locale, TextKey::SettingsToneCurve, None, |ui| {
        egui::ComboBox::from_id_salt("vkit.settings.tone-curve")
            .width(CONTROL_COLUMN_WIDTH)
            .selected_text(text(locale, tone_curve_label(curve)))
            .show_ui(ui, |ui| {
                for candidate in ToneMapping::ALL {
                    ui.selectable_value(
                        &mut curve,
                        candidate,
                        text(locale, tone_curve_label(candidate)),
                    );
                }
            })
            .response
    });
    tooltip(
        row,
        text(locale, TextKey::SettingsToneCurveTooltip),
        crate::ui_components::NO_SHORTCUT,
    );
    if curve != state.tone_mapping {
        state.dispatch(Action::SetToneMapping(curve));
    }
}

fn draw_effect_settings(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;

    group_heading(ui, locale, TextKey::SettingsGraphicsEffects);
    let mut vignette = state.vignette;
    let mut changed = effect_switch(
        ui,
        locale,
        &mut vignette.enabled,
        TextKey::SettingsVignetteTooltip,
    )
    .changed();

    ui.add_enabled_ui(vignette.enabled, |ui| {
        for (key, value, range) in [
            (
                TextKey::SettingsVignetteIntensity,
                &mut vignette.intensity,
                crate::post_process::VIGNETTE_INTENSITY_RANGE,
            ),
            (
                TextKey::SettingsVignetteSmoothness,
                &mut vignette.smoothness,
                crate::post_process::VIGNETTE_SMOOTHNESS_RANGE,
            ),
            (
                TextKey::SettingsVignetteRoundness,
                &mut vignette.roundness,
                crate::post_process::VIGNETTE_ROUNDNESS_RANGE,
            ),
        ] {
            changed |= setting_row(ui, locale, key, None, |ui| {
                ui.add(
                    FilledNumericSlider::new(value, range)
                        .percent()
                        .decimals(0)
                        .min_width(CONTROL_COLUMN_WIDTH),
                )
                .changed()
            });
        }
    });
    if changed {
        state.dispatch(Action::SetVignette(vignette));
    }
}

fn draw_viewport_settings(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;

    group_heading(ui, locale, TextKey::SettingsBackgroundGroup);
    let mut background = state.viewport_background_mode;
    setting_row(ui, locale, TextKey::SettingsBackground, None, |ui| {
        egui::ComboBox::from_id_salt("vkit.settings.background")
            .width(CONTROL_COLUMN_WIDTH)
            .selected_text(text(locale, background_label(background)))
            .show_ui(ui, |ui| {
                for candidate in [
                    ViewportBackgroundMode::Radial,
                    ViewportBackgroundMode::Vertical,
                    ViewportBackgroundMode::Flat,
                ] {
                    ui.selectable_value(
                        &mut background,
                        candidate,
                        text(locale, background_label(candidate)),
                    );
                }
            });
    });
    if background != state.viewport_background_mode {
        state.dispatch(Action::SetViewportBackgroundMode(background));
    }

    group_heading(ui, locale, TextKey::SettingsOverlayGroup);
    let mut wireframe = state.wireframe_color_rgb;
    let changed = setting_row(ui, locale, TextKey::SettingsWireframeColor, None, |ui| {
        ui.color_edit_button_srgb(&mut wireframe).changed()
    });
    if changed {
        state.dispatch(Action::SetWireframeColor(wireframe));
    }

    let mut opacity = state.wireframe_opacity;
    let changed = setting_row(ui, locale, TextKey::SettingsWireframeOpacity, None, |ui| {
        ui.add(
            FilledNumericSlider::new(&mut opacity, 0.0..=1.0)
                .percent()
                .decimals(0)
                .min_width(CONTROL_COLUMN_WIDTH),
        )
        .changed()
    });
    if changed {
        state.dispatch(Action::SetWireframeOpacity(opacity));
    }

    let mut xray = state.xray_opacity;
    let changed = setting_row(ui, locale, TextKey::SettingsXrayOpacity, None, |ui| {
        ui.add(
            FilledNumericSlider::new(&mut xray, 0.0..=1.0)
                .percent()
                .decimals(0)
                .min_width(CONTROL_COLUMN_WIDTH),
        )
        .changed()
    });
    if changed {
        state.dispatch(Action::SetXrayOpacity(xray));
    }

    let mut solid = state.custom_head_solid_color_rgb;
    let changed = setting_row(ui, locale, TextKey::SettingsSolidColor, None, |ui| {
        ui.color_edit_button_srgb(&mut solid).changed()
    });
    if changed {
        state.dispatch(Action::SetCustomHeadSolidColor(solid));
    }

    group_heading(ui, locale, TextKey::SettingsBrushSweepGroup);
    let mut commit = state.brush_sweep_commit;
    setting_row(
        ui,
        locale,
        TextKey::SettingsBrushSweepGroup,
        Some(TextKey::BrushSweepTooltip),
        |ui| {
            egui::ComboBox::from_id_salt("vkit.settings.brush-sweep")
                .width(CONTROL_COLUMN_WIDTH)
                .selected_text(text(locale, commit.label_key()))
                .show_ui(ui, |ui| {
                    for candidate in crate::sweep_gesture::SweepCommit::ALL {
                        ui.selectable_value(
                            &mut commit,
                            candidate,
                            text(locale, candidate.label_key()),
                        );
                    }
                });
        },
    );
    if commit != state.brush_sweep_commit {
        state.dispatch(Action::SetBrushSweepCommit(commit));
    }
}

const fn morph_name_display_key(display: MorphNameDisplay) -> TextKey {
    match display {
        MorphNameDisplay::Localized => TextKey::SettingsMorphNamesTranslated,
        MorphNameDisplay::Original => TextKey::SettingsMorphNamesOriginal,
    }
}

fn draw_general_settings(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;

    group_heading(ui, locale, TextKey::SettingsLanguageGroup);
    let mut selected = state.locale;

    let picker_width = Locale::ALL
        .iter()
        .map(|candidate| {
            ui.painter()
                .layout_no_wrap(
                    candidate.selector_label().to_owned(),
                    egui::FontId::proportional(FONT_SM),
                    COLOR_TEXT,
                )
                .size()
                .x
        })
        .fold(0.0_f32, f32::max)
        + COMBO_LABEL_ALLOWANCE;
    setting_row_sized(
        ui,
        locale,
        TextKey::SettingsLanguage,
        None,
        picker_width,
        |ui| {
            egui::ComboBox::from_id_salt("vkit.settings.locale")
                .width(picker_width)
                .selected_text(selected.selector_label())
                .show_ui(ui, |ui| {
                    for candidate in Locale::ALL {
                        ui.selectable_value(&mut selected, candidate, candidate.selector_label());
                    }
                });
        },
    );
    if selected != state.locale {
        state.dispatch(Action::SetLocale(selected));
    }

    let mut names = state.morph_name_display;
    setting_row_sized(
        ui,
        locale,
        TextKey::SettingsMorphNames,
        None,
        picker_width,
        |ui| {
            egui::ComboBox::from_id_salt("vkit.settings.morph-names")
                .width(picker_width)
                .selected_text(text(locale, morph_name_display_key(names)))
                .show_ui(ui, |ui| {
                    for candidate in [MorphNameDisplay::Localized, MorphNameDisplay::Original] {
                        ui.selectable_value(
                            &mut names,
                            candidate,
                            text(locale, morph_name_display_key(candidate)),
                        );
                    }
                });
        },
    );
    if names != state.morph_name_display {
        state.morph_name_display = names;
    }

    group_heading(ui, locale, TextKey::SettingsInterfaceGroup);
    let mut tooltips = state.tooltips_enabled;

    let changed = ui
        .scope(|ui| {
            ui.style_mut().interaction.tooltip_delay = 0.0;
            ui.style_mut().interaction.tooltip_grace_time = 0.0;
            crate::ui_components::switch_row(
                ui,
                &mut tooltips,
                text(locale, TextKey::SettingsTooltips),
            )
            .on_hover_text(text(locale, TextKey::SettingsTooltipsTooltip))
            .changed()
        })
        .inner;
    if changed {
        state.dispatch(Action::SetTooltipsEnabled(tooltips));
    }

    group_heading(ui, locale, TextKey::SettingsFoldersGroup);
    let root_label = state
        .vam_root
        .as_deref()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| text(locale, TextKey::VaMFolder).to_owned());
    let clicked = setting_row(ui, locale, TextKey::SettingsVaMRoot, None, |ui| {
        let (rect, response) =
            ui.allocate_exact_size(vec2(CONTROL_COLUMN_WIDTH, CONTROL_H_DENSE), Sense::click());
        ui.painter().rect_filled(
            rect,
            CONTROL_RADIUS,
            if response.hovered() {
                hover_fill(COLOR_FIELD)
            } else {
                COLOR_FIELD
            },
        );
        ui.painter().text(
            pos2(rect.left() + SPACE_3, rect.center().y),
            egui::Align2::LEFT_CENTER,
            crate::ui_components::ellipsize_to_width(
                ui,
                &root_label,
                rect.width() - SPACE_3 * 2.0,
                egui::FontId::proportional(FONT_SM),
            ),
            egui::FontId::proportional(FONT_SM),
            COLOR_TEXT,
        );
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        response.clicked()
    });
    if clicked {
        state.dispatch(Action::RequestVaMRootBrowse);
    }

    group_heading(ui, locale, TextKey::SettingsResetGroup);

    if state.cache_bytes.is_none() && !state.cache_measure_requested {
        state.dispatch(Action::MeasureCache);
    }
    let cached = state.cache_bytes.unwrap_or(0);
    let clear = ui
        .add_enabled(
            cached > 0,
            egui::Button::new(
                egui::RichText::new(format!(
                    "{} ({})",
                    text(locale, TextKey::SettingsClearCache),
                    human_bytes(cached)
                ))
                .size(FONT_SM),
            )
            .min_size(vec2(ui.available_width().max(0.0), CONTROL_H_DENSE)),
        )
        .on_hover_text(text(locale, TextKey::SettingsClearCacheTooltip));
    if clear.clicked() {
        state.dispatch(Action::ClearCache);
    }

    ui.add_space(SPACE_1);

    let reset = ui.add_sized(
        vec2(ui.available_width().max(0.0), CONTROL_H_DENSE),
        egui::Button::new(
            egui::RichText::new(text(locale, TextKey::SettingsResetAll)).size(FONT_SM),
        ),
    );
    if reset.clicked() {
        state.dispatch(Action::ResetAllSettings);
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

const fn background_label(mode: ViewportBackgroundMode) -> TextKey {
    match mode {
        ViewportBackgroundMode::Radial => TextKey::BackgroundRadial,
        ViewportBackgroundMode::Vertical => TextKey::BackgroundVertical,
        ViewportBackgroundMode::Flat => TextKey::BackgroundFlat,
    }
}

const fn tone_curve_label(curve: ToneMapping) -> TextKey {
    match curve {
        ToneMapping::Filmic => TextKey::ToneCurveFilmic,
        ToneMapping::Soft => TextKey::ToneCurveSoft,
    }
}

pub(crate) const fn lighting_preset_label(preset: LightingPreset, locale: Locale) -> &'static str {
    text(locale, preset.label_key())
}

fn about_divider(ui: &mut Ui) {
    ui.add_space(SPACE_2);
    let width = ui.available_width().max(0.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().rect_filled(
        Rect::from_min_max(
            pos2(rect.left(), rect.center().y - 0.5),
            pos2(rect.right(), rect.center().y + 0.5),
        ),
        0.0,
        COLOR_HAIRLINE,
    );
    ui.add_space(SPACE_2);
}

fn about_version_row(ui: &mut Ui, locale: Locale) {
    let running = env!("CARGO_PKG_VERSION");
    let Some(tag) = crate::update_check::newer_release() else {
        about_row(ui, "Version", running);
        return;
    };
    let (row, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), ABOUT_ROW_HEIGHT), Sense::hover());
    ui.painter().text(
        pos2(row.left(), row.center().y),
        egui::Align2::LEFT_CENTER,
        "Version",
        egui::FontId::proportional(FONT_SM),
        COLOR_MUTED,
    );
    let value = ui.painter().text(
        pos2(row.right(), row.center().y),
        egui::Align2::RIGHT_CENTER,
        running,
        egui::FontId::proportional(FONT_SM),
        COLOR_TEXT,
    );

    let label = text(locale, TextKey::UpdateAvailable);
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(FONT_XS),
        COLOR_TEXT,
    );
    let capsule = Rect::from_min_size(
        pos2(
            value.left() - SPACE_2 - (ABOUT_UPDATE_ICON + galley.size().x + SPACE_3),
            row.center().y - ABOUT_UPDATE_HEIGHT * 0.5,
        ),
        vec2(
            ABOUT_UPDATE_ICON + galley.size().x + SPACE_3,
            ABOUT_UPDATE_HEIGHT,
        ),
    );
    let response = ui.interact(capsule, Id::new("vkit.about.update"), Sense::click());
    let hovered = response.hovered();
    ui.painter().rect_filled(
        capsule,
        capsule.height() * 0.5,
        if hovered {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE_RAISED
        },
    );
    let ink = if hovered { COLOR_TEXT } else { COLOR_MUTED };
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            pos2(capsule.left() + SPACE_2 + 5.0, capsule.center().y),
            Vec2::splat(11.0),
        ),
        Icon::UpdateAvailable,
        ink,
    );
    ui.painter().galley(
        pos2(
            capsule.left() + ABOUT_UPDATE_ICON,
            capsule.center().y - galley.size().y * 0.5,
        ),
        galley,
        ink,
    );

    let response = response.on_hover_text(format!("{} \u{2192} {tag}", crate::APP_TITLE));
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        open_with_shell(crate::update_check::RELEASES_PAGE);
    }
}

const ABOUT_UPDATE_HEIGHT: f32 = 18.0;

const ABOUT_UPDATE_ICON: f32 = SPACE_2 + 13.0;

fn about_row(ui: &mut Ui, label: &str, value: &str) {
    let (row, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), ABOUT_ROW_HEIGHT), Sense::hover());
    ui.painter().text(
        pos2(row.left(), row.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(FONT_SM),
        COLOR_MUTED,
    );
    ui.painter().text(
        pos2(row.right(), row.center().y),
        egui::Align2::RIGHT_CENTER,
        value,
        egui::FontId::proportional(FONT_SM),
        COLOR_TEXT,
    );
}

pub(crate) fn open_with_shell(target: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let opened = std::process::Command::new("cmd")
            .args(["/C", "start", "", target])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
        if let Err(error) = opened {
            let _ = crate::diagnostics::record(
                crate::diagnostics::Severity::Warning,
                "settings",
                "open_target_failed",
                &format!("{target}: {error}"),
            );
        }
    }
    #[cfg(not(windows))]
    let _ = target;
}

fn issue_tracker_url() -> String {
    format!("{}/issues", crate::REPOSITORY_URL.trim_end_matches('/'))
}

fn about_paragraph(ui: &mut Ui, body: &str) {
    ui.label(egui::RichText::new(body).size(FONT_XS).color(COLOR_MUTED));
}

fn draw_about_settings(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;

    let header = SUPPORT_MARK_SLOT.max(FONT_HEADING + SPACE_1 + FONT_XS);
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), header),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.vertical(|ui| {
                ui.label(
                    egui::RichText::new(crate::APP_TITLE)
                        .size(FONT_HEADING)
                        .color(COLOR_TEXT),
                );
                about_paragraph(ui, text(locale, TextKey::AboutTagline));
            });
            if !crate::SUPPORT_URL.is_empty() {
                crate::ui_components::right_aligned_row(ui, SUPPORT_MARK_SLOT, |ui| {
                    if about_mark(
                        ui,
                        "support",
                        SUPPORT_MARK_SIZE,
                        SUPPORT_MARK_SLOT,
                        text(locale, TextKey::AboutSupport),
                        |ui, rect, eased| match crate::logo_art::texture(
                            ui.ctx(),
                            crate::logo_art::Logo::KoFi,
                        ) {
                            Some(handle) => {
                                let tint = egui::Color32::WHITE.gamma_multiply(0.82 + 0.18 * eased);
                                egui::Image::new(&handle).tint(tint).paint_at(ui, rect);
                            }
                            None => paint_icon(
                                ui.painter(),
                                rect,
                                Icon::Coffee,
                                COLOR_MUTED.lerp_to_gamma(COLOR_TEXT, eased),
                            ),
                        },
                    ) {
                        open_with_shell(crate::SUPPORT_URL);
                    }
                });
            }
        },
    );

    group_heading(ui, locale, TextKey::SettingsAboutBuild);
    about_version_row(ui, locale);
    about_row(ui, "Built for VaM", crate::VAM_TARGET_VERSION);
    about_row(ui, "Renderer", "wgpu \u{00b7} Direct3D 12");
    about_row(ui, "Platform", "Windows x64");

    group_heading(ui, locale, TextKey::SettingsAboutLicense);
    about_row(ui, "License", "MIT OR Apache-2.0");
    ui.add_space(SPACE_1);
    about_paragraph(ui, text(locale, TextKey::AboutNoAffiliation));
    ui.add_space(SPACE_1);
    about_paragraph(ui, text(locale, TextKey::AboutNoWarranty));

    group_heading(ui, locale, TextKey::SettingsAboutDiagnostics);
    about_row(ui, "Log", crate::diagnostics::LOG_FILE_NAME);

    about_row(
        ui,
        "Previous run",
        crate::diagnostics::PREVIOUS_LOG_FILE_NAME,
    );
    about_row(
        ui,
        "Crash report",
        crate::diagnostics::CRASH_REPORT_FILE_NAME,
    );
    about_row(ui, "Folder", LOG_FOLDER_DISPLAY);
    ui.add_space(SPACE_1);
    about_paragraph(ui, text(locale, TextKey::AboutReportProblem));

    about_marks(ui, locale);
}

const LOG_FOLDER_DISPLAY: &str = r"%LOCALAPPDATA%\Vkit\logs";

fn about_marks(ui: &mut Ui, locale: Locale) {
    let mut marks: Vec<(&'static str, &'static str, Icon, String)> = Vec::new();
    if let Ok(folder) = crate::diagnostics::log_directory() {
        marks.push((
            "logs",
            text(locale, TextKey::AboutOpenLogs),
            Icon::Folder,
            folder.display().to_string(),
        ));
    }
    if !crate::REPOSITORY_URL.is_empty() {
        marks.push((
            "issues",
            text(locale, TextKey::AboutReportIssue),
            Icon::Caution,
            issue_tracker_url(),
        ));
        marks.push((
            "repository",
            "GitHub",
            Icon::GitHub,
            crate::REPOSITORY_URL.to_owned(),
        ));
    }

    if marks.is_empty() {
        return;
    }
    about_divider(ui);

    let (row, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), SOURCE_MARK_SLOT), Sense::hover());
    let span = SOURCE_MARK_SLOT * marks.len() as f32 + SPACE_3 * (marks.len() - 1) as f32;
    let mut left = row.center().x - span * 0.5;
    for (id, tooltip, icon, target) in marks {
        let mut cell = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(Rect::from_min_size(
                    pos2(left, row.top()),
                    Vec2::splat(SOURCE_MARK_SLOT),
                ))
                .layout(Layout::top_down(Align::Center)),
        );
        if about_mark(
            &mut cell,
            id,
            SOURCE_MARK_SIZE,
            SOURCE_MARK_SLOT,
            tooltip,
            |ui, rect, eased| {
                let color = COLOR_MUTED.lerp_to_gamma(COLOR_TEXT, eased);
                paint_icon(ui.painter(), rect, icon, color);
            },
        ) {
            open_with_shell(&target);
        }
        left += SOURCE_MARK_SLOT + SPACE_3;
    }
}

fn about_mark(
    ui: &mut Ui,
    id: &str,
    size: f32,
    slot: f32,
    tooltip: &str,
    draw: impl FnOnce(&mut Ui, Rect, f32),
) -> bool {
    let (bounds, response) = ui.allocate_exact_size(Vec2::splat(slot), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(
        Id::new(("vkit.about.mark", id)),
        response.hovered(),
        MARK_GROW_SECONDS,
    );

    let eased = hover * hover * (3.0 - 2.0 * hover);
    let drawn = size * (1.0 + MARK_GROW * eased);
    draw(
        ui,
        Rect::from_center_size(bounds.center(), Vec2::splat(drawn)),
        eased,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response.on_hover_text(tooltip).clicked()
}

pub fn draw_settings_button(ui: &mut Ui, state: &mut AppState, rect: Rect) {
    let response = ui.interact(rect, Id::new("vkit.settings.button"), Sense::click());
    if response.hovered() || state.settings_open {
        ui.painter()
            .rect_filled(rect, CONTROL_RADIUS, COLOR_SURFACE_HOVER);
    }
    crate::ui_components::paint_icon(
        ui.painter(),
        rect.shrink(4.0),
        crate::ui_components::Icon::Settings,
        if response.hovered() || state.settings_open {
            COLOR_TEXT
        } else {
            COLOR_MUTED
        },
    );
    if tooltip(
        response,
        text(state.locale, TextKey::Settings),
        crate::ui_components::NO_SHORTCUT,
    )
    .clicked()
    {
        state.dispatch(if state.settings_open {
            Action::CloseSettings
        } else {
            Action::OpenSettings
        });
    }
}

const CAPTURE_ID: &str = "vkit.settings.shortcut-capture";

fn capturing(ui: &Ui) -> Option<Shortcut> {
    ui.data(|data| data.get_temp::<Shortcut>(Id::new(CAPTURE_ID)))
}

const fn is_modifier_key(key: egui::Key) -> bool {
    matches!(
        key,
        egui::Key::ShiftLeft
            | egui::Key::ShiftRight
            | egui::Key::ControlLeft
            | egui::Key::ControlRight
            | egui::Key::AltLeft
            | egui::Key::AltRight
            | egui::Key::SuperLeft
            | egui::Key::SuperRight
    )
}

fn captured_binding(ui: &Ui) -> Option<Binding> {
    ui.input(|input| {
        let modifiers = if input.modifiers.command {
            ModifierPolicy::Exactly(egui::Modifiers::COMMAND)
        } else if input.modifiers.shift {
            ModifierPolicy::Exactly(egui::Modifiers::SHIFT)
        } else if input.modifiers.alt {
            ModifierPolicy::Exactly(egui::Modifiers::ALT)
        } else {
            ModifierPolicy::Exactly(egui::Modifiers::NONE)
        };
        let key = input.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                ..
            } if *key != egui::Key::Escape && !is_modifier_key(*key) => Some(Trigger::Key(*key)),
            _ => None,
        });
        let left = (!input.modifiers.is_none())
            .then_some(egui::PointerButton::Primary)
            .into_iter();
        let mouse = [
            egui::PointerButton::Secondary,
            egui::PointerButton::Middle,
            egui::PointerButton::Extra1,
            egui::PointerButton::Extra2,
        ]
        .into_iter()
        .chain(left)
        .find(|button| input.pointer.button_pressed(*button))
        .map(Trigger::Mouse);
        key.or(mouse).map(|trigger| Binding { trigger, modifiers })
    })
}

/// Which group of the shortcut list a binding belongs to.
///
/// The contexts already exist — the keymap uses them to decide which bindings
/// can share a key — so the list is grouped by the same answer rather than by a
/// second opinion about where a shortcut belongs.
const fn context_heading(context: crate::shortcuts::ShortcutContext) -> TextKey {
    match context {
        crate::shortcuts::ShortcutContext::Global => TextKey::ShortcutGroupSystem,
        crate::shortcuts::ShortcutContext::Alignment => TextKey::ShortcutGroupAlignment,
        crate::shortcuts::ShortcutContext::DetailEdit => TextKey::ShortcutGroupSculpt,
        crate::shortcuts::ShortcutContext::HairEdit => TextKey::ShortcutGroupHair,
    }
}

/// System first because it is true everywhere, then the three places a binding
/// only means something in.
const SHORTCUT_GROUPS: [crate::shortcuts::ShortcutContext; 4] = [
    crate::shortcuts::ShortcutContext::Global,
    crate::shortcuts::ShortcutContext::Alignment,
    crate::shortcuts::ShortcutContext::DetailEdit,
    crate::shortcuts::ShortcutContext::HairEdit,
];

/// Reset, save and load, as icons at the top right.
///
/// They act on the whole keymap rather than on any one binding, so they belong
/// above the list rather than after the last row of it, where they read as
/// though they belonged to whatever binding happened to be last.
/// Import, export and reset, as icons at the top right of the shortcut list.
fn draw_keymap_actions(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;
    let height = crate::ui_components::icon_button_size(ui);
    crate::ui_components::right_aligned_row(ui, height, |ui| {
        if crate::ui_components::icon_button(
            ui,
            crate::ui_components::Icon::Folder,
            text(locale, TextKey::ShortcutsImport),
        )
        .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("JSON", &["json"])
                .pick_file()
            && let Ok(body) = std::fs::read_to_string(path)
            && let Ok(stored) =
                serde_json::from_str::<std::collections::BTreeMap<String, String>>(&body)
        {
            let loaded = crate::shortcuts::Keymap::from_stored(&stored);
            for shortcut in Shortcut::ALL {
                state.dispatch(Action::RebindShortcut(shortcut, loaded.binding(shortcut)));
            }
        }
        if crate::ui_components::icon_button(
            ui,
            crate::ui_components::Icon::Save,
            text(locale, TextKey::ShortcutsExport),
        )
        .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("JSON", &["json"])
                .set_file_name("vkit-keymap.json")
                .save_file()
            && let Err(detail) = export_keymap(&path, &state.keymap)
        {
            state.status = crate::state::StatusMessage::with_detail(
                TextKey::ExportFailed,
                crate::state::StatusTone::Error,
                detail,
            );
        }
        if crate::ui_components::icon_button(
            ui,
            crate::ui_components::Icon::Refresh,
            text(locale, TextKey::ShortcutsResetAll),
        )
        .clicked()
        {
            state.dispatch(Action::ResetKeymap);
        }
    });
}

fn draw_shortcut_settings(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;
    let armed = capturing(ui);

    let escaped = armed.is_some()
        && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escaped {
        ui.data_mut(|data| data.remove::<Shortcut>(Id::new(CAPTURE_ID)));
    } else if let (Some(shortcut), Some(binding)) = (armed, captured_binding(ui)) {
        ui.data_mut(|data| data.remove::<Shortcut>(Id::new(CAPTURE_ID)));
        if state.keymap.conflict(shortcut, binding).is_none() {
            state.dispatch(Action::RebindShortcut(shortcut, binding));
        } else {
            state.status = crate::state::StatusMessage::new(
                TextKey::ShortcutsTaken,
                crate::state::StatusTone::Warning,
            );
        }
    }

    draw_keymap_actions(ui, state);

    for context in SHORTCUT_GROUPS {
        let mut named = false;
        for shortcut in Shortcut::ALL {
            if shortcut.context() != context {
                continue;
            }
            if !named {
                group_heading(ui, locale, context_heading(context));
                named = true;
            }
            let binding = state.keymap.binding(shortcut);
            let waiting = capturing(ui) == Some(shortcut);
            setting_row(ui, locale, shortcut_label(shortcut), None, |ui| {
                if !state.keymap.is_default(shortcut)
                    && ui
                        .add(egui::Button::new(text(locale, TextKey::Reset)).small())
                        .clicked()
                {
                    state.dispatch(Action::RebindShortcut(shortcut, shortcut.default_binding()));
                }
                let caption = if waiting {
                    text(locale, TextKey::ShortcutsCapturing).to_owned()
                } else {
                    binding.label()
                };
                if ui.add(egui::Button::new(caption)).clicked() {
                    ui.data_mut(|data| data.insert_temp(Id::new(CAPTURE_ID), shortcut));
                }
            });
        }
    }
}
fn export_keymap(path: &std::path::Path, keymap: &crate::shortcuts::Keymap) -> Result<(), String> {
    let body = serde_json::to_string_pretty(&keymap.to_stored())
        .map_err(|error| format!("{}: {error}", path.display()))?;
    std::fs::write(path, body).map_err(|error| format!("{}: {error}", path.display()))
}

const fn shortcut_label(shortcut: Shortcut) -> TextKey {
    match shortcut {
        Shortcut::SculptGrabBrush => TextKey::SculptGrab,
        Shortcut::SculptRestoreBrush => TextKey::SculptBrushRestore,
        Shortcut::HairCombBrush => TextKey::HairToolComb,
        Shortcut::HairPlantTool => TextKey::HairToolPlant,
        Shortcut::HairGrowTool => TextKey::HairToolGrow,
        Shortcut::HairCutTool => TextKey::HairToolCut,
        Shortcut::HairEraseTool => TextKey::HairToolErase,
        Shortcut::HairMirrorPart => TextKey::HairMirrorPart,
        Shortcut::HairPuffTool => TextKey::HairToolPuff,
        Shortcut::HairPinchTool => TextKey::HairToolPinch,
        Shortcut::HairPickTool => TextKey::HairToolPick,
        Shortcut::TexturePinBrush => TextKey::TextureToolPinPair,
        Shortcut::TextureCloneBrush => TextKey::TextureToolClone,
        Shortcut::BrushSizeDown => TextKey::ShortcutBrushSmaller,
        Shortcut::BrushSizeUp => TextKey::ShortcutBrushLarger,
        Shortcut::Undo => TextKey::HelpUndo,
        Shortcut::Redo => TextKey::HelpRedo,
        Shortcut::BrushSizeSweep => TextKey::ShortcutBrushSizeDrag,
        Shortcut::BrushStrengthSweep => TextKey::ShortcutBrushStrengthDrag,
        Shortcut::ViewTrackball => TextKey::HelpTrackball,
        Shortcut::ViewLevelRoll => TextKey::HelpLevelRoll,
        Shortcut::CancelStencil => TextKey::ShortcutStencilCancel,
        Shortcut::FrameSelected => TextKey::HelpFrameView,
        Shortcut::XSymmetry => TextKey::HelpXSymmetry,
        Shortcut::ViewOrbit => TextKey::ShortcutViewOrbit,
        Shortcut::ViewPan => TextKey::ShortcutViewPan,
        Shortcut::ViewDolly => TextKey::ShortcutViewDolly,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A section's height must come from its content, never from the pane it
    /// happens to be drawn in.
    ///
    /// A horizontal layout whose cross axis is `Align::Center`, handed an
    /// unbounded parent, claims the parent's whole height and centres its
    /// contents inside it. That is how three icons at the top of the shortcut
    /// list came to sit in the middle of a page-tall blank: measured, one 18px
    /// button in a 600px pane produced a 600px row.
    ///
    /// Nothing at the call site tells the two cases apart — the expression is
    /// the same, and only the parent's height decides. So the parent's height
    /// is what this test varies.
    #[test]
    fn no_settings_section_is_sized_by_the_pane_it_is_drawn_in() {
        for section in SettingsSection::ALL {
            let mut measured = Vec::new();
            for pane in [360.0_f32, 2400.0] {
                egui::__run_test_ui(|ui| {
                    let mut state = AppState::default();
                    let cell = Rect::from_min_size(ui.cursor().min, vec2(420.0, pane));
                    let mut child = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(cell)
                            .layout(Layout::top_down(Align::Min)),
                    );
                    draw_section_body(&mut child, &mut state, section);
                    measured.push(child.min_rect().height());
                });
            }
            let (small, large) = (measured[0], measured[1]);
            assert!(
                (small - large).abs() < 1.0,
                "{section:?}: {small:.0}px in a 360px pane, {large:.0}px in a 2400px one \n                 — something in it takes its size from the container",
            );
        }
    }

    #[test]
    fn every_section_is_named_in_every_language() {
        for section in SettingsSection::ALL {
            for locale in Locale::ALL {
                assert!(
                    !text(locale, section.label()).trim().is_empty(),
                    "{section:?} has no name in {locale:?}"
                );
            }
        }
    }

    /// The graphics page is three sections down one page, in the order they are
    /// reached for. Each is a heading, and a heading with no name is a gap.
    #[test]
    fn every_graphics_section_is_named_in_every_language() {
        for key in [
            TextKey::SettingsGraphicsQuality,
            TextKey::SettingsGraphicsEffects,
            TextKey::SettingsGraphicsLighting,
        ] {
            for locale in Locale::ALL {
                assert!(
                    !text(locale, key).trim().is_empty(),
                    "{key:?} has no name in {locale:?}"
                );
            }
        }
    }

    #[test]
    fn a_keymap_export_round_trips_the_bindings_it_was_given() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let path = workspace.path().join("vkit-keymap.json");
        let keymap = crate::shortcuts::Keymap::default();
        export_keymap(&path, &keymap).expect("a writable path is the ordinary case");
        let body = std::fs::read_to_string(&path).expect("the export exists");
        let stored = serde_json::from_str::<std::collections::BTreeMap<String, String>>(&body)
            .expect("the export is the map the importer reads back");
        assert_eq!(stored, keymap.to_stored());
    }

    #[test]
    fn a_keymap_export_that_cannot_be_written_says_so_instead_of_nothing() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let path = workspace.path().join("vkit-keymap.json");
        std::fs::create_dir(&path).expect("occupy the destination");
        let detail = export_keymap(&path, &crate::shortcuts::Keymap::default())
            .expect_err("a path the OS refuses may not be reported as a saved file");
        assert!(detail.contains("vkit-keymap.json"), "{detail}");
        for locale in Locale::ALL {
            assert!(
                !text(locale, TextKey::ExportFailed).trim().is_empty(),
                "the channel this detail is shown through has no heading in {locale:?}"
            );
        }
    }

    #[test]
    fn an_effect_switch_never_repeats_its_own_heading() {
        for locale in Locale::ALL {
            let enabled = text(locale, TextKey::SettingsEffectEnabled);
            let heading = TextKey::SettingsGraphicsEffects;
            assert_ne!(enabled, text(locale, heading), "{heading:?} in {locale:?}");
        }
    }

    #[test]
    fn the_about_pane_paints_in_every_language() {
        for locale in Locale::ALL {
            let mut state = AppState::default();
            state.locale = locale;
            state.settings_open = true;
            state.settings_section = SettingsSection::About;
            let context = egui::Context::default();
            let input = || egui::RawInput {
                screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1600.0, 1000.0))),
                ..Default::default()
            };

            let _ = context.run_ui(input(), |root| draw_settings_page(root, &mut state));
            let _ = context.run_ui(input(), |root| draw_settings_page(root, &mut state));
            assert!(state.settings_open, "{locale:?} closed its own page");
        }
    }

    #[test]
    fn nothing_writes_this_programs_own_version_by_hand() {
        let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let dotted_number = |text: &str| {
            text.split(|character: char| !character.is_ascii_digit() && character != '.')
                .any(|token| token.split('.').filter(|part| !part.is_empty()).count() >= 3)
        };
        for relative in ["resources/windows.rc.in", "resources/vkit.manifest.in"] {
            let path = crate_root.join(relative);
            let contents = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            for (index, line) in contents.lines().enumerate() {
                if line.contains("http") || line.contains("supportedOS") {
                    continue;
                }
                assert!(
                    !dotted_number(line),
                    "{relative}:{} writes a version by hand: {line}",
                    index + 1
                );
            }
            assert!(
                contents.contains("@VERSION"),
                "{relative} should carry a version token"
            );
        }

        let main_source = std::fs::read_to_string(crate_root.join("src/main.rs")).unwrap();
        assert!(
            main_source.contains(r#"concat!("Vkit V", env!("CARGO_PKG_VERSION"))"#),
            "the window title should be built from the package version"
        );
    }

    #[test]
    fn the_issue_tracker_is_the_repository_plus_one_segment() {
        assert_eq!(
            issue_tracker_url(),
            format!("{}/issues", crate::REPOSITORY_URL.trim_end_matches('/'))
        );
        assert!(!issue_tracker_url().contains("//issues"));
    }

    #[test]
    fn the_about_page_names_the_files_a_report_needs() {
        assert_eq!(crate::diagnostics::LOG_FILE_NAME, "vkit.log");
        assert_eq!(crate::diagnostics::CRASH_REPORT_FILE_NAME, "crash.log");

        assert!(LOG_FOLDER_DISPLAY.ends_with(r"\logs"));
        if let Ok(resolved) = crate::diagnostics::log_directory() {
            assert!(resolved.ends_with("logs"));
        }
    }

    #[test]
    fn the_section_list_covers_the_enum() {
        assert_eq!(SettingsSection::ALL.len(), 5);
        let mut seen = SettingsSection::ALL.to_vec();
        seen.dedup();
        assert_eq!(seen.len(), SettingsSection::ALL.len());
    }
}

#[cfg(test)]
mod shortcut_group_tests {
    use super::*;

    /// Every binding lands in exactly one group, and every group is named.
    /// A shortcut whose context has no heading would simply not be listed.
    #[test]
    fn every_shortcut_falls_into_a_named_group() {
        for shortcut in Shortcut::ALL {
            let context = shortcut.context();
            assert!(
                SHORTCUT_GROUPS.contains(&context),
                "{shortcut:?} is in {context:?}, which the list does not show",
            );
            for locale in Locale::ALL {
                assert!(
                    !text(locale, context_heading(context)).trim().is_empty(),
                    "{context:?} has no name in {locale:?}",
                );
            }
        }
    }

    /// The groups are the keymap's own contexts, so a binding cannot be filed
    /// in one place and share a key according to another.
    #[test]
    fn the_groups_are_the_contexts_the_keymap_already_uses() {
        let mut seen = Vec::new();
        for context in SHORTCUT_GROUPS {
            assert!(!seen.contains(&context), "{context:?} is listed twice");
            seen.push(context);
        }
        assert_eq!(
            seen.len(),
            4,
            "a context with no group would drop its bindings off the page",
        );
    }
}
