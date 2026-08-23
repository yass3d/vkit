//! The bindings in force, and the rules for changing them.

use std::collections::BTreeMap;

use super::{Binding, Shortcut, Trigger};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Keymap {
    bindings: [Binding; Shortcut::ALL.len()],
}

impl Default for Keymap {
    fn default() -> Self {
        let mut bindings = [Shortcut::Undo.default_binding(); Shortcut::ALL.len()];
        for shortcut in Shortcut::ALL {
            bindings[shortcut.slot()] = shortcut.default_binding();
        }
        Self { bindings }
    }
}

impl Keymap {
    #[must_use]
    pub fn binding(&self, shortcut: Shortcut) -> Binding {
        self.bindings[shortcut.slot()]
    }

    pub fn rebind(&mut self, shortcut: Shortcut, binding: Binding) {
        self.bindings[shortcut.slot()] = binding;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    #[must_use]
    pub fn is_default(&self, shortcut: Shortcut) -> bool {
        self.binding(shortcut) == shortcut.default_binding()
    }

    #[must_use]
    pub fn conflict(&self, shortcut: Shortcut, binding: Binding) -> Option<Shortcut> {
        Shortcut::ALL.into_iter().find(|other| {
            *other != shortcut
                && self.binding(*other) == binding
                && other.context().overlaps(shortcut.context())
        })
    }

    #[must_use]
    pub fn to_stored(&self) -> BTreeMap<String, String> {
        Shortcut::ALL
            .into_iter()
            .filter(|shortcut| !self.is_default(*shortcut))
            .filter_map(|shortcut| {
                let binding = self.binding(shortcut);
                let modifiers = binding.modifier_name()?;
                Some((
                    shortcut.name().to_owned(),
                    format!("{modifiers}+{}", binding.trigger.stored_name()),
                ))
            })
            .collect()
    }

    #[must_use]
    pub fn from_stored(stored: &BTreeMap<String, String>) -> Self {
        let mut keymap = Self::default();
        for (name, spelling) in stored {
            let (Some(shortcut), Some((modifiers, trigger))) =
                (Shortcut::by_name(name), spelling.split_once('+'))
            else {
                continue;
            };
            let (Some(modifiers), Some(trigger)) = (
                Binding::modifiers_by_name(modifiers),
                Trigger::parse(trigger),
            ) else {
                continue;
            };
            keymap.rebind(shortcut, Binding { trigger, modifiers });
        }
        keymap
    }
}
