use std::path::PathBuf;

use egui::{Rect, Vec2, pos2, vec2};

pub const MAX_HEIGHT_SHARE: f32 = 4.0;

pub const MIN_HEIGHT_SHARE: f32 = 0.05;

#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceImage {
    pub id: u64,
    pub path: PathBuf,
    pub name: String,

    pub center: Vec2,

    pub height_share: f32,

    pub aspect: f32,

    pub opacity: f32,
    pub visible: bool,

    pub locked: bool,
}

impl ReferenceImage {
    #[must_use]
    pub fn rect_in(&self, viewport: Rect) -> Rect {
        let height = viewport.height() * self.height_share;
        let size = vec2(height * self.aspect.max(0.01), height);
        Rect::from_center_size(
            pos2(
                viewport.left() + viewport.width() * self.center.x,
                viewport.top() + viewport.height() * self.center.y,
            ),
            size,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceRow {
    Head,
    Image(u64),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReferenceBoard {
    images: Vec<ReferenceImage>,

    head_index: usize,

    hidden: bool,
    next_id: u64,

    selected: Option<ReferenceRow>,
}

impl ReferenceBoard {
    #[must_use]
    pub fn images(&self) -> &[ReferenceImage] {
        &self.images
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    #[must_use]
    pub const fn shown(&self) -> bool {
        !self.hidden
    }

    pub const fn show(&mut self, shown: bool) {
        self.hidden = !shown;
    }

    #[must_use]
    pub fn selected_row(&self) -> Option<ReferenceRow> {
        self.selected
    }

    #[must_use]
    pub fn selected(&self) -> Option<u64> {
        match self.selected {
            Some(ReferenceRow::Image(id)) => Some(id),
            _ => None,
        }
    }

    pub fn select(&mut self, row: Option<ReferenceRow>) {
        self.selected = row.filter(|row| match row {
            ReferenceRow::Head => true,
            ReferenceRow::Image(id) => self.images.iter().any(|image| image.id == *id),
        });
    }

    #[must_use]
    pub fn is_locked(&self, id: u64) -> bool {
        self.get(id).is_some_and(|image| image.locked)
    }

    pub fn toggle_locked(&mut self, id: u64) {
        if let Some(image) = self.get_mut(id) {
            image.locked = !image.locked;
        }
    }

    #[must_use]
    pub fn rows(&self) -> Vec<ReferenceRow> {
        let head = self.head_index.min(self.images.len());
        let mut rows: Vec<ReferenceRow> = Vec::with_capacity(self.images.len() + 1);
        rows.extend(
            self.images[..head]
                .iter()
                .map(|image| ReferenceRow::Image(image.id)),
        );
        rows.push(ReferenceRow::Head);
        rows.extend(
            self.images[head..]
                .iter()
                .map(|image| ReferenceRow::Image(image.id)),
        );
        rows
    }

    #[must_use]
    pub fn behind(&self) -> &[ReferenceImage] {
        &self.images[..self.head_index.min(self.images.len())]
    }

    #[must_use]
    pub fn in_front(&self) -> &[ReferenceImage] {
        &self.images[self.head_index.min(self.images.len())..]
    }

    #[must_use]
    pub fn is_in_front(&self, id: u64) -> bool {
        self.in_front().iter().any(|image| image.id == id)
    }

    pub fn add(&mut self, path: PathBuf) -> u64 {
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "reference".to_owned());
        self.next_id += 1;
        let id = self.next_id;
        let at = self.head_index.min(self.images.len());
        self.images.insert(
            at,
            ReferenceImage {
                id,
                path,
                name,
                center: vec2(0.5, 0.5),
                height_share: 0.5,
                aspect: 1.0,
                opacity: 1.0,
                visible: true,
                locked: false,
            },
        );
        self.head_index = at + 1;
        self.selected = Some(ReferenceRow::Image(id));
        id
    }

    pub fn remove(&mut self, id: u64) {
        if let Some(index) = self.images.iter().position(|image| image.id == id) {
            self.images.remove(index);
            if index < self.head_index {
                self.head_index -= 1;
            }
        }
        if self.selected == Some(ReferenceRow::Image(id)) {
            self.selected = self
                .images
                .last()
                .map(|image| ReferenceRow::Image(image.id));
        }
    }

    #[must_use]
    pub fn get(&self, id: u64) -> Option<&ReferenceImage> {
        self.images.iter().find(|image| image.id == id)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut ReferenceImage> {
        self.images.iter_mut().find(|image| image.id == id)
    }

    pub fn toggle_visible(&mut self, id: u64) {
        if let Some(image) = self.get_mut(id) {
            image.visible = !image.visible;
        }
    }

    pub fn step_row(&mut self, row: ReferenceRow, toward_front: bool) {
        let rows = self.rows();
        let Some(from) = rows.iter().position(|candidate| *candidate == row) else {
            return;
        };
        if toward_front {
            self.move_row(row, from + 2);
        } else if from > 0 {
            self.move_row(row, from - 1);
        }
    }

    pub fn move_row(&mut self, row: ReferenceRow, insertion_index: usize) {
        let mut rows = self.rows();
        let Some(from) = rows.iter().position(|candidate| *candidate == row) else {
            return;
        };
        let taken = rows.remove(from);
        let to = insertion_index
            .saturating_sub(usize::from(from < insertion_index))
            .min(rows.len());
        rows.insert(to, taken);
        self.adopt(&rows);
    }

    fn adopt(&mut self, rows: &[ReferenceRow]) {
        let mut images = Vec::with_capacity(self.images.len());
        let mut head_index = 0;
        for row in rows {
            match row {
                ReferenceRow::Head => head_index = images.len(),
                ReferenceRow::Image(id) => {
                    if let Some(image) = self.images.iter().find(|image| image.id == *id) {
                        images.push(image.clone());
                    }
                }
            }
        }
        self.images = images;
        self.head_index = head_index;
    }

    pub fn note_aspect(&mut self, id: u64, aspect: f32) {
        if let Some(image) = self.get_mut(id)
            && aspect.is_finite()
            && aspect > 0.0
        {
            image.aspect = aspect;
        }
    }

    #[must_use]
    pub fn grabbed(&self, viewport: Rect, point: egui::Pos2) -> Option<u64> {
        self.images
            .iter()
            .rev()
            .find(|image| image.visible && !image.locked && image.rect_in(viewport).contains(point))
            .map(|image| image.id)
    }

    pub fn drag(&mut self, id: u64, viewport: Rect, delta: Vec2) {
        if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
            return;
        }
        let shift = vec2(delta.x / viewport.width(), delta.y / viewport.height());
        let Some(image) = self.get_mut(id) else {
            return;
        };
        if image.locked {
            return;
        }
        image.center += shift;
        clamp_into_view(image);
    }

    pub fn resize(&mut self, id: u64, height_share: f32) {
        if let Some(image) = self.get_mut(id) {
            image.height_share = height_share.clamp(MIN_HEIGHT_SHARE, MAX_HEIGHT_SHARE);
            clamp_into_view(image);
        }
    }

    pub fn set_opacity(&mut self, id: u64, opacity: f32) {
        if let Some(image) = self.get_mut(id) {
            image.opacity = opacity.clamp(0.0, 1.0);
        }
    }
}

fn clamp_into_view(image: &mut ReferenceImage) {
    image.center.x = image.center.x.clamp(0.0, 1.0);
    image.center.y = image.center.y.clamp(0.0, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), vec2(800.0, 400.0))
    }

    #[test]
    fn a_picture_arrives_centred_and_selected() {
        let mut board = ReferenceBoard::default();
        let id = board.add(PathBuf::from("face.png"));
        assert_eq!(board.selected(), Some(id));

        let rect = board.get(id).unwrap().rect_in(viewport());
        assert!((rect.center() - viewport().center()).length() < 0.01);
        assert!((rect.height() - 200.0).abs() < 0.01, "half the viewport");
    }

    #[test]
    fn a_resized_window_leaves_a_picture_where_it_was_put() {
        let mut board = ReferenceBoard::default();
        let id = board.add(PathBuf::from("face.png"));
        board.drag(id, viewport(), vec2(200.0, 0.0));

        let wide = Rect::from_min_size(pos2(0.0, 0.0), vec2(1600.0, 800.0));
        let before = board.get(id).unwrap().rect_in(viewport());
        let after = board.get(id).unwrap().rect_in(wide);
        let share_before = (before.center().x - viewport().left()) / viewport().width();
        let share_after = (after.center().x - wide.left()) / wide.width();
        assert!((share_before - share_after).abs() < 1.0e-5);
    }

    #[test]
    fn a_grab_lands_on_the_front_picture_and_passes_through_a_hidden_one() {
        let mut board = ReferenceBoard::default();
        let back = board.add(PathBuf::from("back.png"));
        let front = board.add(PathBuf::from("front.png"));

        let middle = viewport().center();
        assert_eq!(board.grabbed(viewport(), middle), Some(front));

        board.toggle_visible(front);
        assert_eq!(board.grabbed(viewport(), middle), Some(back));

        board.toggle_visible(back);
        assert_eq!(board.grabbed(viewport(), middle), None);
    }

    #[test]
    fn a_grab_outside_every_picture_lands_on_none() {
        let mut board = ReferenceBoard::default();
        board.add(PathBuf::from("face.png"));
        assert_eq!(board.grabbed(viewport(), pos2(105.0, 55.0)), None);
    }

    #[test]
    fn reordering_stops_at_the_ends_instead_of_wrapping() {
        let mut board = ReferenceBoard::default();
        let first = board.add(PathBuf::from("a.png"));
        let second = board.add(PathBuf::from("b.png"));

        board.step_row(ReferenceRow::Image(first), false);
        assert_eq!(board.images()[0].id, first, "already at the back");

        board.step_row(ReferenceRow::Image(first), true);
        assert_eq!(board.images()[1].id, first);
        assert_eq!(board.images()[0].id, second);

        board.step_row(ReferenceRow::Head, true);
        assert_eq!(
            board.rows().last(),
            Some(&ReferenceRow::Head),
            "the head was already in front of everything",
        );
    }

    #[test]
    fn the_master_switch_puts_back_exactly_what_it_took_away() {
        let mut board = ReferenceBoard::default();
        let front = board.add(PathBuf::from("a.png"));
        let back = board.add(PathBuf::from("b.png"));
        board.toggle_visible(back);
        assert!(board.shown(), "a board starts on");

        board.show(false);
        assert!(!board.shown());
        assert!(
            board.get(front).is_some_and(|image| image.visible),
            "the master switch reached into a picture's own eye",
        );
        assert!(board.get(back).is_some_and(|image| !image.visible));

        board.show(true);
        assert!(board.shown());
        assert!(board.get(front).is_some_and(|image| image.visible));
        assert!(
            board.get(back).is_some_and(|image| !image.visible),
            "the one that was hidden came back on",
        );
    }

    #[test]
    fn a_new_picture_is_a_backdrop_until_somebody_moves_it() {
        let mut board = ReferenceBoard::default();
        let first = board.add(PathBuf::from("a.png"));
        let second = board.add(PathBuf::from("b.png"));

        assert_eq!(
            board.rows(),
            vec![
                ReferenceRow::Image(first),
                ReferenceRow::Image(second),
                ReferenceRow::Head,
            ],
            "the newest picture sits in front of the older ones, behind the head",
        );
        assert_eq!(board.behind().len(), 2);
        assert!(board.in_front().is_empty());
        assert!(!board.is_in_front(second));
    }

    #[test]
    fn a_picture_carried_above_the_head_becomes_an_overlay() {
        let mut board = ReferenceBoard::default();
        let back = board.add(PathBuf::from("a.png"));
        let front = board.add(PathBuf::from("b.png"));

        board.move_row(ReferenceRow::Image(front), 3);
        assert_eq!(
            board.rows(),
            vec![
                ReferenceRow::Image(back),
                ReferenceRow::Head,
                ReferenceRow::Image(front),
            ],
        );
        assert!(board.is_in_front(front));
        assert!(!board.is_in_front(back));
        assert_eq!(board.behind().len(), 1);
        assert_eq!(board.in_front().len(), 1);
    }

    #[test]
    fn moving_the_head_down_says_the_same_thing_as_moving_the_pictures_up() {
        let mut by_head = ReferenceBoard::default();
        let back = by_head.add(PathBuf::from("a.png"));
        let front = by_head.add(PathBuf::from("b.png"));
        by_head.move_row(ReferenceRow::Head, 1);

        assert_eq!(
            by_head.rows(),
            vec![
                ReferenceRow::Image(back),
                ReferenceRow::Head,
                ReferenceRow::Image(front),
            ],
            "the head landed between the two pictures",
        );
        assert!(by_head.is_in_front(front));
    }

    #[test]
    fn taking_a_backdrop_away_leaves_the_head_where_it_was() {
        let mut board = ReferenceBoard::default();
        let back = board.add(PathBuf::from("a.png"));
        let front = board.add(PathBuf::from("b.png"));
        board.move_row(ReferenceRow::Image(front), 3);
        assert!(board.is_in_front(front));

        board.remove(back);
        assert!(
            board.is_in_front(front),
            "the overlay became a backdrop when its neighbour left",
        );
        assert_eq!(
            board.rows(),
            vec![ReferenceRow::Head, ReferenceRow::Image(front)],
        );
    }

    #[test]
    fn every_picture_is_on_exactly_one_side_of_the_head() {
        let mut board = ReferenceBoard::default();
        let ids: Vec<u64> = (0..4)
            .map(|index| board.add(PathBuf::from(format!("{index}.png"))))
            .collect();
        for gap in 0..=5 {
            board.move_row(ReferenceRow::Head, gap);
            let split = board.behind().len() + board.in_front().len();
            assert_eq!(split, ids.len(), "a picture went missing at gap {gap}");
            assert_eq!(board.rows().len(), ids.len() + 1);
            assert_eq!(
                board
                    .rows()
                    .iter()
                    .filter(|row| **row == ReferenceRow::Head)
                    .count(),
                1,
                "the head appeared twice at gap {gap}",
            );
        }
    }

    #[test]
    fn a_picture_keeps_a_corner_on_screen_however_far_it_is_dragged() {
        for (height_share, aspect) in [(0.5, 1.0), (MAX_HEIGHT_SHARE, 3.0), (MIN_HEIGHT_SHARE, 0.2)]
        {
            let mut board = ReferenceBoard::default();
            let id = board.add(PathBuf::from("face.png"));
            board.note_aspect(id, aspect);
            board.resize(id, height_share);
            for delta in [
                vec2(100_000.0, 100_000.0),
                vec2(-100_000.0, -100_000.0),
                vec2(100_000.0, -100_000.0),
            ] {
                board.drag(id, viewport(), delta);
                let rect = board.get(id).unwrap().rect_in(viewport());
                assert!(
                    rect.intersects(viewport()),
                    "at {height_share}x{aspect}, dragged to {rect:?}, off {:?}",
                    viewport()
                );
            }
        }
    }

    #[test]
    fn removing_the_selected_picture_falls_back_to_another() {
        let mut board = ReferenceBoard::default();
        let first = board.add(PathBuf::from("a.png"));
        let second = board.add(PathBuf::from("b.png"));

        board.remove(second);
        assert_eq!(board.selected(), Some(first));

        board.remove(first);
        assert_eq!(board.selected(), None);
        assert!(board.is_empty());
    }

    #[test]
    fn a_size_stays_between_a_speck_and_a_wall() {
        let mut board = ReferenceBoard::default();
        let id = board.add(PathBuf::from("face.png"));

        board.resize(id, 900.0);
        assert!((board.get(id).unwrap().height_share - MAX_HEIGHT_SHARE).abs() < 1.0e-6);

        board.resize(id, -4.0);
        assert!((board.get(id).unwrap().height_share - MIN_HEIGHT_SHARE).abs() < 1.0e-6);
    }

    #[test]
    fn one_file_may_be_added_more_than_once() {
        let mut board = ReferenceBoard::default();
        let first = board.add(PathBuf::from("face.png"));
        let second = board.add(PathBuf::from("face.png"));
        assert_ne!(first, second);
        assert_eq!(board.images().len(), 2);
    }

    #[test]
    fn noting_the_aspect_widens_a_picture_without_moving_its_centre() {
        let mut board = ReferenceBoard::default();
        let id = board.add(PathBuf::from("face.png"));
        let before = board.get(id).unwrap().rect_in(viewport()).center();

        board.note_aspect(id, 2.0);
        let after = board.get(id).unwrap().rect_in(viewport());
        assert!((after.center() - before).length() < 1.0e-5);
        assert!((after.width() - after.height() * 2.0).abs() < 1.0e-3);

        board.note_aspect(id, 0.0);
        board.note_aspect(id, f32::NAN);
        assert!(
            (board.get(id).unwrap().aspect - 2.0).abs() < 1.0e-6,
            "refused"
        );
    }

    #[test]
    fn selecting_something_that_is_not_there_selects_nothing() {
        let mut board = ReferenceBoard::default();
        let id = board.add(PathBuf::from("face.png"));
        board.select(Some(ReferenceRow::Image(id + 99)));
        assert_eq!(board.selected(), None);

        board.select(Some(ReferenceRow::Head));
        assert_eq!(board.selected_row(), Some(ReferenceRow::Head));
        assert_eq!(
            board.selected(),
            None,
            "the divider is not a picture, so no picture-only command may aim at it",
        );
    }

    #[test]
    fn a_locked_picture_is_deaf_to_the_pointer() {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        let mut board = ReferenceBoard::default();
        let id = board.add(PathBuf::from("face.png"));
        let middle = board.get(id).unwrap().rect_in(viewport).center();
        assert_eq!(board.grabbed(viewport, middle), Some(id));

        board.toggle_locked(id);
        assert!(board.is_locked(id));
        assert_eq!(
            board.grabbed(viewport, middle),
            None,
            "a locked picture still answered the pointer",
        );

        let before = board.get(id).unwrap().center;
        board.drag(id, viewport, vec2(40.0, 40.0));
        assert_eq!(
            board.get(id).unwrap().center,
            before,
            "a locked picture moved anyway",
        );

        board.toggle_locked(id);
        board.drag(id, viewport, vec2(40.0, 40.0));
        assert_ne!(
            board.get(id).unwrap().center,
            before,
            "unlocking did not give it back",
        );
    }
}
