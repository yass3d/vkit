//! The thing a binding listens for.

use egui::{Key, PointerButton};

/// A key on the number pad, which egui cannot tell from the top row.
///
/// `egui-winit` folds `KeyCode::Numpad5` into `Key::Num5`, so by the time a
/// press reaches egui the two are the same key. They are not the same key to a
/// reader: the top row picks a tab and the pad picks a view, the way every
/// modelling program has done it since Blender. So the pad is read one layer
/// earlier, off the physical key, and named here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumpadKey {
    Zero,
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Decimal,
}

impl NumpadKey {
    pub const ALL: [Self; 11] = [
        Self::Zero,
        Self::One,
        Self::Two,
        Self::Three,
        Self::Four,
        Self::Five,
        Self::Six,
        Self::Seven,
        Self::Eight,
        Self::Nine,
        Self::Decimal,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Zero => "Num 0",
            Self::One => "Num 1",
            Self::Two => "Num 2",
            Self::Three => "Num 3",
            Self::Four => "Num 4",
            Self::Five => "Num 5",
            Self::Six => "Num 6",
            Self::Seven => "Num 7",
            Self::Eight => "Num 8",
            Self::Nine => "Num 9",
            Self::Decimal => "Num .",
        }
    }

    const fn stored_name(self) -> &'static str {
        match self {
            Self::Zero => "numpad0",
            Self::One => "numpad1",
            Self::Two => "numpad2",
            Self::Three => "numpad3",
            Self::Four => "numpad4",
            Self::Five => "numpad5",
            Self::Six => "numpad6",
            Self::Seven => "numpad7",
            Self::Eight => "numpad8",
            Self::Nine => "numpad9",
            Self::Decimal => "numpaddecimal",
        }
    }

    fn by_stored_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.stored_name() == name)
    }
}

/// A modifier read while something else is already happening.
///
/// Held, not struck. `Shift` while a sculpt stroke is in flight means smooth,
/// and there is no press to catch — the stroke asks, every frame, whether the
/// key is down. These were read straight off `input.modifiers` in nine files,
/// so Settings could not name them and nothing checked one against another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModifierKey {
    Shift,
    Ctrl,
    Alt,
}

impl ModifierKey {
    pub const ALL: [Self; 3] = [Self::Shift, Self::Ctrl, Self::Alt];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Shift => "Shift",
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
        }
    }

    const fn stored_name(self) -> &'static str {
        match self {
            Self::Shift => "hold:shift",
            Self::Ctrl => "hold:ctrl",
            Self::Alt => "hold:alt",
        }
    }

    fn by_stored_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.stored_name() == name)
    }

    /// Whether this modifier is down right now.
    pub const fn held_in(self, modifiers: egui::Modifiers) -> bool {
        match self {
            Self::Shift => modifiers.shift,
            // Either spelling. `command` is what the backend sets on a Mac, and
            // `ctrl` is what a hand-built `Modifiers` carries; reading only one
            // of them makes this answer no on a platform or in a test.
            Self::Ctrl => modifiers.ctrl || modifiers.command,
            Self::Alt => modifiers.alt,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Trigger {
    Key(Key),
    Mouse(PointerButton),
    Numpad(NumpadKey),
    Held(ModifierKey),
}

impl Trigger {
    pub(super) fn stored_name(self) -> String {
        match self {
            Self::Key(key) => key.name().to_owned(),
            Self::Mouse(button) => format!("mouse:{}", mouse_name(button)),
            Self::Numpad(key) => key.stored_name().to_owned(),
            Self::Held(modifier) => modifier.stored_name().to_owned(),
        }
    }

    pub(super) fn parse(text: &str) -> Option<Self> {
        if let Some(button) = text.strip_prefix("mouse:") {
            return mouse_by_name(button).map(Self::Mouse);
        }
        if let Some(key) = NumpadKey::by_stored_name(text) {
            return Some(Self::Numpad(key));
        }
        if let Some(modifier) = ModifierKey::by_stored_name(text) {
            return Some(Self::Held(modifier));
        }
        Key::from_name(text).map(Self::Key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Key(Key::OpenBracket) => "[",
            Self::Key(Key::CloseBracket) => "]",
            Self::Key(Key::Escape) => "Esc",
            Self::Key(key) => key.name(),
            Self::Mouse(button) => mouse_name(button),
            Self::Numpad(key) => key.label(),
            Self::Held(modifier) => modifier.label(),
        }
    }
}

const fn mouse_name(button: PointerButton) -> &'static str {
    match button {
        PointerButton::Primary => "Left click",
        PointerButton::Secondary => "Right click",
        PointerButton::Middle => "Wheel click",
        PointerButton::Extra1 => "Mouse 4",
        PointerButton::Extra2 => "Mouse 5",
    }
}

fn mouse_by_name(name: &str) -> Option<PointerButton> {
    [
        PointerButton::Primary,
        PointerButton::Secondary,
        PointerButton::Middle,
        PointerButton::Extra1,
        PointerButton::Extra2,
    ]
    .into_iter()
    .find(|button| mouse_name(*button) == name)
}
