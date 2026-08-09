use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use vkit_core::{
    anatomy::EyelidConformationPlan,
    fit::suggest_zero_pin_extent_similarity,
    formats::{
        DazGeometry, MorphAuthoring, MorphTarget, OrderedObjMesh, embedded_g2f_curated_morph_pack,
        embedded_g2f_curated_morph_targets, embedded_g2f_curated_morph_targets_for_vam_anchor,
        embedded_g2f_eye_closed_morph, embedded_g2f_eye_closed_morph_for_vam_anchor,
        load_ordered_obj, matches_canonical_g2_topology,
    },
    math::{Mat3 as CoreMat3, SimilarityTransform, Vec3 as CoreVec3},
    surface_smoothing::{VAM_SMOOTH_PASSES_DEFAULT, VAM_SMOOTH_PASSES_MAX},
    symmetry::SymmetryMode,
    texture_bake::{TextureWarpPin, map_g2_point_to_source},
    vam::{
        BuiltinHairScalp, DiscoveredGeometryBase, G2UvMapping, GeometryBaseRequest,
        GeometryBaseTier, GeometrySex, HairPreset, MorphCategory as VaMMorphCategory, PackageIndex,
        SkinPreset, VAM_SHARED_BODY_VERTEX_COUNT, VaMGeometryProvider, VaMRoot, VmbVertexRouting,
        classify_vam_morph_route, decode_vmb_daz_cm_for_topology, discover_geometry_base,
        parse_vmi, vmb_raw_entry_count,
    },
};

#[cfg(test)]
use vkit_core::vam::{load_or_extract_neutral_base, validate_neutral_shared_prefix};

use crate::{
    ambient_occlusion::AmbientOcclusionSettings,
    camera::{ProjectionMode, StandardView},
    hair_preview::HairPreview,
    i18n::{Locale, TextKey},
    importers::is_supported_mesh_path,
    lighting::{DEFAULT_LIGHT_BRIGHTNESS, LightingPreset, ViewportGrading, sanitize_brightness},
    morphs::{
        GenitalMorphTarget, MorphCategory, MorphCategoryFilter, MorphControl, MorphLibrary,
        MorphLibraryValueSnapshot, MorphListFilter, MorphSource,
        apply_genital_applications_to_vam_mesh, unresolved_slider_contract,
    },
    post_process::{BloomSettings, VignetteSettings},
    scene::{Bounds3, MeshSide, ModelTransform, PreparedScan, SurfaceEndpoint, WorkspaceScene},
    sculpt::{SculptBrush, SculptDab, SculptFalloff, SculptSession, SculptTarget},
    shader_color::ToneMapping,
    skin_preview::SkinPreview,
    texture_project::{
        PREVIEW_BAKE_RESOLUTION, ScanTextureBakeSource, TextureBakeBase, TextureBakeQuality,
        TextureBakedSet, TextureChannel, TextureDecodeRequest, TexturePbrConvention,
        TextureProject, TextureSourceMode, TextureTargetPin, TextureTool, TextureWorkRequest,
    },
    theme::INSPECTOR_DEFAULT_WIDTH,
    vam_catalog::{VaMCatalogPayload, VaMMorphCatalogPayload, VaMMorphPayload, VaMWorkRequest},
    vam_edit_sources::{
        PackedMorphPairAsset, ResolvedMorphPairAsset, VaMEditSource, VaMEditSourceKind,
        morph_identity_candidates, parse_appearance_recipe, resolve_morph_pair_asset,
    },
    vam_hair::HairPreviewRequest,
    vam_morph_cache::CachedBuiltinMorph,
    vam_morph_index::{MorphLocation, VaMMorphIndex},
    vam_skin::SkinPreviewRequest,
};

mod template_ingest;
mod types;

pub use types::*;

pub use template_ingest::PreparedTemplateLoad;
use template_ingest::TemplateLoadContext;

#[derive(Clone, Debug)]
struct EnrolledBaseCache {
    provider: Arc<VaMGeometryProvider>,
    base_path: PathBuf,
    provenance: VaMGeometryBaseProvenance,
    geometry: Arc<DazGeometry>,
    detected: DetectedTemplateTopology,
}

const fn enrolled_cache_slot(sex: FigureSex) -> usize {
    match sex {
        FigureSex::Female => 0,
        FigureSex::Male => 1,
    }
}

#[derive(Clone, Debug)]
struct MorphResetUndo {
    values: MorphLibraryValueSnapshot,
    eye_closure: f32,

    seq: u64,
}

pub(crate) const HAIR_SETTLE_SECONDS: f32 = 2.5;

pub const HAIR_SETTLE_GRAVITY: f32 = 1.0;

const MAX_MORPH_VALUE_UNDO: usize = 64;

const COMPARE_PREVIEW_SIGNATURE_TAG: f64 = f64::NEG_INFINITY;

const fn should_request_g2_embedded_assets(
    detected_sex: Option<FigureSex>,
    exact_g2_topology: bool,
) -> bool {
    exact_g2_topology && detected_sex.is_some()
}

const VAM_FEMALE_BASE_ENV: &str = "VKIT_VAM_FEMALE_BASE";
const VAM_MALE_BASE_ENV: &str = "VKIT_VAM_MALE_BASE";

fn geometry_sex_to_figure_sex(sex: GeometrySex) -> FigureSex {
    match sex {
        GeometrySex::Female => FigureSex::Female,
        GeometrySex::Male => FigureSex::Male,
    }
}

fn figure_sex_to_geometry_sex(sex: FigureSex) -> GeometrySex {
    match sex {
        FigureSex::Female => GeometrySex::Female,
        FigureSex::Male => GeometrySex::Male,
    }
}

#[cfg(test)]
fn is_obj_path(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("obj"))
}

fn sanitize_path_component(value: &str, fallback: &str) -> String {
    let sanitized = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .take(64)
        .collect::<String>();
    let sanitized =
        sanitized.trim_matches(|character: char| character == '.' || character.is_whitespace());
    if sanitized.is_empty() {
        fallback.to_owned()
    } else {
        sanitized.to_owned()
    }
}

fn union_head_bounds(first: Option<Bounds3>, second: Option<Bounds3>) -> Option<Bounds3> {
    match (first, second) {
        (Some(first), Some(second)) => Some(Bounds3 {
            min: first.min.min(second.min),
            max: first.max.max(second.max),
        }),
        (first, second) => first.or(second),
    }
}

pub(crate) fn eye_angles_from_gaze(gaze: [f32; 2], left_eye: bool) -> [f64; 2] {
    let x = f64::from(gaze[0].clamp(-1.0, 1.0));
    let y = f64::from(gaze[1].clamp(-1.0, 1.0));
    let yaw = if x < 0.0 {
        x * if left_eye { 25.0 } else { 22.0 }
    } else {
        x * if left_eye { 22.0 } else { 25.0 }
    };
    let pitch = if y < 0.0 { y * 30.0 } else { y * 20.0 };
    [yaw, pitch]
}

fn eyelid_role_of_control_id(
    control_id: &str,
) -> Option<(vkit_core::anatomy::EyelidLookRole, Option<bool>)> {
    let role = vkit_core::anatomy::eyelid_look_role(control_id)?;
    let name = control_id.to_ascii_lowercase();

    let left_eye = name.starts_with("phm").then(|| name.ends_with('l'));
    Some((role, left_eye))
}

