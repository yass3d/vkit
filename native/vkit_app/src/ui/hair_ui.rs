use egui::{RichText, Sense, Ui, vec2};

use crate::hair_project::HairPart;
use crate::hair_settings::{HairParamGroup, HairParamKind, params_in};
use crate::i18n::{TextKey, text};
use crate::state::{Action, AppState};
use crate::theme::{COLOR_FIELD, COLOR_MUTED, COLOR_TEXT, FONT_SM, SPACE_2, SPACE_3};
use crate::ui::{capsule_action, capsule_metadata_field};
use crate::ui_components::{Icon, icon_button};

const HAIR_VALUE_LANE: f32 = 56.0;
pub(crate) fn draw_hair_inspector(ui: &mut Ui, state: &mut AppState) {
    crate::ui::show_inspector_shell(
        ui,
        state,
        "hair.inspector",
        crate::ui::SINGLE_ACTION_FOOTER_HEIGHT,
        true,
        |ui, state, _viewport| {
            ui.add_enabled_ui(!state.busy(), |ui| {
                use crate::hair_project::HairPanelPage;

                let pages = [
                    (HairPanelPage::Parts, TextKey::HairPartsPanel),
                    (HairPanelPage::Settings, TextKey::HairSettingsPanel),
                    (HairPanelPage::Scalp, TextKey::HairScalpPanel),
                ];
                let current = state.hair_panel_page;
                let index = pages
                    .iter()
                    .position(|(page, _)| *page == current)
                    .unwrap_or(0);
                let mut flip = None;
                crate::ui_components::animated_segmented_group(
                    ui,
                    "vkit.hair.subview",
                    pages.len(),
                    index,
                    |ui, segment_width| {
                        for (page, key) in pages {
                            if crate::ui_components::segment_button(
                                ui,
                                segment_width,
                                text(state.locale, key),
                                page == current,
                            )
                            .clicked()
                            {
                                flip = Some(page);
                            }
                        }
                    },
                );
                if let Some(page) = flip {
                    state.hair_panel_page = page;
                }
                ui.add_space(SPACE_3);
                match current {
                    HairPanelPage::Parts => draw_hair_parts_page(ui, state),
                    HairPanelPage::Settings => {
                        if state.hair_project.selected_part().is_some() {
                            draw_parameter_groups(ui, state);
                        } else {
                            ui.label(
                                RichText::new(text(state.locale, TextKey::HairCreateFirst))
                                    .size(FONT_SM)
                                    .color(COLOR_MUTED),
                            );
                        }
                    }
                    HairPanelPage::Scalp => draw_scalp_page(ui, state),
                }
            });
        },
        |ui, state, footer| {
            let enabled = !state.busy()
                && state.tab_available(crate::state::Tab::Result)
                && !state.hair_project.parts.is_empty();
            if crate::ui::footer_primary_button(
                ui,
                crate::ui::primary_action_rect(footer),
                text(state.locale, TextKey::HairExportSection),
                enabled,
            )
            .clicked()
            {
                state.dispatch(Action::SetSaveSection(crate::state::SaveSection::Hair));
                state.dispatch(Action::RequestTab(crate::state::Tab::Result));
            }
        },
    );
}

fn guide_glow(ui: &Ui, rect: egui::Rect, radius: u8) {
    crate::guidance::glow_over(ui.painter(), rect, radius, ui.input(|input| input.time));
    ui.ctx().request_repaint();
}

pub(crate) fn draw_part_thumbnail_slot(ui: &mut Ui, state: &mut AppState, part_id: u64, slot: f32) {
    const ZOOM: f32 = 220.0;
    let (rect, response) = ui.allocate_exact_size(vec2(slot, slot), Sense::click());
    let rect = rect.shrink(1.0);
    let radius = f32::from(crate::theme::RADIUS_S);
    let hovered = response.hovered();
    match state
        .hair_part_thumbnails
        .get(&part_id)
        .and_then(|thumb| part_thumbnail_texture(ui, part_id, thumb))
    {
        Some(handle) => {
            egui::Image::new(egui::load::SizedTexture::new(handle.id(), rect.size()))
                .corner_radius(radius)
                .paint_at(ui, rect);
            if hovered {
                ui.painter()
                    .rect_filled(rect, radius, egui::Color32::from_black_alpha(140));
                crate::ui_components::paint_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(rect.center(), vec2(18.0, 18.0)),
                    Icon::Camera,
                    egui::Color32::WHITE,
                );
            } else {
                response.clone().on_hover_ui(|ui| {
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(
                        handle.id(),
                        vec2(ZOOM, ZOOM),
                    )));
                });
            }
        }
        None => {
            ui.painter().rect_filled(rect, radius, COLOR_FIELD);
            crate::ui_components::paint_icon(
                ui.painter(),
                egui::Rect::from_center_size(rect.center(), vec2(18.0, 18.0)),
                Icon::Camera,
                if hovered {
                    egui::Color32::WHITE
                } else {
                    COLOR_MUTED
                },
            );
        }
    }
    if response
        .on_hover_text(text(state.locale, TextKey::HairThumbnailShoot))
        .clicked()
    {
        state.begin_hair_thumbnail(crate::state::HairThumbnailTarget::Part(part_id));
    }

    if let Some(flash) = state.hair_shot_flash
        && flash.part == Some(part_id)
    {
        let age = ui.input(|input| input.time) - flash.at;
        if (0.0..1.0).contains(&age) {
            ui.ctx().request_repaint();
            let blink = ((age * std::f64::consts::TAU * 3.0).sin() * 0.5 + 0.5) as f32;
            let alpha = (blink * 220.0) as u8;
            ui.painter().rect_stroke(
                rect.expand(2.0),
                4.0,
                egui::Stroke::new(
                    2.0,
                    egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
                ),
                egui::StrokeKind::Outside,
            );
        }
    }
}

