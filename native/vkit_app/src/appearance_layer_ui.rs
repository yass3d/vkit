use egui::containers::scroll_area::ScrollBarVisibility;
use egui::{
    Align, Align2, FontId, Frame, Id, Layout, Margin, Rect, Response, ScrollArea, Sense, Ui, Vec2,
    pos2, vec2,
};

use crate::i18n::{TextKey, text};
use crate::state::{Action, AppState};
use crate::theme::{
    CAPSULE_RADIUS, COLOR_FIELD, COLOR_MUTED, COLOR_SURFACE_HOVER, COLOR_TEXT, CONTROL_RADIUS,
    FONT_SM, FONT_XS, SPACE_1, SPACE_2, SPACE_3,
};
use crate::ui_components::{Icon, paint_icon, paint_list_row_highlight};

const LAYER_ROW_HEIGHT: f32 = 28.0;
const LAYER_BUTTON: f32 = 22.0;
const ADD_ROW_HEIGHT: f32 = 26.0;

pub(crate) fn draw_appearance_layers(ui: &mut Ui, state: &mut AppState, height: f32) {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), ADD_ROW_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            let rect = ui.available_rect_before_wrap();
            ui.painter().text(
                pos2(rect.left() + SPACE_1, rect.center().y),
                Align2::LEFT_CENTER,
                text(state.locale, TextKey::AppearanceLayers),
                FontId::proportional(FONT_XS),
                COLOR_MUTED,
            );
            ui.allocate_space(vec2(ui.available_width(), 0.0));
        },
    );

    let (add_rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width().max(0.0), ADD_ROW_HEIGHT),
        Sense::hover(),
    );
    let armed = state.selected_vam_edit_source_id.is_some();
    if add_button(
        ui,
        add_rect,
        text(state.locale, TextKey::AddAppearanceLayer),
        armed,
    ) && armed
    {
        state.dispatch(Action::AddAppearanceLayer);
    }

    let list_height = (height - ADD_ROW_HEIGHT * 2.0 - SPACE_2).max(LAYER_ROW_HEIGHT);
    let mut command = None;
    Frame::new()
        .fill(COLOR_FIELD)
        .corner_radius(CAPSULE_RADIUS)
        .inner_margin(Margin::same(4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width().max(0.0));
            ScrollArea::vertical()
                .id_salt("vkit.appearance.layers")
                .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                .max_height(list_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().max(0.0));
                    if state.appearance_stack.layers.is_empty() {
                        let (rect, _) = ui.allocate_exact_size(
                            vec2(ui.available_width().max(0.0), LAYER_ROW_HEIGHT),
                            Sense::hover(),
                        );
                        ui.painter().text(
                            pos2(rect.left() + SPACE_2, rect.center().y),
                            Align2::LEFT_CENTER,
                            text(state.locale, TextKey::AppearanceLayersEmpty),
                            FontId::proportional(FONT_XS),
                            COLOR_MUTED,
                        );
                        return;
                    }
                    let rows = state
                        .appearance_stack
                        .layers
                        .iter()
                        .rev()
                        .map(|layer| (layer.id, layer.name.clone(), layer.visible))
                        .collect::<Vec<_>>();
                    let top = rows.first().map(|row| row.0);
                    let bottom = rows.last().map(|row| row.0);
                    for (id, name, visible) in rows {
                        if let Some(action) = layer_row(
                            ui,
                            state,
                            id,
                            &name,
                            visible,
                            Some(id) != top,
                            Some(id) != bottom,
                        ) {
                            command = Some(action);
                        }
                    }
                });
        });
    if let Some(action) = command {
        state.dispatch(action);
    }
}

