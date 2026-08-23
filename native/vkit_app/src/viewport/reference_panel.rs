//! The reference list, under the background panel.
//!
//! A row per picture, front of the stack at the top — the order every layer
//! list uses, and the reverse of the order they are painted in. The add button
//! lives inside the list rather than beside it, so an empty list still says
//! what it is for.

use egui::{Align, Align2, FontId, Layout, Rect, Response, Sense, Ui, pos2, vec2};

use crate::i18n::{TextKey, text};
use crate::reference_board::{MAX_HEIGHT_SHARE, MIN_HEIGHT_SHARE};
use crate::state::{Action, AppState};
use crate::theme::{COLOR_MUTED, COLOR_TEXT, CONTROL_H_DENSE, FONT_SM, FONT_XS, SPACE_1, SPACE_2};
use crate::ui_components::{FilledNumericSlider, Icon, icon_button, paint_list_row_highlight};

const ROW_HEIGHT: f32 = 26.0;

/// How many rows the list shows before it scrolls.
///
/// The panel it sits in is a floating island, not a page. Past this it would
/// grow taller than the viewport it is anchored to.
const VISIBLE_ROWS: usize = 6;

pub(super) fn draw_reference_section(ui: &mut Ui, state: &mut AppState) {
    ui.add_space(SPACE_2);
    heading(ui, text(state.locale, TextKey::ReferenceImages));

    let rows = state.reference_board.images().len().clamp(1, VISIBLE_ROWS);
    egui::ScrollArea::vertical()
        .id_salt("vkit.viewport.reference-list")
        .max_height(ROW_HEIGHT * rows as f32)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            // Front of the stack first. `images()` is back to front, which is
            // painting order and the opposite of reading order.
            let ordered: Vec<u64> = state
                .reference_board
                .images()
                .iter()
                .rev()
                .map(|image| image.id)
                .collect();
            if ordered.is_empty() {
                empty_row(ui, state.locale);
            }
            for id in ordered {
                draw_row(ui, state, id);
            }
        });

    ui.add_space(SPACE_1);
    if add_button(ui, state.locale).clicked()
        && let Some(paths) = rfd::FileDialog::new()
            .add_filter(
                "Images",
                &["png", "jpg", "jpeg", "webp", "bmp", "tif", "tiff"],
            )
            .pick_files()
    {
        for path in paths {
            state.dispatch(Action::AddReferenceImage(path));
        }
    }

    if let Some(selected) = state.reference_board.selected() {
        draw_selected_controls(ui, state, selected);
    }
}

fn heading(ui: &mut Ui, label: &str) {
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), FONT_XS + 2.0), Sense::hover());
    ui.painter().text(
        pos2(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(FONT_XS),
        COLOR_MUTED,
    );
}

fn empty_row(ui: &mut Ui, locale: crate::i18n::Locale) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
    ui.painter().text(
        pos2(rect.left() + SPACE_1, rect.center().y),
        Align2::LEFT_CENTER,
        text(locale, TextKey::ReferenceImagesEmpty),
        FontId::proportional(FONT_XS),
        COLOR_MUTED,
    );
}

fn add_button(ui: &mut Ui, locale: crate::i18n::Locale) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), CONTROL_H_DENSE), Sense::click());
    paint_list_row_highlight(ui, rect, false, response.hovered());
    let icon = Rect::from_center_size(
        pos2(rect.left() + SPACE_2 + 6.0, rect.center().y),
        vec2(12.0, 12.0),
    );
    crate::ui_components::paint_icon(ui.painter(), icon, Icon::Plus, COLOR_TEXT);
    ui.painter().text(
        pos2(icon.right() + SPACE_2, rect.center().y),
        Align2::LEFT_CENTER,
        text(locale, TextKey::ReferenceImageAdd),
        FontId::proportional(FONT_SM),
        COLOR_TEXT,
    );
    response
}