fn part_thumbnail_texture(
    ui: &Ui,
    part_id: u64,
    thumb: &crate::state::HairPartThumbnail,
) -> Option<egui::TextureHandle> {
    crate::ui_components::try_stamped_texture(
        ui,
        "vkit.hair.part-thumb",
        part_id,
        thumb.revision,
        egui::TextureOptions::LINEAR,
        || decoded_thumbnail(&thumb.jpeg),
    )
}

fn decoded_thumbnail(jpeg: &[u8]) -> Option<egui::ColorImage> {
    let decoded = image::load_from_memory(jpeg).ok()?.to_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        decoded.as_raw(),
    ))
}

fn draw_export_fields(ui: &mut Ui, state: &mut AppState) {
    use crate::hair_project::HairExportSexes;
    let selected = state.hair_project.export_sexes;
    let choices = [
        (HairExportSexes::Both, TextKey::HairExportBothSexes),
        (HairExportSexes::Female, TextKey::Female),
        (HairExportSexes::Male, TextKey::Male),
    ];
    let index = choices
        .iter()
        .position(|(value, _)| *value == selected)
        .unwrap_or(0);
    let mut picked = None;
    crate::ui_components::animated_segmented_group(
        ui,
        "vkit.hair.export-sexes",
        choices.len(),
        index,
        |ui, width| {
            for (value, key) in choices {
                if crate::ui_components::segment_button(
                    ui,
                    width,
                    text(state.locale, key),
                    value == selected,
                )
                .clicked()
                {
                    picked = Some(value);
                }
            }
        },
    );
    if let Some(value) = picked {
        state.dispatch(Action::SetHairExportSexes(value));
    }
    ui.add_space(SPACE_2);

    let slot_side = crate::theme::CONTROL_HEIGHT * 2.0 + SPACE_2;
    ui.horizontal(|ui| {
        ui.add_space(SPACE_2);
        let (slot, slot_response) =
            ui.allocate_exact_size(vec2(slot_side, slot_side), Sense::click());
        draw_preset_thumbnail_slot(ui, state, slot, slot_response.hovered());
        if slot_response
            .on_hover_text(text(state.locale, TextKey::HairThumbnailShoot))
            .clicked()
        {
            state.begin_hair_thumbnail(crate::state::HairThumbnailTarget::Preset);
        }
        ui.vertical(|ui| {
            ui.set_width(ui.available_width().max(0.0));
            let mut name = state.hair_project.export_name.clone();
            if capsule_metadata_field(
                ui,
                "vkit.hair.export.name",
                &mut name,
                text(state.locale, TextKey::HairExportName),
            ) {
                state.dispatch(Action::SetHairExportName(name));
            }
            ui.add_space(SPACE_2);
            let mut creator = state.hair_project.export_creator.clone();
            if capsule_metadata_field(
                ui,
                "vkit.hair.export.creator",
                &mut creator,
                text(state.locale, TextKey::HairExportCreator),
            ) {
                state.dispatch(Action::SetHairExportCreator(creator));
            }
        });
    });

    if state.hair_export_attempted_blank && export_blocked_on_metadata(state) {
        ui.add_space(SPACE_2);
        ui.label(
            RichText::new(text(state.locale, TextKey::HairExportNeedsMetadata))
                .size(FONT_SM)
                .color(crate::theme::COLOR_DESTRUCTIVE),
        );
    }

    if let Some(notice) = state.hair_export_notice.clone() {
        ui.add_space(SPACE_3);
        ui.label(
            RichText::new(text(state.locale, TextKey::HairExportRescanNotice))
                .size(FONT_SM)
                .color(crate::theme::COLOR_WARNING),
        );
        let label = notice.folder.to_string_lossy().into_owned();
        let link = ui.add(
            egui::Label::new(
                RichText::new(&label)
                    .size(FONT_SM)
                    .color(crate::theme::COLOR_WARNING)
                    .underline(),
            )
            .truncate()
            .sense(Sense::click()),
        );
        if link
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(&label)
            .clicked()
        {
            open_in_file_manager(&notice.folder);
        }
        if notice.overwrote {
            ui.label(
                RichText::new(text(state.locale, TextKey::HairExportOverwroteNotice))
                    .size(FONT_SM)
                    .color(crate::theme::COLOR_WARNING),
            );
        }
    }
}

fn draw_preset_thumbnail_slot(ui: &Ui, state: &AppState, rect: egui::Rect, hovered: bool) {
    let radius = f32::from(crate::theme::RADIUS_M);
    let inner = rect;
    match state.hair_preset_thumbnail.as_ref() {
        Some(thumb) => {
            ui.painter().rect_filled(inner, radius, COLOR_FIELD);
            if let Some(handle) = preset_thumbnail_texture(ui, thumb) {
                egui::Image::new(egui::load::SizedTexture::new(handle.id(), inner.size()))
                    .corner_radius(radius)
                    .paint_at(ui, inner);
            }
            if hovered {
                ui.painter()
                    .rect_filled(inner, radius, egui::Color32::from_black_alpha(140));
                crate::ui_components::paint_icon(
                    ui.painter(),
                    egui::Rect::from_center_size(inner.center(), vec2(24.0, 24.0)),
                    Icon::Camera,
                    egui::Color32::WHITE,
                );
            }
        }
        None => {
            ui.painter().rect_filled(inner, radius, COLOR_FIELD);
            crate::ui_components::paint_icon(
                ui.painter(),
                egui::Rect::from_center_size(inner.center(), vec2(24.0, 24.0)),
                Icon::Camera,
                if hovered {
                    egui::Color32::WHITE
                } else {
                    COLOR_MUTED
                },
            );
        }
    }
}