fn layer_row(
    ui: &mut Ui,
    state: &AppState,
    id: u64,
    name: &str,
    visible: bool,
    can_raise: bool,
    can_lower: bool,
) -> Option<Action> {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width().max(0.0), LAYER_ROW_HEIGHT),
        Sense::click(),
    );
    let selected = state.appearance_stack.selected_id == Some(id);
    paint_list_row_highlight(ui, rect, selected, response.hovered());

    let mut command = if response.clicked() {
        Some(Action::SelectAppearanceLayer(id))
    } else {
        None
    };

    let eye = Rect::from_center_size(
        pos2(rect.left() + SPACE_1 + LAYER_BUTTON * 0.5, rect.center().y),
        Vec2::splat(LAYER_BUTTON),
    );
    if hit(
        ui,
        eye,
        if visible {
            Icon::EyeOpen
        } else {
            Icon::EyeClosed
        },
        "vkit.appearance.layer.eye",
        id,
        visible,
    )
    .on_hover_text(text(
        state.locale,
        if visible {
            TextKey::TooltipHide
        } else {
            TextKey::TooltipShow
        },
    ))
    .clicked()
    {
        command = Some(Action::SetAppearanceLayerVisible {
            id,
            visible: !visible,
        });
    }

    let mut cursor = rect.right() - SPACE_1 - LAYER_BUTTON * 0.5;
    for (slot_index, (icon, tooltip, action, enabled)) in [
        (
            Icon::Trash,
            TextKey::DeleteLayer,
            Action::RemoveAppearanceLayer(id),
            true,
        ),
        (
            Icon::ChevronDown,
            TextKey::AppearanceLayerLower,
            Action::LowerAppearanceLayer(id),
            can_lower,
        ),
        (
            Icon::ChevronUp,
            TextKey::AppearanceLayerRaise,
            Action::RaiseAppearanceLayer(id),
            can_raise,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let slot = Rect::from_center_size(pos2(cursor, rect.center().y), Vec2::splat(LAYER_BUTTON));
        cursor -= LAYER_BUTTON;
        if !enabled {
            paint_icon(ui.painter(), slot.shrink(5.0), icon, COLOR_SURFACE_HOVER);
            continue;
        }
        if hit(
            ui,
            slot,
            icon,
            "vkit.appearance.layer.button",
            id * 8 + slot_index as u64,
            false,
        )
        .on_hover_text(text(state.locale, tooltip))
        .clicked()
        {
            command = Some(action);
        }
    }

    ui.painter()
        .with_clip_rect(Rect::from_min_max(
            pos2(eye.right() + SPACE_1, rect.top()),
            pos2((cursor - SPACE_1).max(eye.right() + SPACE_1), rect.bottom()),
        ))
        .text(
            pos2(eye.right() + SPACE_2, rect.center().y),
            Align2::LEFT_CENTER,
            name,
            FontId::proportional(FONT_SM),
            if visible { COLOR_TEXT } else { COLOR_MUTED },
        );
    command
}

fn hit(ui: &Ui, rect: Rect, icon: Icon, salt: &'static str, key: u64, active: bool) -> Response {
    let response = ui.interact(rect, Id::new(salt).with(key), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CONTROL_RADIUS, COLOR_SURFACE_HOVER);
    }
    paint_icon(
        ui.painter(),
        rect.shrink(5.0),
        icon,
        if response.hovered() || active {
            COLOR_TEXT
        } else {
            COLOR_MUTED
        },
    );
    response
}

fn add_button(ui: &mut Ui, rect: Rect, caption: &str, armed: bool) -> bool {
    let response = ui.interact(rect, Id::new("vkit.appearance.layer.add"), Sense::click());
    let hovered = response.hovered() && armed;
    if hovered {
        ui.painter()
            .rect_filled(rect, CONTROL_RADIUS, COLOR_SURFACE_HOVER);
    }
    let color = if !armed {
        COLOR_SURFACE_HOVER
    } else if hovered {
        COLOR_TEXT
    } else {
        COLOR_MUTED
    };
    ui.painter().text(
        pos2(rect.center().x + SPACE_3 * 0.5, rect.center().y),
        Align2::CENTER_CENTER,
        caption,
        FontId::proportional(FONT_XS),
        color,
    );
    response.clicked()
}
