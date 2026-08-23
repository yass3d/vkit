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