fn preset_thumbnail_texture(
    ui: &Ui,
    thumb: &crate::state::HairPartThumbnail,
) -> Option<egui::TextureHandle> {
    crate::ui_components::try_stamped_texture(
        ui,
        "vkit.hair.preset-thumb",
        0,
        thumb.revision,
        egui::TextureOptions::LINEAR,
        || decoded_thumbnail(&thumb.jpeg),
    )
}

fn open_in_file_manager(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
    }
}

fn export_blocked_on_metadata(state: &AppState) -> bool {
    state.hair_project.export_name.trim().is_empty()
        || state.hair_project.export_creator.trim().is_empty()
}

const PART_ROW_H: f32 = 52.0;
const PART_LIST_MIN_ROWS: f32 = 3.0;
const PART_LIST_INSET: f32 = 4.0;
const PART_ROW_GAP: f32 = 4.0;
const PART_THUMB: f32 = 44.0;

const PRESET_LIST_HEIGHT: f32 = 148.0;

fn draw_hair_preset_picker(ui: &mut Ui, state: &mut AppState) {
    ui.label(
        RichText::new(text(state.locale, TextKey::HairPresetSection))
            .size(FONT_SM)
            .color(COLOR_MUTED),
    );
    ui.add_space(SPACE_2);

    let indexing = state.busy();
    let has_root = state.vam_root.is_some();
    let mut query = state.hair_query.clone();
    let mut refresh = false;
    ui.horizontal(|ui| {
        let button = crate::ui_components::icon_button_size(ui);
        let field = (ui.available_width() - button - SPACE_2).max(0.0);
        ui.allocate_ui_with_layout(
            vec2(field, crate::theme::CONTROL_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                if crate::ui::capsule_search_field(
                    ui,
                    "vkit.hair.preset-search",
                    &mut query,
                    text(state.locale, TextKey::HairSearch),
                    has_root,
                )
                .changed()
                {
                    state.hair_query = query.clone();
                }
            },
        );
        refresh = ui
            .add_enabled_ui(has_root && !indexing, |ui| {
                crate::ui_components::icon_button(
                    ui,
                    Icon::Refresh,
                    text(state.locale, TextKey::RefreshSkins),
                )
            })
            .inner
            .clicked();
    });
    if refresh {
        state.dispatch(Action::RefreshVaMCatalog);
    }

    ui.add_space(SPACE_2);
    let needle = state.hair_query.trim().to_ascii_lowercase();
    let matching: Vec<usize> = state
        .vam_hair_presets
        .iter()
        .enumerate()
        .filter_map(|(index, preset)| {
            let compatible = preset
                .sex
                .is_compatible_with(state.figure_sex.skin_sex(), false);
            let matches = needle.is_empty()
                || preset.label.to_ascii_lowercase().contains(&needle)
                || preset.stable_id.to_ascii_lowercase().contains(&needle);
            (compatible && matches).then_some(index)
        })
        .collect();

    let mut picked = state.hair_preset_pick.clone();
    egui::Frame::new()
        .fill(COLOR_FIELD)
        .corner_radius(crate::theme::CONTROL_RADIUS)
        .inner_margin(egui::Margin::same(PART_LIST_INSET as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width().max(0.0));
            egui::ScrollArea::vertical()
                .id_salt("vkit.hair.preset-list")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .max_height(PRESET_LIST_HEIGHT)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().max(0.0));
                    ui.set_min_height(PRESET_LIST_HEIGHT);
                    if matching.is_empty() {
                        ui.label(
                            RichText::new(text(state.locale, TextKey::NoMatchingHair))
                                .size(FONT_SM)
                                .color(COLOR_MUTED),
                        );
                    }
                    for index in &matching {
                        let preset = &state.vam_hair_presets[*index];
                        let chosen = picked.as_deref() == Some(preset.stable_id.as_str());
                        if crate::ui::selectable_list_row(ui, &preset.label, chosen)
                            .on_hover_text(format!(
                                "{} · {} {}",
                                preset.label,
                                preset.parts.len(),
                                text(state.locale, TextKey::HairParts)
                            ))
                            .clicked()
                        {
                            picked = Some(preset.stable_id.clone());
                        }
                    }
                });
        });
    state.hair_preset_pick = picked;

    ui.add_space(SPACE_2);
    let ready = state.hair_preset_pick.is_some() && !indexing;
    let width = ui.available_width().max(0.0);
    if crate::ui::capsule_action(
        ui,
        width,
        text(state.locale, TextKey::HairPresetLoad),
        ready,
    )
    .clicked()
        && let Some(preset_id) = state.hair_preset_pick.clone()
    {
        state.dispatch(Action::ImportHairPreset(preset_id));
    }
    ui.add_space(SPACE_3);
}

