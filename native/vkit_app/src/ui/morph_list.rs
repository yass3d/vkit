use super::*;

/// Sit the one-sided toggle in the gap the wrapping category capsules leave.
///
/// The capsules wrap, so the last row usually ends short of the edge, and that
/// gap is dead space directly above the search field. Putting the toggle there
/// costs no vertical room. When the last row happens to fill out, it takes a row
/// of its own rather than overlapping a capsule.
fn draw_one_sided_toggle(ui: &mut Ui, state: &mut AppState, categories: Id, categories_rect: Rect) {
    let label = text(state.locale, TextKey::MorphOneSidedFilter);
    let width = ui
        .painter()
        .layout_no_wrap(
            label.to_owned(),
            FontId::proportional(FONT_SM),
            crate::theme::COLOR_TEXT,
        )
        .size()
        .x
        + SPACE_3 * 2.0;

    let last_chip = crate::ui_components::chips_last_chip(ui, categories);
    let row_top = last_chip.map_or(categories_rect.top(), |chip| chip.top());
    let used_right = last_chip.map_or(categories_rect.left(), |chip| chip.right());
    let fits_beside = ui.max_rect().right() - used_right - SPACE_2 >= width;

    let rect = if fits_beside {
        Rect::from_min_size(
            pos2(ui.max_rect().right() - width, row_top),
            vec2(width, CONTROL_H_DENSE),
        )
    } else {
        ui.add_space(SPACE_2);
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), CONTROL_H_DENSE), Sense::hover());
        Rect::from_min_size(
            pos2(rect.right() - width, rect.top()),
            vec2(width, CONTROL_H_DENSE),
        )
    };

    let shown = state.morph_library.show_one_sided;
    let response = ui.interact(rect, categories.with("one-sided"), Sense::click());
    let (fill, ink) = if shown {
        (crate::theme::COLOR_SURFACE_RAISED, crate::theme::COLOR_TEXT)
    } else {
        (crate::theme::COLOR_SURFACE, crate::theme::COLOR_MUTED)
    };
    ui.painter()
        .rect_filled(rect, crate::theme::CONTROL_RADIUS, fill);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(FONT_SM),
        ink,
    );
    control_affordances(ui, &response, rect, f32::from(crate::theme::CONTROL_RADIUS));
    if response.clicked() {
        state.dispatch(Action::SetShowOneSidedMorphs(!shown));
    }
    response.on_hover_text(text(state.locale, TextKey::MorphOneSidedFilterHint));
}

pub(crate) fn draw_morph_filters(ui: &mut Ui, state: &mut AppState) -> MorphFilterLayout {
    set_capsule_widget_radius(ui);

    let mut filters = Vec::with_capacity(MorphCategory::ALL.len() + 1);
    filters.push((MorphCategoryFilter::All, TextKey::MorphCategoryAll));
    filters.extend(MorphCategory::ALL.map(|category| {
        (
            MorphCategoryFilter::Category(category),
            morph_category_key(category),
        )
    }));
    let labels = filters
        .iter()
        .map(|(_, key)| text(state.locale, *key))
        .collect::<Vec<_>>();
    let active = filters
        .iter()
        .position(|(filter, _)| state.morph_library.category_filter == *filter);
    let categories_id = Id::new("vkit.morph.categories");
    let (category_rect, clicked) = chips(ui, categories_id, active, &labels);
    draw_one_sided_toggle(ui, state, categories_id, category_rect);
    let selected = clicked.map(|index| filters[index].0);
    let scroll_to_top =
        selected.is_some_and(|filter| filter != state.morph_library.category_filter);
    if let Some(filter) = selected {
        state.dispatch(Action::SetMorphCategoryFilter(filter));
    }

    ui.add_space(SPACE_2);
    let mut query = state.morph_library.query.clone();
    let search_rect = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = SPACE_2;
            let spacing = ui.spacing().item_spacing.x;

            let toggle_width = MORPH_FILTER_CAPSULE_WIDTH.min(ui.available_width());
            let search_width = (ui.available_width() - toggle_width - spacing).max(0.0);
            let rect = ui
                .allocate_ui_with_layout(
                    vec2(search_width, CONTROL_HEIGHT),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        capsule_search_field(
                            ui,
                            "vkit.morph.search",
                            &mut query,
                            text(state.locale, TextKey::MorphSearch),
                            true,
                        )
                    },
                )
                .inner
                .rect;

            let current = state.morph_library.list_filter();
            let picked = ui
                .allocate_ui_with_layout(
                    vec2(toggle_width, CONTROL_HEIGHT),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        crate::ui_components::fit_combo(
                            ui,
                            "vkit.morph.list-filter",
                            toggle_width,
                            text(state.locale, morph_list_filter_key(current)),
                            |ui| {
                                let mut picked = current;
                                for candidate in MORPH_LIST_FILTERS {
                                    ui.selectable_value(
                                        &mut picked,
                                        candidate,
                                        text(state.locale, morph_list_filter_key(candidate)),
                                    );
                                }
                                picked
                            },
                        )
                    },
                )
                .inner;
            if let Some(picked) = picked
                && picked != current
            {
                state.dispatch(Action::SetMorphListFilter(picked));
            }

            rect.union(ui.min_rect())
        })
        .inner;
    if query != state.morph_library.query {
        state.dispatch(Action::SetMorphQuery(query));
    }

    MorphFilterLayout {
        rect: search_rect.union(category_rect),
        category_rect,
        search_rect,
        scroll_to_top,
    }
}

pub(crate) fn capsule_search_field(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    query: &mut String,
    hint: &str,
    enabled: bool,
) -> Response {
    let width = ui.available_width().max(0.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, CONTROL_HEIGHT), Sense::hover());
    ui.painter().rect_filled(rect, CAPSULE_RADIUS, COLOR_FIELD);

    let icon_width = SEARCH_ICON_SLOT_WIDTH.min(rect.width());
    let edit_rect = Rect::from_min_max(
        pos2(rect.left() + SEARCH_TEXT_INSET, rect.top()),
        pos2(rect.right() - icon_width, rect.bottom()),
    );
    let icon_rect = Rect::from_min_max(pos2(edit_rect.right(), rect.top()), rect.max);
    let mut edit_ui = ui.new_child(
        UiBuilder::new()
            .id_salt(("capsule.search.edit", &id_salt))
            .max_rect(edit_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    constrain_morph_child_clip(&mut edit_ui, edit_rect);
    if !enabled {
        edit_ui.disable();
    }
    let response = edit_ui.add_sized(
        edit_rect.size(),
        TextEdit::singleline(query)
            .id(Id::new(("capsule.search.input", id_salt)))
            .hint_text(hint)
            .desired_width(f32::INFINITY)
            .frame(Frame::NONE)
            .vertical_align(Align::Center)
            .margin(Margin::same(0)),
    );
    paint_search_icon(ui, icon_rect, response.has_focus(), enabled);
    response
}

pub(crate) fn constrain_morph_child_clip(ui: &mut Ui, cell: Rect) {
    ui.shrink_clip_rect(cell);
}

fn paint_search_icon(ui: &Ui, rect: Rect, focused: bool, enabled: bool) {
    let color = if !enabled {
        crate::theme::disabled(COLOR_MUTED)
    } else if focused {
        COLOR_PRIMARY
    } else {
        COLOR_MUTED
    };
    paint_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(16.0)),
        Icon::Search,
        color,
    );
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MorphControlWidths {
    pub(crate) slider: f32,
    pub(crate) value_reset_gap: f32,
    pub(crate) reset: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MorphRowColumns {
    pub(crate) primary: Rect,
    pub(crate) reset: Rect,
}
