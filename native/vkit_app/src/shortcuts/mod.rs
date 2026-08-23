//! Shortcuts: the catalog of what a key can do, and the map of what is bound.
//!
//! `catalog` names every shortcut and its factory binding, `trigger` and
//! `binding` say what a binding is made of, and `keymap` holds the ones in
//! force. Everything else in this program asks through here.

mod binding;
mod catalog;
mod keymap;
mod trigger;

pub use binding::{Binding, ModifierPolicy};
pub use catalog::{Shortcut, ShortcutContext};
pub use keymap::Keymap;
pub use trigger::{NumpadKey, Trigger};

use egui::Ui;

const KEYMAP_ID: &str = "vkit.shortcuts.keymap";

pub fn install(ctx: &egui::Context, keymap: &Keymap) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(KEYMAP_ID), keymap.clone()));
}

fn current(ui: &Ui) -> Keymap {
    ui.data(|data| data.get_temp::<Keymap>(egui::Id::new(KEYMAP_ID)))
        .unwrap_or_default()
}

pub const BRUSH_SIZE_HINT: &str = "[ / ] / F";

pub const BRUSH_STRENGTH_HINT: &str = "Shift+F";

#[cfg(test)]
mod tests {
    use egui::Modifiers;

    use super::*;

    #[test]
    fn every_label_names_the_press_it_is_bound_to() {
        for shortcut in Shortcut::ALL {
            assert_eq!(
                shortcut.label(),
                shortcut.default_binding().label(),
                "{shortcut:?} advertises a press it does not listen for"
            );
        }
    }

    #[test]
    fn a_joined_hint_still_names_every_press_it_stands_for() {
        assert_eq!(
            BRUSH_SIZE_HINT,
            format!(
                "{} / {} / {}",
                Shortcut::BrushSizeDown.label(),
                Shortcut::BrushSizeUp.label(),
                Shortcut::BrushSizeSweep.label()
            ),
            "the size hint has drifted from the presses that actually resize the brush"
        );
        assert_eq!(BRUSH_STRENGTH_HINT, Shortcut::BrushStrengthSweep.label());
    }

    #[test]
    fn the_hair_toolbox_keys_are_the_ones_the_tooltips_promise() {
        use egui::Key;

        for (shortcut, key, label) in [
            (Shortcut::HairPlantTool, Key::A, "A"),
            (Shortcut::HairGrowTool, Key::E, "E"),
            (Shortcut::HairCutTool, Key::X, "X"),
            (Shortcut::HairEraseTool, Key::T, "T"),
            (Shortcut::HairMirrorPart, Key::M, "M"),
            (Shortcut::HairPuffTool, Key::B, "B"),
            (Shortcut::HairPinchTool, Key::P, "P"),
            (Shortcut::HairPickTool, Key::V, "V"),
        ] {
            assert_eq!(shortcut.trigger(), Trigger::Key(key), "{shortcut:?}");
            assert_eq!(shortcut.label(), label, "{shortcut:?}");
            assert_eq!(
                shortcut.context(),
                ShortcutContext::HairEdit,
                "{shortcut:?} would fire outside the hair tab"
            );
            assert_eq!(
                shortcut.default_binding().label(),
                label,
                "{shortcut:?} draws a different key than it claims"
            );
        }
    }

    #[test]
    fn no_two_shortcuts_claim_the_same_press() {
        for (index, first) in Shortcut::ALL.iter().enumerate() {
            for second in &Shortcut::ALL[index + 1..] {
                let same_press = first.default_binding() == second.default_binding();
                assert!(
                    !same_press || !first.context().overlaps(second.context()),
                    "{first:?} and {second:?} both claim the same press, \
                     and {:?} overlaps {:?}",
                    first.context(),
                    second.context()
                );
            }
        }
    }

    #[test]
    fn a_shared_key_is_only_forgiven_across_stages_that_never_coexist() {
        use ShortcutContext::{Alignment, DetailEdit, Global};
        assert!(!Alignment.overlaps(DetailEdit));
        assert!(Alignment.overlaps(Alignment));
        assert!(Global.overlaps(Alignment));
        assert!(Alignment.overlaps(Global));

        assert_eq!(
            Shortcut::FrameSelected.trigger(),
            Shortcut::BrushSizeSweep.trigger()
        );
        assert!(
            !Shortcut::FrameSelected
                .context()
                .overlaps(Shortcut::BrushSizeSweep.context())
        );
    }

    #[test]
    fn brush_controls_still_fire_while_a_stroke_holds_a_modifier() {
        for shortcut in [
            Shortcut::BrushSizeDown,
            Shortcut::BrushSizeUp,
            Shortcut::SculptGrabBrush,
            Shortcut::SculptRestoreBrush,
        ] {
            let policy = shortcut.modifiers();
            for held in [
                Modifiers::SHIFT,
                Modifiers::CTRL,
                Modifiers::ALT,
                Modifiers::NONE,
            ] {
                assert!(
                    policy.admits(held),
                    "{shortcut:?} must survive {held:?} held during a stroke"
                );
            }
        }
    }

    #[test]
    fn tool_selection_refuses_a_modified_press() {
        for shortcut in [Shortcut::TexturePinBrush, Shortcut::TextureCloneBrush] {
            let policy = shortcut.modifiers();
            assert!(policy.admits(Modifiers::NONE));
            for held in [Modifiers::COMMAND, Modifiers::CTRL, Modifiers::SHIFT] {
                assert!(
                    !policy.admits(held),
                    "{shortcut:?} must not fire while {held:?} is held"
                );
            }
        }
    }
}

#[cfg(test)]
mod registry_tests {
    use egui::{Modifiers, PointerButton};

