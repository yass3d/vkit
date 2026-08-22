use std::borrow::Cow;

use serde::{Deserialize, Serialize};

mod bn;
mod bn_morphs;
mod de;
mod de_morphs;
mod en;
mod es;
mod es_morphs;
mod fr;
mod fr_morphs;
mod hi;
mod hi_morphs;
mod id;
mod id_morphs;
mod ja;
mod ja_morphs;
mod ko;
mod pt;
mod pt_morphs;
mod ru;
mod ru_morphs;
mod th;
mod th_morphs;
mod vi;
mod vi_morphs;
mod zh_hans;
mod zh_hans_morphs;
mod zh_hant;
mod zh_hant_morphs;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Locale {
    #[default]
    #[serde(rename = "ko", alias = "korean", alias = "ko-kr")]
    Korean,
    #[serde(rename = "en", alias = "english", alias = "en-us")]
    English,
    #[serde(rename = "ja", alias = "japanese", alias = "ja-jp")]
    Japanese,
    #[serde(rename = "zh-Hans", alias = "zh", alias = "zh-cn", alias = "chinese")]
    ZhHans,
    #[serde(rename = "zh-Hant", alias = "zh-tw", alias = "zh-hk")]
    ZhHant,
    #[serde(rename = "es", alias = "spanish", alias = "es-es")]
    Spanish,
    #[serde(rename = "hi", alias = "hindi", alias = "hi-in")]
    Hindi,
    #[serde(rename = "pt", alias = "portuguese", alias = "pt-br")]
    Portuguese,
    #[serde(rename = "bn", alias = "bengali", alias = "bn-bd", alias = "bn-in")]
    Bengali,
    #[serde(rename = "fr", alias = "french", alias = "fr-fr")]
    French,
    #[serde(rename = "de", alias = "german", alias = "de-de")]
    German,
    #[serde(rename = "ru", alias = "russian", alias = "ru-ru")]
    Russian,
    #[serde(rename = "id", alias = "indonesian", alias = "id-id")]
    Indonesian,
    #[serde(rename = "vi", alias = "vietnamese", alias = "vi-vn")]
    Vietnamese,
    #[serde(rename = "th", alias = "thai", alias = "th-th")]
    Thai,
}

impl Locale {
    pub const ALL: [Self; 15] = [
        Self::Korean,
        Self::English,
        Self::Japanese,
        Self::ZhHans,
        Self::ZhHant,
        Self::Spanish,
        Self::Hindi,
        Self::Portuguese,
        Self::Bengali,
        Self::French,
        Self::German,
        Self::Russian,
        Self::Indonesian,
        Self::Vietnamese,
        Self::Thai,
    ];

    #[must_use]
    pub fn system_default() -> Locale {
        system_locale_tag()
            .as_deref()
            .and_then(Self::from_bcp47)
            .unwrap_or(Locale::English)
    }

    #[must_use]
    pub fn from_bcp47(tag: &str) -> Option<Self> {
        let lowered = tag.to_ascii_lowercase();
        let mut parts = lowered.split(['-', '_']);
        let language = parts.next()?;
        let rest: Vec<&str> = parts.collect();
        let has = |needle: &str| rest.contains(&needle);
        Some(match language {
            "ko" => Self::Korean,
            "en" => Self::English,
            "ja" => Self::Japanese,
            "zh" => {
                if has("hant") || has("tw") || has("hk") || has("mo") {
                    Self::ZhHant
                } else {
                    Self::ZhHans
                }
            }
            "es" => Self::Spanish,
            "pt" => Self::Portuguese,
            "fr" => Self::French,
            "de" => Self::German,
            "ru" => Self::Russian,
            "hi" => Self::Hindi,

            "id" | "in" => Self::Indonesian,
            "vi" => Self::Vietnamese,
            "th" => Self::Thai,
            "bn" => Self::Bengali,
            _ => return None,
        })
    }

    pub const fn selector_label(self) -> &'static str {
        match self {
            Self::Korean => "한국어",
            Self::English => "English",
            Self::Japanese => "日本語",
            Self::ZhHans => "简体中文",
            Self::ZhHant => "繁體中文",
            Self::Spanish => "Español",
            Self::Hindi => "हिन्दी",
            Self::Portuguese => "Português",
            Self::Bengali => "বাংলা",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Russian => "Русский",
            Self::Indonesian => "Bahasa Indonesia",
            Self::Vietnamese => "Tiếng Việt",
            Self::Thai => "ไทย",
        }
    }
}

