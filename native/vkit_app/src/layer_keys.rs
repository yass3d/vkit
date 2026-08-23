//! Hide, unhide, isolate and invert — on whichever list is in front.
//!
//! Four operations that every layer list in every program has, and that this
//! one had none of. They read the same on a hair part, a texture layer and an
//! appearance layer, so they are one set of shortcuts and not three: the tab
//! decides which list they land on, and nothing else has to know.

use crate::state::{Action, AppState};

/// The list the reader is working in right now.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerScope {
    HairParts,
    TextureLayers,
    AppearanceLayers,
}

/// Which list a layer key would act on, or `None` if none is in front.
///
/// Decided by the tab, not by what was clicked last. A key that acts on
/// whichever list happened to be touched most recently is a key nobody can
/// predict, and the reader cannot see which list is "current" to check.
#[must_use]
pub fn layer_scope(state: &AppState) -> Option<LayerScope> {
    match state.active_tab {
        crate::state::Tab::Hair => Some(LayerScope::HairParts),
        crate::state::Tab::Texture => Some(LayerScope::TextureLayers),
        crate::state::Tab::Morph | crate::state::Tab::Edit => {
            (!state.appearance_stack.layers.is_empty()).then_some(LayerScope::AppearanceLayers)
        }
        _ => None,
    }
}

/// What the four keys do, named once so the dispatch reads as a table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerOperation {
    Hide,
    UnhideAll,
    Isolate,
    Invert,
}

/// Read the four layer keys and dispatch what they mean.
///
/// Nothing happens when no list is in front — a bare `H` on the alignment tab
/// is not a hidden command, it is a key that does nothing, which is what a
/// reader expects from a list shortcut with no list.
pub fn handle_layer_shortcuts(ui: &egui::Ui, state: &mut AppState) {
    let Some(scope) = layer_scope(state) else {
        return;
    };
    use crate::shortcuts::Shortcut;
    let operation = if Shortcut::LayerUnhideAll.pressed(ui) {
        LayerOperation::UnhideAll
    } else if Shortcut::LayerHide.pressed(ui) {
        LayerOperation::Hide
    } else if Shortcut::LayerIsolate.pressed(ui) {
        LayerOperation::Isolate
    } else if Shortcut::LayerInvertSelection.pressed(ui) {
        LayerOperation::Invert
    } else {
        return;
    };
    for action in actions_for(state, scope, operation) {
        state.dispatch(action);
    }
}

/// The actions one key press turns into, for this list.
///
/// Built as a list rather than dispatched inline so a test can ask what a key
/// would do without a `Ui`, a tab, or a frame.
#[must_use]
pub fn actions_for(state: &AppState, scope: LayerScope, operation: LayerOperation) -> Vec<Action> {
    match scope {
        LayerScope::HairParts => hair_actions(state, operation),
        LayerScope::TextureLayers => texture_actions(state, operation),
        LayerScope::AppearanceLayers => appearance_actions(state, operation),
    }
}

fn hair_actions(state: &AppState, operation: LayerOperation) -> Vec<Action> {
    let parts: Vec<(u64, bool)> = state
        .hair_project
        .parts
        .iter()
        .map(|part| (part.id, part.visible))
        .collect();
    let active = &state.hair_project.active_part_ids;
    match operation {
        LayerOperation::Hide => parts
            .iter()
            .filter(|(id, visible)| *visible && active.contains(id))
            .map(|(id, _)| Action::ToggleHairPartVisible(*id))
            .collect(),
        LayerOperation::UnhideAll => parts
            .iter()
            .filter(|(_, visible)| !*visible)
            .map(|(id, _)| Action::ToggleHairPartVisible(*id))
            .collect(),
        LayerOperation::Isolate => parts
            .iter()
            .filter(|(id, visible)| *visible != active.contains(id))
            .map(|(id, _)| Action::ToggleHairPartVisible(*id))
            .collect(),
        // Hair parts are the one list that holds more than one selection, so
        // here "invert" is the selection, exactly as it reads.
        LayerOperation::Invert => {
            let mut actions = Vec::new();
            let mut additive = false;
            for (id, _) in &parts {
                if !active.contains(id) {
                    actions.push(Action::ActivateHairPart { id: *id, additive });
                    additive = true;
                }
            }
            actions
        }
    }
}

fn texture_actions(state: &AppState, operation: LayerOperation) -> Vec<Action> {
    let layers: Vec<(u64, bool)> = state
        .texture_project
        .layers
        .iter()
        .map(|layer| (layer.id, layer.visible))
        .collect();
    let selected = state.texture_project.selected_layer_id;
    visibility_actions(&layers, selected, operation, |id, visible| {
        Action::SetTextureLayerVisible { id, visible }
    })
}

