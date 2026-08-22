use std::collections::BTreeMap;

use egui::{Key, Modifiers, PointerButton, Ui};

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
pub enum ShortcutContext {
    Global,

    Alignment,

    DetailEdit,

    HairEdit,
}

impl ShortcutContext {
    pub const fn overlaps(self, other: Self) -> bool {
        matches!(self, Self::Global) || matches!(other, Self::Global) || self as u8 == other as u8
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Trigger {
    Key(Key),
    Mouse(PointerButton),
}

impl Trigger {
    fn stored_name(self) -> String {
        match self {
            Self::Key(key) => key.name().to_owned(),
            Self::Mouse(button) => format!("mouse:{}", mouse_name(button)),
        }
    }

    fn parse(text: &str) -> Option<Self> {
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

    fn modifier_name(self) -> Option<&'static str> {
        match self.modifiers {
            ModifierPolicy::Ignored => Some("any"),
            ModifierPolicy::Exactly(modifiers) if modifiers == Modifiers::COMMAND => Some("ctrl"),
            ModifierPolicy::Exactly(modifiers) if modifiers == Modifiers::SHIFT => Some("shift"),
            ModifierPolicy::Exactly(modifiers) if modifiers == Modifiers::ALT => Some("alt"),
            ModifierPolicy::Exactly(modifiers) if modifiers.is_none() => Some("none"),
            ModifierPolicy::Exactly(_) => None,
        }
    }

    fn modifiers_by_name(name: &str) -> Option<ModifierPolicy> {
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

const KEYMAP_ID: &str = "vkit.shortcuts.keymap";

pub fn install(ctx: &egui::Context, keymap: &Keymap) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new(KEYMAP_ID), keymap.clone()));
}

fn current(ui: &Ui) -> Keymap {
    ui.data(|data| data.get_temp::<Keymap>(egui::Id::new(KEYMAP_ID)))
        .unwrap_or_default()
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
    Redo,

    BrushSizeSweep,
    BrushStrengthSweep,

    ViewTrackball,

    ViewLevelRoll,

    CancelStencil,

    FrameSelected,

    XSymmetry,

    ViewOrbit,
    ViewPan,
    ViewDolly,

    HairCombBrush,
    HairPlantTool,
    HairGrowTool,
    HairCutTool,
    HairEraseTool,
    HairMirrorPart,
    HairPuffTool,
    HairPinchTool,
    HairPickTool,
}

impl Shortcut {
    pub const ALL: [Self; 27] = [
        Self::SculptGrabBrush,
        Self::SculptRestoreBrush,
        Self::HairCombBrush,
        Self::HairPlantTool,
        Self::HairGrowTool,
        Self::HairCutTool,
        Self::HairEraseTool,
        Self::HairMirrorPart,
        Self::HairPuffTool,
        Self::HairPinchTool,
        Self::HairPickTool,
        Self::TexturePinBrush,
        Self::TextureCloneBrush,
        Self::BrushSizeDown,
        Self::BrushSizeUp,
        Self::Undo,
        Self::Redo,
        Self::BrushSizeSweep,
        Self::BrushStrengthSweep,
        Self::ViewTrackball,
        Self::ViewLevelRoll,
        Self::CancelStencil,
        Self::FrameSelected,
        Self::XSymmetry,
        Self::ViewOrbit,
        Self::ViewPan,
        Self::ViewDolly,
    ];

