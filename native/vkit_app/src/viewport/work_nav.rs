use egui::{Id, Rect, Sense, Ui, pos2, vec2};

use crate::i18n::{TextKey, text};
use crate::state::{Action, AppState};
use crate::ui_components::{Icon, paint_icon};

const BOTTOM_MARGIN: f32 = 44.0;

const BUTTON: f32 = 30.0;

const BUTTON_GAP: f32 = crate::theme::SPACE_1;

const INSET: f32 = 5.0;

pub(super) struct NavButton {
    pub icon: Icon,
    pub tooltip: TextKey,
    pub action: Action,
    pub enabled: bool,
    pub destructive: bool,
}

#[must_use]
pub(super) fn nav_buttons(state: &AppState) -> Vec<NavButton> {
    use crate::state::Tab;
    let busy = state.busy();
    let (back, forward) = state.history_position();
    let mut buttons = vec![NavButton {
        icon: Icon::Undo,
        tooltip: TextKey::HelpUndo,
        action: Action::Undo,
        enabled: !busy && back > 0,
        destructive: false,
    }];

    if state.is_sculpting() {
        buttons.push(NavButton {
            icon: Icon::Hammer,
            tooltip: TextKey::ResetSculpt,
            action: Action::ResetSculpt,
            enabled: !busy,
            destructive: true,
        });
        buttons.push(NavButton {
            icon: Icon::Broom,
            tooltip: TextKey::ResetMorphs,
            action: Action::ResetMorphs,
            enabled: !busy,
            destructive: true,
        });
    } else if state.is_hair_editing() {
        let has_strands = state
            .hair_project
            .editable_parts()
            .into_iter()
            .filter_map(|id| state.hair_project.part(id))
            .any(|part| !part.strands.is_empty());
        buttons.push(NavButton {
            icon: Icon::Hammer,
            tooltip: TextKey::ResetSculpt,
            action: Action::ResetHairShapes,
            enabled: !busy && has_strands,
            destructive: true,
        });
    } else if matches!(state.active_tab, Tab::Alignment | Tab::Edit) && state.scan_path.is_some() {
        buttons.push(NavButton {
            icon: Icon::Broom,
            tooltip: TextKey::ResetAllPins,
            action: Action::ResetPins,
            enabled: !busy,
            destructive: true,
        });
    } else if matches!(state.active_tab, Tab::Texture)
        && let Some(layer) = state.texture_project.selected_layer_id
    {
        buttons.push(NavButton {
            icon: Icon::Broom,
            tooltip: TextKey::ResetSourceRetouch,
            action: Action::ResetTextureLayer(layer),
            enabled: !busy,
            destructive: true,
        });
    }

    buttons.push(NavButton {
        icon: Icon::Redo,
        tooltip: TextKey::HelpRedo,
        action: Action::Redo,
        enabled: !busy && forward > 0,
        destructive: false,
    });
    buttons
}

#[must_use]
pub(super) fn work_nav_rect(count: usize, viewport: Rect) -> Option<Rect> {
    if count == 0 {
        return None;
    }
    let count = count as f32;
    let size = vec2(
        count * BUTTON + (count - 1.0) * BUTTON_GAP + INSET * 2.0,
        BUTTON + INSET * 2.0,
    );
    let rect = Rect::from_center_size(
        pos2(
            viewport.center().x,
            viewport.max.y - BOTTOM_MARGIN - size.y * 0.5,
        ),
        size,
    );
    (rect.width() <= viewport.width() - 32.0 && rect.min.y > viewport.min.y).then_some(rect)
}

