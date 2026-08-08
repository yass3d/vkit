use egui::{Align, Frame, Id, Layout, Margin, Rect, Sense, Stroke, Ui, Vec2, pos2, vec2};

use crate::{
    i18n::{Locale, TextKey, text},
    lighting::LightingPreset,
    shader_color::ToneMapping,
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

    About,
}

impl SettingsSection {
    pub const ALL: [Self; 4] = [Self::Graphics, Self::Viewport, Self::General, Self::About];

    const fn label(self) -> TextKey {
        match self {
            Self::Graphics => TextKey::SettingsGraphics,
            Self::Viewport => TextKey::SettingsViewport,
            Self::General => TextKey::SettingsGeneral,
            Self::About => TextKey::SettingsAbout,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GraphicsPage {
    #[default]
    Lighting,
    Effects,
}

impl GraphicsPage {
    const ALL: [Self; 2] = [Self::Lighting, Self::Effects];

    const fn label(self) -> TextKey {
        match self {
            Self::Lighting => TextKey::SettingsGraphicsLighting,
            Self::Effects => TextKey::SettingsGraphicsEffects,
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

fn draw_section_pane(ui: &mut Ui, state: &mut AppState, rect: Rect, section: SettingsSection) {
    let mut pane = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink(SPACE_4))
            .layout(Layout::top_down(Align::Min)),
    );
    pane.spacing_mut().item_spacing.y = SPACE_2;
    egui::ScrollArea::vertical()
        .id_salt("vkit.settings.pane")
        .auto_shrink([false, false])
        .show(&mut pane, |ui| match section {
            SettingsSection::Graphics => draw_graphics_settings(ui, state),
            SettingsSection::Viewport => draw_viewport_settings(ui, state),
            SettingsSection::General => draw_general_settings(ui, state),
            SettingsSection::About => draw_about_settings(ui, state),
        });
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
    tooltip(row, text(locale, description), None)
}

fn group_heading(ui: &mut Ui, locale: Locale, key: TextKey) {
    ui.add_space(SPACE_3);
    ui.label(
        egui::RichText::new(text(locale, key))
            .size(FONT_XS)
            .color(COLOR_MUTED),
    );
    ui.add_space(SPACE_2);
}

fn draw_graphics_settings(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;
    let mut page = state.settings_graphics_page;
    crate::ui_components::animated_segmented_group(
        ui,
        "vkit.settings.graphics-page",
        GraphicsPage::ALL.len(),
        GraphicsPage::ALL
            .iter()
            .position(|candidate| *candidate == page)
            .unwrap_or(0),
        |ui, segment_width| {
            for candidate in GraphicsPage::ALL {
                if crate::ui_components::segment_button(
                    ui,
                    segment_width,
                    text(locale, candidate.label()),
                    candidate == page,
                )
                .clicked()
                {
                    page = candidate;
                }
            }
        },
    );
    state.settings_graphics_page = page;
    match page {
        GraphicsPage::Lighting => draw_lighting_settings(ui, state),
        GraphicsPage::Effects => draw_effect_settings(ui, state),
    }
}

fn draw_lighting_settings(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;

    group_heading(ui, locale, TextKey::SettingsLightingGroup);
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
    tooltip(row, text(locale, TextKey::SettingsToneCurveTooltip), None);
    if curve != state.tone_mapping {
        state.dispatch(Action::SetToneMapping(curve));
    }
}

fn draw_effect_settings(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;

    group_heading(ui, locale, TextKey::SettingsOcclusionGroup);
    let mut occlusion = state.ambient_occlusion;
    let occlusion_switch = effect_switch(
        ui,
        locale,
        &mut occlusion.enabled,
        TextKey::SettingsOcclusionTooltip,
    );
    let mut occlusion_changed = occlusion_switch.changed();
    ui.add_enabled_ui(occlusion.enabled, |ui| {
        for (key, value, range) in [
            (
                TextKey::SettingsOcclusionIntensity,
                &mut occlusion.intensity,
                crate::ambient_occlusion::INTENSITY_RANGE,
            ),
            (
                TextKey::SettingsOcclusionRadius,
                &mut occlusion.radius,
                crate::ambient_occlusion::RADIUS_RANGE,
            ),
        ] {
            occlusion_changed |= setting_row(ui, locale, key, None, |ui| {
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
    if occlusion_changed {
        state.dispatch(Action::SetAmbientOcclusion(occlusion));
    }

    group_heading(ui, locale, TextKey::SettingsBloomGroup);
    let mut bloom = state.bloom;
    let mut bloom_changed = crate::ui_components::switch_row(
        ui,
        &mut bloom.enabled,
        text(locale, TextKey::SettingsEffectEnabled),
    )
    .changed();
    ui.add_enabled_ui(bloom.enabled, |ui| {
        for (key, value, range, percent) in [
            (
                TextKey::SettingsBloomIntensity,
                &mut bloom.intensity,
                crate::post_process::INTENSITY_RANGE,
                false,
            ),
            (
                TextKey::SettingsBloomThreshold,
                &mut bloom.threshold,
                crate::post_process::THRESHOLD_RANGE,
                false,
            ),
            (
                TextKey::SettingsBloomSoftKnee,
                &mut bloom.soft_knee,
                crate::post_process::SOFT_KNEE_RANGE,
                true,
            ),
            (
                TextKey::SettingsBloomRadius,
                &mut bloom.radius,
                crate::post_process::RADIUS_RANGE,
                false,
            ),
        ] {
            bloom_changed |= setting_row(ui, locale, key, None, |ui| {
                let slider = FilledNumericSlider::new(value, range).min_width(CONTROL_COLUMN_WIDTH);

                let slider = if percent {
                    slider.percent().decimals(0)
                } else {
                    slider.decimals(2)
                };
                ui.add(slider).changed()
            });
        }
    });
    if bloom_changed {
        state.dispatch(Action::SetBloom(bloom));
    }

    group_heading(ui, locale, TextKey::SettingsVignetteGroup);
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

    group_heading(ui, locale, TextKey::SettingsGeometryGroup);
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
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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

    about_divider(ui);

    group_heading(ui, locale, TextKey::SettingsAboutBuild);
    about_row(ui, "Version", env!("CARGO_PKG_VERSION"));
    about_row(ui, "Built for VaM", crate::VAM_TARGET_VERSION);
    about_row(ui, "Renderer", "wgpu \u{00b7} Direct3D 12");
    about_row(ui, "Platform", "Windows x64");

    about_divider(ui);

    group_heading(ui, locale, TextKey::SettingsAboutLicense);
    about_row(ui, "License", "MIT OR Apache-2.0");
    ui.add_space(SPACE_1);
    about_paragraph(ui, text(locale, TextKey::AboutNoAffiliation));
    ui.add_space(SPACE_1);
    about_paragraph(ui, text(locale, TextKey::AboutNoWarranty));

    about_divider(ui);

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
    if tooltip(response, text(state.locale, TextKey::Settings), None).clicked() {
        state.dispatch(if state.settings_open {
            Action::CloseSettings
        } else {
            Action::OpenSettings
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn every_graphics_page_is_named_in_every_language() {
        for page in GraphicsPage::ALL {
            for locale in Locale::ALL {
                assert!(
                    !text(locale, page.label()).trim().is_empty(),
                    "{page:?} has no name in {locale:?}"
                );
            }
        }
    }

    #[test]
    fn an_effect_switch_never_repeats_its_own_heading() {
        for locale in Locale::ALL {
            let enabled = text(locale, TextKey::SettingsEffectEnabled);
            for heading in [
                TextKey::SettingsOcclusionGroup,
                TextKey::SettingsBloomGroup,
                TextKey::SettingsVignetteGroup,
            ] {
                assert_ne!(enabled, text(locale, heading), "{heading:?} in {locale:?}");
            }
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
        assert_eq!(SettingsSection::ALL.len(), 4);
        let mut seen = SettingsSection::ALL.to_vec();
        seen.dedup();
        assert_eq!(seen.len(), SettingsSection::ALL.len());
    }
}