fn draw_hair_parts_page(ui: &mut Ui, state: &mut AppState) {
    draw_hair_preset_picker(ui, state);
    let floor = (PART_ROW_H + PART_ROW_GAP) * PART_LIST_MIN_ROWS;
    let reserve = PART_LIST_INSET * 2.0 + SPACE_2;
    let height = crate::ui::inspector_list_budget(ui, ui.cursor().top(), reserve)
        .map_or(floor, |budget| budget.max(floor));
    egui::Frame::new()
        .fill(COLOR_FIELD)
        .corner_radius(crate::theme::CONTROL_RADIUS)
        .inner_margin(egui::Margin::same(PART_LIST_INSET as i8))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("vkit.hair.parts-list")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .max_height(height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().max(0.0));
                    ui.set_min_height(height);
                    ui.spacing_mut().item_spacing.y = PART_ROW_GAP;
                    let parts: Vec<(u64, String, bool)> = state
                        .hair_project
                        .parts
                        .iter()
                        .map(|part| (part.id, part.name.clone(), part.visible))
                        .collect();
                    for (part_id, name, visible) in parts {
                        draw_sidebar_part_row(ui, state, part_id, &name, visible);
                    }
                    draw_add_part_slot(ui, state);
                });
        });
}

fn draw_sidebar_part_row(
    ui: &mut Ui,
    state: &mut AppState,
    part_id: u64,
    name: &str,
    visible: bool,
) {
    let active = state.hair_project.is_part_active(part_id);
    let (row, row_response) = ui.allocate_exact_size(
        vec2(ui.available_width().max(0.0), PART_ROW_H),
        Sense::click(),
    );
    if active {
        ui.painter().rect_filled(
            row,
            crate::theme::CONTROL_RADIUS,
            crate::theme::COLOR_ACTIVE_ROW,
        );
    } else if row_response.hovered() {
        ui.painter().rect_filled(
            row,
            crate::theme::CONTROL_RADIUS,
            crate::theme::COLOR_SURFACE_HOVER,
        );
    }
    let ink = if active {
        crate::theme::COLOR_ACTIVE_INK
    } else {
        COLOR_TEXT
    };
    let muted_ink = if active {
        crate::theme::COLOR_ACTIVE_INK.gamma_multiply(0.7)
    } else {
        COLOR_MUTED
    };

    let mut row_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("hair-part-row", part_id))
            .max_rect(row.shrink2(vec2(4.0, 3.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    row_ui.spacing_mut().item_spacing.x = 4.0;
    draw_part_thumbnail_slot(&mut row_ui, state, part_id, PART_THUMB);

    let rename_id = egui::Id::new("vkit.hair.rename");
    let editing: Option<u64> = row_ui.data(|data| data.get_temp(rename_id));
    let mut on_control = false;
    let icon_lane = 22.0 * 4.0 + 4.0 * 4.0;
    if editing == Some(part_id) {
        let buffer_id = rename_id.with(part_id);
        let mut buffer: String = row_ui
            .data(|data| data.get_temp(buffer_id))
            .unwrap_or_else(|| name.to_owned());
        let edit = row_ui.add(
            egui::TextEdit::singleline(&mut buffer)
                .font(egui::TextStyle::Small)
                .desired_width((row_ui.available_width() - icon_lane).max(48.0)),
        );
        row_ui.data_mut(|data| data.insert_temp(buffer_id, buffer.clone()));
        on_control = true;
        if edit.lost_focus() {
            if !row_ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                state.dispatch(Action::SetHairPartName {
                    id: part_id,
                    name: buffer,
                });
            }
            row_ui.data_mut(|data| {
                data.remove::<u64>(rename_id);
                data.remove::<String>(buffer_id);
            });
        } else {
            edit.request_focus();
        }
    } else {
        let label_width = (row_ui.available_width() - icon_lane).max(40.0);
        let label = row_ui.add_sized(
            [label_width, 24.0],
            egui::Label::new(RichText::new(name).size(FONT_SM).color(ink))
                .truncate()
                .selectable(false)
                .sense(Sense::click()),
        );
        let label = row_ui.interact(
            label.rect,
            egui::Id::new(("vkit.hair.part-label", part_id)),
            Sense::click(),
        );
        if label.clicked() {
            state.dispatch(Action::ActivateHairPart {
                id: part_id,
                additive: crate::shortcuts::Shortcut::ListAddToSelectionHold.held(&row_ui),
            });
        }
        let (pencil, _) = row_ui.allocate_exact_size(egui::Vec2::splat(22.0), Sense::hover());
        let pencil_response = row_ui.interact(
            pencil,
            egui::Id::new(("vkit.hair.rename-pencil", part_id)),
            Sense::click(),
        );
        if row_response.hovered() || pencil_response.hovered() || label.hovered() {
            crate::ui_components::paint_icon(
                row_ui.painter(),
                pencil.shrink(4.0),
                Icon::Pencil,
                if pencil_response.hovered() {
                    if active {
                        egui::Color32::BLACK
                    } else {
                        egui::Color32::WHITE
                    }
                } else {
                    muted_ink
                },
            );
        }
        if pencil_response
            .on_hover_text(text(state.locale, TextKey::HairRenamePart))
            .clicked()
        {
            on_control = true;
            row_ui.data_mut(|data| data.insert_temp(rename_id, part_id));
        }
    }

    let ghost = |ui: &mut Ui, icon: Icon, tooltip: &str, destructive: bool| {
        let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(22.0), Sense::click());
        let color = if response.hovered() {
            if destructive {
                crate::theme::COLOR_DESTRUCTIVE
            } else if active {
                egui::Color32::BLACK
            } else {
                egui::Color32::WHITE
            }
        } else {
            muted_ink
        };
        crate::ui_components::paint_icon(ui.painter(), rect.shrink(3.0), icon, color);
        response.on_hover_text(tooltip)
    };
    let duplicate = ghost(
        &mut row_ui,
        Icon::Copy,
        text(state.locale, TextKey::HairDuplicatePart),
        false,
    );
    let eye = ghost(
        &mut row_ui,
        if visible {
            Icon::EyeOpen
        } else {
            Icon::EyeClosed
        },
        text(state.locale, TextKey::HairPartVisible),
        false,
    );
    let trash = ghost(
        &mut row_ui,
        Icon::Trash,
        text(state.locale, TextKey::RemoveHairPart),
        true,
    );
    let hovered_controls = duplicate.hovered() || eye.hovered() || trash.hovered();
    if duplicate.clicked() {
        state.dispatch(Action::DuplicateHairPart(part_id));
    } else if eye.clicked() {
        if crate::shortcuts::Shortcut::ListSoloHold.held(&row_ui) {
            state.dispatch(Action::SoloHairPart(part_id));
        } else {
            state.dispatch(Action::ToggleHairPartVisible(part_id));
        }
    } else if trash.clicked() {
        state.dispatch(Action::RemoveHairPart(part_id));
    } else if row_response.clicked() && !on_control && !hovered_controls {
        state.dispatch(Action::ActivateHairPart {
            id: part_id,
            additive: crate::shortcuts::Shortcut::ListAddToSelectionHold.held(&row_ui),
        });
    }
}

