use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SkinBaseChoice {
    Solid,

    VaMSkin,

    PaintOnly,
}

impl SkinBaseChoice {
    const fn label(self) -> TextKey {
        match self {
            Self::Solid => TextKey::SolidColor,
            Self::VaMSkin => TextKey::TextureSkin,
            Self::PaintOnly => TextKey::BaseWithoutSkin,
        }
    }

    const fn tooltip(self) -> TextKey {
        match self {
            Self::Solid => TextKey::SolidColorTooltip,
            Self::VaMSkin => TextKey::TextureSkinTooltip,
            Self::PaintOnly => TextKey::BaseWithoutSkinTooltip,
        }
    }

    fn actions(self) -> [Action; 2] {
        match self {
            Self::Solid => [
                Action::SetBaseViewMode(BaseViewMode::Solid),
                Action::SetTextureHideVaMSkin(false),
            ],
            Self::VaMSkin => [
                Action::SetBaseViewMode(BaseViewMode::Texture),
                Action::SetTextureHideVaMSkin(false),
            ],
            Self::PaintOnly => [
                Action::SetBaseViewMode(BaseViewMode::Texture),
                Action::SetTextureHideVaMSkin(true),
            ],
        }
    }
}

fn current_base_choice(state: &AppState) -> SkinBaseChoice {
    match state.base_view_mode {
        BaseViewMode::Solid => SkinBaseChoice::Solid,
        BaseViewMode::Texture if state.texture_project.hide_vam_skin_preview => {
            SkinBaseChoice::PaintOnly
        }
        BaseViewMode::Texture => SkinBaseChoice::VaMSkin,
    }
}

const fn base_view_choices() -> [SkinBaseChoice; 3] {
    [
        SkinBaseChoice::Solid,
        SkinBaseChoice::VaMSkin,
        SkinBaseChoice::PaintOnly,
    ]
}

pub(crate) fn draw_viewport_skin_panel_contents(ui: &mut Ui, state: &mut AppState) {
    let texture_attention = attention_frame_for(ui, AttentionTarget::SkinTextureMode);

    let bases = base_view_choices();
    let selected = bases
        .iter()
        .position(|choice| *choice == current_base_choice(state))
        .unwrap_or(0);
    animated_segmented_group(
        ui,
        "vkit.viewport.skin.base-mode",
        bases.len(),
        selected,
        |ui, segment_width| {
            for choice in bases.iter().copied() {
                if let Some(frame) = texture_attention.filter(|_| choice == SkinBaseChoice::VaMSkin)
                {
                    let cell = ui.available_rect_before_wrap();
                    let cell = Rect::from_min_size(
                        cell.min + vec2(frame.shake, 0.0),
                        vec2(segment_width, cell.height()),
                    );
                    ui.painter().rect_filled(
                        cell,
                        CAPSULE_RADIUS,
                        crate::ui_components::attention_tint(COLOR_SURFACE_RAISED, frame),
                    );
                }
                let response = segment_button(
                    ui,
                    segment_width,
                    text(state.locale, choice.label()),
                    choice == current_base_choice(state),
                )
                .on_hover_text(text(state.locale, choice.tooltip()));
                if response.clicked() {
                    for action in choice.actions() {
                        state.dispatch(action);
                    }
                }
            }
        },
    );

    if state.base_view_mode == BaseViewMode::Solid {
        ui.add_space(SPACE_3);
        draw_solid_color_controls(ui, state);
    }
    ui.add_space(SPACE_3);
    ui.vertical_centered(|ui| {
        ui.label(
            RichText::new(text(state.locale, TextKey::SurfaceSmooth))
                .size(FONT_SM)
                .color(COLOR_MUTED),
        )
        .on_hover_text(text(state.locale, TextKey::SurfaceSmoothTooltip));
    });
    ui.add_space(SPACE_2);

    crate::ui_components::animated_segmented_group_circular(
        ui,
        "vkit.viewport.skin.surface-smooth",
        5,
        state.surface_smooth_passes as usize,
        |ui, segment_width| {
            for passes in 0..=4_u8 {
                if segment_button(
                    ui,
                    segment_width,
                    &passes.to_string(),
                    state.surface_smooth_passes == passes,
                )
                .on_hover_text(text(state.locale, TextKey::SurfaceSmoothTooltip))
                .clicked()
                {
                    state.dispatch(Action::SetSurfaceSmoothPasses(passes));
                }
            }
        },
    );
    ui.add_space(crate::theme::SPACE_1);
    draw_vam_source(ui, state);

    if state.base_view_mode == BaseViewMode::Texture {
        draw_skin_preview_selector(ui, state);
    }
}

fn draw_solid_color_controls(ui: &mut Ui, state: &mut AppState) {
    set_capsule_widget_radius(ui);

    let mut custom_head = state.custom_head_solid_color_rgb;
    let mut g2 = state.g2_solid_color_rgb;
    let custom_changed = solid_color_row(
        ui,
        &mut custom_head,
        text(state.locale, TextKey::ScanHeadColor),
    );
    ui.add_space(SPACE_2);
    let g2_changed = solid_color_row(ui, &mut g2, text(state.locale, TextKey::G2HeadColor));
    if custom_changed {
        state.dispatch(Action::SetCustomHeadSolidColor(custom_head));
    }
    if g2_changed {
        state.dispatch(Action::SetG2SolidColor(g2));
    }
}

