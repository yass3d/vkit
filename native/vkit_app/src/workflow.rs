use std::{
    any::Any,
    borrow::Cow,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Instant,
};

use sha2::{Digest, Sha256};
use thiserror::Error;
use vkit_core::math::{Mat3, SimilarityTransform, Vec3 as CoreVec3};
use vkit_core::{
    MIN_FIT_PAIRS,
    fit::{LandmarkWarpOptions, MIN_ASSISTED_FIT_PAIRS},
    formats::{DazGeometry, Mesh, MorphTarget, OrderedObjMesh, SurfaceAttachment},
    pipeline::{
        AnatomyPreservation, GeometryAssistedFitOptions, ManualFitRequest, NumericSurfacePin,
        PipelineError, PipelineProgress, PipelineStage, TopologyGuardOptions,
        canonical_g2_neck_constraints, run_geometry_assisted_fit, run_manual_fit,
        run_prior_assisted_fit,
    },
    symmetry::{SymmetryMode, SymmetryOptions},
    vam::{
        ShapeVmiOptions, SparseDelta, VaMGeometryError, VaMGeometryProvider, VmbVertexRouting,
        VmiFormula, VmiNumericScalar, build_shape_vmi_with_options, build_sparse_deltas_daz_cm,
        classify_vam_morph_route, encode_vmb_daz_cm, encode_vmb_daz_cm_for_topology,
        encode_vmi_pretty, transfer_rig_point_idw, vmb_raw_entry_count,
    },
};

use crate::{
    diagnostics::{self, Severity},
    morphs::{GenitalMorphApplication, apply_genital_applications_to_vam_mesh},
    persistence::{PersistenceError, write_vam_morph_pair_atomic},
    scene::{ModelTransform, SurfaceEndpoint, SurfaceMesh},
    state::{
        AppState, ExportOutcome, ExportReceipt, FigureSex, GenerationOutcome, JobStage,
        MORPH_EXPORT_EXTENSION, MORPH_EXPORT_LABEL, OutputTopology, Progress, ScanSymmetry,
    },
    texture_project::{
        TextureExportSnapshot, is_opaque, texture_export_filename, texture_export_images,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FitRoute {
    GeometryAssisted,
    PinAssisted,
    ManualPins,
}

mod writers;

pub use writers::*;

impl FitRoute {
    fn resolve(automatic_matching: bool, complete_pairs: usize) -> Self {
        if automatic_matching || complete_pairs < MIN_ASSISTED_FIT_PAIRS {
            Self::GeometryAssisted
        } else if complete_pairs < MIN_FIT_PAIRS {
            Self::PinAssisted
        } else {
            Self::ManualPins
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::GeometryAssisted => "geometry_assisted",
            Self::PinAssisted => "pin_assisted",
            Self::ManualPins => "manual_pins",
        }
    }
}

const USER_PIN_FIT_WEIGHT: f64 = 6.0;

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("no active generation job exists")]
    NoActiveJob,
    #[error("the scan mesh is not loaded")]
    MissingScan,
    #[error("the canonical G2 base is not loaded; select your VaM folder to prepare it")]
    MissingTemplate,
    #[error("the closed-eye morph is required for a closed-eye fit")]
    MissingEyeMorph,
    #[error("output path is empty")]
    MissingOutput,
    #[error("pin pair {pair} references a missing {side} triangle {triangle}")]
    MissingTriangle {
        pair: usize,
        side: &'static str,
        triangle: u32,
    },
    #[error("failed to prepare the fitting request: {0}")]
    Core(#[from] PipelineError),
    #[error("scan scale axes must be finite positive values")]
    InvalidScale,
    #[error("failed to transform the scan: {0}")]
    Format(#[from] vkit_core::formats::FormatError),
    #[error("failed to resolve an output path: {0}")]
    OutputPath(#[from] std::io::Error),
    #[error("the fitted result is unavailable")]
    MissingResult,
    #[error("the fitted result is stale")]
    StaleResult,
    #[error("failed to write the final export: {0}")]
    Persistence(#[from] PersistenceError),
    #[error(
        "VaM full-mesh export is unavailable because no verified user-owned VaM base/topology provider is configured; select DAZ output instead (Vkit will not relabel DAZ geometry as VaM)"
    )]
    UnsupportedVaMTopology,
    #[error(
        "the validated VaM geometry provider is {provider_sex:?}, but the current output figure is {figure_sex:?}"
    )]
    VaMProviderSexMismatch {
        provider_sex: vkit_core::vam::GeometrySex,
        figure_sex: FigureSex,
    },
    #[error("VaM geometry transfer failed: {0}")]
    VaMGeometry(#[from] VaMGeometryError),
    #[error(
        "the output path does not match the selected {format} format; choose a .{expected_extension} destination"
    )]
    OutputFormatMismatch {
        format: &'static str,
        expected_extension: &'static str,
    },
    #[error("failed to export baked textures: {0}")]
    TextureExport(String),
    #[error("VaM VMI/VMB morph pair is invalid: {0}")]
    VaMMorphPair(String),
}

#[derive(Debug)]
pub struct JobSnapshot {
    pub job_id: u64,
    pub source_revision: u64,

    pub scan: Arc<Mesh>,
    pub template_base: Arc<DazGeometry>,
    pub eye_morph: Option<Arc<MorphTarget>>,
    pub pins: Vec<NumericSurfacePin>,
    pub symmetry: SymmetryOptions,

    pub scan_residual_scale: [f64; 3],

    pub scan_pivot_local: [f64; 3],
    pub alignment_prior: SimilarityTransform,
    pub fit_reference_value: f64,

    pub scan_fidelity: f64,
    fit_route: FitRoute,
}

#[derive(Debug)]
pub struct ExportSnapshot {
    pub source_revision: u64,
    pub output_path: PathBuf,
    pub topology: OutputTopology,
    pub output: Arc<OrderedObjMesh>,

    pub template_basis: Arc<OrderedObjMesh>,
    pub figure_sex: FigureSex,

    pub vam_metadata: ShapeVmiOptions,

    pub vam_bone_correction: bool,

    pub vam_bone_formulas: Vec<VmiFormula>,

    pub provider: Option<Arc<VaMGeometryProvider>>,

    pub genital_applications: Vec<GenitalMorphApplication>,

    pub baked_textures: Option<TextureExportSnapshot>,
}

#[derive(Debug)]
pub enum WorkerEvent {
    Progress {
        job_id: u64,
        source_revision: u64,
        progress: Progress,
    },
    Finished {
        job_id: u64,
        source_revision: u64,
        outcome: GenerationOutcome,
    },
}

#[derive(Debug)]
pub enum ExportWorkerEvent {
    Progress {
        source_revision: u64,
        fraction: f32,
    },
    Finished {
        source_revision: u64,
        outcome: ExportOutcome,
    },
}

#[derive(Debug)]
struct ActiveWorker {
    job_id: u64,
    cancel: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct JobCoordinator {
    sender: Sender<WorkerEvent>,
    receiver: Receiver<WorkerEvent>,
    active: Option<ActiveWorker>,
}

#[derive(Debug)]
pub struct ExportCoordinator {
    sender: Sender<ExportWorkerEvent>,
    receiver: Receiver<ExportWorkerEvent>,
    active_revision: Option<u64>,
}

impl Default for ExportCoordinator {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            active_revision: None,
        }
    }
}

impl ExportCoordinator {
    pub fn start(
        &mut self,
        snapshot: ExportSnapshot,
        wake: impl Fn() + Send + 'static,
    ) -> Result<(), String> {
        if self.active_revision.is_some() {
            return Err("a native export worker is already active".to_owned());
        }
        let source_revision = snapshot.source_revision;
        let sender = self.sender.clone();
        thread::Builder::new()
            .name(format!("vkit-export-{source_revision}"))
            .spawn(move || {
                let _ = sender.send(ExportWorkerEvent::Progress {
                    source_revision,
                    fraction: 0.12,
                });
                wake();
                let outcome =
                    match catch_unwind(AssertUnwindSafe(|| write_export_snapshot(&snapshot))) {
                        Ok(Ok(receipt)) => ExportOutcome::Success { receipt },
                        Ok(Err(error)) => ExportOutcome::Failed(error.to_string()),
                        Err(payload) => ExportOutcome::Failed(export_panic_detail(payload)),
                    };
                let _ = sender.send(ExportWorkerEvent::Finished {
                    source_revision,
                    outcome,
                });
                wake();
            })
            .map_err(|error| format!("failed to spawn the native export worker: {error}"))?;
        self.active_revision = Some(source_revision);
        Ok(())
    }

    pub fn drain(&mut self) -> Vec<ExportWorkerEvent> {
        let events: Vec<_> = self.receiver.try_iter().collect();
        if events.iter().any(|event| {
            matches!(
                event,
                ExportWorkerEvent::Finished {
                    source_revision,
                    ..
                } if self.active_revision == Some(*source_revision)
            )
        }) {
            self.active_revision = None;
        }
        events
    }

    pub const fn is_active(&self) -> bool {
        self.active_revision.is_some()
    }
}

impl Default for JobCoordinator {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            active: None,
        }
    }
}

impl JobCoordinator {
    pub fn start(
        &mut self,
        snapshot: JobSnapshot,
        wake: impl Fn() + Send + 'static,
    ) -> Result<(), String> {
        if self.active.is_some() {
            return Err("a native fitting worker is already active".to_owned());
        }
        let job_id = snapshot.job_id;
        let source_revision = snapshot.source_revision;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let sender = self.sender.clone();
        thread::Builder::new()
            .name(format!("vkit-fit-{job_id}"))
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    execute_job(&snapshot, &worker_cancel, |progress| {
                        let _ = sender.send(WorkerEvent::Progress {
                            job_id,
                            source_revision,
                            progress,
                        });
                        wake();
                    })
                }))
                .unwrap_or_else(|payload| GenerationOutcome::Failed(panic_detail(payload)));
                let _ = sender.send(WorkerEvent::Finished {
                    job_id,
                    source_revision,
                    outcome,
                });
                wake();
            })
            .map_err(|error| format!("failed to spawn the native fitting worker: {error}"))?;
        self.active = Some(ActiveWorker { job_id, cancel });
        Ok(())
    }

    pub fn cancel(&self, job_id: u64) -> bool {
        let Some(active) = self
            .active
            .as_ref()
            .filter(|active| active.job_id == job_id)
        else {
            return false;
        };
        active.cancel.store(true, Ordering::Release);
        true
    }

    pub fn drain(&mut self) -> Vec<WorkerEvent> {
        let events: Vec<_> = self.receiver.try_iter().collect();
        if events.iter().any(|event| {
            matches!(
                event,
                WorkerEvent::Finished { job_id, .. }
                    if self.active.as_ref().is_some_and(|active| active.job_id == *job_id)
            )
        }) {
            self.active = None;
        }
        events
    }

    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }
}

fn panic_detail(payload: Box<dyn Any + Send>) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic payload");
    format!("native fitting worker stopped unexpectedly: {message}")
}

fn export_panic_detail(payload: Box<dyn Any + Send>) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic payload");
    format!("native export worker stopped unexpectedly: {message}")
}

impl Drop for JobCoordinator {
    fn drop(&mut self) {
        if let Some(active) = &self.active {
            active.cancel.store(true, Ordering::Release);
        }
    }
}

pub fn snapshot_from_state(state: &AppState) -> Result<JobSnapshot, WorkflowError> {
    let (job_id, source_revision) = state.active_job().ok_or(WorkflowError::NoActiveJob)?;
    let SnapshotInputs {
        scan,
        template_base,
        eye_morph,
        pins,
        symmetry,
        scan_residual_scale,
        scan_pivot_local,
        alignment_prior,
        fit_reference_value,
        scan_fidelity,
        fit_route,
    } = collect_inputs(state)?;
    Ok(JobSnapshot {
        job_id,
        source_revision,
        scan,
        template_base,
        eye_morph,
        pins,
        symmetry,
        scan_residual_scale,
        scan_pivot_local,
        alignment_prior,
        fit_reference_value,
        scan_fidelity,
        fit_route,
    })
}