macro_rules! text_keys {
    ($($(#[$meta:meta])* $name:ident),* $(,)?) => {
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum TextKey {
            $($(#[$meta])* $name,)*
        }

        #[cfg(test)]
        pub const ALL_TEXT_KEYS: &[TextKey] = &[$(TextKey::$name,)*];
    };
}

text_keys! {
    ResultPreview,
    Save,

    DetailCorrection,
    AutoGroup,
    SculptXSymmetryTooltip,
    AutoGroupTooltip,
    BackfaceProtection,
    TextureStage,
    HairStage,
    HairUnavailable,
    HairPartsPanel,
    AddHairPart,
    HairScalpMissing,
    HairScalpMeshCrowded,
    HairGameOnly,
    HairPartTint,
    HairShowPoints,
    HairShowPointsHint,
    HairPartTintHint,
    HairSettingsPanel,
    HairCreateFirst,
    HairHideStrands,
    HairHideStrandsHint,
    HairShowStreams,
    HairShowStreamsHint,
    HairViewportPhysics,
    HairViewportPhysicsHint,
    HairMirrorEdit,
    HairMirrorEditHint,
    HairAutoPart,
    HairAutoPartHint,
    HairExportSection,
    HairExportBothSexes,
    HairExportRescanNotice,
    HairExportOverwroteNotice,
    HairSimToggleHint,
    HairCollisionToggleHint,
    HairStyleJoints,
    HairStyleJointsHint,
    HairThumbnailPrompt,
    HairThumbnailShoot,
    HairThumbnailSkip,
    HairThumbnailSaved,
    HairThumbnailFailed,
    HairPartVisible,
    RemoveHairPart,
    HairRenamePart,
    HairToolPlant,
    HairToolErase,
    HairToolGrow,
    HairToolComb,
    HairToolPlantHint,
    HairToolGrowHint,
    HairToolEraseHint,
    HairToolCombHint,
    HairToolPinch,
    HairToolPinchHint,
    HairToolCut,
    HairToolCutHint,
    HairToolPuff,
    HairToolPuffHint,
    HairToolVertex,
    HairToolVertexHint,
    HairCopySettings,
    HairPasteSettings,
    HairGroupPerformance,
    HairGroupPhysics,
    HairGroupStiffness,
    HairGroupShape,
    HairGroupCurl,
    HairGroupLook,
    HairColorScalp,
    HairColorRoot,
    HairColorTip,
    HairColorSpecular,
    HairScalpMask,
    HairScalpMaskBuiltIn,
    HairScalpMaskCustom,
    HairExport,
    HairExportName,
    HairExportCreator,
    HairExportNeedsMetadata,
    HairOverwriteTitle,
    HairOverwriteBody,
    HairOverwriteProceed,
    HairExportDone,
    HairExportFailed,
    HairExportNeedsPart,
    HairExportNeedsVaMRoot,
    HairMirrorPart,
    HairDuplicatePart,
    HairSegmentsShort,
    OpenScan,
    SourceMorphMissing,
    EditDetails,
    Female,
    Male,
    Lighting,
    Brightness,
    LightRotation,

    AddHeadFile,
    AddHeadFileHint,
    FaceMatch,
    AutoAlign,
    PlacementReset,
    Scale,
    Position,
    Rotation,
    XMirror,
    XMirrorTooltip,
    NumbersTooltip,
    XMirrorRejected,
    ScanSymmetry,
    ScanFidelity,
    ScanOptions,
    RestoreNeckEars,
    RestoreNeckEarsTooltip,
    RestoreNeckEarsDeferred,
    ScanFidelityTooltip,
    Original,
    Left,
    Right,
    EyesOpen,
    EyesClosed,
    View,
    On,
    Numbers,
    PinPairsPlaced,
    PinPlacement,
    ScanReference,
    AddHeadFileAction,
    AddHeadFileActionTooltip,
    ContinueStep,
    ContinueStepTooltip,
    Undo,
    ResetAllPins,
    ResetAll,

    MorphSave,

    TextureSaveSection,
    Generate,
    Cancel,
    CopyAll,
    Next,
    TextureSkin,
    SolidColor,
    SurfaceSmooth,
    SurfaceSmoothTooltip,
    BackfaceProtectionTooltip,
    WireframeColor,
    WireframeSettings,
    XraySettings,
    ViewportLightingTooltip,
    Background,
    ViewportBackgroundTooltip,
    BackgroundRadial,
    BackgroundVertical,
    BackgroundFlat,
    ViewportWireframeTooltip,
    ViewportXrayTooltip,
    ViewportSkinTooltip,
    FalloffTooltip,
    FalloffSmooth,
    FalloffSmoother,
    FalloffSharp,
    FalloffLinear,
    ScanOverlay,
    PlaceHead,
    PlaceHeadTooltip,

    OverlayDone,

    ProjectImage,

    ProjectionCanvasHint,
    HelpProjectionPaint,
    ShortcutProjectionPaint,
    HelpProjectionMove,
    ShortcutProjectionMove,
    HelpProjectionRotate,
    ShortcutProjectionRotate,
    HelpProjectionZoom,
    ShortcutProjectionZoom,
    HelpBrushSize,
    ShortcutBrushSize,
    HelpBrushStrength,
    ShortcutBrushStrength,
    HelpProjectionDone,
    ShortcutProjectionDone,
    ProjectDone,
    ProjectDoneTooltip,

    TransparentOverlayTooltip,

    CurrentSkinBakeTooltip,

    SkinPresetRequired,

    SkinSettings,
    ScanHeadColor,
    G2HeadColor,

    MorphFilterAll,
    MorphFilterActive,

    BrushDodge,
    BrushBurn,
    BrushSaturate,
    BrushDesaturate,
    MorphCompare,
    Opacity,
    G2Head,
    Eyelashes,
    OverwriteTitle,
    OverwriteBody,
    Overwrite,
    ScanTextureLayer,
    PinPrompt,
    TextureNeedsImage,
    TexturePinPairPrompt,
    TextureToolNeedsPins,
    SymmetrySuggestion,
    SymmetryChangeTitle,
    SymmetryChangeConfirm,
    EyeClosure,
    VaMFolder,

    #[cfg(test)]
    VaMGeometryBaseReady,
    VaMGeometryBaseRejected,
    VaMGeometryBaseHowTo,
    VaMBaseSourceUnityBundle,
    VaMBaseSourceDazAnchorPair,
    VaMBaseSourceUnityAnchorPair,
    VaMBaseSourceUnverifiedObj,
    VaMUvMappingUnavailable,
    VaMIndexing,
    VaMFailed,
    RefreshSkins,
    MorphLoading,
    MorphLoadFailed,
    SkinNone,
    DefaultSkinMissing,
    DefaultSkinSet,
    DefaultSkinClear,
    SkinSearch,
    SkinLoading,
    SkinReady,
    SkinLoadFailed,
    SkinUvUnavailable,
    NoMatchingSkins,
    HairSearch,
    HairReady,
    HairLoadFailed,
    HairPartsSkipped,
    HairParts,
    BaseFace,
    HairNeedsFinishedHead,
    Settings,
    SettingsGraphics,
    SettingsViewport,
    SettingsGeneral,
    SettingsAbout,
    SettingsShortcuts,
    ShortcutsCapturing,
    ShortcutsResetAll,
    ShortcutsExport,
    ShortcutsImport,
    ShortcutsTaken,
    HelpRedo,
    ShortcutRedo,
    HairPresetSection,
    HairPresetLoad,
    HairToolPick,
    HairToolPickHint,
    HelpHairPick,
    ShortcutHairPick,
    HelpHairSmooth,
    ShortcutHairSmooth,
    HairGroupScalp,
    HairScalpAlpha,
    HairScalpPanel,
    HairScalpMesh,
    HairScalpCreate,
    HairScalpAbsent,
    HistoryBranchTitle,
    HistoryBranchProceed,
    DoNotShowAgain,
    SurfaceBaseUnblended,
    DetailEditRedone,
    ShortcutBrushSmaller,
    ShortcutBrushLarger,
    ShortcutBrushSizeDrag,
    ShortcutBrushStrengthDrag,
    ShortcutStencilCancel,
    ShortcutViewOrbit,
    ShortcutViewPan,
    ShortcutViewDolly,
    SettingsAboutBuild,
    SettingsAboutLicense,
    SettingsAboutDiagnostics,
    SettingsTooltipsTooltip,

    AboutTagline,

    AboutNoAffiliation,
    AboutNoWarranty,
    AboutSupport,
    AboutReportProblem,
    AboutOpenLogs,
    AboutReportIssue,
    SettingsLightingGroup,
    SettingsLightingPreset,
    SettingsExposure,
    SettingsGeometryGroup,
    SettingsSmoothPasses,
    SettingsSmoothPassesHint,
    SettingsBackgroundGroup,
    SettingsBackground,
    SettingsOverlayGroup,
    SettingsWireframeColor,
    SettingsWireframeOpacity,
    SettingsXrayOpacity,
    SettingsSolidColor,
    SettingsLanguageGroup,
    SettingsLanguage,
    SettingsMorphNames,
    SettingsMorphNamesTranslated,
    SettingsMorphNamesOriginal,
    SettingsFoldersGroup,
    SettingsVaMRoot,
    NoMatchingHair,
    MorphSearch,
    MorphOneSidedFilter,
    MorphOneSidedFilterShow,
    MorphOneSidedFilterHide,
    AppearanceSearch,
    AppearanceLayers,
    AddAppearanceLayer,
    AppearanceLayersEmpty,
    AppearanceLayerRaise,
    AppearanceLayerLower,
    LookFind,
    CameraTrackballArmed,
    HelpTrackball,
    ShortcutTrackball,
    SplitModelViewTooltip,
    SplitModelViewName,
    HelpLevelRoll,
    ShortcutLevelRoll,
    RecoveryTitle,
    RecoveryBody,
    RecoveryRestore,
    RecoveryDiscard,
    RecoveryRestored,
    RecoveryLookMissing,
    SettingsVignetteGroup,
    SettingsBrushSweepGroup,
    BrushSweepBlender,
    BrushSweepZBrush,
    BrushSweepTooltip,
    HairPackageStyle,
    HairPackageDone,
    HairPackageFailed,
    HairPackageNeedsInstall,
    DialogOpenHeadPreset,
    FindHeadPreset,
    SettingsVignetteIntensity,
    SettingsVignetteSmoothness,
    SettingsVignetteRoundness,
    SculptNotCarried,
    SettingsToneCurve,
    SettingsToneCurveTooltip,
    SettingsVignetteTooltip,

    SettingsEffectEnabled,
    SettingsInterfaceGroup,
    SettingsTooltips,
    SettingsResetGroup,
    SettingsResetAll,
    SettingsClearCache,
    SettingsClearCacheTooltip,
    SettingsGraphicsLighting,
    SettingsGraphicsEffects,
    SettingsGraphicsQuality,
    SettingsQualityAntialiasing,
    SettingsMsaa,
    SettingsMsaaTooltip,
    SettingsMsaaRestart,
    SoloViewHint,
    SoloViewRestoreHint,
    ToneCurveFilmic,
    ToneCurveSoft,
    LooksHiddenForOtherFigure,
    LookBelongsToOtherFigure,
    MorphEdit,
    VaMMorphPairImported,
    VaMMorphPairRejected,
    MorphCategoryAll,
    MorphCategoryEyes,
    MorphCategoryBrows,
    MorphCategoryNose,
    MorphCategoryMouth,
    MorphCategoryJaw,
    MorphCategoryCheeks,
    MorphCategoryEars,
    MorphCategoryHead,
    MorphCategoryBody,
    TextureNamePlaceholder,
    TextureOverwriteTitle,
    MorphCategoryExpression,
    NoMatchingMorphs,
    ResetMorphs,
    UndoMorphReset,

    LightRotationGesture,

    UpdateAvailable,

    PackageFromFiles,
    PackageMorphFiles,
    PackageTextureFiles,
    PackageListEmpty,

    LightingStudio,

    LightingSoft,

    LightingDaylight,

    LightingWarm,

    LightingGloss,

    LightingPortrait,

    ImportLoadingMesh,

    ImportSimplifying,

    StageOptimizeHead,

    StagePrepareInput,

    StageAlignScan,

    StageFitShape,

    StageValidate,

    StagePrepareResult,

    HeavyMeshNote,
    TransformGroups,
    CollapseTransformGroups,
    ExpandTransformGroups,
    Skin,
    SculptTear,
    SculptLips,
    SculptTeethTongue,
    SculptInnerMouth,
    SculptEyes,
    Size,
    SculptXSymmetry,
    EyeTrackingAuto,
    EyeGazeFreeze,
    EyeGazeFreezeTooltip,
    Reset,
    SculptGrab,
    SculptSmooth,
    SculptPushPull,
    ShortcutSculptGrab,
    ShortcutSculptSmooth,
    ShortcutSculptPushPull,
    ResetSculpt,

    #[cfg(test)]
    SculptApplied,
    SculptFailed,
    Ready,
    Busy,
    NeedScan,
    ScanLoadFailed,
    NeedEyeMorph,
    ReviewEyePins,
    ResultUnavailable,
    MorphUnavailable,
    MorphNotInLibrary,
    SculptNeedsBaseHead,
    AlignmentPending,
    ScanLoaded,
    ScanUnloaded,
    PinPairsMismatched,
    UnloadScanTooltip,
    TemplateLoaded,
    PinsReset,
    ResultStale,
    GenerationStarted,
    Cancelling,
    GenerationComplete,
    GenerationCancelled,
    GenerationFailed,
    ExportStarted,
    ExportComplete,
    ExportFailed,
    MorphNameHint,
    MorphGroupHint,
    MorphRegionHint,
    DirectEntry,
    VaMMetadataPoseMorph,
    VaMMetadataPoseMorphTooltip,
    VaMMetadataBoneCorrection,
    VaMMetadataBoneCorrectionTooltip,
    NativeCorePending,
    MorphPreviewUpdated,
    MorphsReset,
    MorphsResetUndone,
    MorphEditUndone,
    MorphApplied,
    HelpPlace,
    HelpMove,
    HelpDelete,
    HelpXSymmetry,
    HelpSnapRotation,
    HelpDragZoom,
    HelpOrbit,
    HelpPan,
    HelpZoom,
    HelpLight,
    HelpFrameView,
    HelpSnapView,
    HelpStandardViews,
    HelpCameraProjection,
    HelpUndo,
    ShortcutPlace,
    ShortcutMove,
    ShortcutDelete,
    ShortcutXSymmetry,
    ShortcutSnapRotation,
    ShortcutDragZoom,
    ShortcutOrbit,
    ShortcutPan,
    ShortcutZoom,
    ShortcutLight,
    ShortcutFrameView,
    ShortcutSnapView,
    ShortcutStandardViews,
    ShortcutCameraProjection,
    ShortcutUndo,
    TemplatePending,
    ResultEmpty,
    ScaleLink,
    ScaleLinkTooltip,
    CameraSettings,
    Perspective,
    Orthographic,
    Fov,
    ResetCamera,
    WindowMinimize,
    WindowMaximize,
    WindowRestore,
    WindowClose,

    TooltipShow,
    TooltipHide,
    TooltipLock,
    TooltipUnlock,
    AdjustEyeGaze,
    ScanLoading,
    TemplateLoading,
    ResultLoading,
    VaMMorphPairLoading,

    SystemLog,

    Strength,
    StrengthShort,

    SculptBrushMove,
    SculptBrushSmooth,
    SculptBrushRestore,
    SculptBrushMoveTooltip,
    SculptBrushSmoothTooltip,
    SculptBrushRestoreTooltip,
    SculptBrushMask,
    SculptBrushMaskTooltip,
    TextureLayers,
    PackageSection,
    PackageCreatorHint,
    PackageNameHint,
    PackageDescriptionHint,
    PackageCreditsHint,
    PackageInstructionsHint,
    PackageLinkHint,
    PackageSave,
    PackageVersionMarker,
    PackageFromThisHead,
    PackageAddMorphs,
    PackageAddTextures,
    PackageSaveTo,
    PackageSavedTo,
    PackageNeedsVamRestart,
    FieldRequired,
    FieldOptional,
    PackageNothingToPack,
    PackageCreatorRequired,
    PackageNameRequired,
    PackageReplaceTitle,
    PackageReplaceBody,
    LicenseByTooltip,
    LicenseBySaTooltip,
    LicenseByNdTooltip,
    LicenseByNcTooltip,
    LicenseByNcSaTooltip,
    LicenseByNcNdTooltip,
    LicensePcEaTooltip,
    AddTextureLayer,
    AddG2UvTextureLayer,

    AddTextureLayerTooltip,
    AddG2UvTextureLayerTooltip,
    AddTextureLayerHint,
    BakingTextures,
    BakeTexturesForSave,
    TextureMetalRough,
    TextureGlossSmooth,
    LayerOpacity,
    PinOpacity,
    TransferPins,
    TransferPinsTooltip,
    DeleteLayer,
    TextureNormalStrength,
    TextureInvertScalar,
    BlendNormal,
    BlendMultiply,
    BlendScreen,
    BlendOverlay,
    MatchToneToLayerBelow,
    ImageMirror,
    ImageMirrorOff,
    ImageMirrorToLeft,
    ImageMirrorToRight,
    ImageAdjustments,
    Exposure,
    Contrast,
    Saturation,
    Hue,
    Temperature,
    TextureMaskPreviewTooltip,
    ClearLayerMask,
    ResetSourceRetouch,
    CloneSampleHint,
    BakeBase,
    TransparentOverlay,
    CurrentSkinBake,
    ShowLayersOnly,
    ShowLayersOnlyTooltip,
    MatchToneTooltip,
    ClearLayerMaskTooltip,
    TextureMetalRoughTooltip,
    TextureGlossSmoothTooltip,
    TextureInvertScalarTooltip,
    BaseWithoutSkin,
    BaseWithoutSkinTooltip,
    SolidColorTooltip,
    TextureSkinTooltip,
    BoundaryFeather,
    BoundaryFeatherTooltip,
    Baking,
    TextureToolPinPair,
    TextureToolMask,
    TextureToolClone,
    TextureToolDodgeBurn,
    TextureToolSponge,
    SelectTextureLayer,
    LoadingTextureImage,
    ScanTextureAlignment,
    NoTextureImage,

    MorphSavedReloadFemale,
    MorphSavedReloadMale,

    BootStarting,
    BootFonts,
    BootGraphics,
    BootSettings,
    BootTemplate,
    BootWorkspace,
    BootReady,

    StartupFailedTitle,
    StartupFailedGraphics,
    StartupFailedOther,
    StartupFailedLogHint,

    DialogOpenScan,
    DialogAddUvLayer,
    DialogAddTextureLayer,
    DialogSaveMorphPair,
    DialogChooseVamFolder,
}

#[cfg(windows)]
#[allow(unsafe_code, reason = "isolated Win32 locale query")]
fn system_locale_tag() -> Option<String> {
    use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;

    const LOCALE_NAME_MAX_LENGTH: usize = 85;
    let mut buffer = [0_u16; LOCALE_NAME_MAX_LENGTH];

    let written = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
    if written <= 1 {
        return None;
    }
    let units = &buffer[..(written as usize).saturating_sub(1)];
    Some(String::from_utf16_lossy(units))
}

#[cfg(not(windows))]
fn system_locale_tag() -> Option<String> {
    std::env::var("LANG").ok()
}

#[must_use]
pub fn hair_param_label(locale: Locale, key: &str) -> Option<&'static str> {
    match locale {
        Locale::English => None,
        Locale::Korean => ko::hair_label(key),
        Locale::Japanese => ja::hair_label(key),
        Locale::ZhHans => zh_hans::hair_label(key),
        Locale::ZhHant => zh_hant::hair_label(key),
        Locale::Spanish => es::hair_label(key),
        Locale::Hindi => hi::hair_label(key),
        Locale::Portuguese => pt::hair_label(key),
        Locale::Bengali => bn::hair_label(key),
        Locale::French => fr::hair_label(key),
        Locale::German => de::hair_label(key),
        Locale::Russian => ru::hair_label(key),
        Locale::Indonesian => id::hair_label(key),
        Locale::Vietnamese => vi::hair_label(key),
        Locale::Thai => th::hair_label(key),
    }
}

#[must_use]
pub fn hair_param_hint(locale: Locale, key: &str) -> Option<&'static str> {
    let translated = match locale {
        Locale::English => None,
        Locale::Korean => ko::hair_hint(key),
        Locale::Japanese => ja::hair_hint(key),
        Locale::ZhHans => zh_hans::hair_hint(key),
        Locale::ZhHant => zh_hant::hair_hint(key),
        Locale::Spanish => es::hair_hint(key),
        Locale::Hindi => hi::hair_hint(key),
        Locale::Portuguese => pt::hair_hint(key),
        Locale::Bengali => bn::hair_hint(key),
        Locale::French => fr::hair_hint(key),
        Locale::German => de::hair_hint(key),
        Locale::Russian => ru::hair_hint(key),
        Locale::Indonesian => id::hair_hint(key),
        Locale::Vietnamese => vi::hair_hint(key),
        Locale::Thai => th::hair_hint(key),
    };
    translated.or_else(|| en::hair_hint(key))
}

pub const fn text(locale: Locale, key: TextKey) -> &'static str {
    match locale {
        Locale::Korean => ko::label(key),
        Locale::English => en::label(key),
        Locale::Japanese => match ja::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
        Locale::ZhHans => match zh_hans::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
        Locale::ZhHant => match zh_hant::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
        Locale::Spanish => match es::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
        Locale::Hindi => match hi::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
        Locale::Portuguese => match pt::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
        Locale::Bengali => match bn::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
        Locale::French => match fr::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
        Locale::German => match de::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
        Locale::Russian => match ru::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
        Locale::Indonesian => match id::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
        Locale::Vietnamese => match vi::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
        Locale::Thai => match th::label(key) {
            Some(label) => label,
            None => en::label(key),
        },
    }
}

pub fn log_event_label(locale: Locale, event: &str) -> Option<&'static str> {
    let (korean, english, japanese) = match event {
        "startup" => (
            "프로그램 시작",
            "Application started",
            "アプリを起動しました",
        ),
        "close_requested" => (
            "설정 저장 후 종료",
            "Saving settings and exiting",
            "設定を保存して終了",
        ),
        "font_loaded" => (
            "시스템 폰트 로드됨",
            "System font loaded",
            "システムフォントを読み込みました",
        ),
        "font_fallback" => (
            "대체 폰트 사용",
            "Using fallback font",
            "代替フォントを使用",
        ),
        "icon_loaded" => (
            "앱 아이콘 로드됨",
            "App icon loaded",
            "アプリアイコンを読み込みました",
        ),
        "workspace_load_starting" => ("파일 불러오는 중", "Loading file", "ファイルを読み込み中"),
        "workspace_load_start_failed" => (
            "파일 불러오기 실패",
            "File load failed",
            "ファイルの読み込みに失敗",
        ),
        "scan_import_failed" => (
            "스캔용 헤드 불러오기 실패",
            "Custom head import failed",
            "カスタムヘッドの読み込みに失敗",
        ),
        "builtin_morph_catalog_ready" => (
            "빌트인 모프 카탈로그 준비 완료",
            "Built-in morph catalog ready",
            "ビルトインモーフカタログ準備完了",
        ),
        "builtin_morph_failed" => (
            "빌트인 모프 오류",
            "Built-in morph error",
            "ビルトインモーフのエラー",
        ),
        "vam_catalog_prewarm" => (
            "VaM 카탈로그 미리 읽는 중",
            "Pre-warming VaM catalog",
            "VaMカタログを事前読み込み中",
        ),
        "vam_appearance_ready" => (
            "VaM 스킨 목록 준비 완료",
            "VaM skin list ready",
            "VaMスキン一覧準備完了",
        ),
        "vam_catalog_appearance_warnings" => (
            "일부 VaM 프리셋 건너뜀",
            "Some VaM presets skipped",
            "一部のVaMプリセットをスキップ",
        ),
        "vam_catalog_failed" => (
            "VaM 카탈로그 오류",
            "VaM catalog error",
            "VaMカタログのエラー",
        ),
        "skin_preview_ready" => (
            "스킨 미리보기 적용됨",
            "Skin preview applied",
            "スキンプレビューを適用",
        ),
        "skin_preview_built" => (
            "스킨 미리보기 준비됨",
            "Skin preview built",
            "スキンプレビュー準備完了",
        ),
        "skin_preview_failed" => (
            "스킨 미리보기 실패",
            "Skin preview failed",
            "スキンプレビューに失敗",
        ),
        "fit_stage_finished" => ("피팅 완료", "Fit finished", "フィッティング完了"),
        "fit_stage_failed" => ("피팅 실패", "Fit failed", "フィッティング失敗"),
        "job_cancelled" => ("작업 취소됨", "Job cancelled", "ジョブをキャンセル"),
        "job_failed" => ("작업 실패", "Job failed", "ジョブが失敗"),
        "job_start_failed" => ("작업 시작 실패", "Job start failed", "ジョブの開始に失敗"),
        "export_failed" => ("내보내기 실패", "Export failed", "エクスポートに失敗"),
        "export_start_failed" => (
            "내보내기 시작 실패",
            "Export start failed",
            "エクスポートの開始に失敗",
        ),
        "texture_transfer_complete" => (
            "텍스처 전송 완료",
            "Texture transfer complete",
            "テクスチャ転送完了",
        ),
        "texture_transfer_skipped" => (
            "텍스처 전송 건너뜀",
            "Texture transfer skipped",
            "テクスチャ転送をスキップ",
        ),
        "settings_load_failed" => (
            "설정 불러오기 실패",
            "Settings load failed",
            "設定の読み込みに失敗",
        ),
        "settings_save_failed" => ("설정 저장 실패", "Settings save failed", "設定の保存に失敗"),
        "initialization_failed" => ("초기화 실패", "Initialization failed", "初期化に失敗"),
        "renderer_install_failed" => (
            "렌더러 리소스 설치 실패",
            "Renderer resource install failed",
            "レンダラーリソースの設置に失敗",
        ),
        _ => return None,
    };
    Some(match locale {
        Locale::Korean => korean,
        Locale::Japanese => japanese,

        _ => english,
    })
}

