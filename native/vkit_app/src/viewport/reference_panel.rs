use egui::{Align, Align2, FontId, Layout, Rect, Response, Sense, Ui, pos2, vec2};

use crate::i18n::{TextKey, text};
use crate::list_reorder::{Reorder, flip};
use crate::reference_board::{MAX_HEIGHT_SHARE, MIN_HEIGHT_SHARE, ReferenceRow};
use crate::state::{Action, AppState};
use crate::theme::{COLOR_MUTED, COLOR_TEXT, FONT_SM, FONT_XS, SPACE_1, SPACE_2};
use crate::ui_components::{
    FilledNumericSlider, Icon, icon_button, icon_button_size, paint_list_row_highlight,
};

fn reorder() -> Reorder<ReferenceRow> {
    Reorder::new("vkit.viewport.reference-reorder")
}

const ROW_HEIGHT: f32 = 26.0;

const BADGE_SIZE: f32 = 11.0;

const VISIBLE_ROWS: usize = 6;

pub(super) fn draw_reference_section(ui: &mut Ui, state: &mut AppState) {
    ui.add_space(SPACE_2);
    let mut shown = state.reference_board.shown();
    if crate::ui_components::switch_row(
        ui,
        &mut shown,
        text(state.locale, TextKey::ReferenceImages),
    )
    .changed()
    {
        state.dispatch(Action::ShowReferenceImages(shown));
    }

    let stack: Vec<ReferenceRow> = state.reference_board.rows().into_iter().rev().collect();
    let shown = stack.len().clamp(1, VISIBLE_ROWS);
    egui::ScrollArea::vertical()
        .id_salt("vkit.viewport.reference-list")
        .max_height(ROW_HEIGHT * shown as f32)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if state.reference_board.is_empty() {
                empty_row(ui, state.locale);
                return;
            }
            let depth = stack.len();
            for (place, row) in stack.into_iter().enumerate() {
                match row {
                    ReferenceRow::Head => draw_head_row(ui, state, place, depth),
                    ReferenceRow::Image(id) => draw_row(ui, state, id, place, depth),
                }
            }
            reorder().settle(ui);
        });

    layer_controls(ui, state);

    if let Some(selected) = state.reference_board.selected() {
        draw_selected_controls(ui, state, selected);
    }
}

fn layer_controls(ui: &mut Ui, state: &mut AppState) {
    let row = state.reference_board.selected_row();
    let image = state.reference_board.selected();
    let visible = image.is_some_and(|id| {
        state
            .reference_board
            .get(id)
            .is_some_and(|image| image.visible)
    });
    let locked = image.is_some_and(|id| state.reference_board.is_locked(id));

    ui.add_space(SPACE_1);
    let mut wanted: Option<Action> = None;
    let mut browse = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = SPACE_1;

        browse = icon_button(
            ui,
            Icon::Plus,
            text(state.locale, TextKey::ReferenceImageAdd),
        )
        .clicked();

        let mut step = |ui: &mut Ui, icon: Icon, key: TextKey, toward_front: bool| {
            let button = ui.add_enabled_ui(row.is_some(), |ui| {
                icon_button(ui, icon, text(state.locale, key))
            });
            if button.inner.clicked()
                && let Some(row) = row
            {
                wanted = Some(Action::ReorderReferenceRow { row, toward_front });
            }
        };
        step(ui, Icon::ChevronUp, TextKey::ReferenceImageRaise, true);
        step(ui, Icon::ChevronDown, TextKey::ReferenceImageLower, false);

        let tail = icon_button_size(ui) * 3.0 + ui.spacing().item_spacing.x * 2.0;
        ui.add_space((ui.available_width() - tail).max(SPACE_2));

        let eye = ui.add_enabled_ui(image.is_some(), |ui| {
            icon_button(
                ui,
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
        });
        if eye.inner.clicked()
            && let Some(id) = image
        {
            wanted = Some(Action::ToggleReferenceImageVisible(id));
        }

        let lock = ui.add_enabled_ui(image.is_some(), |ui| {
            icon_button(
                ui,
                Icon::Lock,
                text(
                    state.locale,
                    if locked {
                        TextKey::ReferenceImageUnlock
                    } else {
                        TextKey::ReferenceImageLock
                    },
                ),
            )
        });
        if lock.inner.clicked()
            && let Some(id) = image
        {
            wanted = Some(Action::ToggleReferenceImageLocked(id));
        }

        let trash = ui.add_enabled_ui(image.is_some(), |ui| {
            icon_button(
                ui,
                Icon::Trash,
                text(state.locale, TextKey::ReferenceImageRemove),
            )
        });
        if trash.inner.clicked()
            && let Some(id) = image
        {
            wanted = Some(Action::RemoveReferenceImage(id));
        }
    });
    if let Some(action) = wanted {
        state.dispatch(action);
    }
    if browse
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