    pub const fn slot(self) -> usize {
        let mut index = 0;
        while index < Self::ALL.len() {
            if Self::ALL[index] as usize == self as usize {
                return index;
            }
            index += 1;
        }
        0
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::SculptGrabBrush => "sculpt-grab-brush",
            Self::HairCombBrush => "hair-comb-brush",
            Self::HairPlantTool => "hair-plant-tool",
            Self::HairGrowTool => "hair-grow-tool",
            Self::HairCutTool => "hair-cut-tool",
            Self::HairEraseTool => "hair-erase-tool",
            Self::HairMirrorPart => "hair-mirror-part",
            Self::HairPuffTool => "hair-puff-tool",
            Self::HairPinchTool => "hair-pinch-tool",
            Self::HairPickTool => "hair-pick-tool",
            Self::SculptRestoreBrush => "sculpt-restore-brush",
            Self::TexturePinBrush => "texture-pin-brush",
            Self::TextureCloneBrush => "texture-clone-brush",
            Self::BrushSizeDown => "brush-size-down",
            Self::BrushSizeUp => "brush-size-up",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::BrushSizeSweep => "brush-size-sweep",
            Self::BrushStrengthSweep => "brush-strength-sweep",
            Self::ViewTrackball => "view-trackball",
            Self::ViewLevelRoll => "view-level-roll",
            Self::CancelStencil => "cancel-stencil",
            Self::FrameSelected => "frame-selected",
            Self::XSymmetry => "x-symmetry",
            Self::ViewOrbit => "view-orbit",
            Self::ViewPan => "view-pan",
            Self::ViewDolly => "view-dolly",
        }
    }

