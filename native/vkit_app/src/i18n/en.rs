use super::TextKey;

pub(super) const fn label(key: TextKey) -> &'static str {
    match key {
        TextKey::SurfaceSmooth => "Mesh Smooth",
        TextKey::SurfaceSmoothTooltip => {
            "VaM-compatible render smoothing. 0 is the editable base mesh, 3 is the VaM default, and 4 is the maximum. Morphs, UVs, sculpting, and texture baking keep the base topology"
        }
        TextKey::FaceMatch => "Create",
        TextKey::ResultPreview | TextKey::Save => "Save",
        TextKey::DetailCorrection => "Sculpt",
        TextKey::TextureStage => "Texture",
        TextKey::HairStage => "Hair",
        TextKey::HairUnavailable => "Finish the sculpt result before editing hair",
        TextKey::HairPartsPanel => "Hair Parts",
        TextKey::AddHairPart => "Create Part",
        TextKey::HairScalpMeshCrowded => {
            "Some strands had nowhere to sit on the new scalp mesh and were dropped"
        }
        TextKey::HairScalpMissing => {
            "Scalp provider mesh is not available; open a VaM root and rescan"
        }
        TextKey::HairShowPoints => "Show points",
        TextKey::HairShowPointsHint => {
            "Shows the scalp planting points on the cap. Turn it off to look at the hair itself; brushes keep working either way."
        }
        TextKey::HairPartTint => "Tint parts",
        TextKey::HairPartTintHint => {
            "Gives each part a distinct viewport tint so a combined style reads as its parts. Display only -- exports are untouched."
        }
        TextKey::HairSettingsPanel => "Hair Settings",
        TextKey::HairCreateFirst => "Create some hair first.",
        TextKey::HairHideStrands => "Hide hair",
        TextKey::HairHideStrandsHint => {
            "Takes the grown hair off so the streams can be read on their own. Turning this on turns the streams on."
        }
        TextKey::HairShowStreams => "Show hair streams",
        TextKey::HairShowStreamsHint => {
            "Draws the strand you actually edit, over the hair it grows into."
        }
        TextKey::HairViewportPhysics => "Viewport physics",
        TextKey::HairViewportPhysicsHint => {
            "Previews the physics on screen. Nothing to do with the export."
        }
        TextKey::HairMirrorEdit => "Mirror Edit",
        TextKey::HairMirrorEditHint => {
            "Brushes edit both sides of the head at once, left-right symmetric."
        }
        TextKey::HairAutoPart => "Auto Part",
        TextKey::HairAutoPartHint => {
            "The brush edits only the part it recognizes under the cursor, instead of the active layers."
        }
        TextKey::HairExportSection => "Save hair",
        TextKey::HairExportBothSexes => "Both",
        TextKey::HairExportRescanNotice => {
            "Click VaM - Hair - Rescan Files to see it in the hair list"
        }
        TextKey::HairExportOverwroteNotice => {
            "An overwritten thumbnail shows after Hard Reset or a VaM restart"
        }
        TextKey::HairSimToggleHint => {
            "Runs the hair solver here and in the game -- gravity, rigidity, cling."
        }
        TextKey::HairCollisionToggleHint => {
            "Keeps strands out of the head and body while simulating."
        }
        TextKey::HairStyleJoints => "Style joints",
        TextKey::HairStyleJointsHint => {
            "Ties nearby strands together so the style holds its shape in the game -- what Cling scales. For long or tied hair; adds file weight, so short styles leave it off."
        }
        TextKey::HairThumbnailPrompt => "Frame your angle, then click to capture",
        TextKey::HairThumbnailShoot => "Capture",
        TextKey::HairThumbnailSkip => "Skip",
        TextKey::HairThumbnailSaved => "Thumbnail saved",
        TextKey::HairThumbnailFailed => "Thumbnail failed",
        TextKey::HairGameOnly => {
            "Physics setting: the viewport does not simulate, so it takes effect in VaM only.              Exported with the style."
        }
        TextKey::HairPartVisible => "Show or hide this hair part",
        TextKey::RemoveHairPart => "Remove hair part",
        TextKey::HairRenamePart => "Click to rename",
        TextKey::HairToolPlant => "Plant",
        TextKey::HairToolErase => "Erase",
        TextKey::HairToolGrow => "Grow (Alt shortens)",
        TextKey::HairToolComb => "Comb",
        TextKey::HairToolPlantHint => "Grow strands on the scalp vertices under the brush",
        TextKey::HairToolGrowHint => "Lengthen the strands under the brush; hold Alt to shorten",
        TextKey::HairToolEraseHint => "Remove the strands under the brush",
        TextKey::HairToolCombHint => {
            "Drag the strands under the brush into shape; the roots stay put"
        }
        TextKey::HairToolPinch => "Pinch",
        TextKey::HairToolPinchHint => "Gather the strands under the brush. Alt spreads them apart",
        TextKey::HairToolCut => "Cut",
        TextKey::HairToolCutHint => "End the strands under the brush where you draw across them",
        TextKey::HairToolPuff => "Puff",
        TextKey::HairToolPuffHint => {
            "Stand the strands under the brush up off the skin. Alt lays them down"
        }
        TextKey::HairToolVertex => "Point",
        TextKey::HairToolVertexHint => {
            "Take hold of one joint on a strand and move it; the length is kept"
        }
        TextKey::HairCopySettings => "Copy All",
        TextKey::HairPasteSettings => "Paste All",
        TextKey::HairGroupPerformance => "Performance",
        TextKey::HairGroupPhysics => "Physics",
        TextKey::HairGroupStiffness => "Stiffness",
        TextKey::HairGroupShape => "Shape",
        TextKey::HairGroupCurl => "Curl",
        TextKey::HairGroupLook => "Look",
        TextKey::HairColorScalp => "Scalp",
        TextKey::HairColorRoot => "Root",
        TextKey::HairColorTip => "Tip",
        TextKey::HairColorSpecular => "Shine",
        TextKey::HairScalpMask => "Scalp Texture",
        TextKey::HairScalpMaskBuiltIn => "Built-in",
        TextKey::HairScalpMaskCustom => "From file...",
        TextKey::HairExport => "Export to VaM",
        TextKey::HairExportName => "Item name",
        TextKey::HairExportCreator => "Creator",
        TextKey::HairExportNeedsMetadata => "Name the item and its creator before exporting",
        TextKey::HairOverwriteTitle => "Replace this style?",
        TextKey::HairOverwriteBody => {
            "A style with this name and creator is already installed. Exporting removes the old install and writes anew. The thumbnail updates on Hard Reset or a VaM restart."
        }
        TextKey::HairOverwriteProceed => "Replace",
        TextKey::HairExportDone => "Hair item saved",
        TextKey::HairExportFailed => "Hair export failed",
        TextKey::HairExportNeedsPart => "Select a hair part with strands first",
        TextKey::HairExportNeedsVaMRoot => "Set a VaM root before exporting",
        TextKey::HairMirrorPart => "Mirror part to the other side",
        TextKey::HairDuplicatePart => "Duplicate part",
        TextKey::HairSegmentsShort => "Seg",
        TextKey::MorphCategoryBody => "Body",
        TextKey::TextureNamePlaceholder => "Enter a texture name",
        TextKey::TextureOverwriteTitle => "This replaces textures of the same name",
        TextKey::OpenScan => "Scan Head",
        TextKey::SourceMorphMissing => "None of this look's morphs are installed",
        TextKey::EditDetails => "Edit Details",
        TextKey::Female => "Female",
        TextKey::Male => "Male",
        TextKey::Lighting => "Lighting",
        TextKey::Brightness => "Brightness",
        TextKey::LightRotation => "Light Rotation",
        TextKey::AddHeadFileHint => "or drag an OBJ, GLB or FBX file here",
        TextKey::AddHeadFile => "Add a head file",
        TextKey::AutoAlign => "Auto Match",
        TextKey::PlacementReset => "Placement Reset",
        TextKey::Scale => "Scale",
        TextKey::Position => "Position",
        TextKey::Rotation => "Rotation",
        TextKey::XMirror => "X Mirror",
        TextKey::XMirrorTooltip => "Placing a pin also places its twin on the other side",
        TextKey::NumbersTooltip => "Number each pin in the order it was placed",
        TextKey::XMirrorRejected => "X Mirror was disabled because no safe counterpart was found",
        TextKey::ScanFidelity => "Scan fidelity",
        TextKey::ScanOptions => "Scan options",
        TextKey::RestoreNeckEars => "Restore neck & ears",
        TextKey::RestoreNeckEarsTooltip => {
            "Keep the ears, nape and back of the skull on the G2 base, blending the fit in only toward the face"
        }
        TextKey::RestoreNeckEarsDeferred => {
            "Sculpt strokes exist, so the change applies on the next generation"
        }
        TextKey::ScanFidelityTooltip => {
            "Above 1 the fit follows the scan's form more and keeps less of the figure's smoothness. Up to 2 that is all it does; past 2 the scan's noise -- speckle, holes, stitching seams -- starts coming through as bumps too, which a clean dense scan can be worth pushing into. Pins are never traded away at any setting, and no guard is relaxed."
        }
        TextKey::ScanSymmetry => "Scan Head Symmetry",
        TextKey::Original => "Original",
        TextKey::Left => "Left",
        TextKey::Right => "Right",
        TextKey::EyesOpen => "Open Eyes",
        TextKey::EyesClosed => "Closed Eyes",
        TextKey::View => "View Options",
        TextKey::On => "On",
        TextKey::Numbers => "Numbers",
        TextKey::PinPairsPlaced => "Pins",
        TextKey::PinPlacement => "Pin placement",
        TextKey::ScanReference => "Scan reference",
        TextKey::AddHeadFileAction => "Add a head file",
        TextKey::AddHeadFileActionTooltip => "Load a scanned head file (OBJ · GLB · FBX)",
        TextKey::ContinueStep => "Start from the base head",
        TextKey::ContinueStepTooltip => {
            "Edit the G2 head as it is, with no scan; a scan can still be loaded and fitted later"
        }
        TextKey::ResetAllPins => "Reset All Pins",
        TextKey::ResetAll => "Reset all",
        TextKey::MorphSave => "Save morph",
        TextKey::TextureSaveSection => "Save textures",
        TextKey::Generate => "Fit Face",
        TextKey::Cancel => "Cancel",
        TextKey::CopyAll => "Copy All",
        TextKey::Next => "Save",
        TextKey::TextureSkin => "Preset",
        TextKey::SolidColor => "Solid",
        TextKey::BackfaceProtectionTooltip => "Protects the backs of thin meshes",
        TextKey::WireframeColor => "Wireframe Color",
        TextKey::WireframeSettings => "Wireframe",
        TextKey::XraySettings => "X-ray",
        TextKey::ViewportLightingTooltip => "Adjust lighting",
        TextKey::Background => "Background",
        TextKey::ViewportBackgroundTooltip => "Adjust the background and reference image",
        TextKey::BackgroundRadial => "Radial",
        TextKey::BackgroundVertical => "Vertical",
        TextKey::BackgroundFlat => "Flat",
        TextKey::ViewportWireframeTooltip => "Show and adjust the Wireframe overlay",
        TextKey::ViewportXrayTooltip => "Show and adjust the X-ray overlay",
        TextKey::ViewportSkinTooltip => "Choose a VaM skin texture",
        TextKey::FalloffTooltip => "Choose the brush influence curve",
        TextKey::FalloffSmooth => "Smooth",
        TextKey::FalloffSmoother => "Smoother",
        TextKey::FalloffSharp => "Sharp",
        TextKey::FalloffLinear => "Linear",
        TextKey::PlaceHead => "Place the Head",
        TextKey::PlaceHeadTooltip => {
            "Overlays the loaded head on G2 to line up its position, rotation and size."
        }
        TextKey::ScanOverlay => "Scan Overlay",
        TextKey::OverlayDone => "Done Overlay",
        TextKey::ProjectImage => "Project",
        TextKey::ProjectionCanvasHint => "Paint in the 3D view and it lands on this UV canvas",
        TextKey::HelpProjectionPaint => "Paint (project)",
        TextKey::ShortcutProjectionPaint => "Left-drag",
        TextKey::HelpProjectionMove => "Move image",
        TextKey::ShortcutProjectionMove => "Right-drag",
        TextKey::HelpProjectionRotate => "Rotate image",
        TextKey::ShortcutProjectionRotate => "Shift+right-drag",
        TextKey::HelpProjectionZoom => "Scale image",
        TextKey::ShortcutProjectionZoom => "Wheel",
        TextKey::HelpBrushSize => "Brush size",
        TextKey::ShortcutBrushSize => "F drag · [ ]",
        TextKey::HelpBrushStrength => "Brush strength",
        TextKey::ShortcutBrushStrength => "Shift+F drag",
        TextKey::HelpProjectionDone => "Finish projecting",
        TextKey::ShortcutProjectionDone => "Done · Esc",
        TextKey::ProjectDone => "Done",
        TextKey::ProjectDoneTooltip => "Finish projecting; everything painted stays on the layer",
        TextKey::MorphCompare => "Compare",
        TextKey::Opacity => "Opacity",
        TextKey::G2Head => "G2 Head",
        TextKey::Eyelashes => "Eyelashes",
        TextKey::OverwriteTitle => "Overwrite File",
        TextKey::OverwriteBody => "A file with the same name already exists. Overwrite it?",
        TextKey::Overwrite => "Overwrite",
        TextKey::ScanTextureLayer => "Scan texture",
        TextKey::PinPrompt => "Place a pin on each, or fit without them",
        TextKey::TextureNeedsImage => "Add an image from the side panel",
        TextKey::TexturePinPairPrompt => "Place matching pins on the left and the right",
        TextKey::TextureToolNeedsPins => "Place the pins first to use this",
        TextKey::SymmetrySuggestion => "Mirroring is recommended for a cleaner match",
        TextKey::SymmetryChangeTitle => "The pins will be reset",
        TextKey::SymmetryChangeConfirm => "Confirm",
        TextKey::EyeClosure => "Close Eyes",
        TextKey::VaMFolder => "VaM Folder",
        #[cfg(test)]
        TextKey::VaMGeometryBaseReady => "VaM geometry base verified",
        TextKey::VaMGeometryBaseRejected => "VaM geometry base cannot be used",
        TextKey::VaMGeometryBaseHowTo => {
            "Select your VaM folder; the neutral G2 base is extracted from your install automatically"
        }
        TextKey::VaMBaseSourceUnityBundle => "VaM built-in neutral base (auto-extracted)",
        TextKey::VaMBaseSourceDazAnchorPair => "DAZ anchor + verified OBJ base",
        TextKey::VaMBaseSourceUnityAnchorPair => "Extracted neutral anchor + verified OBJ base",
        TextKey::VaMBaseSourceUnverifiedObj => "Unverified OBJ base",
        TextKey::VaMUvMappingUnavailable => {
            "The VaM skin UV mapping is not ready. Check your VaM folder and refresh the skin catalog"
        }
        TextKey::VaMIndexing => "Indexing VaM catalog",
        TextKey::VaMFailed => "VaM catalog failed",
        TextKey::RefreshSkins => "Refresh skin library",
        TextKey::MorphLoading => "Loading morph",
        TextKey::MorphLoadFailed => "Morph load failed",
        TextKey::DefaultSkinMissing => "Could not find that skin. Check the VaM folder",
        TextKey::DefaultSkinSet => "Open with this skin",
        TextKey::DefaultSkinClear => "Stop opening with this skin",
        TextKey::SkinNone => "None",
        TextKey::SkinSearch => "Search Skins",
        TextKey::SkinLoading => "Loading skin",
        TextKey::SkinReady => "Skin preview ready",
        TextKey::SkinLoadFailed => "Skin load failed",
        TextKey::SkinUvUnavailable => "Skin UV unavailable",
        TextKey::NoMatchingSkins => "No matching skins",
        TextKey::HairSearch => "Search Hair",
        TextKey::HairReady => "Hair preview ready",
        TextKey::HairLoadFailed => "Hair load failed",
        TextKey::HairPartsSkipped => "Showing without some parts",
        TextKey::HairParts => "parts",
        TextKey::BaseFace => "Base face",
        TextKey::Settings => "Settings",
        TextKey::SettingsGraphics => "Graphics",
        TextKey::SettingsViewport => "Viewport",
        TextKey::SettingsGeneral => "General",
        TextKey::SettingsAbout => "About",
        TextKey::SettingsShortcuts => "Shortcuts",
        TextKey::ShortcutsCapturing => "Press…",
        TextKey::ShortcutsResetAll => "Reset every shortcut",
        TextKey::ShortcutGroupSystem => "Everywhere",
        TextKey::ShortcutGroupAlignment => "Placing the head",
        TextKey::ShortcutGroupSculpt => "Sculpt brushes",
        TextKey::ShortcutGroupHair => "Hair brushes",
        TextKey::ShortcutGroupView => "Camera views",
        TextKey::ShortcutGroupNavigation => "Switching tabs",
        TextKey::ShortcutViewReset => "Reset camera",
        TextKey::ShortcutViewProjection => "Perspective / orthographic",
        TextKey::ShortcutViewFront => "Front view",
        TextKey::ShortcutViewLeftSide => "Left view",
        TextKey::ShortcutViewRightSide => "Right view",
        TextKey::ShortcutViewTop => "Top view",
        TextKey::ShortcutViewBottom => "Bottom view",
        TextKey::ShortcutViewFrontUpperLeft => "Front upper left",
        TextKey::ShortcutViewFrontUpperRight => "Front upper right",
        TextKey::ShortcutViewFrontLowerLeft => "Front lower left",
        TextKey::ShortcutViewFrontLowerRight => "Front lower right",
        TextKey::ShortcutTabFaceMatch => "Face match tab",
        TextKey::ShortcutTabDetail => "Detail tab",
        TextKey::ShortcutTabTexture => "Texture tab",
        TextKey::ShortcutTabHair => "Hair tab",
        TextKey::ShortcutTabSave => "Save tab",
        TextKey::ShortcutGroupLists => "Layers and parts",
        TextKey::ShortcutGroupTexture => "Texture canvas",
        TextKey::ReferenceImages => "Reference",
        TextKey::ReferenceImagesEmpty => "No reference pictures",
        TextKey::ReferenceImageAdd => "Add a picture",
        TextKey::ReferenceImageRemove => "Remove",
        TextKey::ReferenceImageRaise => "Bring forward",
        TextKey::ReferenceImageLower => "Send back",
        TextKey::ShortcutTextureCanvasPan => "Pan the canvas (hold)",
        TextKey::ShortcutDiagnosticLogCopy => "Copy the log",
        TextKey::ShortcutLightRotate => "Turn the light (hold)",
        TextKey::ShortcutLayerHide => "Hide layer",
        TextKey::ShortcutLayerUnhideAll => "Unhide every layer",
        TextKey::ShortcutLayerIsolate => "Hide the other layers",
        TextKey::ShortcutLayerInvertSelection => "Invert the selection",
        TextKey::ShortcutSculptSmoothHold => "Hold to smooth",
        TextKey::ShortcutSculptInflateHold => "Hold to inflate",
        TextKey::ShortcutSculptAlternateHold => "Hold for the other way round",
        TextKey::ShortcutHairSmoothHold => "Hold to smooth hair",
        TextKey::ShortcutHairInvertHold => "Hold to reverse the hair tool",
        TextKey::ShortcutTextureInvertHold => "Hold to reverse the texture tool",
        TextKey::ShortcutListAddToSelection => "Hold to add to the selection",
        TextKey::ShortcutListSolo => "Hold to show one on its own",
        TextKey::ShortcutsExport => "Save keymap to a file",
        TextKey::ShortcutsImport => "Load a keymap file",
        TextKey::ShortcutsTaken => "Another action already answers to that",
        TextKey::SettingsAboutBuild => "Build",
        TextKey::SettingsAboutLicense => "License",
        TextKey::SettingsAboutDiagnostics => "Diagnostics",
        TextKey::SettingsTooltipsTooltip => "Help like this, which is what just happened",
        TextKey::AboutTagline => "A companion tool for making VaM head morphs.",
        TextKey::AboutNoAffiliation => {
            "Virt-A-Mate is the property of MeshedVR; Genesis 2 is the property of DAZ 3D. Vkit is an independent tool, unaffiliated with either."
        }
        TextKey::AboutNoWarranty => {
            "Provided as is, without warranty. What you make with it, and the licence of anything you load, are yours to answer for."
        }
        TextKey::AboutSupport => "Support",
        TextKey::AboutReportProblem => {
            "Vkit writes a log every run, and a file of its own if it falls over. Open the folder, attach both to a report, and a fault nobody else can see becomes one somebody can fix."
        }
        TextKey::AboutOpenLogs => "Open the log folder",
        TextKey::AboutReportIssue => "Report a problem",
        TextKey::SettingsLightingPreset => "Preset",
        TextKey::SettingsExposure => "Exposure",
        TextKey::SettingsSmoothPasses => "Surface smoothing",
        TextKey::SettingsSmoothPassesHint => {
            "Smooths the display only, the way VaM does. The saved shape is unchanged."
        }
        TextKey::SettingsBackgroundGroup => "Background",
        TextKey::SettingsBackground => "Style",
        TextKey::SettingsOverlayGroup => "Overlays",
        TextKey::SettingsWireframeColor => "Wireframe color",
        TextKey::SettingsWireframeOpacity => "Wireframe opacity",
        TextKey::SettingsXrayOpacity => "X-ray opacity",
        TextKey::SettingsSolidColor => "Solid head color",
        TextKey::SettingsLanguageGroup => "Language",
        TextKey::SettingsMorphNames => "Built-in morph names",
        TextKey::SettingsMorphNamesTranslated => "Translated",
        TextKey::SettingsMorphNamesOriginal => "Original English",
        TextKey::SettingsLanguage => "Display language",
        TextKey::SettingsFoldersGroup => "Folders",
        TextKey::SettingsVaMRoot => "VaM install path",
        TextKey::HairNeedsFinishedHead => "Hair is worn on a finished head.",
        TextKey::NoMatchingHair => "No matching hair presets",
        TextKey::MorphSearch => "Search Morphs",
        TextKey::MorphOneSidedFilter => "L/R",
        TextKey::MorphOneSidedFilterShow => "Show morphs that move one side only",
        TextKey::MorphOneSidedFilterHide => "Hide morphs that move one side only",
        TextKey::AppearanceSearch => "Search Looks",
        TextKey::AppearanceLayers => "Appearance layers",
        TextKey::AddAppearanceLayer => "Add the selected appearance",
        TextKey::AppearanceLayersEmpty => "No layers yet",
        TextKey::AppearanceLayerRaise => "Move up",
        TextKey::AppearanceLayerLower => "Move down",
        TextKey::LookFind => "Load Look",
        TextKey::CameraTrackballArmed => "Trackball · drag to turn freely · click or R to leave",
        TextKey::HelpTrackball => "Trackball rotate",
        TextKey::ShortcutTrackball => "R",
        TextKey::SplitModelViewTooltip => "See it from two angles — split the view",
        TextKey::SplitModelViewName => "Split View",
        TextKey::HelpLevelRoll => "Level the horizon",
        TextKey::ShortcutLevelRoll => "Alt+R",
        TextKey::RecoveryTitle => "The last session ended unexpectedly",
        TextKey::RecoveryBody => {
            "Your morphs, sculpting and texture layers were saved just before it stopped. Bring them back?"
        }
        TextKey::RecoveryRestore => "Restore",
        TextKey::RecoveryDiscard => "Start fresh",
        TextKey::RecoveryRestored => "Restored the previous session.",
        TextKey::RecoveryLookMissing => {
            "That session's look is not installed yet — textures were restored, and the edits will apply when the look is selected."
        }
        TextKey::SettingsBrushSweepGroup => "Brush size gesture",
        TextKey::BrushSweepBlender => "Blender",
        TextKey::BrushSweepZBrush => "ZBrush",
        TextKey::BrushSweepTooltip => {
            "Blender commits on a click or a second press; ZBrush commits when the key comes up"
        }
        TextKey::HairPackageStyle => "Package as .var",
        TextKey::HairPackageDone => "Hair packaged",
        TextKey::HairPackageFailed => "Packaging failed",
        TextKey::HairPackageNeedsInstall => "Install the style first",
        TextKey::DialogOpenHeadPreset => "Open a head preset",
        TextKey::FindHeadPreset => "Find a preset…",
        TextKey::SettingsVignetteIntensity => "Amount",
        TextKey::SettingsVignetteSmoothness => "Softness",
        TextKey::SettingsVignetteRoundness => "Roundness",
        TextKey::SculptNotCarried => "Sculpting could not follow this look",
        TextKey::SettingsToneCurve => "Tone curve",
        TextKey::SettingsToneCurveTooltip => "Adjusts the shadows and the highlights",
        TextKey::SettingsVignetteTooltip => "Darkens the edges of the frame",
        TextKey::SettingsEffectEnabled => "Enabled",
        TextKey::SettingsInterfaceGroup => "Interface",
        TextKey::SettingsTooltips => "Show tooltips",
        TextKey::SettingsResetGroup => "Reset",
        TextKey::SettingsClearCache => "Empty the cache",
        TextKey::SettingsClearCacheTooltip => {
            "Extracted bases and morph banks, rebuilt on demand. Emptying this costs one slower start, nothing else."
        }
        TextKey::SettingsResetAll => "Reset all settings and restart",
        TextKey::SettingsGraphicsLighting => "Lighting",
        TextKey::SettingsGraphicsEffects => "Effects",
        TextKey::SettingsGraphicsQuality => "Quality",
        TextKey::SettingsMsaa => "MSAA",
        TextKey::SettingsMsaaTooltip => {
            "Smooths stepped edges with the sample count you pick. Higher is cleaner and costlier."
        }
        TextKey::SoloViewHint => "Show only this part",
        TextKey::SoloViewRestoreHint => "Put the previous view back",
        TextKey::ToneCurveFilmic => "Filmic",
        TextKey::ToneCurveSoft => "Soft",
        TextKey::LooksHiddenForOtherFigure => "Looks for the other figure are hidden",
        TextKey::LookBelongsToOtherFigure => "This look is built for the other figure",
        TextKey::MorphEdit => "Edit Morphs",
        TextKey::VaMMorphPairImported => "VaM morph pair imported",
        TextKey::VaMMorphPairRejected => "VaM morph pair cannot be imported",
        TextKey::MorphCategoryAll => "All",
        TextKey::MorphCategoryEyes => "Eyes",
        TextKey::MorphCategoryBrows => "Brows",
        TextKey::MorphCategoryNose => "Nose",
        TextKey::MorphCategoryMouth => "Mouth",
        TextKey::MorphCategoryJaw => "Jaw",
        TextKey::MorphCategoryCheeks => "Cheeks",
        TextKey::MorphCategoryEars => "Ears",
        TextKey::MorphCategoryHead => "Face",
        TextKey::MorphCategoryExpression => "Expression",
        TextKey::NoMatchingMorphs => "No matching morphs",
        TextKey::ResetMorphs => "Reset morphs",
        TextKey::LightRotationGesture => "Shift+Right-drag",
        TextKey::UpdateAvailable => "Update",
        TextKey::PackageFromFiles => "Pick files",
        TextKey::PackageMorphFiles => "Morphs",
        TextKey::PackageTextureFiles => "Textures",
        TextKey::PackageListEmpty => "Nothing added yet",
        TextKey::LightingStudio => "Studio",
        TextKey::LightingSoft => "Soft",
        TextKey::LightingDaylight => "Daylight",
        TextKey::LightingWarm => "Warm",
        TextKey::LightingGloss => "Gloss Check",
        TextKey::LightingPortrait => "Portrait",
        TextKey::ImportLoadingMesh => "Loading mesh",
        TextKey::ImportSimplifying => "Simplifying mesh",
        TextKey::StageOptimizeHead => "Optimize custom head",
        TextKey::StagePrepareInput => "Prepare input",
        TextKey::StageAlignScan => "Align scan",
        TextKey::StageFitShape => "Fit G2 shape",
        TextKey::StageValidate => "Validate structure",
        TextKey::StagePrepareResult => "Prepare result",
        TextKey::HeavyMeshNote => "about {count} triangles -- more of them takes longer",
        TextKey::TransformGroups => "Parts",
        TextKey::CollapseTransformGroups => "Collapse parts",
        TextKey::ExpandTransformGroups => "Expand parts",
        TextKey::Skin => "Skin",
        TextKey::SculptTear => "Tear Film",
        TextKey::SculptLips => "Lips",
        TextKey::SculptTeethTongue => "Teeth · Tongue",
        TextKey::SculptInnerMouth => "Inner Mouth",
        TextKey::SculptEyes => "Eyes",
        TextKey::Size => "Size",
        TextKey::SculptXSymmetry => "X Symmetry",
        TextKey::SculptXSymmetryTooltip => "Applies the same stroke to both sides",
        TextKey::BackfaceProtection => "Backface Guard",
        TextKey::AutoGroup => "Auto Group",
        TextKey::AutoGroupTooltip => "Deforms within one polygon group only",
        TextKey::EyeTrackingAuto => "Auto",
        TextKey::EyeGazeFreeze => "Freeze as morph",
        TextKey::EyeGazeFreezeTooltip => {
            "Locks the current gaze into the morphs, so the save tab exports this eye pose"
        }
        TextKey::Reset => "Reset",
        TextKey::SculptGrab => "Grab",
        TextKey::SculptSmooth => "Smooth",
        TextKey::SculptPushPull => "Push · Pull",
        TextKey::ShortcutSculptGrab => "Drag",
        TextKey::ShortcutSculptSmooth => "Shift + Drag",
        TextKey::ShortcutSculptPushPull => "Ctrl / Alt + Drag",
        TextKey::ResetSculpt => "Reset Deformation",
        #[cfg(test)]
        TextKey::SculptApplied => "Sculpt edits applied",
        TextKey::SculptFailed => "Sculpt operation failed",
        TextKey::Ready => "Ready",
        TextKey::Busy => "Editing is locked while generating",
        TextKey::NeedScan => {
            "Open a scan head OBJ, GLB, or FBX first, or start from a look already in VaM"
        }
        TextKey::ScanLoadFailed => "That head file could not be read",
        TextKey::NeedEyeMorph => "The built-in eye morph is not compatible with this G2",
        TextKey::ReviewEyePins => "Review the red eyelid pins",
        TextKey::ResultUnavailable => "Fit the face first",
        TextKey::MorphNotInLibrary => {
            "That morph is not in the library yet; the catalog is still loading."
        }
        TextKey::SculptNeedsBaseHead => "Start from the base head on the Create tab first.",
        TextKey::MorphUnavailable => "No compatible result morph is available",
        TextKey::AlignmentPending => "Load a scan head and select your VaM folder",
        TextKey::ScanLoaded => "Custom head OBJ, GLB, or FBX loaded",
        TextKey::ScanUnloaded => "Head file removed; the G2 base is ready to edit",
        TextKey::PinPairsMismatched => "Pin counts do not match; pins without a partner",
        TextKey::UnloadScanTooltip => "Remove this head file",
        TextKey::TemplateLoaded => "G2 loaded",
        TextKey::PinsReset => "All pins were reset",
        TextKey::ResultStale => "Changes require a regenerated result",
        TextKey::GenerationStarted => "Fitting face",
        TextKey::Cancelling => "Cancelling face fit",
        TextKey::GenerationComplete => "Face fit complete",
        TextKey::GenerationCancelled => "Face fit cancelled",
        TextKey::GenerationFailed => "Face fit failed",
        TextKey::ExportStarted => "Exporting",
        TextKey::ExportComplete => "Export complete",
        TextKey::ExportFailed => "Export failed",
        TextKey::MorphNameHint => "Enter a morph name",
        TextKey::MorphGroupHint => "Group  (e.g. Custom, Morphs/Morph Loader)",
        TextKey::MorphRegionHint => "Region  (e.g. Custom, Morphs/Body)",
        TextKey::DirectEntry => "Type it in",
        TextKey::VaMMetadataPoseMorph => "Register as Pose Morph",
        TextKey::VaMMetadataPoseMorphTooltip => "Handled together with expressions and poses",
        TextKey::VaMMetadataBoneCorrection => "Skeleton Correction",
        TextKey::VaMMetadataBoneCorrectionTooltip => {
            "Recenters the pivots on the reshaped skeleton"
        }
        TextKey::NativeCorePending => "Native fitting core integration is in progress",
        TextKey::MorphPreviewUpdated => "Morph preview updated",
        TextKey::MorphsReset => "Morphs reset",
        TextKey::MorphsResetUndone => "Morph reset undone",
        TextKey::MorphEditUndone => "Morph adjustment undone",
        TextKey::MorphApplied => "Morph applied",
        TextKey::HelpPlace => "Place Pin",
        TextKey::HelpMove => "Move Pin",
        TextKey::HelpDelete => "Remove Pin",
        TextKey::HelpXSymmetry => "Pin X Symmetry",
        TextKey::HelpSnapRotation => "Snap Rotation",
        TextKey::HelpDragZoom => "Drag Zoom",
        TextKey::HelpOrbit => "Orbit",
        TextKey::HelpPan => "Pan",
        TextKey::HelpZoom => "Zoom",
        TextKey::HelpLight => "Rotate Light",
        TextKey::HelpFrameView => "Frame Head",
        TextKey::HelpUndo => "Undo",
        TextKey::ShortcutPlace => "Left Click",
        TextKey::ShortcutMove => "Alt + Drag",
        TextKey::ShortcutDelete => "Right Click",
        TextKey::ShortcutXSymmetry => "X",
        TextKey::ShortcutSnapRotation => "Shift / Ctrl + Rotate Drag",
        TextKey::ShortcutDragZoom => "Ctrl + Middle Mouse Vertical Drag",
        TextKey::ShortcutOrbit => "Middle Mouse Drag",
        TextKey::ShortcutPan => "Shift + Middle Mouse Drag",
        TextKey::ShortcutZoom => "Mouse Wheel",
        TextKey::ShortcutLight => "Shift + Right Drag",
        TextKey::ShortcutFrameView => "F",
        TextKey::HelpSnapView => "Snap View",
        TextKey::ShortcutSnapView => "Alt + Wheel Drag",
        TextKey::HelpStandardViews => "Standard Views",
        TextKey::HelpCameraProjection => "Perspective / Ortho",
        TextKey::ShortcutStandardViews => "Numpad 1-9",
        TextKey::ShortcutCameraProjection => "Numpad .",
        TextKey::ShortcutUndo => "Ctrl+Z",
        TextKey::HelpRedo => "Redo",
        TextKey::ShortcutRedo => "Ctrl+Y",
        TextKey::HairPresetSection => "Hair presets",
        TextKey::HairPresetLoad => "Load",
        TextKey::HairToolPick => "Pick layer",
        TextKey::HairToolPickHint => {
            "Click a strand in the viewport to make its layer the active one. The brush waits for the next click."
        }
        TextKey::HelpHairPick => "Pick the layer under the cursor",
        TextKey::ShortcutHairPick => "V, or Ctrl + click with any brush",
        TextKey::HelpHairSmooth => "Smooth instead of comb",
        TextKey::ShortcutHairSmooth => "Shift while combing",
        TextKey::HairGroupScalp => "Scalp",
        TextKey::HairScalpAlpha => "Scalp Cutout",
        TextKey::HairScalpPanel => "Scalp",
        TextKey::HairScalpMesh => "Scalp mesh",
        TextKey::HairScalpCreate => "Create scalp",
        TextKey::HairScalpAbsent => "This style has no scalp layer yet.",
        TextKey::HistoryBranchTitle => "The steps ahead will be lost",
        TextKey::HistoryBranchProceed => "Edit anyway",
        TextKey::DoNotShowAgain => "Don't show this again",
        TextKey::SurfaceBaseUnblended => "Some PBR maps could not be blended with the VaM skin",
        TextKey::DetailEditRedone => "Stepped forward again",
        TextKey::ShortcutBrushSmaller => "Smaller brush",
        TextKey::ShortcutBrushLarger => "Larger brush",
        TextKey::ShortcutBrushSizeDrag => "Brush size",
        TextKey::ShortcutBrushStrengthDrag => "Brush strength",
        TextKey::ShortcutStencilCancel => "Cancel stencil placement",
        TextKey::ShortcutViewOrbit => "Orbit the view",
        TextKey::ShortcutViewPan => "Pan the view",
        TextKey::ShortcutViewDolly => "Dolly the view in and out",
        TextKey::TemplatePending => "Select your VaM folder to prepare the G2 base automatically",
        TextKey::ResultEmpty => "No prepared morph",
        TextKey::ScaleLink => "Link Scale",
        TextKey::ScaleLinkTooltip => "Link X/Y/Z scale values",
        TextKey::CameraSettings => "Camera Settings",
        TextKey::Perspective => "Perspective",
        TextKey::Orthographic => "Orthographic",
        TextKey::Fov => "FOV",
        TextKey::ResetCamera => "Reset Camera",
        TextKey::WindowMinimize => "Minimize",
        TextKey::WindowMaximize => "Maximize",
        TextKey::WindowRestore => "Restore",
        TextKey::WindowClose => "Close",
        TextKey::TooltipShow => "Show",
        TextKey::TooltipHide => "Hide",
        TextKey::TooltipLock => "Lock",
        TextKey::TooltipUnlock => "Unlock",
        TextKey::AdjustEyeGaze => "Adjust eye gaze",
        TextKey::ScanLoading => "Loading scan head",
        TextKey::TemplateLoading => "Loading G2 template",
        TextKey::ResultLoading => "Loading result mesh",
        TextKey::VaMMorphPairLoading => "Loading VaM morph pair",
        TextKey::SystemLog => "System log",
        TextKey::Strength => "Strength",
        TextKey::StrengthShort => "Str",
        TextKey::SculptBrushMove => "Grab",
        TextKey::SculptBrushSmooth => "Smooth",
        TextKey::SculptBrushRestore => "Restore",
        TextKey::SculptBrushMoveTooltip => {
            "Pulls the surface with the pointer; strength is how far it follows"
        }
        TextKey::SculptBrushSmoothTooltip => "Smooths out surface detail",
        TextKey::SculptBrushRestoreTooltip => "Restores toward the base shape",
        TextKey::SculptBrushMask => "Mask",
        TextKey::SculptBrushMaskTooltip => {
            "Carve the selected appearance layer away so the one below shows. Alt paints it back"
        }
        TextKey::TextureLayers => "Texture Layers",
        TextKey::PackageSection => "Export VAR",
        TextKey::PackageCreatorHint => "Creator",
        TextKey::PackageNameHint => "Package name",
        TextKey::PackageDescriptionHint => "Description",
        TextKey::PackageCreditsHint => "Credits",
        TextKey::PackageInstructionsHint => "Instructions",
        TextKey::PackageLinkHint => "Support link",
        TextKey::PackageSave => "Save VAR",
        TextKey::PackageFromThisHead => "Build from this head",
        TextKey::PackageAddMorphs => "Add morphs",
        TextKey::PackageAddTextures => "Add textures",
        TextKey::PackageNeedsVamRestart => "VaM must be restarted",
        TextKey::PackageSavedTo => "Saved to",
        TextKey::FieldRequired => "(required)",
        TextKey::FieldOptional => "(optional)",
        TextKey::PackageSaveTo => "Save to",
        TextKey::PackageNothingToPack => "Nothing to package. Turn on this head, or add a file.",
        TextKey::PackageVersionMarker => "V",
        TextKey::PackageCreatorRequired => "A creator name is required.",
        TextKey::PackageNameRequired => "A package name is required.",
        TextKey::PackageReplaceTitle => "A VAR with this name is already there",
        TextKey::PackageReplaceBody => {
            "Replace it? Scenes that reference this version will read the new contents."
        }
        TextKey::LicenseByTooltip => "credit the author",
        TextKey::LicenseBySaTooltip => "credit the author / share alike",
        TextKey::LicenseByNdTooltip => "credit the author / no derivatives",
        TextKey::LicenseByNcTooltip => "credit the author / no commercial use",
        TextKey::LicenseByNcSaTooltip => "credit the author / no commercial use / share alike",
        TextKey::LicenseByNcNdTooltip => "credit the author / no commercial use / no derivatives",
        TextKey::LicensePcEaTooltip => "patron early access, no redistribution",
        TextKey::AddTextureLayer => "Add image",
        TextKey::AddG2UvTextureLayer => "Add UV map",
        TextKey::AddTextureLayerTooltip => {
            "Places a face photograph on the head and edits it as a texture"
        }
        TextKey::AddG2UvTextureLayerTooltip => {
            "Adds a G2 face texture in its own UV layout and edits it"
        }
        TextKey::AddTextureLayerHint => "Add a face image or G2 texture with the + button.",
        TextKey::BakingTextures => "Baking textures…",
        TextKey::BakeTexturesForSave => "Bake Textures for Save",
        TextKey::TextureMetalRough => "Metallic / Roughness",
        TextKey::TextureGlossSmooth => "Glossiness / Smoothness",
        TextKey::LayerOpacity => "Layer opacity",
        TextKey::PinOpacity => "Pin opacity",
        TextKey::TransferPins => "Transfer pins",
        TextKey::TransferPinsTooltip => "Copy the pins you placed onto the other layer images.",
        TextKey::DeleteLayer => "Delete layer",
        TextKey::TextureNormalStrength => "Normal strength",
        TextKey::TextureInvertScalar => "Invert scalar values",
        TextKey::BlendNormal => "Normal blend",
        TextKey::BlendMultiply => "Multiply",
        TextKey::BlendScreen => "Screen",
        TextKey::BlendOverlay => "Overlay",
        TextKey::MatchToneToLayerBelow => "Match tone to layer below",
        TextKey::ImageMirror => "Mirror left/right",
        TextKey::ImageMirrorOff => "Off",
        TextKey::ImageMirrorToLeft => "To left",
        TextKey::ImageMirrorToRight => "To right",
        TextKey::ImageAdjustments => "Image adjustments",
        TextKey::Exposure => "Exposure",
        TextKey::Contrast => "Contrast",
        TextKey::Saturation => "Saturation",
        TextKey::Hue => "Hue",
        TextKey::Temperature => "Temperature",
        TextKey::TextureMaskPreviewTooltip => {
            "Show the active layer mask as a translucent red overlay"
        }
        TextKey::ClearLayerMask => "Clear layer mask",
        TextKey::ResetSourceRetouch => "Reset source retouch",
        TextKey::CloneSampleHint => "Alt-click the source image to choose a clone sample.",
        TextKey::BakeBase => "Baking options",
        TextKey::TransparentOverlay => "Bake layers only",
        TextKey::CurrentSkinBake => "Bake with VaM skin",
        TextKey::TransparentOverlayTooltip => {
            "Bakes the layer images alone, without the skin preset"
        }
        TextKey::CurrentSkinBakeTooltip => "Bakes the layers over the skin preset as the base",
        TextKey::SkinPresetRequired => "Choose a skin preset in Skin settings first",
        TextKey::SkinSettings => "Skin settings",
        TextKey::ScanHeadColor => "Scan head color",
        TextKey::G2HeadColor => "G2 head color",
        TextKey::MorphFilterAll => "All",
        TextKey::MorphFilterActive => "Active",
        TextKey::BrushDodge => "Dodge",
        TextKey::BrushBurn => "Burn",
        TextKey::BrushSaturate => "Saturate",
        TextKey::BrushDesaturate => "Desaturate",
        TextKey::ShowLayersOnly => "Layer images only",
        TextKey::ShowLayersOnlyTooltip => "Hide the VaM skin and show the layer images alone",
        TextKey::MatchToneTooltip => {
            "Set exposure, saturation and temperature to match the layer below"
        }
        TextKey::ClearLayerMaskTooltip => "Erase this layer's mask",
        TextKey::TextureMetalRoughTooltip => "Save maps in the metallic/roughness convention",
        TextKey::TextureGlossSmoothTooltip => {
            "Save maps in the glossiness/smoothness convention VaM uses"
        }
        TextKey::TextureInvertScalarTooltip => {
            "Flip the values when the source uses the opposite convention (roughness vs glossiness)"
        }
        TextKey::BaseWithoutSkin => "Layer",
        TextKey::BaseWithoutSkinTooltip => {
            "The painted layers on the solid colour, without the preset; the exported texture is unchanged"
        }
        TextKey::SolidColorTooltip => "The solid colour alone, no texture",
        TextKey::TextureSkinTooltip => "The painted layers over the VaM skin preset",
        TextKey::BoundaryFeather => "Soften face edge",
        TextKey::BoundaryFeatherTooltip => {
            "Diffuse only. Maps like normal and roughness cannot be blended through alpha, so their edge stays hard"
        }
        TextKey::Baking => "Baking…",
        TextKey::TextureToolPinPair => {
            "Pin Pair — place matching points on the 3D face and source image"
        }
        TextKey::TextureToolMask => "Mask — paint to reveal; hold Alt to hide",
        TextKey::TextureToolClone => "Clone Stamp — Alt-click a sample, then paint to copy it",
        TextKey::TextureToolDodgeBurn => "Dodge/Burn — brighten; hold Alt to darken",
        TextKey::TextureToolSponge => "Sponge — add saturation; hold Alt to remove it",
        TextKey::SelectTextureLayer => "Select a texture layer.",
        TextKey::LoadingTextureImage => "Loading image…",
        TextKey::ScanTextureAlignment => "The scan texture inherits the fitted 3D alignment.",
        TextKey::NoTextureImage => "No image.",
        TextKey::MorphSavedReloadFemale => {
            "Press Reload Custom Morphs on VaM's Female Morphs tab to update."
        }
        TextKey::MorphSavedReloadMale => {
            "Press Reload Custom Morphs on VaM's Male Morphs tab to update."
        }
        TextKey::BootStarting => "Starting Vkit",
        TextKey::BootFonts => "Loading fonts",
        TextKey::BootGraphics => "Starting graphics",
        TextKey::BootSettings => "Loading settings",
        TextKey::BootTemplate => "Loading G2",
        TextKey::BootWorkspace => "Preparing the workspace",
        TextKey::BootReady => "Opening",
        TextKey::StartupFailedTitle => "Vkit could not start",
        TextKey::StartupFailedGraphics => {
            "Vkit draws with DirectX 12, and this machine's graphics could not provide it. Update your graphics driver, or run Vkit on a machine with a DirectX 12 GPU. A remote desktop session or a virtual machine often has no DirectX 12 display of its own."
        }
        TextKey::StartupFailedOther => {
            "Vkit stopped while starting up and could not open its window."
        }
        TextKey::StartupFailedLogHint => "What went wrong was written to:",
        TextKey::DialogOpenScan => "Open custom head OBJ, GLB, or FBX",
        TextKey::DialogAddUvLayer => "Add G2 UV texture layer",
        TextKey::DialogAddTextureLayer => "Add texture layer",
        TextKey::DialogSaveMorphPair => "Save VaM VMI + VMB morph pair",
        TextKey::DialogChooseVamFolder => "Choose Virt-A-Mate folder",
    }
}