fn draw_add_part_slot(ui: &mut Ui, state: &mut AppState) {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width().max(0.0), PART_ROW_H),
        Sense::click(),
    );
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            crate::theme::CONTROL_RADIUS,
            crate::theme::COLOR_SURFACE_HOVER,
        );
    }
    let color = if response.hovered() {
        COLOR_TEXT
    } else {
        COLOR_MUTED
    };
    let caption = text(state.locale, TextKey::AddHairPart);
    let font = egui::FontId::proportional(FONT_SM);
    let caption_width = ui
        .painter()
        .layout_no_wrap(caption.to_owned(), font.clone(), color)
        .rect
        .width();
    const GLYPH: f32 = 14.0;
    const GAP: f32 = 5.0;
    let group = (GLYPH + GAP + caption_width).min(rect.width() - 8.0);
    let start = rect.center().x - group * 0.5;
    crate::ui_components::paint_icon(
        ui.painter(),
        egui::Rect::from_center_size(
            egui::pos2(start + GLYPH * 0.5, rect.center().y),
            egui::Vec2::splat(GLYPH),
        ),
        Icon::Plus,
        color,
    );
    ui.painter().text(
        egui::pos2(start + GLYPH + GAP, rect.center().y),
        egui::Align2::LEFT_CENTER,
        caption,
        font,
        color,
    );
    if state.hair_project.parts.is_empty() {
        guide_glow(ui, rect, crate::theme::CONTROL_RADIUS);
    }
    if response.clicked() {
        let provider = state.hair_project.effective_provider().to_owned();
        state.dispatch(Action::AddHairPart {
            provider_name: provider,
        });
    }
}

pub(crate) fn draw_hair_export_section(ui: &mut Ui, state: &mut AppState) {
    let has_parts = !state.hair_project.parts.is_empty();
    if !has_parts {
        ui.label(
            RichText::new(text(state.locale, TextKey::HairCreateFirst))
                .size(FONT_SM)
                .color(COLOR_MUTED),
        );
        return;
    }
    draw_export_fields(ui, state);
    ui.add_space(SPACE_3);
    let has_hair = state
        .hair_project
        .parts
        .iter()
        .any(|part| !part.strands.is_empty());
    let clicked = crate::ui::capsule_action(
        ui,
        ui.available_width().max(0.0),
        text(state.locale, TextKey::HairExport),
        has_hair,
    )
    .clicked();
    if clicked && has_hair {
        if export_blocked_on_metadata(state) {
            state.hair_export_attempted_blank = true;
        } else {
            state.hair_export_attempted_blank = false;
            state.dispatch(Action::ExportHairPart);
        }
    }

    let installed = !state.hair_export_files.is_empty();
    let package = crate::ui::capsule_action(
        ui,
        ui.available_width().max(0.0),
        text(state.locale, TextKey::HairPackageStyle),
        installed,
    )
    .clicked();
    if package && installed {
        state.dispatch(Action::PackageHairStyle);
    }
}

