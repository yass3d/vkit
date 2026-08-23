//! What a shortcut is bound to: a trigger plus the modifiers it demands.

use egui::Modifiers;

use super::Trigger;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModifierPolicy {
    Exactly(Modifiers),

    Ignored,
}

impl ModifierPolicy {
    pub fn admits(self, held: Modifiers) -> bool {
        match self {
            Self::Ignored => true,
            Self::Exactly(required) if required.is_none() => held.is_none(),
            Self::Exactly(required) => held.matches_logically(required),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Binding {
    pub trigger: Trigger,
    pub modifiers: ModifierPolicy,
}

impl Binding {
    pub fn label(self) -> String {
        let prefix = match self.modifiers {
            ModifierPolicy::Exactly(modifiers) if modifiers == Modifiers::COMMAND => "Ctrl+",
            ModifierPolicy::Exactly(modifiers) if modifiers == Modifiers::SHIFT => "Shift+",
            ModifierPolicy::Exactly(modifiers) if modifiers == Modifiers::ALT => "Alt+",
            ModifierPolicy::Exactly(_) | ModifierPolicy::Ignored => "",
        };
        format!("{prefix}{}", self.trigger.label())
    }

    pub(super) fn modifier_name(self) -> Option<&'static str> {
        match self.modifiers {
            ModifierPolicy::Ignored => Some("any"),
            ModifierPolicy::Exactly(modifiers) if modifiers == Modifiers::COMMAND => Some("ctrl"),
            ModifierPolicy::Exactly(modifiers) if modifiers == Modifiers::SHIFT => Some("shift"),
            ModifierPolicy::Exactly(modifiers) if modifiers == Modifiers::ALT => Some("alt"),
            ModifierPolicy::Exactly(modifiers) if modifiers.is_none() => Some("none"),
            ModifierPolicy::Exactly(_) => None,
        }
    }

    pub(super) fn modifiers_by_name(name: &str) -> Option<ModifierPolicy> {
        match name {
            "any" => Some(ModifierPolicy::Ignored),
            "ctrl" => Some(ModifierPolicy::Exactly(Modifiers::COMMAND)),
            "shift" => Some(ModifierPolicy::Exactly(Modifiers::SHIFT)),
            "alt" => Some(ModifierPolicy::Exactly(Modifiers::ALT)),
            "none" => Some(ModifierPolicy::Exactly(Modifiers::NONE)),
            _ => None,
        }
    }
}
