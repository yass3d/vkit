use super::*;

pub(super) const RESULT_ISLAND_ROW: f32 = 30.0;

pub(super) const RESULT_ISLAND_INSET: f32 = 10.0;

pub(super) const RESULT_ISLAND: crate::responsive::Responsive = crate::responsive::Responsive {
    min: 240.0,
    ideal: 360.0,
    fraction: 0.6,
};

pub(super) fn draw_result_view_island(ui: &mut Ui, state: &mut AppState, viewport: Rect) {
    if state.hair_thumbnail.is_some() {
        return;
    }
    let overlay = state.workspace.scan.is_some();
    let compare = state.morph_compare_available();
    if !overlay && !compare {
        return;
    }
    let rows = usize::from(overlay) + usize::from(compare);
    let Some(width) = RESULT_ISLAND.resolve(viewport.width()) else {
        return;
    };
    let height = RESULT_ISLAND_INSET * 2.0
        + RESULT_ISLAND_ROW * rows as f32
        + SPACE_2 * (rows.saturating_sub(1)) as f32;
    let island = Rect::from_center_size(
        pos2(
            viewport.center().x,
            viewport.bottom() - RESULT_ISLAND_BOTTOM_GAP - height * 0.5,
        ),
        vec2(width, height),
    );
    let layer = egui::LayerId::new(
        egui::Order::Middle,
        Id::new("vkit.viewport.result-island.layer"),
    );
    let ui = &mut ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(island));
    ui.painter().rect_filled(
        island,
        crate::theme::capsule_radius(island.height().min(44.0)),
        COLOR_TOPBAR,
    );
    let _blocker = ui.interact(
        island,
        Id::new("vkit.viewport.result-island"),
        Sense::click_and_drag(),
    );
    let content = island.shrink(RESULT_ISLAND_INSET);
    let mut row_top = content.top();
    let mut next_row = || {
        let row = Rect::from_min_size(
            pos2(content.left(), row_top),
            vec2(content.width(), RESULT_ISLAND_ROW),
        );
        row_top += RESULT_ISLAND_ROW + SPACE_2;
        row
    };
    if overlay {
        let row = next_row();
        let mut opacity = state.overlay_opacity;
        if crate::ui::island_labelled_slider(
            ui,
            row,
            text(state.locale, TextKey::ScanOverlay),
            &mut opacity,
        ) {
            state.dispatch(Action::SetOverlayOpacity(opacity));
        }
    }
    if compare {
        let row = next_row();
        let mut blend = state.morph_compare_blend;
        if crate::ui::island_labelled_slider(
            ui,
            row,
            text(state.locale, TextKey::MorphCompare),
            &mut blend,
        ) {
            state.dispatch(Action::SetMorphCompareBlend(blend));
        }
    }
}

pub(super) const RESULT_ISLAND_BOTTOM_GAP: f32 = 16.0 + CONTROL_HEIGHT;

pub(super) const PIN_PROMPT_HEIGHT: f32 = 52.0;

pub(super) const PIN_PROMPT_INSET: f32 = 22.0;

pub(super) const PIN_PROMPT_CHEVRON: f32 = 22.0;

pub(super) const PIN_PROMPT_SWING: f32 = 7.0;

pub(super) const PIN_PROMPT_SWING_PERIOD: f64 = 2.2;

pub(super) fn draw_pin_prompt_island(ui: &Ui, state: &AppState, workspace: Rect, split_x: f32) {
    if crate::guidance::next_step(state) != Some(crate::guidance::NextStep::PlacePins) {
        return;
    }
    draw_prompt_island(
        ui,
        text(state.locale, TextKey::PinPrompt),
        workspace,
        prompt_island_centre(workspace, split_x),
    );
}

pub(crate) fn draw_texture_prompt_island(ui: &Ui, state: &AppState, workspace: Rect, split_x: f32) {
    let Some(message) = texture_prompt(state) else {
        return;
    };
    draw_prompt_island(
        ui,
        text(state.locale, message),
        workspace,
        prompt_island_centre(workspace, split_x),
    );
}