fn solid_color_row(ui: &mut Ui, color: &mut [u8; 3], label: &str) -> bool {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width().max(0.0), CONTROL_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            let swatch = crate::ui_components::COMPACT_COLOR_SWATCH_WIDTH;
            let label_width =
                (ui.available_width() - swatch - ui.spacing().item_spacing.x).max(0.0);
            ui.add_sized(
                [label_width, CONTROL_HEIGHT],
                egui::Label::new(RichText::new(label).color(COLOR_MUTED)).truncate(),
            );
            color_capsule_picker(ui, color, "", vec2(swatch, CONTROL_HEIGHT)).changed()
        },
    )
    .inner
}

fn draw_vam_source(ui: &mut Ui, state: &mut AppState) {
    set_capsule_widget_radius(ui);
    let status = match &state.vam_catalog_status {
        VaMCatalogStatus::Unconfigured => None,
        VaMCatalogStatus::Indexing => Some((
            text(state.locale, TextKey::VaMIndexing).to_owned(),
            COLOR_PRIMARY,
            None,
        )),

        VaMCatalogStatus::Ready { .. } => None,
        VaMCatalogStatus::Failed { detail } => Some((
            text(state.locale, TextKey::VaMFailed).to_owned(),
            COLOR_DESTRUCTIVE,
            Some(detail.as_str()),
        )),
    };

    let (label, color, detail) = status.map_or_else(
        || (String::new(), COLOR_MUTED, None),
        |(label, color, detail)| (label, color, detail),
    );
    let response = ui.add_sized(
        [ui.available_width(), VAM_SOURCE_STATUS_HEIGHT],
        egui::Label::new(RichText::new(label).size(FONT_XS).color(color)).truncate(),
    );
    if let Some(detail) = detail {
        response.on_hover_text(detail);
    }
}

fn panel_tail_id(panel: &'static str) -> Id {
    Id::new(("vkit.panel.list.tail", panel))
}

pub(crate) fn panel_observed_tail(ui: &Ui, panel: &'static str) -> f32 {
    ui.data(|data| data.get_temp::<f32>(panel_tail_id(panel)))
        .unwrap_or(SKIN_SELECTOR_FALLBACK_TAIL)
}

fn record_panel_tail(ui: &Ui, panel: &'static str, library_bottom: f32) {
    let tail = (ui.min_rect().bottom().max(ui.cursor().top()) - library_bottom).max(0.0);
    ui.data_mut(|data| data.insert_temp(panel_tail_id(panel), tail));
}

pub(crate) const SKIN_PANEL_TAIL: &str = "skin";

pub(crate) fn readable_windows_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{unc}");
    }
    value
        .strip_prefix(r"\\?\")
        .unwrap_or(value.as_ref())
        .to_owned()
}