    pub fn by_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|shortcut| shortcut.name() == name)
    }

    pub const fn default_binding(self) -> Binding {
        Binding {
            trigger: self.trigger(),
            modifiers: self.modifiers(),
        }
    }

    pub const fn trigger(self) -> Trigger {
        match self {
            Self::ViewOrbit | Self::ViewPan | Self::ViewDolly => {
                Trigger::Mouse(egui::PointerButton::Middle)
            }
            other => Trigger::Key(other.key()),
        }
    }

    pub const fn key(self) -> Key {
        match self {
            Self::SculptGrabBrush => Key::G,
            Self::HairCombBrush => Key::G,
            Self::HairPlantTool => Key::A,
            Self::HairGrowTool => Key::E,
            Self::HairCutTool => Key::X,
            Self::HairEraseTool => Key::T,
            Self::HairMirrorPart => Key::M,
            Self::HairPuffTool => Key::B,
            Self::HairPinchTool => Key::P,
            Self::HairPickTool => Key::V,
            Self::SculptRestoreBrush => Key::H,
            Self::TexturePinBrush => Key::P,
            Self::TextureCloneBrush => Key::C,
            Self::BrushSizeDown => Key::OpenBracket,
            Self::BrushSizeUp => Key::CloseBracket,
            Self::Undo => Key::Z,
            Self::Redo => Key::Y,
            Self::BrushSizeSweep | Self::BrushStrengthSweep => Key::F,
            Self::ViewTrackball | Self::ViewLevelRoll => Key::R,
            Self::CancelStencil => Key::Escape,
            Self::FrameSelected => Key::F,
            Self::XSymmetry => Key::X,
            Self::ViewOrbit | Self::ViewPan | Self::ViewDolly => Key::X,
        }
    }

    pub const fn context(self) -> ShortcutContext {
        match self {
            Self::Undo | Self::Redo => ShortcutContext::Global,

            Self::ViewTrackball | Self::ViewLevelRoll => ShortcutContext::Global,

            Self::ViewOrbit | Self::ViewPan | Self::ViewDolly => ShortcutContext::Global,

            Self::XSymmetry => ShortcutContext::DetailEdit,

            Self::FrameSelected => ShortcutContext::Alignment,
            Self::HairCombBrush
            | Self::HairPlantTool
            | Self::HairGrowTool
            | Self::HairCutTool
            | Self::HairEraseTool
            | Self::HairMirrorPart
            | Self::HairPuffTool
            | Self::HairPinchTool
            | Self::HairPickTool => ShortcutContext::HairEdit,
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
            Self::Undo | Self::Redo => ModifierPolicy::Exactly(Modifiers::COMMAND),

            Self::TexturePinBrush | Self::TextureCloneBrush => {
                ModifierPolicy::Exactly(Modifiers::NONE)
            }

            Self::SculptGrabBrush
            | Self::SculptRestoreBrush
            | Self::HairCombBrush
            | Self::BrushSizeDown
            | Self::BrushSizeUp => ModifierPolicy::Ignored,

            Self::HairPlantTool
            | Self::HairGrowTool
            | Self::HairCutTool
            | Self::HairEraseTool
            | Self::HairMirrorPart
            | Self::HairPuffTool
            | Self::HairPinchTool
            | Self::HairPickTool => ModifierPolicy::Exactly(Modifiers::NONE),

            Self::BrushStrengthSweep => ModifierPolicy::Exactly(Modifiers::SHIFT),

            Self::ViewLevelRoll => ModifierPolicy::Exactly(Modifiers::ALT),

            Self::ViewPan => ModifierPolicy::Exactly(Modifiers::SHIFT),
            Self::ViewDolly => ModifierPolicy::Exactly(Modifiers::COMMAND),

            Self::BrushSizeSweep
            | Self::ViewTrackball
            | Self::CancelStencil
            | Self::FrameSelected
            | Self::XSymmetry
            | Self::ViewOrbit => ModifierPolicy::Exactly(Modifiers::NONE),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::SculptGrabBrush => "G",
            Self::HairCombBrush => "G",
            Self::HairPlantTool => "A",
            Self::HairGrowTool => "E",
            Self::HairCutTool => "X",
            Self::HairEraseTool => "T",
            Self::HairMirrorPart => "M",
            Self::HairPuffTool => "B",
            Self::HairPinchTool => "P",
            Self::HairPickTool => "V",
            Self::SculptRestoreBrush => "H",
            Self::TexturePinBrush => "P",
            Self::TextureCloneBrush => "C",
            Self::BrushSizeDown => "[",
            Self::BrushSizeUp => "]",
            Self::Undo => "Ctrl+Z",
            Self::Redo => "Ctrl+Y",
            Self::BrushSizeSweep => "F",
            Self::BrushStrengthSweep => "Shift+F",
            Self::ViewTrackball => "R",
            Self::ViewLevelRoll => "Alt+R",
            Self::CancelStencil => "Esc",
            Self::FrameSelected => "F",
            Self::XSymmetry => "X",
            Self::ViewOrbit => "Wheel click",
            Self::ViewPan => "Shift+Wheel click",
            Self::ViewDolly => "Ctrl+Wheel click",
        }
    }

    pub fn binding(self, ui: &Ui) -> Binding {
        current(ui).binding(self)
    }

    pub fn label_now(self, ui: &Ui) -> String {
        self.binding(ui).label()
    }

    pub fn pressed(self, ui: &Ui) -> bool {
        if ui.ctx().egui_wants_keyboard_input() {
            return false;
        }
        let binding = self.binding(ui);
        ui.input(|input| {
            binding.modifiers.admits(input.modifiers)
                && match binding.trigger {
                    Trigger::Key(key) => input.events.iter().any(|event| {
                        matches!(
                            event,
                            egui::Event::Key {
                                key: struck,
                                pressed: true,
                                repeat: false,
                                ..
                            } if *struck == key
                        )
                    }),
                    Trigger::Mouse(button) => input.pointer.button_pressed(button),
                }
        })
    }

    #[must_use]
    pub fn released(self, ui: &Ui) -> bool {
        if ui.ctx().egui_wants_keyboard_input() {
            return false;
        }
        let binding = self.binding(ui);
        ui.input(|input| match binding.trigger {
            Trigger::Key(key) => input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: struck,
                        pressed: false,
                        ..
                    } if *struck == key
                )
            }),
            Trigger::Mouse(button) => input.pointer.button_released(button),
        })
    }
}

pub const BRUSH_SIZE_HINT: &str = "[ / ] / F";

pub const BRUSH_STRENGTH_HINT: &str = "Shift+F";

#[cfg(test)]
mod tests {
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
