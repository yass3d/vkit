use egui::{Color32, CornerRadius, Rect, Stroke, Ui, pos2};

use crate::state::{Action, AppState};
use crate::theme::{COLOR_PRIMARY, CONTROL_RADIUS};

const DRAG_ID: &str = "vkit.reference.dragging";

const SELECTED_EDGE: f32 = 1.5;

pub(super) fn paint_reference_images(ui: &Ui, state: &AppState, viewport: Rect) {
    if state.reference_board.is_empty() || shot_is_framed(state) {
        return;
    }
    let painter = ui.painter().with_clip_rect(viewport);
    for image in state.reference_board.images() {
        if !image.visible {
            continue;
        }
        let rect = image.rect_in(viewport);
        let Some(texture) = texture_for(ui, image) else {
            continue;
        };
        let tint = Color32::WHITE.gamma_multiply(image.opacity);
        painter.image(
            texture.id(),
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            tint,
        );
        if state.reference_board.selected() == Some(image.id) {
            painter.rect_stroke(
                rect,
                CornerRadius::same(CONTROL_RADIUS),
                Stroke::new(SELECTED_EDGE, COLOR_PRIMARY),
                egui::StrokeKind::Inside,
            );
        }
    }
}

pub(super) fn handle_reference_drag(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    model_is_under_pointer: impl FnOnce() -> bool,
) -> bool {
    if state.reference_board.is_empty() || shot_is_framed(state) {
        return false;
    }
    let id = egui::Id::new(DRAG_ID);
    let dragging: Option<u64> = ui.data(|data| data.get_temp(id));

    let (pointer, primary_down, released) = ui.input(|input| {
        (
            input.pointer.interact_pos(),
            input.pointer.primary_down(),
            input.pointer.primary_released(),
        )
    });

    if let Some(image) = dragging {
        if released || !primary_down {
            ui.data_mut(|data| data.remove::<u64>(id));
            return true;
        }
        let delta = ui.input(|input| input.pointer.delta());
        if delta != egui::Vec2::ZERO {
            state.dispatch(Action::DragReferenceImage {
                id: image,
                viewport,
                delta,
            });
        }
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        return true;
    }

    let Some(pointer) = pointer.filter(|pointer| viewport.contains(*pointer)) else {
        return false;
    };
    let Some(hit) = state.reference_board.hit(viewport, pointer) else {
        return false;
    };

    if !ui.input(|input| input.pointer.primary_pressed()) {
        return false;
    }
    if model_is_under_pointer() {
        return false;
    }
    ui.data_mut(|data| data.insert_temp(id, hit));
    state.dispatch(Action::SelectReferenceImage(Some(hit)));
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    true
}

fn shot_is_framed(state: &AppState) -> bool {
    state.hair_thumbnail.is_some()
}

fn texture_for(
    ui: &Ui,
    image: &crate::reference_board::ReferenceImage,
) -> Option<egui::TextureHandle> {
    crate::ui_components::try_stamped_texture(
        ui,
        "vkit.reference.texture",
        image.id,
        image.path.clone(),
        egui::TextureOptions::LINEAR,
        || decode(&image.path),
    )
}

fn decode(path: &std::path::Path) -> Option<egui::ColorImage> {
    let decoded = image::open(path).ok()?.into_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        decoded.as_raw(),
    ))
}

pub(super) fn note_reference_aspects(ui: &Ui, state: &mut AppState) {
    let mut corrections = Vec::new();
    for image in state.reference_board.images() {
        let Some(texture) = texture_for(ui, image) else {
            continue;
        };
        let size = texture.size();
        if size[1] == 0 {
            continue;
        }
        let aspect = size[0] as f32 / size[1] as f32;
        if (aspect - image.aspect).abs() > 1.0e-3 {
            corrections.push((image.id, aspect));
        }
    }
    for (id, aspect) in corrections {
        state.dispatch(Action::NoteReferenceImageAspect { id, aspect });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hovering_a_reference_does_not_take_the_pointer() {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        egui::__run_test_ui(|ui| {
            let mut state = AppState::default();
            state
                .reference_board
                .add(std::path::PathBuf::from("face.png"));
            assert!(
                !handle_reference_drag(ui, &mut state, viewport, || false),
                "no press, no claim"
            );
        });
    }

    #[test]
    fn a_framed_shot_hides_every_reference_and_refuses_the_pointer() {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        egui::__run_test_ui(|ui| {
            let mut state = AppState::default();
            state
                .reference_board
                .add(std::path::PathBuf::from("face.png"));
            assert!(!shot_is_framed(&state));

            state.hair_thumbnail = Some(crate::state::HairThumbnailJob {
                target: crate::state::HairThumbnailTarget::Preset,
                square: None,
                shoot: false,
            });
            assert!(shot_is_framed(&state));
            assert!(
                !handle_reference_drag(ui, &mut state, viewport, || false),
                "a reference cannot be grabbed while the shot is framed"
            );
            paint_reference_images(ui, &state, viewport);
        });
    }

    #[test]
    fn an_empty_board_never_takes_the_pointer_or_paints() {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        egui::__run_test_ui(|ui| {
            let mut state = AppState::default();
            assert!(!handle_reference_drag(ui, &mut state, viewport, || false));
            paint_reference_images(ui, &state, viewport);
        });
    }
}