fn appearance_actions(state: &AppState, operation: LayerOperation) -> Vec<Action> {
    let layers: Vec<(u64, bool)> = state
        .appearance_stack
        .layers
        .iter()
        .map(|layer| (layer.id, layer.visible))
        .collect();
    let selected = state.appearance_stack.selected_id;
    visibility_actions(&layers, selected, operation, |id, visible| {
        Action::SetAppearanceLayerVisible { id, visible }
    })
}

/// The three visibility operations, for a list that selects one row at a time.
///
/// `Invert` flips every row's visibility here rather than the selection: a list
/// with one selected row has no selection to invert, and "show me what I was
/// not looking at" is what the key means on such a list.
fn visibility_actions(
    layers: &[(u64, bool)],
    selected: Option<u64>,
    operation: LayerOperation,
    action: impl Fn(u64, bool) -> Action,
) -> Vec<Action> {
    match operation {
        LayerOperation::Hide => selected
            .filter(|id| layers.iter().any(|(row, visible)| row == id && *visible))
            .map(|id| vec![action(id, false)])
            .unwrap_or_default(),
        LayerOperation::UnhideAll => layers
            .iter()
            .filter(|(_, visible)| !*visible)
            .map(|(id, _)| action(*id, true))
            .collect(),
        LayerOperation::Isolate => selected.map_or_else(Vec::new, |kept| {
            layers
                .iter()
                .filter(|(id, visible)| *visible != (*id == kept))
                .map(|(id, _)| action(*id, *id == kept))
                .collect()
        }),
        LayerOperation::Invert => layers
            .iter()
            .map(|(id, visible)| action(*id, !*visible))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<(u64, bool)> {
        vec![(1, true), (2, false), (3, true)]
    }

    /// Only what has to change is asked to change.
    ///
    /// An operation that dispatched a row's current value back at it would
    /// branch the history and mark the session dirty for a key that did
    /// nothing the reader can see.
    #[test]
    fn unhiding_touches_only_the_rows_that_were_hidden() {
        let actions = visibility_actions(&rows(), Some(1), LayerOperation::UnhideAll, |id, on| {
            Action::SetAppearanceLayerVisible { id, visible: on }
        });
        assert_eq!(actions.len(), 1, "only row 2 was hidden");
        assert!(matches!(
            actions[0],
            Action::SetAppearanceLayerVisible {
                id: 2,
                visible: true
            }
        ));
    }

    #[test]
    fn isolating_hides_the_others_and_shows_the_one_kept() {
        let actions = visibility_actions(&rows(), Some(2), LayerOperation::Isolate, |id, on| {
            Action::SetAppearanceLayerVisible { id, visible: on }
        });
        let mut changed: Vec<(u64, bool)> = actions
            .iter()
            .map(|action| match action {
                Action::SetAppearanceLayerVisible { id, visible } => (*id, *visible),
                _ => unreachable!(),
            })
            .collect();
        changed.sort_unstable();
        assert_eq!(changed, vec![(1, false), (2, true), (3, false)]);
    }

    #[test]
    fn isolating_with_nothing_selected_changes_nothing() {
        let actions = visibility_actions(&rows(), None, LayerOperation::Isolate, |id, on| {
            Action::SetAppearanceLayerVisible { id, visible: on }
        });
        assert!(actions.is_empty());
    }

    #[test]
    fn hiding_an_already_hidden_row_asks_for_nothing() {
        let actions = visibility_actions(&rows(), Some(2), LayerOperation::Hide, |id, on| {
            Action::SetAppearanceLayerVisible { id, visible: on }
        });
        assert!(actions.is_empty());
    }

    /// On a list that selects one row, "invert" is the visibility.
    #[test]
    fn inverting_a_single_selection_list_flips_every_rows_visibility() {
        let actions = visibility_actions(&rows(), Some(1), LayerOperation::Invert, |id, on| {
            Action::SetAppearanceLayerVisible { id, visible: on }
        });
        let changed: Vec<(u64, bool)> = actions
            .iter()
            .map(|action| match action {
                Action::SetAppearanceLayerVisible { id, visible } => (*id, *visible),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(changed, vec![(1, false), (2, true), (3, false)]);
    }

    /// A tab with no list of its own answers with no scope, so the keys do
    /// nothing rather than acting on whatever was open last.
    #[test]
    fn a_tab_without_a_list_has_no_scope() {
        let mut state = AppState::default();
        state.active_tab = crate::state::Tab::Alignment;
        assert_eq!(layer_scope(&state), None);

        state.active_tab = crate::state::Tab::Hair;
        assert_eq!(layer_scope(&state), Some(LayerScope::HairParts));

        state.active_tab = crate::state::Tab::Texture;
        assert_eq!(layer_scope(&state), Some(LayerScope::TextureLayers));

        state.active_tab = crate::state::Tab::Morph;
        assert_eq!(
            layer_scope(&state),
            None,
            "an empty appearance stack is not a list to act on"
        );
    }
}