fn draw_row(ui: &mut Ui, state: &mut AppState, id: u64) {
    let Some(image) = state.reference_board.get(id) else {
        return;
    };
    let name = image.name.clone();
    let visible = image.visible;
    let selected = state.reference_board.selected() == Some(id);

    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    paint_list_row_highlight(ui, rect, selected, response.hovered());

    let mut row_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(SPACE_1, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    let mut wanted: Option<Action> = None;
    if icon_button(
        &mut row_ui,
        Icon::Trash,
        text(state.locale, TextKey::ReferenceImageRemove),
    )
    .clicked()
    {
        wanted = Some(Action::RemoveReferenceImage(id));
    }
    if icon_button(
        &mut row_ui,
        Icon::ChevronUp,
        text(state.locale, TextKey::ReferenceImageRaise),
    )
    .clicked()
    {
        wanted = Some(Action::ReorderReferenceImage {
            id,
            toward_front: true,
        });
    }
    if icon_button(
        &mut row_ui,
        Icon::ChevronDown,
        text(state.locale, TextKey::ReferenceImageLower),
    )
    .clicked()
    {
        wanted = Some(Action::ReorderReferenceImage {
            id,
            toward_front: false,
        });
    }
    if icon_button(
        &mut row_ui,
        if visible {
            Icon::EyeOpen
        } else {
            Icon::EyeClosed
        },
        text(
            state.locale,
            if visible {
                TextKey::TooltipHide
            } else {
                TextKey::TooltipShow
            },
        ),
    )
    .clicked()
    {
        wanted = Some(Action::ToggleReferenceImageVisible(id));
    }

    let label_right = row_ui.min_rect().left() - SPACE_2;
    let label_width = (label_right - rect.left() - SPACE_1).max(0.0);
    ui.painter().with_clip_rect(rect).text(
        pos2(rect.left() + SPACE_1, rect.center().y),
        Align2::LEFT_CENTER,
        crate::ui_components::ellipsize_to_width(
            ui,
            &name,
            label_width,
            FontId::proportional(FONT_SM),
        ),
        FontId::proportional(FONT_SM),
        if visible { COLOR_TEXT } else { COLOR_MUTED },
    );

    if let Some(action) = wanted {
        state.dispatch(action);
    } else if response.clicked() {
        state.dispatch(Action::SelectReferenceImage(Some(id)));
    }
}

/// Size and opacity, for the picture the reader last touched.
fn draw_selected_controls(ui: &mut Ui, state: &mut AppState, id: u64) {
    let Some(image) = state.reference_board.get(id) else {
        return;
    };
    let mut height_share = image.height_share;
    let mut opacity = image.opacity;

    ui.add_space(SPACE_1);
    if ui
        .add(
            FilledNumericSlider::new(&mut height_share, MIN_HEIGHT_SHARE..=MAX_HEIGHT_SHARE)
                .percent()
                .decimals(0)
                .min_width(120.0),
        )
        .changed()
    {
        state.dispatch(Action::ResizeReferenceImage { id, height_share });
    }
    if ui
        .add(
            FilledNumericSlider::new(&mut opacity, 0.0..=1.0)
                .percent()
                .decimals(0)
                .min_width(120.0),
        )
        .changed()
    {
        state.dispatch(Action::SetReferenceImageOpacity { id, opacity });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Locale;

    /// The list reads front-first while the board paints back-first.
    #[test]
    fn the_list_shows_the_front_picture_at_the_top() {
        let mut state = AppState::default();
        let back = state
            .reference_board
            .add(std::path::PathBuf::from("back.png"));
        let front = state
            .reference_board
            .add(std::path::PathBuf::from("front.png"));

        let painted: Vec<u64> = state
            .reference_board
            .images()
            .iter()
            .map(|image| image.id)
            .collect();
        assert_eq!(painted, vec![back, front], "painted back to front");

        let listed: Vec<u64> = painted.into_iter().rev().collect();
        assert_eq!(listed, vec![front, back], "listed front to back");
    }

    /// Every locale draws the section without panicking or overflowing.
    #[test]
    fn the_section_fits_the_panel_in_every_language() {
        for locale in Locale::ALL {
            egui::__run_test_ui(|ui| {
                let mut state = AppState::default();
                state.locale = locale;
                state
                    .reference_board
                    .add(std::path::PathBuf::from("a-rather-long-reference-name.png"));
                let cell = Rect::from_min_size(ui.cursor().min, vec2(240.0, 400.0));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(cell));
                child.set_clip_rect(cell);
                child.set_width(cell.width());
                draw_reference_section(&mut child, &mut state);
                assert!(
                    child.min_rect().right() <= cell.right() + 0.5,
                    "{locale:?} ran past the panel"
                );
            });
        }
    }

    /// An empty list still says what it is for.
    #[test]
    fn an_empty_list_draws_a_row_saying_so() {
        egui::__run_test_ui(|ui| {
            let mut state = AppState::default();
            let cell = Rect::from_min_size(ui.cursor().min, vec2(240.0, 400.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(cell));
            child.set_width(cell.width());
            draw_reference_section(&mut child, &mut state);
            assert!(state.reference_board.is_empty());
        });
    }
}
