use egui::{Key, PointerButton};

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
    Divide,
    Multiply,
    Subtract,
    Add,
}

impl NumpadKey {
    pub const ALL: [Self; 15] = [
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
        Self::Divide,
        Self::Multiply,
        Self::Subtract,
        Self::Add,
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
            Self::Divide => "Num /",
            Self::Multiply => "Num *",
            Self::Subtract => "Num -",
            Self::Add => "Num +",
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
            Self::Divide => "numpaddivide",
            Self::Multiply => "numpadmultiply",
            Self::Subtract => "numpadsubtract",
            Self::Add => "numpadadd",
        }
    }

    fn by_stored_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.stored_name() == name)
    }
}

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

    pub const fn held_in(self, modifiers: egui::Modifiers) -> bool {
        match self {
            Self::Shift => modifiers.shift,
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