fn draw_parameter_groups(ui: &mut Ui, state: &mut AppState) {
    if ui
        .ctx()
        .input(|input| input.pointer.button_released(egui::PointerButton::Primary))
    {
        state.hair_project.end_control();
    }

    let Some(part_id) = state.hair_project.selected_part_id else {
        return;
    };
    let selected = state.hair_param_group;
    let index = HairParamGroup::ALL
        .iter()
        .position(|group| *group == selected)
        .unwrap_or(0);
    let mut picked = None;
    let mut copy = false;
    let mut paste = false;
    let clipboard_filled = state.hair_settings_clipboard.is_some();
    ui.horizontal(|ui| {
        let icon_lane =
            crate::ui_components::icon_button_size(ui) * 2.0 + ui.spacing().item_spacing.x * 2.0;
        ui.scope(|ui| {
            ui.set_max_width((ui.available_width() - icon_lane).max(0.0));
            crate::ui_components::animated_segmented_group(
                ui,
                "vkit.hair.param-groups",
                HairParamGroup::ALL.len(),
                index,
                |ui, segment_width| {
                    for group in HairParamGroup::ALL {
                        if crate::ui_components::segment_button(
                            ui,
                            segment_width,
                            text(state.locale, group_title(group)),
                            group == selected,
                        )
                        .clicked()
                        {
                            picked = Some(group);
                        }
                    }
                },
            );
        });
        copy = icon_button(
            ui,
            Icon::Copy,
            text(state.locale, TextKey::HairCopySettings),
        )
        .clicked();
        ui.add_enabled_ui(clipboard_filled, |ui| {
            paste = icon_button(
                ui,
                Icon::Paste,
                text(state.locale, TextKey::HairPasteSettings),
            )
            .clicked();
        });
    });
    if copy {
        state.dispatch(Action::CopyHairSettings(part_id));
    }
    if paste && clipboard_filled {
        state.dispatch(Action::PasteHairSettings(part_id));
    }
    if let Some(group) = picked {
        state.dispatch(Action::SelectHairParamGroup(group));
    }
    ui.add_space(SPACE_3);

    if selected == HairParamGroup::Look {
        draw_color_capsule_grid(ui, state, part_id, &HAIR_COLOR_CAPSULES);
        ui.add_space(SPACE_3);
    }
    if selected == HairParamGroup::Physics {
        draw_physics_toggle_row(ui, state, part_id);
    }
    if selected == HairParamGroup::Stiffness {
        draw_style_joint_toggle(ui, state, part_id);
    }
    for param in params_in(selected) {
        if matches!(param.key, "simulationEnabled" | "collisionEnabled") {
            continue;
        }
        if param.kind == HairParamKind::Color {
            continue;
        }
        draw_parameter(ui, state, part_id, param);
    }
    ui.add_space(SPACE_2);
    if capsule_action(
        ui,
        ui.available_width().max(0.0),
        text(state.locale, TextKey::ResetAll),
        true,
    )
    .clicked()
    {
        state.dispatch(Action::ResetHairParams(part_id));
    }
}

fn draw_style_joint_toggle(ui: &mut Ui, state: &mut AppState, part_id: u64) {
    let Some(part) = state.hair_project.part(part_id) else {
        return;
    };
    let mut on = part.style_joints;
    let response =
        crate::ui_components::switch_row(ui, &mut on, text(state.locale, TextKey::HairStyleJoints))
            .on_hover_text(text(state.locale, TextKey::HairStyleJointsHint));
    if response.changed() {
        state.dispatch(Action::SetHairPartStyleJoints { id: part_id, on });
    }
    ui.add_space(SPACE_2);
}

fn physics_live(part: &HairPart) -> bool {
    crate::hair_settings::HAIR_PARAMS
        .iter()
        .find(|param| param.key == "simulationEnabled")
        .is_some_and(|param| part.settings.get(param) >= 0.5)
}

