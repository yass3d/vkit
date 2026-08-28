use std::marker::PhantomData;

use egui::{CursorIcon, Id, PointerButton, Rect, Response, Stroke, Ui, pos2};

use crate::theme::{COLOR_PRIMARY, SPACE_2};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Landing<Row> {
    pub row: Row,

    pub insertion_index: usize,
}

pub struct Reorder<Row> {
    id: Id,
    row: PhantomData<Row>,
}

impl<Row> Reorder<Row>
where
    Row: Clone + PartialEq + Send + Sync + 'static,
{
    #[must_use]
    pub fn new(key: &'static str) -> Self {
        Self {
            id: Id::new(key),
            row: PhantomData,
        }
    }

    #[must_use]
    pub fn carried(&self, ui: &Ui) -> Option<Row> {
        ui.data(|data| data.get_temp::<Row>(self.id))
    }

    pub fn picked_up(&self, response: &Response) -> bool {
        response.drag_started_by(PointerButton::Primary)
    }

    pub fn begin(&self, ui: &Ui, row: Row) {
        ui.data_mut(|data| data.insert_temp(self.id, row));
    }

    pub fn cancel(&self, ui: &Ui) {
        ui.data_mut(|data| data.remove::<Row>(self.id));
    }

    pub fn offer(&self, ui: &Ui, rect: Rect, index: usize) -> Option<Landing<Row>> {
        let carried = self.carried(ui)?;
        let pointer = ui.input(|input| input.pointer.interact_pos())?;
        if !rect.contains(pointer) {
            return None;
        }
        let after = pointer.y >= rect.center().y;
        let insertion_index = index + usize::from(after);
        let line_y = if after { rect.bottom() } else { rect.top() };
        ui.painter().line_segment(
            [
                pos2(rect.left() + SPACE_2, line_y),
                pos2(rect.right() - SPACE_2, line_y),
            ],
            Stroke::new(2.0, COLOR_PRIMARY),
        );
        if !ui.input(|input| input.pointer.primary_released()) {
            return None;
        }
        self.cancel(ui);
        Some(Landing {
            row: carried,
            insertion_index,
        })
    }

    pub fn settle(&self, ui: &Ui) {
        if ui.input(|input| input.pointer.primary_released()) {
            self.cancel(ui);
        }
    }

    pub fn cursor(&self, ui: &Ui, row: &Row, hovered: bool) {
        if self.carried(ui).as_ref() == Some(row) {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if hovered && self.carried(ui).is_none() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
    }
}

#[must_use]
pub fn flip(insertion_index: usize, rows: usize) -> usize {
    rows.saturating_sub(insertion_index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gap_seen_upside_down_is_the_same_gap() {
        assert_eq!(flip(0, 3), 3, "above the first row listed is the front");
        assert_eq!(flip(3, 3), 0, "below the last row listed is the back");
        assert_eq!(flip(1, 3), 2);
        assert_eq!(flip(2, 3), 1);
    }

    #[test]
    fn flipping_twice_is_where_you_started() {
        for rows in 0..6 {
            for gap in 0..=rows {
                assert_eq!(flip(flip(gap, rows), rows), gap);
            }
        }
    }

    #[test]
    fn a_list_with_nothing_carried_offers_no_landing() {
        egui::__run_test_ui(|ui| {
            let reorder = Reorder::<u64>::new("vkit.test.reorder");
            let rect = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(100.0, 20.0));
            assert!(reorder.offer(ui, rect, 0).is_none());
            assert!(reorder.carried(ui).is_none());

            reorder.begin(ui, 7);
            assert_eq!(reorder.carried(ui), Some(7));
            reorder.cancel(ui);
            assert!(reorder.carried(ui).is_none());
        });
    }
}