fn draw_head_row(ui: &mut Ui, state: &mut AppState, place: usize, depth: usize) {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), ROW_HEIGHT),
        Sense::click_and_drag(),
    );
    let selected = state.reference_board.selected_row() == Some(ReferenceRow::Head);
    paint_list_row_highlight(ui, rect, selected, response.hovered());

    let label = text(state.locale, TextKey::ReferenceHeadRow);
    let label_width = ui
        .painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(FONT_SM), COLOR_TEXT)
        .size()
        .x;
    let text_left = rect.left() + SPACE_2;
    let painter = ui.painter().with_clip_rect(rect);
    painter.text(
        pos2(text_left, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(FONT_SM),
        COLOR_TEXT,
    );
    let rule = egui::Stroke::new(1.0, COLOR_MUTED.gamma_multiply(0.7));
    let after = text_left + label_width + SPACE_2;
    if after < rect.right() - SPACE_1 {
        painter.line_segment(
            [
                pos2(after, rect.center().y),
                pos2(rect.right() - SPACE_1, rect.center().y),
            ],
            rule,
        );
    }
    painter.line_segment(
        [
            pos2(rect.left() + SPACE_1, rect.center().y),
            pos2(text_left - SPACE_1, rect.center().y),
        ],
        rule,
    );

    let on_control = false;
    if response.clicked() || response.drag_started_by(egui::PointerButton::Primary) {
        state.dispatch(Action::SelectReferenceRow(Some(ReferenceRow::Head)));
    }
    drag_row(
        ui,
        state,
        ReferenceRow::Head,
        rect,
        &response,
        place,
        depth,
        on_control,
    );
    if !on_control {
        response.on_hover_text(text(state.locale, TextKey::ReferenceHeadRowHint));
    }
}

#[allow(clippy::too_many_arguments)]
fn drag_row(
    ui: &mut Ui,
    state: &mut AppState,
    row: ReferenceRow,
    rect: Rect,
    response: &Response,
    place: usize,
    depth: usize,
    on_control: bool,
) {
    let reorder = reorder();
    if reorder.picked_up(response) && !on_control {
        reorder.begin(ui, row);
    }
    if let Some(landing) = reorder.offer(ui, rect, place) {
        state.dispatch(Action::MoveReferenceRow {
            row: landing.row,
            insertion_index: flip(landing.insertion_index, depth),
        });
    }
    if !on_control {
        reorder.cursor(ui, &row, response.hovered());
    }
}

fn draw_row(ui: &mut Ui, state: &mut AppState, id: u64, place: usize, depth: usize) {
    let Some(image) = state.reference_board.get(id) else {
        return;
    };
    let name = image.name.clone();
    let visible = image.visible;
    let locked = image.locked;
    let selected = state.reference_board.selected_row() == Some(ReferenceRow::Image(id));

    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), ROW_HEIGHT),
        Sense::click_and_drag(),
    );
    paint_list_row_highlight(ui, rect, selected, response.hovered());

    let mut row_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(SPACE_1, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    let mut badges = Vec::new();
    if !visible {
        badges.push(Icon::EyeClosed);
    }
    if locked {
        badges.push(Icon::Lock);
    }
    for badge in badges {
        let lane = icon_button_size(&row_ui);
        let (slot, _) = row_ui.allocate_exact_size(vec2(lane, lane), Sense::hover());
        crate::ui_components::paint_icon(
            row_ui.painter(),
            Rect::from_center_size(slot.center(), vec2(BADGE_SIZE, BADGE_SIZE)),
            badge,
            COLOR_MUTED,
        );
    }
    let on_control = false;

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

    if (response.clicked() || response.drag_started_by(egui::PointerButton::Primary)) && !on_control
    {
        state.dispatch(Action::SelectReferenceRow(Some(ReferenceRow::Image(id))));
    }
    drag_row(
        ui,
        state,
        ReferenceRow::Image(id),
        rect,
        &response,
        place,
        depth,
        on_control,
    );
}

fn draw_selected_controls(ui: &mut Ui, state: &mut AppState, id: u64) {
    let Some(image) = state.reference_board.get(id) else {
        return;
    };
    let mut height_share = image.height_share;
    let mut opacity = image.opacity;

    ui.add_space(SPACE_1);
    heading(ui, text(state.locale, TextKey::Size));
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
    ui.add_space(SPACE_1);
    heading(ui, text(state.locale, TextKey::Opacity));
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
