//! The texture tools, out of the header and into a toolbox.
//!
//! Six of them, in a header row that also carries the brush controls for
//! whichever one is chosen — so picking a tool and adjusting it competed for
//! the same strip. The sculpt and hair tabs both put their tools in a floating
//! box; this is the third caller of that same box, and the only thing that
//! differs is which icons go in it.
//!
//! Unlike the other two, the set is not fixed: a layer's source mode decides
//! which tools it has, so the box grows and shrinks with the selection.

use egui::{Id, Rect, Ui};

use crate::i18n::{TextKey, text};
use crate::state::{Action, AppState};
use crate::texture_project::{TextureSourceMode, TextureTool};

fn columns(state: &AppState) -> usize {
    state.texture_toolbox_columns.clamp(1, 2) as usize
}

/// The tools the selected layer offers, in the order they are drawn.
#[must_use]
pub(super) fn tools_for(state: &AppState) -> &'static [TextureTool] {
    state
        .texture_project
        .selected_layer()
        .map_or(TextureSourceMode::default().available_tools(), |layer| {
            layer.source_mode.available_tools()
        })
}

/// Where the toolbox stands, or `None` when this tab is not painting.
#[must_use]
pub(super) fn texture_toolbox_rect(state: &AppState, viewport: Rect) -> Option<Rect> {
    if !state.is_texturing() || state.hair_thumbnail.is_some() {
        return None;
    }
    let slots = tools_for(state).len();
    if slots == 0 {
        return None;
    }
    super::toolbox::toolbox_rect(
        viewport,
        slots,
        columns(state),
        state.texture_toolbox_pos,
        None,
    )
}

pub(super) fn draw_texture_toolbox(ui: &mut Ui, state: &mut AppState, viewport: Rect) {
    let Some(rect) = texture_toolbox_rect(state, viewport) else {
        return;
    };
    let tools = tools_for(state);
    let mut position = state.texture_toolbox_pos;
    let mut across = state.texture_toolbox_columns;
    let cells = super::toolbox::draw_toolbox(
        ui,
        Id::new("vkit.viewport.texture.toolbox"),
        rect,
        tools.len(),
        &mut position,
        &mut across,
    );
    state.texture_toolbox_pos = position;
    state.texture_toolbox_columns = across;

    let active = state.texture_project.active_tool;
    let mut chosen = None;
    for (index, mut cell_ui) in cells.into_iter().enumerate() {
        let Some(tool) = tools.get(index).copied() else {
            continue;
        };
        // A tool the layer cannot run yet is shown and refused, not hidden:
        // hiding it would reflow the box every time the pin count crossed a
        // threshold, under whatever the reader was about to click.
        let usable = state.texture_project.tool_usable(tool);
        let shortcut = usable
            .then(|| {
                crate::texture_ui::texture_tool_shortcut(tool).map(|key| key.label_now(&cell_ui))
            })
            .flatten();
        let response = super::detail_hud::detail_hud_toggle_icon(
            &mut cell_ui,
            crate::texture_ui::tool_icon(tool),
            active == tool,
            text(
                state.locale,
                if usable {
                    crate::texture_ui::texture_tool_text_key(tool)
                } else {
                    TextKey::TextureToolNeedsPins
                },
            ),
            "",
            shortcut.as_deref(),
        );
        if usable && response.clicked() {
            chosen = Some(tool);
        }
    }
    if let Some(tool) = chosen {
        state.dispatch(Action::SetTextureTool(tool));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn viewport() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0))
    }

    fn texturing() -> AppState {
        let mut state = AppState::default();
        state.active_tab = crate::state::Tab::Texture;
        state
    }

    /// The box exists on the texture tab and nowhere else.
    #[test]
    fn only_the_texture_tab_has_a_texture_toolbox() {
        assert!(texture_toolbox_rect(&texturing(), viewport()).is_some());

        let mut elsewhere = AppState::default();
        elsewhere.active_tab = crate::state::Tab::Hair;
        assert!(texture_toolbox_rect(&elsewhere, viewport()).is_none());
    }

    /// The set is the layer's, not a fixed list.
    #[test]
    fn the_slots_follow_what_the_layer_can_do() {
        let state = texturing();
        assert_eq!(
            tools_for(&state),
            TextureSourceMode::default().available_tools(),
            "with no layer selected the default mode decides",
        );
        assert!(!tools_for(&state).is_empty());
    }

    /// A shot in progress takes every overlay off the screen, this one too.
    #[test]
    fn a_framed_thumbnail_hides_the_toolbox() {
        let mut state = texturing();
        state.hair_thumbnail = Some(crate::state::HairThumbnailJob {
            target: crate::state::HairThumbnailTarget::Preset,
            square: None,
            shoot: false,
        });
        assert!(texture_toolbox_rect(&state, viewport()).is_none());
    }
}
