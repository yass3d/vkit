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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReferenceBoard {
    images: Vec<ReferenceImage>,
    next_id: u64,
    selected: Option<u64>,
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
    pub fn selected(&self) -> Option<u64> {
        self.selected
    }

    pub fn select(&mut self, id: Option<u64>) {
        self.selected = id.filter(|id| self.images.iter().any(|image| image.id == *id));
    }

    pub fn add(&mut self, path: PathBuf) -> u64 {
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "reference".to_owned());
        self.next_id += 1;
        let id = self.next_id;
        self.images.push(ReferenceImage {
            id,
            path,
            name,
            center: vec2(0.5, 0.5),
            height_share: 0.5,
            aspect: 1.0,
            opacity: 1.0,
            visible: true,
        });
        self.selected = Some(id);
        id
    }

    pub fn remove(&mut self, id: u64) {
        self.images.retain(|image| image.id != id);
        if self.selected == Some(id) {
            self.selected = self.images.last().map(|image| image.id);
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

    pub fn reorder(&mut self, id: u64, toward_front: bool) {
        let Some(index) = self.images.iter().position(|image| image.id == id) else {
            return;
        };
        if toward_front {
            if index + 1 < self.images.len() {
                self.images.swap(index, index + 1);
            }
        } else if index > 0 {
            self.images.swap(index - 1, index);
        }
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
    pub fn hit(&self, viewport: Rect, point: egui::Pos2) -> Option<u64> {
        self.images
            .iter()
            .rev()
            .find(|image| image.visible && image.rect_in(viewport).contains(point))
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
    fn a_click_lands_on_the_front_picture_and_passes_through_a_hidden_one() {
        let mut board = ReferenceBoard::default();
        let back = board.add(PathBuf::from("back.png"));
        let front = board.add(PathBuf::from("front.png"));

        let middle = viewport().center();
        assert_eq!(board.hit(viewport(), middle), Some(front));

        board.toggle_visible(front);
        assert_eq!(board.hit(viewport(), middle), Some(back));

        board.toggle_visible(back);
        assert_eq!(board.hit(viewport(), middle), None);
    }

    #[test]
    fn a_click_outside_every_picture_lands_on_none() {
        let mut board = ReferenceBoard::default();
        board.add(PathBuf::from("face.png"));
        assert_eq!(board.hit(viewport(), pos2(105.0, 55.0)), None);
    }

    #[test]
    fn reordering_stops_at_the_ends_instead_of_wrapping() {
        let mut board = ReferenceBoard::default();
        let first = board.add(PathBuf::from("a.png"));
        let second = board.add(PathBuf::from("b.png"));

        board.reorder(first, false);
        assert_eq!(board.images()[0].id, first, "already at the back");

        board.reorder(first, true);
        assert_eq!(board.images()[1].id, first);
        assert_eq!(board.images()[0].id, second);

        board.reorder(first, true);
        assert_eq!(board.images()[1].id, first, "already at the front");
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
        board.select(Some(id + 99));
        assert_eq!(board.selected(), None);
    }
}
