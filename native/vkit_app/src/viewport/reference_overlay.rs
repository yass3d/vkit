use egui::{Color32, Rect, Ui, pos2};

use crate::state::{Action, AppState};

const DRAG_ID: &str = "vkit.reference.dragging";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Band {
    Behind,

    InFront,
}

pub(super) fn paint_reference_images(ui: &Ui, state: &AppState, viewport: Rect, band: Band) {
    if !state.reference_board.shown() || state.reference_board.is_empty() || shot_is_framed(state) {
        return;
    }
    let painter = ui.painter().with_clip_rect(viewport);
    let images = match band {
        Band::Behind => state.reference_board.behind(),
        Band::InFront => state.reference_board.in_front(),
    };
    for image in images {
        if !image.visible {
            continue;
        }
        let rect = image.rect_in(viewport);
        let Some(texture) = texture_for(ui, image) else {
            continue;
        };
        let tint = Color32::WHITE.gamma_multiply(image.opacity);
        painter.add(egui::Shape::mesh(rounded_image_mesh(
            texture.id(),
            rect,
            tint,
        )));

        let selected = state.reference_board.selected() == Some(image.id);

        if !selected {
            continue;
        }
        painter.rect_stroke(
            rect,
            CORNER_RADIUS,
            egui::Stroke::new(1.0, Color32::WHITE.gamma_multiply(0.85)),
            egui::StrokeKind::Inside,
        );
        paint_corner_controls(&painter, rect, image.locked);
        if !image.locked {
            paint_resize_grip(&painter, rect);
        }
    }
}

const CORNER_RADIUS: f32 = 8.0;

pub(super) fn rounded_image_mesh(
    texture: egui::TextureId,
    rect: Rect,
    tint: Color32,
) -> egui::Mesh {
    const ARC_STEPS: usize = 6;

    let mut mesh = egui::Mesh::with_texture(texture);
    let radius = CORNER_RADIUS
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5);
    if radius <= 0.5 {
        mesh.add_rect_with_uv(
            rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            tint,
        );
        return mesh;
    }

    let uv_of = |point: egui::Pos2| {
        pos2(
            (point.x - rect.left()) / rect.width(),
            (point.y - rect.top()) / rect.height(),
        )
    };
    let push = |mesh: &mut egui::Mesh, point: egui::Pos2| {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: point,
            uv: uv_of(point),
            color: tint,
        });
    };
    push(&mut mesh, rect.center());

    let corners = [
        (
            rect.left_top() + egui::vec2(radius, radius),
            std::f32::consts::PI,
        ),
        (
            rect.right_top() + egui::vec2(-radius, radius),
            std::f32::consts::FRAC_PI_2 * 3.0,
        ),
        (rect.right_bottom() + egui::vec2(-radius, -radius), 0.0),
        (
            rect.left_bottom() + egui::vec2(radius, -radius),
            std::f32::consts::FRAC_PI_2,
        ),
    ];
    for (centre, start) in corners {
        for step in 0..=ARC_STEPS {
            let angle = start + std::f32::consts::FRAC_PI_2 * step as f32 / ARC_STEPS as f32;
            push(
                &mut mesh,
                centre + egui::vec2(angle.cos(), angle.sin()) * radius,
            );
        }
    }

    let rim = mesh.vertices.len() as u32 - 1;
    for index in 1..rim {
        mesh.indices.extend([0, index, index + 1]);
    }
    mesh.indices.extend([0, rim, 1]);
    mesh
}

const GRIP_SIZE: f32 = 16.0;

fn resize_grip(rect: Rect) -> Rect {
    Rect::from_min_size(
        pos2(rect.right() - GRIP_SIZE, rect.bottom() - GRIP_SIZE),
        egui::Vec2::splat(GRIP_SIZE),
    )
}

fn paint_resize_grip(painter: &egui::Painter, rect: Rect) {
    let grip = resize_grip(rect);
    for step in 1..=3 {
        let inset = GRIP_SIZE * step as f32 / 4.0;
        painter.line_segment(
            [
                pos2(grip.right() - inset, grip.bottom() - 2.0),
                pos2(grip.right() - 2.0, grip.bottom() - inset),
            ],
            egui::Stroke::new(1.4, Color32::WHITE.gamma_multiply(0.85)),
        );
    }
}

const CORNER_BUTTON: f32 = 18.0;

fn corner_controls(rect: Rect) -> (Rect, Rect) {
    let close = Rect::from_min_size(
        pos2(rect.right() - CORNER_BUTTON - 2.0, rect.top() + 2.0),
        egui::Vec2::splat(CORNER_BUTTON),
    );
    let lock = close.translate(egui::vec2(-(CORNER_BUTTON + 2.0), 0.0));
    (close, lock)
}

