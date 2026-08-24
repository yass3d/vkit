use egui::{Id, Rect, Ui};

use crate::i18n::text;
use crate::sculpt::SculptBrush;
use crate::state::{Action, AppState};

const STACK_GAP: f32 = crate::theme::SPACE_2;

fn columns(state: &AppState) -> usize {
    state.sculpt_toolbox_columns.clamp(1, 2) as usize
}

#[must_use]
pub(super) fn sculpt_toolbox_rect(state: &AppState, viewport: Rect) -> Option<Rect> {
    if !state.is_sculpting() || state.hair_thumbnail.is_some() {
        return None;
    }
    super::toolbox::toolbox_rect(
        viewport,
        SculptBrush::ALL.len(),
        columns(state),
        state.sculpt_toolbox_pos,
        Some(default_stack_top(state, viewport)),
    )
}

fn default_stack_top(state: &AppState, viewport: Rect) -> f32 {
    let toolbox = super::toolbox::toolbox_size(SculptBrush::ALL.len(), columns(state)).y;
    let group = super::detail_hud::detail_group_panel_height(state);
    viewport.center().y - (toolbox + STACK_GAP + group) * 0.5
}

#[must_use]
pub(super) fn group_panel_default_top(state: &AppState, viewport: Rect, height: f32) -> f32 {
    if !state.is_sculpting() {
        return viewport.center().y - height * 0.5;
    }
    let toolbox = super::toolbox::toolbox_size(SculptBrush::ALL.len(), columns(state)).y;
    default_stack_top(state, viewport) + toolbox + STACK_GAP
}

pub(super) fn draw_sculpt_toolbox(ui: &mut Ui, state: &mut AppState, viewport: Rect) {
    let Some(rect) = sculpt_toolbox_rect(state, viewport) else {
        return;
    };
    let mut position = state.sculpt_toolbox_pos;
    let mut across = state.sculpt_toolbox_columns;
    let cells = super::toolbox::draw_toolbox(
        ui,
        Id::new("vkit.viewport.sculpt.toolbox"),
        rect,
        SculptBrush::ALL.len(),
        &mut position,
        &mut across,
    );
    state.sculpt_toolbox_pos = position;
    state.sculpt_toolbox_columns = across;

    let shown = super::displayed_sculpt_brush(ui, state);
    let mut chosen = None;
    for (index, mut cell_ui) in cells.into_iter().enumerate() {
        let Some(brush) = SculptBrush::ALL.get(index).copied() else {
            continue;
        };
        let hint = super::sculpt_input::sculpt_brush_hint(&cell_ui, brush);
        if super::detail_hud::detail_hud_toggle_icon(
            &mut cell_ui,
            super::sculpt_brush_icon(brush),
            shown == brush,
            text(state.locale, super::sculpt_brush_text_key(brush)),
            text(state.locale, super::sculpt_brush_tooltip_key(brush)),
            Some(hint.as_str()),
        )
        .clicked()
        {
            chosen = Some(brush);
        }
    }
    if let Some(brush) = chosen {
        state.dispatch(Action::SetSculptBrush(brush));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn viewport() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0))
    }

    #[test]
    fn the_toolbox_sits_above_the_group_island_and_they_do_not_overlap() {
        let mut state = AppState::default();
        state.active_tab = crate::state::Tab::Morph;
        state.result_preview_phase = crate::state::ResultPreviewPhase::Sculpt;
        assert!(state.is_sculpting(), "the fixture must be sculpting");

        let toolbox = sculpt_toolbox_rect(&state, viewport()).expect("room for a toolbox");
        let group_height = super::super::detail_hud::detail_group_panel_height(&state);
        let group_top = group_panel_default_top(&state, viewport(), group_height);
        assert!(
            group_top >= toolbox.bottom(),
            "the group island starts at {group_top} and the toolbox ends at {}",
            toolbox.bottom()
        );
    }

    #[test]
    fn the_pair_is_centred_on_the_viewport() {
        let mut state = AppState::default();
        state.active_tab = crate::state::Tab::Morph;
        state.result_preview_phase = crate::state::ResultPreviewPhase::Sculpt;

        let toolbox = sculpt_toolbox_rect(&state, viewport()).expect("room for a toolbox");
        let group_height = super::super::detail_hud::detail_group_panel_height(&state);
        let group_bottom = group_panel_default_top(&state, viewport(), group_height) + group_height;
        let middle = (toolbox.top() + group_bottom) * 0.5;
        assert!(
            (middle - viewport().center().y).abs() < 1.0,
            "the pair centres at {middle}, not {}",
            viewport().center().y
        );
    }

    #[test]
    fn no_toolbox_outside_the_sculpt_tab() {
        for tab in [
            crate::state::Tab::Texture,
            crate::state::Tab::Hair,
            crate::state::Tab::Alignment,
            crate::state::Tab::Result,
        ] {
            let mut state = AppState::default();
            state.active_tab = tab;
            assert!(
                sculpt_toolbox_rect(&state, viewport()).is_none(),
                "{tab:?} drew the sculpt toolbox"
            );
        }
    }

    #[test]
    fn no_tab_shows_two_toolboxes() {
        for tab in [
            crate::state::Tab::Alignment,
            crate::state::Tab::Edit,
            crate::state::Tab::Morph,
            crate::state::Tab::Texture,
            crate::state::Tab::Hair,
            crate::state::Tab::Result,
        ] {
            let mut state = AppState::default();
            state.active_tab = tab;
            let showing = [
                sculpt_toolbox_rect(&state, viewport()).is_some(),
                super::super::texture_toolbox::texture_toolbox_rect(&state, viewport()).is_some(),
                super::super::hair_hud::hair_toolbox_rect(&state, viewport()).is_some(),
            ]
            .into_iter()
            .filter(|shown| *shown)
            .count();
            assert!(showing <= 1, "{tab:?} shows {showing} toolboxes");
        }
    }
}