#[cfg(test)]
pub fn morph_label<'a>(locale: Locale, raw: &'a str) -> Cow<'a, str> {
    morph_label_for(locale, "", raw)
}

fn builtin_name_table(locale: Locale) -> Option<fn(&str) -> Option<&'static str>> {
    match locale {
        Locale::Bengali => Some(bn_morphs::builtin),
        Locale::German => Some(de_morphs::builtin),
        Locale::Spanish => Some(es_morphs::builtin),
        Locale::French => Some(fr_morphs::builtin),
        Locale::Hindi => Some(hi_morphs::builtin),
        Locale::Indonesian => Some(id_morphs::builtin),
        Locale::Japanese => Some(ja_morphs::builtin),
        Locale::Portuguese => Some(pt_morphs::builtin),
        Locale::Russian => Some(ru_morphs::builtin),
        Locale::Thai => Some(th_morphs::builtin),
        Locale::Vietnamese => Some(vi_morphs::builtin),
        Locale::ZhHans => Some(zh_hans_morphs::builtin),
        Locale::ZhHant => Some(zh_hant_morphs::builtin),
        Locale::English | Locale::Korean => None,
    }
}

pub fn morph_label_for<'a>(locale: Locale, stable_id: &str, raw: &'a str) -> Cow<'a, str> {
    if let Some(lookup) = builtin_name_table(locale) {
        let canonical_id = canonical_morph_id(stable_id);
        let key = canonical_id
            .strip_prefix("builtin")
            .unwrap_or(&canonical_id);
        if let Some(name) = lookup(key) {
            return Cow::Borrowed(name);
        }
        if locale != Locale::Japanese {
            return Cow::Borrowed(raw);
        }

        return match ja_morphs::compose(&morph_words(raw)) {
            Some(composed) => Cow::Owned(composed),
            None => Cow::Borrowed(raw),
        };
    }
    if locale != Locale::Korean {
        return Cow::Borrowed(raw);
    }

    let canonical_id = canonical_morph_id(stable_id);
    if let Some(exact) = exact_korean_morph_id(&canonical_id) {
        return Cow::Borrowed(exact);
    }

    let words = morph_words(raw);
    let canonical = words.join(" ").to_ascii_lowercase();
    if let Some(exact) = exact_korean_morph_label(&canonical) {
        return Cow::Borrowed(exact);
    }

    let translated = order_korean_morph_tokens(contextual_korean_morph_words(&words));
    if translated.chars().any(is_hangul) {
        Cow::Owned(translated)
    } else {
        Cow::Borrowed(raw)
    }
}

