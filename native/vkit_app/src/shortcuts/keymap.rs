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

    /// Which shortcut, if any, this trigger currently stands for.
    ///
    /// The inverse of `binding`, and the only way a reader of physical keys is
    /// allowed to reach the catalog: `runtime.rs` sees a number-pad press,
    /// which egui cannot tell from the top row, and asks here rather than
    /// keeping a table of its own. It kept one for months, and nothing in
    /// Settings knew those keys existed.
    #[must_use]
    pub fn shortcut_for(&self, trigger: Trigger) -> Option<Shortcut> {
        Shortcut::ALL
            .into_iter()
            .find(|shortcut| self.binding(*shortcut).trigger == trigger)
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
            // Split from the RIGHT: the modifier half may now be `ctrl+shift`,
            // and no trigger name carries a `+` of its own.
            let (Some(shortcut), Some((modifiers, trigger))) =
                (Shortcut::by_name(name), spelling.rsplit_once('+'))
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
        keymap.drop_collisions();
        keymap
    }

    /// Send back to its factory binding anything that ended up sharing a key.
    ///
    /// The capture field enforces this on the reader; a file has to be held to
    /// the same rule, or a hand-edited one could put two shortcuts on one key
    /// and the press would go to whichever the catalog happened to list first,
    /// with nothing anywhere to say why.
    ///
    /// Checked AFTER every entry is applied, never as each one lands. Two
    /// shortcuts that swap keys each collide with the other's factory binding
    /// halfway through, so checking on the way in would throw out a swap the
    /// reader made deliberately and had just exported.
    fn drop_collisions(&mut self) {
        for shortcut in Shortcut::ALL {
            if self.conflict(shortcut, self.binding(shortcut)).is_some() {
                self.rebind(shortcut, shortcut.default_binding());
            }
        }
    }
}