pub(super) fn hair_hint(key: &str) -> Option<&'static str> {
    match key {
        "Diffuse Color" => Some(
            "The cap's own diffuse colour. White is what VaM ships, letting the skin sheet underneath show through.",
        ),
        "Gloss" => Some("How glossy the scalp cap is where it shows."),
        "Specular Intensity" => Some("How strongly the scalp cap catches a highlight."),
        "Global Illumination Filter" => Some("How much ambient light the cap takes in."),
        "Diffuse Texture Offset" => {
            Some("Added to the cap's diffuse colour. Not a sheet index, despite the name.")
        }
        "Alpha Adjust" => Some(
            "How opaque the scalp cap is. Most parts wear somebody else's cap and keep this at the bottom, where the cap is invisible; raise it only when a parting should show scalp.",
        ),
        "hairMultiplier" => Some(
            "How many strands grow between each triple of guides. The look of the hair, and most of its cost.",
        ),
        "curveDensity" => Some(
            "Points drawn along each strand. More is smoother and slower; the shape reads long before the count is high.",
        ),
        "iterations" => {
            Some("Solver passes per frame. More holds the constraints tighter under fast motion.")
        }
        "simulationEnabled" => {
            Some("Whether this part moves in the game. Nothing to do with the viewport switch.")
        }
        "collisionEnabled" => Some("Whether this part's strands are pushed out of the body."),
        "weight" => {
            Some("How heavily a strand hangs. Scales gravity and its resistance to being moved.")
        }
        "drag" => Some(
            "How much the air holds a strand back. High drag settles quickly and swings little.",
        ),
        "gravityMultiplier" => Some("Gravity on this part, as a multiple of the scene's."),
        "collisionRadius" => Some("The thickness a strand collides with, along its length."),
        "collisionRadiusRoot" => {
            Some("The thickness near the root, when it should differ from the rest.")
        }
        "friction" => Some("How much a strand grips what it touches instead of sliding along it."),
        "bendResistance" => Some("How much a strand resists being bent away from straight."),
        "rootRigidity" => Some("How strongly the root end holds the shape it was styled into."),
        "mainRigidity" => Some("How strongly the middle holds the shape it was styled into."),
        "tipRigidity" => Some("How strongly the tip holds the shape it was styled into."),
        "jointRigidity" => {
            Some("How stiff the style joints are, the springs that keep a ponytail a ponytail.")
        }
        "rigidityRolloffPower" => Some(
            "How the three rigidities blend along the strand. Higher carries the root's value further down.",
        ),
        "cling" => Some("How strongly style joints pull strands back toward each other."),
        "clingRolloff" => {
            Some("How far along the strand the cling fades. Higher confines it nearer the root.")
        }
        "snap" => Some("How firmly each segment is held to its authored length."),
        "usePaintedRigidity" => {
            Some("Take rigidity from the painted map rather than from the three sliders.")
        }
        "width" => Some(
            "How thick a strand is drawn. VaM's own slider tops out at a tenth of a millimetre.",
        ),
        "length1" => Some("Length for strands nearest the first guide of their triangle."),
        "length2" => Some("Length for strands nearest the second guide of their triangle."),
        "length3" => Some("Length for strands nearest the third guide of their triangle."),
        "maxSpread" => Some(
            "A ceiling on how far a strand may drift from its guide, not a gain. Once it clears the guide spacing, raising it does nothing.",
        ),
        "spreadRoot" => {
            Some("How freely strands separate at the root. Low gathers them toward the guide.")
        }
        "spreadMid" => Some("How freely strands separate at the middle."),
        "spreadTip" => Some("How freely strands separate at the tip."),
        "spreadMidpoint" => Some("Where along the strand the middle spread applies."),
        "spreadCurvePower" => Some("How sharply the spread turns between root, middle and tip."),
        "curlScale" => Some("How wide the curl is: the radius it coils about, not its length."),
        "curlFrequency" => Some("Turns per centimetre of strand."),
        "curlScaleRandomness" => Some("How much the curl width varies from strand to strand."),
        "curlFrequencyRandomness" => Some("How much the turn rate varies from strand to strand."),
        "curlRoot" => Some("How much curl reaches the root."),
        "curlMid" => Some("How much curl reaches the middle."),
        "curlTip" => Some("How much curl reaches the tip."),
        "curlMidpoint" => Some("Where along the strand the middle curl applies."),
        "curlCurvePower" => Some("How sharply the curl turns between root, middle and tip."),
        "curlX" => Some(
            "The curl's displacement along X. A vector that gets rotated, not an axis to coil about.",
        ),
        "curlY" => Some("The curl's displacement along Y."),
        "curlZ" => Some("The curl's displacement along Z."),
        "curlNormalAdjust" => Some(
            "Added to the curl offset along the surface normal. It lifts the coil off the head rather than leaning it.",
        ),
        "curlAllowReverse" => Some(
            "Let some strands coil the other way, so a head of curls does not read as one strand repeated.",
        ),
        "curlAllowFlipAxis" => {
            Some("Let some strands coil about a flipped axis, for the same reason.")
        }
        "rootColor" => Some("The colour at the root."),
        "tipColor" => Some("The colour at the tip."),
        "specularColor" => Some("The colour of the highlight."),
        "colorRolloff" => Some("How the root colour gives way to the tip colour along the strand."),
        "randomColorPower" => Some("How strongly strands vary in colour from one another."),
        "randomColorOffset" => Some("Which way that variation leans, lighter or darker."),
        "primarySpecularSharpness" => Some("How tight the main highlight is. Higher is glossier."),
        "secondarySpecularSharpness" => {
            Some("How tight the second highlight is, the coloured one that travels with the hair.")
        }
        "specularShift" => {
            Some("How far the second highlight sits from the first along the strand.")
        }
        "diffuseSoftness" => Some(
            "How softly light wraps around a strand rather than lighting only the side facing it.",
        ),
        "fresnelPower" => {
            Some("How sharply the rim brightens where the hair turns away from the eye.")
        }
        "fresnelAttenuation" => Some("How strong that rim brightening is."),
        "IBLFactor" => Some("How much of the scene's ambient light the hair takes."),
        "normalRandomize" => Some(
            "How much each strand's shading normal is jittered, so neighbours do not shade identically.",
        ),
        _ => None,
    }
}