pub fn export_snapshot_from_state(state: &AppState) -> Result<ExportSnapshot, WorkflowError> {
    if !state.export_ready() {
        return Err(match state.result {
            crate::state::ResultState::Stale { .. } => WorkflowError::StaleResult,
            crate::state::ResultState::Ready {
                source_revision, ..
            } if source_revision != state.revision => WorkflowError::StaleResult,
            _ => WorkflowError::MissingResult,
        });
    }

    let output = Arc::clone(
        state
            .workspace
            .result_output
            .as_ref()
            .ok_or(WorkflowError::MissingResult)?,
    );
    let template_geometry = state
        .workspace
        .template_geometry
        .as_deref()
        .ok_or(WorkflowError::MissingTemplate)?;
    let template_basis = Arc::new(template_geometry.to_ordered_obj(None)?);

    let source_revision = state.revision;
    let output_path = absolute_output_path(&state.output_path)?;
    validate_output_format(&output_path)?;

    let vam_metadata = resolve_vam_metadata(state, &output_path)?;
    let vam_bone_correction = state.vam_export_bone_correction;
    let vam_bone_formulas = if vam_bone_correction {
        build_vam_bone_correction_formulas(template_geometry, &output)?
    } else {
        Vec::new()
    };

    Ok(ExportSnapshot {
        source_revision,
        output_path,
        topology: state.output_topology,
        output,
        template_basis,
        figure_sex: state.figure_sex,
        vam_metadata,
        vam_bone_correction,
        vam_bone_formulas,
        provider: state.vam_geometry_provider.as_ref().map(Arc::clone),
        genital_applications: state.morph_library.genital_applications()?,
        baked_textures: None,
    })
}

fn resolve_vam_metadata(
    state: &AppState,
    output_path: &Path,
) -> Result<ShapeVmiOptions, WorkflowError> {
    let stem = output_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            WorkflowError::VaMMorphPair("output filename has no usable UTF-8 stem".to_owned())
        })?;
    let id = deterministic_vam_identifier(stem);
    let display_fallback = normalize_vam_metadata_text(stem, "Vkit Morph");
    Ok(ShapeVmiOptions {
        id,
        display_name: normalize_vam_metadata_text(
            &state.vam_export_display_name,
            &display_fallback,
        ),
        group: normalize_vam_metadata_text(&state.vam_export_group, "Vkit"),
        region: normalize_vam_metadata_text(&state.vam_export_region, "Morph"),
        is_pose_control: state.vam_export_is_pose_control,
    })
}

fn deterministic_vam_identifier(stem: &str) -> String {
    let original = stem.trim();
    let hash = format!("{:x}", Sha256::digest(original.as_bytes()));
    let hash = &hash[..12];
    let slug = sanitize_ascii_slug(original, 60);
    if slug.is_empty() {
        format!("VkitMorph_{hash}")
    } else {
        format!("Vkit_{slug}_{hash}")
    }
}

fn sanitize_ascii_slug(value: &str, maximum_length: usize) -> String {
    let mut result = String::with_capacity(value.len().min(maximum_length));
    let mut last_was_separator = false;
    for character in value.trim().chars() {
        if result.len() >= maximum_length {
            break;
        }
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
            result.push(character);
            last_was_separator = false;
        } else if !last_was_separator && !result.is_empty() {
            result.push('_');
            last_was_separator = true;
        }
    }
    result
        .trim_matches(|character| matches!(character, '-' | '_' | '.'))
        .to_owned()
}

const CANONICAL_G2_JOINTS: [(&str, [f64; 3]); 65] = [
    ("neck", [0.0, 153.581691, -4.22459]),
    ("head", [0.0, 161.843002, -2.2822]),
    ("rEye", [-3.13, 168.03, 5.828937]),
    ("lEye", [3.13, 168.03, 5.828937]),
    ("upperJaw", [0.01, 163.366294, 0.095005]),
    ("lowerJaw", [0.0, 163.303292, -0.111159]),
    ("tongueBase", [0.0, 159.990907, 3.365112]),
    ("tongue01", [0.0, 160.733593, 3.556737]),
    ("tongue02", [0.0, 161.059296, 4.323429]),
    ("tongue03", [0.0, 161.178207, 5.0875]),
    ("tongue04", [0.0, 161.189497, 5.850345]),
    ("tongue05", [0.0, 161.177087, 6.496374]),
    ("tongueTip", [0.0, 161.134696, 7.102378]),
    ("chest", [0.0, 127.272797, -3.404906]),
    ("abdomen2", [0.0, 112.653995, -1.2884]),
    ("abdomen", [0.0, 102.690005, -1.50178]),
    ("hip", [0.0, 103.875995, 0.0]),
    ("lCollar", [1.70688, 145.863008, 0.459902]),
    ("lShldr", [15.352699, 145.815992, -4.38912]),
    ("lForeArm", [43.375018, 145.67759, -2.518034]),
    ("lHand", [64.138383, 145.6092, 7.856268]),
    ("lPectoral", [3.078442, 131.854296, -7.387229]),
    ("lThumb1", [65.318203, 145.325196, 10.80072]),
    ("lThumb2", [65.472633, 144.610596, 14.8632]),
    ("lThumb3", [66.727173, 143.985105, 17.3059]),
    ("lIndex1", [70.126957, 146.7484, 14.090329]),
    ("lIndex2", [72.832042, 146.899199, 16.130649]),
    ("lIndex3", [74.413961, 146.442389, 17.300989]),
    ("lMid1", [71.402657, 146.629691, 12.36845]),
    ("lMid2", [74.71801, 146.734095, 14.124349]),
    ("lMid3", [76.75404, 146.350598, 15.357991]),
    ("lRing1", [72.012329, 145.946896, 10.56422]),
    ("lRing2", [75.26347, 145.827997, 11.73227]),
    ("lRing3", [77.093017, 145.463502, 12.527819]),
    ("lPinky1", [72.037262, 144.854701, 8.678017]),
    ("lPinky2", [74.616128, 144.662905, 9.381476]),
    ("lPinky3", [76.075256, 144.255495, 9.937938]),
    ("rCollar", [-1.70688, 145.863008, 0.459902]),
    ("rShldr", [-15.352699, 145.815992, -4.38912]),
    ("rForeArm", [-43.375018, 145.67759, -2.518034]),
    ("rHand", [-64.138383, 145.6092, 7.856268]),
    ("rPectoral", [-3.078442, 131.854296, -7.387229]),
    ("rThumb1", [-65.318203, 145.325196, 10.80072]),
    ("rThumb2", [-65.472633, 144.610596, 14.8632]),
    ("rThumb3", [-66.727173, 143.985105, 17.3059]),
    ("rIndex1", [-70.126957, 146.7484, 14.090329]),
    ("rIndex2", [-72.832042, 146.899199, 16.130649]),
    ("rIndex3", [-74.413961, 146.442389, 17.300989]),
    ("rMid1", [-71.402657, 146.629691, 12.36845]),
    ("rMid2", [-74.71801, 146.734095, 14.124349]),
    ("rMid3", [-76.75404, 146.350598, 15.357991]),
    ("rRing1", [-72.012329, 145.946896, 10.56422]),
    ("rRing2", [-75.26347, 145.827997, 11.73227]),
    ("rRing3", [-77.093017, 145.463502, 12.527819]),
    ("rPinky1", [-72.037262, 144.854701, 8.678017]),
    ("rPinky2", [-74.616128, 144.662905, 9.381476]),
    ("rPinky3", [-76.075256, 144.255495, 9.937938]),
    ("lThigh", [8.516807, 97.990662, 0.116879]),
    ("lShin", [9.959139, 53.509653, 0.099076]),
    ("lFoot", [10.87584, 6.367682, -2.719793]),
    ("lToe", [14.16523, 1.798746, 8.568646]),
    ("rThigh", [-8.516807, 97.990662, 0.116879]),
    ("rShin", [-9.959139, 53.509653, 0.099076]),
    ("rFoot", [-10.87584, 6.367682, -2.719793]),
    ("rToe", [-14.16523, 1.798746, 8.568646]),
];

fn build_vam_bone_correction_formulas(
    template: &DazGeometry,
    result: &OrderedObjMesh,
) -> Result<Vec<VmiFormula>, WorkflowError> {
    if result.vertices.len() != template.vertices.len() {
        return Err(WorkflowError::VaMMorphPair(format!(
            "bone correction needs {} canonical vertices, but the result has {}",
            template.vertices.len(),
            result.vertices.len()
        )));
    }
    let mut formulas = Vec::with_capacity(CANONICAL_G2_JOINTS.len() * 3);
    for (group, _) in CANONICAL_G2_JOINTS {
        let basis_center = template
            .bone(group)
            .map(|bone| CoreVec3::from(bone.center_point))
            .or_else(|| canonical_g2_joint_center(group))
            .ok_or_else(|| {
                WorkflowError::VaMMorphPair(format!(
                    "canonical rig joint {group} has no authored center point"
                ))
            })?;
        let result_center =
            transfer_rig_point_idw(basis_center, &template.vertices, &result.vertices)
                .map_err(WorkflowError::VaMMorphPair)?;
        let delta = result_center - basis_center;
        for (target_type, value) in [
            ("BoneCenterX", -delta.x / 100.0),
            ("BoneCenterY", delta.y / 100.0),
            ("BoneCenterZ", delta.z / 100.0),
        ] {
            if value.abs() <= 1.0e-14 {
                continue;
            }
            formulas.push(VmiFormula {
                target_type: Some(target_type.to_owned()),
                target: Some(group.to_owned()),
                multiplier: Some(VmiNumericScalar::String(value.to_string())),
                unknown_fields: Default::default(),
            });
        }
    }
    Ok(formulas)
}

fn canonical_g2_joint_center(id: &str) -> Option<CoreVec3> {
    CANONICAL_G2_JOINTS
        .iter()
        .find(|(joint, _)| *joint == id)
        .map(|(_, point)| CoreVec3::from(*point))
}

fn normalize_vam_metadata_text(value: &str, fallback: &str) -> String {
    let normalized = value
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(128)
        .collect::<String>();
    if normalized.is_empty() {
        fallback.to_owned()
    } else {
        normalized
    }
}

struct SnapshotInputs {
    scan: Arc<Mesh>,
    template_base: Arc<DazGeometry>,
    eye_morph: Option<Arc<MorphTarget>>,
    pins: Vec<NumericSurfacePin>,
    symmetry: SymmetryOptions,
    scan_residual_scale: [f64; 3],
    scan_pivot_local: [f64; 3],
    alignment_prior: SimilarityTransform,
    scan_fidelity: f64,
    fit_reference_value: f64,
    fit_route: FitRoute,
}

fn collect_inputs(state: &AppState) -> Result<SnapshotInputs, WorkflowError> {
    let displayed_scan = state
        .workspace
        .scan
        .as_deref()
        .or(state.workspace.scan_source.as_deref())
        .ok_or(WorkflowError::MissingScan)?;
    let template_surface = state
        .workspace
        .template
        .as_deref()
        .ok_or(WorkflowError::MissingTemplate)?;
    let template_base = Arc::clone(
        state
            .workspace
            .template_geometry
            .as_ref()
            .ok_or(WorkflowError::MissingTemplate)?,
    );
    let fit_reference_value = match state.eye_state {
        crate::state::EyeState::Open => 0.0,
        crate::state::EyeState::Closed => 1.0,
    };
    let eye_morph = state.workspace.eye_morph.as_ref().map(Arc::clone);
    if fit_reference_value != 0.0 && eye_morph.is_none() {
        return Err(WorkflowError::MissingEyeMorph);
    }

    let scan_pivot_local = displayed_scan
        .facial_focus_bounds()
        .center()
        .as_dvec3()
        .to_array();
    let (uniform_scale, scan_residual_scale) = decompose_scale_xyz(state.transform.scale_xyz)?;
    let transform = ModelTransform::from_components_with_pivot(
        uniform_scale,
        state.transform.translation_cm,
        state.transform.rotation_degrees,
        scan_pivot_local,
    );

    let scan = Arc::clone(&displayed_scan.mesh);
    let alignment_prior = similarity_prior(transform);
    let pins = state
        .workspace
        .pins
        .pairs()
        .iter()
        .enumerate()
        .filter_map(|(pair_index, pair)| {
            pair.scan
                .zip(pair.template)
                .map(|(scan, template)| (pair_index, scan, template))
        })
        .enumerate()
        .map(
            |(dense_index, (slot_index, scan_endpoint, template_endpoint))| {
                Ok(NumericSurfacePin {
                    pair_index: dense_index as u32,
                    scan: attachment(displayed_scan, scan_endpoint, slot_index, "scan")?,
                    template: attachment(
                        template_surface,
                        template_endpoint,
                        slot_index,
                        "template",
                    )?,
                    alignment_weight: 1.0,
                    fit_weight: USER_PIN_FIT_WEIGHT,
                    confidence: 1.0,
                })
            },
        )
        .collect::<Result<Vec<_>, WorkflowError>>()?;
    let fit_route = FitRoute::resolve(state.resolved_automatic_matching(), pins.len());

    Ok(SnapshotInputs {
        scan_fidelity: f64::from(state.scan_fidelity),
        scan,
        template_base,
        eye_morph,
        pins,

        symmetry: match state.scan_symmetry {
            ScanSymmetry::Original => SymmetryOptions {
                center_x: state.workspace.scan_mirror_plane_x(),
                ..Default::default()
            },
            ScanSymmetry::PositiveX => SymmetryOptions::preapplied(
                SymmetryMode::PositiveX,
                state.workspace.scan_mirror_plane_x(),
            ),
            ScanSymmetry::NegativeX => SymmetryOptions::preapplied(
                SymmetryMode::NegativeX,
                state.workspace.scan_mirror_plane_x(),
            ),
        },
        scan_residual_scale,
        scan_pivot_local,
        alignment_prior,
        fit_reference_value,
        fit_route,
    })
}

