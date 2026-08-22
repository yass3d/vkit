use std::{
    collections::{BTreeMap, VecDeque},
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
    camera::{ProjectionMode, StandardView},
    i18n::{Locale, TextKey},
    importers::is_supported_mesh_path,
    lighting::{DEFAULT_LIGHT_BRIGHTNESS, LightingPreset, ViewportGrading, sanitize_brightness},
    morphs::{
        GenitalMorphTarget, MorphCategory, MorphCategoryFilter, MorphControl, MorphLibrary,
        MorphLibraryValueSnapshot, MorphListFilter, MorphSource,
        apply_genital_applications_to_vam_mesh, unresolved_slider_contract,
    },
    post_process::VignetteSettings,
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
struct MorphMaskUndo {
    layer_id: u64,
    mask: crate::morph_mask::MorphMask,
    seq: u64,
}

#[derive(Clone, Debug)]
struct MorphResetUndo {
    values: MorphLibraryValueSnapshot,
    eye_closure: f32,

    seq: u64,
}

const MAX_MORPH_VALUE_UNDO: usize = 64;

pub(crate) const HISTORY_BRANCH_WARN_STEPS: usize = 5;

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

    edit_seq: u64,
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

    pub vignette: VignetteSettings,
    pub eye_closure: f32,
    pub morph_library: MorphLibrary,
    morph_reset_undo: Option<MorphResetUndo>,

    morph_reset_forward: Option<MorphResetUndo>,

    morph_value_history: VecDeque<MorphResetUndo>,
    morph_mask_history: VecDeque<MorphMaskUndo>,
    morph_mask_forward: VecDeque<MorphMaskUndo>,

    morph_value_forward: VecDeque<MorphResetUndo>,

    morph_edit_open: Option<String>,
    pub sculpt_brush_radius_points: f32,
    pub sculpt_strength: f32,

    pub sculpt_x_symmetry: bool,

    pub split_model_view: bool,

    pub keymap: crate::shortcuts::Keymap,

    pub model_split_ratio: f32,

    pub active_view_pane: crate::viewport::ViewPane,

    pub pending_history_branch: bool,
    pub pending_hair_overwrite: bool,
    pub pending_hair_portraits: Vec<crate::state::HairThumbnailTarget>,
    pub hair_preset_pick: Option<String>,
    pub hair_overwrite_confirmed: bool,
    pub history_branch_warning_muted: bool,

    pub sculpt_connected_topology_only: bool,

    pub sculpt_eye_tracking: bool,

    pub sculpt_groups_collapsed: bool,

    pub detail_group_panel_pos: Option<[f32; 2]>,
    pub hair_toolbox_pos: Option<[f32; 2]>,
    pub hair_panel_page: crate::hair_project::HairPanelPage,
    pub hair_scalp_mesh: String,
    pub hair_toolbox_columns: u8,

    pub sculpt_brush: SculptBrush,

    pub vam_root: Option<PathBuf>,

    pub edit_source_mode: EditSourceMode,

    pub vam_edit_sources: Vec<VaMEditSource>,

    pending_edit_carry: Option<CarriedEdit>,

    vam_morph_index: Arc<VaMMorphIndex>,

    pub vam_package_index: Arc<PackageIndex>,

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

    pub shared_scalp: Option<Box<vkit_core::vam::HairPartReference>>,

    pub builtin_hair_scalps: Arc<Vec<BuiltinHairScalp>>,
    pub builtin_scalp_textures: Arc<Vec<vkit_core::vam::BuiltinScalpTextureSet>>,

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
    pub hair_project: crate::hair_project::HairProject,
    pub hair_scalps:
        std::collections::HashMap<String, std::sync::Arc<crate::hair_project::ScalpAuthoring>>,
    pub appearance_stack: crate::appearance_layers::AppearanceStack,
    pub hair_brush_radius_points: f32,
    pub hair_brush_strength: f32,
    hair_collider: Option<(u64, std::sync::Arc<crate::hair_collision::HeadCollider>)>,
    pub hair_thumbnail: Option<HairThumbnailJob>,
    pub hair_part_thumbnails: std::collections::HashMap<u64, HairPartThumbnail>,
    pub hair_preset_thumbnail: Option<HairPartThumbnail>,
    pub hair_export_notice: Option<HairExportNotice>,
    pub hair_shot_flash: Option<HairShotFlash>,
    pub hair_part_tint: bool,
    pub hair_show_points: bool,
    pub hair_viewport_physics: bool,
    pub hair_show_streams: bool,
    pub hair_hide_strands: bool,
    pub hair_mirror_edit: bool,
    pub hair_auto_part: bool,
    pub(crate) vam_scan_signature: Option<(std::path::PathBuf, FigureSex, Option<usize>)>,
    pub hair_export_attempted_blank: bool,
    pub hair_param_group: crate::hair_settings::HairParamGroup,
    pub hair_settings_clipboard: Option<crate::hair_settings::HairSettings>,

    pub hair_settle_seconds: f32,
    pub hair_simulation_seconds: f32,

    pub scan_fidelity: f32,
    pub restore_neck_ears: bool,
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

    pub last_saved_paths: BTreeMap<SaveSection, PathBuf>,

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
            edit_seq: 0,
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
            vignette: VignetteSettings::default(),
            eye_closure: 0.0,
            morph_library: MorphLibrary::default(),
            morph_reset_undo: None,
            morph_reset_forward: None,
            morph_value_history: VecDeque::new(),
            morph_mask_history: VecDeque::new(),
            morph_mask_forward: VecDeque::new(),
            morph_value_forward: VecDeque::new(),
            morph_edit_open: None,
            sculpt_brush_radius_points: 64.0,
            sculpt_strength: 0.35,
            sculpt_x_symmetry: true,
            split_model_view: false,
            keymap: crate::shortcuts::Keymap::default(),
            model_split_ratio: 0.5,
            active_view_pane: crate::viewport::ViewPane::Primary,
            pending_history_branch: false,
            pending_hair_overwrite: false,
            pending_hair_portraits: Vec::new(),
            hair_preset_pick: None,
            hair_overwrite_confirmed: false,
            history_branch_warning_muted: false,
            sculpt_connected_topology_only: false,
            sculpt_eye_tracking: false,
            sculpt_groups_collapsed: true,
            detail_group_panel_pos: None,
            hair_toolbox_pos: None,
            hair_toolbox_columns: 1,
            hair_panel_page: crate::hair_project::HairPanelPage::default(),
            hair_scalp_mesh: String::new(),
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
            shared_scalp: None,
            builtin_hair_scalps: Arc::new(Vec::new()),
            builtin_scalp_textures: Arc::new(Vec::new()),
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
            hair_project: crate::hair_project::HairProject::default(),
            hair_scalps: std::collections::HashMap::new(),
            appearance_stack: crate::appearance_layers::AppearanceStack::default(),
            hair_brush_radius_points: 64.0,
            hair_brush_strength: 0.5,
            hair_collider: None,
            hair_thumbnail: None,
            hair_part_thumbnails: std::collections::HashMap::new(),
            hair_preset_thumbnail: None,
            hair_export_notice: None,
            hair_shot_flash: None,
            hair_part_tint: false,
            hair_show_points: true,
            hair_viewport_physics: false,
            hair_show_streams: false,
            hair_hide_strands: false,
            hair_mirror_edit: false,
            hair_auto_part: false,
            vam_scan_signature: None,
            hair_export_attempted_blank: false,
            hair_param_group: crate::hair_settings::HairParamGroup::Performance,
            hair_settings_clipboard: None,
            hair_settle_seconds: 0.0,
            hair_simulation_seconds: 0.0,
            scan_fidelity: 1.0,
            restore_neck_ears: true,
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
            last_saved_paths: BTreeMap::new(),
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

    pub const fn autosave_edit_signal(&self) -> (u64, u64, u64) {
        (
            self.revision,
            self.edit_seq,
            self.texture_project.edit_revision(),
        )
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

    pub const fn is_hair_editing(&self) -> bool {
        matches!(self.active_tab, Tab::Hair)
    }

    pub const fn is_texturing(&self) -> bool {
        matches!(self.active_tab, Tab::Texture)
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
            return self.neutral_skin_preview.as_ref().map(Arc::clone);
        }
        self.skin_preview.as_ref().map(Arc::clone)
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
    pub fn vam_name_locale(&self) -> Locale {
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
            Tab::Morph | Tab::Texture | Tab::Hair => {
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
        self.edit_seq = self.edit_seq.saturating_add(1);
        let texture_undo = action
            .records_texture_undo()
            .then(|| self.texture_project.capture_undo_checkpoint())
            .flatten();
        let stage_was_reachable = self.tab_available(self.active_tab);
        let was_busy = self.busy();
        if action.branches_hair_history()
            && !self.history_branch_warning_muted
            && self.hair_project.history_position().1 >= HISTORY_BRANCH_WARN_STEPS
        {
            self.pending_history_branch = true;
            return;
        }
        match action {
            Action::SetVarMetadata(field, value) => {
                let Some(slot) = self.var_metadata_slot(field) else {
                    self.var_version_text =
                        value.chars().filter(char::is_ascii_digit).take(9).collect();
                    return;
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
            Action::UnloadScan => self.unload_scan(),
            Action::RequestTextureImageBrowse(source_mode) => {
                self.pending_dialog = Some(DialogIntent::OpenTextureImage(source_mode));
            }
            Action::RequestVaMRootBrowse => {
                self.pending_dialog = Some(DialogIntent::ChooseVaMRoot);
            }
            Action::SetVaMRoot(path) => self.set_vam_root(path),

            Action::RefreshVaMCatalog => self.force_vam_catalog_refresh(),
            Action::SelectVaMEditSource(id) => self.select_vam_edit_source(&id),
            Action::AddHairPart { provider_name } => {
                self.hair_project.active_tool = crate::hair_project::HairTool::Plant;
                self.hair_project.checkpoint();
                self.add_hair_part(&provider_name);
            }
            Action::RemoveHairPart(id) => {
                self.hair_part_thumbnails.remove(&id);
                self.hair_project.checkpoint();
                self.hair_project.remove_part(id);
            }
            Action::ActivateHairPart { id, additive } => {
                self.hair_project.activate_part(id, additive);
            }
            Action::ToggleHairPartVisible(id) => {
                self.hair_project.checkpoint();
                self.hair_project.toggle_part_visible(id);
            }
            Action::SetHairTool(tool) => self.hair_project.active_tool = tool,
            Action::SelectHairParamGroup(group) => self.hair_param_group = group,
            Action::SetHairBrushStrength(strength) => {
                self.hair_brush_strength = strength.clamp(0.05, 1.0);
            }
            Action::SetHairBrushRadius(radius) => {
                self.hair_brush_radius_points = radius.clamp(8.0, 220.0);
            }
            Action::PlantHairStrands {
                part_id,
                scalp_indices,
            } => {
                self.hair_project.begin_stroke();
                self.plant_hair_strands(part_id, &scalp_indices);
                self.clear_hair_of_the_head(part_id, &scalp_indices);
            }
            Action::UnplantHairStrands {
                part_id,
                scalp_indices,
            } => {
                self.hair_project.begin_stroke();
                self.unplant_hair_strands(part_id, &scalp_indices);
            }
            Action::SetHairPartSegments { id, segments } => {
                self.set_hair_part_segments(id, segments);
            }
            Action::ScaleHairStrands {
                part_id,
                scalp_indices,
                factor,
            } => {
                self.hair_project.begin_stroke();
                self.scale_hair_strands(part_id, &scalp_indices, factor);
                self.clear_hair_of_the_head(part_id, &scalp_indices);
            }
            Action::SetHairStrandPoints { part_id, strands } => {
                self.hair_project.begin_stroke();
                self.set_hair_strand_points(part_id, strands);
            }
            Action::PaintMorphMask {
                vertices,
                target,
                amount,
                begins_step,
            } => self.paint_morph_mask(vertices, target, amount, begins_step),
            Action::AddAppearanceLayer => {
                self.add_appearance_layer();
                self.recompose_appearance_blend();
            }
            Action::SelectAppearanceLayer(id) => {
                if self.appearance_stack.layer(id).is_some() {
                    self.appearance_stack.selected_id = Some(id);
                }
            }
            Action::SetAppearanceLayerVisible { id, visible } => {
                if let Some(layer) = self.appearance_stack.layer_mut(id) {
                    layer.visible = visible;
                }
                self.recompose_appearance_blend();
            }
            Action::RaiseAppearanceLayer(id) => {
                self.appearance_stack.raise(id);
                self.recompose_appearance_blend();
            }
            Action::LowerAppearanceLayer(id) => {
                self.appearance_stack.lower(id);
                self.recompose_appearance_blend();
            }
            Action::RemoveAppearanceLayer(id) => self.remove_appearance_layer(id),
            Action::ExportHairPart => self.request_hair_export(),
            Action::ConfirmHairOverwrite => {
                self.pending_hair_overwrite = false;
                self.hair_overwrite_confirmed = true;
                self.export_hair_style();
                self.hair_overwrite_confirmed = false;
            }
            Action::CancelHairOverwrite => self.pending_hair_overwrite = false,
            Action::EndHairStroke => {
                self.hair_project.end_stroke();
                self.hair_project.end_control();
            }
            Action::SetHairParam { id, key, value } => self.set_hair_param(id, key, value),
            Action::SetHairColorChannel {
                id,
                key,
                channel,
                value,
            } => self.set_hair_color_channel(id, key, channel, value),
            Action::ResetHairParams(id) => self.reset_hair_params(id),
            Action::ResetHairShapes => self.reset_hair_shapes(),
            Action::SetHairPartName { id, name } => self.set_hair_part_name(id, name),
            Action::SetHairPartStyleJoints { id, on } => self.set_hair_part_style_joints(id, on),
            Action::AddHairScalp(mesh) => self.add_hair_scalp(&mesh),
            Action::SetHairScalpMesh { id, mesh } => self.set_hair_scalp_mesh(id, &mesh),
            Action::SetHairScalpTexture { id, texture } => self.set_hair_scalp_texture(id, texture),
            Action::CopyHairSettings(id) => self.copy_hair_settings(id),
            Action::PasteHairSettings(id) => self.paste_hair_settings(id),
            Action::SetHairExportSexes(sexes) => {
                self.hair_project.export_sexes = sexes;
            }
            Action::SetHairExportName(name) => {
                self.hair_overwrite_confirmed = false;
                self.hair_project.export_name = name;
            }
            Action::SetHairExportCreator(creator) => {
                self.hair_overwrite_confirmed = false;
                self.hair_project.export_creator = creator;
            }
            Action::MirrorHairPart(id) => self.mirror_hair_part(id),
            Action::DuplicateHairPart(id) => {
                self.hair_project.checkpoint();
                self.hair_project.duplicate_part(id);
            }
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
            Action::AutoAlign => self.auto_align(),
            Action::PlacementReset => self.reset_placement(),
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
                self.set_scale(value);
            }
            Action::SetScaleLinked(value) => self.scale_linked = value,
            Action::SetScaleAxis { axis, value } => {
                if axis >= 3 || !value.is_finite() || value <= 0.0 {
                    return;
                }
                let Some(scale_xyz) = self.scale_xyz_with_axis(axis, value) else {
                    return;
                };
                self.set_scale(scale_xyz);
            }
            Action::SetPosition { axis, value_cm } => {
                if axis >= 3 || !value_cm.is_finite() {
                    return;
                }
                self.set_position(axis, value_cm);
            }
            Action::SetRotation {
                axis,
                value_degrees,
            } => {
                if axis >= 3 || !value_degrees.is_finite() {
                    return;
                }
                self.set_rotation(axis, value_degrees);
            }
            Action::ToggleXMirror(value) => self.x_mirror = value,
            Action::RequestScanSymmetry(value) => {
                if value == self.scan_symmetry {
                    return;
                }
                self.request_scan_symmetry(value);
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
            Action::SetCameraControl(mode) => self.camera_control = mode,
            Action::RestoreSession => self.answer_recovery(true),
            Action::DiscardSession => self.answer_recovery(false),
            Action::SetVignette(value) => self.vignette = value,
            Action::ToggleHelp => {
                self.help_visible = !self.help_visible;
                self.help_opened_for_projection = false;
            }
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::ResetPins => self.reset_pins(),
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
            Action::RebindShortcut(shortcut, binding) => {
                self.keymap.rebind(shortcut, binding);
            }
            Action::ResetKeymap => {
                self.keymap.reset();
            }
            Action::SetModelSplitRatio(value) => {
                if value.is_finite() {
                    self.model_split_ratio = value.clamp(0.05, 0.95);
                }
            }
            Action::SetSplitModelView(value) => {
                self.split_model_view = value;
                if value {
                    self.workspace.result_camera_split = self.workspace.result_camera;
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
                self.set_texture_projection_stencil(active);
            }
            Action::SetTextureProjectionPlacement(placement) => {
                self.set_texture_projection_placement(placement);
            }
            Action::SetTextureProjectionOpacity(value) => {
                if value.is_finite() {
                    self.texture_project.projection_opacity = value.clamp(0.0, 1.0);
                }
            }
            Action::SetTextureRetouchReverse(value) => {
                self.texture_project.retouch_reverse = value;
            }
            Action::BeginTextureEdit => {
                if self.history_branch_needs_asking() {
                    self.pending_history_branch = true;
                } else {
                    self.texture_project.begin_undo_transaction();
                }
            }
            Action::ConfirmHistoryBranch => {
                self.pending_history_branch = false;
                match self.active_tab {
                    Tab::Texture => self.texture_project.begin_undo_transaction(),
                    Tab::Morph => self.clear_detail_forward_history(),
                    Tab::Hair => self.hair_project.clear_forward_history(),
                    _ => {}
                }
            }
            Action::ImportHairPreset(preset_id) => self.import_hair_preset(&preset_id),
            Action::CancelHistoryBranch => self.pending_history_branch = false,
            Action::MuteHistoryBranchWarning => self.history_branch_warning_muted = true,
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
                self.edit_texture_layer(id, |layer| layer.visible = visible);
            }
            Action::SetTextureLayerChannel { id, channel } => {
                self.edit_texture_layer(id, |layer| layer.channel = channel);
            }
            Action::SetTextureLayerOpacity { id, opacity } => {
                if opacity.is_finite() {
                    self.edit_texture_layer(id, |layer| layer.opacity = opacity.clamp(0.0, 1.0));
                }
            }
            Action::SetTextureLayerMirror { id, mirror } => {
                self.edit_texture_layer(id, |layer| {
                    layer.mirror = mirror;

                    layer.raster_revision = layer.raster_revision.saturating_add(1);
                });
            }
            Action::SetTextureLayerBlendMode { id, blend_mode } => {
                self.edit_texture_layer(id, |layer| layer.blend_mode = blend_mode);
            }
            Action::SetTextureLayerAdjustments { id, adjustments } => {
                self.edit_texture_layer(id, |layer| layer.adjustments = adjustments);
            }
            Action::SetTextureLayerNormalStrength { id, strength } => {
                if strength.is_finite() {
                    self.edit_texture_layer(id, |layer| {
                        layer.normal_strength = strength.clamp(0.0, 3.0);
                    });
                }
            }
            Action::SetTextureLayerScalarInvert { id, invert } => {
                self.edit_texture_layer(id, |layer| layer.scalar_invert = invert);
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
                    self.warn_skin_preset_required();
                    return;
                }
                self.texture_project.bake_base = value;
                self.texture_project.mark_dirty();
            }
            Action::SetTextureHideVaMSkin(value) => {
                if value == self.texture_project.hide_vam_skin_preview {
                    return;
                }
                self.set_texture_hide_vam_skin(value);
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
            } => self.report_progress(job_id, source_revision, progress),
            Action::ReportImportProgress { path, progress } => {
                self.report_import_progress(path, progress);
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

            Action::SetScanFidelity(value) => {
                self.scan_fidelity = value.clamp(
                    *vkit_core::fit::SCAN_FIDELITY_RANGE.start() as f32,
                    *vkit_core::fit::SCAN_FIDELITY_RANGE.end() as f32,
                );
            }
            Action::SetRestoreNeckEars(value) => {
                if self.restore_neck_ears == value {
                    return;
                }
                self.restore_neck_ears = value;
                if self.workspace.fitted_result.is_none() {
                    return;
                }
                if self.sculpt.top_undo_seq().is_some() {
                    self.status =
                        StatusMessage::new(TextKey::RestoreNeckEarsDeferred, StatusTone::Warning);
                    return;
                }
                match self.apply_neck_ear_restore_to_result() {
                    Ok(()) => {
                        if let Err(detail) = self.begin_sculpt_stage() {
                            self.status = StatusMessage::with_detail(
                                TextKey::GenerationFailed,
                                StatusTone::Error,
                                detail,
                            );
                            return;
                        }
                        self.morph_preview_dirty = true;
                        let phase = self.result_preview_phase;
                        if let Err(detail) = self.compose_current_morphs(phase) {
                            self.status = StatusMessage::with_detail(
                                TextKey::MorphPreviewUpdated,
                                StatusTone::Warning,
                                detail,
                            );
                        }
                    }
                    Err(detail) => {
                        self.status = StatusMessage::with_detail(
                            TextKey::GenerationFailed,
                            StatusTone::Warning,
                            detail,
                        );
                    }
                }
            }
            Action::OpenSettings => self.settings_open = true,
            Action::CloseSettings => self.settings_open = false,
            Action::SpendHairSettle(elapsed) => {
                self.hair_settle_seconds = (self.hair_settle_seconds - elapsed).max(0.0);
                self.hair_simulation_seconds = (self.hair_simulation_seconds - elapsed).max(0.0);
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
            Action::SetPackageFromThisHead(value) => self.set_package_from_this_head(value),
            Action::AddPackageFiles(slot, paths) => self.add_package_files(slot, paths),
            Action::RemovePackageFile(slot, index) => self.remove_package_file(slot, index),
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

    fn undo(&mut self) {
        if self.active_tab == Tab::Alignment && self.undo_alignment_transform() {
        } else if self.active_tab == Tab::Hair {
            if !self.hair_project.undo() {
                self.status = StatusMessage::new(TextKey::NativeCorePending, StatusTone::Info);
            }
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

    fn redo(&mut self) {
        match self.active_tab {
            Tab::Texture => {
                self.texture_project.redo();
            }
            Tab::Morph => {
                self.redo_detail_edit();
            }
            Tab::Hair => {
                self.hair_project.redo();
            }
            _ => {}
        }
    }

    fn reset_pins(&mut self) {
        if self.workspace.pins.reset() {
            self.active_tab = Tab::Edit;
            self.complete_pairs = 0;
            self.invalidate_geometry(TextKey::PinsReset);
        }
    }

    fn request_scan_symmetry(&mut self, value: ScanSymmetry) {
        if self.workspace.pins.pairs().is_empty() {
            self.set_scan_symmetry(value);
        } else {
            self.pending_symmetry_change = Some(value);
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

        if matches!(tab, Tab::Morph | Tab::Texture | Tab::Hair | Tab::Result)
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
                Tab::Hair => TextKey::HairUnavailable,
            };
            self.status = StatusMessage::new(key, StatusTone::Warning);
            return;
        }
        let transition = match tab {
            Tab::Morph | Tab::Texture | Tab::Hair => {
                self.compose_current_morphs(ResultPreviewPhase::Morph)
            }
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

    fn add_appearance_layer(&mut self) {
        let Some(id) = self.selected_vam_edit_source_id.clone() else {
            return;
        };
        let Some(source) = self
            .vam_edit_sources
            .iter()
            .find(|source| source.stable_id == id)
        else {
            return;
        };
        let label = source.label.clone();
        let deltas = self.loaded_appearance_deltas();
        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Debug,
            "state",
            "appearance_layer_added",
            &format!(
                "layer={label}; vertices={}; moved={}",
                deltas.len(),
                deltas.iter().filter(|delta| *delta != &[0.0; 3]).count(),
            ),
        );
        self.appearance_stack.add(label, deltas);
    }

    fn remove_appearance_layer(&mut self, id: u64) {
        self.appearance_stack.remove(id);
        self.morph_mask_history.retain(|step| step.layer_id != id);
        self.morph_mask_forward.retain(|step| step.layer_id != id);
        self.recompose_appearance_blend();
    }

    fn loaded_appearance_deltas(&self) -> Vec<[f32; 3]> {
        let Some(origin) = self.custom_morph_origin.as_deref() else {
            return Vec::new();
        };
        let Some(template) = self.workspace.template.as_ref() else {
            return Vec::new();
        };
        let base = &template.mesh.vertices;
        if base.len() != origin.len() {
            return Vec::new();
        }
        origin
            .iter()
            .zip(base)
            .map(|(baked, base)| {
                [
                    (baked[0] - base[0]) as f32,
                    (baked[1] - base[1]) as f32,
                    (baked[2] - base[2]) as f32,
                ]
            })
            .collect()
    }

    pub(super) fn appearance_blend(&self, vertex_count: usize) -> Option<Vec<[f32; 3]>> {
        let contributing = self.appearance_stack.contributing();
        if contributing.is_empty() {
            return None;
        }
        let borrowed = contributing
            .iter()
            .map(|layer| layer.deltas.as_slice())
            .collect::<Vec<_>>();
        let masks = contributing
            .iter()
            .map(|layer| &layer.mask)
            .collect::<Vec<_>>();
        Some(crate::appearance_layers::blend_vertex_deltas(
            &borrowed,
            &masks,
            vertex_count,
        ))
    }

    fn recompose_appearance_blend(&mut self) {
        self.morph_preview_dirty = true;
        if !self.tab_available(Tab::Morph) {
            return;
        }
        if let Err(detail) = self.compose_current_morphs(self.result_preview_phase) {
            self.status =
                StatusMessage::with_detail(TextKey::GenerationFailed, StatusTone::Error, detail);
        }
    }

    #[must_use]
    pub fn history_position(&self) -> (usize, usize) {
        match self.active_tab {
            Tab::Texture => self.texture_project.history_position(),
            Tab::Hair => self.hair_project.history_position(),
            Tab::Morph => {
                let (sculpt_back, sculpt_forward) = self.sculpt.history_position();
                (
                    self.morph_value_history.len()
                        + usize::from(self.morph_reset_undo.is_some())
                        + sculpt_back
                        + self.morph_mask_history.len(),
                    self.morph_value_forward.len()
                        + usize::from(self.morph_reset_forward.is_some())
                        + sculpt_forward
                        + self.morph_mask_forward.len(),
                )
            }
            _ => (0, 0),
        }
    }

    #[must_use]
    pub fn history_branch_needs_asking(&self) -> bool {
        !self.history_branch_warning_muted && self.history_position().1 >= HISTORY_BRANCH_WARN_STEPS
    }

    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "history inspection, test-asserted")
    )]
    pub fn has_forward_history(&self) -> bool {
        self.history_position().1 != 0
    }

    fn active_stage_camera_mut(
        &mut self,
        stage: Tab,
    ) -> Option<&mut crate::camera::TurntableCamera> {
        if self.split_model_view
            && self.active_view_pane == crate::viewport::ViewPane::Split
            && matches!(stage, Tab::Morph | Tab::Texture | Tab::Hair | Tab::Result)
        {
            return Some(&mut self.workspace.result_camera_split);
        }
        self.workspace.stage_camera_mut(stage)
    }

    fn sync_pin_count_and_invalidate(&mut self) {
        self.complete_pairs = self.workspace.pins.complete_count();
        self.invalidate_geometry(TextKey::ResultStale);
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
mod hair;
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

pub(crate) use hair::HAIR_SIMULATION_SECONDS;
pub use hair::{
    HAIR_SETTLE_GRAVITY, HairExportNotice, HairPartThumbnail, HairShotFlash, HairThumbnailJob,
    HairThumbnailTarget,
};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod autosave_signal_tests {
    use super::*;

    #[test]
    fn an_edit_that_never_refits_geometry_still_moves_the_autosave_signal() {
        let mut state = AppState::default();
        let before = state.autosave_edit_signal();
        state.dispatch(Action::SetEyeClosure(0.4));
        assert_eq!(state.revision, 0, "eye closure is not a geometry refit");
        assert_ne!(
            before,
            state.autosave_edit_signal(),
            "the autosave signal has to move on the edits the snapshot exists to save"
        );
    }

    #[test]
    fn a_texture_edit_made_without_an_action_still_moves_the_autosave_signal() {
        let mut state = AppState::default();
        let before = state.autosave_edit_signal();
        state.texture_project.add_image_layer(
            PathBuf::from("face.png"),
            crate::texture_project::TextureSourceMode::LandmarkPins,
        );
        assert_ne!(
            before,
            state.autosave_edit_signal(),
            "texture mutations reach the project directly, bypassing dispatch"
        );
    }
}
