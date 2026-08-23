//! Every shortcut this program has, and what each is bound to out of the box.
//!
//! One entry per thing a key can do. Nothing outside this file may read a key
//! for itself: an input the catalog does not name cannot be shown in Settings,
//! cannot be rebound, and cannot be checked for a collision with anything else.

use egui::{Key, Modifiers, Ui};

use super::{Binding, ModifierKey, ModifierPolicy, NumpadKey, Trigger, current};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortcutContext {
    Global,

    Alignment,

    DetailEdit,

    HairEdit,

    /// Standing the camera in a named place. Live wherever the pointer is.
    View,

    /// Moving between the top tabs. Live wherever the pointer is.
    Navigation,

    /// The texture canvas and the tools that paint on it.
    ///
    /// Its own context because `Alt` reverses the texture tool there and
    /// reverses the sculpt brush on the model, and the two are never both under
    /// the pointer.
    TextureEdit,

    /// The part, layer and morph lists down the side.
    ///
    /// Its own context so that `Shift` can mean "add to the selection" here and
    /// "smooth" on the surface without the two being called a collision. They
    /// are never both reachable by one press: the pointer is over a list or it
    /// is over the model.
    Lists,
}

impl ShortcutContext {
    /// Whether a shortcut in this context can fire anywhere.
    ///
    /// Separate from how the list is grouped on purpose. A view key and a tab
    /// key are as reachable as `Global` and must be checked for collisions
    /// against everything, but a reader looking for "the key that shows the
    /// left side" wants them under their own heading and not in one wall.
    pub const fn is_everywhere(self) -> bool {
        matches!(self, Self::Global | Self::View | Self::Navigation)
    }

