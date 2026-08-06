use super::*;

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
    let (category_rect, clicked) = chips(ui, Id::new("vkit.morph.categories"), active, &labels);
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