fn similarity_prior(transform: ModelTransform) -> SimilarityTransform {
    let rotation = transform.rotation_quat();
    let x = rotation * glam::DVec3::X;
    let y = rotation * glam::DVec3::Y;
    let z = rotation * glam::DVec3::Z;
    let uniform_scale = transform.scale_xyz.x;
    debug_assert!(
        transform
            .scale_xyz
            .abs_diff_eq(glam::DVec3::splat(uniform_scale), 0.0)
    );
    let affine_translation = transform.pivot_local + transform.translation
        - rotation * transform.pivot_local * uniform_scale;
    SimilarityTransform {
        scale: uniform_scale,
        rotation: Mat3::from_rows([[x.x, y.x, z.x], [x.y, y.y, z.y], [x.z, y.z, z.z]]),
        translation: CoreVec3::from_array(affine_translation.to_array()),
    }
}

fn decompose_scale_xyz(scale_xyz: [f64; 3]) -> Result<(f64, [f64; 3]), WorkflowError> {
    if scale_xyz
        .iter()
        .any(|axis| !axis.is_finite() || *axis <= 0.0)
    {
        return Err(WorkflowError::InvalidScale);
    }
    if scale_xyz[0] == scale_xyz[1] && scale_xyz[1] == scale_xyz[2] {
        return Ok((scale_xyz[0], [1.0; 3]));
    }
    let uniform_scale = ((scale_xyz[0].ln() + scale_xyz[1].ln() + scale_xyz[2].ln()) / 3.0).exp();
    if !uniform_scale.is_finite() || uniform_scale <= 0.0 {
        return Err(WorkflowError::InvalidScale);
    }
    Ok((
        uniform_scale,
        [
            scale_xyz[0] / uniform_scale,
            scale_xyz[1] / uniform_scale,
            scale_xyz[2] / uniform_scale,
        ],
    ))
}

fn residual_scale_scan(
    source: &Mesh,
    residual_scale: [f64; 3],
    pivot_local: [f64; 3],
) -> Result<Mesh, vkit_core::formats::FormatError> {
    if residual_scale == [1.0; 3] {
        return Ok(source.clone());
    }
    let residual_scale = glam::DVec3::from_array(residual_scale);
    let pivot_local = glam::DVec3::from_array(pivot_local);
    Mesh::new(
        source
            .vertices
            .iter()
            .map(|point| {
                let local = glam::DVec3::from_array(*point);
                (pivot_local + residual_scale * (local - pivot_local)).to_array()
            })
            .collect(),
        source.triangles.clone(),
    )
}

fn attachment(
    mesh: &SurfaceMesh,
    endpoint: SurfaceEndpoint,
    pair: usize,
    side: &'static str,
) -> Result<SurfaceAttachment, WorkflowError> {
    let triangle = mesh.mesh.triangles.get(endpoint.triangle as usize).ok_or(
        WorkflowError::MissingTriangle {
            pair,
            side,
            triangle: endpoint.triangle,
        },
    )?;
    Ok(SurfaceAttachment {
        triangle_vertex_ids: *triangle,
        barycentric: endpoint.barycentric,
        primitive_id: Some(endpoint.triangle),
    })
}

