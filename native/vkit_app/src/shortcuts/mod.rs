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
pub use trigger::{ModifierKey, NumpadKey, Trigger};

use egui::Ui;

const KEYMAP_ID: &str = "vkit.shortcuts.keymap";

const PAD_PRESS_ID: &str = "vkit.shortcuts.pad-press";

/// Tell egui that a pad key went down.
///
/// egui never sees one: `egui-winit` folds `Numpad5` into `Num5`, which is why
/// the pad is read off the physical key in `runtime.rs` instead. The Settings
/// capture field is inside egui, so without this it could offer no way to put a
/// view on a different pad key — the binding would be listed and unbindable.
pub fn note_pad_press(ctx: &egui::Context, key: NumpadKey) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(PAD_PRESS_ID), key));
}

/// The pad key struck since this was last asked, taken rather than read.
///
/// Taken, so one press cannot be captured twice by two frames of the same
/// dialog.
#[must_use]
pub fn take_pad_press(ui: &Ui) -> Option<NumpadKey> {
    ui.data_mut(|data| {
        let id = egui::Id::new(PAD_PRESS_ID);
        let key = data.get_temp::<NumpadKey>(id);
        if key.is_some() {
            data.remove::<NumpadKey>(id);
        }
        key
    })
}

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

    /// A pad key answers through the catalog, or it is left to egui.
    ///
    /// Not every pad key has to be bound — `Num *` and `Num -` are free. What
    /// must not happen is two shortcuts sharing one, or one being taken away
    /// from egui with nothing to answer it, which would eat the keystroke of
    /// somebody typing a number into a field.
    #[test]
    fn no_pad_key_stands_for_more_than_one_shortcut() {
        let keymap = Keymap::default();
        let mut bound = 0;
        for key in NumpadKey::ALL {
            let claimants = Shortcut::ALL
                .into_iter()
                .filter(|candidate| keymap.binding(*candidate).trigger == Trigger::Numpad(key))
                .count();
            assert!(claimants <= 1, "{claimants} shortcuts answer {key:?}");
            assert_eq!(
                claimants == 1,
                keymap.shortcut_for(Trigger::Numpad(key)).is_some()
            );
            bound += claimants;
        }
        assert!(
            bound >= 12,
            "only {bound} pad keys are bound; the views have gone missing"
        );
    }

    /// A binding may demand more than one modifier, and survive being saved.
    ///
    /// It could not before. `modifier_name` answered `None` for anything that
    /// was not exactly one modifier, and `to_stored` DROPPED an entry it could
    /// not name — so a `Ctrl+Shift+I` a reader had set was gone on the next
    /// launch with nothing anywhere to say why.
    #[test]
    fn a_binding_with_two_modifiers_is_written_down_and_read_back() {
        let mut keymap = Keymap::default();
        let combo = Binding {
            trigger: Trigger::Key(egui::Key::I),
            modifiers: ModifierPolicy::Exactly(Modifiers::COMMAND | Modifiers::SHIFT),
        };
        keymap.rebind(Shortcut::LayerInvertSelection, combo);
        assert_eq!(combo.label(), "Ctrl+Shift+I");

        let stored = keymap.to_stored();
        assert!(
            stored.contains_key(Shortcut::LayerInvertSelection.name()),
            "the entry was dropped instead of written",
        );
        assert_eq!(
            Keymap::from_stored(&stored).binding(Shortcut::LayerInvertSelection),
            combo,
        );
    }

    /// One order for every spelling, so two equal bindings cannot look unequal.
    #[test]
    fn modifiers_are_always_spelled_in_the_same_order() {
        let one_way = Binding {
            trigger: Trigger::Key(egui::Key::A),
            modifiers: ModifierPolicy::Exactly(Modifiers::ALT | Modifiers::SHIFT),
        };
        let other_way = Binding {
            trigger: Trigger::Key(egui::Key::A),
            modifiers: ModifierPolicy::Exactly(Modifiers::SHIFT | Modifiers::ALT),
        };
        assert_eq!(one_way.label(), "Shift+Alt+A");
        assert_eq!(one_way.label(), other_way.label());
    }

    /// The four layer keys read as a set, and none collides with anything.
    #[test]
    fn the_layer_keys_read_as_one_family() {
        let keymap = Keymap::default();
        assert_eq!(keymap.binding(Shortcut::LayerHide).label(), "H");
        assert_eq!(keymap.binding(Shortcut::LayerUnhideAll).label(), "Alt+H");
        assert_eq!(
            keymap.binding(Shortcut::LayerInvertSelection).label(),
            "Ctrl+I"
        );
        assert_eq!(keymap.binding(Shortcut::LayerIsolate).label(), "Num /");
        for shortcut in [
            Shortcut::LayerHide,
            Shortcut::LayerUnhideAll,
            Shortcut::LayerIsolate,
            Shortcut::LayerInvertSelection,
        ] {
            assert!(
                keymap
                    .conflict(shortcut, keymap.binding(shortcut))
                    .is_none()
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

#[cfg(test)]
mod untokenised_input_tests {
    /// Every way this program reads a key, a button or a modifier for itself,
    /// by file, with a count and a reason.
    ///
    /// Not a ban — several of these should never be bindings. `settings.rs`
    /// reads keys in order to BIND them; `ui_components.rs` answers Space and
    /// Enter on a focused widget, which is how a keyboard reaches a button in
    /// any program; `runtime.rs` is deciding whether egui wants the keystroke
    /// before anything else looks at it.
    ///
    /// What it stops is the thing that had already happened three times over:
    /// a file quietly reading an input and meaning something by it, with
    /// nothing in Settings to say the shortcut exists. The number-pad views,
    /// the tab keys, space-to-pan, the log's copy key and the light's
    /// right-drag were all found this way after the first, narrower sweep
    /// missed them.
    ///
    /// Adding a read fails this test. Either name it in the catalog and read it
    /// through `Shortcut`, or add it here with a line saying why it is not a
    /// binding.
    const ALLOWED: &[(&str, usize, &str)] = &[
        (
            "runtime.rs",
            2,
            "the winit arm the catalog's own number-pad path goes through",
        ),
        (
            "settings.rs",
            5,
            "the capture field: reading keys and modifiers in order to bind them",
        ),
        (
            "texture_ui.rs",
            13,
            "which button is down while the canvas is being painted or dragged",
        ),
        (
            "ui.rs",
            3,
            "the diagnostic log's own bound copy key, and a paste event",
        ),
        (
            "ui_components.rs",
            1,
            "Space and Enter reaching a focused widget, as in any program",
        ),
        (
            "viewport/alignment_gizmo.rs",
            4,
            "which button is down mid-drag, and the axis constraint the gizmo shows on screen",
        ),
        (
            "viewport/camera_input.rs",
            1,
            "the alt snap on an orbit whose BUTTON already comes from a Shortcut",
        ),
        (
            "viewport/detail_panels.rs",
            11,
            "button state mid-drag, plus list ordering and the nudge step, which are panel affordances",
        ),
        (
            "viewport/hair_input.rs",
            12,
            "button state mid-stroke, and the pick fallback that follows the tool",
        ),
        (
            "viewport/panels.rs",
            1,
            "whether a tool button is being pressed, for how it is drawn",
        ),
        (
            "viewport/reference_overlay.rs",
            3,
            "button state while a reference is being dragged",
        ),
        (
            "viewport/sculpt_input.rs",
            6,
            "button state mid-stroke, not a mode",
        ),
        (
            "sweep_gesture.rs",
            2,
            "pointer bookkeeping for a sweep whose KEY already comes from a Shortcut",
        ),
        (
            "ui/hair_ui.rs",
            2,
            "Escape cancelling a rename inside a text field, which is not a shortcut",
        ),
        (
            "viewport/pins.rs",
            6,
            "click gating for placing and picking pins, with no name a reader would look up",
        ),
    ];

    /// The spellings that reach the input queue directly.
    const NEEDLES: &[&str] = &[
        "modifiers.shift",
        "modifiers.alt",
        "modifiers.ctrl",
        "modifiers.command",
        "key_pressed(",
        "key_down(",
        "key_released(",
        "consume_key(",
        "keys_down",
        "button_pressed(",
        "button_released(",
        "button_down(",
        "primary_pressed(",
        "primary_released(",
        "primary_down(",
        "secondary_pressed(",
        "secondary_released(",
        "secondary_down(",
    ];

    fn counted() -> std::collections::BTreeMap<String, usize> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut counted: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut pending = vec![root.clone()];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory).expect("read the source tree") {
                let path = entry.expect("read a source entry").path();
                if path.is_dir() {
                    // The catalog is where reading input is the job.
                    if path.file_name().is_some_and(|name| name == "shortcuts") {
                        continue;
                    }
                    pending.push(path);
                    continue;
                }
                if path.extension().is_none_or(|extension| extension != "rs") {
                    continue;
                }
                if path.file_name().is_some_and(|name| name == "tests.rs") {
                    continue;
                }
                let relative = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let body = std::fs::read_to_string(&path).expect("read a source file");
                let mut in_tests = false;
                for line in body.lines() {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("mod tests") || trimmed.starts_with("mod capture_tests")
                    {
                        in_tests = true;
                    }
                    if in_tests || trimmed.starts_with("//") || trimmed.starts_with("///") {
                        continue;
                    }
                    if NEEDLES.iter().any(|needle| line.contains(needle)) {
                        *counted.entry(relative.clone()).or_default() += 1;
                    }
                }
            }
        }
        counted
    }

    #[test]
    fn no_file_reads_an_input_the_catalog_has_not_accounted_for() {
        let counted = counted();
        let allowed: std::collections::BTreeMap<&str, usize> = ALLOWED
            .iter()
            .map(|(file, count, _)| (*file, *count))
            .collect();

        let mut wrong = Vec::new();
        for (file, count) in &counted {
            let permitted = allowed.get(file.as_str()).copied().unwrap_or(0);
            if *count != permitted {
                wrong.push(format!("{file}: {count} reads, {permitted} accounted for"));
            }
        }
        for (file, count, _) in ALLOWED {
            if !counted.contains_key(*file) && *count != 0 {
                wrong.push(format!("{file}: down for {count} reads and has none"));
            }
        }
        assert!(
            wrong.is_empty(),
            "the inventory is out of date. Name the input in the catalog and read \
             it through `Shortcut`, or update ALLOWED with a reason: {}",
            wrong.join("; ")
        );
    }

    /// Every reason is written down, and no file is listed twice.
    #[test]
    fn every_allowance_carries_a_reason() {
        let mut seen = std::collections::BTreeSet::new();
        for (file, _, reason) in ALLOWED {
            assert!(seen.insert(*file), "{file} is allowed twice");
            assert!(reason.len() > 12, "{file} has no real reason: {reason}");
        }
    }
}
