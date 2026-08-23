use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VarMetadataField {
    Creator,
    Package,

    Version,
    License,
    Description,
    Credits,
    Instructions,
    PromotionalLink,
}

#[derive(Debug)]
pub enum Action {
    SetLocale(Locale),
    RequestTab(Tab),
    RequestOpenScan,
    UnloadScan,
    RequestTextureImageBrowse(TextureSourceMode),

    RequestVaMRootBrowse,
    SetVaMRoot(PathBuf),
    RefreshVaMCatalog,
    SelectVaMEditSource(String),

    SelectBaseFace,

    AddHairPart {
        provider_name: String,
    },
    RemoveHairPart(u64),
    SetHairPartName {
        id: u64,
        name: String,
    },
    SetHairPartStyleJoints {
        id: u64,
        on: bool,
    },
    SetHairExportSexes(crate::hair_project::HairExportSexes),
    ActivateHairPart {
        id: u64,
        additive: bool,
    },
    ToggleHairPartVisible(u64),
    SoloHairPart(u64),

    AddReferenceImage(std::path::PathBuf),
    RemoveReferenceImage(u64),
    SelectReferenceImage(Option<u64>),
    ToggleReferenceImageVisible(u64),
    ReorderReferenceImage {
        id: u64,
        toward_front: bool,
    },
    DragReferenceImage {
        id: u64,
        viewport: egui::Rect,
        delta: egui::Vec2,
    },
    ResizeReferenceImage {
        id: u64,
        height_share: f32,
    },
    SetReferenceImageOpacity {
        id: u64,
        opacity: f32,
    },
    NoteReferenceImageAspect {
        id: u64,
        aspect: f32,
    },
    SetHairTool(crate::hair_project::HairTool),
    SetHairBrushRadius(f32),
    SetHairBrushStrength(f32),
    SelectHairParamGroup(crate::hair_settings::HairParamGroup),
    PlantHairStrands {
        part_id: u64,
        scalp_indices: Vec<u32>,
    },
    UnplantHairStrands {
        part_id: u64,
        scalp_indices: Vec<u32>,
    },
    SetHairPartSegments {
        id: u64,
        segments: usize,
    },
    ScaleHairStrands {
        part_id: u64,
        scalp_indices: Vec<u32>,
        factor: f32,
    },
    SetHairStrandPoints {
        part_id: u64,
        strands: Vec<(u32, Vec<[f32; 3]>)>,
    },
    PaintMorphMask {
        vertices: Vec<(u32, f32)>,
        target: f32,
        amount: f32,
        begins_step: bool,
    },
    AddAppearanceLayer,
    SelectAppearanceLayer(u64),
    SetAppearanceLayerVisible {
        id: u64,
        visible: bool,
    },
    RaiseAppearanceLayer(u64),
    LowerAppearanceLayer(u64),
    RemoveAppearanceLayer(u64),
    ExportHairPart,
    ConfirmHairOverwrite,
    CancelHairOverwrite,
    SetHairParam {
        id: u64,
        key: &'static str,
        value: f32,
    },
    SetHairColorChannel {
        id: u64,
        key: &'static str,
        channel: usize,
        value: f32,
    },
    ResetHairParams(u64),
    ResetHairShapes,
    AddHairScalp(String),
    SetHairScalpMesh {
        id: u64,
        mesh: String,
    },
    SetHairScalpTexture {
        id: u64,
        texture: crate::hair_project::HairScalpTexture,
    },
    CopyHairSettings(u64),
    PasteHairSettings(u64),
    SetHairExportName(String),
    EndHairStroke,
    SetHairExportCreator(String),
    MirrorHairPart(u64),
    DuplicateHairPart(u64),

    FreezeEyeGaze([f32; 2]),
    BeginDirectEdit,
    SetFigureSex(FigureSex),
    LoadScan(PathBuf),
    FinishScanImport {
        path: PathBuf,
        outcome: Result<PreparedScan, String>,
    },
    LoadTemplate(PathBuf),
    LoadResult(PathBuf),