fn absolute_output_path(value: &str) -> Result<PathBuf, WorkflowError> {
    if value.trim().is_empty() {
        return Err(WorkflowError::MissingOutput);
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn execute_job(
    snapshot: &JobSnapshot,
    cancel: &AtomicBool,
    mut emit_progress: impl FnMut(Progress),
) -> GenerationOutcome {
    const PREPARE_STARTED_FRACTION: f32 = 0.01;
    const PREPARE_COMPLETE_FRACTION: f32 = 0.03;

    emit_progress(Progress::new(JobStage::Prepare, PREPARE_STARTED_FRACTION));
    if cancel.load(Ordering::Acquire) {
        return GenerationOutcome::Cancelled;
    }

    let prepare_started = Instant::now();
    let request = match materialize_request(snapshot, cancel) {
        Ok(Some(request)) => request,
        Ok(None) => return GenerationOutcome::Cancelled,
        Err(error) => return GenerationOutcome::Failed(error.to_string()),
    };
    let prepare_elapsed = prepare_started.elapsed();
    let _ = diagnostics::record(
        Severity::Info,
        "workflow",
        "job_request_materialized",
        &format!(
            "job_id={}; revision={}; elapsed_ms={:.3}; scan_vertices={}; template_vertices={}; pins={}",
            snapshot.job_id,
            snapshot.source_revision,
            prepare_elapsed.as_secs_f64() * 1_000.0,
            request.scan.vertices.len(),
            request.template.vertices.len(),
            request.pins.len(),
        ),
    );
    emit_progress(Progress::new(JobStage::Prepare, PREPARE_COMPLETE_FRACTION));

    let mut last_fraction = PREPARE_COMPLETE_FRACTION;
    let mut last_pipeline_progress = None;
    let fit_started = Instant::now();
    let outcome = execute_fit_request(
        &request,
        snapshot.fit_route,
        snapshot.alignment_prior,
        snapshot.fit_reference_value,
        cancel,
        |progress| {
            last_pipeline_progress = Some(progress);
            let mut mapped = map_progress(progress);
            mapped.fraction = mapped.fraction.max(last_fraction);
            last_fraction = mapped.fraction;
            emit_progress(mapped);
        },
    );
    let status = match &outcome {
        GenerationOutcome::Success { .. } => "success",
        GenerationOutcome::Cancelled => "cancelled",
        GenerationOutcome::Failed(_) => "failed",
    };
    if let GenerationOutcome::Failed(error) = &outcome {
        let _ = diagnostics::record(
            Severity::Error,
            "workflow",
            "fit_stage_failed",
            &format!(
                "job_id={}; revision={}; route={}; pins={}; pipeline_stage={:?}; pipeline_fraction={:.3}; error={error}",
                snapshot.job_id,
                snapshot.source_revision,
                snapshot.fit_route.label(),
                request.pins.len(),
                last_pipeline_progress.map(|progress| progress.stage),
                last_pipeline_progress
                    .map_or(f64::from(last_fraction), |progress| { progress.fraction() }),
            ),
        );
    }
    let _ = diagnostics::record(
        Severity::Info,
        "workflow",
        "fit_stage_finished",
        &format!(
            "job_id={}; revision={}; elapsed_ms={:.3}; status={status}; pins={}",
            snapshot.job_id,
            snapshot.source_revision,
            fit_started.elapsed().as_secs_f64() * 1_000.0,
            request.pins.len(),
        ),
    );
    outcome
}

fn materialize_template(snapshot: &JobSnapshot) -> Result<DazGeometry, WorkflowError> {
    match snapshot.eye_morph.as_deref() {
        Some(morph) => {
            Ok(morph
                .apply_to_daz_geometry(&snapshot.template_base, snapshot.fit_reference_value)?)
        }
        None if snapshot.fit_reference_value == 0.0 => Ok((*snapshot.template_base).clone()),
        None => Err(WorkflowError::MissingEyeMorph),
    }
}

fn materialize_scan(snapshot: &JobSnapshot) -> Result<Mesh, WorkflowError> {
    Ok(residual_scale_scan(
        &snapshot.scan,
        snapshot.scan_residual_scale,
        snapshot.scan_pivot_local,
    )?)
}

fn materialize_request(
    snapshot: &JobSnapshot,
    cancel: &AtomicBool,
) -> Result<Option<ManualFitRequest>, WorkflowError> {
    if cancel.load(Ordering::Acquire) {
        return Ok(None);
    }
    let scan = materialize_scan(snapshot)?;
    if cancel.load(Ordering::Acquire) {
        return Ok(None);
    }
    let template = materialize_template(snapshot)?;
    if cancel.load(Ordering::Acquire) {
        return Ok(None);
    }
    let neck = canonical_g2_neck_constraints(&template)?;
    Ok(Some(ManualFitRequest {
        scan,
        template,
        pins: snapshot.pins.clone(),
        symmetry: snapshot.symmetry,
        seam_vertices: neck.seam_vertices,
        anchor_weights: neck.anchor_weights,
        warp: LandmarkWarpOptions::default(),
        topology_guard: TopologyGuardOptions::canonical_g2(),
        anatomy: AnatomyPreservation::default(),
        scan_fidelity: snapshot.scan_fidelity,
    }))
}

fn execute_fit_request(
    request: &ManualFitRequest,
    fit_route: FitRoute,
    alignment_prior: SimilarityTransform,
    fit_reference_value: f64,
    cancel: &AtomicBool,
    mut emit_progress: impl FnMut(PipelineProgress),
) -> GenerationOutcome {
    let _ = diagnostics::record(
        Severity::Info,
        "workflow",
        "fit_route_selected",
        &format!("route={}; pairs={}", fit_route.label(), request.pins.len()),
    );
    let result = match fit_route {
        FitRoute::ManualPins => run_manual_fit(request, &mut emit_progress, || {
            cancel.load(Ordering::Acquire)
        }),
        FitRoute::PinAssisted => {
            let assisted = run_prior_assisted_fit(
                request,
                &Default::default(),
                alignment_prior,
                &mut emit_progress,
                || cancel.load(Ordering::Acquire),
            );
            if let Ok(assisted) = &assisted {
                let receipt = &assisted.assisted;
                let _ = diagnostics::record(
                    Severity::Info,
                    "workflow",
                    "pin_assisted_fit_complete",
                    &format!(
                        "pairs={}; mode={:?}; robust_inliers={}; pin_rms_cm={:?}; fallback_prior_rms_cm={:?}; fallback_candidate_rms_cm={:?}; fallback_maximum_rms_cm={:?}",
                        receipt.pair_count,
                        receipt.initialization_mode,
                        receipt.inlier_count,
                        receipt.pin_all_rms_cm,
                        receipt.fallback_prior_rms_cm,
                        receipt.fallback_candidate_rms_cm,
                        receipt.fallback_maximum_rms_cm,
                    ),
                );
            }
            assisted.map(|assisted| assisted.fit)
        }
        FitRoute::GeometryAssisted => {
            let geometry_result = run_geometry_assisted_fit(
                request,
                &GeometryAssistedFitOptions {
                    prior: alignment_prior,
                    ..Default::default()
                },
                &mut emit_progress,
                || cancel.load(Ordering::Acquire),
            );
            match geometry_result {
                Ok(assisted) => {
                    let receipt = &assisted.receipt;
                    let eyelid_conformation = if assisted.fit.anatomy.eyelid_conformation.is_some()
                    {
                        "applied"
                    } else if assisted.fit.anatomy.eyelid_conformation_skipped {
                        "skipped_invalid_local_cage"
                    } else {
                        "not_applicable"
                    };
                    if let Some(semantic) = &receipt.semantic_constraints
                        && let Some(reason) = semantic.rejection_reason
                    {
                        let _ = diagnostics::record(
                            Severity::Warning,
                            "workflow",
                            "semantic_matching_fallback",
                            &format!(
                                "reason={reason:?}; detail={}; scan_presence={:?}; template_presence={:?}; accepted_pairs={}",
                                semantic.failure_detail.as_deref().unwrap_or("none"),
                                semantic.scan_presence_probability,
                                semantic.template_presence_probability,
                                semantic.accepted_pairs,
                            ),
                        );
                    }
                    let _ = diagnostics::record(
                        Severity::Info,
                        "workflow",
                        "geometry_assisted_fit_complete",
                        &format!(
                            "user_pairs={}; automatic_pairs={}; automatic_source={:?}; semantic_pairs={}; semantic_rejection={:?}; mode={:?}; pin_rms_cm={:?}; guarded_mode={:?}; guarded_pin_rms_cm={:?}; guarded_fallback_prior_rms_cm={:?}; guarded_fallback_candidate_rms_cm={:?}; icp_iterations={}; icp_skip={:?}; eyelid_conformation={eyelid_conformation}",
                            receipt.user_pair_count,
                            receipt.automatic_pair_count,
                            receipt.automatic_pair_source,
                            receipt
                                .semantic_constraints
                                .as_ref()
                                .map_or(0, |semantic| semantic.accepted_pairs),
                            receipt
                                .semantic_constraints
                                .as_ref()
                                .and_then(|semantic| semantic.rejection_reason),
                            receipt.initialization.mode,
                            receipt.initialization.pin_rms_after_cm,
                            receipt.guarded_fit.initialization_mode,
                            receipt.guarded_fit.pin_all_rms_cm,
                            receipt.guarded_fit.fallback_prior_rms_cm,
                            receipt.guarded_fit.fallback_candidate_rms_cm,
                            receipt.initialization.icp_iterations.len(),
                            receipt.initialization.icp_skip_reason,
                        ),
                    );
                    Ok(assisted.fit)
                }
                Err(error @ PipelineError::AutomaticSurfaceConstraints(_))
                    if request.pins.len() >= MIN_ASSISTED_FIT_PAIRS =>
                {
                    let fallback = FitRoute::resolve(false, request.pins.len());
                    let _ = diagnostics::record(
                        Severity::Warning,
                        "workflow",
                        "geometry_assisted_surface_fallback",
                        &format!(
                            "pairs={}; fallback={}; reason={error}",
                            request.pins.len(),
                            fallback.label()
                        ),
                    );
                    match fallback {
                        FitRoute::ManualPins => run_manual_fit(request, &mut emit_progress, || {
                            cancel.load(Ordering::Acquire)
                        }),
                        FitRoute::PinAssisted => run_prior_assisted_fit(
                            request,
                            &Default::default(),
                            alignment_prior,
                            &mut emit_progress,
                            || cancel.load(Ordering::Acquire),
                        )
                        .map(|assisted| assisted.fit),
                        FitRoute::GeometryAssisted => {
                            unreachable!("four or more valid pins always have a pin-only fallback")
                        }
                    }
                }
                Err(error) => Err(error),
            }
        }
    };
    let result = match result {
        Ok(result) => result,
        Err(PipelineError::Cancelled(_)) => return GenerationOutcome::Cancelled,
        Err(error) => return GenerationOutcome::Failed(error.to_string()),
    };
    if cancel.load(Ordering::Acquire) {
        return GenerationOutcome::Cancelled;
    }
    GenerationOutcome::Success {
        output: result.output,
        fit_reference_value,
    }
}

fn map_progress(progress: PipelineProgress) -> Progress {
    let stage = match progress.stage {
        PipelineStage::Validation | PipelineStage::ScanSymmetry | PipelineStage::PinResolution => {
            JobStage::Prepare
        }
        PipelineStage::Alignment => JobStage::Align,
        PipelineStage::InitialWarp
        | PipelineStage::DenseRegistration
        | PipelineStage::AnatomyPreservation => JobStage::Fit,
        PipelineStage::OutputAssembly
        | PipelineStage::QualityValidation
        | PipelineStage::Complete => JobStage::Validate,
    };
    Progress::new(stage, progress.fraction() as f32)
}

pub fn write_var_package_from_exports(
    directory: &Path,
    metadata: &vkit_core::vam::VarMetadata,
    morph: Option<ExportSnapshot>,
    textures: Option<&TextureExportSnapshot>,
    figure_sex: FigureSex,
    extras: &PackageExtras,
    existing: vkit_core::vam::ExistingPackage,
) -> Result<vkit_core::vam::VarPackage, WorkflowError> {
    let package_name = vkit_core::vam::safe_identity(&metadata.package);
    let staging = tempfile::Builder::new()
        .prefix(".vkit-var-")
        .tempdir()
        .map_err(|error| WorkflowError::TextureExport(format!("staging folder: {error}")))?;
    let person = staging.path().join("Custom").join("Atom").join("Person");

    if let Some(mut snapshot) = morph {
        let folder = person.join("Morphs").join(match figure_sex {
            FigureSex::Female => "female",
            FigureSex::Male => "male",
        });
        fs::create_dir_all(&folder).map_err(|error| {
            WorkflowError::VaMMorphPair(format!("{}: {error}", folder.display()))
        })?;

        snapshot.output_path = folder.join(format!("{package_name}.vmi"));
        write_export_snapshot(&snapshot)?;
    }

    if let Some(snapshot) = textures {
        let staged = TextureExportSnapshot {
            directory: person.join("Textures").join(&package_name),
            ..snapshot.clone()
        };
        write_texture_bundle(&staged)?;
    }

    for (files, folder) in [
        (
            &extras.morphs,
            person.join("Morphs").join(match figure_sex {
                FigureSex::Female => "female",
                FigureSex::Male => "male",
            }),
        ),
        (
            &extras.textures,
            person.join("Textures").join(&package_name),
        ),
    ] {
        if files.is_empty() {
            continue;
        }
        fs::create_dir_all(&folder).map_err(|error| {
            WorkflowError::TextureExport(format!("{}: {error}", folder.display()))
        })?;
        for source in files {
            let Some(name) = source.file_name() else {
                continue;
            };
            let target = folder.join(name);
            if target.exists() {
                continue;
            }
            fs::copy(source, &target).map_err(|error| {
                WorkflowError::TextureExport(format!("{}: {error}", source.display()))
            })?;
        }
    }

    let contents = collect_package_contents(staging.path())?;
    vkit_core::vam::write_var_package(directory, metadata, &contents, existing)
        .map_err(|error| WorkflowError::TextureExport(error.to_string()))
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PackageExtras {
    pub morphs: Vec<PathBuf>,
    pub textures: Vec<PathBuf>,
}

fn collect_package_contents(root: &Path) -> Result<Vec<vkit_core::vam::VarContent>, WorkflowError> {
    fn walk(
        root: &Path,
        directory: &Path,
        found: &mut Vec<vkit_core::vam::VarContent>,
    ) -> Result<(), WorkflowError> {
        let entries = fs::read_dir(directory).map_err(|error| {
            WorkflowError::TextureExport(format!("{}: {error}", directory.display()))
        })?;
        for entry in entries {
            let entry =
                entry.map_err(|error| WorkflowError::TextureExport(format!("staging: {error}")))?;
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, found)?;
                continue;
            }
            let relative = path.strip_prefix(root).map_err(|_| {
                WorkflowError::TextureExport("a staged file escaped the staging folder".to_owned())
            })?;

            let internal_path = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let bytes = fs::read(&path).map_err(|error| {
                WorkflowError::TextureExport(format!("{}: {error}", path.display()))
            })?;
            found.push(vkit_core::vam::VarContent {
                internal_path,
                bytes,
            });
        }
        Ok(())
    }
    let mut found = Vec::new();
    walk(root, root, &mut found)?;

    found.sort_by(|left, right| left.internal_path.cmp(&right.internal_path));
    if found.is_empty() {
        return Err(WorkflowError::TextureExport(
            "there is no saved morph or texture to package".to_owned(),
        ));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::narrowest_png_encoding;

    #[test]
    fn a_channel_with_alpha_is_written_as_a_png_whatever_was_asked_for() {
        use crate::texture_project::TextureContainer;
        let directory = tempfile::tempdir().expect("a temp dir");

        let opaque = [10, 20, 30, 255, 40, 50, 60, 255];
        let written = super::write_texture_channel(
            &directory.path().join("face_diffuse.jpg"),
            TextureContainer::Jpeg,
            &opaque,
            2,
            1,
        )
        .expect("opaque writes");
        assert_eq!(written.extension().and_then(|e| e.to_str()), Some("jpg"));

        let translucent = [10, 20, 30, 128, 40, 50, 60, 255];
        let written = super::write_texture_channel(
            &directory.path().join("decal_diffuse.jpg"),
            TextureContainer::Jpeg,
            &translucent,
            2,
            1,
        )
        .expect("translucent writes");
        assert_eq!(written.extension().and_then(|e| e.to_str()), Some("png"));
        let read_back = image::open(&written).expect("it decodes").to_rgba8();
        assert_eq!(read_back.into_raw(), translucent, "alpha survives");
    }

    #[test]
    fn a_channel_is_encoded_as_narrowly_as_its_pixels_allow() {
        let colour = [10, 20, 30, 255, 40, 50, 60, 255];
        let (kind, pixels) = narrowest_png_encoding(&colour);
        assert_eq!(kind, image::ColorType::Rgb8);
        assert_eq!(pixels.as_ref(), &[10, 20, 30, 40, 50, 60]);

        let scalar = [90, 90, 90, 255, 12, 12, 12, 255];
        let (kind, pixels) = narrowest_png_encoding(&scalar);
        assert_eq!(kind, image::ColorType::L8);
        assert_eq!(pixels.as_ref(), &[90, 12]);

        let masked = [90, 90, 90, 128, 12, 12, 12, 255];
        let (kind, pixels) = narrowest_png_encoding(&masked);
        assert_eq!(kind, image::ColorType::La8);
        assert_eq!(pixels.as_ref(), &[90, 128, 12, 255]);

        let full = [10, 20, 30, 128, 40, 50, 60, 255];
        let (kind, pixels) = narrowest_png_encoding(&full);
        assert_eq!(kind, image::ColorType::Rgba8);
        assert!(matches!(pixels, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    #[ignore = "needs a real exported texture"]
    fn narrowing_a_real_face_texture_is_smaller_and_identical() {
        let Ok(sample) = std::env::var("VKIT_TEXTURE_SAMPLE") else {
            eprintln!("VKIT_TEXTURE_SAMPLE unset");
            return;
        };
        let source = image::open(&sample).expect("the sample decodes").to_rgba8();
        let (width, height) = source.dimensions();
        let rgba8 = source.into_raw();

        let directory = tempfile::tempdir().expect("a temp dir");
        let before = directory.path().join("before.png");
        image::save_buffer_with_format(
            &before,
            &rgba8,
            width,
            height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .expect("the old encoding writes");
        let after = directory.path().join("after.png");
        super::write_narrow_png(&after, &rgba8, width, height).expect("the new encoding writes");

        let old_bytes = std::fs::metadata(&before).expect("before").len();
        let new_bytes = std::fs::metadata(&after).expect("after").len();
        eprintln!(
            "{sample}: {width}x{height}  {:.2} MB -> {:.2} MB ({:.0}%)",
            old_bytes as f64 / 1_048_576.0,
            new_bytes as f64 / 1_048_576.0,
            100.0 - new_bytes as f64 / old_bytes as f64 * 100.0
        );

        let read_back = image::open(&after).expect("it decodes").to_rgba8();
        assert_eq!(read_back.dimensions(), (width, height));
        assert_eq!(read_back.into_raw(), rgba8, "every pixel survives");
        assert!(new_bytes < old_bytes, "narrowing did not help");

        let rgb: Vec<u8> = rgba8
            .chunks_exact(4)
            .flat_map(|pixel| &pixel[..3])
            .copied()
            .collect();
        use image::ImageEncoder as _;
        for quality in [98_u8, 95, 92, 85] {
            let jpeg = directory.path().join(format!("q{quality}.jpg"));
            let file = std::fs::File::create(&jpeg).expect("a jpeg file");
            image::codecs::jpeg::JpegEncoder::new_with_quality(
                std::io::BufWriter::new(file),
                quality,
            )
            .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
            .expect("the jpeg encodes");
            let bytes = std::fs::metadata(&jpeg).expect("jpeg").len();
            let decoded = image::open(&jpeg).expect("it decodes").to_rgb8().into_raw();
            let worst = rgb
                .iter()
                .zip(&decoded)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            let mean = rgb
                .iter()
                .zip(&decoded)
                .map(|(a, b)| f64::from(a.abs_diff(*b)))
                .sum::<f64>()
                / rgb.len() as f64;
            eprintln!(
                "  jpeg q{quality}: {:.2} MB  mean |delta| {mean:.2}/255  worst {worst}",
                bytes as f64 / 1_048_576.0
            );
        }
    }

    #[test]
    fn a_package_carries_the_staged_tree_and_nothing_else() {
        let staging = tempfile::tempdir().expect("a staging dir");
        let morphs = staging
            .path()
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Morphs")
            .join("female");
        let textures = staging
            .path()
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Textures")
            .join("MyFace");
        fs::create_dir_all(&morphs).expect("morph folder");
        fs::create_dir_all(&textures).expect("texture folder");
        fs::write(morphs.join("MyFace.vmi"), b"vmi").expect("vmi");
        fs::write(morphs.join("MyFace.vmb"), b"vmb").expect("vmb");
        fs::write(textures.join("MyFaceD.png"), b"png").expect("png");

        let contents = collect_package_contents(staging.path()).expect("it collects");

        let names: Vec<&str> = contents
            .iter()
            .map(|content| content.internal_path.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "Custom/Atom/Person/Morphs/female/MyFace.vmb",
                "Custom/Atom/Person/Morphs/female/MyFace.vmi",
                "Custom/Atom/Person/Textures/MyFace/MyFaceD.png",
            ],
            "sorted, forward-slashed, and archive-relative"
        );
        assert_eq!(contents[2].bytes, b"png");
    }

    #[test]
    fn an_empty_staging_tree_is_refused() {
        let staging = tempfile::tempdir().expect("a staging dir");
        assert!(collect_package_contents(staging.path()).is_err());
    }

    #[test]
    fn the_names_a_texture_save_would_claim_are_known_before_writing() {
        use crate::skin_preview::{SkinImage, SkinUvOrientation};
        use crate::texture_project::{TextureChannel, TextureExportSnapshot};
        use std::sync::Arc;

        let directory = tempfile::tempdir().expect("a place to write");
        let image = |channel| {
            (
                channel,
                Arc::new(SkinImage {
                    revision: 1,
                    width: 1,
                    height: 1,
                    rgba8: Arc::new(vec![0, 0, 0, 255]),
                    uv_orientation: SkinUvOrientation::ObjFlipV,
                }),
            )
        };
        let snapshot = TextureExportSnapshot {
            directory: directory.path().to_path_buf(),
            prefix: "winter".to_owned(),
            pbr_convention: crate::texture_project::TexturePbrConvention::default(),
            images: [
                image(TextureChannel::Diffuse),
                image(TextureChannel::Normal),
            ]
            .into_iter()
            .collect(),
        };

        let claimed = texture_bundle_paths(&snapshot);
        assert!(claimed.len() >= 2, "{claimed:?}");
        assert!(
            claimed
                .iter()
                .all(|path| path.starts_with(directory.path())),
            "everything lands in the chosen folder: {claimed:?}"
        );
        assert!(
            claimed.iter().all(|path| path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("winter"))),
            "the user's name is on every file: {claimed:?}"
        );

        assert!(claimed.iter().all(|path| !path.exists()));

        std::fs::write(&claimed[0], b"already here").expect("fixture write");
        let colliding: Vec<_> = claimed
            .iter()
            .filter(|path| path.exists())
            .cloned()
            .collect();
        assert_eq!(colliding, vec![claimed[0].clone()]);
    }
    use super::*;
    use std::{
        fs,
        sync::atomic::AtomicU64,
        time::{Duration, Instant},
    };

    use serde_json::json;
    use vkit_core::formats::{
        DazBone, DazGeometry, MorphTarget, ObjMorphSource, load_dsf_path, load_obj_morph_target,
        load_ordered_obj,
    };

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn fit_route_restores_pin_first_defaults_and_keeps_auto_explicit() {
        let cases = [
            (false, 0, FitRoute::GeometryAssisted),
            (true, 0, FitRoute::GeometryAssisted),
            (false, 1, FitRoute::GeometryAssisted),
            (true, 1, FitRoute::GeometryAssisted),
            (false, 3, FitRoute::GeometryAssisted),
            (true, 3, FitRoute::GeometryAssisted),
            (false, 4, FitRoute::PinAssisted),
            (true, 4, FitRoute::GeometryAssisted),
            (false, 11, FitRoute::PinAssisted),
            (true, 11, FitRoute::GeometryAssisted),
            (false, 12, FitRoute::ManualPins),
            (true, 12, FitRoute::GeometryAssisted),
            (false, 40, FitRoute::ManualPins),
            (true, 40, FitRoute::GeometryAssisted),
        ];
        for (automatic, complete_pairs, expected) in cases {
            assert_eq!(
                FitRoute::resolve(automatic, complete_pairs),
                expected,
                "automatic={automatic}; complete_pairs={complete_pairs}"
            );
        }
    }

    fn faces() -> Vec<Vec<u32>> {
        vec![
            vec![0, 2, 4],
            vec![2, 1, 4],
            vec![1, 3, 4],
            vec![3, 0, 4],
            vec![2, 0, 5],
            vec![1, 2, 5],
            vec![3, 1, 5],
            vec![0, 3, 5],
        ]
    }

    fn vertices() -> Vec<[f64; 3]> {
        vec![
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ]
    }

    fn geometry() -> DazGeometry {
        let faces = faces();
        DazGeometry::new(
            "synthetic".into(),
            vertices(),
            faces.clone(),
            vkit_core::formats::GroupTable {
                indices: vec![0; faces.len()],
                names: vec!["Head".into()],
            },
            vkit_core::formats::GroupTable {
                indices: vec![0; faces.len()],
                names: vec!["Face".into()],
            },
            json!({}),
        )
        .unwrap()
    }

    fn bone_group_geometry() -> DazGeometry {
        let groups = CANONICAL_G2_JOINTS.len();
        let vertices = (0..groups)
            .flat_map(|group| {
                let x = group as f64 * 10.0;
                [[x, 0.0, 0.0], [x + 1.0, 0.0, 0.0], [x, 1.0, 0.0]]
            })
            .collect::<Vec<_>>();
        let faces = (0..groups)
            .map(|group| {
                let first = group as u32 * 3;
                vec![first, first + 1, first + 2]
            })
            .collect::<Vec<_>>();
        DazGeometry::new(
            "bone-groups".into(),
            vertices,
            faces,
            vkit_core::formats::GroupTable {
                indices: (0..groups as u32).collect(),
                names: CANONICAL_G2_JOINTS
                    .iter()
                    .map(|(id, _)| (*id).into())
                    .collect(),
            },
            vkit_core::formats::GroupTable {
                indices: vec![0; groups],
                names: vec!["Skin".into()],
            },
            json!({}),
        )
        .unwrap()
    }

    #[test]
    fn vam_metadata_uses_a_stable_filename_derived_identifier_and_name_fallback() {
        let first = deterministic_vam_identifier("My Face / 01");
        let second = deterministic_vam_identifier("My Face / 01");
        assert_eq!(first, second);
        assert!(first.starts_with("Vkit_My_Face_01_"));
        assert_ne!(first, deterministic_vam_identifier("My Face / 02"));

        let mut state = AppState::default();
        state.vam_export_display_name = " \0 ".to_owned();
        let metadata = resolve_vam_metadata(&state, Path::new("My Face.vmi")).unwrap();
        assert_eq!(metadata.id, deterministic_vam_identifier("My Face"));
        assert_eq!(metadata.display_name, "My Face");
    }

    #[test]
    fn vam_bone_correction_uses_authored_joint_points_and_vam_axis_units() {
        let mut geometry = bone_group_geometry();
        geometry.bones = CANONICAL_G2_JOINTS
            .into_iter()
            .enumerate()
            .map(|(group, (id, _))| DazBone {
                id: id.to_owned(),
                center_point: geometry.vertices[group * 3],
                end_point: [
                    geometry.vertices[group * 3][0],
                    geometry.vertices[group * 3][1],
                    geometry.vertices[group * 3][2] + 1.0,
                ],
            })
            .collect();
        let mut result = geometry.to_ordered_obj(None).unwrap();
        for group in 0..CANONICAL_G2_JOINTS.len() {
            let amount = (group + 1) as f64;
            for vertex in &mut result.vertices[group * 3..group * 3 + 3] {
                vertex[0] += 100.0 * amount;
                vertex[1] += 200.0 * amount;
                vertex[2] -= 300.0 * amount;
            }
        }

        let formulas = build_vam_bone_correction_formulas(&geometry, &result).unwrap();

        assert_eq!(formulas.len(), CANONICAL_G2_JOINTS.len() * 3);
        for (group_index, (target, _)) in CANONICAL_G2_JOINTS.into_iter().enumerate() {
            let amount = (group_index + 1) as f64;
            for (axis_index, (target_type, expected)) in [
                ("BoneCenterX", -amount),
                ("BoneCenterY", 2.0 * amount),
                ("BoneCenterZ", -3.0 * amount),
            ]
            .into_iter()
            .enumerate()
            {
                let formula = &formulas[group_index * 3 + axis_index];
                assert_eq!(formula.target.as_deref(), Some(target));
                assert_eq!(formula.target_type.as_deref(), Some(target_type));
                assert_eq!(formula.multiplier_value(), Some(expected));
            }
        }
    }

    #[test]
    fn the_canonical_joint_table_is_whole_and_in_the_right_frame() {
        let mut seen = std::collections::BTreeSet::new();
        for (id, _) in CANONICAL_G2_JOINTS {
            assert!(
                canonical_g2_joint_center(id).is_some(),
                "no canonical rest centre for {id}"
            );
            assert!(seen.insert(id), "{id} is in the joint table twice");
        }

        let left = canonical_g2_joint_center("lEye").unwrap();
        let right = canonical_g2_joint_center("rEye").unwrap();
        assert_eq!(left.x, -right.x);
        assert!(left.x > 0.0 && left.y > 160.0);
        assert!(canonical_g2_joint_center("lToe").unwrap().y < 5.0);

        let base = canonical_g2_joint_center("tongueBase").unwrap();
        let tip = canonical_g2_joint_center("tongueTip").unwrap();
        assert!(tip.z > base.z, "the tongue tip sits forward of its base");
    }

    fn canonical_pair_mesh() -> OrderedObjMesh {
        let mut vertices = vec![[0.0, 0.0, 0.0]; CANONICAL_G2_BODY_VERTEX_COUNT];
        vertices[1] = [1.0, 0.0, 0.0];
        vertices[2] = [0.0, 1.0, 0.0];
        OrderedObjMesh {
            vertices,
            faces: vec![vkit_core::formats::ObjFace {
                vertex_indices: vec![0, 1, 2],
                group: Some("Body".to_owned()),
                material: Some("Skin".to_owned()),
            }],
        }
    }

    fn eye_morph() -> MorphTarget {
        let geometry = geometry();
        let mut target = geometry.to_ordered_obj(None).unwrap();
        target.vertices[4][1] -= 0.2;
        load_obj_morph_target(
            "eye_closed",
            ObjMorphSource {
                target: &target,
                base_vertices: &geometry.vertices,
                base_faces: &geometry.faces,
                unit_scale: Some(1.0),
            },
            1.0e-12,
            0.0,
            1.0,
            0.0,
        )
        .unwrap()
    }

    fn synthetic_request() -> ManualFitRequest {
        let geometry = geometry();
        let scan = Mesh::new(
            vertices()
                .into_iter()
                .map(|point| [point[0] + 2.0, point[1] - 1.0, point[2] + 0.5])
                .collect(),
            faces()
                .into_iter()
                .map(|face| [face[0], face[1], face[2]])
                .collect(),
        )
        .unwrap();
        let pins = (0..12)
            .map(|index| {
                let primitive = index % faces().len();
                let face = &faces()[primitive];
                let surface = SurfaceAttachment {
                    triangle_vertex_ids: [face[0], face[1], face[2]],
                    barycentric: match index % 3 {
                        0 => [0.6, 0.2, 0.2],
                        1 => [0.2, 0.6, 0.2],
                        _ => [0.2, 0.2, 0.6],
                    },
                    primitive_id: Some(primitive as u32),
                };
                NumericSurfacePin {
                    pair_index: index as u32,
                    scan: surface.clone(),
                    template: surface,
                    alignment_weight: 1.0,
                    fit_weight: 1.0,
                    confidence: 1.0,
                }
            })
            .collect();
        ManualFitRequest {
            scan,
            template: geometry,
            pins,
            symmetry: SymmetryOptions::default(),
            seam_vertices: Vec::new(),
            anchor_weights: Vec::new(),
            warp: LandmarkWarpOptions::default(),
            topology_guard: TopologyGuardOptions::default(),
            anatomy: AnatomyPreservation::default(),
            scan_fidelity: 1.0,
        }
    }

    fn job_snapshot_from_inputs(
        job_id: u64,
        source_revision: u64,
        inputs: SnapshotInputs,
    ) -> JobSnapshot {
        JobSnapshot {
            job_id,
            source_revision,
            scan: inputs.scan,
            template_base: inputs.template_base,
            eye_morph: inputs.eye_morph,
            pins: inputs.pins,
            symmetry: inputs.symmetry,
            scan_residual_scale: inputs.scan_residual_scale,
            scan_pivot_local: inputs.scan_pivot_local,
            alignment_prior: inputs.alignment_prior,
            fit_reference_value: inputs.fit_reference_value,
            scan_fidelity: inputs.scan_fidelity,
            fit_route: inputs.fit_route,
        }
    }

    fn job_snapshot_from_request(
        job_id: u64,
        source_revision: u64,
        request: ManualFitRequest,
    ) -> JobSnapshot {
        let fit_route = FitRoute::resolve(false, request.pins.len());
        JobSnapshot {
            job_id,
            source_revision,
            scan: Arc::new(request.scan),
            template_base: Arc::new(request.template),
            eye_morph: None,
            pins: request.pins,
            symmetry: request.symmetry,
            scan_residual_scale: [1.0; 3],
            scan_pivot_local: [0.0; 3],
            alignment_prior: SimilarityTransform::IDENTITY,
            fit_reference_value: 0.0,
            scan_fidelity: request.scan_fidelity,
            fit_route,
        }
    }

    fn temp_output(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "vkit-workflow-{label}-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        root.join("result.vmi")
    }

    fn temp_morph_output(label: &str) -> PathBuf {
        let route = temp_output(label)
            .parent()
            .unwrap()
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Morphs")
            .join("female");
        fs::create_dir_all(&route).unwrap();
        route.join("result.vmi")
    }

    fn sculpt_then_morph_state(output_path: &Path) -> AppState {
        let mut state = AppState::default();

        state.output_topology = OutputTopology::Daz;
        state.scan_path = Some(PathBuf::from("scan.obj"));
        state.template_path = Some(PathBuf::from("Genesis2Female.dsf"));
        state.output_path = output_path.display().to_string();
        state.workspace.load_template_geometry(geometry()).unwrap();

        state.workspace.eye_morph = Some(Arc::new(eye_morph()));
        state.dispatch(crate::state::Action::BeginGeneration);
        let (job_id, source_revision) = state.active_job().unwrap();
        state.dispatch(crate::state::Action::FinishGeneration {
            job_id,
            source_revision,
            outcome: GenerationOutcome::Success {
                output: geometry().to_ordered_obj(None).unwrap(),
                fit_reference_value: 0.0,
            },
            morph_available: true,
        });

        assert_eq!(state.active_tab, crate::state::Tab::Morph);
        state.dispatch(crate::state::Action::ApplySculpt);
        assert_eq!(state.active_tab, crate::state::Tab::Morph);
        assert!(!state.export_ready());
        state
    }

    #[test]
    fn snapshot_preserves_local_pins_and_encodes_display_transform_as_prior() {
        let mut state = AppState::default();
        state.transform = crate::state::ScanTransform {
            scale_xyz: [2.0; 3],
            translation_cm: [3.0, -4.0, 5.0],
            rotation_degrees: [0.0, 0.0, 90.0],
        };
        let surface = Arc::new(
            SurfaceMesh::new(
                geometry()
                    .to_ordered_obj(None)
                    .unwrap()
                    .triangulated()
                    .unwrap(),
            )
            .unwrap(),
        );
        state.workspace.scan_source = Some(Arc::clone(&surface));
        state.workspace.scan = Some(Arc::clone(&surface));
        state.workspace.load_template_geometry(geometry()).unwrap();
        for index in 0..12 {
            let endpoint = SurfaceEndpoint {
                triangle: (index % 8) as u32,
                barycentric: [0.6, 0.2, 0.2],
            };
            state.add_surface_pin(crate::scene::MeshSide::Scan, endpoint);
            state.add_surface_pin(crate::scene::MeshSide::Template, endpoint);
        }
        let inputs = collect_inputs(&state).unwrap();
        assert!(Arc::ptr_eq(&inputs.scan, &surface.mesh));
        assert!(Arc::ptr_eq(
            &inputs.template_base,
            state.workspace.template_geometry.as_ref().unwrap()
        ));
        assert_eq!(inputs.pins.len(), 12);
        assert_eq!(inputs.pins[7].pair_index, 7);
        assert_eq!(inputs.pins[7].scan.primitive_id, Some(7));
        assert_eq!(inputs.pins[7].fit_weight, USER_PIN_FIT_WEIGHT);
        assert_eq!(inputs.fit_route, FitRoute::GeometryAssisted);
        assert_eq!(inputs.scan_residual_scale, [1.0; 3]);
        assert_eq!(inputs.scan.vertices[0], [1.0, 0.0, 0.0]);
        assert!(
            glam::DVec3::from_array(
                inputs
                    .alignment_prior
                    .apply(CoreVec3::from_array(inputs.scan.vertices[0]))
                    .to_array()
            )
            .abs_diff_eq(glam::DVec3::new(3.0, -2.0, 5.0), 1.0e-12)
        );
        assert_eq!(inputs.symmetry.mode, SymmetryMode::Off);
    }

    #[test]
    fn similarity_prior_matches_the_pivoted_viewport_transform() {
        let transform = ModelTransform::from_components_with_pivot(
            1.8,
            [4.0, -7.0, 2.0],
            [23.0, -41.0, 67.0],
            [12.0, 28.0, -6.0],
        );
        let prior = similarity_prior(transform);
        for local in [
            glam::DVec3::new(12.0, 28.0, -6.0),
            glam::DVec3::new(14.0, 31.0, -2.0),
            glam::DVec3::new(-3.0, 9.0, 11.0),
        ] {
            let expected = transform.point_to_world(local);
            let actual = prior
                .apply(CoreVec3::from_array(local.to_array()))
                .to_array();
            assert!(glam::DVec3::from_array(actual).abs_diff_eq(expected, 1.0e-12));
        }
    }

    #[test]
    fn nonuniform_residual_and_uniform_prior_equal_the_full_viewport_transform() {
        let scale_xyz = [1.25, 2.4, 0.72];
        let translation_cm = [4.0, -7.0, 2.0];
        let rotation_degrees = [23.0, -41.0, 67.0];
        let pivot_local = [12.0, 28.0, -6.0];
        let (uniform_scale, residual_scale) = decompose_scale_xyz(scale_xyz).unwrap();
        let prior = similarity_prior(ModelTransform::from_components_with_pivot(
            uniform_scale,
            translation_cm,
            rotation_degrees,
            pivot_local,
        ));
        let full_transform = ModelTransform::from_components_xyz_with_pivot(
            scale_xyz,
            translation_cm,
            rotation_degrees,
            pivot_local,
        );
        let source = Mesh::new(
            vec![[12.0, 28.0, -6.0], [14.0, 31.0, -2.0], [-3.0, 9.0, 11.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let residual = residual_scale_scan(&source, residual_scale, pivot_local).unwrap();

        for (local, residual_local) in source.vertices.iter().zip(&residual.vertices) {
            let expected = full_transform.point_to_world(glam::DVec3::from_array(*local));
            let actual = prior
                .apply(CoreVec3::from_array(*residual_local))
                .to_array();
            assert!(glam::DVec3::from_array(actual).abs_diff_eq(expected, 1.0e-11));
        }
        assert!((residual_scale.iter().product::<f64>() - 1.0).abs() <= 1.0e-12);
    }

    #[test]
    fn uniform_scale_decomposition_preserves_source_vertices_exactly() {
        let (uniform, residual) = decompose_scale_xyz([1.8; 3]).unwrap();
        assert_eq!(uniform, 1.8);
        assert_eq!(residual, [1.0; 3]);
        let source = synthetic_request().scan;
        let materialized = residual_scale_scan(&source, residual, [8.0, -2.0, 4.0]).unwrap();
        assert_eq!(materialized.vertices, source.vertices);
        assert_eq!(materialized.triangles, source.triangles);
    }

    #[test]
    fn snapshot_defers_residual_scale_until_worker_materialization() {
        let mut state = AppState::default();
        state.transform.scale_xyz = [1.5, 0.75, 2.0];
        let surface = Arc::new(
            SurfaceMesh::new(
                geometry()
                    .to_ordered_obj(None)
                    .unwrap()
                    .triangulated()
                    .unwrap(),
            )
            .unwrap(),
        );
        let original_vertices = surface.mesh.vertices.clone();
        state.workspace.scan_source = Some(Arc::clone(&surface));
        state.workspace.scan = Some(Arc::clone(&surface));
        state.workspace.load_template_geometry(geometry()).unwrap();

        let inputs = collect_inputs(&state).unwrap();
        assert!(Arc::ptr_eq(&inputs.scan, &surface.mesh));
        assert_eq!(inputs.scan.vertices, original_vertices);
        assert_ne!(inputs.scan_residual_scale, [1.0; 3]);
        let snapshot = job_snapshot_from_inputs(7, 9, inputs);
        let materialized = materialize_scan(&snapshot).unwrap();
        let expected = residual_scale_scan(
            &surface.mesh,
            snapshot.scan_residual_scale,
            snapshot.scan_pivot_local,
        )
        .unwrap();

        assert_eq!(materialized.vertices, expected.vertices);
        assert_eq!(surface.mesh.vertices, original_vertices);
    }

    #[test]
    fn residual_scaling_preserves_pin_barycentrics_and_world_position() {
        let scale_xyz = [1.35, 0.8, 2.1];
        let pivot_local = [0.4, -0.7, 1.2];
        let source = synthetic_request().scan;
        let attachment = SurfaceAttachment {
            triangle_vertex_ids: source.triangles[0],
            barycentric: [0.2, 0.3, 0.5],
            primitive_id: Some(0),
        };
        let (uniform, residual_scale) = decompose_scale_xyz(scale_xyz).unwrap();
        let residual = residual_scale_scan(&source, residual_scale, pivot_local).unwrap();
        let transform = ModelTransform::from_components_xyz_with_pivot(
            scale_xyz,
            [2.0, -3.0, 5.0],
            [17.0, 29.0, -38.0],
            pivot_local,
        );
        let prior = similarity_prior(ModelTransform::from_components_with_pivot(
            uniform,
            [2.0, -3.0, 5.0],
            [17.0, 29.0, -38.0],
            pivot_local,
        ));
        let barycentric_point = |mesh: &Mesh| {
            attachment
                .triangle_vertex_ids
                .iter()
                .zip(attachment.barycentric)
                .fold(glam::DVec3::ZERO, |point, (&vertex_id, weight)| {
                    point + glam::DVec3::from_array(mesh.vertices[vertex_id as usize]) * weight
                })
        };
        let expected = transform.point_to_world(barycentric_point(&source));
        let actual = prior
            .apply(CoreVec3::from_array(
                barycentric_point(&residual).to_array(),
            ))
            .to_array();

        assert_eq!(attachment.triangle_vertex_ids, residual.triangles[0]);
        assert_eq!(attachment.barycentric, [0.2, 0.3, 0.5]);
        assert!(glam::DVec3::from_array(actual).abs_diff_eq(expected, 1.0e-11));
    }

    #[test]
    fn invalid_nonpositive_or_nonfinite_scale_is_rejected() {
        for scale in [[0.0, 1.0, 1.0], [-1.0, 1.0, 1.0], [f64::NAN, 1.0, 1.0]] {
            assert!(matches!(
                decompose_scale_xyz(scale),
                Err(WorkflowError::InvalidScale)
            ));
        }
    }

    #[test]
    fn snapshot_skips_incomplete_slots_and_densely_renumbers_complete_pairs() {
        let mut state = AppState::default();
        let surface = Arc::new(
            SurfaceMesh::new(
                geometry()
                    .to_ordered_obj(None)
                    .unwrap()
                    .triangulated()
                    .unwrap(),
            )
            .unwrap(),
        );
        state.workspace.scan_source = Some(Arc::clone(&surface));
        state.workspace.scan = Some(Arc::clone(&surface));
        state.workspace.load_template_geometry(geometry()).unwrap();
        for index in 0..4 {
            let endpoint = SurfaceEndpoint {
                triangle: index,
                barycentric: [0.6, 0.2, 0.2],
            };
            state
                .workspace
                .pins
                .add(crate::scene::MeshSide::Scan, endpoint);
            state
                .workspace
                .pins
                .add(crate::scene::MeshSide::Template, endpoint);
        }
        assert!(state.workspace.pins.delete(crate::scene::MeshSide::Scan, 0));
        assert!(
            state
                .workspace
                .pins
                .delete(crate::scene::MeshSide::Template, 1)
        );
        assert!(
            state
                .workspace
                .pins
                .delete(crate::scene::MeshSide::Template, 3)
        );

        let inputs = collect_inputs(&state).unwrap();
        assert_eq!(inputs.pins.len(), 1);
        assert_eq!(inputs.pins[0].pair_index, 0);
        assert_eq!(inputs.pins[0].scan.primitive_id, Some(2));
        assert_eq!(inputs.pins[0].template.primitive_id, Some(2));
    }

    #[test]
    fn local_symmetry_snapshot_stays_local_and_preserves_output_symmetry_intent() {
        let source = Arc::new(
            SurfaceMesh::new(
                Mesh::new(
                    vertices(),
                    faces()
                        .into_iter()
                        .map(|face| [face[0], face[1], face[2]])
                        .collect(),
                )
                .unwrap(),
            )
            .unwrap(),
        );
        let mut displayed_vertices = vertices();
        displayed_vertices[0][0] = 2.5;
        let displayed = Arc::new(
            SurfaceMesh::new(
                Mesh::new(
                    displayed_vertices.clone(),
                    faces()
                        .into_iter()
                        .map(|face| [face[0], face[1], face[2]])
                        .collect(),
                )
                .unwrap(),
            )
            .unwrap(),
        );
        let mut state = AppState::default();
        state.workspace.scan_source = Some(source);
        state.workspace.scan = Some(Arc::clone(&displayed));
        state.workspace.load_template_geometry(geometry()).unwrap();
        state.scan_symmetry = ScanSymmetry::PositiveX;
        state.transform = crate::state::ScanTransform {
            scale_xyz: [1.4; 3],
            translation_cm: [3.0, -2.0, 1.0],
            rotation_degrees: [12.0, 25.0, 70.0],
        };

        let inputs = collect_inputs(&state).unwrap();
        let transform = ModelTransform::from_components_with_pivot(
            state.transform.scale_xyz[0],
            state.transform.translation_cm,
            state.transform.rotation_degrees,
            displayed
                .facial_focus_bounds()
                .center()
                .as_dvec3()
                .to_array(),
        );
        let expected = transform
            .point_to_world(glam::DVec3::from_array(displayed_vertices[0]))
            .to_array();
        assert_eq!(inputs.scan.vertices[0], displayed_vertices[0]);
        assert!(
            glam::DVec3::from_array(
                inputs
                    .alignment_prior
                    .apply(CoreVec3::from_array(inputs.scan.vertices[0]))
                    .to_array()
            )
            .abs_diff_eq(glam::DVec3::from_array(expected), 1.0e-12)
        );
        assert_eq!(inputs.symmetry.mode, SymmetryMode::PositiveX);
        assert_eq!(inputs.symmetry.center_x, 0.0);
        assert!(inputs.symmetry.source_preapplied());
    }

    #[test]
    fn open_and_closed_snapshots_apply_morph_to_a_clone_with_explicit_reference() {
        let mut state = AppState::default();
        let scan = Arc::new(
            SurfaceMesh::new(
                geometry()
                    .to_ordered_obj(None)
                    .unwrap()
                    .triangulated()
                    .unwrap(),
            )
            .unwrap(),
        );
        state.workspace.scan_source = Some(Arc::clone(&scan));
        state.workspace.scan = Some(scan);
        state.workspace.load_template_geometry(geometry()).unwrap();
        state.workspace.eye_morph = Some(Arc::new(eye_morph()));
        let original = state
            .workspace
            .template_geometry
            .as_ref()
            .unwrap()
            .vertices
            .clone();

        let open = job_snapshot_from_inputs(1, 1, collect_inputs(&state).unwrap());
        assert_eq!(open.fit_reference_value, 0.0);
        assert!(Arc::ptr_eq(
            &open.template_base,
            state.workspace.template_geometry.as_ref().unwrap()
        ));
        assert_eq!(materialize_template(&open).unwrap().vertices, original);

        state.eye_state = crate::state::EyeState::Closed;
        let closed = job_snapshot_from_inputs(2, 1, collect_inputs(&state).unwrap());
        assert_eq!(closed.fit_reference_value, 1.0);
        assert_eq!(
            materialize_template(&closed).unwrap().vertices[4][1],
            original[4][1] - 0.2
        );
        assert_eq!(
            state.workspace.template_geometry.as_ref().unwrap().vertices,
            original,
            "the selected DSF must remain immutable"
        );
    }

    #[test]
    fn real_pipeline_events_map_monotonically_without_fake_intervals() {
        let events = [
            PipelineProgress {
                stage: PipelineStage::Validation,
                completed_units: 0,
                total_units: 1_000,
            },
            PipelineProgress {
                stage: PipelineStage::DenseRegistration,
                completed_units: 700,
                total_units: 1_000,
            },
            PipelineProgress {
                stage: PipelineStage::Complete,
                completed_units: 1_000,
                total_units: 1_000,
            },
        ];
        let mapped: Vec<_> = events.into_iter().map(map_progress).collect();
        assert!(
            mapped
                .windows(2)
                .all(|pair| pair[0].fraction <= pair[1].fraction)
        );
        assert_eq!(mapped.last().unwrap().fraction, 1.0);
    }

    #[test]
    #[ignore = "requires a local head scan and a licensed canonical G2F DSF"]
    fn zero_pin_workflow_clears_the_anatomy_transition_gate() {
        let scan_path = std::env::var_os("VKIT_SCAN").map(PathBuf::from);
        let template_path = std::env::var_os("VKIT_G2_DSF").map(PathBuf::from);
        let (Some(scan_path), Some(template_path)) = (scan_path, template_path) else {
            eprintln!("scan/G2 corpus is absent; set VKIT_SCAN and VKIT_G2_DSF");
            return;
        };
        if !scan_path.is_file() || !template_path.is_file() {
            eprintln!("scan/G2 corpus is absent; set VKIT_SCAN and VKIT_G2_DSF");
            return;
        }

        let mut state = AppState::default();
        state.workspace.install_prepared_scan(
            crate::scene::PreparedScan::load_with_progress(&scan_path, |_| {}).unwrap(),
        );
        state
            .workspace
            .load_template_geometry(load_dsf_path(&template_path, 0).unwrap())
            .unwrap();
        state.transform = crate::state::ScanTransform {
            scale_xyz: [28.0; 3],
            translation_cm: [0.0, 164.0, -0.3],
            rotation_degrees: [0.0; 3],
        };

        let snapshot = job_snapshot_from_inputs(1, 22, collect_inputs(&state).unwrap());
        assert_eq!(snapshot.fit_route, FitRoute::GeometryAssisted);
        assert!(snapshot.pins.is_empty());
        let outcome = execute_job(&snapshot, &AtomicBool::new(false), |_| {});
        assert!(
            matches!(outcome, GenerationOutcome::Success { .. }),
            "the exact zero-pin UI workflow must clear every guarded stage: {outcome:?}"
        );
    }

    #[test]
    fn cancellation_is_observed_before_any_output_commit() {
        let output = temp_output("cancel");
        let snapshot = job_snapshot_from_request(1, 2, synthetic_request());
        let cancel = AtomicBool::new(true);
        let mut progress = Vec::new();
        let outcome = execute_job(&snapshot, &cancel, |event| progress.push(event));
        assert!(matches!(outcome, GenerationOutcome::Cancelled));
        assert_eq!(progress, vec![Progress::new(JobStage::Prepare, 0.01)]);
        assert!(!output.exists());
        fs::remove_dir_all(output.parent().unwrap()).unwrap();
    }

    #[test]
    fn noninteractive_job_returns_an_in_memory_preview_without_writing() {
        let output = temp_output("success");
        let request = synthetic_request();
        let cancel = AtomicBool::new(false);
        let mut progress = Vec::new();
        let outcome = execute_fit_request(
            &request,
            FitRoute::ManualPins,
            SimilarityTransform::IDENTITY,
            0.0,
            &cancel,
            |event| progress.push(map_progress(event)),
        );
        let GenerationOutcome::Success {
            output: mesh,
            fit_reference_value,
        } = outcome
        else {
            panic!("synthetic native job did not succeed: {outcome:?}");
        };
        assert_eq!(fit_reference_value, 0.0);
        assert!(!output.exists());
        assert_eq!(mesh.vertices.len(), 6);
        assert_eq!(mesh.faces.len(), 8);
        assert_eq!(progress.last().unwrap().fraction, 1.0);
        assert!(
            progress
                .windows(2)
                .all(|pair| pair[0].fraction <= pair[1].fraction)
        );
        fs::remove_dir_all(output.parent().unwrap()).unwrap();
    }

    #[test]
    fn four_valid_pins_fall_back_when_the_tiny_surface_cannot_supply_auto_coverage() {
        let mut request = synthetic_request();
        request.pins.truncate(MIN_ASSISTED_FIT_PAIRS);
        let outcome = execute_fit_request(
            &request,
            FitRoute::GeometryAssisted,
            SimilarityTransform::IDENTITY,
            0.0,
            &AtomicBool::new(false),
            |_| {},
        );
        assert!(
            matches!(outcome, GenerationOutcome::Success { .. }),
            "four reliable pins must retain the established assisted fallback: {outcome:?}"
        );
    }

    #[test]
    fn final_export_writes_the_captured_result_without_running_a_fit() {
        let output_path = temp_morph_output("final-export");
        let basis = Arc::new(canonical_pair_mesh());
        let mut moved = basis.as_ref().clone();
        moved.vertices[9] = [0.5, 0.25, -0.75];
        let expected = Arc::new(moved);
        let snapshot = ExportSnapshot {
            source_revision: 9,
            output_path: output_path.clone(),
            topology: OutputTopology::Daz,
            output: Arc::clone(&expected),
            template_basis: Arc::clone(&basis),
            figure_sex: FigureSex::Female,
            vam_metadata: ShapeVmiOptions::default(),
            vam_bone_correction: false,
            vam_bone_formulas: Vec::new(),
            provider: None,
            genital_applications: Vec::new(),
            baked_textures: None,
        };
        let receipt = write_export_snapshot(&snapshot).unwrap();
        assert_eq!(receipt.committed_paths.len(), 2, "the pair is both halves");
        assert!(output_path.is_file());
        assert!(output_path.with_extension("vmb").is_file());
    }

    #[test]
    fn final_export_commits_baked_channels_and_pbr_counterpart() {
        let output_path = temp_morph_output("final-export-textures");
        let root = output_path.parent().unwrap().to_path_buf();
        let texture_directory = root.join("Textures").join("Female").join("Vkit");
        let basis = Arc::new(canonical_pair_mesh());
        let mut moved = basis.as_ref().clone();
        moved.vertices[9] = [0.5, 0.25, -0.75];
        let expected = Arc::new(moved);
        let gloss = Arc::new(
            crate::skin_preview::SkinImage::new(41, 2, 2, [64, 64, 64, 255].repeat(4)).unwrap(),
        );
        let mut images = std::collections::BTreeMap::new();
        images.insert(crate::texture_project::TextureChannel::Glossiness, gloss);
        let snapshot = ExportSnapshot {
            source_revision: 9,
            output_path: output_path.clone(),
            topology: OutputTopology::Daz,
            output: Arc::clone(&expected),
            template_basis: Arc::clone(&basis),
            figure_sex: FigureSex::Female,
            vam_metadata: ShapeVmiOptions::default(),
            vam_bone_correction: false,
            vam_bone_formulas: Vec::new(),
            provider: None,
            genital_applications: Vec::new(),
            baked_textures: Some(TextureExportSnapshot {
                directory: texture_directory.clone(),
                prefix: "Portrait".to_owned(),
                pbr_convention: crate::texture_project::TexturePbrConvention::MetallicRoughness,
                images,
            }),
        };

        let receipt = write_export_snapshot(&snapshot).unwrap();
        let gloss_path = texture_directory.join("Portrait_gloss.png");
        let roughness_path = texture_directory.join("Portrait_roughness.png");
        assert!(receipt.committed_paths.contains(&output_path));
        assert!(receipt.committed_paths.contains(&gloss_path));
        assert!(receipt.committed_paths.contains(&roughness_path));
        let roughness = image::open(roughness_path).unwrap().into_rgba8();
        assert_eq!(roughness.get_pixel(0, 0).0, [191, 191, 191, 255]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn standalone_texture_bundle_writes_channel_pngs_into_its_folder() {
        let base = temp_output("standalone-texture-bundle");
        let directory = base
            .parent()
            .unwrap()
            .join("Textures")
            .join("Female")
            .join("Vkit");
        let gloss = Arc::new(
            crate::skin_preview::SkinImage::new(7, 2, 2, [64, 64, 64, 255].repeat(4)).unwrap(),
        );
        let mut images = std::collections::BTreeMap::new();
        images.insert(crate::texture_project::TextureChannel::Glossiness, gloss);
        let snapshot = TextureExportSnapshot {
            directory: directory.clone(),
            prefix: "Face".to_owned(),
            pbr_convention: crate::texture_project::TexturePbrConvention::MetallicRoughness,
            images,
        };

        let written = write_texture_bundle(&snapshot).unwrap();
        assert!(!written.is_empty(), "at least one channel is committed");
        for path in &written {
            assert!(
                path.starts_with(&directory),
                "writes stay inside the texture folder"
            );
            assert!(
                path.is_file(),
                "each reported path exists: {}",
                path.display()
            );
            assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("png"));
        }

        let empty = TextureExportSnapshot {
            directory: directory.join("unused"),
            prefix: "Face".to_owned(),
            pbr_convention: crate::texture_project::TexturePbrConvention::MetallicRoughness,
            images: std::collections::BTreeMap::new(),
        };
        assert!(write_texture_bundle(&empty).unwrap().is_empty());
        assert!(!directory.join("unused").exists());

        fs::remove_dir_all(base.parent().unwrap()).ok();
    }

    #[test]
    fn vam_pair_export_uses_authoritative_result_minus_template_and_round_trips() {
        let seed = temp_output("vam-pair-roundtrip");
        let root = seed.parent().unwrap().to_path_buf();
        let output_path = root
            .join("Custom")
            .join("Atom")
            .join("Person")
            .join("Morphs")
            .join("female")
            .join("roundtrip.vmi");
        fs::create_dir_all(output_path.parent().unwrap()).unwrap();
        let basis = Arc::new(canonical_pair_mesh());
        let mut result = basis.as_ref().clone();
        result.vertices[17] = [0.25, -0.5, 1.0];
        let mut snapshot = ExportSnapshot {
            source_revision: 31,
            output_path: output_path.clone(),
            topology: OutputTopology::VaM,
            output: Arc::new(result),
            template_basis: Arc::clone(&basis),
            figure_sex: FigureSex::Female,
            vam_metadata: ShapeVmiOptions {
                id: "roundtrip-id".to_owned(),
                display_name: "Roundtrip Test".to_owned(),
                group: "Vkit Tests".to_owned(),
                region: "Face".to_owned(),
                is_pose_control: false,
            },
            vam_bone_correction: false,
            vam_bone_formulas: Vec::new(),
            provider: None,
            genital_applications: Vec::new(),
            baked_textures: None,
        };

        write_export_snapshot(&snapshot).unwrap();
        let vmi = vkit_core::vam::parse_vmi(&fs::read(&output_path).unwrap()).unwrap();
        assert_eq!(vmi.num_deltas_value(), Some(1));
        assert_eq!(vmi.is_pose_control_value(), Some(false));
        assert!(vmi.formulas.as_ref().is_some_and(Vec::is_empty));
        let mut vmb_path = output_path.clone();
        vmb_path.set_extension("vmb");
        let deltas = vkit_core::vam::decode_vmb_daz_cm_for_topology(
            &fs::read(&vmb_path).unwrap(),
            CANONICAL_G2_BODY_VERTEX_COUNT,
            None,
        )
        .unwrap();
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].vertex_index, 17);
        for (actual, expected) in deltas[0].delta_cm.into_iter().zip([0.25, -0.5, 1.0]) {
            assert!((actual - expected).abs() < 1.0e-6);
        }

        snapshot.output_path = output_path.with_file_name("roundtrip-bones.vmi");
        snapshot.vam_bone_correction = true;
        snapshot.vam_bone_formulas = vec![VmiFormula {
            target_type: Some("BoneCenterX".to_owned()),
            target: Some("lEye".to_owned()),
            multiplier: Some(VmiNumericScalar::String("-0.0125".to_owned())),
            unknown_fields: Default::default(),
        }];
        write_export_snapshot(&snapshot).unwrap();
        let corrected =
            vkit_core::vam::parse_vmi(&fs::read(&snapshot.output_path).unwrap()).unwrap();
        let formulas = corrected.formulas.unwrap();
        assert_eq!(formulas.len(), 1);
        assert_eq!(formulas[0].target_type.as_deref(), Some("BoneCenterX"));
        assert_eq!(formulas[0].target.as_deref(), Some("lEye"));
        assert_eq!(formulas[0].multiplier_value(), Some(-0.0125));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn genital_pair_delta_round_trip_uses_local_wire_indices_only() {
        let mut basis = canonical_pair_mesh();
        basis.vertices.extend([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
        let mut result = basis.clone();
        result.vertices[CANONICAL_G2_BODY_VERTEX_COUNT] = [1.005, 1.9975, 3.00125];
        let deltas =
            sparse_appended_result_delta(CANONICAL_G2_BODY_VERTEX_COUNT, 100.0, &basis, &result)
                .unwrap();
        assert_eq!(deltas.len(), 1);
        assert_eq!(
            deltas[0].vertex_index as usize,
            CANONICAL_G2_BODY_VERTEX_COUNT
        );

        let encoded = encode_vmb_daz_cm_for_topology(
            &deltas,
            basis.vertices.len(),
            Some(VmbVertexRouting::Genitalia),
        )
        .unwrap();
        let local = vkit_core::vam::decode_vmb_daz_cm(&encoded).unwrap();
        assert_eq!(local[0].vertex_index, 0);
        let routed = vkit_core::vam::decode_vmb_daz_cm_for_topology(
            &encoded,
            basis.vertices.len(),
            Some(VmbVertexRouting::Genitalia),
        )
        .unwrap();
        assert_eq!(
            routed[0].vertex_index as usize,
            CANONICAL_G2_BODY_VERTEX_COUNT
        );
        for (actual, expected) in routed[0].delta_cm.into_iter().zip([0.5, -0.25, 0.125]) {
            assert!((actual - expected).abs() < 1.0e-5);
        }

        let metadata = build_shape_vmi_with_options(
            &ShapeVmiOptions {
                id: "graft".to_owned(),
                display_name: "Graft".to_owned(),
                ..ShapeVmiOptions::default()
            },
            deltas.len(),
        );
        assert_eq!(metadata.num_deltas_value(), Some(1));
        assert_eq!(metadata.is_pose_control_value(), Some(false));
        assert!(metadata.formulas.as_ref().is_some_and(Vec::is_empty));
    }

    #[test]
    fn vam_topology_fails_closed_without_writing_daz_geometry() {
        let output_path = temp_morph_output("unsupported-vam-export")
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("female_genitalia")
            .join("result.vmi");
        fs::create_dir_all(output_path.parent().unwrap()).unwrap();
        let snapshot = ExportSnapshot {
            source_revision: 21,
            output_path: output_path.clone(),
            topology: OutputTopology::VaM,
            output: Arc::new(canonical_pair_mesh()),
            template_basis: Arc::new(canonical_pair_mesh()),
            figure_sex: FigureSex::Female,
            vam_metadata: ShapeVmiOptions::default(),
            vam_bone_correction: false,
            vam_bone_formulas: Vec::new(),
            provider: None,
            genital_applications: Vec::new(),
            baked_textures: None,
        };

        assert!(
            write_export_snapshot(&snapshot).is_err(),
            "a genitalia route without a VaM provider must not write anything"
        );
        assert!(!output_path.exists());
    }

    #[test]
    #[ignore = "requires separately licensed VKIT_G2_DSF and VKIT_VAM_FEMALE_BASE files"]
    fn user_owned_vam_provider_routes_a_full_mesh_export() {
        let anchor_path = std::env::var_os("VKIT_G2_DSF")
            .map(PathBuf::from)
            .expect("set VKIT_G2_DSF");
        let base_path = std::env::var_os("VKIT_VAM_FEMALE_BASE")
            .map(PathBuf::from)
            .expect("set VKIT_VAM_FEMALE_BASE");
        let anchor = load_dsf_path(anchor_path, 0).expect("licensed canonical G2F anchor");
        let basis = load_ordered_obj(base_path).expect("user-owned VaM female full-body OBJ");
        let provider = Arc::new(
            VaMGeometryProvider::from_user_owned_pair(
                vkit_core::vam::GeometrySex::Female,
                anchor.clone(),
                basis,
            )
            .expect("validated clean-room provider"),
        );
        let output_path = temp_output("user-owned-vam-full-mesh");
        let canonical = Arc::new(anchor.to_ordered_obj(None).unwrap());
        let snapshot = ExportSnapshot {
            source_revision: 1,
            output_path: output_path.clone(),
            topology: OutputTopology::VaM,
            output: Arc::clone(&canonical),
            template_basis: canonical,
            figure_sex: FigureSex::Female,
            vam_metadata: ShapeVmiOptions::default(),
            vam_bone_correction: false,
            vam_bone_formulas: Vec::new(),
            provider: Some(provider),
            genital_applications: Vec::new(),
            baked_textures: None,
        };

        write_export_snapshot(&snapshot).expect("route canonical Save mesh through VaM provider");
        let written = load_ordered_obj(&output_path).expect("written VaM OBJ");
        assert_eq!(
            written.vertices.len(),
            vkit_core::vam::VAM_FEMALE_VERTEX_COUNT
        );
        assert_eq!(written.faces.len(), vkit_core::vam::VAM_FEMALE_FACE_COUNT);
        fs::remove_dir_all(output_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn format_mismatch_is_rejected_before_any_external_export() {
        let output_path = temp_output("mismatched-format").with_extension("obj");
        let snapshot = ExportSnapshot {
            source_revision: 22,
            output_path: output_path.clone(),
            topology: OutputTopology::Daz,
            output: Arc::new(geometry().to_ordered_obj(None).unwrap()),
            template_basis: Arc::new(geometry().to_ordered_obj(None).unwrap()),
            figure_sex: FigureSex::Female,
            vam_metadata: ShapeVmiOptions::default(),
            vam_bone_correction: false,
            vam_bone_formulas: Vec::new(),
            provider: None,
            genital_applications: Vec::new(),
            baked_textures: None,
        };

        assert!(matches!(
            write_export_snapshot(&snapshot),
            Err(WorkflowError::OutputFormatMismatch {
                format: "VaM (VMI + VMB)",
                expected_extension: "vmi"
            })
        ));
        assert!(!output_path.exists());
        fs::remove_dir_all(output_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn completed_container_replaces_only_at_the_promotion_boundary() {
        let output_path = temp_output("container-promotion").with_extension("glb");
        let root = output_path.parent().unwrap();
        let staged = root.join("staged.glb");
        fs::write(&output_path, b"confirmed previous export").unwrap();
        fs::write(&staged, b"complete new export").unwrap();

        promote_completed_file(&staged, &output_path).unwrap();
        assert_eq!(fs::read(&output_path).unwrap(), b"complete new export");
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn export_snapshot_rejects_an_intermediate_morph_preview() {
        let output_path = temp_output("intermediate-morph-preview");
        let mut state = sculpt_then_morph_state(&output_path);
        assert!(state.workspace.result_output.is_some());

        assert!(matches!(
            export_snapshot_from_state(&state),
            Err(WorkflowError::MissingResult)
        ));

        state.dispatch(crate::state::Action::BakeMorph);
        let snapshot = export_snapshot_from_state(&state).unwrap();
        assert!(Arc::ptr_eq(
            &snapshot.output,
            state.workspace.result_output.as_ref().unwrap()
        ));
        fs::remove_dir_all(output_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn export_coordinator_writes_off_thread_and_rejects_duplicate_start() {
        let output_path = temp_morph_output("async-final-export");
        let basis = Arc::new(canonical_pair_mesh());
        let mut moved = basis.as_ref().clone();
        moved.vertices[9] = [0.5, 0.25, -0.75];
        let expected = Arc::new(moved);
        let snapshot = ExportSnapshot {
            source_revision: 11,
            output_path: output_path.clone(),
            topology: OutputTopology::Daz,
            output: Arc::clone(&expected),
            template_basis: Arc::clone(&basis),
            figure_sex: FigureSex::Female,
            vam_metadata: ShapeVmiOptions::default(),
            vam_bone_correction: false,
            vam_bone_formulas: Vec::new(),
            provider: None,
            genital_applications: Vec::new(),
            baked_textures: None,
        };
        let duplicate = ExportSnapshot {
            source_revision: 11,
            output_path: output_path.clone(),
            topology: OutputTopology::Daz,
            output: Arc::clone(&expected),
            template_basis: Arc::clone(&basis),
            figure_sex: FigureSex::Female,
            vam_metadata: ShapeVmiOptions::default(),
            vam_bone_correction: false,
            vam_bone_formulas: Vec::new(),
            provider: None,
            genital_applications: Vec::new(),
            baked_textures: None,
        };
        let mut coordinator = ExportCoordinator::default();
        coordinator.start(snapshot, || {}).unwrap();
        assert!(coordinator.is_active());
        assert!(coordinator.start(duplicate, || {}).is_err());

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut saw_progress = false;
        let outcome = loop {
            let mut finished = None;
            for event in coordinator.drain() {
                match event {
                    ExportWorkerEvent::Progress { fraction, .. } => {
                        saw_progress |= fraction > 0.0;
                    }
                    ExportWorkerEvent::Finished { outcome, .. } => finished = Some(outcome),
                }
            }
            if let Some(outcome) = finished {
                break outcome;
            }
            assert!(Instant::now() < deadline, "export worker timed out");
            thread::sleep(Duration::from_millis(5));
        };

        assert!(saw_progress);
        assert!(matches!(outcome, ExportOutcome::Success { .. }));
        assert!(!coordinator.is_active());
        assert!(output_path.is_file());
        assert!(output_path.with_extension("vmb").is_file());
    }

    #[test]
    fn coordinator_cancel_is_job_scoped() {
        let coordinator = JobCoordinator::default();
        assert!(!coordinator.cancel(99));
    }

    #[test]
    fn output_path_is_made_absolute_before_final_export() {
        let path = absolute_output_path("relative.obj").unwrap();
        assert!(path.is_absolute());
        assert!(path.ends_with(std::path::Path::new("relative.obj")));
    }
}