pub(super) fn texture_prompt(state: &AppState) -> Option<TextKey> {
    if !state.is_texturing() || state.busy() {
        return None;
    }
    let tool = state.texture_project.active_tool;
    if !matches!(
        tool,
        TextureTool::Projection | TextureTool::PinPair | TextureTool::MaskBrush
    ) {
        return None;
    }
    let layer = state.texture_project.selected_layer()?;
    if layer.edited_image.is_none() && layer.image.is_none() {
        return Some(TextKey::TextureNeedsImage);
    }
    match tool {
        TextureTool::PinPair if layer.pins.is_empty() => Some(TextKey::TexturePinPairPrompt),
        TextureTool::MaskBrush
            if layer.source_mode == crate::texture_project::TextureSourceMode::LandmarkPins
                && !layer.landmark_warp_ready() =>
        {
            Some(TextKey::TexturePinPairPrompt)
        }
        _ => None,
    }
}

pub(crate) fn prompt_island_centre(workspace: Rect, split_x: f32) -> Pos2 {
    pos2(split_x, workspace.center().y)
}

pub(crate) fn draw_prompt_island(ui: &Ui, message: &str, workspace: Rect, centre: Pos2) {
    let galley = ui.painter().layout_no_wrap(
        message.to_owned(),
        FontId::proportional(FONT_BODY),
        COLOR_TEXT,
    );
    let width = galley.size().x + (PIN_PROMPT_CHEVRON + PIN_PROMPT_INSET) * 2.0;
    if width > workspace.width() - 32.0 {
        return;
    }
    let island = Rect::from_center_size(centre, vec2(width, PIN_PROMPT_HEIGHT));

    ui.painter().rect_filled(
        island,
        PIN_PROMPT_HEIGHT * 0.5,
        COLOR_TOPBAR.gamma_multiply(0.82),
    );
    let text_pos = pos2(
        island.center().x - galley.size().x * 0.5,
        island.center().y - galley.size().y * 0.5,
    );
    ui.painter().galley(text_pos, galley, COLOR_TEXT);

    let time = ui.input(|input| input.time);
    let phase = (time / PIN_PROMPT_SWING_PERIOD).rem_euclid(1.0) as f32;
    let swing = (phase * std::f32::consts::TAU).sin() * PIN_PROMPT_SWING;
    for (icon, side) in [(Icon::ChevronLeft, -1.0_f32), (Icon::ChevronRight, 1.0)] {
        let centre = pos2(
            island.center().x
                + side * (island.width() * 0.5 - PIN_PROMPT_INSET * 0.5 - PIN_PROMPT_CHEVRON * 0.5)
                + side * swing,
            island.center().y,
        );
        paint_icon(
            ui.painter(),
            Rect::from_center_size(centre, Vec2::splat(PIN_PROMPT_CHEVRON)),
            icon,
            COLOR_TEXT,
        );
    }
    ui.ctx().request_repaint();
}

pub(super) const PIN_ISLAND_HEIGHT: f32 = 40.0;

pub(super) const PIN_ISLAND_BOTTOM_GAP: f32 = 56.0;

pub(super) const PIN_ISLAND_INSET: f32 = 8.0;