    FinishWorkspaceLoad {
        path: PathBuf,
        outcome: WorkspaceLoadOutcome,
    },

    AutoAlign,

    PlacementReset,

    BeginAlignmentTransform,
    CommitAlignmentTransform,
    CancelAlignmentTransform,
    SetScale([f64; 3]),
    SetScaleLinked(bool),
    SetScaleAxis {
        axis: usize,
        value: f64,
    },
    SetPosition {
        axis: usize,
        value_cm: f64,
    },
    SetRotation {
        axis: usize,
        value_degrees: f64,
    },
    ToggleXMirror(bool),

    RequestScanSymmetry(ScanSymmetry),

    SetTextureExportPrefix(String),
    ConfirmTextureOverwrite,
    CancelTextureOverwrite,
    ConfirmScanSymmetry,

    CancelScanSymmetry,
    SetEyeState(EyeState),
    ResetCamera,
    SetStandardView(StandardView),
    ToggleProjection,
    SetFov(f32),
    SetBaseViewMode(BaseViewMode),
    SetSurfaceSmoothPasses(u8),
    SetTooltipsEnabled(bool),
    SetShowOneSidedMorphs(bool),
    SetSaveSection(SaveSection),

    ResetAllSettings,
    MeasureCache,
    ClearCache,
    SetViewportBackgroundMode(ViewportBackgroundMode),

    SetBrushSweepCommit(crate::sweep_gesture::SweepCommit),

    PackageHairStyle,

    OpenHeadPresetFile(std::path::PathBuf),
    SetManualEyeGaze([f32; 2]),
    SetEyeGazeMode(EyeGazeMode),
    ResetEyeGaze,
    SetCustomHeadSolidColor([u8; 3]),
    SetG2SolidColor([u8; 3]),
    SetWireframeColor([u8; 3]),
    ToggleWireframe(bool),
    SetWireframeOpacity(f32),
    ToggleXray(bool),
    SetXrayOpacity(f32),
    ToggleViewportToolPanel(ViewportToolPanel),
    TogglePinNumbers(bool),
    SetAlignmentOpacity(f32),
    SetAlignmentG2Opacity(f32),
    RotateLight(f32),
    SetLightYaw(f32),
    SetLightingPreset(LightingPreset),
    SetLightBrightness(f32),
    SetToneMapping(ToneMapping),
    SetMsaaSamples(u32),

    SetCameraControl(crate::camera_control::ControlMode),
    RestoreSession,
    DiscardSession,
    SetVignette(VignetteSettings),
    ToggleHelp,
    Undo,
    ResetPins,
    SetOutputPath(String),
    SetVaMExportDisplayName(String),
    SetVaMExportGroup(String),
    SetVaMExportRegion(String),
    SetVaMExportIsPoseControl(bool),
    SetVaMExportBoneCorrection(bool),
    BeginGeneration,
    BakeMorph,
    BeginSculptStroke {
        view_direction_local: Option<[f64; 3]>,
        brush_direction_local: Option<[f64; 3]>,
    },

    #[cfg(test)]
    SculptDab(SculptDab),

    SculptDabDeferred(SculptDab),
    SyncSculptPreview,
    EndSculptStroke,
    SetSculptTarget {
        target: SculptTarget,
        enabled: bool,
    },
    SetSculptGroupsCollapsed(bool),
    SetSculptBrushRadius(f32),
    SetSculptStrength(f32),
    SetSculptXSymmetry(bool),
    SetSplitModelView(bool),
    SetModelSplitRatio(f32),
    RebindShortcut(crate::shortcuts::Shortcut, crate::shortcuts::Binding),
    ResetKeymap,
    Redo,
    ImportHairPreset(String),
    ConfirmHistoryBranch,
    CancelHistoryBranch,
    MuteHistoryBranchWarning,
    SetSculptConnectedTopologyOnly(bool),
    ToggleSculptEyeTracking(bool),
    ToggleSculptBackfaceMasking(bool),
    SetSculptFalloff(SculptFalloff),
    SetSculptBrush(SculptBrush),
    ResetSculpt,
    SetTextureTool(TextureTool),

