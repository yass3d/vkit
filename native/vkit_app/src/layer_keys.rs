use crate::state::{Action, AppState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerScope {
    ReferenceImages,
    HairParts,
    TextureLayers,
    AppearanceLayers,
}

#[must_use]
pub fn layer_scope(state: &AppState) -> Option<LayerScope> {
    if state.viewport_tool_panel == Some(crate::state::ViewportToolPanel::BaseView)
        && !state.reference_board.images().is_empty()
    {
        return Some(LayerScope::ReferenceImages);
    }
    match state.active_tab {
        crate::state::Tab::Hair => Some(LayerScope::HairParts),
        crate::state::Tab::Texture => Some(LayerScope::TextureLayers),
        crate::state::Tab::Morph | crate::state::Tab::Edit => {
            (!state.appearance_stack.layers.is_empty()).then_some(LayerScope::AppearanceLayers)
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerOperation {
    Hide,
    UnhideAll,
    Isolate,

    LocalView,

    Invert,

    SelectAll,

    Remove,
}

pub fn handle_layer_shortcuts(ui: &egui::Ui, state: &mut AppState) {
    let Some(scope) = layer_scope(state) else {
        return;
    };
    use crate::shortcuts::Shortcut;
    let operation = if Shortcut::LayerUnhideAll.pressed(ui) {
        LayerOperation::UnhideAll
    } else if Shortcut::LayerIsolate.pressed(ui) {
        LayerOperation::Isolate
    } else if Shortcut::LayerHide.pressed(ui) {
        LayerOperation::Hide
    } else if Shortcut::LayerLocalView.pressed(ui) {
        LayerOperation::LocalView
    } else if Shortcut::LayerInvertSelection.pressed(ui) {
        LayerOperation::Invert
    } else if Shortcut::LayerSelectAll.pressed(ui) {
        LayerOperation::SelectAll
    } else if Shortcut::LayerRemove.pressed(ui) {
        LayerOperation::Remove
    } else {
        return;
    };
    for action in actions_for(state, scope, operation) {
        state.dispatch(action);
    }
}

#[must_use]
pub fn actions_for(state: &AppState, scope: LayerScope, operation: LayerOperation) -> Vec<Action> {
    match scope {
        LayerScope::ReferenceImages => reference_actions(state, operation),
        LayerScope::HairParts => hair_actions(state, operation),
        LayerScope::TextureLayers => texture_actions(state, operation),
        LayerScope::AppearanceLayers => appearance_actions(state, operation),
    }
}

fn reference_actions(state: &AppState, operation: LayerOperation) -> Vec<Action> {
    let images: Vec<(u64, bool)> = state
        .reference_board
        .images()
        .iter()
        .map(|image| (image.id, image.visible))
        .collect();
    let selected = state.reference_board.selected();
    match operation {
        LayerOperation::Hide => selected
            .into_iter()
            .map(Action::ToggleReferenceImageVisible)
            .collect(),
        LayerOperation::UnhideAll => images
            .iter()
            .filter(|(_, visible)| !visible)
            .map(|(id, _)| Action::ToggleReferenceImageVisible(*id))
            .collect(),
        LayerOperation::Isolate | LayerOperation::LocalView => images
            .iter()
            .filter(|(id, visible)| *visible != (Some(*id) == selected))
            .map(|(id, _)| Action::ToggleReferenceImageVisible(*id))
            .collect(),
        LayerOperation::Invert => images
            .iter()
            .map(|(id, _)| Action::ToggleReferenceImageVisible(*id))
            .collect(),
        LayerOperation::SelectAll => Vec::new(),
        LayerOperation::Remove => selected
            .into_iter()
            .map(Action::RemoveReferenceImage)
            .collect(),
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
        LayerOperation::LocalView => {
            let isolated = parts
                .iter()
                .all(|(id, visible)| *visible == active.contains(id))
                && parts.iter().any(|(_, visible)| !*visible);
            let wanted = |id: &u64| isolated || active.contains(id);
            parts
                .iter()
                .filter(|(id, visible)| *visible != wanted(id))
                .map(|(id, _)| Action::ToggleHairPartVisible(*id))
                .collect()
        }
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
        LayerOperation::SelectAll => {
            if active.is_empty() {
                let mut actions = Vec::new();
                let mut additive = false;
                for (id, _) in &parts {
                    actions.push(Action::ActivateHairPart { id: *id, additive });
                    additive = true;
                }
                actions
            } else {
                vec![Action::SelectAllHairStrands]
            }
        }
        LayerOperation::Remove => parts
            .iter()
            .filter(|(id, _)| active.contains(id))
            .map(|(id, _)| Action::RemoveHairPart(*id))
            .collect(),
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
    visibility_actions(
        &layers,
        selected,
        operation,
        Action::RemoveTextureLayer,
        |id, visible| Action::SetTextureLayerVisible { id, visible },
    )
}

fn appearance_actions(state: &AppState, operation: LayerOperation) -> Vec<Action> {
    let layers: Vec<(u64, bool)> = state
        .appearance_stack
        .layers
        .iter()
        .map(|layer| (layer.id, layer.visible))
        .collect();
    let selected = state.appearance_stack.selected_id;
    visibility_actions(
        &layers,
        selected,
        operation,
        Action::RemoveAppearanceLayer,
        |id, visible| Action::SetAppearanceLayerVisible { id, visible },
    )
}

fn visibility_actions(
    layers: &[(u64, bool)],
    selected: Option<u64>,
    operation: LayerOperation,
    remove: impl Fn(u64) -> Action,
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
        LayerOperation::LocalView => selected.map_or_else(Vec::new, |kept| {
            let isolated =
                layers.iter().all(|(id, visible)| *visible == (*id == kept)) && layers.len() > 1;
            layers
                .iter()
                .filter(|(id, visible)| *visible != (isolated || *id == kept))
                .map(|(id, _)| action(*id, isolated || *id == kept))
                .collect()
        }),
        LayerOperation::Invert => layers
            .iter()
            .map(|(id, visible)| action(*id, !*visible))
            .collect(),
        LayerOperation::SelectAll => Vec::new(),
        LayerOperation::Remove => selected.into_iter().map(remove).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<(u64, bool)> {
        vec![(1, true), (2, false), (3, true)]
    }

    #[test]
    fn unhiding_touches_only_the_rows_that_were_hidden() {
        let actions = visibility_actions(
            &rows(),
            Some(1),
            LayerOperation::UnhideAll,
            Action::RemoveAppearanceLayer,
            |id, on| Action::SetAppearanceLayerVisible { id, visible: on },
        );
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
        let actions = visibility_actions(
            &rows(),
            Some(2),
            LayerOperation::Isolate,
            Action::RemoveAppearanceLayer,
            |id, on| Action::SetAppearanceLayerVisible { id, visible: on },
        );
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
        let actions = visibility_actions(
            &rows(),
            None,
            LayerOperation::Isolate,
            Action::RemoveAppearanceLayer,
            |id, on| Action::SetAppearanceLayerVisible { id, visible: on },
        );
        assert!(actions.is_empty());
    }

    #[test]
    fn hiding_an_already_hidden_row_asks_for_nothing() {
        let actions = visibility_actions(
            &rows(),
            Some(2),
            LayerOperation::Hide,
            Action::RemoveAppearanceLayer,
            |id, on| Action::SetAppearanceLayerVisible { id, visible: on },
        );
        assert!(actions.is_empty());
    }

    #[test]
    fn inverting_a_single_selection_list_flips_every_rows_visibility() {
        let actions = visibility_actions(
            &rows(),
            Some(1),
            LayerOperation::Invert,
            Action::RemoveAppearanceLayer,
            |id, on| Action::SetAppearanceLayerVisible { id, visible: on },
        );
        let changed: Vec<(u64, bool)> = actions
            .iter()
            .map(|action| match action {
                Action::SetAppearanceLayerVisible { id, visible } => (*id, *visible),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(changed, vec![(1, false), (2, true), (3, false)]);
    }

    #[test]
    fn local_view_comes_back_and_isolate_does_not() {
        let hidden_others = vec![(1, true), (2, false), (3, false)];
        let build = |rows: &[(u64, bool)], operation| {
            visibility_actions(
                rows,
                Some(1),
                operation,
                Action::RemoveAppearanceLayer,
                |id, visible| Action::SetAppearanceLayerVisible { id, visible },
            )
        };

        assert!(
            build(&hidden_others, LayerOperation::Isolate).is_empty(),
            "already isolated, so isolating again asks for nothing",
        );

        let coming_back = build(&hidden_others, LayerOperation::LocalView);
        let mut restored: Vec<(u64, bool)> = coming_back
            .iter()
            .map(|action| match action {
                Action::SetAppearanceLayerVisible { id, visible } => (*id, *visible),
                _ => unreachable!(),
            })
            .collect();
        restored.sort_unstable();
        assert_eq!(restored, vec![(2, true), (3, true)], "the rest come back");

        let all_visible = vec![(1, true), (2, true), (3, true)];
        let going_in = build(&all_visible, LayerOperation::LocalView);
        let mut hidden: Vec<(u64, bool)> = going_in
            .iter()
            .map(|action| match action {
                Action::SetAppearanceLayerVisible { id, visible } => (*id, *visible),
                _ => unreachable!(),
            })
            .collect();
        hidden.sort_unstable();
        assert_eq!(hidden, vec![(2, false), (3, false)]);
    }

    #[test]
    fn a_single_row_list_never_thinks_it_is_isolated() {
        let alone = vec![(1, true)];
        let actions = visibility_actions(
            &alone,
            Some(1),
            LayerOperation::LocalView,
            Action::RemoveAppearanceLayer,
            |id, on| Action::SetAppearanceLayerVisible { id, visible: on },
        );
        assert!(actions.is_empty(), "nothing to hide and nothing to restore");
    }

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
    #[test]
    fn an_open_reference_panel_takes_the_keys_from_the_tab_behind_it() {
        let mut state = AppState::default();
        state.active_tab = crate::state::Tab::Hair;
        state.viewport_tool_panel = Some(crate::state::ViewportToolPanel::BaseView);
        assert_eq!(
            layer_scope(&state),
            Some(LayerScope::HairParts),
            "an empty reference list is not the thing in front of anyone",
        );

        state.reference_board.add("a.png".into());
        assert_eq!(
            layer_scope(&state),
            Some(LayerScope::ReferenceImages),
            "the panel the reader opened owns the keys",
        );

        state.viewport_tool_panel = None;
        assert_eq!(
            layer_scope(&state),
            Some(LayerScope::HairParts),
            "closing the panel hands them back",
        );
    }

    #[test]
    fn the_catalogues_never_answer_these_keys() {
        let mut state = AppState::default();
        for tab in [crate::state::Tab::Alignment, crate::state::Tab::Result] {
            state.active_tab = tab;
            assert_eq!(layer_scope(&state), None, "{tab:?} answered a layer key");
        }

        state.active_tab = crate::state::Tab::Morph;
        assert_eq!(
            layer_scope(&state),
            None,
            "the morph library is a catalogue, not a stack you authored",
        );
    }

    #[test]
    fn delete_removes_the_picture_the_reference_panel_has_selected() {
        let mut state = AppState::default();
        state.viewport_tool_panel = Some(crate::state::ViewportToolPanel::BaseView);
        state.reference_board.add("a.png".into());
        let second = state.reference_board.add("b.png".into());
        let actions = actions_for(&state, LayerScope::ReferenceImages, LayerOperation::Remove);
        assert!(matches!(actions[..], [Action::RemoveReferenceImage(id)] if id == second));
    }

    #[test]
    fn hair_is_the_one_list_that_can_take_every_row_and_delete_a_set() {
        let mut state = AppState::default();
        state.active_tab = crate::state::Tab::Hair;
        let first = state.hair_project.add_part("scalp");
        let second = state.hair_project.add_part("scalp");
        state.hair_project.active_part_ids = [first].into_iter().collect();

        let inner = actions_for(&state, LayerScope::HairParts, LayerOperation::SelectAll);
        assert!(
            matches!(inner[..], [Action::SelectAllHairStrands]),
            "select-all reached past the active part instead of into it: {inner:?}",
        );

        state.hair_project.active_part_ids.clear();
        let taken = actions_for(&state, LayerScope::HairParts, LayerOperation::SelectAll);
        assert!(
            matches!(
                taken[..],
                [
                    Action::ActivateHairPart {
                        id: a,
                        additive: false,
                    },
                    Action::ActivateHairPart {
                        id: b,
                        additive: true,
                    },
                ] if a == first && b == second
            ),
            "the first row opens a fresh selection and the rest join it: {taken:?}",
        );
        state.hair_project.active_part_ids = [first].into_iter().collect();

        let removed = actions_for(&state, LayerScope::HairParts, LayerOperation::Remove);
        assert!(
            matches!(removed[..], [Action::RemoveHairPart(id)] if id == first),
            "only what is selected goes: {removed:?}",
        );
    }

    #[test]
    fn a_list_holding_one_selection_says_nothing_to_select_all() {
        assert!(
            visibility_actions(
                &rows(),
                Some(1),
                LayerOperation::SelectAll,
                Action::RemoveAppearanceLayer,
                |id, visible| Action::SetAppearanceLayerVisible { id, visible },
            )
            .is_empty(),
            "there is no set here to take every row into",
        );
        let removed = visibility_actions(
            &rows(),
            Some(2),
            LayerOperation::Remove,
            Action::RemoveAppearanceLayer,
            |id, visible| Action::SetAppearanceLayerVisible { id, visible },
        );
        assert!(matches!(removed[..], [Action::RemoveAppearanceLayer(2)]));
    }
}