    pub const fn overlaps(self, other: Self) -> bool {
        self.is_everywhere() || other.is_everywhere() || self as u8 == other as u8
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

    ViewReset,
    ViewToggleProjection,
    ViewFront,
    ViewLeftSide,
    ViewRightSide,
    ViewTop,
    ViewBottom,
    ViewFrontUpperLeft,
    ViewFrontUpperRight,
    ViewFrontLowerLeft,
    ViewFrontLowerRight,

    TabFaceMatch,
    TabDetail,
    TabTexture,
    TabHair,
    TabSave,

    SculptSmoothHold,
    SculptInflateHold,
    SculptAlternateHold,

    HairSmoothHold,
    HairInvertHold,

    TextureInvertHold,

    ListAddToSelectionHold,
    ListSoloHold,
}

impl Shortcut {
    pub const ALL: [Self; 51] = [
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
        Self::ViewReset,
        Self::ViewToggleProjection,
        Self::ViewFront,
        Self::ViewLeftSide,
        Self::ViewRightSide,
        Self::ViewTop,
        Self::ViewBottom,
        Self::ViewFrontUpperLeft,
        Self::ViewFrontUpperRight,
        Self::ViewFrontLowerLeft,
        Self::ViewFrontLowerRight,
        Self::TabFaceMatch,
        Self::TabDetail,
        Self::TabTexture,
        Self::TabHair,
        Self::TabSave,
        Self::SculptSmoothHold,
        Self::SculptInflateHold,
        Self::SculptAlternateHold,
        Self::HairSmoothHold,
        Self::HairInvertHold,
        Self::TextureInvertHold,
        Self::ListAddToSelectionHold,
        Self::ListSoloHold,
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
            Self::ViewReset => "view-reset",
            Self::ViewToggleProjection => "view-toggle-projection",
            Self::ViewFront => "view-front",
            Self::ViewLeftSide => "view-left-side",
            Self::ViewRightSide => "view-right-side",
            Self::ViewTop => "view-top",
            Self::ViewBottom => "view-bottom",
            Self::ViewFrontUpperLeft => "view-front-upper-left",
            Self::ViewFrontUpperRight => "view-front-upper-right",
            Self::ViewFrontLowerLeft => "view-front-lower-left",
            Self::ViewFrontLowerRight => "view-front-lower-right",
            Self::TabFaceMatch => "tab-face-match",
            Self::TabDetail => "tab-detail",
            Self::TabTexture => "tab-texture",
            Self::TabHair => "tab-hair",
            Self::TabSave => "tab-save",
            Self::SculptSmoothHold => "sculpt-smooth-hold",
            Self::SculptInflateHold => "sculpt-inflate-hold",
            Self::SculptAlternateHold => "sculpt-alternate-hold",
            Self::HairSmoothHold => "hair-smooth-hold",
            Self::HairInvertHold => "hair-invert-hold",
            Self::TextureInvertHold => "texture-invert-hold",
            Self::ListAddToSelectionHold => "list-add-to-selection-hold",
            Self::ListSoloHold => "list-solo-hold",
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
            // Blender's pad, which every modelling program copied.
            Self::ViewReset => Trigger::Numpad(NumpadKey::Zero),
            Self::ViewToggleProjection => Trigger::Numpad(NumpadKey::Decimal),
            Self::ViewFront => Trigger::Numpad(NumpadKey::Five),
            Self::ViewLeftSide => Trigger::Numpad(NumpadKey::Four),
            Self::ViewRightSide => Trigger::Numpad(NumpadKey::Six),
            Self::ViewTop => Trigger::Numpad(NumpadKey::Eight),
            Self::ViewBottom => Trigger::Numpad(NumpadKey::Two),
            Self::ViewFrontUpperLeft => Trigger::Numpad(NumpadKey::Seven),
            Self::ViewFrontUpperRight => Trigger::Numpad(NumpadKey::Nine),
            Self::ViewFrontLowerLeft => Trigger::Numpad(NumpadKey::One),
            Self::ViewFrontLowerRight => Trigger::Numpad(NumpadKey::Three),

            Self::SculptSmoothHold | Self::HairSmoothHold | Self::ListAddToSelectionHold => {
                Trigger::Held(ModifierKey::Shift)
            }
            Self::SculptInflateHold => Trigger::Held(ModifierKey::Ctrl),
            Self::SculptAlternateHold
            | Self::HairInvertHold
            | Self::TextureInvertHold
            | Self::ListSoloHold => Trigger::Held(ModifierKey::Alt),

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
            Self::TabFaceMatch => Key::Num1,
            Self::TabDetail => Key::Num2,
            Self::TabTexture => Key::Num3,
            Self::TabHair => Key::Num4,
            Self::TabSave => Key::Num5,
            // The pad keys answer through `trigger`; this arm is unreachable for
            // them and returns a key nothing is bound to rather than panicking.
            Self::ViewReset
            | Self::ViewToggleProjection
            | Self::ViewFront
            | Self::ViewLeftSide
            | Self::ViewRightSide
            | Self::ViewTop
            | Self::ViewBottom
            | Self::ViewFrontUpperLeft
            | Self::ViewFrontUpperRight
            | Self::ViewFrontLowerLeft
            | Self::ViewFrontLowerRight
            | Self::SculptSmoothHold
            | Self::SculptInflateHold
            | Self::SculptAlternateHold
            | Self::HairSmoothHold
            | Self::HairInvertHold
            | Self::TextureInvertHold
            | Self::ListAddToSelectionHold
            | Self::ListSoloHold => Key::Escape,
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
            Self::TexturePinBrush | Self::TextureCloneBrush => ShortcutContext::TextureEdit,

            Self::SculptGrabBrush
            | Self::SculptRestoreBrush
            | Self::BrushSizeDown
            | Self::BrushSizeUp
            | Self::BrushSizeSweep
            | Self::BrushStrengthSweep
            | Self::CancelStencil => ShortcutContext::DetailEdit,

            Self::ViewReset
            | Self::ViewToggleProjection
            | Self::ViewFront
            | Self::ViewLeftSide
            | Self::ViewRightSide
            | Self::ViewTop
            | Self::ViewBottom
            | Self::ViewFrontUpperLeft
            | Self::ViewFrontUpperRight
            | Self::ViewFrontLowerLeft
            | Self::ViewFrontLowerRight => ShortcutContext::View,

            Self::TabFaceMatch
            | Self::TabDetail
            | Self::TabTexture
            | Self::TabHair
            | Self::TabSave => ShortcutContext::Navigation,

            Self::SculptSmoothHold | Self::SculptInflateHold | Self::SculptAlternateHold => {
                ShortcutContext::DetailEdit
            }

            Self::TextureInvertHold => ShortcutContext::TextureEdit,

            Self::HairSmoothHold | Self::HairInvertHold => ShortcutContext::HairEdit,

            Self::ListAddToSelectionHold | Self::ListSoloHold => ShortcutContext::Lists,
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

            Self::ViewReset
            | Self::ViewToggleProjection
            | Self::ViewFront
            | Self::ViewLeftSide
            | Self::ViewRightSide
            | Self::ViewTop
            | Self::ViewBottom
            | Self::ViewFrontUpperLeft
            | Self::ViewFrontUpperRight
            | Self::ViewFrontLowerLeft
            | Self::ViewFrontLowerRight
            | Self::TabFaceMatch
            | Self::TabDetail
            | Self::TabTexture
            | Self::TabHair
            | Self::TabSave => ModifierPolicy::Exactly(Modifiers::NONE),

            // A held modifier IS the binding. Asking which others are down
            // besides it would make `Shift` stop smoothing the moment the
            // reader also reached for `Alt`.
            Self::SculptSmoothHold
            | Self::SculptInflateHold
            | Self::SculptAlternateHold
            | Self::HairSmoothHold
            | Self::HairInvertHold
            | Self::TextureInvertHold
            | Self::ListAddToSelectionHold
            | Self::ListSoloHold => ModifierPolicy::Ignored,
        }
    }