    SetTextureProjectionStencil(bool),

    SetTextureProjectionPlacement(crate::texture_project::StencilPlacement),

    SetTextureProjectionOpacity(f32),

    SetTextureRetouchReverse(bool),
    BeginTextureEdit,
    EndTextureEdit,
    AddTextureImage(PathBuf, TextureSourceMode),

    SetVarMetadata(VarMetadataField, String),

    SavePackage,

    ReplacePackage,

    KeepPackage,
    RemoveTextureLayer(u64),
    MoveTextureLayerTo {
        id: u64,
        insertion_index: usize,
    },
    SelectTextureLayer(u64),
    SetTextureWorkspaceSplit(f32),
    SetTextureSourceView {
        id: u64,
        zoom: f32,
        center: [f32; 2],
    },
    SetTextureLayerVisible {
        id: u64,
        visible: bool,
    },
    SetTextureLayerChannel {
        id: u64,
        channel: TextureChannel,
    },
    SetTextureLayerOpacity {
        id: u64,
        opacity: f32,
    },
    SetTextureLayerMirror {
        id: u64,
        mirror: vkit_core::texture_mirror::FaceMirror,
    },
    SetTextureLayerBlendMode {
        id: u64,
        blend_mode: vkit_core::texture_bake::TextureBlendMode,
    },
    SetTextureLayerAdjustments {
        id: u64,
        adjustments: vkit_core::texture_bake::TextureColorAdjustments,
    },
    SetTextureLayerNormalStrength {
        id: u64,
        strength: f32,
    },
    SetTextureLayerScalarInvert {
        id: u64,
        invert: bool,
    },
    SetTextureMaskBrushRadius(f32),
    SetTextureMaskBrushFalloff(SculptFalloff),
    SetTextureMaskBrushOpacity(f32),
    SetTextureMaskPreviewEnabled(bool),
    SetTexturePinOpacity(f32),
    AddTextureMaskDab {
        id: u64,
        uv: [f32; 2],
        source: Option<[f32; 2]>,
        subtract: bool,
    },
    ClearTextureLayerMask(u64),
    SetTextureCloneSample([f32; 2]),
    AddTextureRetouchDab {
        id: u64,
        point: [f32; 2],
        tool: TextureTool,
        reverse: bool,
    },

    ResetTextureLayer(u64),
    MatchTextureLayerColor(u64),
    SetTextureResolution(u32),
    SetTextureBoundaryFeather(u16),
    SetTextureBakeBase(TextureBakeBase),
    SetTextureHideVaMSkin(bool),
    SetTextureOutputPbr(TexturePbrConvention),
    RequestTextureBake(TextureBakeQuality),
    FinishTextureDecode {
        request_id: u64,
        layer_id: u64,
        outcome: Result<Arc<crate::skin_preview::SkinImage>, String>,
    },
    FinishTextureBake {
        request_id: u64,
        outcome: Result<TextureBakedSet, String>,
    },

    #[cfg(test)]
    ApplySculpt,
    ExportResult,
    SaveTextures,
    ConfirmOverwrite,
    CancelOverwrite,
    CancelGeneration,
    ReportProgress {
        job_id: u64,
        source_revision: u64,
        progress: Progress,
    },

    ReportImportProgress {
        path: PathBuf,
        progress: ImportProgress,
    },
    FinishGeneration {
        job_id: u64,
        source_revision: u64,
        outcome: GenerationOutcome,
        morph_available: bool,
    },
    FinishExport {
        source_revision: u64,
        outcome: ExportOutcome,
    },
    ReportExportProgress {
        source_revision: u64,
        fraction: f32,
    },

    SetScanFidelity(f32),
    SetRestoreNeckEars(bool),
    OpenSettings,
    CloseSettings,