fn paint_corner_controls(painter: &egui::Painter, rect: Rect, locked: bool) {
    let (close, lock) = corner_controls(rect);
    for (slot, icon) in [
        (close, crate::ui_components::Icon::Trash),
        (lock, crate::ui_components::Icon::Lock),
    ] {
        painter.rect_filled(slot, 3.0, Color32::from_black_alpha(150));
        crate::ui_components::paint_icon(
            painter,
            slot.shrink(4.0),
            icon,
            if icon == crate::ui_components::Icon::Lock && locked {
                crate::theme::COLOR_PRIMARY
            } else {
                Color32::WHITE
            },
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Grabbed {
    Move(u64),
    Resize(u64),
}

pub(super) fn handle_reference_drag(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    model_is_under_pointer: impl Fn() -> bool,
) -> bool {
    if !state.reference_board.shown() || state.reference_board.is_empty() || shot_is_framed(state) {
        return false;
    }
    let id = egui::Id::new(DRAG_ID);
    let dragging: Option<Grabbed> = ui.data(|data| data.get_temp(id));

    let (pointer, primary_down, released) = ui.input(|input| {
        (
            input.pointer.interact_pos(),
            input.pointer.primary_down(),
            input.pointer.primary_released(),
        )
    });

    if let Some(grabbed) = dragging {
        if released || !primary_down {
            ui.data_mut(|data| data.remove::<Grabbed>(id));
            return true;
        }
        match grabbed {
            Grabbed::Move(image) => {
                let delta = ui.input(|input| input.pointer.delta());
                if delta != egui::Vec2::ZERO {
                    state.dispatch(Action::DragReferenceImage {
                        id: image,
                        viewport,
                        delta,
                    });
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            Grabbed::Resize(image) => {
                if let Some(pointer) = pointer
                    && let Some(picture) = state.reference_board.get(image)
                    && viewport.height() > 0.0
                {
                    let centre = picture.rect_in(viewport).center();
                    let height = (pointer.y - centre.y).abs() * 2.0;
                    state.dispatch(Action::ResizeReferenceImage {
                        id: image,
                        height_share: height / viewport.height(),
                    });
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            }
        }
        return true;
    }

    let Some(pointer) = pointer.filter(|pointer| viewport.contains(*pointer)) else {
        return false;
    };

    let pressed = ui.input(|input| input.pointer.primary_pressed());

    if let Some(selected) = state.reference_board.selected()
        && let Some(image) = state.reference_board.get(selected)
        && image.visible
        && (state.reference_board.is_in_front(selected) || !model_is_under_pointer())
    {
        let rect = image.rect_in(viewport);
        let (close, lock) = corner_controls(rect);
        let grip = (!image.locked).then(|| resize_grip(rect));

        if close.contains(pointer) || lock.contains(pointer) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if pressed {
                state.dispatch(if close.contains(pointer) {
                    Action::RemoveReferenceImage(selected)
                } else {
                    Action::ToggleReferenceImageLocked(selected)
                });
            }
            return true;
        }
        if grip.is_some_and(|grip| grip.contains(pointer)) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            if pressed {
                ui.data_mut(|data| data.insert_temp(id, Grabbed::Resize(selected)));
            }
            return true;
        }
    }

    let Some(hit) = state.reference_board.grabbed(viewport, pointer) else {
        return false;
    };

    if !pressed {
        return false;
    }
    if model_is_under_pointer() && !state.reference_board.is_in_front(hit) {
        return false;
    }
    ui.data_mut(|data| data.insert_temp(id, Grabbed::Move(hit)));
    state.dispatch(Action::SelectReferenceRow(Some(
        crate::reference_board::ReferenceRow::Image(hit),
    )));
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
    let mut unreadable = Vec::new();
    for image in state.reference_board.images() {
        let Some(texture) = texture_for(ui, image) else {
            unreadable.push(image.name.clone());
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
    if unreadable.is_empty() {
        return;
    }
    let names = unreadable.join(", ");
    let mut fresh = false;
    crate::ui_components::log_once(
        ui.ctx(),
        egui::Id::new("vkit.reference.unreadable"),
        &names,
        || fresh = true,
    );
    if fresh {
        state.status = crate::state::StatusMessage::with_detail(
            crate::i18n::TextKey::ReferenceImageUnreadable,
            crate::state::StatusTone::Warning,
            names,
        );
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
            paint_reference_images(ui, &state, viewport, Band::Behind);
        });
    }

    #[test]
    fn a_picture_listed_above_the_head_takes_the_pointer_from_the_model() {
        let mut state = AppState::default();
        let id = state
            .reference_board
            .add(std::path::PathBuf::from("face.png"));
        assert!(
            !state.reference_board.is_in_front(id),
            "a backdrop gives way: the model is the thing being worked on",
        );

        state
            .reference_board
            .move_row(crate::reference_board::ReferenceRow::Image(id), 2);
        assert!(
            state.reference_board.is_in_front(id),
            "an overlay is what the pointer lands on",
        );
    }

    #[test]
    fn each_band_paints_its_own_half_and_nothing_twice() {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        egui::__run_test_ui(|ui| {
            let mut state = AppState::default();
            let back = state.reference_board.add(std::path::PathBuf::from("a.png"));
            let front = state.reference_board.add(std::path::PathBuf::from("b.png"));
            state
                .reference_board
                .move_row(crate::reference_board::ReferenceRow::Image(front), 3);

            let behind: Vec<u64> = state
                .reference_board
                .behind()
                .iter()
                .map(|image| image.id)
                .collect();
            let over: Vec<u64> = state
                .reference_board
                .in_front()
                .iter()
                .map(|image| image.id)
                .collect();
            assert_eq!(behind, vec![back]);
            assert_eq!(over, vec![front]);

            paint_reference_images(ui, &state, viewport, Band::Behind);
            paint_reference_images(ui, &state, viewport, Band::InFront);
        });
    }

    #[test]
    fn an_empty_board_never_takes_the_pointer_or_paints() {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        egui::__run_test_ui(|ui| {
            let mut state = AppState::default();
            assert!(!handle_reference_drag(ui, &mut state, viewport, || false));
            paint_reference_images(ui, &state, viewport, Band::Behind);
            paint_reference_images(ui, &state, viewport, Band::InFront);
        });
    }
}
