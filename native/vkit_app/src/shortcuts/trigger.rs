//! The thing a binding listens for.

use egui::{Key, PointerButton};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Trigger {
    Key(Key),
    Mouse(PointerButton),
}

impl Trigger {
    pub(super) fn stored_name(self) -> String {
        match self {
            Self::Key(key) => key.name().to_owned(),
            Self::Mouse(button) => format!("mouse:{}", mouse_name(button)),
        }
    }

    pub(super) fn parse(text: &str) -> Option<Self> {
        text.strip_prefix("mouse:").map_or_else(
            || Key::from_name(text).map(Self::Key),
            |button| mouse_by_name(button).map(Self::Mouse),
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Key(Key::OpenBracket) => "[",
            Self::Key(Key::CloseBracket) => "]",
            Self::Key(Key::Escape) => "Esc",
            Self::Key(key) => key.name(),
            Self::Mouse(button) => mouse_name(button),
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