fn draw_skin_preview_selector(ui: &mut Ui, state: &mut AppState) {
    set_capsule_widget_radius(ui);

    let has_root = state.vam_root.is_some();
    let indexing = matches!(state.vam_catalog_status, VaMCatalogStatus::Indexing);
    let refresh_clicked = ui
        .allocate_ui_with_layout(
            vec2(ui.available_width().max(0.0), CONTROL_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                let gap = ui.spacing().item_spacing.x.max(0.0);
                let search_width = (ui.available_width() - icon_button_size(ui) - gap).max(0.0);
                ui.allocate_ui_with_layout(
                    vec2(search_width, CONTROL_HEIGHT),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        capsule_search_field(
                            ui,
                            "vkit.skin.search",
                            &mut state.skin_query,
                            text(state.locale, TextKey::SkinSearch),
                            !state.vam_skin_presets.is_empty(),
                        );
                    },
                );
                ui.add_enabled_ui(has_root && !indexing, |ui| {
                    icon_button(ui, Icon::Refresh, text(state.locale, TextKey::RefreshSkins))
                })
                .inner
                .clicked()
            },
        )
        .inner;
    if refresh_clicked {
        state.dispatch(Action::RefreshVaMCatalog);
    }

    let filtered = state
        .vam_skin_presets
        .iter()
        .enumerate()
        .filter_map(|(index, preset)| {
            (preset
                .sex
                .is_compatible_with(state.figure_sex.skin_sex(), false)
                && skin_matches_query(&state.skin_query, &preset.label, &preset.stable_id))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let uv_available = state.vam_uv_mapping.is_some();
    let mut selected = state.selected_skin_id.clone();
    let mut default_skin_change: Option<Option<String>> = None;

    let footer_reserve = f32::from(SKIN_SELECTOR_FRAME_MARGIN) * 2.0
        + panel_observed_tail(ui, SKIN_PANEL_TAIL)
        + MINI_POPUP_CONTENT_INSET_Y;
    let list_height = crate::viewport::panel_list_budget(ui, ui.cursor().top(), footer_reserve)
        .map_or(SKIN_SELECTOR_LIST_HEIGHT, |budget| {
            budget.max(SKIN_SELECTOR_MIN_LIST_HEIGHT)
        });
    Frame::new()
        .fill(COLOR_FIELD)
        .stroke(Stroke::NONE)
        .corner_radius(CAPSULE_RADIUS)
        .inner_margin(Margin::same(SKIN_SELECTOR_FRAME_MARGIN))
        .show(ui, |ui| {
            ui.set_width(ui.available_width().max(0.0));
            ScrollArea::vertical()
                .id_salt("vkit.skin.list")
                .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                .max_height(list_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().max(0.0));
                    if selectable_list_row(
                        ui,
                        text(state.locale, TextKey::SkinNone),
                        selected.is_none(),
                    )
                    .clicked()
                    {
                        selected = None;
                    }
                    if filtered.is_empty() && !state.skin_query.trim().is_empty() {
                        ui.label(
                            RichText::new(text(state.locale, TextKey::NoMatchingSkins))
                                .size(FONT_SM)
                                .color(COLOR_MUTED),
                        );
                    }
                    for index in &filtered {
                        let preset = &state.vam_skin_presets[*index];
                        ui.add_enabled_ui(uv_available, |ui| {
                            let row_selected =
                                selected.as_deref() == Some(preset.stable_id.as_str());
                            let response = selectable_list_row(
                                ui,
                                display_skin_label(&preset.label),
                                row_selected,
                            )
                            .on_hover_text(&preset.stable_id);

                            let starred =
                                state.default_skin_id.as_deref() == Some(preset.stable_id.as_str());
                            let star = Rect::from_center_size(
                                pos2(
                                    response.rect.right()
                                        - SPACE_2
                                        - crate::theme::SKIN_STAR_SIZE * 0.5,
                                    response.rect.center().y,
                                ),
                                Vec2::splat(crate::theme::SKIN_STAR_SIZE),
                            );
                            let hit = ui.interact(
                                star,
                                ui.id().with(("skin-star", *index)),
                                Sense::click(),
                            );
                            if starred || hit.hovered() || response.hovered() {
                                crate::ui_components::paint_icon(
                                    ui.painter(),
                                    star,
                                    if starred {
                                        crate::ui_components::Icon::StarFilled
                                    } else {
                                        crate::ui_components::Icon::Star
                                    },
                                    if starred || hit.hovered() {
                                        COLOR_TEXT
                                    } else {
                                        COLOR_MUTED
                                    },
                                );
                            }
                            let hit = hit.on_hover_text(text(
                                state.locale,
                                if starred {
                                    TextKey::DefaultSkinClear
                                } else {
                                    TextKey::DefaultSkinSet
                                },
                            ));
                            if hit.clicked() {
                                default_skin_change = Some(Some(preset.stable_id.clone()));
                            } else if response.clicked() {
                                selected = Some(preset.stable_id.clone());
                            }
                        });
                    }
                });
        });
    let library_bottom = ui.min_rect().bottom();
    if let Some(change) = default_skin_change {
        state.dispatch(Action::SetDefaultSkin(change));
    }
    if selected != state.selected_skin_id {
        state.dispatch(Action::SelectVaMSkin(selected));
    }

    let failure_detail = (state.status.key == TextKey::SkinLoadFailed)
        .then_some(state.status.detail.as_deref())
        .flatten();
    let status = if !state.vam_skin_presets.is_empty() && !uv_available {
        Some((
            text(state.locale, TextKey::SkinUvUnavailable),
            COLOR_WARNING,
            None,
        ))
    } else if state.skin_preview_loading {
        Some((
            text(state.locale, TextKey::SkinLoading),
            COLOR_PRIMARY,
            None,
        ))
    } else if state.selected_skin_id.is_some() && state.skin_preview.is_some() {
        None
    } else if state.selected_skin_id.is_some() {
        Some((
            text(state.locale, TextKey::SkinLoadFailed),
            COLOR_DESTRUCTIVE,
            failure_detail,
        ))
    } else {
        None
    };

    let (label, color, detail) = status.map_or_else(
        || (String::new(), COLOR_MUTED, None),
        |(label, color, detail)| (label.to_owned(), color, detail),
    );
    let response = ui.add_sized(
        [ui.available_width(), VAM_SOURCE_STATUS_HEIGHT],
        egui::Label::new(RichText::new(label).size(FONT_XS).color(color)).truncate(),
    );
    if let Some(detail) = detail {
        response.on_hover_text(detail);
    }
    record_panel_tail(ui, SKIN_PANEL_TAIL, library_bottom);
}

pub(crate) fn skin_matches_query(query: &str, label: &str, stable_id: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || label.to_lowercase().contains(&query)
        || stable_id.to_lowercase().contains(&query)
}

pub(crate) fn display_skin_label(label: &str) -> &str {
    label
        .strip_prefix("Preset_")
        .filter(|display| !display.is_empty())
        .unwrap_or(label)
}