    /// What this shortcut is bound to out of the box, spelled for a reader.
    ///
    /// Derived, never listed. A second table of "G", "A", "[" beside the table
    /// that produces them drifts the moment one is edited, and this one had a
    /// test pinning the two to each other, which is a tautology and not a check.
    ///
    /// This is the FACTORY binding. Anywhere the reader is being told which key
    /// to press right now, use `label_now`, which reads the keymap in force.
    pub fn label(self) -> String {
        self.default_binding().label()
    }

    pub fn binding(self, ui: &Ui) -> Binding {
        current(ui).binding(self)
    }

    pub fn label_now(self, ui: &Ui) -> String {
        self.binding(ui).label()
    }

    /// Whether this gesture's modifier is down right now.
    ///
    /// Answers `false` for anything not bound to a held modifier, so a reader
    /// who moves a gesture onto a letter key does not get a stuck stroke.
    #[must_use]
    pub fn held(self, ui: &Ui) -> bool {
        self.held_in(ui, ui.input(|input| input.modifiers))
    }

    /// The same question against modifiers already in hand.
    ///
    /// The sculpt and hair strokes read `input.modifiers` once and pass it down
    /// through several decisions; making each of them re-enter `ui.input`
    /// would let the answer change halfway through one stroke.
    #[must_use]
    pub fn held_in(self, ui: &Ui, modifiers: egui::Modifiers) -> bool {
        match self.binding(ui).trigger {
            Trigger::Held(modifier) => modifier.held_in(modifiers),
            _ => false,
        }
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
                    // The pad never reaches egui as itself. `runtime.rs` reads
                    // the physical key and dispatches through the same keymap.
                    // Neither is a press. The pad is read a layer earlier in
                    // `runtime.rs`; a held modifier is asked about by `held`.
                    Trigger::Numpad(_) | Trigger::Held(_) => false,
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
            Trigger::Numpad(_) | Trigger::Held(_) => false,
        })
    }
}