pub(super) fn draw_pin_island(ui: &mut Ui, state: &mut AppState, workspace: Rect, split_x: f32) {
    if state.scan_path.is_none() || state.busy() {
        return;
    }
    let undo_label = text(state.locale, TextKey::Undo);
    let reset_label = text(state.locale, TextKey::ResetAllPins);
    let measure = |label: &str| {
        ui.painter()
            .layout_no_wrap(
                label.to_owned(),
                FontId::proportional(crate::theme::FONT_SM),
                COLOR_TEXT,
            )
            .size()
            .x
    };
    let undo_width = measure(undo_label) + 28.0;
    let reset_width = measure(reset_label) + 28.0;
    let width = undo_width + reset_width + PIN_ISLAND_INSET * 3.0;
    if width > workspace.width() - 32.0 {
        return;
    }
    let island = Rect::from_center_size(
        pos2(
            split_x,
            workspace.bottom() - PIN_ISLAND_BOTTOM_GAP - PIN_ISLAND_HEIGHT * 0.5,
        ),
        vec2(width, PIN_ISLAND_HEIGHT),
    );

    let layer = egui::LayerId::new(
        egui::Order::Middle,
        Id::new("vkit.viewport.pin-island.layer"),
    );
    let ui = &mut ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(island));
    ui.painter().rect_filled(
        island,
        f32::from(crate::theme::RADIUS_POPOVER),
        COLOR_TOPBAR,
    );

    let _blocker = ui.interact(
        island,
        Id::new("vkit.viewport.pin-island"),
        Sense::click_and_drag(),
    );
    let content = island.shrink2(vec2(PIN_ISLAND_INSET, 6.0));
    let undo_rect = Rect::from_min_size(content.min, vec2(undo_width, content.height()));
    let reset_rect = Rect::from_min_size(
        pos2(undo_rect.right() + PIN_ISLAND_INSET, content.top()),
        vec2(reset_width, content.height()),
    );
    let undo = crate::ui_components::tooltip(
        crate::ui::island_capsule_button(ui, undo_rect, undo_label, false),
        undo_label,
        Some(crate::shortcuts::Shortcut::Undo.label_now(ui)),
    );
    if undo.clicked() {
        state.dispatch(Action::Undo);
    }

    if crate::ui::island_capsule_button(ui, reset_rect, reset_label, true).clicked() {
        state.dispatch(Action::ResetPins);
    }
}

pub(super) fn paint_pin_count(ui: &Ui, state: &AppState, pane: Rect) {
    if state.scan_path.is_none() {
        return;
    }
    let label = format!(
        "{} {}",
        text(state.locale, TextKey::PinPairsPlaced),
        state.complete_pairs
    );
    ui.painter().text(
        pos2(pane.center().x, pane.top() + 16.0),
        Align2::CENTER_TOP,
        label,
        FontId::proportional(FONT_XS),
        COLOR_MUTED,
    );
}

fn draw_reset_island(
    ui: &Ui,
    state: &mut AppState,
    rect: Rect,
    id: &'static str,
    label: TextKey,
    action: Action,
) {
    const WIDTH: f32 = 160.0;
    const BOTTOM_MARGIN: f32 = 44.0;
    let enabled = !state.busy();
    let height = crate::theme::CONTROL_H_PRIMARY;
    let island = Rect::from_center_size(
        egui::pos2(rect.center().x, rect.max.y - BOTTOM_MARGIN - height * 0.5),
        egui::vec2(WIDTH.min(rect.width() - 32.0), height),
    );
    if island.width() < 96.0 || island.min.y < rect.min.y {
        return;
    }
    let mut clicked = false;
    egui::Area::new(Id::new(id))
        .order(egui::Order::Foreground)
        .fixed_pos(island.min)
        .show(ui.ctx(), |ui| {
            let (button_rect, response) = ui.allocate_exact_size(
                island.size(),
                if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                },
            );
            let fill = if !enabled {
                crate::theme::COLOR_SURFACE
            } else if response.hovered() || response.is_pointer_button_down_on() {
                COLOR_DESTRUCTIVE
            } else {
                crate::theme::COLOR_SURFACE_RAISED
            };
            ui.painter().rect_filled(
                button_rect,
                crate::theme::capsule_radius(button_rect.height()),
                fill,
            );
            ui.painter().text(
                button_rect.center(),
                egui::Align2::CENTER_CENTER,
                crate::i18n::text(state.locale, label),
                egui::FontId::proportional(crate::theme::FONT_BODY),
                if enabled {
                    crate::theme::COLOR_TEXT
                } else {
                    crate::theme::disabled(crate::theme::COLOR_TEXT)
                },
            );
            clicked = response.clicked();
        });
    if clicked {
        state.dispatch(action);
    }
}

pub(super) fn draw_hair_reset_island(ui: &Ui, state: &mut AppState, rect: Rect) {
    let has_strands = state
        .hair_project
        .editable_parts()
        .into_iter()
        .filter_map(|id| state.hair_project.part(id))
        .any(|part| !part.strands.is_empty());
    if !has_strands {
        return;
    }
    draw_reset_island(
        ui,
        state,
        rect,
        "vkit.viewport.hair-reset",
        TextKey::ResetSculpt,
        Action::ResetHairShapes,
    );
}