fn sanitize_eye_gaze_axis(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CarriedEdit {
    pub(crate) morph_values: MorphLibraryValueSnapshot,

    pub(crate) sculpt: Option<Vec<[f64; 3]>>,

    pub(crate) eye_closure: f32,
}

#[derive(Debug)]
pub struct AppState {
    pub locale: Locale,
    pub active_tab: Tab,

    pub morph_look_find_open: bool,

    pub inspector_width: f32,
    pub revision: u64,
    pub scan_path: Option<PathBuf>,
    pub template_path: Option<PathBuf>,

    pub detected_template_topology: DetectedTemplateTopology,

    pub output_topology: OutputTopology,
    pub figure_sex: FigureSex,
    pub transform: ScanTransform,

    pub scale_linked: bool,
    pub x_mirror: bool,
    pub scan_symmetry: ScanSymmetry,
    pub eye_state: EyeState,
    pub cameras_linked: bool,
    pub pin_numbers: bool,
    pub help_visible: bool,

    pub help_opened_for_projection: bool,

    pub projection_help_offered: bool,
    pub complete_pairs: usize,

    pub output_path: String,

    pub vam_export_display_name: String,
    pub vam_export_group: String,
    pub vam_export_region: String,
    pub vam_export_is_pose_control: bool,

    pub vam_export_bone_correction: bool,

    pub pending_overwrite_path: Option<PathBuf>,

    pub pending_texture_overwrite: Vec<PathBuf>,

    pub morphs_before_browsing: Option<crate::morphs::MorphLibraryValueSnapshot>,

    pub placed_head: bool,

    pub pending_symmetry_change: Option<ScanSymmetry>,
    pub result: ResultState,
    pub base_view_mode: BaseViewMode,

    pub surface_smooth_passes: u8,
    pub viewport_background_mode: ViewportBackgroundMode,

    pub manual_eye_gaze: [f32; 2],
    pub eye_gaze_mode: EyeGazeMode,

    pub frozen_eye_gaze: Option<[f32; 2]>,

    pub custom_head_solid_color_rgb: [u8; 3],

    pub g2_solid_color_rgb: [u8; 3],

    pub wireframe_color_rgb: [u8; 3],
    pub wireframe_visible: bool,
    pub wireframe_opacity: f32,
    pub xray_visible: bool,
    pub xray_opacity: f32,
    pub viewport_tool_panel: Option<ViewportToolPanel>,

    pub hair_visible: bool,
    pub scan_overlay: bool,
    pub overlay_opacity: f32,

    pub morph_compare_blend: f32,

    pub custom_morph_origin: Option<Arc<Vec<[f64; 3]>>>,
    pub show_result_tear_lacrimals: bool,
    pub show_result_eyelashes: bool,

    pub alignment_opacity: f32,

    pub alignment_g2_opacity: f32,
    pub light_yaw_radians: f32,
    pub lighting_preset: LightingPreset,
    pub light_brightness: f32,

    pub tone_mapping: ToneMapping,

    pub camera_control: crate::camera_control::ControlMode,

    pub pending_recovery: Option<crate::session_snapshot::SessionSnapshot>,

    pub ambient_occlusion: AmbientOcclusionSettings,

    pub bloom: BloomSettings,

    pub vignette: VignetteSettings,
    pub eye_closure: f32,
    pub morph_library: MorphLibrary,
    morph_reset_undo: Option<MorphResetUndo>,

    morph_value_history: VecDeque<MorphResetUndo>,

    morph_edit_open: Option<String>,
    pub sculpt_brush_radius_points: f32,
    pub sculpt_strength: f32,

    pub sculpt_x_symmetry: bool,

    pub sculpt_connected_topology_only: bool,

    pub sculpt_eye_tracking: bool,

    pub sculpt_groups_collapsed: bool,

    pub detail_group_panel_pos: Option<[f32; 2]>,

    pub sculpt_brush: SculptBrush,

    pub vam_root: Option<PathBuf>,

    pub edit_source_mode: EditSourceMode,

    pub vam_edit_sources: Vec<VaMEditSource>,

    pending_edit_carry: Option<CarriedEdit>,

    vam_morph_index: Arc<VaMMorphIndex>,

    pub vam_package_index: Arc<PackageIndex>,

    /// What the head wears in "layers only" until a bake arrives.
    ///
    /// Kept ready rather than derived on demand because the viewport asks with
    /// a shared borrow; rebuilt when the toggle, the base colour or the UV
    /// mapping changes, which is nowhere near every frame.
    pub neutral_skin_preview: Option<Arc<SkinPreview>>,
    pub var_metadata: vkit_core::vam::VarMetadata,

    pub package_from_this_head: bool,

    pub package_morphs: Vec<PackageFile>,
    pub package_textures: Vec<PackageFile>,

    pub package_directory: Option<PathBuf>,

    pub var_version_text: String,

    pub missing_package_fields: MissingPackageFields,

    pub pending_package_replace: Option<PathBuf>,

    pub package_fields_want_attention: bool,
    pub vam_morph_groups: Vec<String>,
    pub vam_morph_regions: Vec<String>,
    pub vam_edit_query: String,
    pub selected_vam_edit_source_id: Option<String>,
    pending_direct_edit_source: Option<VaMEditSource>,

    pub vam_geometry_base_path: Option<PathBuf>,

    pub vam_geometry_base_provenance: Option<VaMGeometryBaseProvenance>,

    pub vam_geometry_provider: Option<Arc<VaMGeometryProvider>>,

    pub vam_full_preview: Option<Arc<OrderedObjMesh>>,

    enrolled_base_cache: [Option<EnrolledBaseCache>; 2],
    pub vam_catalog_status: VaMCatalogStatus,

    pub vam_catalog_progress: f32,

    pub vam_morph_cache_status: VaMMorphCacheStatus,

    pub save_section: SaveSection,

    pub toast: Option<Toast>,
    toast_serial: u64,

    pub vam_skin_presets: Vec<SkinPreset>,

    pub default_skin_id: Option<String>,

    pub last_skin_id: Option<String>,
    face_mirror_map: Option<Arc<vkit_core::texture_mirror::FaceMirrorMap>>,

    face_uv_rows: Option<Arc<std::collections::HashMap<u32, usize>>>,

    pub vam_hair_presets: Vec<HairPreset>,
    pub vam_uv_mapping: Option<Arc<G2UvMapping>>,

    pub vam_uv_mapping_warning: Option<String>,

    pub skin_query: String,

    pub selected_skin_id: Option<String>,
    pub skin_preview_loading: bool,

    pub skin_preview: Option<Arc<SkinPreview>>,

    skin_preview_lru: VecDeque<(String, Arc<SkinPreview>)>,
    pub hair_query: String,
    pub selected_hair_id: Option<String>,

    pub failed_hair_ids: BTreeSet<String>,
    pub hair_preview_loading: bool,
    pub hair_preview: Option<Arc<HairPreview>>,

    pub shared_scalp: Option<Box<vkit_core::vam::HairPartReference>>,

    pub builtin_hair_scalps: Arc<Vec<BuiltinHairScalp>>,
    hair_preview_lru: VecDeque<(String, Arc<HairPreview>)>,

    pub morph_preview_timing: Option<MorphPreviewTiming>,

    pub sculpt_dab_timing: Option<SculptDabTiming>,
    pub fit_reference_value: Option<f64>,

    eyelid_conformation_skipped: bool,
    pub status: StatusMessage,
    pub progress: Option<Progress>,

    pub import_progress: Option<ImportProgress>,

    pub scan_import_completion: Option<ScanImportCompletion>,

    pub template_install_generation: u64,
    pub workspace: WorkspaceScene,

    pub sculpt: SculptSession,

    pub texture_project: TextureProject,

    pub hair_settle_seconds: f32,

    pub scan_fidelity: f32,
    pub settings_open: bool,
    pub settings_section: crate::settings::SettingsSection,
    pub settings_graphics_page: crate::settings::GraphicsPage,

    pub tooltips_enabled: bool,

    pub settings_reset: bool,

    pub cache_bytes: Option<u64>,
    pub cache_measure_requested: bool,
    pub cache_clear_requested: bool,

    pending_attention: Option<AttentionTarget>,
    next_job_id: u64,
    pending_dialog: Option<DialogIntent>,
    scan_import: ScanImportLifecycle,
    workspace_load: WorkspaceLoadLifecycle,
    next_scan_import_completion_generation: u64,

    alignment_undo_history: VecDeque<ScanTransform>,
    alignment_transform_baseline: Option<ScanTransform>,
    cancel_requested: Option<u64>,
    export: ExportLifecycle,
    pub result_preview_phase: ResultPreviewPhase,

    morph_preview_dirty: bool,
    vam_appearance_revision: u64,
    vam_catalog_revision: u64,

    pub morph_name_display: MorphNameDisplay,

    pub last_package_path: Option<PathBuf>,

    pub last_morph_save_hint: Option<TextKey>,
    vam_cached_morphs: BTreeMap<String, CachedBuiltinMorph>,

    vam_morph_faces: Option<Arc<Vec<Vec<u32>>>>,
    pending_vam_work: VecDeque<VaMWorkRequest>,
    next_skin_request_id: u64,
    expected_skin_request: Option<u64>,
    pending_skin_work: Option<SkinPreviewRequest>,
    next_texture_request_id: u64,
    expected_texture_bake_request: Option<(u64, u64, u32)>,
    pending_texture_work: VecDeque<TextureWorkRequest>,
    next_hair_request_id: u64,
    expected_hair_request: Option<u64>,
    pending_hair_work: Option<HairPreviewRequest>,
    eyelid_conformation_plan: Option<Arc<EyelidConformationPlan>>,

    morph_preview_reference_arrays: Vec<[f64; 3]>,
    morph_preview_reference_scratch: Vec<CoreVec3>,
    morph_preview_conform_scratch: Vec<CoreVec3>,
    next_morph_timing_serial: u64,
    next_sculpt_timing_serial: u64,
}

impl Default for AppState {
    fn default() -> Self {
        let mut sculpt = SculptSession::default();
        sculpt.set_x_symmetry(true);
        sculpt.set_connected_topology_only(false);
        Self {
            locale: Locale::default(),
            active_tab: Tab::Alignment,
            morph_look_find_open: false,
            inspector_width: INSPECTOR_DEFAULT_WIDTH,
            revision: 0,
            scan_path: None,
            template_path: None,
            detected_template_topology: DetectedTemplateTopology::default(),

            output_topology: OutputTopology::VaM,
            figure_sex: FigureSex::Female,
            transform: ScanTransform::default(),
            scale_linked: true,
            x_mirror: true,
            scan_symmetry: ScanSymmetry::Original,
            eye_state: EyeState::Open,
            cameras_linked: true,
            pin_numbers: true,
            help_visible: false,
            help_opened_for_projection: false,
            projection_help_offered: false,
            complete_pairs: 0,
            output_path: String::new(),
            vam_export_display_name: String::new(),
            // Blank, not a guess. Whatever is typed here is remembered, so a
            // seeded default would be a value the user never chose and would
            // have to notice and clear before their own choice could stick.
            vam_export_group: String::new(),
            vam_export_region: String::new(),
            vam_export_is_pose_control: false,
            vam_export_bone_correction: true,
            pending_overwrite_path: None,
            pending_texture_overwrite: Vec::new(),
            morphs_before_browsing: None,
            placed_head: false,
            pending_symmetry_change: None,
            result: ResultState::Empty,
            base_view_mode: BaseViewMode::Solid,
            surface_smooth_passes: VAM_SMOOTH_PASSES_DEFAULT,
            viewport_background_mode: ViewportBackgroundMode::Radial,
            manual_eye_gaze: [0.0; 2],
            eye_gaze_mode: EyeGazeMode::Manual,
            frozen_eye_gaze: None,
            custom_head_solid_color_rgb: DEFAULT_CUSTOM_HEAD_SOLID_COLOR_RGB,
            g2_solid_color_rgb: DEFAULT_G2_SOLID_COLOR_RGB,
            wireframe_color_rgb: DEFAULT_WIREFRAME_COLOR_RGB,
            wireframe_visible: false,
            wireframe_opacity: 0.32,
            xray_visible: false,
            xray_opacity: 0.28,
            viewport_tool_panel: None,
            scan_overlay: false,
            hair_visible: true,
            overlay_opacity: 0.5,
            morph_compare_blend: 1.0,
            custom_morph_origin: None,
            show_result_tear_lacrimals: true,
            show_result_eyelashes: true,

            alignment_opacity: 1.0,
            alignment_g2_opacity: 1.0,
            light_yaw_radians: 0.0,
            lighting_preset: LightingPreset::default(),
            light_brightness: DEFAULT_LIGHT_BRIGHTNESS,
            tone_mapping: ToneMapping::default(),
            camera_control: crate::camera_control::ControlMode::default(),
            pending_recovery: None,
            ambient_occlusion: AmbientOcclusionSettings::default(),
            bloom: BloomSettings::default(),
            vignette: VignetteSettings::default(),
            eye_closure: 0.0,
            morph_library: MorphLibrary::default(),
            morph_reset_undo: None,
            morph_value_history: VecDeque::new(),
            morph_edit_open: None,
            sculpt_brush_radius_points: 64.0,
            sculpt_strength: 0.35,
            sculpt_x_symmetry: true,
            sculpt_connected_topology_only: false,
            sculpt_eye_tracking: false,
            sculpt_groups_collapsed: true,
            detail_group_panel_pos: None,
            sculpt_brush: SculptBrush::default(),
            vam_root: None,
            edit_source_mode: EditSourceMode::ScanHead,
            vam_edit_sources: Vec::new(),
            pending_edit_carry: None,
            vam_morph_index: Arc::new(VaMMorphIndex::default()),
            vam_package_index: Arc::new(PackageIndex::default()),
            neutral_skin_preview: None,
            var_metadata: vkit_core::vam::VarMetadata::default(),
            package_from_this_head: true,
            package_morphs: Vec::new(),
            package_textures: Vec::new(),
            package_directory: None,
            var_version_text: "1".to_owned(),
            missing_package_fields: MissingPackageFields::default(),
            pending_package_replace: None,
            package_fields_want_attention: false,
            vam_morph_groups: Vec::new(),
            vam_morph_regions: Vec::new(),
            vam_edit_query: String::new(),
            selected_vam_edit_source_id: None,
            pending_direct_edit_source: None,
            vam_geometry_base_path: None,
            vam_geometry_base_provenance: None,
            vam_geometry_provider: None,
            vam_full_preview: None,
            enrolled_base_cache: [None, None],
            vam_catalog_status: VaMCatalogStatus::Unconfigured,
            vam_catalog_progress: 0.0,
            vam_morph_cache_status: VaMMorphCacheStatus::Unavailable,
            save_section: SaveSection::default(),
            toast: None,
            toast_serial: 0,
            vam_skin_presets: Vec::new(),
            default_skin_id: None,
            last_skin_id: None,
            face_mirror_map: None,
            face_uv_rows: None,
            vam_hair_presets: Vec::new(),
            vam_uv_mapping: None,
            vam_uv_mapping_warning: None,
            skin_query: String::new(),
            selected_skin_id: None,
            skin_preview_loading: false,
            skin_preview: None,
            skin_preview_lru: VecDeque::new(),
            hair_query: String::new(),
            selected_hair_id: None,
            failed_hair_ids: BTreeSet::new(),
            hair_preview_loading: false,
            hair_preview: None,
            shared_scalp: None,
            builtin_hair_scalps: Arc::new(Vec::new()),
            hair_preview_lru: VecDeque::new(),
            morph_preview_timing: None,
            sculpt_dab_timing: None,
            fit_reference_value: None,
            eyelid_conformation_skipped: false,
            status: StatusMessage::default(),
            progress: None,
            import_progress: None,
            scan_import_completion: None,
            template_install_generation: 0,
            workspace: WorkspaceScene::default(),
            sculpt,
            texture_project: TextureProject::default(),
            hair_settle_seconds: 0.0,
            scan_fidelity: 1.0,
            settings_open: false,
            settings_section: crate::settings::SettingsSection::default(),
            settings_graphics_page: crate::settings::GraphicsPage::default(),
            tooltips_enabled: true,
            settings_reset: false,
            cache_bytes: None,
            cache_measure_requested: false,
            cache_clear_requested: false,
            pending_attention: None,
            next_job_id: 1,
            pending_dialog: None,
            scan_import: ScanImportLifecycle::Idle,
            workspace_load: WorkspaceLoadLifecycle::Idle,
            next_scan_import_completion_generation: 1,
            alignment_undo_history: VecDeque::new(),
            alignment_transform_baseline: None,
            cancel_requested: None,
            export: ExportLifecycle::Idle,
            result_preview_phase: ResultPreviewPhase::Empty,
            morph_preview_dirty: false,
            vam_appearance_revision: 0,
            vam_catalog_revision: 0,

            morph_name_display: MorphNameDisplay::default(),
            last_package_path: None,
            last_morph_save_hint: None,
            vam_cached_morphs: BTreeMap::new(),
            vam_morph_faces: None,
            pending_vam_work: VecDeque::new(),
            next_skin_request_id: 1,
            expected_skin_request: None,
            pending_skin_work: None,
            next_texture_request_id: 1,
            expected_texture_bake_request: None,
            pending_texture_work: VecDeque::new(),
            next_hair_request_id: 1,
            expected_hair_request: None,
            pending_hair_work: None,
            eyelid_conformation_plan: None,
            morph_preview_reference_arrays: Vec::new(),
            morph_preview_reference_scratch: Vec::new(),
            morph_preview_conform_scratch: Vec::new(),
            next_morph_timing_serial: 1,
            next_sculpt_timing_serial: 1,
        }
    }
}

const fn morph_category_from_vam(category: VaMMorphCategory) -> MorphCategory {
    match category {
        VaMMorphCategory::Eyes => MorphCategory::Eyes,
        VaMMorphCategory::Brows => MorphCategory::Brows,
        VaMMorphCategory::Nose => MorphCategory::Nose,
        VaMMorphCategory::Mouth => MorphCategory::Mouth,
        VaMMorphCategory::Jaw => MorphCategory::Jaw,
        VaMMorphCategory::Cheeks => MorphCategory::Cheeks,
        VaMMorphCategory::Cheekbones => MorphCategory::Cheekbones,
        VaMMorphCategory::Ears => MorphCategory::Ears,
        VaMMorphCategory::Expression => MorphCategory::Expression,
        VaMMorphCategory::Head => MorphCategory::Head,
        VaMMorphCategory::Body => MorphCategory::Body,
    }
}

fn curated_morph_category(category: &str) -> MorphCategory {
    match category {
        "eyes" => MorphCategory::Eyes,
        "brows" => MorphCategory::Brows,
        "nose" => MorphCategory::Nose,
        "mouth" => MorphCategory::Mouth,
        "jaw" => MorphCategory::Jaw,
        "cheeks" => MorphCategory::Cheeks,
        "cheekbones" => MorphCategory::Cheekbones,
        "ears" => MorphCategory::Ears,
        "expression" => MorphCategory::Expression,
        "head" => MorphCategory::Head,
        _ => MorphCategory::Body,
    }
}

mod actions;
pub use actions::{Action, VarMetadataField};

impl AppState {
    pub const fn busy(&self) -> bool {
        self.scan_import.is_active()
            || self.workspace_load.is_active()
            || self.result.is_running()
            || self.export.is_active()
    }

    pub const fn active_job(&self) -> Option<(u64, u64)> {
        match self.result {
            ResultState::Running {
                job_id,
                source_revision,
            } => Some((job_id, source_revision)),
            _ => None,
        }
    }

    pub const fn viewport_grading(&self) -> ViewportGrading {
        ViewportGrading {
            light_yaw_radians: self.light_yaw_radians,
            preset: self.lighting_preset,
            brightness: self.light_brightness,
            tone_mapping: self.tone_mapping,
        }
    }

    pub const fn is_detail_editing(&self) -> bool {
        self.is_sculpting() || self.is_texturing()
    }

    pub const fn is_sculpting(&self) -> bool {
        matches!(self.active_tab, Tab::Morph)
    }

    pub const fn is_texturing(&self) -> bool {
        matches!(self.active_tab, Tab::Texture)
    }

    pub fn morph_compare_available(&self) -> bool {
        self.compare_origin().is_some()
    }

    fn compare_origin(&self) -> Option<&[[f64; 3]]> {
        let baked = self.workspace.result_output.as_deref()?.vertices.len();
        if self.edit_source_mode == EditSourceMode::CustomMorph
            && let Some(origin) = self.custom_morph_origin.as_deref()
            && origin.len() == baked
        {
            return Some(origin);
        }
        self.workspace
            .template
            .as_ref()
            .map(|template| template.mesh.vertices.as_slice())
            .filter(|vertices| vertices.len() == baked)
    }

    fn morph_compare_active(&self) -> bool {
        self.active_tab == Tab::Result
            && self.morph_compare_blend < 1.0
            && self.morph_compare_available()
    }

    pub fn morph_compare_signature(&self) -> Option<[f64; 2]> {
        self.morph_compare_active().then(|| {
            [
                f64::from(self.morph_compare_blend),
                COMPARE_PREVIEW_SIGNATURE_TAG,
            ]
        })
    }

    pub fn morph_compare_vertices(&self) -> Option<Vec<[f64; 3]>> {
        if !self.morph_compare_active() {
            return None;
        }
        let origin = self.compare_origin()?;
        let baked = &self.workspace.result_output.as_deref()?.vertices;
        let blend = f64::from(self.morph_compare_blend.clamp(0.0, 1.0));
        Some(
            origin
                .iter()
                .zip(baked)
                .map(|(from, to)| {
                    [
                        from[0] + (to[0] - from[0]) * blend,
                        from[1] + (to[1] - from[1]) * blend,
                        from[2] + (to[2] - from[2]) * blend,
                    ]
                })
                .collect(),
        )
    }

    #[must_use]
    pub fn vam_morph_directory(&self) -> Option<PathBuf> {
        let morphs = self
            .vam_root
            .as_ref()?
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Morphs")
            .join(match self.figure_sex {
                FigureSex::Male => "male",
                _ => "female",
            });
        morphs.is_dir().then_some(morphs)
    }

    pub fn package_output_directory(&self) -> Option<PathBuf> {
        if let Some(chosen) = self.package_directory.clone() {
            return Some(chosen);
        }
        Some(self.vam_root.as_ref()?.join("AddonPackages"))
    }

    pub fn vam_texture_directory(&self) -> Option<PathBuf> {
        let textures = self
            .vam_root
            .as_ref()?
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Textures");
        textures.is_dir().then_some(textures)
    }

    pub fn take_dialog_intent(&mut self) -> Option<DialogIntent> {
        self.pending_dialog.take()
    }

    pub fn take_pending_attention(&mut self) -> Option<AttentionTarget> {
        self.pending_attention.take()
    }

    pub fn flash_attention(&mut self, target: AttentionTarget) {
        self.pending_attention = Some(target);
    }

    pub fn template_load_active(&self) -> bool {
        matches!(
            self.workspace_load,
            WorkspaceLoadLifecycle::Requested {
                kind: WorkspaceLoadKind::Template,
                ..
            } | WorkspaceLoadLifecycle::Running {
                kind: WorkspaceLoadKind::Template,
                ..
            }
        )
    }

    pub fn take_scan_import_request(&mut self) -> Option<PathBuf> {
        let ScanImportLifecycle::Requested { path } = &self.scan_import else {
            return None;
        };
        let path = path.clone();
        self.scan_import = ScanImportLifecycle::Running { path: path.clone() };
        Some(path)
    }

    pub fn sculpt_eye_follow_vertices(
        &self,
        left_pitch_degrees: f64,
        right_pitch_degrees: f64,
    ) -> Option<Vec<[f64; 3]>> {
        let output = self.workspace.result_output.as_deref()?;
        let mut vertices = output.vertices.clone();
        let mut applied = false;
        let morph = |name: &str| {
            self.vam_cached_morphs
                .values()
                .find(|morph| morph.internal_name.eq_ignore_ascii_case(name))
        };
        for (pitch, suffix) in [(left_pitch_degrees, "L"), (right_pitch_degrees, "R")] {
            let weights = vkit_core::anatomy::eyelid_look_weights(-pitch.to_radians());
            if weights.is_rest() {
                continue;
            }
            for target in weights.named(suffix) {
                if target.weight <= 1.0e-6 {
                    continue;
                }

                let Some(resolved) = morph(&target.per_side)
                    .or_else(|| (suffix == "L").then(|| morph(target.shared)).flatten())
                else {
                    continue;
                };
                for &(vertex_id, delta) in resolved.deltas.iter() {
                    let Some(vertex) = vertices.get_mut(vertex_id as usize) else {
                        continue;
                    };
                    for axis in 0..3 {
                        vertex[axis] += delta[axis] * target.weight;
                    }
                }
                applied = true;
            }
        }
        applied.then_some(vertices)
    }

    pub fn take_vam_appearance_work(&mut self) -> Option<VaMWorkRequest> {
        let index = self
            .pending_vam_work
            .iter()
            .position(VaMWorkRequest::is_appearance_scan)?;
        self.pending_vam_work.remove(index)
    }

    pub fn take_vam_morph_work(&mut self) -> Option<VaMWorkRequest> {
        let index = self
            .pending_vam_work
            .iter()
            .position(|request| !request.is_appearance_scan())?;
        self.pending_vam_work.remove(index)
    }

    #[cfg(test)]
    pub fn take_vam_work(&mut self) -> Option<VaMWorkRequest> {
        self.pending_vam_work.pop_front()
    }

    pub fn take_skin_work(&mut self) -> Option<SkinPreviewRequest> {
        self.pending_skin_work.take()
    }

    pub fn take_texture_work(&mut self) -> Option<TextureWorkRequest> {
        self.pending_texture_work.pop_front()
    }

    pub fn active_skin_preview(&self) -> Option<Arc<SkinPreview>> {
        if let Some(baked) = self.texture_project.baked_preview() {
            return Some(baked);
        }

        if self.texture_project.hide_vam_skin_preview {
            // The plain base, so the head is never the untextured fallback.
            // Flipping the whole viewport to the solid view and waiting for a
            // bake to flip it back is what used to leave a white face wherever
            // that bake was late, failed, or never came.
            return self.neutral_skin_preview.as_ref().map(Arc::clone);
        }
        self.skin_preview.as_ref().map(Arc::clone)
    }

    pub fn take_hair_work(&mut self) -> Option<HairPreviewRequest> {
        self.pending_hair_work.take()
    }

    pub fn take_cancel_request(&mut self) -> Option<u64> {
        self.cancel_requested.take()
    }

    pub fn take_export_request(&mut self) -> Option<u64> {
        let ExportLifecycle::Queued { revision } = self.export else {
            return None;
        };
        self.export = ExportLifecycle::Writing { revision };
        Some(revision)
    }

    #[cfg(test)]
    pub fn overwrite_confirmation_path(&self) -> Option<&Path> {
        self.pending_overwrite_path.as_deref()
    }

    pub fn overwrite_confirmation_paths(&self) -> Vec<PathBuf> {
        let Some(primary) = self.pending_overwrite_path.as_deref() else {
            return Vec::new();
        };
        self.export_destination_paths(primary)
            .into_iter()
            .filter(|path| path.exists())
            .collect()
    }

    #[cfg(test)]
    pub fn alignment_undo_len(&self) -> usize {
        self.alignment_undo_history.len()
    }

    pub fn begin_alignment_transform(&mut self) -> bool {
        if self.block_mutation_while_busy() || self.alignment_transform_baseline.is_some() {
            return false;
        }
        self.alignment_transform_baseline = Some(self.transform);
        true
    }

    pub fn commit_alignment_transform(&mut self) -> bool {
        let Some(before) = self.alignment_transform_baseline.take() else {
            return false;
        };
        if before == self.transform {
            return false;
        }
        self.push_alignment_undo(before);
        self.refresh_active_scan_symmetry();
        true
    }

    pub fn cancel_alignment_transform(&mut self) -> bool {
        if self.block_mutation_while_busy() {
            return false;
        }
        let Some(before) = self.alignment_transform_baseline.take() else {
            return false;
        };
        if before == self.transform {
            return false;
        }
        let had_downstream_state = self.has_downstream_geometry();
        self.transform = before;
        self.refresh_active_scan_symmetry();
        self.invalidate_geometry_if_downstream(TextKey::ResultStale, had_downstream_state);
        true
    }

    pub fn undo_alignment_transform(&mut self) -> bool {
        if self.block_mutation_while_busy() {
            return false;
        }
        let had_downstream_state = self.has_downstream_geometry();
        if let Some(before) = self.alignment_transform_baseline.take()
            && before != self.transform
        {
            self.transform = before;
            self.refresh_active_scan_symmetry();
            self.invalidate_geometry_if_downstream(TextKey::ResultStale, had_downstream_state);
            return true;
        }
        let Some(before) = self.alignment_undo_history.pop_back() else {
            return false;
        };
        if before == self.transform {
            return false;
        }
        self.transform = before;
        self.refresh_active_scan_symmetry();
        self.invalidate_geometry_if_downstream(TextKey::ResultStale, had_downstream_state);
        true
    }

    #[must_use]
    pub fn morph_name_locale(&self) -> Locale {
        match self.morph_name_display {
            MorphNameDisplay::Localized => self.locale,
            MorphNameDisplay::Original => Locale::English,
        }
    }

    pub fn tab_available(&self, tab: Tab) -> bool {
        match tab {
            Tab::Alignment => true,

            Tab::Edit => self.workspace.template_geometry.is_some(),
            Tab::Result => {
                matches!(
                    self.result,
                    ResultState::Ready { source_revision, .. } if source_revision == self.revision
                ) && self.sculpt.is_ready()
                    && !self.morph_library.has_loading_controls()
            }
            Tab::Morph | Tab::Texture => {
                matches!(
                    self.result,
                    ResultState::Ready { source_revision, .. } if source_revision == self.revision
                ) && self.sculpt.is_ready()
            }
        }
    }

    pub(crate) fn create_landing_tab(&self) -> Tab {
        if self.tab_available(Tab::Edit) {
            Tab::Edit
        } else {
            Tab::Alignment
        }
    }

    pub fn can_enter_detail_from_template(&self) -> bool {
        self.scan_path.is_none() && self.workspace.template_geometry.is_some()
    }

    pub fn export_ready(&self) -> bool {
        matches!(
            self.result,
            ResultState::Ready { source_revision, .. } if source_revision == self.revision
        ) && self.result_preview_phase == ResultPreviewPhase::Save
            && !self.morph_preview_dirty
            && !self.morph_library.has_loading_controls()
            && self.sculpt.is_applied()
            && self.workspace.result_output.is_some()
    }

    pub const fn resolved_automatic_matching(&self) -> bool {
        true
    }

    pub fn scan_name(&self) -> Option<&str> {
        self.scan_path
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
    }

    pub fn add_surface_pin(&mut self, side: MeshSide, endpoint: SurfaceEndpoint) -> Option<usize> {
        if self.block_mutation_while_busy() {
            return None;
        }
        let index = self.workspace.pins.add(side, endpoint);
        self.active_tab = Tab::Edit;
        self.sync_pin_count_and_invalidate();
        Some(index)
    }

    pub fn add_surface_pins(
        &mut self,
        side: MeshSide,
        endpoints: impl IntoIterator<Item = SurfaceEndpoint>,
    ) -> Vec<usize> {
        if self.block_mutation_while_busy() {
            return Vec::new();
        }
        let indices = self.workspace.pins.add_many(side, endpoints);
        if !indices.is_empty() {
            self.active_tab = Tab::Edit;
            self.sync_pin_count_and_invalidate();
        }
        indices
    }

    pub fn delete_surface_pin(&mut self, side: MeshSide, index: usize) -> bool {
        if self.block_mutation_while_busy() {
            return false;
        }
        if !self.workspace.pins.delete(side, index) {
            return false;
        }
        self.active_tab = Tab::Edit;
        self.sync_pin_count_and_invalidate();
        true
    }

    pub fn begin_surface_pin_drag(&mut self, side: MeshSide, index: usize) -> bool {
        if self.block_mutation_while_busy() {
            return false;
        }
        if !self.workspace.pins.begin_drag(side, index) {
            return false;
        }
        self.active_tab = Tab::Edit;
        self.sync_pin_count_and_invalidate();
        true
    }

    pub fn move_surface_pin(
        &mut self,
        side: MeshSide,
        index: usize,
        endpoint: SurfaceEndpoint,
    ) -> bool {
        if self.block_mutation_while_busy() {
            return false;
        }
        self.workspace
            .pins
            .move_without_history(side, index, endpoint)
    }

    pub fn dispatch(&mut self, action: Action) {
        if action.is_mutating() && self.block_mutation_while_busy() {
            return;
        }
        let texture_undo = action
            .records_texture_undo()
            .then(|| self.texture_project.capture_undo_checkpoint())
            .flatten();
        let stage_was_reachable = self.tab_available(self.active_tab);
        let was_busy = self.busy();
        match action {
            Action::SetVarMetadata(field, value) => {
                let slot = match field {
                    VarMetadataField::Version => {
                        self.var_version_text =
                            value.chars().filter(char::is_ascii_digit).take(9).collect();
                        return;
                    }
                    VarMetadataField::Creator => {
                        self.missing_package_fields.creator = false;
                        &mut self.var_metadata.creator
                    }
                    VarMetadataField::Package => {
                        self.missing_package_fields.package = false;
                        &mut self.var_metadata.package
                    }
                    VarMetadataField::License => &mut self.var_metadata.license,
                    VarMetadataField::Description => &mut self.var_metadata.description,
                    VarMetadataField::Credits => &mut self.var_metadata.credits,
                    VarMetadataField::Instructions => &mut self.var_metadata.instructions,
                    VarMetadataField::PromotionalLink => &mut self.var_metadata.promotional_link,
                };
                *slot = value;
            }
            Action::SavePackage => self.save_package(),
            Action::ReplacePackage => self.replace_package(),
            Action::KeepPackage => self.pending_package_replace = None,
            Action::SetLocale(locale) => self.locale = locale,
            Action::RequestTab(tab) => self.request_tab(tab),
            Action::RequestOpenScan => {
                self.pending_dialog = Some(DialogIntent::OpenScan);
            }
            Action::RequestTextureImageBrowse(source_mode) => {
                self.pending_dialog = Some(DialogIntent::OpenTextureImage(source_mode));
            }
            Action::RequestVaMRootBrowse => {
                self.pending_dialog = Some(DialogIntent::ChooseVaMRoot);
            }
            Action::SetVaMRoot(path) => self.set_vam_root(path),

            Action::RefreshVaMCatalog => self.force_vam_catalog_refresh(),
            Action::SelectVaMEditSource(id) => self.select_vam_edit_source(&id),
            Action::SelectBaseFace => self.select_base_face(),
            Action::BeginDirectEdit => self.enter_direct_edit(Tab::Morph),
            Action::SetFigureSex(value) => self.set_figure_sex(value),
            Action::LoadScan(path) => self.load_scan(path),
            Action::FinishScanImport { path, outcome } => self.finish_scan_import(path, outcome),
            Action::LoadTemplate(path) => self.load_template(path),
            Action::LoadResult(path) => self.load_result(path),
            Action::FinishWorkspaceLoad { path, outcome } => {
                self.finish_workspace_load(path, outcome);
            }
            Action::AutoAlign => {
                if let Some(transform) = self.suggested_auto_match() {
                    self.active_tab = Tab::Alignment;
                    self.apply_alignment_transform(transform);
                } else {
                    self.status =
                        StatusMessage::new(TextKey::AlignmentPending, StatusTone::Warning);
                }
            }
            Action::PlacementReset => {
                self.active_tab = Tab::Alignment;
                if let Some(transform) = self.suggested_alignment() {
                    self.apply_alignment_transform(transform);
                } else {
                    self.status =
                        StatusMessage::new(TextKey::AlignmentPending, StatusTone::Warning);
                }
            }
            Action::BeginAlignmentTransform => {
                self.begin_alignment_transform();
            }
            Action::CommitAlignmentTransform => {
                self.commit_alignment_transform();
            }
            Action::CancelAlignmentTransform => {
                self.cancel_alignment_transform();
            }
            Action::SetScale(value) => {
                if value.iter().any(|axis| !axis.is_finite() || *axis <= 0.0) {
                    return;
                }
                self.active_tab = Tab::Alignment;

                let next = ScanTransform {
                    scale_xyz: value,
                    ..self.transform
                };
                self.apply_alignment_transform(next);
            }
            Action::SetScaleLinked(value) => self.scale_linked = value,
            Action::SetScaleAxis { axis, value } => {
                if axis >= 3 || !value.is_finite() || value <= 0.0 {
                    return;
                }
                let mut scale_xyz = self.transform.scale_xyz;
                if self.scale_linked {
                    let current = scale_xyz[axis];
                    if !current.is_finite() || current <= 0.0 {
                        return;
                    }
                    let factor = value / current;
                    if !factor.is_finite() || factor <= 0.0 {
                        return;
                    }
                    for axis_scale in &mut scale_xyz {
                        *axis_scale *= factor;
                    }
                    if scale_xyz
                        .iter()
                        .any(|axis_scale| !axis_scale.is_finite() || *axis_scale <= 0.0)
                    {
                        return;
                    }
                } else {
                    scale_xyz[axis] = value;
                }
                self.active_tab = Tab::Alignment;
                let next = ScanTransform {
                    scale_xyz,
                    ..self.transform
                };
                self.apply_alignment_transform(next);
            }
            Action::SetPosition { axis, value_cm } => {
                if axis >= 3 || !value_cm.is_finite() {
                    return;
                }
                self.active_tab = Tab::Alignment;
                let mut translation_cm = self.transform.translation_cm;
                translation_cm[axis] = value_cm;
                let next = ScanTransform {
                    translation_cm,
                    ..self.transform
                };
                self.apply_alignment_transform(next);
            }
            Action::SetRotation {
                axis,
                value_degrees,
            } => {
                if axis >= 3 || !value_degrees.is_finite() {
                    return;
                }
                self.active_tab = Tab::Alignment;
                let mut rotation_degrees = self.transform.rotation_degrees;
                rotation_degrees[axis] = value_degrees;
                let next = ScanTransform {
                    rotation_degrees,
                    ..self.transform
                };
                self.apply_alignment_transform(next);
            }
            Action::ToggleXMirror(value) => self.x_mirror = value,
            Action::RequestScanSymmetry(value) => {
                if value == self.scan_symmetry {
                    return;
                }
                if self.workspace.pins.pairs().is_empty() {
                    self.set_scan_symmetry(value);
                } else {
                    self.pending_symmetry_change = Some(value);
                }
            }
            Action::ConfirmScanSymmetry => {
                if let Some(value) = self.pending_symmetry_change.take() {
                    self.set_scan_symmetry(value);
                }
            }
            Action::CancelScanSymmetry => {
                self.pending_symmetry_change = None;
            }
            Action::SetEyeState(value) => {
                if value == self.eye_state {
                    return;
                }
                self.set_eye_state(value);
            }
            Action::ResetCamera => self.reset_relevant_cameras(),
            Action::SetStandardView(view) => self.set_relevant_standard_view(view),
            Action::ToggleProjection => self.toggle_relevant_projection(),
            Action::SetFov(value) => self.set_relevant_fov(value),
            Action::SetBaseViewMode(mode) => self.base_view_mode = mode,
            Action::SetSurfaceSmoothPasses(passes) => {
                self.surface_smooth_passes = passes.min(VAM_SMOOTH_PASSES_MAX);
            }
            Action::SetTooltipsEnabled(enabled) => self.tooltips_enabled = enabled,
            Action::SetShowOneSidedMorphs(show) => self.morph_library.show_one_sided = show,
            Action::SetSaveSection(section) => self.save_section = section,
            Action::ResetAllSettings => self.settings_reset = true,
            Action::MeasureCache => self.cache_measure_requested = true,
            Action::ClearCache => self.cache_clear_requested = true,
            Action::SetViewportBackgroundMode(mode) => {
                self.viewport_background_mode = mode;
            }
            Action::SetManualEyeGaze(value) => {
                self.manual_eye_gaze = value.map(sanitize_eye_gaze_axis);
                self.eye_gaze_mode = EyeGazeMode::Manual;
            }
            Action::SetEyeGazeMode(mode) => self.eye_gaze_mode = mode,
            Action::ResetEyeGaze => {
                self.manual_eye_gaze = [0.0; 2];
                self.eye_gaze_mode = EyeGazeMode::Manual;
                self.thaw_frozen_eye_gaze();
            }
            Action::FreezeEyeGaze(gaze) => self.freeze_eye_gaze(gaze),
            Action::SetCustomHeadSolidColor(value) => {
                self.custom_head_solid_color_rgb = value;
            }
            Action::SetG2SolidColor(value) => {
                self.g2_solid_color_rgb = value;
                if self.texture_project.hide_vam_skin_preview {
                    self.refresh_neutral_skin_preview();
                }
            }
            Action::SetWireframeColor(value) => {
                self.wireframe_color_rgb = value;
            }
            Action::ToggleWireframe(value) => self.wireframe_visible = value,
            Action::SetWireframeOpacity(value) => {
                if value.is_finite() {
                    self.wireframe_opacity = value.clamp(0.0, 1.0);
                }
            }
            Action::ToggleXray(value) => self.xray_visible = value,
            Action::SetXrayOpacity(value) => {
                if value.is_finite() {
                    self.xray_opacity = value.clamp(0.0, 1.0);
                }
            }
            Action::ToggleViewportToolPanel(panel) => {
                self.viewport_tool_panel =
                    (self.viewport_tool_panel != Some(panel)).then_some(panel);
            }
            Action::TogglePinNumbers(value) => self.pin_numbers = value,
            Action::SetAlignmentOpacity(value) => {
                if value.is_finite() {
                    self.alignment_opacity = value.clamp(0.0, 1.0);
                }
            }
            Action::SetAlignmentG2Opacity(value) => {
                if value.is_finite() {
                    self.alignment_g2_opacity = value.clamp(0.0, 1.0);
                }
            }
            Action::RotateLight(delta_radians) => {
                if delta_radians.is_finite() {
                    self.light_yaw_radians =
                        (self.light_yaw_radians + delta_radians).rem_euclid(std::f32::consts::TAU);
                }
            }
            Action::SetLightYaw(value) => {
                if value.is_finite() {
                    self.light_yaw_radians = value.rem_euclid(std::f32::consts::TAU);
                }
            }
            Action::SetLightingPreset(preset) => self.lighting_preset = preset,
            Action::SetLightBrightness(value) => {
                self.light_brightness = sanitize_brightness(value);
            }
            Action::SetToneMapping(value) => self.tone_mapping = value,
            Action::SetBloom(value) => self.bloom = value,
            Action::SetAmbientOcclusion(value) => self.ambient_occlusion = value,
            Action::SetCameraControl(mode) => self.camera_control = mode,
            Action::RestoreSession => self.answer_recovery(true),
            Action::DiscardSession => self.answer_recovery(false),
            Action::SetVignette(value) => self.vignette = value,
            Action::ToggleHelp => {
                self.help_visible = !self.help_visible;
                self.help_opened_for_projection = false;
            }
            Action::Undo => {
                if self.active_tab == Tab::Alignment && self.undo_alignment_transform() {
                } else if self.active_tab == Tab::Texture {
                    self.texture_project.undo();
                } else if self.active_tab == Tab::Morph {
                    self.undo_detail_edit();
                } else if self.workspace.pins.undo() {
                    self.active_tab = Tab::Edit;
                    self.sync_pin_count_and_invalidate();
                } else {
                    self.status = StatusMessage::new(TextKey::NativeCorePending, StatusTone::Info);
                }
            }
            Action::ResetPins => {
                if self.workspace.pins.reset() {
                    self.active_tab = Tab::Edit;
                    self.complete_pairs = 0;
                    self.invalidate_geometry(TextKey::PinsReset);
                }
            }
            Action::SetOutputPath(path) => {
                self.output_path = path;
                self.pending_overwrite_path = None;
            }
            Action::SetVaMExportDisplayName(value) => {
                let was_automatic = self.output_path_is_automatic_vam_default();
                self.vam_export_display_name = value;
                self.refresh_automatic_vam_output_path(was_automatic);
            }
            Action::SetVaMExportGroup(value) => {
                let was_automatic = self.output_path_is_automatic_vam_default();
                self.vam_export_group = value;
                self.refresh_automatic_vam_output_path(was_automatic);
            }
            Action::SetVaMExportRegion(value) => self.vam_export_region = value,
            Action::SetVaMExportIsPoseControl(value) => self.vam_export_is_pose_control = value,
            Action::SetVaMExportBoneCorrection(value) => self.vam_export_bone_correction = value,
            Action::BeginGeneration => self.begin_generation(),
            Action::BakeMorph => self.bake_morph(),
            Action::BeginSculptStroke {
                view_direction_local,
                brush_direction_local,
            } => self.begin_sculpt_stroke(view_direction_local, brush_direction_local),
            #[cfg(test)]
            Action::SculptDab(dab) => self.sculpt_dab(dab),
            Action::SculptDabDeferred(dab) => self.sculpt_dab_deferred(dab),
            Action::SyncSculptPreview => {
                if self.is_sculpting() && self.tab_available(Tab::Morph) {
                    self.sync_sculpt_preview();
                }
            }
            Action::EndSculptStroke => self.end_sculpt_stroke(),
            Action::SetSculptTarget { target, enabled } => {
                if self.is_sculpting() && self.tab_available(Tab::Morph) {
                    self.sculpt.set_target_enabled(target, enabled);
                }
            }
            Action::SetSculptGroupsCollapsed(collapsed) => {
                self.sculpt_groups_collapsed = collapsed;
            }
            Action::SetSculptBrushRadius(value) => {
                if value.is_finite() {
                    self.sculpt_brush_radius_points = value.clamp(8.0, 220.0);
                }
            }
            Action::SetSculptStrength(value) => {
                if value.is_finite() {
                    self.sculpt_strength = value.clamp(0.01, 1.0);
                }
            }
            Action::SetSculptXSymmetry(value) => {
                self.sculpt_x_symmetry = value;
                self.sculpt.set_x_symmetry(value);
            }
            Action::SetSculptConnectedTopologyOnly(value) => {
                self.sculpt_connected_topology_only = value;
                self.sculpt.set_connected_topology_only(value);
            }
            Action::ToggleSculptEyeTracking(value) => {
                self.sculpt_eye_tracking = value;

                if value {
                    self.thaw_frozen_eye_gaze();
                }
            }
            Action::ToggleSculptBackfaceMasking(value) => {
                self.sculpt.set_backface_masking(value);
            }
            Action::SetSculptFalloff(preset) => self.sculpt.set_falloff_preset(preset),
            Action::SetSculptBrush(brush) => self.sculpt_brush = brush,
            Action::ResetSculpt => self.reset_sculpt(),
            Action::SetTextureTool(tool) => {
                self.texture_project.end_undo_transaction();
                self.texture_project.set_active_tool(tool);
                self.offer_projection_help();
            }
            Action::SetTextureProjectionStencil(active) => {
                self.texture_project.end_undo_transaction();

                if active {
                    self.texture_project
                        .set_active_tool(TextureTool::Projection);
                } else if self.texture_project.active_tool == TextureTool::Projection {
                    self.texture_project.set_active_tool(TextureTool::PinPair);
                }
                self.offer_projection_help();
            }
            Action::SetTextureProjectionPlacement(placement) => {
                if placement.offset.iter().all(|value| value.is_finite())
                    && placement.scale.is_finite()
                    && placement.rotation.is_finite()
                {
                    self.texture_project.place_projection_stencil(placement);
                }
            }
            Action::SetTextureProjectionOpacity(value) => {
                if value.is_finite() {
                    self.texture_project.projection_opacity = value.clamp(0.0, 1.0);
                }
            }
            Action::SetTextureRetouchReverse(value) => {
                self.texture_project.retouch_reverse = value;
            }
            Action::BeginTextureEdit => self.texture_project.begin_undo_transaction(),
            Action::EndTextureEdit => {
                self.texture_project.end_undo_transaction();
                self.texture_project.end_clone_stroke();
            }
            Action::AddTextureImage(path, source_mode) => {
                self.add_texture_image(path, source_mode);
            }
            Action::RemoveTextureLayer(id) => {
                self.texture_project.remove_layer(id);
            }
            Action::MoveTextureLayerTo {
                id,
                insertion_index,
            } => self.texture_project.move_layer_to(id, insertion_index),
            Action::SelectTextureLayer(id) => self.texture_project.select_layer(id),
            Action::SetTextureWorkspaceSplit(value) => {
                if value.is_finite() {
                    self.texture_project.workspace_split_ratio = value.clamp(0.2, 0.8);
                }
            }
            Action::SetTextureSourceView { id, zoom, center } => {
                self.texture_project.set_source_view(id, zoom, center);
            }
            Action::SetTextureLayerVisible { id, visible } => {
                if let Some(layer) = self
                    .texture_project
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == id)
                {
                    layer.visible = visible;
                    self.texture_project.mark_dirty();
                }
            }
            Action::SetTextureLayerChannel { id, channel } => {
                if let Some(layer) = self
                    .texture_project
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == id)
                {
                    layer.channel = channel;

                    self.texture_project.mark_dirty();
                }
            }
            Action::SetTextureLayerOpacity { id, opacity } => {
                if opacity.is_finite()
                    && let Some(layer) = self
                        .texture_project
                        .layers
                        .iter_mut()
                        .find(|layer| layer.id == id)
                {
                    layer.opacity = opacity.clamp(0.0, 1.0);
                    self.texture_project.mark_dirty();
                }
            }
            Action::SetTextureLayerMirror { id, mirror } => {
                if let Some(layer) = self
                    .texture_project
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == id)
                {
                    layer.mirror = mirror;

                    layer.raster_revision = layer.raster_revision.saturating_add(1);
                    self.texture_project.mark_dirty();
                }
            }
            Action::SetTextureLayerBlendMode { id, blend_mode } => {
                if let Some(layer) = self
                    .texture_project
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == id)
                {
                    layer.blend_mode = blend_mode;
                    self.texture_project.mark_dirty();
                }
            }
            Action::SetTextureLayerAdjustments { id, adjustments } => {
                if let Some(layer) = self
                    .texture_project
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == id)
                {
                    layer.adjustments = adjustments;

                    self.texture_project.mark_dirty();
                }
            }
            Action::SetTextureLayerNormalStrength { id, strength } => {
                if strength.is_finite()
                    && let Some(layer) = self
                        .texture_project
                        .layers
                        .iter_mut()
                        .find(|layer| layer.id == id)
                {
                    layer.normal_strength = strength.clamp(0.0, 3.0);
                    self.texture_project.mark_dirty();
                }
            }
            Action::SetTextureLayerScalarInvert { id, invert } => {
                if let Some(layer) = self
                    .texture_project
                    .layers
                    .iter_mut()
                    .find(|layer| layer.id == id)
                {
                    layer.scalar_invert = invert;
                    self.texture_project.mark_dirty();
                }
            }
            Action::SetTextureMaskBrushRadius(value) => {
                if value.is_finite() {
                    self.texture_project.mask_brush_radius = value.clamp(0.002, 0.25);
                }
            }
            Action::SetTextureMaskBrushFalloff(falloff) => {
                self.texture_project.mask_brush_falloff = falloff;
            }
            Action::SetTextureMaskBrushOpacity(value) => {
                if value.is_finite() {
                    self.texture_project.mask_brush_opacity = value.clamp(0.01, 1.0);
                }
            }
            Action::SetTextureMaskPreviewEnabled(value) => {
                self.texture_project.mask_preview_enabled = value;
            }
            Action::SetTexturePinOpacity(value) => {
                if value.is_finite() {
                    self.texture_project.pin_opacity = value.clamp(0.15, 1.0);
                }
            }
            Action::AddTextureMaskDab {
                id,
                uv,
                source,
                subtract,
            } => {
                self.texture_project.add_mask_dab(id, uv, source, subtract);
            }
            Action::ClearTextureLayerMask(id) => self.texture_project.clear_mask(id),
            Action::SetTextureCloneSample(point) => {
                self.texture_project.set_clone_sample(point);
            }
            Action::AddTextureRetouchDab {
                id,
                point,
                tool,
                reverse,
            } => self
                .texture_project
                .apply_retouch_dab(id, tool, point, reverse),
            Action::ResetTextureLayer(id) => {
                self.texture_project.reset_layer(id);
            }
            Action::MatchTextureLayerColor(id) => {
                self.texture_project.match_layer_color_to_previous(id);
            }
            Action::SetTextureResolution(value) => {
                if crate::texture_project::is_texture_resolution(value) {
                    self.texture_project.resolution = value;
                    self.texture_project.mark_dirty();
                }
            }
            Action::SetTextureBoundaryFeather(value) => {
                let deepest = self.texture_project.max_boundary_feather_pixels();
                self.texture_project.boundary_feather_pixels = value.min(deepest);
                self.texture_project.mark_dirty();
            }
            Action::SetTextureBakeBase(value) => {
                if value == TextureBakeBase::CurrentSkin && !self.skin_base_available() {
                    self.status =
                        StatusMessage::new(TextKey::SkinPresetRequired, StatusTone::Warning);
                    self.flash_attention(
                        if self.viewport_tool_panel == Some(ViewportToolPanel::Skin) {
                            AttentionTarget::SkinTextureMode
                        } else {
                            AttentionTarget::SkinPanel
                        },
                    );
                    return;
                }
                self.texture_project.bake_base = value;
                self.texture_project.mark_dirty();
            }
            Action::SetTextureHideVaMSkin(value) => {
                if value == self.texture_project.hide_vam_skin_preview {
                    return;
                }
                self.texture_project.hide_vam_skin_preview = value;
                if value {
                    self.refresh_neutral_skin_preview();
                }
                self.texture_project.mark_dirty();
            }
            Action::SetTextureOutputPbr(value) => self.texture_project.output_pbr = value,
            Action::RequestTextureBake(quality) => self.request_texture_bake(quality),
            Action::FinishTextureDecode {
                request_id,
                layer_id,
                outcome,
            } => {
                debug_assert!(request_id > 0);
                self.texture_project.finish_decode(layer_id, outcome);
            }
            Action::FinishTextureBake {
                request_id,
                outcome,
            } => self.finish_texture_bake(request_id, outcome),
            #[cfg(test)]
            Action::ApplySculpt => self.apply_sculpt(),
            Action::ExportResult => self.request_export(),
            Action::SaveTextures => self.save_textures(),
            Action::ConfirmOverwrite => self.confirm_overwrite(),
            Action::CancelOverwrite => self.pending_overwrite_path = None,
            Action::SetTextureExportPrefix(value) => {
                self.texture_project.export_prefix = value;
            }
            Action::ConfirmTextureOverwrite => self.write_textures(),
            Action::CancelTextureOverwrite => self.pending_texture_overwrite.clear(),
            Action::CancelGeneration => {
                if let Some((job_id, _)) = self.active_job() {
                    self.cancel_requested = Some(job_id);
                    self.status = StatusMessage::new(TextKey::Cancelling, StatusTone::Info);
                }
            }
            Action::ReportProgress {
                job_id,
                source_revision,
                progress,
            } => {
                if matches!(self.result, ResultState::Running { job_id: active, source_revision: active_revision } if active == job_id && active_revision == source_revision)
                {
                    let floor = self.progress.map_or(0.0, |current| current.fraction);
                    self.progress =
                        Some(Progress::new(progress.stage, progress.fraction.max(floor)));
                }
            }
            Action::ReportImportProgress { path, progress } => {
                if self.scan_import.active_path() == Some(path.as_path()) {
                    let progress = ImportProgress::with_size(
                        progress.phase,
                        progress.fraction,
                        progress.source_triangles,
                    );
                    let fraction = self.import_progress.map_or(progress.fraction, |current| {
                        current.fraction.max(progress.fraction)
                    });

                    let triangles = progress.source_triangles.or_else(|| {
                        self.import_progress
                            .and_then(|current| current.source_triangles)
                    });
                    self.import_progress = Some(ImportProgress::with_size(
                        progress.phase,
                        fraction,
                        triangles,
                    ));
                }
            }
            Action::FinishGeneration {
                job_id,
                source_revision,
                outcome,
                morph_available,
            } => self.finish_generation(job_id, source_revision, outcome, morph_available),
            Action::FinishExport {
                source_revision,
                outcome,
            } => self.finish_export(source_revision, outcome),
            Action::ReportExportProgress {
                source_revision,
                fraction,
            } => self.report_export_progress(source_revision, fraction),
            Action::ToggleScanOverlay(value) => self.scan_overlay = value,
            Action::ToggleHairVisible(value) => self.hair_visible = value,

            Action::SetScanFidelity(value) => {
                self.scan_fidelity = value.clamp(
                    *vkit_core::fit::SCAN_FIDELITY_RANGE.start() as f32,
                    *vkit_core::fit::SCAN_FIDELITY_RANGE.end() as f32,
                );
            }
            Action::OpenSettings => self.settings_open = true,
            Action::CloseSettings => self.settings_open = false,
            Action::SpendHairSettle(elapsed) => {
                self.hair_settle_seconds = (self.hair_settle_seconds - elapsed).max(0.0);
            }
            Action::SetOverlayOpacity(value) => self.overlay_opacity = value.clamp(0.0, 1.0),
            Action::SetMorphCompareBlend(value) => {
                self.morph_compare_blend = if value.is_finite() {
                    value.clamp(0.0, 1.0)
                } else {
                    1.0
                };
            }
            Action::ToggleResultTearLacrimals(value) => {
                self.show_result_tear_lacrimals = value;
            }
            Action::ToggleResultEyelashes(value) => self.show_result_eyelashes = value,
            Action::SetMorphQuery(query) => self.morph_library.query = query,
            Action::SetMorphCategoryFilter(filter) => {
                if self.morph_library.category_filter != filter {
                    self.morph_library.category_filter = filter;
                    self.morph_library.query.clear();
                }
            }
            Action::SetMorphListFilter(filter) => {
                self.morph_library.set_list_filter(filter);
            }
            Action::SetEyeClosure(value) => self.set_eye_closure(value),
            Action::SetFaceMorph { id, value } => self.set_face_morph(&id, value),
            Action::SetDefaultSkin(id) => {
                self.default_skin_id = if self.default_skin_id == id { None } else { id };
            }
            Action::SetLastSkin(id) => self.last_skin_id = id,
            Action::SelectVaMSkin(preset_id) => self.select_vam_skin(preset_id.as_deref()),
            Action::FinishVaMSkin {
                request_id,
                preset_id,
                outcome,
            } => self.finish_vam_skin(request_id, &preset_id, outcome),
            Action::SelectVaMHair(preset_id) => self.select_vam_hair(preset_id.as_deref()),
            Action::FinishVaMHair {
                request_id,
                preset_id,
                outcome,
            } => self.finish_vam_hair(request_id, &preset_id, outcome),
            Action::ReportVaMCatalogProgress {
                catalog_revision,
                fraction,
            } => self.report_vam_catalog_progress(catalog_revision, fraction),
            Action::FinishVaMCatalog {
                catalog_revision,
                outcome,
            } => self.finish_vam_catalog(catalog_revision, outcome),
            Action::FinishVaMMorphCatalog {
                catalog_revision,
                outcome,
            } => self.finish_vam_morph_catalog(catalog_revision, outcome),
            Action::FinishVaMMorph {
                catalog_revision,
                geometry_revision,
                control_id,
                outcome,
            } => self.finish_vam_morph(catalog_revision, geometry_revision, &control_id, outcome),
            Action::SetPackageFromThisHead(value) => {
                self.package_from_this_head = value;
                // The two are a choice, not two switches. Turning the head back
                // on drops the attachments rather than leaving a list that is
                // still on screen but no longer packaged -- and turning it off
                // is how picking a file stops the head being baked at all,
                // which is what used to fail with "identical to the loaded G2
                // template" when the head had not been touched.
                if value {
                    self.package_morphs.clear();
                    self.package_textures.clear();
                }
            }
            Action::AddPackageFiles(slot, paths) => {
                // Picking a file is the same statement as choosing the file
                // route, so it makes it rather than requiring a second click.
                self.package_from_this_head = false;
                let paths = match slot {
                    // A VaM morph is two files that are useless apart: the .vmi
                    // describes it and the .vmb holds the deltas. Picking one
                    // and shipping a package without the other is a mistake
                    // nobody makes on purpose, so the twin comes along.
                    PackageSlot::Morph => with_morph_twins(paths),
                    PackageSlot::Texture => paths,
                };
                let list = match slot {
                    PackageSlot::Morph => &mut self.package_morphs,
                    PackageSlot::Texture => &mut self.package_textures,
                };

                for path in paths {
                    if !list.iter().any(|existing| existing.path == path) {
                        list.push(PackageFile::new(path));
                    }
                }
            }
            Action::RemovePackageFile(slot, index) => {
                let list = match slot {
                    PackageSlot::Morph => &mut self.package_morphs,
                    PackageSlot::Texture => &mut self.package_textures,
                };
                if index < list.len() {
                    list.remove(index);
                }
            }
            Action::SetPackageDirectory(directory) => self.package_directory = directory,
            Action::ResetMorphs => self.reset_morphs(),
        }
        self.texture_project.commit_undo_checkpoint(texture_undo);

        if stage_was_reachable || (was_busy && !self.busy()) {
            self.settle_active_tab();
        }

        if !self.is_texturing() && self.texture_project.active_tool == TextureTool::Projection {
            self.texture_project.set_active_tool(TextureTool::PinPair);
        }
        self.settle_projection_entry();
    }

    /// A stencil raised over an image it has never been placed against starts centred on the pane,
    /// with the head framed behind it. Re-raising the stencil over a layer the user has already
    /// dragged leaves both the placement and the camera exactly where they left them.
    fn settle_projection_entry(&mut self) {
        if !self.texture_project.projection_stencil()
            || !self.texture_project.stencil_needs_fresh_placement()
        {
            return;
        }
        self.texture_project.centre_projection_stencil();
        if let Some(bounds) = self.result_head_bounds() {
            self.workspace.result_camera.frame(bounds);
        }
    }

    fn set_scan_symmetry(&mut self, value: ScanSymmetry) {
        self.pending_symmetry_change = None;

        self.texture_project.invalidate_scan_projection();
        if value == self.scan_symmetry {
            return;
        }

        let had_downstream_state = self.has_downstream_geometry();
        let had_pins = !self.workspace.pins.pairs().is_empty();
        let mode = match value {
            ScanSymmetry::Original => SymmetryMode::Off,
            ScanSymmetry::PositiveX => SymmetryMode::PositiveX,
            ScanSymmetry::NegativeX => SymmetryMode::NegativeX,
        };
        let transform = self.current_scan_model_transform();
        if let Err(error) = self
            .workspace
            .set_scan_symmetry_with_transform(mode, transform)
        {
            self.status = StatusMessage::with_detail(
                TextKey::GenerationFailed,
                StatusTone::Error,
                error.to_string(),
            );
            return;
        }
        self.scan_symmetry = value;

        self.workspace.pins.clear_for_mesh_change();
        self.complete_pairs = 0;
        self.invalidate_geometry_if_downstream(
            if had_pins {
                TextKey::PinsReset
            } else {
                TextKey::ResultStale
            },
            had_downstream_state,
        );
    }

    fn settle_active_tab(&mut self) {
        if self.busy() || self.tab_available(self.active_tab) {
            return;
        }
        self.active_tab = self.create_landing_tab();
    }

    fn request_tab(&mut self, tab: Tab) {
        if tab == Tab::Alignment {
            self.placed_head = true;
        }

        // No scan opened means the G2 template *is* the head, so every stage
        // downstream of the fit has something to work on. This used to say
        // `tab == Tab::Morph`, which is why sculpt let you in from a cold start
        // and texture and save did not -- not a rule about what they need, just
        // a condition that was never widened when the others were added.
        if matches!(tab, Tab::Morph | Tab::Texture | Tab::Result)
            && self.can_enter_detail_from_template()
            && !self.tab_available(tab)
        {
            if self.busy() {
                self.status = StatusMessage::new(TextKey::MorphLoading, StatusTone::Warning);
                return;
            }
            self.set_edit_source_mode(EditSourceMode::CustomMorph);
            self.enter_direct_edit(tab);
            return;
        }
        if !self.tab_available(tab) {
            let key = match tab {
                Tab::Alignment => return,
                Tab::Edit => TextKey::AlignmentPending,
                Tab::Result => TextKey::ResultUnavailable,
                Tab::Morph | Tab::Texture => TextKey::MorphUnavailable,
            };
            self.status = StatusMessage::new(key, StatusTone::Warning);
            return;
        }
        let transition = match tab {
            Tab::Morph | Tab::Texture => self.compose_current_morphs(ResultPreviewPhase::Morph),
            Tab::Result => self.commit_save_preview(),
            Tab::Alignment | Tab::Edit => Ok(()),
        };
        match transition {
            Ok(()) => self.active_tab = tab,
            Err(detail) => {
                self.status = StatusMessage::with_detail(
                    TextKey::GenerationFailed,
                    StatusTone::Error,
                    detail,
                );
            }
        }
    }

    fn capture_edit_carry(&self) -> Option<CarriedEdit> {
        let morph_values = self.morph_library.snapshot_values();
        let sculpt = self
            .sculpt
            .displacement()
            .filter(|delta| delta.iter().flatten().any(|axis| *axis != 0.0));
        if sculpt.is_none() && !self.morph_library.has_modified_controls() {
            return None;
        }
        Some(CarriedEdit {
            morph_values,
            sculpt,
            eye_closure: self.eye_closure,
        })
    }

    fn offer_projection_help(&mut self) {
        if self.texture_project.active_tool != TextureTool::Projection
            || self.projection_help_offered
        {
            return;
        }
        self.projection_help_offered = true;
        if !self.help_visible {
            self.help_visible = true;
            self.help_opened_for_projection = true;
        }
    }

    fn select_vam_edit_source(&mut self, id: &str) {
        let Some(source) = self
            .vam_edit_sources
            .iter()
            .find(|source| source.stable_id == id)
            .cloned()
        else {
            self.status = StatusMessage::with_detail(
                TextKey::VaMMorphPairRejected,
                StatusTone::Error,
                format!("VaM edit source is unavailable: {id}"),
            );
            return;
        };

        if !self.look_suits_current_figure(&source) {
            self.status = StatusMessage::with_detail(
                TextKey::LookBelongsToOtherFigure,
                StatusTone::Warning,
                source.label.clone(),
            );
            return;
        }

        self.pending_edit_carry = self.capture_edit_carry();
        self.set_edit_source_mode(EditSourceMode::CustomMorph);
        self.selected_vam_edit_source_id = Some(source.stable_id.clone());
        self.pending_direct_edit_source = Some(source.clone());
        self.clear_generated_buffers();
        self.result = ResultState::Empty;
        self.start_pending_direct_edit_source_if_ready();
    }

    pub fn look_suits_current_figure(&self, source: &VaMEditSource) -> bool {
        match source.sex {
            Some(vkit_core::vam::SkinSex::Female) => self.figure_sex == FigureSex::Female,
            Some(vkit_core::vam::SkinSex::Male) => self.figure_sex == FigureSex::Male,
            Some(vkit_core::vam::SkinSex::Unknown) | None => true,
        }
    }

    fn start_pending_direct_edit_source_if_ready(&mut self) {
        if self.edit_source_mode != EditSourceMode::CustomMorph {
            return;
        }
        let Some(source) = self.pending_direct_edit_source.as_ref() else {
            return;
        };
        if self.workspace_load.is_active() || self.workspace.template_geometry.is_none() {
            return;
        }
        if source.kind == VaMEditSourceKind::AppearancePreset
            && matches!(self.vam_morph_cache_status, VaMMorphCacheStatus::Loading)
        {
            return;
        }
        let source = self
            .pending_direct_edit_source
            .take()
            .expect("direct edit source checked above");
        self.request_workspace_load(
            WorkspaceLoadKind::DirectEditSource,
            source.path.clone(),
            TextKey::MorphLoading,
        );
        self.pending_direct_edit_source = Some(source);
    }

    fn enter_direct_edit(&mut self, landing: Tab) {
        if self.edit_source_mode != EditSourceMode::CustomMorph {
            return;
        }
        self.begin_template_edit(false, landing);
    }

    fn select_base_face(&mut self) {
        if self.busy() {
            self.status = StatusMessage::new(TextKey::MorphLoading, StatusTone::Warning);
            return;
        }
        self.selected_vam_edit_source_id = None;
        self.pending_direct_edit_source = None;
        self.clear_morph_reset_undo();
        self.morph_value_history.clear();
        self.morph_edit_open = None;
        self.morph_library.reset();
        self.custom_morph_origin = None;
        self.morph_compare_blend = 1.0;
        self.begin_template_edit(true, Tab::Morph);
    }

    fn resolve_eyelid_control_now(&mut self, id: &str) {
        if !self.morph_library.needs_resolution(id) {
            return;
        }
        let Some(descriptor) = self.vam_cached_morphs.get(id) else {
            return;
        };
        let (Some(geometry), Some(canonical_faces)) = (
            self.workspace.template_geometry.as_deref(),
            self.vam_morph_faces.as_ref(),
        ) else {
            return;
        };
        let Ok(target) = vkit_core::formats::MorphTarget::from_sparse_deltas_shared(
            format!("builtin:{}", descriptor.internal_name),
            geometry.vertices.len(),
            descriptor.deltas.as_slice(),
            Arc::clone(canonical_faces),
            vkit_core::formats::MorphAuthoring::vam(
                descriptor.minimum,
                descriptor.maximum,
                descriptor.default,
            ),
        ) else {
            return;
        };
        let _ = self
            .morph_library
            .install_resolved_target(id, Arc::new(target));
    }

    pub(crate) fn builtin_morph_id_by_internal_name(&self, internal_name: &str) -> Option<String> {
        self.morph_library
            .controls()
            .iter()
            .find(|control| {
                control.source == MorphSource::VaMBuiltin && control.id == internal_name
            })
            .map(|control| control.id.clone())
    }

    fn freeze_eye_gaze(&mut self, gaze: [f32; 2]) {
        let gaze = gaze.map(sanitize_eye_gaze_axis);
        for (left_eye, suffix) in [(true, "L"), (false, "R")] {
            let [_, pitch] = eye_angles_from_gaze(gaze, left_eye);
            let weights = vkit_core::anatomy::eyelid_look_weights(-pitch.to_radians());
            for target in weights.named(suffix) {
                let id = self
                    .builtin_morph_id_by_internal_name(&target.per_side)
                    .or_else(|| {
                        (suffix == "L")
                            .then(|| self.builtin_morph_id_by_internal_name(target.shared))
                            .flatten()
                    });
                if let Some(id) = id {
                    self.resolve_eyelid_control_now(&id);
                    self.set_face_morph(&id, target.weight as f32);
                }
            }
        }
        self.frozen_eye_gaze = Some(gaze);
        self.sculpt_eye_tracking = false;
        self.manual_eye_gaze = gaze;
        self.eye_gaze_mode = EyeGazeMode::Manual;
        if let Err(detail) = self.compose_current_morphs(self.result_preview_phase) {
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, detail);
        }
    }

    fn thaw_frozen_eye_gaze(&mut self) {
        if self.frozen_eye_gaze.take().is_none() {
            return;
        }
        for suffix in ["L", "R"] {
            let weights = vkit_core::anatomy::EyelidLookWeights::default();
            for target in weights.named(suffix) {
                let id = self
                    .builtin_morph_id_by_internal_name(&target.per_side)
                    .or_else(|| {
                        (suffix == "L")
                            .then(|| self.builtin_morph_id_by_internal_name(target.shared))
                            .flatten()
                    });
                if let Some(id) = id {
                    self.resolve_eyelid_control_now(&id);
                    self.set_face_morph(&id, 0.0);
                }
            }
        }
        if let Err(detail) = self.compose_current_morphs(self.result_preview_phase) {
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, detail);
        }
    }

    pub(crate) fn eyelid_control_live_value(&self, control_id: &str) -> Option<f32> {
        if !self.sculpt_eye_tracking {
            return None;
        }
        let (role, left_eye) = eyelid_role_of_control_id(control_id)?;
        let [_, pitch] = eye_angles_from_gaze(self.manual_eye_gaze, left_eye.unwrap_or(true));
        let weights = vkit_core::anatomy::eyelid_look_weights(-pitch.to_radians());
        Some(match role {
            vkit_core::anatomy::EyelidLookRole::TopDown => weights.top_down as f32,
            vkit_core::anatomy::EyelidLookRole::TopUp => weights.top_up as f32,
            vkit_core::anatomy::EyelidLookRole::BottomDown => weights.bottom_down as f32,
            vkit_core::anatomy::EyelidLookRole::BottomUp => weights.bottom_up as f32,
        })
    }

    pub(crate) fn gaze_for_eyelid_control_value(
        &self,
        control_id: &str,
        value: f32,
    ) -> Option<[f32; 2]> {
        if !self.sculpt_eye_tracking {
            return None;
        }
        let (role, _) = eyelid_role_of_control_id(control_id)?;
        let pitch_down_radians =
            vkit_core::anatomy::gaze_pitch_for_lid_weight(role, f64::from(value));
        let pitch_view_degrees = -pitch_down_radians.to_degrees();
        let gaze_y = if pitch_view_degrees < 0.0 {
            pitch_view_degrees / 30.0
        } else {
            pitch_view_degrees / 20.0
        };
        Some([self.manual_eye_gaze[0], gaze_y.clamp(-1.0, 1.0) as f32])
    }

    fn begin_template_edit(&mut self, force_reinstall: bool, landing: Tab) {
        if self.busy() {
            self.status = StatusMessage::new(TextKey::MorphLoading, StatusTone::Warning);
            return;
        }
        let started = std::time::Instant::now();
        let mut install_ms = 0.0;
        if force_reinstall || !self.tab_available(Tab::Morph) {
            if self.selected_vam_edit_source_id.is_some() {
                self.status = StatusMessage::new(TextKey::MorphLoading, StatusTone::Warning);
                return;
            }

            let output = match self.workspace.template_ordered_obj() {
                Some(cached) => cached,
                None => {
                    let Some(template) = self.workspace.template_geometry.as_deref() else {
                        self.status =
                            StatusMessage::new(TextKey::TemplatePending, StatusTone::Warning);
                        return;
                    };
                    match template.to_ordered_obj(None) {
                        Ok(output) => Arc::new(output),
                        Err(error) => {
                            self.status = StatusMessage::with_detail(
                                TextKey::GenerationFailed,
                                StatusTone::Error,
                                error.to_string(),
                            );
                            return;
                        }
                    }
                }
            };
            let install_started = std::time::Instant::now();
            if let Err(detail) = self.install_direct_edit_output(output, None, Vec::new(), 0) {
                self.status = StatusMessage::with_detail(
                    TextKey::GenerationFailed,
                    StatusTone::Error,
                    detail,
                );
                return;
            }
            install_ms = install_started.elapsed().as_secs_f64() * 1000.0;
        }
        let compose_started = std::time::Instant::now();
        if let Err(detail) = self.compose_current_morphs(ResultPreviewPhase::Morph) {
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, detail);
            return;
        }
        // Landing where the click asked rather than always on sculpt. Being
        // dragged to a tab you did not press is what made the old behaviour
        // read as "sculpt must be visited first".
        self.active_tab = landing;
        if landing == Tab::Result {
            // Save composes its own preview, and entering it without one shows
            // an empty stage.
            if let Err(detail) = self.commit_save_preview() {
                self.status = StatusMessage::with_detail(
                    TextKey::GenerationFailed,
                    StatusTone::Error,
                    detail,
                );
            }
        }

        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Debug,
            "runtime",
            "direct_edit_timing",
            &format!(
                "install_ms={install_ms:.3}; compose_ms={:.3}; total_ms={:.3}",
                compose_started.elapsed().as_secs_f64() * 1000.0,
                started.elapsed().as_secs_f64() * 1000.0,
            ),
        );
    }

    fn set_edit_source_mode(&mut self, mode: EditSourceMode) {
        if self.edit_source_mode == mode {
            return;
        }
        self.edit_source_mode = mode;
        self.active_tab = Tab::Alignment;
        self.custom_morph_origin = None;
        self.morph_compare_blend = 1.0;
        self.clear_generated_buffers();
        self.result = ResultState::Empty;
        self.clear_morph_reset_undo();
        self.morph_value_history.clear();
        self.morph_edit_open = None;
        self.morph_library.reset();
        match mode {
            EditSourceMode::ScanHead => {
                self.selected_vam_edit_source_id = None;
                self.pending_direct_edit_source = None;
            }
            EditSourceMode::CustomMorph => {
                self.scan_path = None;
                self.workspace.clear_scan();
                self.complete_pairs = 0;
                self.transform = ScanTransform::default();
                self.clear_alignment_history();
            }
        }
    }

    fn install_direct_edit_output(
        &mut self,
        output: Arc<OrderedObjMesh>,
        source_path: Option<&Path>,
        missing_morphs: Vec<String>,
        resolved_morphs: usize,
    ) -> Result<(), String> {
        let carry = self.pending_edit_carry.take();
        self.clear_generated_buffers();
        self.clear_morph_reset_undo();
        self.morph_value_history.clear();
        self.morph_edit_open = None;
        self.morph_library.reset();
        let carried_morphs = carry
            .as_ref()
            .is_some_and(|carry| self.morph_library.restore_values(&carry.morph_values));
        self.custom_morph_origin = Some(Arc::new(output.vertices.clone()));
        self.workspace
            .install_result(output)
            .map_err(|error| error.to_string())?;
        self.morph_compare_blend = 1.0;
        self.result = ResultState::Ready {
            source_revision: self.revision,
            morph_available: self.workspace.eye_morph.is_some(),
        };
        self.fit_reference_value = Some(0.0);
        self.eye_closure = carry.as_ref().map_or(0.0, |carry| carry.eye_closure);
        self.begin_sculpt_stage()?;
        if let Some(delta) = carry.and_then(|carry| carry.sculpt) {
            match self.sculpt.graft_displacement(&delta) {
                Ok(_) => {}
                Err(_) => {
                    self.status =
                        StatusMessage::new(TextKey::SculptNotCarried, StatusTone::Warning);
                }
            }
        }
        self.result_preview_phase = ResultPreviewPhase::Sculpt;
        self.morph_preview_dirty = false;
        self.output_topology = OutputTopology::VaM;

        self.vam_export_display_name.clear();
        self.clear_output_destination();
        if let Some(path) = source_path.filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("vmi"))
        }) {
            self.output_path = path.to_string_lossy().into_owned();
        } else {
            self.set_default_vam_output_path_if_empty();
        }
        // Naming the morphs that went missing tells the user nothing they can
        // act on: the file is gone, so what it would have done to this face is
        // exactly the thing that can no longer be determined. Interrupting with
        // an alarm of unknowable size, on load after load, trains the user to
        // ignore the status line. The names still go to vkit.log -- now with
        // the count that resolved beside them, which is what was missing when
        // reading that log. The user is only stopped when nothing loaded at
        // all, which certainly means the base face is on screen.
        if !missing_morphs.is_empty() {
            let _ = crate::diagnostics::record(
                crate::diagnostics::Severity::Warning,
                "appearance",
                "missing_morphs",
                &format!(
                    "resolved={resolved_morphs}; missing={}; {}",
                    missing_morphs.len(),
                    missing_morphs.join(" | ")
                ),
            );
        }
        self.status = if missing_morphs.is_empty() || resolved_morphs > 0 {
            StatusMessage::new(TextKey::MorphPreviewUpdated, StatusTone::Success)
        } else {
            StatusMessage::new(TextKey::SourceMorphMissing, StatusTone::Warning)
        };
        if carried_morphs {
            self.morph_preview_dirty = true;
            if let Err(detail) = self.compose_current_morphs(ResultPreviewPhase::Morph) {
                self.morph_preview_dirty = false;
                self.result_preview_phase = ResultPreviewPhase::Sculpt;
                self.status = StatusMessage::with_detail(
                    TextKey::MorphPreviewUpdated,
                    StatusTone::Warning,
                    detail,
                );
            }
        }
        Ok(())
    }

    fn restore_sculpt_preview(&mut self) -> Result<(), String> {
        let vertices = self
            .sculpt
            .working_mesh()
            .map(|mesh| mesh.vertices.clone())
            .ok_or_else(|| "sculpt working mesh is unavailable".to_owned())?;
        self.sculpt.clear_presentation_vertices();
        self.refresh_vam_full_preview(&vertices)?;
        self.workspace
            .update_result_preview_vertices(vertices)
            .map_err(|error| error.to_string())?;
        self.result_preview_phase = ResultPreviewPhase::Sculpt;
        Ok(())
    }

    fn refresh_vam_full_preview(&mut self, canonical_vertices: &[[f64; 3]]) -> Result<(), String> {
        let has_genital_controls = self
            .morph_library
            .controls()
            .iter()
            .any(|control| control.genital_target.is_some());
        if !has_genital_controls {
            self.vam_full_preview = None;
            return Ok(());
        }
        let provider = self
            .vam_geometry_provider
            .as_ref()
            .map(Arc::clone)
            .ok_or_else(|| {
                "Genital morph preview requires its validated user-owned VaM geometry provider"
                    .to_owned()
            })?;
        let expected_sex = figure_sex_to_geometry_sex(self.figure_sex);
        if provider.sex() != expected_sex {
            return Err(format!(
                "Genital morph preview provider is {:?}, but the current figure is {:?}",
                provider.sex(),
                expected_sex
            ));
        }
        if canonical_vertices.len() != provider.daz_anchor().vertices.len() {
            return Err(format!(
                "Genital morph preview requires {} canonical vertices, got {}",
                provider.daz_anchor().vertices.len(),
                canonical_vertices.len()
            ));
        }
        let mut target = provider.daz_anchor().clone();
        target.vertices.clone_from_slice(canonical_vertices);
        target.validate().map_err(|error| error.to_string())?;
        let mut full = provider
            .compose_vam_output(&target)
            .map_err(|error| error.to_string())?;
        let applications = self
            .morph_library
            .genital_applications()
            .map_err(|error| error.to_string())?;
        apply_genital_applications_to_vam_mesh(&provider, &mut full, &applications)
            .map_err(|error| error.to_string())?;
        self.vam_full_preview = Some(Arc::new(full));
        Ok(())
    }

    fn compose_current_morphs(&mut self, phase: ResultPreviewPhase) -> Result<(), String> {
        let morph_available = matches!(
            self.result,
            ResultState::Ready {
                morph_available: true,
                ..
            }
        );
        if morph_available {
            self.rebuild_morph_preview(self.eye_closure)?;
        } else {
            self.restore_sculpt_preview()?;
        }
        self.morph_preview_dirty = false;
        self.result_preview_phase = phase;
        Ok(())
    }

    fn commit_save_preview(&mut self) -> Result<(), String> {
        if self.morph_library.has_loading_controls() {
            return Err("morph controls are still loading".to_owned());
        }

        self.sculpt_eye_tracking = false;
        self.manual_eye_gaze = [0.0; 2];
        self.eye_gaze_mode = EyeGazeMode::Manual;

        self.morph_compare_blend = 1.0;

        if !self.texture_project.baked_preview_enabled || self.texture_project.hide_vam_skin_preview
        {
            self.texture_project.baked_preview_enabled = true;
            self.texture_project.hide_vam_skin_preview = false;
            self.texture_project.mark_dirty();
        }
        self.compose_current_morphs(ResultPreviewPhase::Save)?;
        self.sculpt.mark_applied();
        Ok(())
    }

    fn sync_pin_count_and_invalidate(&mut self) {
        self.complete_pairs = self.workspace.pins.complete_count();
        self.invalidate_geometry(TextKey::ResultStale);
    }

    fn begin_generation(&mut self) {
        if self.scan_path.is_none() {
            self.status = StatusMessage::new(TextKey::NeedScan, StatusTone::Error);

            self.flash_attention(AttentionTarget::CustomHeadLoad);
            return;
        }
        if self.workspace.template_geometry.is_none() {
            self.status = StatusMessage::new(TextKey::TemplatePending, StatusTone::Warning);
            return;
        }
        if self.eye_state == EyeState::Closed && self.workspace.eye_morph.is_none() {
            self.status = StatusMessage::new(TextKey::NeedEyeMorph, StatusTone::Warning);
            return;
        }
        if self.workspace.pins.review_count() != 0 {
            self.status = StatusMessage::new(TextKey::ReviewEyePins, StatusTone::Warning);
            return;
        }

        self.pending_edit_carry = self.capture_edit_carry();
        let job_id = self.next_job_id;
        self.next_job_id = self.next_job_id.saturating_add(1);
        self.clear_morph_reset_undo();
        self.clear_generated_buffers();
        self.result = ResultState::Running {
            job_id,
            source_revision: self.revision,
        };
        self.progress = Some(Progress::new(JobStage::Prepare, 0.0));
        self.active_tab = Tab::Edit;
        self.status = StatusMessage::new(TextKey::GenerationStarted, StatusTone::Info);
    }

    fn bake_morph(&mut self) {
        if self.morph_library.has_loading_controls() {
            self.status = StatusMessage::new(TextKey::MorphLoading, StatusTone::Warning);
            return;
        }
        if !matches!(
            self.result,
            ResultState::Ready { source_revision, .. } if source_revision == self.revision
        ) || self.workspace.result_output.is_none()
        {
            self.status = StatusMessage::new(TextKey::ResultUnavailable, StatusTone::Warning);
            return;
        }
        if let Err(detail) = self.commit_save_preview() {
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, detail);
            return;
        }
        self.active_tab = Tab::Result;
        self.status = StatusMessage::new(TextKey::MorphApplied, StatusTone::Success);
    }

    fn finish_generation(
        &mut self,
        job_id: u64,
        reported_revision: u64,
        outcome: GenerationOutcome,
        morph_available: bool,
    ) {
        let ResultState::Running {
            job_id: active_job,
            source_revision,
        } = self.result
        else {
            return;
        };
        if active_job != job_id || source_revision != reported_revision {
            return;
        }
        self.progress = None;
        if source_revision != self.revision {
            self.clear_generated_buffers();
            self.result = ResultState::Stale {
                generated_revision: source_revision,
            };
            self.active_tab = Tab::Edit;
            self.status = StatusMessage::new(TextKey::ResultStale, StatusTone::Warning);
            return;
        }
        match outcome {
            GenerationOutcome::Success {
                output,
                fit_reference_value,
            } => match self.workspace.install_result(Arc::new(output)) {
                Ok(()) => {
                    let carry = self.pending_edit_carry.take();
                    self.clear_morph_reset_undo();
                    self.morph_value_history.clear();
                    self.morph_edit_open = None;
                    self.morph_library.reset();
                    if let Some(carry) = carry.as_ref() {
                        self.morph_library.restore_values(&carry.morph_values);
                    }
                    self.fit_reference_value = Some(fit_reference_value);
                    self.eye_closure = fit_reference_value as f32;
                    self.result = ResultState::Ready {
                        source_revision,
                        morph_available,
                    };
                    if let Err(detail) = self.begin_sculpt_stage() {
                        self.clear_generated_buffers();
                        self.result = ResultState::Empty;
                        self.active_tab = Tab::Edit;
                        self.status = StatusMessage::with_detail(
                            TextKey::GenerationFailed,
                            StatusTone::Error,
                            detail,
                        );
                        return;
                    }
                    if let Some(delta) = carry.and_then(|carry| carry.sculpt)
                        && self.sculpt.graft_displacement(&delta).is_err()
                    {
                        self.status =
                            StatusMessage::new(TextKey::SculptNotCarried, StatusTone::Warning);
                    }
                    self.morph_preview_dirty = true;
                    if let Err(detail) = self.compose_current_morphs(ResultPreviewPhase::Morph) {
                        self.clear_generated_buffers();
                        self.result = ResultState::Empty;
                        self.active_tab = Tab::Edit;
                        self.status = StatusMessage::with_detail(
                            TextKey::GenerationFailed,
                            StatusTone::Error,
                            detail,
                        );
                        return;
                    }
                    self.active_tab = Tab::Morph;
                    self.ensure_scan_texture_layer();
                    self.status =
                        StatusMessage::new(TextKey::GenerationComplete, StatusTone::Success);
                }
                Err(error) => {
                    self.clear_generated_buffers();
                    self.result = ResultState::Empty;
                    self.active_tab = Tab::Edit;
                    self.status = StatusMessage::with_detail(
                        TextKey::GenerationFailed,
                        StatusTone::Error,
                        error.to_string(),
                    );
                }
            },
            GenerationOutcome::Failed(detail) => {
                self.clear_generated_buffers();
                self.result = ResultState::Empty;
                self.active_tab = Tab::Edit;
                self.status = StatusMessage::with_detail(
                    TextKey::GenerationFailed,
                    StatusTone::Error,
                    detail,
                );
            }
            GenerationOutcome::Cancelled => {
                self.clear_generated_buffers();
                self.result = ResultState::Empty;
                self.active_tab = Tab::Edit;
                self.status = StatusMessage::new(TextKey::GenerationCancelled, StatusTone::Info);
            }
        }
    }

    fn block_mutation_while_busy(&mut self) -> bool {
        if self.busy() {
            self.status = StatusMessage::new(TextKey::Busy, StatusTone::Warning);
            true
        } else {
            false
        }
    }

    fn invalidate_geometry(&mut self, status_key: TextKey) {
        let generated_revision = match self.result {
            ResultState::Ready {
                source_revision, ..
            } => Some(source_revision),
            ResultState::Stale { generated_revision } => Some(generated_revision),
            ResultState::Empty | ResultState::Running { .. } => None,
        };
        self.revision = self.revision.saturating_add(1);
        self.clear_generated_buffers();
        if let Some(generated_revision) = generated_revision {
            self.result = ResultState::Stale { generated_revision };
        }

        if status_key == TextKey::ResultStale {
            return;
        }
        self.status = StatusMessage::new(
            status_key,
            if status_key == TextKey::PinsReset || status_key == TextKey::ScanLoaded {
                StatusTone::Info
            } else {
                StatusTone::Warning
            },
        );
    }

    fn invalidate_geometry_if_downstream(&mut self, status_key: TextKey, had_downstream: bool) {
        if had_downstream {
            self.invalidate_geometry(status_key);
        } else {
            self.revision = self.revision.saturating_add(1);
            self.clear_generated_buffers();
        }
    }

    fn has_downstream_geometry(&self) -> bool {
        !self.workspace.pins.pairs().is_empty()
            || !matches!(self.result, ResultState::Empty)
            || self.workspace.result.is_some()
            || self.workspace.fitted_result.is_some()
            || self.workspace.result_output.is_some()
    }

    fn clear_output_destination(&mut self) {
        self.output_path.clear();
        self.pending_overwrite_path = None;
    }

    fn sync_sculpt_preferences(&mut self) {
        self.sculpt.set_x_symmetry(self.sculpt_x_symmetry);
        self.sculpt
            .set_connected_topology_only(self.sculpt_connected_topology_only);
    }

    fn clear_generated_buffers(&mut self) {
        if self.workspace.result.is_some() {
            let _ = crate::diagnostics::record(
                crate::diagnostics::Severity::Debug,
                "state",
                "generated_buffers_cleared",
                &format!(
                    "tab={:?}; source_mode={:?}",
                    self.active_tab, self.edit_source_mode
                ),
            );
        }
        self.workspace.result = None;
        self.workspace.result_tear_lacrimals = None;
        self.workspace.result_eyelashes = None;
        self.workspace.fitted_result = None;
        self.workspace.result_output = None;
        self.vam_full_preview = None;
        self.fit_reference_value = None;
        self.result_preview_phase = ResultPreviewPhase::Empty;
        self.morph_preview_dirty = false;
        self.sculpt.clear();
        self.sync_sculpt_preferences();
        self.export = ExportLifecycle::Idle;
        self.pending_overwrite_path = None;
        self.expected_texture_bake_request = None;
        self.texture_project.bake_loading = false;
        self.texture_project.baked = None;

        self.texture_project.forget_geometry_bound_rasters();
        if !self.texture_project.layers.is_empty() {
            self.texture_project.dirty = true;
        }
    }
}

mod alignment;
mod cameras;
mod export;
mod figure_base;
mod jobs;
mod loading;
mod morph_values;
mod recovery;
mod sculpting;
mod texture_bake;
mod texture_pins;
mod vam_assets;

use jobs::*;
pub use jobs::{
    DirectEditLoadJob, PreparedDirectEdit, TemplateLoadJob, VaMPairLoadJob, WorkspaceLoadJob,
    WorkspaceLoadOutcome,
};

#[cfg(test)]
mod tests;