    SpendHairSettle(f32),
    ToggleScanOverlay(bool),
    SetOverlayOpacity(f32),
    SetMorphCompareBlend(f32),
    ToggleResultTearLacrimals(bool),
    ToggleResultEyelashes(bool),
    SetMorphQuery(String),
    SetMorphCategoryFilter(MorphCategoryFilter),
    SetMorphListFilter(MorphListFilter),
    SetEyeClosure(f32),
    SetFaceMorph {
        id: String,
        value: f32,
    },
    SelectVaMSkin(Option<String>),

    SetDefaultSkin(Option<String>),

    SetLastSkin(Option<String>),
    FinishVaMSkin {
        request_id: u64,
        preset_id: String,
        outcome: Result<Arc<SkinPreview>, String>,
    },
    ReportVaMCatalogProgress {
        catalog_revision: u64,
        fraction: f32,
    },
    FinishVaMCatalog {
        catalog_revision: u64,
        outcome: Result<VaMCatalogPayload, String>,
    },
    FinishVaMMorphCatalog {
        catalog_revision: u64,
        outcome: Result<VaMMorphCatalogPayload, String>,
    },
    FinishVaMMorph {
        catalog_revision: u64,
        geometry_revision: u64,
        control_id: String,
        outcome: Result<VaMMorphPayload, String>,
    },

    SetPackageFromThisHead(bool),

    AddPackageFiles(PackageSlot, Vec<PathBuf>),
    RemovePackageFile(PackageSlot, usize),

    SetPackageDirectory(Option<PathBuf>),
    ResetMorphs,
}

impl Action {
    pub fn branches_hair_history(&self) -> bool {
        matches!(
            self,
            Self::AddHairPart { .. }
                | Self::RemoveHairPart(_)
                | Self::ToggleHairPartVisible(_)
                | Self::SoloHairPart(_)
                | Self::PlantHairStrands { .. }
                | Self::UnplantHairStrands { .. }
                | Self::SetHairPartSegments { .. }
                | Self::ScaleHairStrands { .. }
                | Self::SetHairStrandPoints { .. }
                | Self::SetHairParam { .. }
                | Self::SetHairColorChannel { .. }
                | Self::ResetHairParams(_)
                | Self::ResetHairShapes
                | Self::SetHairPartName { .. }
                | Self::SetHairPartStyleJoints { .. }
                | Self::SetHairScalpMesh { .. }
                | Self::SetHairScalpTexture { .. }
                | Self::PasteHairSettings(_)
                | Self::MirrorHairPart(_)
                | Self::DuplicateHairPart(_)
        )
    }

    pub(super) const fn records_texture_undo(&self) -> bool {
        matches!(
            self,
            Self::AddTextureImage(..)
                | Self::RemoveTextureLayer(_)
                | Self::MoveTextureLayerTo { .. }
                | Self::SetTextureLayerVisible { .. }
                | Self::SetTextureLayerChannel { .. }
                | Self::SetTextureLayerOpacity { .. }
                | Self::SetTextureLayerMirror { .. }
                | Self::SetTextureLayerBlendMode { .. }
                | Self::SetTextureLayerAdjustments { .. }
                | Self::SetTextureLayerNormalStrength { .. }
                | Self::SetTextureLayerScalarInvert { .. }
                | Self::AddTextureMaskDab { .. }
                | Self::ClearTextureLayerMask(_)
                | Self::AddTextureRetouchDab { .. }
                | Self::ResetTextureLayer(_)
                | Self::MatchTextureLayerColor(_)
                | Self::SetTextureResolution(_)
                | Self::SetTextureBoundaryFeather(_)
                | Self::SetTextureBakeBase(_)
                | Self::SetTextureHideVaMSkin(_)
        )
    }