fn contextual_korean_morph_words(words: &[String]) -> Vec<(String, MorphTokenRank)> {
    let lowered = words
        .iter()
        .map(|word| word.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let is_lip_control = lowered
        .iter()
        .any(|word| matches!(word.as_str(), "lip" | "lips"));
    let mut translated = Vec::with_capacity(words.len());
    let mut index = 0;
    while index < words.len() {
        let word = lowered[index].as_str();
        let next = lowered.get(index + 1).map(String::as_str);
        let pair = match (word, next) {
            ("fore", Some("head")) => Some("이마"),
            ("jaw", Some("line")) => Some("턱선"),
            ("nose", Some("bridge")) => Some("콧대"),
            ("nose", Some("tip")) => Some("코끝"),
            ("cheek" | "cheeks", Some("bone" | "bones")) => Some("광대뼈"),
            ("eye" | "eyes", Some("bags")) => Some("눈밑 지방"),
            ("eye" | "eyes", Some("cornea")) => Some("각막"),
            ("mouth", Some("marionette")) => Some("입가"),
            ("pupil" | "pupils", Some("slit")) => Some("세로 동공"),
            ("upper" | "top" | "up", Some("lip" | "lips")) => Some("윗입술"),
            ("lower" | "bottom" | "low" | "lo", Some("lip" | "lips")) => Some("아랫입술"),
            ("lip" | "lips", Some("upper" | "top" | "up")) => Some("윗입술"),
            ("lip" | "lips", Some("lower" | "bottom" | "low" | "lo")) => Some("아랫입술"),

            ("upper" | "top", Some("jaw")) => Some("위턱"),
            ("lower" | "bottom", Some("jaw")) => Some("아래턱"),

            ("brow" | "brows", Some("size" | "thickness")) => Some("눈썹 굵기"),
            ("side", Some("side")) => Some("좌우 이동"),

            ("ja", Some("r")) => Some(""),
            _ => None,
        };
        if let Some(pair) = pair {
            if !pair.is_empty() {
                translated.push((pair.to_owned(), morph_token_rank(pair)));
            }
            index += 2;
            continue;
        }

        let contextual = match word {
            "full" if is_lip_control => Some("볼륨"),
            "full" => Some("전체"),
            "out"
                if index
                    .checked_sub(1)
                    .and_then(|previous| lowered.get(previous))
                    .is_some_and(|previous| previous.as_str() == "chin") =>
            {
                Some("내밀기")
            }
            "jar" => Some(""),
            _ => korean_morph_word(word),
        };
        if words[index]
            .chars()
            .all(|character| character.is_ascii_digit())
        {
            translated.push((words[index].clone(), MorphTokenRank::Variant));
        } else if let Some(name) = korean_morph_name(word) {
            translated.push((name.to_owned(), MorphTokenRank::Name));
        } else if let Some(contextual) = contextual {
            if !contextual.is_empty() {
                translated.push((contextual.to_owned(), morph_token_rank(contextual)));
            }
        } else {
            translated.push(("고유 형태".to_owned(), MorphTokenRank::Shape));
        }
        index += 1;
    }
    translated
}

fn canonical_morph_id(stable_id: &str) -> String {
    stable_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn exact_korean_morph_id(canonical: &str) -> Option<&'static str> {
    let suffixes = [
        ("outermouthshapev", "입 바깥선 V형"),
        ("mouthmarionettelines", "입가 주름"),
        ("eyescorneabulge", "각막 돌출"),
        ("eyeswidth", "눈 너비"),
        ("eyewidth", "눈 너비"),
        ("foreheadtopwidth", "이마 위 너비"),
        ("jawlinedepth", "턱선 깊이"),
        ("lipuppersizeren", "윗입술 크기"),
        ("upperlipcurve2", "윗입술 곡선 2"),
        ("lipsbottomfull", "아랫입술 볼륨"),
        ("lipbottomfull", "아랫입술 볼륨"),
        ("lipstopfull", "윗입술 볼륨"),
        ("liptopfull", "윗입술 볼륨"),
        ("nosesideside", "코 좌우 이동"),
        ("lacrimalspinch", "눈물언덕 오므리기"),
        ("nosebridgedcurve", "콧대 곡선"),
        ("nosebridgecurve", "콧대 곡선"),
        ("nosetipshort", "코끝 짧게"),
        ("pupilsslit", "세로 동공"),
        ("eyesbags", "눈밑 지방"),
        ("cheekjowl", "볼 처짐"),
        ("chinout", "턱끝 내밀기"),
        ("julianheadjar", "줄리안 머리"),
        ("leeheadjar", "리 머리"),
        ("nosev", "코 V형"),
        ("noseg", "코 G형"),
    ];
    suffixes
        .iter()
        .find_map(|(suffix, label)| canonical.ends_with(*suffix).then_some(*label))
}

fn is_hangul(character: char) -> bool {
    matches!(character, '\u{1100}'..='\u{11ff}' | '\u{3130}'..='\u{318f}' | '\u{ac00}'..='\u{d7af}')
}

fn exact_korean_morph_label(canonical: &str) -> Option<&'static str> {
    match canonical {
        "curvy lips" => Some("입술 곡선"),
        "lip lower depth" => Some("아랫입술 내밀기"),
        "mouth open wide" => Some("입 크게 벌리기"),
        "mouth open wide 2" => Some("입 크게 벌리기 2"),
        "mouth open wide 3" => Some("입 크게 벌리기 3"),
        "mouth smile" => Some("미소"),
        "mouth smile open" => Some("입 벌린 미소"),
        "mouth smile simple" => Some("단순 미소"),
        "mouth smile simple left" => Some("단순 미소 왼쪽"),
        "mouth smile simple right" => Some("단순 미소 오른쪽"),
        "smile full face" => Some("얼굴 전체 미소"),
        "smile open full face" => Some("입 벌린 얼굴 전체 미소"),

        "tongue in out" => Some("혀 내밀기"),
        "tongue up down" => Some("혀 위아래"),
        "tongue raise lower" => Some("혀 높이"),
        "tongue bend tip" => Some("혀끝 굽히기"),
        "tongue center dip" => Some("혀 가운데 패임"),
        "tongue curl" => Some("혀 말기"),
        "eyes closed" | "eyes closed ren" | "eye closure" | "close eyes" => Some("눈 감기"),
        "eyes open" => Some("눈 뜨기"),
        "lips pucker" | "lips pucker ren" => Some("입술 오므리기"),
        "teeth bucked" => Some("앞니 내밀기"),
        "nose turned up" => Some("들창코"),
        "narrow nostrils" => Some("콧구멍 좁히기"),
        "cheek hollows" | "cheek hollow" => Some("볼 패임"),
        "cheek volume" => Some("볼 볼륨"),
        "cheek bulge" => Some("볼 돌출"),
        "cheek width" => Some("볼 너비"),
        "laugh lines" => Some("팔자 주름"),
        "crows feet" => Some("눈가 주름"),
        "vampire fangs" => Some("송곳니"),
        "nose g" => Some("코 G형"),
        "nose v" => Some("코 V형"),
        "outer mouth shape v" => Some("입 바깥선 V형"),
        "pupils slit" | "pupil slit" => Some("세로 동공"),
        "eyes bags" | "eye bags" => Some("눈밑 지방"),
        "jaw line depth" | "jawline depth" => Some("턱선 깊이"),
        "forehead top width" | "fore head top width" => Some("이마 위 너비"),
        "nose bridge curve" | "nose bridged curve" => Some("콧대 곡선"),
        "eyes cornea bulge" | "eye cornea bulge" => Some("각막 돌출"),
        "eyes width" | "eye width" => Some("눈 너비"),
        "eyes angle" | "eye angle" => Some("눈 각도"),
        "eyes height" | "eye height" => Some("눈 높이"),
        "mouth marionette lines" | "mouth marionette line" => Some("입가 주름"),
        "lips bottom full" | "lip bottom full" => Some("아랫입술 볼륨"),
        "lips top full" | "lip top full" => Some("윗입술 볼륨"),
        "lacrimals pinch" | "lacrimal pinch" => Some("눈물언덕 오므리기"),
        "nose side side" => Some("코 좌우 이동"),
        "chin out" => Some("턱끝 내밀기"),
        "cheek jowl" | "cheeks jowl" => Some("볼 처짐"),
        "lip upper size ren" | "lip upper size" => Some("윗입술 크기"),
        "nose tip short" => Some("코끝 짧게"),
        "upper lip curve 2" | "lip upper curve 2" => Some("윗입술 곡선 2"),
        "julian head ja r" | "julian head jar" => Some("줄리안 머리"),
        "lee head ja r" | "lee head jar" => Some("리 머리"),
        _ => None,
    }
}

