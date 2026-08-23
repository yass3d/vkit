//! The floating icon toolbox, once, for every tab that wants one.
//!
//! Chrome and nothing else: the island, the grab pill that moves it, the corner
//! that switches it between one column and two, and the grid the caller fills.
//! Which icons go in it is the caller's business and is the only part that
//! differs between the hair tab and the sculpt tab.
//!
//! It was written for the hair tab and lived there. Copying it to a second tab
//! would have been the second implementation of a thing already implemented,
//! and the two would have drifted the first time either was touched.

use egui::{Id, Rect, Sense, Ui, Vec2, pos2, vec2};

use super::DETAIL_HUD_TOGGLE_SIZE;
use crate::theme::{COLOR_TOPBAR, SPACE_2};

/// The strip along the top that the box is dragged by.
const GRAB_HEIGHT: f32 = 14.0;

/// The margin between the island's edge and the icons inside it.
const INSET: f32 = 6.0;

/// The square at the bottom-right that switches the column count.
const CORNER: f32 = 12.0;

const MARGIN: f32 = crate::viewport_chrome::EDGE_INSET;

/// Where a toolbox of this many slots would stand.
///
/// `None` when it would not fit the viewport, which is how a small window ends
/// up with no toolbox rather than one hanging off the bottom.
#[must_use]
pub(super) fn toolbox_rect(
    viewport: Rect,
    slots: usize,
    columns: usize,
    stored: Option<[f32; 2]>,
    default_top: Option<f32>,
) -> Option<Rect> {
    let size = toolbox_size(slots, columns);
    if size.y > viewport.height() - 24.0 {
        return None;
    }
    let default_min = pos2(
        viewport.right() - size.x - MARGIN,
        default_top.unwrap_or(viewport.center().y - size.y * 0.5),
    );
    Some(crate::ui_components::island_rect(
        viewport,
        size,
        stored,
        default_min,
        MARGIN,
    ))
}

/// How big a toolbox of this shape is, before it is placed.
#[must_use]
pub(super) fn toolbox_size(slots: usize, columns: usize) -> Vec2 {
    let columns = columns.max(1);
    let rows = slots.div_ceil(columns) as f32;
    let columns = columns as f32;
    vec2(
        columns * DETAIL_HUD_TOGGLE_SIZE + (columns - 1.0) * SPACE_2 + INSET * 2.0,
        GRAB_HEIGHT + rows * DETAIL_HUD_TOGGLE_SIZE + (rows - 1.0) * SPACE_2 + INSET * 2.0,
    )
}

/// Paint the island and its handles, and hand back a `Ui` per slot.
///
/// `columns` is read and written: dragging the corner changes it, and the
/// caller owns where that number lives so each toolbox remembers its own.
pub(super) fn draw_toolbox(
    ui: &mut Ui,
    id: Id,
    rect: Rect,
    slots: usize,
    position: &mut Option<[f32; 2]>,
    columns: &mut u8,
) -> Vec<Ui> {
    // Nothing under a toolbox is reachable through it: the island is opaque,
    // and a click that lands on it must not also orbit the camera behind it.
    let _blocker = ui.interact(rect, id.with("blocker"), Sense::click_and_drag());
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(
        rect,
        f32::from(crate::theme::RADIUS_POPOVER),
        COLOR_TOPBAR.gamma_multiply(0.96),
    );

    let content = rect.shrink(INSET);
    let grab_rect = Rect::from_min_size(content.min, vec2(content.width(), GRAB_HEIGHT));
    let grab = ui.interact(grab_rect, id.with("grab"), Sense::drag());
    let pill = Rect::from_center_size(grab_rect.center(), vec2(26.0, 5.0));
    painter.rect_filled(
        pill,
        2.5,
        if grab.hovered() || grab.dragged() {
            crate::theme::COLOR_TEXT.gamma_multiply(0.55)
        } else {
            crate::theme::COLOR_MUTED.gamma_multiply(0.5)
        },
    );
    crate::ui_components::island_move_handle(&grab, rect, position);

    let corner = Rect::from_min_max(rect.max - Vec2::splat(CORNER), rect.max);
    let corner_grab = ui.interact(corner, id.with("corner"), Sense::drag());
    if corner_grab.hovered() || corner_grab.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
    }
    if corner_grab.dragged()
        && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
    {
        let one = toolbox_size(slots, 1).x;
        let two = toolbox_size(slots, 2).x;
        let wanted: u8 = if pointer.x - rect.left() < (one + two) * 0.5 {
            1
        } else {
            2
        };
        if wanted != *columns {
            *columns = wanted;
        }
    }
    for step in 1..=3 {
        let offset = step as f32 * 3.0;
        painter.line_segment(
            [
                pos2(rect.right() - offset, rect.bottom() - 3.0),
                pos2(rect.right() - 3.0, rect.bottom() - offset),
            ],
            egui::Stroke::new(
                1.0,
                if corner_grab.hovered() || corner_grab.dragged() {
                    crate::theme::COLOR_TEXT.gamma_multiply(0.6)
                } else {
                    crate::theme::COLOR_MUTED.gamma_multiply(0.45)
                },
            ),
        );
    }

    let across = (*columns).clamp(1, 2) as usize;
    (0..slots)
        .map(|index| {
            let cell = Rect::from_min_size(
                pos2(
                    content.left() + (index % across) as f32 * (DETAIL_HUD_TOGGLE_SIZE + SPACE_2),
                    content.top()
                        + GRAB_HEIGHT
                        + (index / across) as f32 * (DETAIL_HUD_TOGGLE_SIZE + SPACE_2),
                ),
                Vec2::splat(DETAIL_HUD_TOGGLE_SIZE),
            );
            let mut cell_ui = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(id.with(("cell", index)))
                    .max_rect(cell)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            cell_ui.shrink_clip_rect(rect);
            cell_ui
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second column halves the rows and roughly doubles the width.
    #[test]
    fn two_columns_are_wider_and_shorter_than_one() {
        let one = toolbox_size(6, 1);
        let two = toolbox_size(6, 2);
        assert!(two.x > one.x);
        assert!(two.y < one.y);
    }

    /// An odd slot count still gets a row for the leftover.
    #[test]
    fn an_odd_slot_count_keeps_a_row_for_the_last_one() {
        let five = toolbox_size(5, 2);
        let six = toolbox_size(6, 2);
        assert!((five.y - six.y).abs() < 0.01, "both need three rows");
        let four = toolbox_size(4, 2);
        assert!(four.y < five.y, "four fit in two rows and five do not");
    }

    /// It refuses to exist rather than hang off the edge of a short viewport.
    #[test]
    fn a_short_viewport_gets_no_toolbox_at_all() {
        let tall = Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 700.0));
        let squashed = Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 60.0));
        assert!(toolbox_rect(tall, 6, 1, None, None).is_some());
        assert!(toolbox_rect(squashed, 6, 1, None, None).is_none());
    }

    /// Two toolboxes on one edge stack rather than sit on top of each other.
    #[test]
    fn a_given_top_places_the_box_where_it_is_asked_to_go() {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 700.0));
        let centred = toolbox_rect(viewport, 4, 1, None, None).unwrap();
        let raised = toolbox_rect(viewport, 4, 1, None, Some(80.0)).unwrap();
        assert!((raised.top() - 80.0).abs() < 0.01);
        assert!(raised.top() < centred.top());
        assert!((raised.left() - centred.left()).abs() < 0.01, "same edge");
    }
}
