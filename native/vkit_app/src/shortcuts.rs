use egui::{Key, Modifiers, Ui};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModifierPolicy {
    Exactly(Modifiers),

    Ignored,
}

impl ModifierPolicy {
    fn admits(self, held: Modifiers) -> bool {
        match self {
            Self::Ignored => true,
            Self::Exactly(required) if required.is_none() => held.is_none(),
            Self::Exactly(required) => held.matches_logically(required),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortcutContext {
    Global,

    Alignment,

    DetailEdit,
}

impl ShortcutContext {
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the conflict test"))]
    pub const fn overlaps(self, other: Self) -> bool {
        matches!(self, Self::Global) || matches!(other, Self::Global) || self as u8 == other as u8
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shortcut {
    SculptGrabBrush,

    SculptRestoreBrush,

    TexturePinBrush,

    TextureCloneBrush,

    BrushSizeDown,

    BrushSizeUp,

    Undo,

    BrushSizeSweep,
    BrushStrengthSweep,

    ViewTrackball,

    CancelStencil,

    FrameSelected,

    XSymmetry,
}

impl Shortcut {
    #[cfg(test)]
    pub const ALL: [Self; 13] = [
        Self::SculptGrabBrush,
        Self::SculptRestoreBrush,
        Self::TexturePinBrush,
        Self::TextureCloneBrush,
        Self::BrushSizeDown,
        Self::BrushSizeUp,
        Self::Undo,
        Self::BrushSizeSweep,
        Self::BrushStrengthSweep,
        Self::ViewTrackball,
        Self::CancelStencil,
        Self::FrameSelected,
        Self::XSymmetry,
    ];

    pub const fn key(self) -> Key {
        match self {
            Self::SculptGrabBrush => Key::G,
            Self::SculptRestoreBrush => Key::H,
            Self::TexturePinBrush => Key::P,
            Self::TextureCloneBrush => Key::C,
            Self::BrushSizeDown => Key::OpenBracket,
            Self::BrushSizeUp => Key::CloseBracket,
            Self::Undo => Key::Z,
            Self::BrushSizeSweep | Self::BrushStrengthSweep => Key::F,
            Self::ViewTrackball => Key::R,
            Self::CancelStencil => Key::Escape,
            Self::FrameSelected => Key::F,
            Self::XSymmetry => Key::X,
        }
    }

    #[cfg_attr(not(test), expect(dead_code, reason = "read by the conflict test"))]
    pub const fn context(self) -> ShortcutContext {
        match self {
            Self::Undo => ShortcutContext::Global,

            Self::ViewTrackball => ShortcutContext::Global,

            Self::XSymmetry => ShortcutContext::Global,

            Self::FrameSelected => ShortcutContext::Alignment,
            Self::SculptGrabBrush
            | Self::SculptRestoreBrush
            | Self::TexturePinBrush
            | Self::TextureCloneBrush
            | Self::BrushSizeDown
            | Self::BrushSizeUp
            | Self::BrushSizeSweep
            | Self::BrushStrengthSweep
            | Self::CancelStencil => ShortcutContext::DetailEdit,
        }
    }

    pub const fn modifiers(self) -> ModifierPolicy {
        match self {
            Self::Undo => ModifierPolicy::Exactly(Modifiers::COMMAND),

            Self::TexturePinBrush | Self::TextureCloneBrush => {
                ModifierPolicy::Exactly(Modifiers::NONE)
            }

            Self::SculptGrabBrush
            | Self::SculptRestoreBrush
            | Self::BrushSizeDown
            | Self::BrushSizeUp => ModifierPolicy::Ignored,

            Self::BrushStrengthSweep => ModifierPolicy::Exactly(Modifiers::SHIFT),

            Self::BrushSizeSweep
            | Self::ViewTrackball
            | Self::CancelStencil
            | Self::FrameSelected
            | Self::XSymmetry => ModifierPolicy::Exactly(Modifiers::NONE),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::SculptGrabBrush => "G",
            Self::SculptRestoreBrush => "H",
            Self::TexturePinBrush => "P",
            Self::TextureCloneBrush => "C",
            Self::BrushSizeDown => "[",
            Self::BrushSizeUp => "]",
            Self::Undo => "Ctrl+Z",
            Self::BrushSizeSweep => "F",
            Self::BrushStrengthSweep => "Shift+F",
            Self::ViewTrackball => "R",
            Self::CancelStencil => "Esc",
            Self::FrameSelected => "F",
            Self::XSymmetry => "X",
        }
    }

    pub fn pressed(self, ui: &Ui) -> bool {
        if ui.ctx().egui_wants_keyboard_input() {
            return false;
        }
        let policy = self.modifiers();
        ui.input(|input| input.key_pressed(self.key()) && policy.admits(input.modifiers))
    }
}

/// One control can answer to more than one press. A tooltip has room for a
/// single line, so the presses are joined into one hint; the test below pins
/// each join to the labels it stands for, so a rebound key cannot leave a hint
/// behind advertising the old one.
pub const BRUSH_SIZE_HINT: &str = "[ / ] / F";

pub const BRUSH_STRENGTH_HINT: &str = "Shift+F";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_label_names_the_key_it_is_bound_to() {
        for shortcut in Shortcut::ALL {
            let key = match shortcut.key() {
                Key::OpenBracket => "[",
                Key::CloseBracket => "]",
                Key::Tab => "Tab",
                Key::Z => "Ctrl+Z",
                Key::Escape => "Esc",
                key => key.name(),
            };
            // When a policy demands a modifier the modifier is half the binding,
            // so the label has to name it too. Two shortcuts share the F key and
            // are told apart by Shift alone; without this, one of them would
            // advertise a press that belongs to the other. Ctrl+Z spells its own
            // modifier in the table above.
            let expected = match shortcut.modifiers() {
                ModifierPolicy::Exactly(modifiers) if modifiers == Modifiers::SHIFT => {
                    format!("Shift+{key}")
                }
                _ => key.to_owned(),
            };
            assert_eq!(
                shortcut.label(),
                expected,
                "{shortcut:?} advertises a key it does not listen for"
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
    fn no_two_shortcuts_claim_the_same_press() {
        for (index, first) in Shortcut::ALL.iter().enumerate() {
            for second in &Shortcut::ALL[index + 1..] {
                let same_press =
                    first.key() == second.key() && first.modifiers() == second.modifiers();
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
            Shortcut::FrameSelected.key(),
            Shortcut::BrushSizeSweep.key()
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