pub(super) fn draw_work_nav(ui: &Ui, state: &mut AppState, viewport: Rect) {
    let buttons = nav_buttons(state);
    let Some(bar) = work_nav_rect(buttons.len(), viewport) else {
        return;
    };
    let locale = state.locale;
    let mut wanted: Option<Action> = None;
    egui::Area::new(Id::new("vkit.viewport.work-nav"))
        .order(egui::Order::Foreground)
        .fixed_pos(bar.min)
        .show(ui.ctx(), |ui| {
            let (rect, _) = ui.allocate_exact_size(bar.size(), Sense::hover());
            ui.painter().rect_filled(
                rect,
                crate::theme::capsule_radius(rect.height()),
                crate::theme::COLOR_TOPBAR.gamma_multiply(0.96),
            );
            for (index, button) in buttons.into_iter().enumerate() {
                let cell = Rect::from_min_size(
                    pos2(
                        rect.left() + INSET + index as f32 * (BUTTON + BUTTON_GAP),
                        rect.top() + INSET,
                    ),
                    vec2(BUTTON, BUTTON),
                );
                let response = ui.interact(
                    cell,
                    Id::new(("vkit.viewport.work-nav", index)),
                    if button.enabled {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                );
                let hovered = button.enabled && response.hovered();
                if hovered {
                    ui.painter().circle_filled(
                        cell.center(),
                        BUTTON * 0.5,
                        if button.destructive {
                            crate::theme::COLOR_DESTRUCTIVE
                        } else {
                            crate::theme::COLOR_SURFACE_HOVER
                        },
                    );
                }
                let colour = if !button.enabled {
                    crate::theme::disabled(crate::theme::COLOR_ICON)
                } else if hovered && button.destructive {
                    crate::theme::COLOR_BG
                } else {
                    crate::theme::COLOR_TEXT
                };
                paint_icon(
                    ui.painter(),
                    cell.shrink(BUTTON * 0.26),
                    button.icon,
                    colour,
                );
                let response = response.on_hover_text(text(locale, button.tooltip));
                if button.enabled && response.clicked() {
                    wanted = Some(button.action);
                }
            }
        });
    if let Some(action) = wanted {
        state.dispatch(action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sculpting() -> AppState {
        let mut state = AppState::default();
        state.active_tab = crate::state::Tab::Morph;
        state.result_preview_phase = crate::state::ResultPreviewPhase::Sculpt;
        state
    }

    #[test]
    fn back_is_always_first_and_forward_always_last() {
        let mut hair = AppState::default();
        hair.active_tab = crate::state::Tab::Hair;
        let mut texture = AppState::default();
        texture.active_tab = crate::state::Tab::Texture;

        for state in [sculpting(), hair, texture, AppState::default()] {
            let buttons = nav_buttons(&state);
            assert!(buttons.len() >= 2);
            assert_eq!(buttons.first().map(|button| button.icon), Some(Icon::Undo));
            assert_eq!(buttons.last().map(|button| button.icon), Some(Icon::Redo));
        }
    }

    #[test]
    fn the_sculpt_tab_offers_both_resets_between_the_arrows() {
        let buttons = nav_buttons(&sculpting());
        let icons: Vec<Icon> = buttons.iter().map(|button| button.icon).collect();
        assert_eq!(
            icons,
            vec![Icon::Undo, Icon::Hammer, Icon::Broom, Icon::Redo]
        );
        assert!(
            buttons[1].destructive && buttons[2].destructive,
            "both resets throw work away and have to look like it"
        );
        assert!(!buttons[0].destructive && !buttons[3].destructive);
    }

    #[test]
    fn the_pin_tab_resets_its_pins_from_the_same_bar() {
        let mut state = AppState::default();
        state.active_tab = crate::state::Tab::Edit;
        assert_eq!(nav_buttons(&state).len(), 2, "no scan, nothing to reset");

        state.scan_path = Some(std::path::PathBuf::from("head.obj"));
        let buttons = nav_buttons(&state);
        assert_eq!(buttons.len(), 3);
        assert_eq!(buttons[1].icon, Icon::Broom);
        assert!(buttons[1].destructive);
    }

    #[test]
    fn a_tab_with_no_reset_still_has_somewhere_to_step_back_to() {
        let buttons = nav_buttons(&AppState::default());
        assert_eq!(buttons.len(), 2);
    }

    #[test]
    fn an_unavailable_step_is_greyed_rather_than_removed() {
        let state = AppState::default();
        assert_eq!(
            state.history_position().0,
            0,
            "a fresh session has nothing to undo"
        );
        let buttons = nav_buttons(&state);
        assert!(!buttons[0].enabled);
        assert_eq!(buttons.len(), 2, "still drawn");
    }

    #[test]
    fn a_narrow_viewport_gets_no_bar() {
        let wide = Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 700.0));
        let sliver = Rect::from_min_size(pos2(0.0, 0.0), vec2(80.0, 700.0));
        assert!(work_nav_rect(4, wide).is_some());
        assert!(work_nav_rect(4, sliver).is_none());
    }

    #[test]
    fn the_bar_grows_around_its_own_centre() {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 700.0));
        let two = work_nav_rect(2, viewport).unwrap();
        let four = work_nav_rect(4, viewport).unwrap();
        assert!(four.width() > two.width());
        assert!((four.center().x - two.center().x).abs() < 0.01);
        assert!((four.center().y - two.center().y).abs() < 0.01);
    }
}
