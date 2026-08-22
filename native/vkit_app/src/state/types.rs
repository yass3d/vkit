use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use vkit_core::{
    formats::OrderedObjMesh,
    vam::{DiscoveredGeometryBase, GeometryBaseTier, NeutralityReceipt, SkinSex},
};

use serde::{Deserialize, Serialize};

use crate::{
    i18n::{Locale, TextKey, text},
    importers::{MeshImportPhase, MeshImportProgress},
    texture_project::TextureSourceMode,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MorphPreviewTiming {
    pub serial: u64,
    pub compose_ms: f64,
    pub eyelid_ms: f64,
    pub scene_ms: f64,
    pub total_ms: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SculptDabTiming {
    pub serial: u64,

    pub dab_ms: f64,

    pub changed_vertices: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Tab {
    #[default]
    Alignment,
    Edit,

    Morph,

    Texture,

    Hair,

    Result,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Toast {
    pub key: TextKey,
    pub tone: StatusTone,
    pub serial: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum SaveSection {
    #[default]
    Morph,
    Texture,
    Package,
    Hair,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScanSymmetry {
    #[default]
    Original,
    PositiveX,
    NegativeX,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EyeState {
    #[default]
    Open,
    Closed,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FigureSex {
    #[default]
    Female,
    Male,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EditSourceMode {
    #[default]
    ScanHead,
    CustomMorph,
}

impl FigureSex {
    pub const fn skin_sex(self) -> SkinSex {
        match self {
            Self::Female => SkinSex::Female,
            Self::Male => SkinSex::Male,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutputTopology {
    #[default]
    Daz,
    VaM,
}

#[cfg(test)]
pub const DAZ_G2_VERTEX_COUNT: usize = 21_556;
#[cfg(test)]
pub const DAZ_G2_FACE_COUNT: usize = 21_098;
#[cfg(test)]
pub const DAZ_G2F_VERTEX_COUNT: usize = DAZ_G2_VERTEX_COUNT;
#[cfg(test)]
pub const DAZ_G2F_FACE_COUNT: usize = DAZ_G2_FACE_COUNT;
pub const VAM_FEMALE_VERTEX_COUNT: usize = 24_928;
pub const VAM_FEMALE_FACE_COUNT: usize = 44_474;
pub const VAM_MALE_VERTEX_COUNT: usize = 24_801;
pub const VAM_MALE_FACE_COUNT: usize = 44_500;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VaMGeometryBaseProvenance {
    pub tier: GeometryBaseTier,
    pub neutrality: Option<NeutralityReceipt>,
}

impl VaMGeometryBaseProvenance {
    pub const fn text_key(self) -> TextKey {
        match self.tier {
            GeometryBaseTier::UnityBundle => TextKey::VaMBaseSourceUnityBundle,
            GeometryBaseTier::DazAnchorPair => TextKey::VaMBaseSourceDazAnchorPair,
            GeometryBaseTier::UnityAnchorPair => TextKey::VaMBaseSourceUnityAnchorPair,
            GeometryBaseTier::UnverifiedUserObj => TextKey::VaMBaseSourceUnverifiedObj,
        }
    }

    pub(super) fn from_discovery(discovered: &DiscoveredGeometryBase) -> Self {
        Self {
            tier: discovered.tier,
            neutrality: discovered.neutrality,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TemplateTopologyKind {
    DazG2Female,
    VaMFemale,
    VaMMale,
    #[default]
    Unknown,
}

impl TemplateTopologyKind {
    pub const fn from_counts(vertex_count: usize, face_count: usize) -> Self {
        match (vertex_count, face_count) {
            (VAM_FEMALE_VERTEX_COUNT, VAM_FEMALE_FACE_COUNT) => Self::VaMFemale,
            (VAM_MALE_VERTEX_COUNT, VAM_MALE_FACE_COUNT) => Self::VaMMale,
            _ => Self::Unknown,
        }
    }

    pub const fn output_topology(self) -> Option<OutputTopology> {
        match self {
            Self::DazG2Female => Some(OutputTopology::Daz),
            Self::VaMFemale | Self::VaMMale => Some(OutputTopology::VaM),
            Self::Unknown => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DetectedTemplateTopology {
    pub vertex_count: usize,
    pub face_count: usize,
    pub classification: TemplateTopologyKind,
}

impl DetectedTemplateTopology {
    pub const fn from_counts(vertex_count: usize, face_count: usize) -> Self {
        Self {
            vertex_count,
            face_count,
            classification: TemplateTopologyKind::from_counts(vertex_count, face_count),
        }
    }

    pub const fn output_topology(self) -> Option<OutputTopology> {
        self.classification.output_topology()
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BaseViewMode {
    #[default]
    Solid,
    Texture,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EyeGazeMode {
    #[default]
    Manual,
    AutoCursor,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewportBackgroundMode {
    #[default]
    Radial,
    Vertical,
    Flat,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImportPhase {
    #[default]
    MeshLoading,

    Simplification,
}

impl ImportPhase {
    pub const fn label(self, locale: Locale) -> &'static str {
        match (locale, self) {
            (_, Self::MeshLoading) => text(locale, TextKey::ImportLoadingMesh),
            (_, Self::Simplification) => text(locale, TextKey::ImportSimplifying),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImportProgress {
    pub phase: ImportPhase,
    pub fraction: f32,

    pub source_triangles: Option<usize>,
}

pub const HEAVY_MESH_TRIANGLES: usize = 100_000;

fn group_digits(value: usize) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

impl ImportProgress {
    pub fn new(phase: ImportPhase, fraction: f32) -> Self {
        Self::with_size(phase, fraction, None)
    }

    pub fn with_size(phase: ImportPhase, fraction: f32, source_triangles: Option<usize>) -> Self {
        Self {
            phase,
            fraction: if fraction.is_finite() {
                fraction.clamp(0.0, 1.0)
            } else {
                0.0
            },
            source_triangles,
        }
    }

    #[must_use]
    pub fn size_note(&self, locale: Locale) -> Option<String> {
        let triangles = self.source_triangles?;
        if triangles < HEAVY_MESH_TRIANGLES {
            return None;
        }
        Some(text(locale, TextKey::HeavyMeshNote).replace("{count}", &group_digits(triangles)))
    }

    pub fn from_mesh_import(progress: MeshImportProgress) -> Self {
        let phase = match progress.phase {
            MeshImportPhase::MeshLoading => ImportPhase::MeshLoading,
            MeshImportPhase::Simplification => ImportPhase::Simplification,
        };
        Self::with_size(phase, progress.progress, progress.source_triangles)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScanImportCompletion {
    pub generation: u64,
    pub completed_at: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttentionTarget {
    CustomHeadLoad,

    VaMRoot,

    SkinPanel,

    SkinTextureMode,

    PackageCreator,

    PackageName,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MissingPackageFields {
    pub creator: bool,
    pub package: bool,
}

impl MissingPackageFields {
    #[must_use]
    pub const fn any(self) -> bool {
        self.creator || self.package
    }
}

pub const MORPH_EXPORT_EXTENSION: &str = "vmi";

pub const MORPH_EXPORT_LABEL: &str = "VaM (VMI + VMB)";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportReceipt {
    pub committed_paths: Vec<PathBuf>,
}

impl ExportReceipt {
    pub fn primary_path(&self) -> Option<&Path> {
        self.committed_paths.first().map(PathBuf::as_path)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MorphNameDisplay {
    #[default]
    Localized,

    Original,
}

pub const MORPH_PAIR_EXTENSIONS: [&str; 2] = ["vmi", "vmb"];

#[must_use]
pub fn with_morph_twins(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut complete = Vec::with_capacity(paths.len() * 2);
    for path in paths {
        let twin = morph_twin_of(&path).filter(|twin| twin.is_file());
        if !complete.contains(&path) {
            complete.push(path);
        }
        if let Some(twin) = twin
            && !complete.contains(&twin)
        {
            complete.push(twin);
        }
    }
    complete
}

#[must_use]
pub fn morph_twin_of(path: &Path) -> Option<PathBuf> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let [vmi, vmb] = MORPH_PAIR_EXTENSIONS;
    let twin = match extension.as_str() {
        value if value == vmi => vmb,
        value if value == vmb => vmi,
        _ => return None,
    };
    Some(path.with_extension(twin))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageFile {
    pub path: PathBuf,

    pub label: String,
}

impl PackageFile {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        let label = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        Self { path, label }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageSlot {
    Morph,
    Texture,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ViewportToolPanel {
    Lighting,
    Camera,
    BaseView,
    Wireframe,
    Xray,
    Skin,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ResultPreviewPhase {
    #[default]
    Empty,

    Sculpt,

    Morph,

    Save,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogIntent {
    OpenScan,

    OpenTextureImage(TextureSourceMode),
    ChooseOutput,
    ChooseVaMRoot,

    OpenHeadPreset,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum VaMCatalogStatus {
    #[default]
    Unconfigured,
    Indexing,
    Ready {
        morph_count: usize,
        skin_count: usize,
    },
    Failed {
        detail: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum VaMMorphCacheStatus {
    #[default]
    Unavailable,
    Loading,
    Ready {
        morph_count: usize,
    },
    Missing {
        detail: String,
    },
}

#[derive(Debug)]
pub enum GenerationOutcome {
    Success {
        output: OrderedObjMesh,
        fit_reference_value: f64,
    },
    Failed(String),
    Cancelled,
}

#[derive(Debug)]
pub enum ExportOutcome {
    Success { receipt: ExportReceipt },
    Failed(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusTone {
    Neutral,
    Info,
    Success,
    Warning,
    Error,
}

static STATUS_MESSAGE_REVISION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct StatusMessage {
    pub key: TextKey,
    pub tone: StatusTone,
    pub detail: Option<String>,

    pub(crate) revision: u64,
}

impl StatusMessage {
    pub fn new(key: TextKey, tone: StatusTone) -> Self {
        Self {
            key,
            tone,
            detail: None,
            revision: STATUS_MESSAGE_REVISION.fetch_add(1, Ordering::Relaxed),
        }
    }

    pub fn with_detail(key: TextKey, tone: StatusTone, detail: impl Into<String>) -> Self {
        Self {
            key,
            tone,
            detail: Some(detail.into()),
            revision: STATUS_MESSAGE_REVISION.fetch_add(1, Ordering::Relaxed),
        }
    }
}

impl PartialEq for StatusMessage {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.tone == other.tone && self.detail == other.detail
    }
}

impl Eq for StatusMessage {}

impl Default for StatusMessage {
    fn default() -> Self {
        Self::new(TextKey::Ready, StatusTone::Neutral)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    dead_code,
    reason = "the native worker will report every pipeline stage"
)]
pub enum JobStage {
    Import,
    Prepare,
    Align,
    Fit,
    Validate,
    Export,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    pub stage: JobStage,
    pub fraction: f32,
}

impl Progress {
    pub fn new(stage: JobStage, fraction: f32) -> Self {
        Self {
            stage,
            fraction: fraction.clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResultState {
    Empty,
    Running {
        job_id: u64,
        source_revision: u64,
    },
    Ready {
        source_revision: u64,
        morph_available: bool,
    },
    Stale {
        generated_revision: u64,
    },
}

impl ResultState {
    pub const fn is_running(self) -> bool {
        matches!(self, Self::Running { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScanTransform {
    pub scale_xyz: [f64; 3],
    pub translation_cm: [f64; 3],
    pub rotation_degrees: [f64; 3],
}

impl Default for ScanTransform {
    fn default() -> Self {
        Self {
            scale_xyz: [1.0; 3],
            translation_cm: [0.0; 3],
            rotation_degrees: [0.0; 3],
        }
    }
}

pub const MAX_ALIGNMENT_UNDO_HISTORY: usize = 64;

pub const DEFAULT_CUSTOM_HEAD_SOLID_COLOR_RGB: [u8; 3] = [0x73, 0xa7, 0xd7];
pub const DEFAULT_G2_SOLID_COLOR_RGB: [u8; 3] = [0xe8, 0xb2, 0x78];

pub const DEFAULT_WIREFRAME_COLOR_RGB: [u8; 3] = [0xe8, 0xee, 0xf8];

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ScanImportLifecycle {
    #[default]
    Idle,
    Requested {
        path: PathBuf,
    },
    Running {
        path: PathBuf,
    },
}

impl ScanImportLifecycle {
    pub const fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub fn active_path(&self) -> Option<&Path> {
        match self {
            Self::Idle => None,
            Self::Requested { path } | Self::Running { path } => Some(path),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ExportLifecycle {
    #[default]
    Idle,
    Queued {
        revision: u64,
    },
    Writing {
        revision: u64,
    },
}

impl ExportLifecycle {
    pub const fn is_active(self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub const fn writing_revision(self) -> Option<u64> {
        match self {
            Self::Writing { revision } => Some(revision),
            Self::Idle | Self::Queued { .. } => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceLoadKind {
    Template,

    Result,

    VaMMorphPair,

    DirectEditSource,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum WorkspaceLoadLifecycle {
    #[default]
    Idle,
    Requested {
        kind: WorkspaceLoadKind,
        path: PathBuf,
    },
    Running {
        kind: WorkspaceLoadKind,
        path: PathBuf,
    },
}

impl WorkspaceLoadLifecycle {
    pub const fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub fn active_path(&self) -> Option<&Path> {
        match self {
            Self::Idle => None,
            Self::Requested { path, .. } | Self::Running { path, .. } => Some(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_morph_is_two_files_and_picking_one_brings_the_other() {
        let folder = tempfile::tempdir().expect("a folder to pick from");
        let vmi = folder.path().join("cheekbones.vmi");
        let vmb = folder.path().join("cheekbones.vmb");
        std::fs::write(&vmi, b"{}").expect("the descriptor");
        std::fs::write(&vmb, b"\x00").expect("the deltas");

        let from_vmi = with_morph_twins(vec![vmi.clone()]);
        assert_eq!(from_vmi, vec![vmi.clone(), vmb.clone()]);
        let from_vmb = with_morph_twins(vec![vmb.clone()]);
        assert_eq!(from_vmb, vec![vmb.clone(), vmi.clone()]);

        assert_eq!(
            with_morph_twins(vec![vmi.clone(), vmb.clone()]),
            vec![vmi.clone(), vmb.clone()]
        );
    }

    #[test]
    fn a_twin_that_is_not_on_disk_is_not_invented() {
        let folder = tempfile::tempdir().expect("a folder to pick from");
        let lonely = folder.path().join("lonely.vmi");
        std::fs::write(&lonely, b"{}").expect("the descriptor");
        assert_eq!(with_morph_twins(vec![lonely.clone()]), vec![lonely]);

        let texture = folder.path().join("face.png");
        std::fs::write(&texture, b"\x00").expect("a texture");
        assert_eq!(morph_twin_of(&texture), None);
        assert_eq!(with_morph_twins(vec![texture.clone()]), vec![texture]);
    }
}