    use super::*;

    /// Every shortcut has a slot of its own.
    ///
    /// `slot` scans `ALL` for the discriminant and falls back to 0 when it does
    /// not find one, so a shortcut left out of `ALL` would silently read and
    /// write `Undo`'s binding. Nothing else would say a word.
    #[test]
    fn every_shortcut_owns_one_slot_and_no_two_share_it() {
        assert_eq!(
            Shortcut::ALL.len(),
            Shortcut::ALL
                .into_iter()
                .map(|shortcut| shortcut.name())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            "two shortcuts answer to one name",
        );
        for (index, shortcut) in Shortcut::ALL.into_iter().enumerate() {
            assert_eq!(
                shortcut.slot(),
                index,
                "{shortcut:?} sits in the wrong slot"
            );
        }
    }

    /// No two shortcuts that can both fire start out on the same binding.
    ///
    /// The capture field refuses a collision the reader makes. This says the
    /// factory keymap does not ship with one already in it.
    #[test]
    fn no_two_reachable_shortcuts_ship_on_the_same_binding() {
        let keymap = Keymap::default();
        for shortcut in Shortcut::ALL {
            let clash = keymap.conflict(shortcut, keymap.binding(shortcut));
            assert!(
                clash.is_none(),
                "{shortcut:?} and {:?} both answer {}",
                clash.unwrap(),
                keymap.binding(shortcut).label(),
            );
        }
    }

    /// A keymap file cannot smuggle a collision past the capture field.
    #[test]
    fn an_imported_keymap_refuses_a_binding_another_shortcut_already_holds() {
        let taken = Keymap::default().binding(Shortcut::HairCutTool);
        let stored = std::collections::BTreeMap::from([(
            Shortcut::HairPlantTool.name().to_owned(),
            format!("none+{}", taken.trigger.stored_name()),
        )]);

        let keymap = Keymap::from_stored(&stored);
        assert_eq!(
            keymap.binding(Shortcut::HairPlantTool),
            Shortcut::HairPlantTool.default_binding(),
            "the collided entry is dropped, not applied",
        );
        assert_eq!(keymap.binding(Shortcut::HairCutTool), taken);
    }

    /// What a keymap writes, a keymap reads back.
    #[test]
    fn every_kind_of_trigger_survives_the_round_trip() {
        let mut keymap = Keymap::default();
        // The two pad entries are a SWAP. Each one collides with the other's
        // factory binding halfway through the read, which is exactly the case
        // that made checking-as-we-go wrong.
        let cases = [
            (
                Shortcut::ViewTop,
                Trigger::Numpad(NumpadKey::Two),
                ModifierPolicy::Exactly(Modifiers::NONE),
            ),
            (
                Shortcut::ViewBottom,
                Trigger::Numpad(NumpadKey::Eight),
                ModifierPolicy::Exactly(Modifiers::NONE),
            ),
            (
                Shortcut::TabSave,
                Trigger::Key(egui::Key::Q),
                ModifierPolicy::Exactly(Modifiers::SHIFT),
            ),
            (
                Shortcut::HairPickTool,
                Trigger::Mouse(PointerButton::Extra1),
                ModifierPolicy::Ignored,
            ),
        ];
        for (shortcut, trigger, modifiers) in cases {
            keymap.rebind(shortcut, Binding { trigger, modifiers });
        }

        let read_back = Keymap::from_stored(&keymap.to_stored());
        for (shortcut, trigger, modifiers) in cases {
            assert_eq!(
                read_back.binding(shortcut),
                Binding { trigger, modifiers },
                "{shortcut:?} did not survive being written down",
            );
        }
    }

    /// The number pad answers through the catalog and through nothing else.
    #[test]
    fn every_pad_key_stands_for_exactly_one_shortcut() {
        let keymap = Keymap::default();
        for key in NumpadKey::ALL {
            let shortcut = keymap.shortcut_for(Trigger::Numpad(key));
            assert!(
                shortcut.is_some(),
                "{key:?} is taken from egui and answers to nothing",
            );
            assert_eq!(
                Shortcut::ALL
                    .into_iter()
                    .filter(|candidate| keymap.binding(*candidate).trigger == Trigger::Numpad(key))
                    .count(),
                1,
            );
        }
    }

    /// A view key and a tab key are as reachable as a global one.
    #[test]
    fn a_context_that_fires_anywhere_is_checked_against_everything() {
        for context in [
            ShortcutContext::Global,
            ShortcutContext::View,
            ShortcutContext::Navigation,
        ] {
            assert!(context.is_everywhere());
            for other in [
                ShortcutContext::Alignment,
                ShortcutContext::DetailEdit,
                ShortcutContext::HairEdit,
            ] {
                assert!(context.overlaps(other), "{context:?} vs {other:?}");
                assert!(other.overlaps(context), "{other:?} vs {context:?}");
            }
        }
        assert!(!ShortcutContext::HairEdit.overlaps(ShortcutContext::DetailEdit));
    }

    /// The factory label is derived from the factory binding, not listed beside
    /// it. Two tables holding one value is how a rebound key kept showing the
    /// letter it used to be on.
    #[test]
    fn a_label_is_read_off_the_binding_it_describes() {
        for shortcut in Shortcut::ALL {
            assert_eq!(shortcut.label(), shortcut.default_binding().label());
        }
        assert_eq!(Shortcut::Undo.label(), "Ctrl+Z");
        assert_eq!(Shortcut::BrushSizeDown.label(), "[");
        assert_eq!(Shortcut::ViewFront.label(), "Num 5");
        assert_eq!(Shortcut::ViewPan.label(), "Shift+Wheel click");
    }
}