fn morph_words(raw: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous = None;
    for character in raw.chars() {
        if character.is_ascii_alphanumeric() {
            let camel_boundary = character.is_ascii_uppercase()
                && previous.is_some_and(|value: char| value.is_ascii_lowercase());
            let number_boundary = previous
                .is_some_and(|value: char| value.is_ascii_digit() != character.is_ascii_digit());
            if (camel_boundary || number_boundary) && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(character);
            previous = Some(character);
        } else {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            previous = None;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

#[allow(clippy::too_many_lines)]
fn korean_morph_word(word: &str) -> Option<&'static str> {
    Some(match word {
        "eye" | "eyes" => "눈",
        "eyeball" | "eyeballs" => "눈알",
        "eyelid" | "eyelids" => "눈꺼풀",
        "eyelashes" | "lashes" => "속눈썹",
        "brow" | "brows" => "눈썹",
        "cornea" => "각막",
        "iris" => "홍채",
        "pupil" | "pupils" => "동공",
        "lacrimal" | "lacrimals" => "눈물언덕",
        "nose" => "코",
        "nostril" | "nostrils" => "콧구멍",
        "septum" => "비중격",
        "bridge" => "콧대",
        "ridge" => "융기",
        "philtrum" => "인중",
        "mouth" => "입",
        "lip" | "lips" => "입술",
        "teeth" => "치아",
        "tongue" => "혀",
        "uvula" => "목젖",
        "fangs" => "송곳니",
        "jaw" | "jawline" => "턱선",
        "chin" => "턱끝",
        "cheek" | "cheeks" => "볼",
        "ear" | "ears" => "귀",
        "earlobes" => "귓불",
        "face" => "얼굴",
        "head" => "머리",
        "cranium" => "두상",
        "forehead" | "fore" => "이마",
        "temples" => "관자놀이",
        "bone" | "bones" => "뼈",
        "area" => "주변",
        "flesh" => "살집",
        "muscles" => "근육",

        "left" | "l" => "왼쪽",
        "right" | "r" => "오른쪽",
        "inner" | "in" => "안쪽",
        "outer" | "out" => "바깥쪽",
        "upper" | "top" | "up" => "위",
        "lower" | "bottom" | "low" | "lo" | "down" => "아래",
        "center" | "middle" | "middlle" | "mid" => "가운데",
        "side" => "측면",
        "forward" => "앞으로",
        "back" => "뒤로",
        "height" => "높이",
        "width" => "너비",
        "gauge" | "girth" => "굵기",
        "depth" => "깊이",
        "size" | "scale" => "크기",
        "length" => "길이",
        "long" => "길게",
        "thickness" | "thick" => "두께",
        "thin" => "얇게",
        "narrow" => "좁게",
        "wide" | "wider" => "넓게",
        "large" | "larger" | "lg" => "크게",
        "small" | "smaller" => "작게",
        "high" => "높게",
        "shape" | "model" | "sculpt" => "형태",
        "define" | "defined" => "선명하게",
        "smooth" => "매끈하게",
        "flat" | "flatter" => "평평하게",
        "round" | "rounder" => "둥글게",
        "square" => "각지게",
        "heart" => "하트형",
        "oval" => "타원형",
        "almond" => "아몬드형",
        "arch" => "아치형",
        "angle" => "각도",
        "curve" | "curves" | "curvy" => "곡선",
        "slope" | "slant" | "tilt" => "기울기",
        "skew" => "비틀기",
        "twist" => "회전 비틀기",
        "rotate" | "rotation" => "회전",
        "shift" | "move" | "placement" => "위치",
        "xscale" => "가로 크기",
        "ratio" | "proportion" => "비율",
        "correction" => "보정",
        "all" => "전체",
        "volume" => "볼륨",
        "more" => "강하게",

        "crease" | "line" | "lines" | "wrinkle" => "주름",
        "dimple" => "보조개",
        "puffy" | "puff" => "도톰하게",
        "sink" | "hollows" | "recede" => "들어가게",
        "bulge" | "bump" => "돌출",
        "jowl" => "처짐",
        "squish" | "pinch" => "오므리기",
        "fold" => "접힘",
        "heavy" => "두툼하게",
        "bags" => "눈밑 지방",
        "feet" => "가장자리",
        "crows" => "눈가",
        "curl" => "말림",
        "hide" => "숨기기",
        "layer" => "층",
        "point" | "peak" | "tip" => "끝",
        "edge" | "corner" | "corners" => "가장자리",
        "bow" => "활 모양",
        "cleft" => "갈라짐",
        "pucker" | "pouty" => "오므리기",
        "smile" => "미소",
        "laugh" => "웃음",

        "afraid" => "겁먹음",
        "angry" => "화남",
        "concentrate" => "집중",
        "confused" => "당황",
        "contempt" => "경멸",
        "desire" => "욕정",
        "disgust" => "혐오",
        "excitement" => "흥분",
        "fear" => "공포",
        "flirting" => "유혹",
        "frown" => "찡그림",
        "glare" => "노려보기",
        "happy" => "행복",
        "pain" => "고통",
        "sad" => "슬픔",
        "scream" => "비명",
        "shock" => "충격",
        "snarl" => "비웃음",
        "surprise" => "놀람",
        "masculine" => "남성형",
        "feminine" => "여성형",
        "simple" => "단순",
        "bend" => "굽히기",
        "roll" => "굴리기",
        "open" => "열기",
        "closed" => "감기",
        "young" | "teen" => "어리게",
        "irregular" => "불규칙",
        "gap" => "틈",
        "bucked" => "내밀기",
        "forked" => "갈라짐",
        "slit" => "세로 동공",
        "dilate" | "dialate" => "확장",
        "flare" => "벌리기",
        "beak" => "매부리",
        "attached" => "붙이기",
        "elf" | "fairy" | "faerie" | "fae" => "요정형",
        "exotic" => "이국적으로",
        "african" => "아프리카형",
        "anime" => "애니형",
        "female" => "여성형",
        "vampire" => "흡혈귀형",
        "marionette" => "입가",
        "mousey" => "작고 뾰족하게",
        "root" => "뿌리",
        "turned" => "들린",
        "reduce" => "줄이기",
        "stretch" => "늘이기",

        "a" => "A형",
        "b" => "B형",
        "g" => "G형",
        "m" => "M형",
        "v" => "V형",
        "jar" | "ren" | "vam" | "sc" | "si" | "ja" => "",
        _ => return korean_morph_name(word),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MorphTokenRank {
    Name,

    Part,

    Region,

    Direction,

    Shape,

    Side,

    Variant,
}

fn morph_token_rank(token: &str) -> MorphTokenRank {
    match token {
        "눈" | "눈알" | "눈꺼풀" | "속눈썹" | "눈썹" | "각막" | "홍채" | "동공" | "눈물언덕"
        | "눈밑 지방" | "눈가" | "코" | "콧구멍" | "비중격" | "콧대" | "코끝" | "인중" | "입"
        | "입술" | "윗입술" | "아랫입술" | "입가" | "치아" | "앞니" | "송곳니" | "혀" | "목젖"
        | "턱선" | "위턱" | "아래턱" | "턱끝" | "볼" | "광대뼈" | "귀" | "귓불" | "얼굴"
        | "머리" | "두상" | "이마" | "관자놀이" | "세로 동공" => MorphTokenRank::Part,
        "주변" | "전체" => MorphTokenRank::Region,

        "안쪽" | "바깥쪽" | "위" | "아래" | "가운데" | "측면" => {
            MorphTokenRank::Direction
        }
        "왼쪽" | "오른쪽" => MorphTokenRank::Side,
        "A형" | "B형" | "G형" | "M형" | "V형" => MorphTokenRank::Variant,
        _ => MorphTokenRank::Shape,
    }
}

fn order_korean_morph_tokens(mut tokens: Vec<(String, MorphTokenRank)>) -> String {
    if tokens
        .iter()
        .any(|(token, _)| matches!(token.as_str(), "윗입술" | "아랫입술" | "입술"))
    {
        tokens.retain(|(token, _)| token != "입");
    }

    if tokens.iter().any(|(token, _)| token == "좌우 이동") {
        tokens.retain(|(token, _)| token != "위치");
    }
    tokens.sort_by_key(|(_, rank)| *rank);
    tokens.dedup_by(|left, right| left.0 == right.0);
    tokens
        .into_iter()
        .map(|(token, _)| token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn korean_morph_name(word: &str) -> Option<&'static str> {
    Some(match word {
        "adrianna" => "아드리아나",
        "aiko" => "아이코",
        "alessa" => "알레사",
        "ana" => "아나",
        "aneta" => "아네타",
        "beau" => "보",
        "candy" => "캔디",
        "carmen" => "카르멘",
        "daisy" => "데이지",
        "danika" => "다니카",
        "darius" => "다리우스",
        "destiny" => "데스티니",
        "eisa" => "에이사",
        "gia" => "지아",
        "gianni" => "지아니",
        "greta" => "그레타",
        "hector" => "헥터",
        "izarra" => "이자라",
        "jennifer" => "제니퍼",
        "josie" => "조시",
        "julian" => "줄리안",
        "kimi" => "키미",
        "kori" => "코리",
        "lee" => "리",
        "lexi" => "렉시",
        "lilith" => "릴리스",
        "lin" => "린",
        "lorraine" => "로레인",
        "lucille" => "루실",
        "lyric" => "리릭",
        "mack" => "맥",
        "madeline" => "매들린",
        "maria" => "마리아",
        "mei" => "메이",
        "mia" => "미아",
        "michael" => "마이클",
        "monique" => "모니크",
        "neomi" => "네오미",
        "norma" => "노마",
        "nyssa" => "니사",
        "olympia" => "올림피아",
        "parisa" => "파리사",
        "ryze" => "라이즈",
        "scott" => "스콧",
        "stephanie" => "스테파니",
        "sumiko" => "스미코",
        "tara" => "타라",
        "taric" => "타릭",
        "vianne" => "비안",
        "victoria" => "빅토리아",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn korean_morphs_localize_recognized_tokens_and_keep_custom_names_english() {
        let recognized_samples = [
            "CurvyLips",
            "Lip Lower Depth",
            "Eyes Almond Inner Left",
            "Eyelids Lower Puffy",
            "Nostrils Shape Top Angle",
            "Cheek Bones Width Upper",
            "Aiko 6 Head",
            "SC_AlessaEars",
            "Mouth Open Wide 3",
            "Smile Open Full Face",
            "FutureUnknownVendorMorph",
        ];
        for raw in recognized_samples {
            let localized = morph_label(Locale::Korean, raw);
            assert!(localized.chars().any(is_hangul), "{raw} -> {localized}");
            assert_ne!(localized, raw);
            assert_eq!(morph_label(Locale::English, raw), raw);
        }

        assert_eq!(morph_label(Locale::Korean, "123"), "123");
    }

    #[test]
    fn important_morph_names_use_intuitive_korean_phrasing() {
        assert_eq!(morph_label(Locale::Korean, "CurvyLips"), "입술 곡선");
        assert_eq!(
            morph_label(Locale::Korean, "Lip Lower Depth"),
            "아랫입술 내밀기"
        );
        assert_eq!(
            morph_label(Locale::Korean, "Mouth Open Wide 3"),
            "입 크게 벌리기 3"
        );
        assert_eq!(
            morph_label(Locale::Korean, "Smile Open Full Face"),
            "입 벌린 얼굴 전체 미소"
        );
    }

    #[test]
    fn korean_morph_labels_follow_one_token_order() {
        for (raw, expected) in [
            ("Cheek Bones Width Upper", "광대뼈 위 너비"),
            ("Cheek Bones Upper Depth", "광대뼈 위 깊이"),
            ("Nostrils Shape Bottom", "콧구멍 아래 형태"),
            ("Nostrils Bottom Shape", "콧구멍 아래 형태"),
            ("Eyes Round Lower", "눈 아래 둥글게"),
            ("Eyes Lower Shape", "눈 아래 형태"),
            ("Lower Mouth Puffy", "입 아래 도톰하게"),
        ] {
            assert_eq!(morph_label(Locale::Korean, raw), expected, "{raw}");
        }

        assert_eq!(
            morph_label(Locale::Korean, "Eyes Almond Inner Left"),
            "눈 안쪽 아몬드형 왼쪽"
        );
        assert_eq!(morph_label(Locale::Korean, "Aiko 6 Head"), "아이코 머리 6");
        assert_eq!(
            morph_label(Locale::Korean, "Cheek Bones Shape 1"),
            "광대뼈 형태 1"
        );

        assert_eq!(
            morph_label(Locale::Korean, "MouthLowLipDefine"),
            "아랫입술 선명하게"
        );
        assert_eq!(
            morph_label(Locale::Korean, "Teeth Lower Jaw Move Side-Side"),
            "치아 아래턱 좌우 이동"
        );
    }

    #[test]
    fn built_in_expressions_are_named_in_korean_and_japanese() {
        for (internal_name, raw, korean, japanese) in [
            ("PHMAngry", "Angry", "화남", "怒り"),
            ("PHMSad", "Sad", "슬픔", "悲しみ"),
            ("PHMConfused", "Confused", "당황", "困惑"),
            ("PHMFlirting", "Flirting", "유혹", "誘惑"),
            ("PHMContempt", "Contempt", "경멸", "軽蔑"),
            ("PHMSurprise", "Surprise", "놀람", "驚き"),
            ("PHMSnarlLeft", "Snarl Left", "비웃음 왼쪽", "冷笑 左"),
            (
                "PHMFlirtingFeminineL",
                "Flirting Feminine Left",
                "유혹 여성형 왼쪽",
                "誘惑 女性的 左",
            ),
            (
                "CTRLMouthSmileSimple",
                "Mouth Smile Simple",
                "단순 미소",
                "シンプルな笑顔",
            ),
            ("CTRLTongueIn-Out", "Tongue In-Out", "혀 내밀기", "舌を出す"),
        ] {
            let stable_id = format!("builtin:{internal_name}");
            assert_eq!(
                morph_label_for(Locale::Korean, &stable_id, raw),
                korean,
                "{raw}"
            );
            assert_eq!(
                morph_label_for(Locale::Japanese, &stable_id, raw),
                japanese,
                "{raw}"
            );
            assert_eq!(morph_label_for(Locale::English, &stable_id, raw), raw);
        }
    }

    #[test]
    fn morph_measurement_terms_are_unified_across_the_vocabulary() {
        assert_eq!(morph_label(Locale::Korean, "Eyes Width"), "눈 너비");
        assert_eq!(morph_label(Locale::Korean, "Brows Size"), "눈썹 굵기");
        assert_eq!(morph_label(Locale::Korean, "Lips Top Full"), "윗입술 볼륨");
        assert_eq!(morph_label(Locale::Korean, "SmileMuscles"), "미소 근육");
        assert_eq!(
            morph_label(Locale::Korean, "FaceSmoothAll"),
            "얼굴 전체 매끈하게"
        );
    }

    #[test]
    fn cheekbone_compounds_and_soft_cheek_controls_use_distinct_anatomy_terms() {
        assert_eq!(
            morph_label(Locale::Korean, "Cheek Bones Width Upper"),
            "광대뼈 위 너비"
        );
        assert_eq!(morph_label(Locale::Korean, "Cheek Volume"), "볼 볼륨");
        assert_eq!(morph_label(Locale::Korean, "Cheek Jowl"), "볼 처짐");
        assert_eq!(morph_label(Locale::Korean, "Cheek Hollow"), "볼 패임");
    }

    #[test]
    fn audited_vam_face_morphs_have_exact_context_safe_korean_names() {
        let audited = [
            ("PHMNoseG", "Nose G", "코 G형"),
            ("FHMJulianHeadJaR", "Julian Head (JaR)", "줄리안 머리"),
            ("FHMLeeHeadJaR", "Lee Head (JaR)", "리 머리"),
            ("PHMPupilsSlit", "Pupils Slit", "세로 동공"),
            ("PHMEyesBags", "Eyes Bags", "눈밑 지방"),
            ("PHMJawLineDepth", "Jaw Line Depth", "턱선 깊이"),
            ("PHMForeHeadTopWidth", "ForeHead Top Width", "이마 위 너비"),
            ("PHMNoseBridgeCurve", "Nose Bridge Curve", "콧대 곡선"),
            ("PHMEyesCorneaBulge", "Eyes Cornea Bulge", "각막 돌출"),
            (
                "PHMMouthMarionetteLines",
                "Mouth Marionette lines",
                "입가 주름",
            ),
            ("PHMLipsBottomFull", "Lips Bottom Full", "아랫입술 볼륨"),
            ("PHMLipsTopFull", "Lips Top Full", "윗입술 볼륨"),
            ("PHMLacrimalsPinch", "Lacrimals Pinch", "눈물언덕 오므리기"),
            ("PHMNoseSideSide", "Nose Side-Side", "코 좌우 이동"),
            ("PHMChinOut", "ChinOut", "턱끝 내밀기"),
            ("PHMCheekJowl", "Cheek Jowl", "볼 처짐"),
            ("PHMLipUpperSizeRen", "Lip Upper Size (ren)", "윗입술 크기"),
            ("PHMNoseTipShort", "NoseFlatter", "코끝 짧게"),
            ("PHMUpperLipCurve2", "Upper Lip Curve", "윗입술 곡선 2"),
            ("PHMNoseV", "Nose V", "코 V형"),
            (
                "PHMOuterMouthShapeV",
                "Outer Mouth Shape V",
                "입 바깥선 V형",
            ),
        ];

        for (stable_id, raw, expected) in audited {
            let localized = morph_label_for(Locale::Korean, stable_id, raw);
            assert_eq!(localized, expected, "{stable_id} / {raw}");
            assert!(
                !localized.contains("고유 형태"),
                "{stable_id} -> {localized}"
            );

            assert!(
                !localized
                    .split(|character: char| !character.is_ascii_alphabetic())
                    .any(|run| run.len() > 1),
                "{stable_id} -> {localized}"
            );
            assert_eq!(morph_label_for(Locale::English, stable_id, raw), raw);
        }
    }

    #[test]
    fn stable_morph_id_overrides_unreliable_display_metadata() {
        assert_eq!(
            morph_label_for(Locale::Korean, "vendor.PHMJawLineDepth", "Depth"),
            "턱선 깊이"
        );
        assert_eq!(
            morph_label_for(Locale::Korean, "vendor/PHMOuterMouthShapeV", "Shape"),
            "입 바깥선 V형"
        );
        assert_eq!(
            morph_label_for(Locale::English, "vendor.PHMJawLineDepth", "Depth"),
            "Depth"
        );
    }

    #[test]
    fn compositional_face_terms_use_korean_anatomy_not_literal_word_order() {
        let cases = [
            ("JawLineWidth", "턱선 너비"),
            ("ForeHeadDepth", "이마 깊이"),
            ("NoseBridgeWidth", "콧대 너비"),
            ("NoseTipHeight", "코끝 높이"),
            ("Eyes Bags Size", "눈밑 지방 크기"),
            ("Upper Lip Height", "윗입술 높이"),
            ("Lips Lower Width", "아랫입술 너비"),
            ("Nose Side-Side More", "코 좌우 이동 강하게"),
            ("JulianHeadJaR", "줄리안 머리"),
        ];
        for (raw, expected) in cases {
            assert_eq!(morph_label(Locale::Korean, raw), expected, "{raw}");
        }
    }

    fn leaks_english(localized: &str) -> bool {
        let mut characters = localized.chars().peekable();
        while let Some(character) = characters.next() {
            if !character.is_ascii_alphabetic() {
                continue;
            }
            let names_a_shape = character.is_ascii_uppercase()
                && characters.peek() == Some(&'형')
                && !localized
                    .chars()
                    .zip(localized.chars().skip(1))
                    .any(|(before, after)| after == character && before.is_ascii_alphabetic());
            if !names_a_shape {
                return true;
            }
        }
        false
    }

    #[test]
    fn a_shape_letter_is_korean_and_an_english_word_is_not() {
        for korean in ["V형 턱선", "M형 눈썹", "눈 아래 형태 M형", "코 너비"] {
            assert!(!leaks_english(korean), "rejected {korean}");
        }
        for leaked in ["Eye Shape Bottom", "눈 Shape", "코 Width", "AB형"] {
            assert!(leaks_english(leaked), "accepted {leaked}");
        }
    }

    #[test]
    fn korean_is_the_default_locale() {
        assert_eq!(Locale::default(), Locale::Korean);
        assert_eq!(Locale::Korean.selector_label(), "한국어");
        assert_eq!(text(Locale::default(), TextKey::PinPairsPlaced), "핀");
    }

    #[test]
    fn locale_selector_uses_native_names_for_every_language() {
        assert_eq!(
            Locale::ALL.map(Locale::selector_label),
            [
                "한국어",
                "English",
                "日本語",
                "简体中文",
                "繁體中文",
                "Español",
                "हिन्दी",
                "Português",
                "বাংলা",
                "Français",
                "Deutsch",
                "Русский",
                "Bahasa Indonesia",
                "Tiếng Việt",
                "ไทย",
            ]
        );
    }

    #[test]
    fn a_system_tag_picks_the_pack_that_matches_it() {
        for (tag, expected) in [
            ("ko-KR", Locale::Korean),
            ("en-GB", Locale::English),
            ("ja-JP", Locale::Japanese),
            ("pt-BR", Locale::Portuguese),
            ("de-AT", Locale::German),
            ("id-ID", Locale::Indonesian),
            ("in-ID", Locale::Indonesian),
            ("bn-BD", Locale::Bengali),
            ("vi", Locale::Vietnamese),
        ] {
            assert_eq!(Locale::from_bcp47(tag), Some(expected), "{tag}");
        }

        for tag in ["zh-CN", "zh-Hans", "zh-SG", "zh"] {
            assert_eq!(Locale::from_bcp47(tag), Some(Locale::ZhHans), "{tag}");
        }
        for tag in ["zh-TW", "zh-Hant", "zh-HK", "zh-MO", "zh-Hant-TW"] {
            assert_eq!(Locale::from_bcp47(tag), Some(Locale::ZhHant), "{tag}");
        }

        for tag in ["pl-PL", "sv-SE", "", "xx"] {
            assert_eq!(Locale::from_bcp47(tag), None, "{tag}");
        }
        assert_eq!(Locale::system_default(), Locale::system_default());
    }

    #[test]
    fn every_pack_meets_the_coverage_it_promises() {
        const FLOORS: &[(Locale, f64)] = &[
            (Locale::Japanese, 1.0),
            (Locale::ZhHans, 1.0),
            (Locale::ZhHant, 1.0),
            (Locale::Spanish, 1.0),
            (Locale::Hindi, 1.0),
            (Locale::Portuguese, 1.0),
            (Locale::Bengali, 1.0),
            (Locale::French, 1.0),
            (Locale::German, 1.0),
            (Locale::Russian, 1.0),
            (Locale::Indonesian, 1.0),
            (Locale::Vietnamese, 1.0),
            (Locale::Thai, 1.0),
        ];

        let contract = [Locale::Korean, Locale::English];
        for locale in Locale::ALL {
            let floored = FLOORS.iter().any(|(named, _)| *named == locale);
            assert_eq!(
                floored,
                !contract.contains(&locale),
                "{locale:?} is missing from the coverage table"
            );
        }

        for (locale, floor) in FLOORS {
            let missing: Vec<TextKey> = ALL_TEXT_KEYS
                .iter()
                .copied()
                .filter(|key| pack_label(*locale, *key).is_none())
                .collect();
            let total = ALL_TEXT_KEYS.len();
            let covered = total - missing.len();
            let coverage = covered as f64 / total as f64;
            eprintln!("{locale:?} pack coverage: {covered}/{total}");
            assert!(
                coverage >= *floor,
                "{locale:?} coverage dropped to {covered}/{total}; missing: {missing:?}"
            );
        }
    }

    fn pack_label(locale: Locale, key: TextKey) -> Option<&'static str> {
        match locale {
            Locale::Korean => Some(ko::label(key)),
            Locale::English => Some(en::label(key)),
            Locale::Japanese => ja::label(key),
            Locale::ZhHans => zh_hans::label(key),
            Locale::ZhHant => zh_hant::label(key),
            Locale::Spanish => es::label(key),
            Locale::Hindi => hi::label(key),
            Locale::Portuguese => pt::label(key),
            Locale::Bengali => bn::label(key),
            Locale::French => fr::label(key),
            Locale::German => de::label(key),
            Locale::Russian => ru::label(key),
            Locale::Indonesian => id::label(key),
            Locale::Vietnamese => vi::label(key),
            Locale::Thai => th::label(key),
        }
    }

    #[test]
    fn every_locale_prefers_its_pack_and_falls_back_to_english() {
        for locale in Locale::ALL {
            for key in ALL_TEXT_KEYS {
                let expected = match pack_label(locale, *key) {
                    Some(label) => label,
                    None => en::label(*key),
                };
                assert_eq!(text(locale, *key), expected, "{locale:?} {key:?}");
            }
        }
    }

    #[test]
    fn no_pack_ships_an_empty_or_mojibake_label() {
        for locale in Locale::ALL {
            for key in ALL_TEXT_KEYS {
                let Some(label) = pack_label(locale, *key) else {
                    continue;
                };
                assert!(!label.is_empty(), "{locale:?} {key:?} is empty");
                assert!(
                    !label.contains(char::REPLACEMENT_CHARACTER),
                    "{locale:?} {key:?} carries a replacement character"
                );
            }
        }
    }

    #[test]
    fn japanese_text_lookup_prefers_the_pack_and_falls_back_to_english() {
        for key in ALL_TEXT_KEYS {
            let expected = match ja::label(*key) {
                Some(label) => label,
                None => en::label(*key),
            };
            assert_eq!(text(Locale::Japanese, *key), expected, "{key:?}");
        }
    }

    #[test]
    fn japanese_workflow_labels_are_exact() {
        assert_eq!(text(Locale::Japanese, TextKey::PinPairsPlaced), "ピン");
        assert_eq!(text(Locale::Japanese, TextKey::DetailCorrection), "編集");
        assert_eq!(text(Locale::Japanese, TextKey::Save), "保存");
        assert_eq!(
            text(Locale::Japanese, TextKey::Generate),
            "顔フィッティング"
        );
        assert_eq!(
            text(Locale::Japanese, TextKey::TransformGroups),
            "変形パーツ"
        );
        assert_eq!(text(Locale::Japanese, TextKey::WindowClose), "閉じる");
    }

    #[test]
    fn english_presents_raw_labels_for_imported_vam_morphs() {
        assert_eq!(
            morph_label_for(Locale::English, "vendor.PHMJawLineDepth", "Jaw Line Depth"),
            "Jaw Line Depth"
        );
    }

    #[test]
    fn japanese_builtin_morph_names_resolve_by_stable_id() {
        assert_eq!(
            morph_label_for(
                Locale::Japanese,
                "builtin:PHMJawLineDepth",
                "Jaw Line Depth"
            ),
            "フェイスラインの奥行き"
        );
        assert_eq!(
            morph_label_for(Locale::Japanese, "builtin:Brow Arch", "Brow Shape Middle"),
            "眉中央の形"
        );
        assert_eq!(
            morph_label_for(Locale::Japanese, "builtin:Eyelids Lower Puffy", "whatever"),
            "涙袋"
        );
        assert_eq!(
            morph_label_for(Locale::Japanese, "builtin:MouthShape10", "MouthShape10"),
            "口の形 10"
        );

        assert_eq!(
            morph_label_for(Locale::English, "builtin:PHMJawLineDepth", "Jaw Line Depth"),
            "Jaw Line Depth"
        );

        assert_eq!(
            morph_label_for(
                Locale::Japanese,
                "builtin:SomeFutureMorph",
                "Some Future Morph"
            ),
            "Some Future Morph"
        );
    }

    fn assert_builtin_pack_japanese_coverage(sex: vkit_core::vam::SkinSex) {
        const REQUIRED_COVERAGE: f64 = 1.0;

        const UNNAMED: &str = "\u{0}unnamed";

        let index = crate::vam_morph_cache::builtin_morph_names(sex).collect::<Vec<_>>();
        assert!(!index.is_empty());
        let mut named = Vec::new();
        let mut missing = Vec::new();
        for internal_name in &index {
            let stable_id = format!("builtin:{internal_name}");
            let localized = morph_label_for(Locale::Japanese, &stable_id, UNNAMED);
            if localized == UNNAMED {
                missing.push((*internal_name).to_owned());
            } else {
                named.push(localized.into_owned());
            }
        }
        let coverage = named.len() as f64 / index.len() as f64;
        eprintln!(
            "Japanese {sex:?} built-in morph coverage: {}/{} ({} missing)",
            named.len(),
            index.len(),
            missing.len()
        );
        assert!(
            coverage >= REQUIRED_COVERAGE,
            "Japanese coverage dropped to {}/{}; missing: {missing:?}",
            named.len(),
            index.len()
        );

        let mut sorted = named.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), named.len(), "duplicate Japanese morph names");
    }

    #[test]
    fn japanese_names_imported_morphs_the_authored_table_never_saw() {
        for (raw, expected) in [
            ("Nose Width", "鼻の幅"),
            ("Brow Inner Height Left", "眉内側の高さ 左"),
            ("Cheek Bones Define", "頬骨の強調"),
            ("Eyes Almond Inner Left", "目内側のアーモンド型 左"),
            ("Mouth Open Wide 3", "口の開き広く 3"),
        ] {
            let localized = morph_label_for(Locale::Japanese, "vam-var:Someone.Pack.1:/x.vmi", raw);
            assert_eq!(localized, expected, "{raw}");
        }

        assert_eq!(
            morph_label_for(Locale::Japanese, "vam-pair:/x.vmi", "Unlisted Morph"),
            "Unlisted Morph"
        );
    }

    #[test]
    fn every_localized_builtin_table_is_complete_and_unambiguous() {
        let keys: Vec<&str> = ja_morphs::ALL_KEYS.to_vec();
        for locale in Locale::ALL {
            let Some(lookup) = builtin_name_table(locale) else {
                continue;
            };
            let mut seen: std::collections::BTreeMap<&'static str, &str> =
                std::collections::BTreeMap::new();
            for key in &keys {
                let name =
                    lookup(key).unwrap_or_else(|| panic!("{locale:?} has no name for {key}"));
                assert!(
                    !name.trim().is_empty(),
                    "{locale:?} names {key} with nothing"
                );
                if let Some(previous) = seen.insert(name, key) {
                    assert_eq!(
                        previous, *key,
                        "{locale:?} gives {previous} and {key} the same name: {name}"
                    );
                }
            }
        }
    }

    #[test]
    fn japanese_g2f_builtin_pack_is_fully_named() {
        assert_builtin_pack_japanese_coverage(vkit_core::vam::SkinSex::Female);
    }

    #[test]
    fn japanese_g2m_builtin_pack_is_fully_named() {
        assert_builtin_pack_japanese_coverage(vkit_core::vam::SkinSex::Male);
    }

    #[test]
    fn current_korean_workflow_labels_are_exact() {
        assert_eq!(text(Locale::Korean, TextKey::FaceMatch), "생성");
        assert_eq!(text(Locale::Korean, TextKey::OpenScan), "스캔용 헤드");
        assert_eq!(
            text(Locale::Korean, TextKey::AddHeadFile),
            "헤드 파일을 추가하세요"
        );
        assert_eq!(
            text(Locale::Korean, TextKey::ScanSymmetry),
            "스캔 헤드 대칭"
        );
        assert_eq!(text(Locale::Korean, TextKey::View), "보기 옵션");
        assert_eq!(text(Locale::Korean, TextKey::Generate), "얼굴 맞추기");
        assert_eq!(text(Locale::English, TextKey::Generate), "Fit Face");
        assert_eq!(text(Locale::Korean, TextKey::Next), "저장하기");
        assert_eq!(text(Locale::English, TextKey::Next), "Save");
        assert_eq!(text(Locale::Korean, TextKey::TransformGroups), "변형 부위");
        assert_eq!(text(Locale::English, TextKey::TransformGroups), "Parts");
        assert_eq!(text(Locale::Korean, TextKey::Skin), "피부");
        assert_eq!(text(Locale::Korean, TextKey::EyesOpen), "뜬 눈");
        assert_eq!(text(Locale::Korean, TextKey::EyesClosed), "감은 눈");
        assert_eq!(text(Locale::Korean, TextKey::MorphCategoryHead), "얼굴");
        assert_eq!(text(Locale::Korean, TextKey::ResultPreview), "저장");
        assert_eq!(text(Locale::Korean, TextKey::Eyelashes), "속눈썹");
    }

    #[test]
    fn compact_workflow_viewport_and_sculpt_labels_are_exact() {
        let tabs = [
            (TextKey::FaceMatch, "생성", "Create"),
            (TextKey::Save, "저장", "Save"),
        ];
        for (key, korean, english) in tabs {
            assert_eq!(text(Locale::Korean, key), korean);
            assert_eq!(text(Locale::English, key), english);
        }

        assert_eq!(text(Locale::Korean, TextKey::SculptXSymmetry), "좌우대칭");
        assert_eq!(text(Locale::English, TextKey::SculptGrab), "Grab");
        assert_eq!(text(Locale::Korean, TextKey::SculptSmooth), "스무스");
        assert_eq!(
            text(Locale::English, TextKey::SculptPushPull),
            "Push · Pull"
        );
        assert_eq!(
            text(Locale::Korean, TextKey::ShortcutSculptPushPull),
            "Ctrl / Alt + 드래그"
        );
        assert_eq!(text(Locale::Korean, TextKey::SculptBrushMove), "그랩");
        assert_eq!(text(Locale::Korean, TextKey::SculptBrushRestore), "복구");
        assert_eq!(
            text(Locale::English, TextKey::SculptBrushRestore),
            "Restore"
        );
        assert_eq!(text(Locale::Japanese, TextKey::SculptBrushRestore), "復元");

        assert_eq!(
            morph_label(Locale::Korean, "Mouth Smile Open"),
            "입 벌린 미소"
        );
    }

    #[test]
    fn output_and_window_controls_have_complete_compact_labels() {
        let compact = [
            (TextKey::On, "켜기", "On"),
            (TextKey::OverwriteTitle, "파일 덮어쓰기", "Overwrite File"),
            (TextKey::Overwrite, "덮어쓰기", "Overwrite"),
            (TextKey::WindowMinimize, "최소화", "Minimize"),
            (TextKey::WindowMaximize, "최대화", "Maximize"),
            (TextKey::WindowRestore, "이전 크기", "Restore"),
            (TextKey::WindowClose, "닫기", "Close"),
        ];
        for (key, korean, english) in compact {
            assert_eq!(text(Locale::Korean, key), korean);
            assert_eq!(text(Locale::English, key), english);
        }
        assert_eq!(text(Locale::Korean, TextKey::ScanOverlay), "겹쳐보기");
        assert_eq!(text(Locale::Korean, TextKey::SculptEyes), "눈알");
    }

    #[test]
    fn consolidated_detail_and_viewport_labels_are_concise_and_complete() {
        let labels = [
            (TextKey::DetailCorrection, "조형", "Sculpt"),
            (TextKey::TextureStage, "텍스처", "Texture"),
            (TextKey::SolidColor, "솔리드", "Solid"),
            (TextKey::TextureSkin, "프리셋", "Preset"),
            (TextKey::CameraSettings, "카메라 설정", "Camera Settings"),
            (TextKey::Perspective, "원근", "Perspective"),
            (TextKey::Orthographic, "직교", "Orthographic"),
            (TextKey::Fov, "시야각", "FOV"),
            (TextKey::ResetCamera, "카메라 초기화", "Reset Camera"),
            (TextKey::WireframeSettings, "와이어프레임", "Wireframe"),
            (TextKey::XraySettings, "X-ray", "X-ray"),
        ];

        for (key, korean, english) in labels {
            assert_eq!(text(Locale::Korean, key), korean);
            assert_eq!(text(Locale::English, key), english);
        }
    }

    #[test]
    fn eye_width_reads_as_width_in_every_morph_mapping_path() {
        for raw in ["Eyes Width", "Eye Width", "EyesWidth", "EyeWidth"] {
            assert_eq!(morph_label(Locale::Korean, raw), "눈 너비", "{raw}");
        }
        assert_eq!(
            morph_label_for(Locale::Korean, "vendor.PHMEyesWidth", "Width"),
            "눈 너비"
        );
        assert_eq!(
            morph_label_for(Locale::Korean, "vendor.PHMEyeWidth", "Width"),
            "눈 너비"
        );

        assert_eq!(
            morph_label_for(Locale::Korean, "builtin:PHMEyesWidthL", "Eyes Width Left"),
            "눈 너비 왼쪽"
        );
    }

    #[test]
    fn lighting_controls_follow_the_selected_locale() {
        assert_eq!(text(Locale::Korean, TextKey::Lighting), "조명");
        assert_eq!(text(Locale::English, TextKey::Lighting), "Lighting");
        assert_eq!(text(Locale::Korean, TextKey::Brightness), "밝기");
        assert_eq!(text(Locale::English, TextKey::Brightness), "Brightness");
    }

    #[test]
    fn workflow_refinement_copy_is_available_in_both_locales() {
        let labels = [
            (TextKey::Size, "크기", "Size"),
            (TextKey::LightRotation, "조명 회전", "Light Rotation"),
            (
                TextKey::VaMMetadataPoseMorph,
                "포즈 모프로 등록",
                "Register as Pose Morph",
            ),
            (
                TextKey::VaMMetadataBoneCorrection,
                "뼈대 보정",
                "Skeleton Correction",
            ),
        ];
        for (key, korean, english) in labels {
            assert_eq!(text(Locale::Korean, key), korean);
            assert_eq!(text(Locale::English, key), english);
        }

        assert_eq!(text(Locale::English, TextKey::ResetMorphs), "Reset morphs");
        assert_eq!(text(Locale::Korean, TextKey::ResetMorphs), "모프 초기화");
        assert_eq!(
            text(Locale::English, TextKey::ResetSculpt),
            "Reset Deformation"
        );
        assert_eq!(text(Locale::Korean, TextKey::ResetSculpt), "변형 초기화");

        assert_eq!(
            text(Locale::English, TextKey::ExportComplete),
            "Export complete"
        );
    }

    #[test]
    fn help_rows_keep_functions_and_shortcuts_separate() {
        let rows = [
            (
                TextKey::HelpPlace,
                TextKey::ShortcutPlace,
                "핀 배치",
                "마우스 좌클릭",
            ),
            (
                TextKey::HelpMove,
                TextKey::ShortcutMove,
                "핀 이동",
                "Alt + 드래그",
            ),
            (
                TextKey::HelpDelete,
                TextKey::ShortcutDelete,
                "핀 제거",
                "마우스 우클릭",
            ),
            (
                TextKey::HelpOrbit,
                TextKey::ShortcutOrbit,
                "회전",
                "휠클릭 드래그",
            ),
            (
                TextKey::HelpPan,
                TextKey::ShortcutPan,
                "이동",
                "Shift + 휠클릭 드래그",
            ),
            (
                TextKey::HelpZoom,
                TextKey::ShortcutZoom,
                "확대/축소",
                "마우스 휠",
            ),
            (
                TextKey::HelpLight,
                TextKey::ShortcutLight,
                "조명 회전",
                "Shift + 우클릭 드래그",
            ),
            (
                TextKey::HelpUndo,
                TextKey::ShortcutUndo,
                "실행 취소",
                "Ctrl+Z",
            ),
        ];
        for (function, shortcut, expected_function, expected_shortcut) in rows {
            assert_eq!(text(Locale::Korean, function), expected_function);
            assert_eq!(text(Locale::Korean, shortcut), expected_shortcut);
        }
    }

    #[test]
    fn every_key_has_nonempty_uncorrupted_translations_in_every_locale() {
        for key in ALL_TEXT_KEYS {
            for locale in Locale::ALL {
                let value = text(locale, *key);
                assert!(!value.trim().is_empty(), "{locale:?} {key:?}");

                assert!(
                    !value.contains('\u{fffd}'),
                    "corrupt {locale:?} text for {key:?}: {value}"
                );

                if matches!(
                    locale,
                    Locale::Korean | Locale::Japanese | Locale::ZhHans | Locale::ZhHant
                ) {
                    assert!(
                        !value.contains('?'),
                        "corrupt {locale:?} text for {key:?}: {value}"
                    );
                }
            }
        }
    }
}
