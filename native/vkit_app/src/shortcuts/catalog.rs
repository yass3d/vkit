//! Every shortcut this program has, and what each is bound to out of the box.
//!
//! One entry per thing a key can do. Nothing outside this file may read a key
//! for itself: an input the catalog does not name cannot be shown in Settings,
//! cannot be rebound, and cannot be checked for a collision with anything else.

use egui::{Key, Modifiers, Ui};

use super::{Binding, ModifierPolicy, Trigger, current};

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