fn draw_physics_toggle_row(ui: &mut Ui, state: &mut AppState, part_id: u64) {
    let Some(part) = state.hair_project.part(part_id) else {
        return;
    };
    let toggles: Vec<(&'static crate::hair_settings::HairParam, bool)> =
        crate::hair_settings::HAIR_PARAMS
            .iter()
            .filter(|param| matches!(param.key, "simulationEnabled" | "collisionEnabled"))
            .map(|param| (param, part.settings.get(param) >= 0.5))
            .collect();
    let live = physics_live(part);
    let mut flip: Option<(&'static str, bool)> = None;
    for (param, on) in &toggles {
        let mut value = *on;
        let response =
            crate::ui_components::switch_row(ui, &mut value, param.title(state.vam_name_locale()));
        let response = match param.hint(state.locale) {
            Some(hint) => response.on_hover_text(hint),
            None => response,
        };
        let hint = match param.key {
            "simulationEnabled" => Some(TextKey::HairSimToggleHint),
            "collisionEnabled" => Some(TextKey::HairCollisionToggleHint),
            _ => None,
        };
        let response = if let Some(hint) = hint {
            response.on_hover_text(text(state.locale, hint))
        } else if param.game_only() && !live {
            response.on_hover_text(text(state.locale, TextKey::HairGameOnly))
        } else {
            response
        };
        if response.changed() {
            flip = Some((param.key, value));
        }
    }
    if let Some((key, value)) = flip {
        state.dispatch(Action::SetHairParam {
            id: part_id,
            key,
            value: if value { 1.0 } else { 0.0 },
        });
    }
    ui.add_space(SPACE_2);
}

fn draw_parameter(
    ui: &mut Ui,
    state: &mut AppState,
    part_id: u64,
    param: &'static crate::hair_settings::HairParam,
) {
    let Some(part) = state.hair_project.part(part_id) else {
        return;
    };
    let current = part.settings.get(param);
    match param.kind {
        HairParamKind::Color => {}
        HairParamKind::Toggle => {
            let mut on = current >= 0.5;
            let live = physics_live(part);
            let mut response =
                crate::ui_components::switch_row(ui, &mut on, param.title(state.vam_name_locale()));
            if let Some(hint) = param.hint(state.locale) {
                response = response.on_hover_text(hint);
            }
            if param.game_only() && !live {
                response = response.on_hover_text(text(state.locale, TextKey::HairGameOnly));
            }
            if response.changed() {
                state.dispatch(Action::SetHairParam {
                    id: part_id,
                    key: param.key,
                    value: if on { 1.0 } else { 0.0 },
                });
            }
        }
        HairParamKind::Count | HairParamKind::Float { .. } => {
            let decimals = match param.kind {
                HairParamKind::Float { decimals } => decimals,
                _ => 0,
            };
            let mut value = current;
            let touched = !part.settings.is_default(param);
            let cell = crate::ui_components::slider_cell(
                ui,
                RichText::new(param.title(state.vam_name_locale()))
                    .size(FONT_SM)
                    .color(if touched { COLOR_TEXT } else { COLOR_MUTED }),
                touched,
                touched,
                text(state.locale, TextKey::Reset),
                |ui| {
                    ui.add(
                        crate::ui_components::FilledNumericSlider::new(
                            &mut value,
                            param.min..=param.max,
                        )
                        .decimals(decimals)
                        .value_lane(HAIR_VALUE_LANE)
                        .right_align_value(),
                    )
                },
            );
            let explain = |response: egui::Response| {
                let response = match param.hint(state.locale) {
                    Some(hint) => response.on_hover_text(hint),
                    None => response,
                };
                if param.game_only() && !physics_live(part) {
                    response.on_hover_text(text(state.locale, TextKey::HairGameOnly))
                } else {
                    response
                }
            };
            explain(cell.label);
            explain(cell.slider.clone());
            if cell.reset_clicked {
                state.dispatch(Action::SetHairParam {
                    id: part_id,
                    key: param.key,
                    value: param.default,
                });
            } else if cell.slider.changed() {
                state.dispatch(Action::SetHairParam {
                    id: part_id,
                    key: param.key,
                    value,
                });
            }
        }
    }
}

const fn group_title(group: HairParamGroup) -> TextKey {
    match group {
        HairParamGroup::Performance => TextKey::HairGroupPerformance,
        HairParamGroup::Physics => TextKey::HairGroupPhysics,
        HairParamGroup::Stiffness => TextKey::HairGroupStiffness,
        HairParamGroup::Shape => TextKey::HairGroupShape,
        HairParamGroup::Curl => TextKey::HairGroupCurl,
        HairParamGroup::Look => TextKey::HairGroupLook,
        HairParamGroup::Scalp => TextKey::HairGroupScalp,
    }
}

fn draw_color_capsule_grid(
    ui: &mut Ui,
    state: &mut AppState,
    part_id: u64,
    order: &[(&'static str, TextKey)],
) {
    let Some(part) = state.hair_project.part(part_id) else {
        return;
    };
    struct Cell {
        param: &'static crate::hair_settings::HairParam,
        label: TextKey,
        color: [u8; 3],
        touched: bool,
    }
    let cells: Vec<Cell> = order
        .iter()
        .filter_map(|(key, label)| {
            let param = crate::hair_settings::HAIR_PARAMS
                .iter()
                .find(|param| param.key == *key)?;
            Some(Cell {
                param,
                label: *label,
                color: [
                    part.settings.color_channel(param, 0).round() as u8,
                    part.settings.color_channel(param, 1).round() as u8,
                    part.settings.color_channel(param, 2).round() as u8,
                ],
                touched: !part.settings.is_color_default(param),
            })
        })
        .collect();

    let mut changed: Option<(&'static str, [u8; 3])> = None;
    let spacing = ui.spacing().item_spacing.x;
    if cells.is_empty() {
        return;
    }
    let lanes = cells.len() as f32;
    let column = ((ui.available_width() - spacing * (lanes - 1.0)) / lanes).max(24.0);
    ui.horizontal(|ui| {
        for cell in &cells {
            ui.allocate_ui_with_layout(
                vec2(column, 18.0),
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    ui.label(
                        RichText::new(text(state.locale, cell.label))
                            .size(FONT_SM)
                            .color(if cell.touched {
                                COLOR_TEXT
                            } else {
                                COLOR_MUTED
                            }),
                    );
                },
            );
        }
    });
    ui.horizontal(|ui| {
        for cell in &cells {
            ui.allocate_ui_with_layout(
                vec2(column, crate::ui_components::COMPACT_COLOR_SWATCH_HEIGHT),
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    let mut color = cell.color;
                    let width = crate::ui_components::COMPACT_COLOR_SWATCH_WIDTH.min(column);
                    if crate::ui_components::color_capsule_picker(
                        ui,
                        &mut color,
                        "",
                        vec2(width, crate::ui_components::COMPACT_COLOR_SWATCH_HEIGHT),
                    )
                    .changed()
                    {
                        changed = Some((cell.param.key, color));
                    }
                },
            );
        }
    });
    if let Some((key, color)) = changed {
        for (channel, value) in color.iter().enumerate() {
            state.dispatch(Action::SetHairColorChannel {
                id: part_id,
                key,
                channel,
                value: f32::from(*value),
            });
        }
    }
}

const HAIR_COLOR_CAPSULES: [(&str, TextKey); 3] = [
    ("rootColor", TextKey::HairColorRoot),
    ("tipColor", TextKey::HairColorTip),
    ("specularColor", TextKey::HairColorSpecular),
];

const SCALP_COLOR_CAPSULES: [(&str, TextKey); 1] = [("Diffuse Color", TextKey::HairColorScalp)];

fn draw_scalp_page(ui: &mut Ui, state: &mut AppState) {
    let Some(part_id) = state.hair_project.editing_scalp_part_id() else {
        draw_scalp_creator(ui, state);
        return;
    };

    let editing = state
        .hair_project
        .part(part_id)
        .map(|part| part.name.clone())
        .unwrap_or_default();
    ui.label(RichText::new(editing).size(FONT_SM).color(COLOR_TEXT));
    ui.add_space(SPACE_3);

    draw_scalp_mesh_picker(ui, state, Some(part_id));
    ui.add_space(SPACE_3);
    draw_scalp_mask_chooser(ui, state, part_id);
    ui.add_space(SPACE_3);
    draw_color_capsule_grid(ui, state, part_id, &SCALP_COLOR_CAPSULES);
    ui.add_space(SPACE_3);

    for param in crate::hair_settings::HAIR_PARAMS
        .iter()
        .filter(|param| param.group == crate::hair_settings::HairParamGroup::Scalp)
    {
        if param.kind == HairParamKind::Color {
            continue;
        }
        draw_parameter(ui, state, part_id, param);
    }
    ui.add_space(SPACE_3);
    draw_scalp_add_button(ui, state);
}

fn draw_scalp_creator(ui: &mut Ui, state: &mut AppState) {
    ui.label(
        RichText::new(text(state.locale, TextKey::HairScalpAbsent))
            .size(FONT_SM)
            .color(COLOR_MUTED),
    );
    ui.add_space(SPACE_3);
    draw_scalp_mesh_picker(ui, state, None);
    ui.add_space(SPACE_3);
    draw_scalp_add_button(ui, state);
}

fn draw_scalp_add_button(ui: &mut Ui, state: &mut AppState) {
    let provider = scalp_mesh_choice(state);
    if capsule_action(
        ui,
        ui.available_width(),
        text(state.locale, TextKey::HairScalpCreate),
        true,
    )
    .clicked()
    {
        state.dispatch(Action::AddHairScalp(provider));
    }
}

fn scalp_mesh_choice(state: &AppState) -> String {
    if !state.hair_scalp_mesh.is_empty() {
        return state.hair_scalp_mesh.clone();
    }
    head_scalp_meshes(state)
        .first()
        .cloned()
        .unwrap_or_else(|| "UdaneScalp".to_owned())
}

fn head_scalp_meshes(state: &AppState) -> Vec<String> {
    crate::hair_export::HEAD_SCALP_PROVIDERS
        .iter()
        .filter(|name| {
            state
                .builtin_hair_scalps
                .iter()
                .any(|scalp| scalp.provider_name == **name)
        })
        .map(|name| (*name).to_owned())
        .collect()
}

fn draw_scalp_mesh_picker(ui: &mut Ui, state: &mut AppState, part_id: Option<u64>) {
    let meshes = head_scalp_meshes(state);
    if meshes.is_empty() {
        return;
    }
    let current = match part_id {
        Some(id) => state
            .hair_project
            .part(id)
            .map(|part| part.provider_name.clone())
            .unwrap_or_default(),
        None => scalp_mesh_choice(state),
    };
    ui.label(
        RichText::new(text(state.locale, TextKey::HairScalpMesh))
            .size(FONT_SM)
            .color(COLOR_TEXT),
    );
    ui.add_space(SPACE_2);
    let mut picked = None;
    crate::ui_components::fit_combo(
        ui,
        "vkit.hair.scalp-mesh",
        ui.available_width(),
        &current,
        |ui| {
            for mesh in &meshes {
                if ui.selectable_label(*mesh == current, mesh).clicked() {
                    picked = Some(mesh.clone());
                }
            }
        },
    );
    if let Some(mesh) = picked {
        state.hair_scalp_mesh = mesh.clone();
        if let Some(id) = part_id {
            state.dispatch(Action::SetHairScalpMesh { id, mesh });
        }
    }
}

fn draw_scalp_mask_chooser(ui: &mut Ui, state: &mut AppState, part_id: u64) {
    use crate::hair_project::ScalpSlot;

    for (slot, label) in [
        (ScalpSlot::Diffuse, TextKey::HairScalpMask),
        (ScalpSlot::Alpha, TextKey::HairScalpAlpha),
    ] {
        draw_scalp_slot_chooser(ui, state, part_id, slot, label);
        if slot == ScalpSlot::Diffuse {
            ui.add_space(SPACE_3);
        }
    }
}

fn draw_scalp_slot_chooser(
    ui: &mut Ui,
    state: &mut AppState,
    part_id: u64,
    slot: crate::hair_project::ScalpSlot,
    label: TextKey,
) {
    use crate::hair_project::ScalpSlot;

    let Some(part) = state.hair_project.part(part_id) else {
        return;
    };
    let mut texture = part.scalp_texture.clone();
    let held = match slot {
        ScalpSlot::Diffuse => texture.diffuse.clone(),
        ScalpSlot::Alpha => texture.alpha.clone(),
    };
    let custom = held.is_some();
    ui.label(
        RichText::new(text(state.locale, label))
            .size(FONT_SM)
            .color(if custom { COLOR_TEXT } else { COLOR_MUTED }),
    );
    ui.add_space(SPACE_2);

    let mut chosen: Option<Option<std::path::PathBuf>> = None;
    crate::ui_components::animated_segmented_group(
        ui,
        match slot {
            ScalpSlot::Diffuse => "vkit.hair.scalp-diffuse",
            ScalpSlot::Alpha => "vkit.hair.scalp-alpha",
        },
        2,
        usize::from(custom),
        |ui, width| {
            if crate::ui_components::segment_button(
                ui,
                width,
                text(state.locale, TextKey::HairScalpMaskBuiltIn),
                !custom,
            )
            .clicked()
            {
                chosen = Some(None);
            }
            if crate::ui_components::segment_button(
                ui,
                width,
                text(state.locale, TextKey::HairScalpMaskCustom),
                custom,
            )
            .clicked()
                && let Some(path) = crate::dialogs::pick_scalp_mask(state)
            {
                chosen = Some(Some(path));
            }
        },
    );

    if let Some(path) = &held {
        ui.add_space(SPACE_2);
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_owned();
        ui.label(RichText::new(name).size(FONT_SM).color(COLOR_MUTED));
    }

    if let Some(picked) = chosen {
        match slot {
            ScalpSlot::Diffuse => texture.diffuse = picked,
            ScalpSlot::Alpha => texture.alpha = picked,
        }
        state.dispatch(Action::SetHairScalpTexture {
            id: part_id,
            texture,
        });
    }
}