    pub(super) const fn is_mutating(&self) -> bool {
        match self {
            Self::RequestTab(_)
            | Self::SavePackage
            | Self::ReplacePackage
            | Self::KeepPackage
            | Self::RequestOpenScan
            | Self::UnloadScan
            | Self::SelectVaMEditSource(_)
            | Self::SelectBaseFace
            | Self::AddHairPart { .. }
            | Self::RemoveHairPart(_)
            | Self::ToggleHairPartVisible(_)
            | Self::SoloHairPart(_)
            | Self::AddReferenceImage(_)
            | Self::RemoveReferenceImage(_)
            | Self::SelectReferenceImage(_)
            | Self::ToggleReferenceImageVisible(_)
            | Self::ReorderReferenceImage { .. }
            | Self::DragReferenceImage { .. }
            | Self::ResizeReferenceImage { .. }
            | Self::SetReferenceImageOpacity { .. }
            | Self::NoteReferenceImageAspect { .. }
            | Self::PlantHairStrands { .. }
            | Self::UnplantHairStrands { .. }
            | Self::SetHairPartSegments { .. }
            | Self::ScaleHairStrands { .. }
            | Self::SetHairStrandPoints { .. }
            | Self::PaintMorphMask { .. }
            | Self::AddAppearanceLayer
            | Self::SelectAppearanceLayer(..)
            | Self::SetAppearanceLayerVisible { .. }
            | Self::RaiseAppearanceLayer(..)
            | Self::LowerAppearanceLayer(..)
            | Self::RemoveAppearanceLayer(..)
            | Self::ExportHairPart
            | Self::ConfirmHairOverwrite
            | Self::CancelHairOverwrite
            | Self::SetHairPartName { .. }
            | Self::SetHairPartStyleJoints { .. }
            | Self::SetHairExportSexes(_)
            | Self::SetHairParam { .. }
            | Self::SetHairColorChannel { .. }
            | Self::ResetHairParams(_)
            | Self::ResetHairShapes
            | Self::ImportHairPreset(_)
            | Self::AddHairScalp(_)
            | Self::SetHairScalpMesh { .. }
            | Self::SetHairScalpTexture { .. }
            | Self::PasteHairSettings(_)
            | Self::SetHairExportName(_)
            | Self::EndHairStroke
            | Self::SetHairExportCreator(_)
            | Self::MirrorHairPart(_)
            | Self::DuplicateHairPart(_)
            | Self::FreezeEyeGaze(_)
            | Self::BeginDirectEdit
            | Self::SetFigureSex(_)
            | Self::LoadScan(_)
            | Self::LoadTemplate(_)
            | Self::LoadResult(_)
            | Self::AutoAlign
            | Self::PlacementReset
            | Self::BeginAlignmentTransform
            | Self::CancelAlignmentTransform
            | Self::SetScale(_)
            | Self::SetScaleLinked(_)
            | Self::SetScaleAxis { .. }
            | Self::SetPosition { .. }
            | Self::SetRotation { .. }
            | Self::ToggleXMirror(_)
            | Self::RequestScanSymmetry(_)
            | Self::SetTextureExportPrefix(_)
            | Self::ConfirmTextureOverwrite
            | Self::CancelTextureOverwrite
            | Self::ConfirmScanSymmetry
            | Self::CancelScanSymmetry
            | Self::SetEyeState(_)
            | Self::Undo
            | Self::ResetPins
            | Self::SetOutputPath(_)
            | Self::BeginGeneration
            | Self::BakeMorph
            | Self::BeginSculptStroke { .. }
            | Self::SculptDabDeferred(_)
            | Self::SyncSculptPreview
            | Self::EndSculptStroke
            | Self::SetSculptTarget { .. }
            | Self::ToggleSculptEyeTracking(_)
            | Self::ToggleSculptBackfaceMasking(_)
            | Self::SetSculptFalloff(_)
            | Self::SetSculptBrush(_)
            | Self::ResetSculpt
            | Self::SetTextureTool(_)
            | Self::SetTextureProjectionStencil(_)
            | Self::SetTextureProjectionPlacement(_)
            | Self::SetTextureProjectionOpacity(_)
            | Self::SetTextureRetouchReverse(_)
            | Self::BeginTextureEdit
            | Self::EndTextureEdit
            | Self::AddTextureImage(..)
            | Self::RemoveTextureLayer(_)
            | Self::MoveTextureLayerTo { .. }
            | Self::SelectTextureLayer(_)
            | Self::SetTextureLayerVisible { .. }
            | Self::SetTextureLayerChannel { .. }
            | Self::SetTextureLayerOpacity { .. }
            | Self::SetTextureLayerMirror { .. }
            | Self::SetTextureLayerBlendMode { .. }
            | Self::SetTextureLayerAdjustments { .. }
            | Self::SetTextureLayerNormalStrength { .. }
            | Self::SetTextureLayerScalarInvert { .. }
            | Self::SetTextureMaskBrushRadius(_)
            | Self::SetTextureMaskBrushFalloff(_)
            | Self::SetTextureMaskBrushOpacity(_)
            | Self::AddTextureMaskDab { .. }
            | Self::ClearTextureLayerMask(_)
            | Self::SetTextureCloneSample(_)
            | Self::AddTextureRetouchDab { .. }
            | Self::ResetTextureLayer(_)
            | Self::MatchTextureLayerColor(_)
            | Self::SetTextureResolution(_)
            | Self::SetTextureBoundaryFeather(_)
            | Self::SetTextureBakeBase(_)
            | Self::SetTextureHideVaMSkin(_)
            | Self::SetTextureOutputPbr(_)
            | Self::RequestTextureBake(_)
            | Self::ExportResult
            | Self::SaveTextures
            | Self::ConfirmOverwrite
            | Self::SetEyeClosure(_)
            | Self::SetFaceMorph { .. }
            | Self::SetDefaultSkin(_)
            | Self::SetLastSkin(_)
            | Self::SelectVaMSkin(_)
            | Self::ResetMorphs => true,
            Self::SetRestoreNeckEars(_) => true,
            #[cfg(test)]
            Self::SculptDab(_) => true,
            #[cfg(test)]
            Self::ApplySculpt => true,

            Self::SetLocale(_)
            | Self::RequestTextureImageBrowse(_)
            | Self::SetVarMetadata(..)
            | Self::RequestVaMRootBrowse
            | Self::SetVaMRoot(_)
            | Self::RefreshVaMCatalog
            | Self::FinishScanImport { .. }
            | Self::FinishWorkspaceLoad { .. }
            | Self::CommitAlignmentTransform
            | Self::ResetCamera
            | Self::SetStandardView(_)
            | Self::ToggleProjection
            | Self::SetFov(_)
            | Self::SetBaseViewMode(_)
            | Self::SetScanFidelity(_)
            | Self::SetSurfaceSmoothPasses(_)
            | Self::SetTooltipsEnabled(_)
            | Self::SetShowOneSidedMorphs(_)
            | Self::SetSaveSection(_)
            | Self::ResetAllSettings
            | Self::MeasureCache
            | Self::ClearCache
            | Self::SetViewportBackgroundMode(_)
            | Self::SetBrushSweepCommit(_)
            | Self::PackageHairStyle
            | Self::OpenHeadPresetFile(_)
            | Self::SetManualEyeGaze(_)
            | Self::SetEyeGazeMode(_)
            | Self::ResetEyeGaze
            | Self::SetCustomHeadSolidColor(_)
            | Self::SetG2SolidColor(_)
            | Self::SetWireframeColor(_)
            | Self::ToggleWireframe(_)
            | Self::SetWireframeOpacity(_)
            | Self::ToggleXray(_)
            | Self::SetXrayOpacity(_)
            | Self::ToggleViewportToolPanel(_)
            | Self::TogglePinNumbers(_)
            | Self::SetAlignmentOpacity(_)
            | Self::SetAlignmentG2Opacity(_)
            | Self::RotateLight(_)
            | Self::SetLightYaw(_)
            | Self::SetLightingPreset(_)
            | Self::SetLightBrightness(_)
            | Self::SetToneMapping(_)
            | Self::SetMsaaSamples(_)
            | Self::SetCameraControl(_)
            | Self::RestoreSession
            | Self::DiscardSession
            | Self::SetVignette(_)
            | Self::ToggleHelp
            | Self::SetSculptBrushRadius(_)
            | Self::SetHairBrushRadius(_)
            | Self::SetHairBrushStrength(_)
            | Self::SelectHairParamGroup(_)
            | Self::SetHairTool(_)
            | Self::ActivateHairPart { .. }
            | Self::CopyHairSettings(_)
            | Self::SetSculptStrength(_)
            | Self::SetSculptXSymmetry(_)
            | Self::SetSplitModelView(_)
            | Self::SetModelSplitRatio(_)
            | Self::RebindShortcut(..)
            | Self::ResetKeymap
            | Self::Redo
            | Self::ConfirmHistoryBranch
            | Self::CancelHistoryBranch
            | Self::MuteHistoryBranchWarning
            | Self::SetSculptConnectedTopologyOnly(_)
            | Self::SetSculptGroupsCollapsed(_)
            | Self::SetTextureWorkspaceSplit(_)
            | Self::SetTextureSourceView { .. }
            | Self::SetTextureMaskPreviewEnabled(_)
            | Self::SetTexturePinOpacity(_)
            | Self::CancelOverwrite
            | Self::CancelGeneration
            | Self::ReportProgress { .. }
            | Self::ReportImportProgress { .. }
            | Self::FinishGeneration { .. }
            | Self::FinishExport { .. }
            | Self::ReportExportProgress { .. }
            | Self::OpenSettings
            | Self::CloseSettings
            | Self::SpendHairSettle(_)
            | Self::ToggleScanOverlay(_)
            | Self::SetOverlayOpacity(_)
            | Self::SetMorphCompareBlend(_)
            | Self::ToggleResultTearLacrimals(_)
            | Self::ToggleResultEyelashes(_)
            | Self::SetMorphQuery(_)
            | Self::SetMorphCategoryFilter(_)
            | Self::SetMorphListFilter(_)
            | Self::FinishVaMSkin { .. }
            | Self::FinishTextureDecode { .. }
            | Self::FinishTextureBake { .. }
            | Self::ReportVaMCatalogProgress { .. }
            | Self::FinishVaMCatalog { .. }
            | Self::FinishVaMMorphCatalog { .. }
            | Self::FinishVaMMorph { .. }
            | Self::SetPackageFromThisHead(_)
            | Self::AddPackageFiles(..)
            | Self::RemovePackageFile(..)
            | Self::SetPackageDirectory(_)
            | Self::SetVaMExportDisplayName(_)
            | Self::SetVaMExportGroup(_)
            | Self::SetVaMExportRegion(_)
            | Self::SetVaMExportIsPoseControl(_)
            | Self::SetVaMExportBoneCorrection(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn naming_the_export_is_not_refused_while_a_worker_owns_the_workspace() {
        for naming in [
            Action::SetVaMExportDisplayName("Head Fit".to_owned()),
            Action::SetVaMExportGroup("Look: Head".to_owned()),
            Action::SetVaMExportRegion("Morph".to_owned()),
            Action::SetVaMExportIsPoseControl(true),
            Action::SetVaMExportBoneCorrection(true),
        ] {
            assert!(!naming.is_mutating(), "{naming:?} would be refused");
        }
    }

    #[test]
    fn what_a_worker_owns_is_still_refused() {
        for owned in [
            Action::SetFigureSex(crate::state::FigureSex::Male),
            Action::SetOutputPath("C:/x.vmi".to_owned()),
            Action::BeginGeneration,
            Action::ResetPins,
        ] {
            assert!(owned.is_mutating(), "{owned:?} would be let through");
        }
    }
}
