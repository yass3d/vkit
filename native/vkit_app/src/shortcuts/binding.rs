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

struct Held {
    shown: &'static str,
    stored: &'static str,
    is_held: fn(Modifiers) -> bool,
}

const HELD: [Held; 3] = [
    Held {
        shown: "Ctrl",
        stored: "ctrl",
        is_held: |modifiers| modifiers.ctrl || modifiers.command,
    },
    Held {
        shown: "Shift",
        stored: "shift",
        is_held: |modifiers| modifiers.shift,
    },
    Held {
        shown: "Alt",
        stored: "alt",
        is_held: |modifiers| modifiers.alt,
    },
];

impl Binding {
    pub fn label(self) -> String {
        let ModifierPolicy::Exactly(modifiers) = self.modifiers else {
            return self.trigger.label().to_owned();
        };
        let mut spelled = String::new();
        for held in HELD {
            if (held.is_held)(modifiers) {
                spelled.push_str(held.shown);
                spelled.push('+');
            }
        }
        spelled.push_str(self.trigger.label());
        spelled
    }

    pub(super) fn modifier_name(self) -> Option<String> {
        let ModifierPolicy::Exactly(modifiers) = self.modifiers else {
            return Some("any".to_owned());
        };
        let held: Vec<&str> = HELD
            .into_iter()
            .filter(|held| (held.is_held)(modifiers))
            .map(|held| held.stored)
            .collect();
        if held.is_empty() {
            return Some("none".to_owned());
        }
        Some(held.join("+"))
    }

    pub(super) fn modifiers_by_name(name: &str) -> Option<ModifierPolicy> {
        if name == "any" {
            return Some(ModifierPolicy::Ignored);
        }
        if name == "none" {
            return Some(ModifierPolicy::Exactly(Modifiers::NONE));
        }
        let mut modifiers = Modifiers::NONE;
        for part in name.split('+') {
            match part {
                "ctrl" => modifiers |= Modifiers::COMMAND,
                "shift" => modifiers |= Modifiers::SHIFT,
                "alt" => modifiers |= Modifiers::ALT,
                _ => return None,
            }
        }
        Some(ModifierPolicy::Exactly(modifiers))
    }
}
