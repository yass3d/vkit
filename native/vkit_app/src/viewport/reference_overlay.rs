//! Painting and dragging the reference pictures.
//!
//! Painted straight after the backdrop and before any 3D layer, which is what
//! puts them behind the head without a line of renderer code: the scene surface
//! clears to transparent and blends over whatever egui already drew, so the
//! head simply covers them.
//!
//! A thumbnail must never have one in it, and being an egui shape is NOT
//! enough to guarantee that. `render_portrait` draws the scene into a target of
//! its own and could not pick one up — but the hair thumbnail is a `BitBlt` of
//! the window, which photographs whatever is on screen, references included.
//! So they hide themselves while a shot is framed, the same way every other
//! overlay in the viewport does, on the same `state.hair_thumbnail` flag.

use egui::{Color32, CornerRadius, Rect, Stroke, Ui, pos2};

use crate::state::{Action, AppState};
use crate::theme::{COLOR_PRIMARY, CONTROL_RADIUS};

/// A picture the reader is dragging, held between frames.
const DRAG_ID: &str = "vkit.reference.dragging";

/// How thick the selected picture's edge is drawn.
const SELECTED_EDGE: f32 = 1.5;

/// Paint every visible picture, furthest back first.
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
        // Painted as a shape rather than added as a widget: an `Image` widget
        // would allocate space in whatever layout happens to be open, and this
        // has a rectangle of its own that no layout decides.
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

/// Let the reader pick a picture up and move it.
///
/// Returns `true` when a picture has the pointer, so the viewport leaves the
/// camera and the brushes alone for the rest of the frame.
///
/// Asked LAST, after the head and the hair have had their turn. A reference is
/// behind everything, and a click that reaches it is a click nothing else
/// wanted — which is exactly how it behaves on screen.
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

    // Hovering one is not taking the pointer: the camera and the brushes still
    // work over a reference, because a reference is behind them.
    if !ui.input(|input| input.pointer.primary_pressed()) {
        return false;
    }
    // And neither is a press that landed on the model. A reference is BEHIND
    // the head and the hair, so it only gets a click nothing in front of it
    // wanted — asked here rather than assumed, because "behind" is the whole
    // description of how this thing behaves.
    if model_is_under_pointer() {
        return false;
    }
    ui.data_mut(|data| data.insert_temp(id, hit));
    state.dispatch(Action::SelectReferenceImage(Some(hit)));
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    true
}

/// Whether a thumbnail is being framed right now.
///
/// The hair thumbnail is taken by copying the window off the screen, so
/// anything drawn is in the picture. Every other overlay in the viewport hides
/// itself on this same flag; a reference has more reason to than most, being
/// somebody's photograph.
fn shot_is_framed(state: &AppState) -> bool {
    state.hair_thumbnail.is_some()
}

/// The decoded picture, cached against its own path.
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

/// Tell the board what shape each picture turned out to be.
///
/// Read once per picture, from the texture that is already decoded, and only
/// when it disagrees with what the board holds — a dispatch every frame would
/// mark the session dirty forever.
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

    /// The pointer is only taken on a press, never on a hover.
    ///
    /// A reference sits behind the model. If hovering one took the pointer, the
    /// camera would stop orbiting wherever a photograph happened to be, which
    /// is most of the screen.
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

    /// A photograph must not end up in somebody's preset thumbnail.
    ///
    /// Not a matter of being an egui shape: the hair thumbnail is a copy of the
    /// window off the screen, so it photographs anything that is drawn.
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
