use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use thiserror::Error;

use crate::anatomy::{
    AnatomyError, EyelidCanonicalGuard, EyelidConformationPlan, EyelidConformationReceipt,
    HeadAnatomyReceipt, apply_component_similarity, apply_rigid_translation,
    bounded_control_similarity, mean_control_displacement,
    propagate_head_anatomy_auto_with_topology_guard,
};
use crate::fit::{
    AssistedAlignmentError, AssistedFitOptions, AssistedFitReceipt, AssistedIcpSkipReason,
    AutomaticSurfaceConstraintError, AutomaticSurfaceConstraintOptions,
    AutomaticSurfaceConstraintReceipt, AutomaticSurfaceCorrespondence, BarycentricConstraint,
    DenseRegistrationError, DenseRegistrationOptions, DenseRegistrationReport,
    G2F_HEAD_SKIN_TRIANGLE_COUNT, G2F_HEAD_SKIN_VERTEX_COUNT, G2F_NECK_SEAM_VERTEX_COUNT,
    GeometryInitializationError, GeometryInitializationReceipt, GeometryInitializationRequest,
    LandmarkWarpError, LandmarkWarpOptions, LandmarkWarpReport, LandmarkWarpResult,
    MIN_ASSISTED_FIT_PAIRS, PYTHON_PARITY_DENSE_REGISTRATION_STAGES,
    SemanticSurfaceConstraintReceipt, SkinRegistrationInputs,
    build_automatic_surface_correspondences, build_semantic_surface_correspondences,
    compose_similarity, dense_stages_at_fidelity, estimate_assisted_similarity,
    estimate_assisted_similarity_with_prior, initialize_geometry_assisted_similarity,
    landmark_laplacian_warp, nonrigid_skin_registration, refine_assisted_similarity_icp,
};
use crate::formats::{
    DazGeometry, FormatError, HEAD_SKIN_MATERIALS, HEAD_VISUAL_MATERIALS, Mesh, OrderedObjMesh,
    SurfaceAttachment, matches_canonical_g2f_topology,
};
use crate::math::{SimilarityError, SimilarityTransform, Vec3, estimate_similarity};
use crate::quality::{TopologyQuality, output_topology_quality};
use crate::spatial::{GeometryKernelError, deformation_safety_mask};
use crate::symmetry::{
    SymmetrizedMesh, SymmetryError, SymmetryMode, SymmetryOptions, SymmetryReceipt,
    symmetrize_mesh_x,
};
use crate::{MIN_ALIGNMENT_PAIRS, MIN_FIT_PAIRS};

#[derive(Clone, Debug, PartialEq)]
pub struct NumericSurfacePin {
    pub pair_index: u32,
    pub scan: SurfaceAttachment,
    pub template: SurfaceAttachment,
    pub alignment_weight: f64,
    pub fit_weight: f64,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RigidAnatomyComponent {
    pub vertices: Vec<usize>,
    pub control_vertices: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimilarityAnatomyComponent {
    pub vertices: Vec<usize>,
    pub control_vertices: Vec<usize>,
    pub minimum_scale: f64,
    pub maximum_scale: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnatomyPreservation {
    pub rigid_components: Vec<RigidAnatomyComponent>,
    pub similarity_components: Vec<SimilarityAnatomyComponent>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TopologyGuardOptions {
    pub minimum_orientation_cosine: f64,
    pub minimum_area_ratio: f64,
    pub maximum_area_ratio: f64,
}

impl Default for TopologyGuardOptions {
    fn default() -> Self {
        Self {
            minimum_orientation_cosine: 0.0,
            minimum_area_ratio: 0.05,
            maximum_area_ratio: 20.0,
        }
    }
}

impl TopologyGuardOptions {
    pub const fn canonical_g2() -> Self {
        Self {
            minimum_orientation_cosine: 0.03,
            minimum_area_ratio: 0.08,
            maximum_area_ratio: 4.0,
        }
    }

    fn tightened_for_canonical_g2(self) -> Self {
        let required = Self::canonical_g2();
        Self {
            minimum_orientation_cosine: self
                .minimum_orientation_cosine
                .max(required.minimum_orientation_cosine),
            minimum_area_ratio: self.minimum_area_ratio.max(required.minimum_area_ratio),
            maximum_area_ratio: self.maximum_area_ratio.min(required.maximum_area_ratio),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ManualFitRequest {
    pub scan: Mesh,
    pub template: DazGeometry,
    pub pins: Vec<NumericSurfacePin>,
    pub symmetry: SymmetryOptions,

    pub seam_vertices: Vec<usize>,

    pub anchor_weights: Vec<f64>,
    pub warp: LandmarkWarpOptions,
    pub topology_guard: TopologyGuardOptions,
    pub anatomy: AnatomyPreservation,

    pub scan_fidelity: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalG2NeckConstraints {
    pub seam_vertices: Vec<usize>,
    pub anchor_weights: Vec<f64>,
    pub fade_end: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PipelineStage {
    Validation,
    ScanSymmetry,
    PinResolution,
    Alignment,
    InitialWarp,
    DenseRegistration,
    AnatomyPreservation,
    OutputAssembly,
    QualityValidation,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineProgress {
    pub stage: PipelineStage,
    pub completed_units: u16,
    pub total_units: u16,
}

impl PipelineProgress {
    pub const TOTAL_UNITS: u16 = 1_000;

    #[must_use]
    pub fn fraction(self) -> f64 {
        f64::from(self.completed_units) / f64::from(self.total_units)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnatomyReceipt {
    pub rigid_translations: Vec<Vec3>,
    pub similarity_transforms: Vec<SimilarityTransform>,

    pub head: Option<HeadAnatomyReceipt>,

    pub final_skin_safety_repair: Option<FinalSkinSafetyRepairReceipt>,

    pub eyelid_conformation: Option<EyelidConformationReceipt>,

    pub eyelid_conformation_skipped: bool,

    pub eyelid_conformation_skip_reason: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FinalSkinSafetyRepairReceipt {
    pub attempted: bool,
    pub initial_unsafe_triangles: usize,
    pub final_unsafe_triangles: usize,
    pub adjusted_skin_vertices: usize,
    pub iterations: usize,
    pub minimum_retained_anatomy_displacement_fraction: f64,
    pub protected_vertices_changed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ManualFitResult {
    pub output: OrderedObjMesh,
    pub prepared_scan: Mesh,
    pub aligned_scan: Mesh,
    pub alignment: SimilarityTransform,
    pub symmetry: SymmetryReceipt,
    pub warp: LandmarkWarpReport,

    pub dense_registration: Option<DenseRegistrationReport>,

    pub neck_blend: Option<NeckBlendReceipt>,
    pub anatomy: AnatomyReceipt,
    pub topology: TopologyQuality,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssistedFitResult {
    pub fit: ManualFitResult,
    pub assisted: AssistedFitReceipt,
    pub pin_weight_multipliers: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometryAssistedFitOptions {
    pub prior: SimilarityTransform,
    pub initialization: AssistedFitOptions,
    pub automatic_constraints: AutomaticSurfaceConstraintOptions,

    pub user_constraint_weight_multiplier: f64,
}

impl Default for GeometryAssistedFitOptions {
    fn default() -> Self {
        Self {
            prior: SimilarityTransform::IDENTITY,
            initialization: AssistedFitOptions::default(),
            automatic_constraints: AutomaticSurfaceConstraintOptions::default(),
            user_constraint_weight_multiplier: 4.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomaticPairSource {
    SemanticFaceMeshV2,
    ClosestSurfaceProjection,
    CombinedSemanticAndClosestSurface,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeometryAssistedFitReceipt {
    pub user_pair_count: usize,

    pub automatic_pair_count: usize,
    pub automatic_pair_source: AutomaticPairSource,

    pub effective_automatic_alignment_weight: f64,
    pub effective_automatic_fit_weight: f64,

    pub automatic_regional_constraint_counts: [usize; 6],

    pub effective_automatic_regional_alignment_votes: [f64; 6],

    pub effective_automatic_regional_fit_votes: [f64; 6],

    pub automatic_constraints_applied_to_guarded_fit: bool,
    pub user_constraint_weight_multiplier: f64,
    pub initialization: GeometryInitializationReceipt,

    pub semantic_constraints: Option<SemanticSurfaceConstraintReceipt>,

    pub automatic_constraints: AutomaticSurfaceConstraintReceipt,
    pub guarded_fit: AssistedFitReceipt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeometryAssistedFitResult {
    pub fit: ManualFitResult,
    pub receipt: GeometryAssistedFitReceipt,

    pub user_pin_weight_multipliers: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinSide {
    Scan,
    Template,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnatomyPipelineSubstage {
    HeadPropagation,
    EyelidConformation,
}

impl fmt::Display for AnatomyPipelineSubstage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::HeadPropagation => "head_propagation",
            Self::EyelidConformation => "eyelid_conformation",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardPipelineSubstage {
    FinalSkinSafetyRepair,
    FinalGeometryValidation,
}

impl fmt::Display for GuardPipelineSubstage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::FinalSkinSafetyRepair => "final_skin_safety_repair",
            Self::FinalGeometryValidation => "final_geometry_validation",
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnatomyFailureContext {
    pub substage: AnatomyPipelineSubstage,
    pub strict_user_landmark_count: usize,
    pub automatic_landmark_count: usize,
    pub automatic_landmark_guard_fallback: bool,
    pub landmark_guard_limit_cm: Option<f64>,
    pub topology_guard: TopologyGuardOptions,
}

impl fmt::Display for AnatomyFailureContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "substage={}; strict_user_landmarks={}; automatic_landmarks={}; landmark_guard={}; landmark_guard_limit_cm={:?}; topology_guard=orientation>={:.6},area={:.6}..{:.6}",
            self.substage,
            self.strict_user_landmark_count,
            self.automatic_landmark_count,
            if self.automatic_landmark_guard_fallback {
                "automatic_fallback"
            } else {
                "strict_user"
            },
            self.landmark_guard_limit_cm,
            self.topology_guard.minimum_orientation_cosine,
            self.topology_guard.minimum_area_ratio,
            self.topology_guard.maximum_area_ratio,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GuardFailureContext {
    pub substage: GuardPipelineSubstage,
    pub topology_guard: TopologyGuardOptions,
}

impl fmt::Display for GuardFailureContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "substage={}; topology_guard=orientation>={:.6},area={:.6}..{:.6}",
            self.substage,
            self.topology_guard.minimum_orientation_cosine,
            self.topology_guard.minimum_area_ratio,
            self.topology_guard.maximum_area_ratio,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeometryAssistedFailureContext {
    pub user_pair_count: usize,
    pub automatic_pair_count: usize,
    pub automatic_pair_source: AutomaticPairSource,
    pub automatic_constraints: AutomaticSurfaceConstraintReceipt,
    pub semantic_constraints: Option<SemanticSurfaceConstraintReceipt>,
    pub topology_guard: TopologyGuardOptions,
}

impl fmt::Display for GeometryAssistedFailureContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let semantic_pairs = self
            .semantic_constraints
            .as_ref()
            .map_or(0, |receipt| receipt.accepted_pairs);
        let semantic_rejection = self
            .semantic_constraints
            .as_ref()
            .and_then(|receipt| receipt.rejection_reason);
        write!(
            formatter,
            "user_pairs={}; automatic_pairs={}; automatic_source={:?}; geometry_pairs={}; geometry_rejection={:?}; semantic_pairs={}; semantic_rejection={:?}; topology_guard=orientation>={:.6},area={:.6}..{:.6}",
            self.user_pair_count,
            self.automatic_pair_count,
            self.automatic_pair_source,
            self.automatic_constraints.accepted_constraints,
            self.automatic_constraints.rejection_reason,
            semantic_pairs,
            semantic_rejection,
            self.topology_guard.minimum_orientation_cosine,
            self.topology_guard.minimum_area_ratio,
            self.topology_guard.maximum_area_ratio,
        )
    }
}

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("manual fitting was cancelled during {0:?}")]
    Cancelled(PipelineStage),
    #[error("pin at request position {position} declares pair index {actual}; expected {position}")]
    PinIndexMismatch { position: usize, actual: u32 },
    #[error(
        "pin {pin} has invalid {field}; weights must be finite and non-negative, and confidence must lie in (0, 1]"
    )]
    InvalidPinScalar { pin: usize, field: &'static str },
    #[error("pin {0} is disabled for both alignment and fitting")]
    InactivePin(usize),
    #[error("alignment requires at least {required} positive paired pins, got {actual}")]
    TooFewAlignmentPins { required: usize, actual: usize },
    #[error("fitting requires at least {required} positive paired pins, got {actual}")]
    TooFewFitPins { required: usize, actual: usize },
    #[error("pin {pin} {side:?} primitive {primitive} is outside {triangle_count} triangles")]
    PinPrimitiveOutOfBounds {
        pin: usize,
        side: PinSide,
        primitive: u32,
        triangle_count: usize,
    },
    #[error("pin {pin} {side:?} attachment vertices do not match its mesh primitive")]
    PinTriangleMismatch { pin: usize, side: PinSide },
    #[error("anchor weight count {actual} does not match template vertex count {expected}")]
    AnchorCountMismatch { expected: usize, actual: usize },
    #[error("topology guard thresholds must be finite, ordered, and within their valid ranges")]
    InvalidTopologyGuard,
    #[error("geometry-assisted user constraint multiplier must be finite and at least one")]
    InvalidGeometryAssistedOptions,
    #[error("protected anatomy vertex {vertex} occurs in more than one component")]
    OverlappingAnatomyVertex { vertex: usize },
    #[error(
        "the final anatomy-preserved result contains {unsafe_triangles} unsafe triangles at {triangle_ids:?}"
    )]
    UnsafeFinalGeometry {
        unsafe_triangles: usize,
        triangle_ids: Vec<usize>,
    },
    #[error(
        "canonical G2 head-skin contract mismatch: {vertices} vertices, {triangles} triangles, {seam_vertices} neck seam vertices"
    )]
    CanonicalSkinContract {
        vertices: usize,
        triangles: usize,
        seam_vertices: usize,
    },
    #[error("canonical G2 head skin has no boundary component for a neck seam")]
    CanonicalSkinBoundaryMissing,
    #[error("canonical G2 head-skin symmetry map is invalid: {detail}")]
    CanonicalSkinSymmetryMap { detail: String },
    #[error("request neck seam does not match the canonical 52-vertex G2 seam")]
    CanonicalNeckSeamMismatch,
    #[error("fit landmark {constraint} is attached outside the canonical G2 head skin")]
    LandmarkOutsideHeadSkin { constraint: usize },
    #[error("geometry-assisted guarded fit failed ({context}): {source}")]
    GeometryAssistedGuardedFit {
        context: Box<GeometryAssistedFailureContext>,
        #[source]
        source: Box<PipelineError>,
    },
    #[error("anatomy stage failed ({context}): {source}")]
    AnatomyStage {
        context: AnatomyFailureContext,
        #[source]
        source: Box<AnatomyError>,
    },
    #[error("topology guard stage failed ({context}): {source}")]
    GuardStage {
        context: GuardFailureContext,
        #[source]
        source: GeometryKernelError,
    },
    #[error("generated OBJ violates canonical template topology or material ordering")]
    OutputTopologyViolation { report: TopologyQuality },
    #[error(transparent)]
    Format(#[from] FormatError),
    #[error(transparent)]
    Symmetry(#[from] SymmetryError),
    #[error(transparent)]
    Similarity(#[from] SimilarityError),
    #[error(transparent)]
    AssistedAlignment(#[from] AssistedAlignmentError),
    #[error(transparent)]
    GeometryInitialization(#[from] GeometryInitializationError),
    #[error(transparent)]
    AutomaticSurfaceConstraints(#[from] AutomaticSurfaceConstraintError),
    #[error(transparent)]
    Warp(#[from] LandmarkWarpError),
    #[error(transparent)]
    DenseRegistration(#[from] DenseRegistrationError),
    #[error(transparent)]
    Anatomy(#[from] AnatomyError),
    #[error(transparent)]
    Geometry(#[from] GeometryKernelError),
}

#[derive(Clone)]
struct ResolvedPin {
    scan: Vec3,
    template: SurfaceAttachment,
    template_point: Vec3,
    alignment_weight: f64,
    fit_weight: f64,
    confidence: f64,
}

struct ResolvedAttachment {
    point: Vec3,
    surface: SurfaceAttachment,
}

struct CompactHeadSkin {
    global_vertex_indices: Vec<usize>,
    base_vertices: Vec<Vec3>,
    triangles: Vec<[u32; 3]>,
    constraints: Vec<BarycentricConstraint>,
    seam: Vec<usize>,
    anchor_weights: Vec<f64>,
    fade_end: f64,
}

struct GuardedWarpInput<'a> {
    base_vertices: &'a [Vec3],
    triangles: &'a [[usize; 3]],
    triangle_arrays: &'a [[u32; 3]],
    constraints: &'a [BarycentricConstraint],
    seam: &'a [usize],
    anchor_weights: &'a [f64],
    warp: LandmarkWarpOptions,
    guard: TopologyGuardOptions,
}

pub fn canonical_g2_neck_constraints(
    template: &DazGeometry,
) -> Result<CanonicalG2NeckConstraints, PipelineError> {
    template.validate()?;
    let face_mask = template.face_mask_for_materials(HEAD_SKIN_MATERIALS.iter().copied());
    let global_triangles = triangulated_selected_faces(template, &face_mask);
    let skin_vertices = global_triangles
        .iter()
        .flat_map(|triangle| triangle.iter().copied())
        .collect::<BTreeSet<_>>();
    if skin_vertices.len() != G2F_HEAD_SKIN_VERTEX_COUNT
        || global_triangles.len() != G2F_HEAD_SKIN_TRIANGLE_COUNT
    {
        return Err(PipelineError::CanonicalSkinContract {
            vertices: skin_vertices.len(),
            triangles: global_triangles.len(),
            seam_vertices: 0,
        });
    }
    let mut edge_counts = std::collections::BTreeMap::<(usize, usize), usize>::new();
    for triangle in &global_triangles {
        for (first, second) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let edge = if first < second {
                (first, second)
            } else {
                (second, first)
            };
            *edge_counts.entry(edge).or_default() += 1;
        }
    }
    let mut boundary = std::collections::BTreeMap::<usize, BTreeSet<usize>>::new();
    for ((first, second), count) in edge_counts {
        if count == 1 {
            boundary.entry(first).or_default().insert(second);
            boundary.entry(second).or_default().insert(first);
        }
    }
    let mut seen = BTreeSet::new();
    let mut components = Vec::new();
    for &start in boundary.keys() {
        if !seen.insert(start) {
            continue;
        }
        let mut stack = vec![start];
        let mut component = Vec::new();
        while let Some(vertex) = stack.pop() {
            component.push(vertex);
            if let Some(neighbors) = boundary.get(&vertex) {
                for &neighbor in neighbors.iter().rev() {
                    if seen.insert(neighbor) {
                        stack.push(neighbor);
                    }
                }
            }
        }
        component.sort_unstable();
        components.push(component);
    }
    let seam_vertices = components
        .into_iter()
        .min_by(|left, right| {
            let mean_y = |vertices: &[usize]| {
                vertices
                    .iter()
                    .map(|&index| template.vertices[index][1])
                    .sum::<f64>()
                    / vertices.len() as f64
            };
            mean_y(left).total_cmp(&mean_y(right))
        })
        .ok_or(PipelineError::CanonicalSkinBoundaryMissing)?;
    if seam_vertices.len() != G2F_NECK_SEAM_VERTEX_COUNT {
        return Err(PipelineError::CanonicalSkinContract {
            vertices: skin_vertices.len(),
            triangles: global_triangles.len(),
            seam_vertices: seam_vertices.len(),
        });
    }
    let seam_high = seam_vertices
        .iter()
        .map(|&index| template.vertices[index][1])
        .fold(f64::NEG_INFINITY, f64::max);
    let seam_low = seam_vertices
        .iter()
        .map(|&index| template.vertices[index][1])
        .fold(f64::INFINITY, f64::min);
    let fade_end = seam_high + 3.0;
    let denominator = (fade_end - seam_low).max(1.0e-6);
    let mut anchor_weights = vec![0.0; template.vertices.len()];
    for &index in &skin_vertices {
        let y = template.vertices[index][1];
        if y < fade_end {
            let fade = ((fade_end - y) / denominator).clamp(0.0, 1.0);
            anchor_weights[index] = 30.0 * fade * fade;
        }
    }
    for &index in &seam_vertices {
        anchor_weights[index] = 1_000_000.0;
    }
    Ok(CanonicalG2NeckConstraints {
        seam_vertices,
        anchor_weights,
        fade_end,
    })
}

pub const NECK_BLEND_FULL_RESTORE_GEODESIC_CM: f64 = 1.0;

pub const NECK_BLEND_FALLOFF_SPAN_CM: f64 = 8.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NeckBlendReceipt {
    pub band_vertex_count: usize,

    pub full_weight_vertex_count: usize,
    pub minimum_weight: f64,
    pub maximum_weight: f64,
    pub falloff_start_cm: f64,
    pub falloff_span_cm: f64,

    pub guard_relaxed_vertices: usize,
}

fn neck_blend_weights(rest: &[Vec3], triangles: &[[u32; 3]], seam: &[usize]) -> Vec<f64> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let mut adjacency: Vec<Vec<(u32, f64)>> = vec![Vec::new(); rest.len()];
    for triangle in triangles {
        for (first, second) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let length = (rest[first as usize] - rest[second as usize]).norm();
            adjacency[first as usize].push((second, length));
            adjacency[second as usize].push((first, length));
        }
    }
    let mut distances = vec![f64::INFINITY; rest.len()];
    let mut heap = BinaryHeap::new();
    for &vertex in seam {
        distances[vertex] = 0.0;
        heap.push(Reverse((0_u64, vertex)));
    }
    while let Some(Reverse((bits, vertex))) = heap.pop() {
        let distance = f64::from_bits(bits);
        if distance > distances[vertex] {
            continue;
        }
        for &(neighbor, length) in &adjacency[vertex] {
            let candidate = distance + length;
            if candidate < distances[neighbor as usize] {
                distances[neighbor as usize] = candidate;
                heap.push(Reverse((candidate.to_bits(), neighbor as usize)));
            }
        }
    }
    distances
        .into_iter()
        .map(|distance| {
            let t = ((distance - NECK_BLEND_FULL_RESTORE_GEODESIC_CM) / NECK_BLEND_FALLOFF_SPAN_CM)
                .clamp(0.0, 1.0);
            1.0 - t * t * (3.0 - 2.0 * t)
        })
        .collect()
}

fn apply_canonical_neck_blend(
    rest: &[Vec3],
    triangles: &[[u32; 3]],
    seam: &[usize],
    fitted: &mut [Vec3],
    guard: TopologyGuardOptions,
) -> Result<NeckBlendReceipt, PipelineError> {
    let weights = neck_blend_weights(rest, triangles, seam);
    let rest_arrays = rest.iter().copied().map(Vec3::to_array).collect::<Vec<_>>();
    let before_arrays = fitted
        .iter()
        .copied()
        .map(Vec3::to_array)
        .collect::<Vec<_>>();
    let unsafe_before = deformation_safety_mask(
        &rest_arrays,
        &before_arrays,
        triangles,
        guard.minimum_orientation_cosine,
        guard.minimum_area_ratio,
        guard.maximum_area_ratio,
    )
    .map_err(|source| PipelineError::CanonicalSkinSymmetryMap {
        detail: format!("neck blend guard failed: {source}"),
    })?;
    let before = fitted.to_vec();
    let mut effective = weights.clone();
    let mut guard_relaxed = vec![false; effective.len()];

    let mut halving_passes = 0_usize;
    loop {
        for (vertex, position) in fitted.iter_mut().enumerate() {
            let weight = effective[vertex];
            *position = if weight > 0.0 {
                rest[vertex] * weight + before[vertex] * (1.0 - weight)
            } else {
                before[vertex]
            };
        }
        let blended_arrays = fitted
            .iter()
            .copied()
            .map(Vec3::to_array)
            .collect::<Vec<_>>();
        let unsafe_after = deformation_safety_mask(
            &rest_arrays,
            &blended_arrays,
            triangles,
            guard.minimum_orientation_cosine,
            guard.minimum_area_ratio,
            guard.maximum_area_ratio,
        )
        .map_err(|source| PipelineError::CanonicalSkinSymmetryMap {
            detail: format!("neck blend guard failed: {source}"),
        })?;
        let mut changed = false;
        for (triangle, (&after, &before_unsafe)) in triangles
            .iter()
            .zip(unsafe_after.iter().zip(&unsafe_before))
        {
            if after && !before_unsafe {
                for &vertex in triangle {
                    let vertex = vertex as usize;
                    if effective[vertex] > 0.0 {
                        effective[vertex] = if halving_passes < 8 {
                            effective[vertex] * 0.5
                        } else {
                            0.0
                        };
                        guard_relaxed[vertex] = true;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
        halving_passes += 1;
    }
    let band_vertex_count = effective.iter().filter(|&&weight| weight > 0.0).count();
    let full_weight_vertex_count = effective.iter().filter(|&&weight| weight >= 1.0).count();
    let minimum_weight = effective.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum_weight = effective.iter().copied().fold(0.0_f64, f64::max);
    Ok(NeckBlendReceipt {
        band_vertex_count,
        full_weight_vertex_count,
        minimum_weight,
        maximum_weight,
        falloff_start_cm: NECK_BLEND_FULL_RESTORE_GEODESIC_CM,
        falloff_span_cm: NECK_BLEND_FALLOFF_SPAN_CM,
        guard_relaxed_vertices: guard_relaxed.iter().filter(|&&relaxed| relaxed).count(),
    })
}

pub fn run_manual_fit<P, C>(
    request: &ManualFitRequest,
    mut progress: P,
    mut is_cancelled: C,
) -> Result<ManualFitResult, PipelineError>
where
    P: FnMut(PipelineProgress),
    C: FnMut() -> bool,
{
    run_fit_with_minimums(
        request,
        ValidationMinimums::MANUAL,
        request.symmetry.mode,
        None,
        DenseSolveProfile::PythonParity,
        &mut progress,
        &mut is_cancelled,
    )
}

pub fn run_assisted_fit<P, C>(
    request: &ManualFitRequest,
    options: &AssistedFitOptions,
    progress: P,
    is_cancelled: C,
) -> Result<AssistedFitResult, PipelineError>
where
    P: FnMut(PipelineProgress),
    C: FnMut() -> bool,
{
    run_assisted_fit_internal(
        request,
        options,
        AssistedFitEntry {
            fallback_prior: None,
            icp_mode: AssistedIcpMode::RefineCanonical,
            automatic_pin_start: None,
            pre_symmetrized: None,
        },
        progress,
        is_cancelled,
    )
}

pub fn run_prior_assisted_fit<P, C>(
    request: &ManualFitRequest,
    options: &AssistedFitOptions,
    prior: SimilarityTransform,
    progress: P,
    is_cancelled: C,
) -> Result<AssistedFitResult, PipelineError>
where
    P: FnMut(PipelineProgress),
    C: FnMut() -> bool,
{
    run_assisted_fit_internal(
        request,
        options,
        AssistedFitEntry {
            fallback_prior: Some(prior),
            icp_mode: AssistedIcpMode::RefineCanonical,
            automatic_pin_start: None,
            pre_symmetrized: None,
        },
        progress,
        is_cancelled,
    )
}

#[derive(Clone, Copy)]
enum AssistedIcpMode {
    RefineCanonical,
    SkipAuthoritativeUserPins,
}

fn run_guarded_prior_assisted_fit<P, C>(
    request: &ManualFitRequest,
    options: &AssistedFitOptions,
    prior: SimilarityTransform,
    automatic_pin_start: Option<usize>,
    pre_symmetrized: Option<SymmetrizedMesh>,
    progress: P,
    is_cancelled: C,
) -> Result<AssistedFitResult, PipelineError>
where
    P: FnMut(PipelineProgress),
    C: FnMut() -> bool,
{
    run_assisted_fit_internal(
        request,
        options,
        AssistedFitEntry {
            fallback_prior: Some(prior),
            icp_mode: AssistedIcpMode::SkipAuthoritativeUserPins,
            automatic_pin_start,
            pre_symmetrized,
        },
        progress,
        is_cancelled,
    )
}

struct AssistedFitEntry {
    fallback_prior: Option<SimilarityTransform>,
    icp_mode: AssistedIcpMode,
    automatic_pin_start: Option<usize>,
    pre_symmetrized: Option<SymmetrizedMesh>,
}

fn run_assisted_fit_internal<P, C>(
    request: &ManualFitRequest,
    options: &AssistedFitOptions,
    entry: AssistedFitEntry,
    mut progress: P,
    mut is_cancelled: C,
) -> Result<AssistedFitResult, PipelineError>
where
    P: FnMut(PipelineProgress),
    C: FnMut() -> bool,
{
    let AssistedFitEntry {
        fallback_prior,
        icp_mode,
        automatic_pin_start,
        pre_symmetrized,
    } = entry;
    validate_request(request, ValidationMinimums::ASSISTED)?;
    if is_cancelled() {
        return Err(PipelineError::Cancelled(PipelineStage::Validation));
    }
    let template_surface = request.template.to_ordered_obj(None)?.triangulated()?;

    let symmetrized = match pre_symmetrized {
        Some(symmetrized) => symmetrized,
        None => symmetrize_mesh_x(&request.scan, request.symmetry)?,
    };
    if is_cancelled() {
        return Err(PipelineError::Cancelled(PipelineStage::ScanSymmetry));
    }
    let resolved = resolve_pins(&symmetrized.mesh, &template_surface, &request.pins)?;
    if is_cancelled() {
        return Err(PipelineError::Cancelled(PipelineStage::PinResolution));
    }
    let selected_indices = resolved
        .iter()
        .enumerate()
        .filter_map(|(index, pin)| (pin.alignment_weight > 0.0).then_some(index))
        .collect::<Vec<_>>();
    let source = selected_indices
        .iter()
        .map(|&index| resolved[index].scan)
        .collect::<Vec<_>>();
    let target = selected_indices
        .iter()
        .map(|&index| resolved[index].template_point)
        .collect::<Vec<_>>();
    let weights = selected_indices
        .iter()
        .map(|&index| resolved[index].alignment_weight * resolved[index].confidence)
        .collect::<Vec<_>>();
    let robust = match fallback_prior {
        Some(prior) => {
            estimate_assisted_similarity_with_prior(&source, &target, &weights, prior, options)?
        }
        None => estimate_assisted_similarity(&source, &target, &weights, options)?,
    };
    let mut initialization = robust.transform;
    let mut assisted = robust.receipt;
    let is_canonical_g2 = supports_g2_head_fit(&request.template)?;
    if matches!(icp_mode, AssistedIcpMode::SkipAuthoritativeUserPins) {
        assisted.icp_skip_reason = Some(AssistedIcpSkipReason::AuthoritativeUserPins);
    } else if is_canonical_g2 {
        let constraints = build_warp_constraints(&resolved, initialization, automatic_pin_start)?;
        let skin = compact_head_skin(request, &constraints)?;
        let (refined, iterations, skip_reason) = refine_assisted_similarity_icp(
            initialization,
            &symmetrized.mesh,
            &skin.base_vertices,
            &skin.triangles,
            options,
        );
        initialization = refined;
        assisted.icp_iterations = iterations;
        assisted.icp_skip_reason = skip_reason;
    } else {
        assisted.icp_skip_reason = Some(AssistedIcpSkipReason::NonCanonicalTemplate);
    }
    if is_cancelled() {
        return Err(PipelineError::Cancelled(PipelineStage::Alignment));
    }

    let mut multipliers = vec![1.0; request.pins.len()];
    for (&request_index, multiplier) in selected_indices.iter().zip(&robust.pin_weight_multipliers)
    {
        multipliers[request_index] = *multiplier;
    }
    let transformed_scan = Mesh::new(
        symmetrized
            .mesh
            .vertices
            .iter()
            .copied()
            .map(Vec3::from)
            .map(|vertex| initialization.apply(vertex).to_array())
            .collect(),
        symmetrized.mesh.triangles.clone(),
    )?;
    let mut prepared_request = ManualFitRequest {
        scan: transformed_scan,
        template: request.template.clone(),
        pins: request.pins.clone(),
        symmetry: SymmetryOptions::default(),
        seam_vertices: request.seam_vertices.clone(),
        anchor_weights: request.anchor_weights.clone(),
        warp: request.warp,
        topology_guard: request.topology_guard,
        anatomy: request.anatomy.clone(),
        scan_fidelity: request.scan_fidelity,
    };
    for (pin, multiplier) in prepared_request.pins.iter_mut().zip(&multipliers) {
        pin.alignment_weight *= multiplier;
    }
    let mut fit = run_fit_with_minimums(
        &prepared_request,
        ValidationMinimums::ASSISTED,
        request.symmetry.mode,
        automatic_pin_start,
        DenseSolveProfile::Interactive {
            fidelity: request.scan_fidelity,
        },
        &mut progress,
        &mut is_cancelled,
    )?;
    fit.alignment = compose_similarity(fit.alignment, initialization);
    fit.prepared_scan = symmetrized.mesh;
    fit.symmetry = symmetrized.receipt;
    Ok(AssistedFitResult {
        fit,
        assisted,
        pin_weight_multipliers: multipliers,
    })
}

pub fn run_geometry_assisted_fit<P, C>(
    request: &ManualFitRequest,
    options: &GeometryAssistedFitOptions,
    mut progress: P,
    mut is_cancelled: C,
) -> Result<GeometryAssistedFitResult, PipelineError>
where
    P: FnMut(PipelineProgress),
    C: FnMut() -> bool,
{
    validate_request(request, ValidationMinimums::GEOMETRY_ASSISTED)?;
    if !options.user_constraint_weight_multiplier.is_finite()
        || options.user_constraint_weight_multiplier < 1.0
    {
        return Err(PipelineError::InvalidGeometryAssistedOptions);
    }
    if is_cancelled() {
        return Err(PipelineError::Cancelled(PipelineStage::Validation));
    }

    let template_surface = request.template.to_ordered_obj(None)?.triangulated()?;
    let is_canonical_g2 = supports_g2_head_fit(&request.template)?;
    let symmetrized = symmetrize_mesh_x(&request.scan, request.symmetry)?;
    if is_cancelled() {
        return Err(PipelineError::Cancelled(PipelineStage::ScanSymmetry));
    }
    let resolved = resolve_pins(&symmetrized.mesh, &template_surface, &request.pins)?;
    let selected_indices = resolved
        .iter()
        .enumerate()
        .filter_map(|(index, pin)| (pin.alignment_weight > 0.0).then_some(index))
        .collect::<Vec<_>>();
    let source = selected_indices
        .iter()
        .map(|&index| resolved[index].scan)
        .collect::<Vec<_>>();
    let target = selected_indices
        .iter()
        .map(|&index| resolved[index].template_point)
        .collect::<Vec<_>>();
    let weights = selected_indices
        .iter()
        .map(|&index| resolved[index].alignment_weight * resolved[index].confidence)
        .collect::<Vec<_>>();
    let face_mask = request
        .template
        .face_mask_for_materials(HEAD_SKIN_MATERIALS.iter().copied());
    let mut candidate_triangles = triangulated_selected_face_ids(&request.template, &face_mask);
    if candidate_triangles.is_empty() {
        candidate_triangles = (0..template_surface.triangles.len())
            .map(|index| index as u32)
            .collect();
    }
    let visual_mask = request
        .template
        .face_mask_for_materials(HEAD_VISUAL_MATERIALS.iter().copied());
    let mut visual_triangles = triangulated_selected_face_ids(&request.template, &visual_mask);
    if visual_triangles.is_empty() {
        visual_triangles.clone_from(&candidate_triangles);
    }
    let (initialization_vertices, initialization_triangles) =
        compact_selected_triangle_surface(&template_surface, &candidate_triangles);
    let initialization = initialize_geometry_assisted_similarity(
        GeometryInitializationRequest {
            prior: options.prior,
            scan: &symmetrized.mesh,
            template_vertices: &initialization_vertices,
            template_triangles: &initialization_triangles,
            scan_points: &source,
            template_points: &target,
            weights: &weights,
        },
        &options.initialization,
    )?;
    if is_cancelled() {
        return Err(PipelineError::Cancelled(PipelineStage::Alignment));
    }

    let user_fit_pair_count = resolved.iter().filter(|pin| pin.fit_weight > 0.0).count();
    let user_pins_can_guard_fit = selected_indices.len() >= MIN_ASSISTED_FIT_PAIRS
        && user_fit_pair_count >= MIN_ASSISTED_FIT_PAIRS;

    let geometry_attempt = build_automatic_surface_correspondences(
        &symmetrized.mesh,
        &template_surface,
        &candidate_triangles,
        initialization.transform,
        &options.automatic_constraints,
    );
    let (geometry_correspondences, geometry_receipt, geometry_failed) = match geometry_attempt {
        Ok(result) => (result.correspondences, result.receipt, false),
        Err(error) => (Vec::new(), *error.receipt, true),
    };
    let semantic_attempt = (is_canonical_g2 && !user_pins_can_guard_fit).then(|| {
        build_semantic_surface_correspondences(
            &symmetrized.mesh,
            &template_surface,
            &visual_triangles,
            &candidate_triangles,
            initialization.transform,
        )
    });
    let (semantic_correspondences, semantic_receipt) = match semantic_attempt {
        Some(Ok(result)) => (Some(result.correspondences), Some(result.receipt)),
        Some(Err(error)) => (None, Some(*error.receipt)),
        None => (None, None),
    };
    let (mut automatic_correspondences, automatic_pair_source) =
        if let Some(mut semantic) = semantic_correspondences {
            if geometry_failed {
                (semantic, AutomaticPairSource::SemanticFaceMeshV2)
            } else {
                semantic.extend(geometry_correspondences);
                (
                    semantic,
                    AutomaticPairSource::CombinedSemanticAndClosestSurface,
                )
            }
        } else if !geometry_failed || user_pins_can_guard_fit {
            (
                geometry_correspondences,
                AutomaticPairSource::ClosestSurfaceProjection,
            )
        } else {
            return Err(AutomaticSurfaceConstraintError {
                receipt: Box::new(geometry_receipt),
            }
            .into());
        };
    deduplicate_and_normalize_automatic_correspondences(&mut automatic_correspondences);
    if is_cancelled() {
        return Err(PipelineError::Cancelled(PipelineStage::PinResolution));
    }
    let failure_context = GeometryAssistedFailureContext {
        user_pair_count: request.pins.len(),
        automatic_pair_count: automatic_correspondences.len(),
        automatic_pair_source,
        automatic_constraints: geometry_receipt.clone(),
        semantic_constraints: semantic_receipt.clone(),
        topology_guard: effective_topology_guard(is_canonical_g2, request.topology_guard)?,
    };

    let mut prepared = request.clone();
    for pin in &mut prepared.pins {
        pin.alignment_weight = bounded_user_weight(
            pin.alignment_weight,
            options.user_constraint_weight_multiplier,
        );
        pin.fit_weight =
            bounded_user_weight(pin.fit_weight, options.user_constraint_weight_multiplier);
    }
    let user_fit_weight = prepared
        .pins
        .iter()
        .map(|pin| pin.fit_weight * pin.confidence)
        .sum::<f64>();
    let user_alignment_weight = prepared
        .pins
        .iter()
        .map(|pin| pin.alignment_weight * pin.confidence)
        .sum::<f64>();
    let automatic_fit_weight = effective_automatic_fit_weight(
        user_fit_pair_count,
        user_fit_weight,
        options.automatic_constraints.fit_weight,
        options.automatic_constraints.confidence,
    );

    let automatic_alignment_weight = effective_automatic_alignment_weight(
        selected_indices.len(),
        user_alignment_weight,
        options.automatic_constraints.alignment_weight,
        options.automatic_constraints.confidence,
    );
    if !user_pins_can_guard_fit {
        append_automatic_pins(
            &mut prepared,
            &automatic_correspondences,
            &options.automatic_constraints,
            automatic_alignment_weight,
            automatic_fit_weight,
        );
    }
    renumber_pins(&mut prepared.pins);

    let guarded = if user_pins_can_guard_fit {
        run_guarded_prior_assisted_fit(
            &prepared,
            &options.initialization,
            initialization.transform,
            Some(request.pins.len()),
            Some(symmetrized.clone()),
            &mut progress,
            &mut is_cancelled,
        )
    } else {
        run_assisted_fit_internal(
            &prepared,
            &options.initialization,
            AssistedFitEntry {
                fallback_prior: None,
                icp_mode: AssistedIcpMode::RefineCanonical,
                automatic_pin_start: Some(request.pins.len()),
                pre_symmetrized: Some(symmetrized.clone()),
            },
            &mut progress,
            &mut is_cancelled,
        )
    };
    let (fitted, user_pin_weight_multipliers, used_automatic_only_fallback) = match guarded {
        Ok(fitted) => {
            let multipliers = fitted.pin_weight_multipliers[..request.pins.len()].to_vec();
            (fitted, multipliers, false)
        }
        Err(PipelineError::AssistedAlignment(_))
            if !user_pins_can_guard_fit && !request.pins.is_empty() =>
        {
            let mut automatic_only = request.clone();
            automatic_only.pins.clear();
            append_automatic_pins(
                &mut automatic_only,
                &automatic_correspondences,
                &options.automatic_constraints,
                options.automatic_constraints.alignment_weight,
                options.automatic_constraints.fit_weight,
            );
            renumber_pins(&mut automatic_only.pins);
            let fitted = run_assisted_fit_internal(
                &automatic_only,
                &options.initialization,
                AssistedFitEntry {
                    fallback_prior: None,
                    icp_mode: AssistedIcpMode::RefineCanonical,
                    automatic_pin_start: Some(0),
                    pre_symmetrized: Some(symmetrized.clone()),
                },
                &mut progress,
                &mut is_cancelled,
            )
            .map_err(|error| attach_geometry_assisted_failure(error, &failure_context))?;
            (
                fitted,
                vec![options.initialization.outlier_weight_floor; request.pins.len()],
                true,
            )
        }
        Err(error) => {
            return Err(attach_geometry_assisted_failure(error, &failure_context));
        }
    };
    let effective_alignment_total = if used_automatic_only_fallback {
        options.automatic_constraints.alignment_weight
    } else {
        automatic_alignment_weight
    };
    let effective_fit_total = if used_automatic_only_fallback {
        options.automatic_constraints.fit_weight
    } else {
        automatic_fit_weight
    };
    let (regional_counts, regional_alignment_shares, regional_fit_shares) =
        AutomaticSurfaceCorrespondence::regional_receipt(&automatic_correspondences);
    Ok(GeometryAssistedFitResult {
        fit: fitted.fit,
        receipt: GeometryAssistedFitReceipt {
            user_pair_count: request.pins.len(),
            automatic_pair_count: automatic_correspondences.len(),
            automatic_pair_source,
            effective_automatic_alignment_weight: effective_alignment_total,
            effective_automatic_fit_weight: effective_fit_total,
            automatic_regional_constraint_counts: regional_counts,
            effective_automatic_regional_alignment_votes: regional_alignment_shares.map(|share| {
                share * effective_alignment_total * options.automatic_constraints.confidence
            }),
            effective_automatic_regional_fit_votes: regional_fit_shares.map(|share| {
                share * effective_fit_total * options.automatic_constraints.confidence
            }),
            automatic_constraints_applied_to_guarded_fit: !user_pins_can_guard_fit,
            user_constraint_weight_multiplier: options.user_constraint_weight_multiplier,
            initialization: initialization.receipt,
            semantic_constraints: semantic_receipt,
            automatic_constraints: geometry_receipt,
            guarded_fit: fitted.assisted,
        },
        user_pin_weight_multipliers,
    })
}

fn bounded_user_weight(weight: f64, multiplier: f64) -> f64 {
    if weight == 0.0 {
        0.0
    } else {
        (weight * multiplier).min(25.0)
    }
}

fn attach_geometry_assisted_failure(
    error: PipelineError,
    context: &GeometryAssistedFailureContext,
) -> PipelineError {
    if matches!(
        &error,
        PipelineError::DenseRegistration(_)
            | PipelineError::AnatomyStage { .. }
            | PipelineError::GuardStage { .. }
            | PipelineError::UnsafeFinalGeometry { .. }
            | PipelineError::OutputTopologyViolation { .. }
    ) {
        PipelineError::GeometryAssistedGuardedFit {
            context: Box::new(context.clone()),
            source: Box::new(error),
        }
    } else {
        error
    }
}

fn append_automatic_pins(
    request: &mut ManualFitRequest,
    correspondences: &[AutomaticSurfaceCorrespondence],
    options: &AutomaticSurfaceConstraintOptions,
    alignment_weight: f64,
    fit_weight: f64,
) {
    request.pins.extend(
        correspondences
            .iter()
            .map(|correspondence| NumericSurfacePin {
                pair_index: 0,
                scan: correspondence.scan.clone(),
                template: correspondence.template.clone(),
                alignment_weight: alignment_weight * correspondence.alignment_share,
                fit_weight: fit_weight * correspondence.fit_share,
                confidence: options.confidence,
            }),
    );
}

fn renumber_pins(pins: &mut [NumericSurfacePin]) {
    for (index, pin) in pins.iter_mut().enumerate() {
        pin.pair_index = index as u32;
    }
}

fn effective_automatic_alignment_weight(
    user_pair_count: usize,
    user_weight: f64,
    configured_weight: f64,
    automatic_confidence: f64,
) -> f64 {
    effective_automatic_sparse_weight(
        user_pair_count,
        user_weight,
        configured_weight,
        automatic_confidence,
    )
}

fn effective_automatic_fit_weight(
    user_fit_pair_count: usize,
    user_fit_weight: f64,
    configured_weight: f64,
    automatic_confidence: f64,
) -> f64 {
    effective_automatic_sparse_weight(
        user_fit_pair_count,
        user_fit_weight,
        configured_weight,
        automatic_confidence,
    )
}

fn effective_automatic_sparse_weight(
    user_pair_count: usize,
    user_weight: f64,
    configured_weight: f64,
    automatic_confidence: f64,
) -> f64 {
    if user_pair_count == 0 {
        return configured_weight;
    }
    if user_pair_count >= MIN_ASSISTED_FIT_PAIRS {
        return 0.0;
    }
    let maximum_vote_ratio = 0.25;
    if automatic_confidence <= 0.0 {
        return configured_weight;
    }
    configured_weight.min(user_weight * maximum_vote_ratio / automatic_confidence)
}

fn deduplicate_and_normalize_automatic_correspondences(
    correspondences: &mut Vec<AutomaticSurfaceCorrespondence>,
) {
    let mut attachment_pairs = BTreeSet::new();
    correspondences.retain(|pair| {
        attachment_pairs.insert((pair.scan.primitive_id, pair.template.primitive_id))
    });
    AutomaticSurfaceCorrespondence::normalize_weights(correspondences);
}

#[derive(Clone, Copy)]
struct ValidationMinimums {
    alignment: usize,
    fit: usize,
}

impl ValidationMinimums {
    const MANUAL: Self = Self {
        alignment: MIN_ALIGNMENT_PAIRS,
        fit: MIN_FIT_PAIRS,
    };
    const ASSISTED: Self = Self {
        alignment: MIN_ASSISTED_FIT_PAIRS,
        fit: MIN_ASSISTED_FIT_PAIRS,
    };
    const GEOMETRY_ASSISTED: Self = Self {
        alignment: 0,
        fit: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DenseSolveProfile {
    PythonParity,

    Interactive { fidelity: f64 },
}

impl DenseSolveProfile {
    const fn is_interactive(self) -> bool {
        matches!(self, Self::Interactive { .. })
    }
}

fn run_fit_with_minimums<P, C>(
    request: &ManualFitRequest,
    minimums: ValidationMinimums,
    output_symmetry_mode: SymmetryMode,
    automatic_pin_start: Option<usize>,
    dense_profile: DenseSolveProfile,
    progress: &mut P,
    is_cancelled: &mut C,
) -> Result<ManualFitResult, PipelineError>
where
    P: FnMut(PipelineProgress),
    C: FnMut() -> bool,
{
    checkpoint(PipelineStage::Validation, 0, progress, is_cancelled)?;
    validate_request(request, minimums)?;
    let template_surface = request.template.to_ordered_obj(None)?.triangulated()?;

    checkpoint(PipelineStage::ScanSymmetry, 100, progress, is_cancelled)?;
    let symmetrized = symmetrize_mesh_x(&request.scan, request.symmetry)?;

    checkpoint(PipelineStage::PinResolution, 200, progress, is_cancelled)?;
    let resolved = resolve_pins(&symmetrized.mesh, &template_surface, &request.pins)?;

    checkpoint(PipelineStage::Alignment, 350, progress, is_cancelled)?;
    let alignment = estimate_alignment(&resolved)?;
    let aligned_scan = Mesh::new(
        alignment
            .apply_slice(
                &symmetrized
                    .mesh
                    .vertices
                    .iter()
                    .copied()
                    .map(Vec3::from)
                    .collect::<Vec<_>>(),
            )
            .into_iter()
            .map(Vec3::to_array)
            .collect(),
        symmetrized.mesh.triangles.clone(),
    )?;

    checkpoint(PipelineStage::InitialWarp, 500, progress, is_cancelled)?;
    let canonical: Vec<Vec3> = request
        .template
        .vertices
        .iter()
        .copied()
        .map(Vec3::from)
        .collect();
    let constraints = build_warp_constraints(&resolved, alignment, automatic_pin_start)?;
    let strict_landmark_count = automatic_pin_start.map_or(constraints.len(), |start| {
        resolved
            .iter()
            .take(start)
            .filter(|pin| pin.fit_weight > 0.0)
            .count()
    });
    let is_canonical_g2 = supports_g2_head_fit(&request.template)?;
    let effective_guard = effective_topology_guard(is_canonical_g2, request.topology_guard)?;
    let compact_skin = if is_canonical_g2 {
        Some(compact_head_skin(request, &constraints)?)
    } else {
        None
    };
    let warp = if let Some(skin) = &compact_skin {
        let skin_triangles = skin
            .triangles
            .iter()
            .map(|triangle| triangle.map(|index| index as usize))
            .collect::<Vec<_>>();
        let skin_warp = guarded_landmark_warp(GuardedWarpInput {
            base_vertices: &skin.base_vertices,
            triangles: &skin_triangles,
            triangle_arrays: &skin.triangles,
            constraints: &skin.constraints,
            seam: &skin.seam,
            anchor_weights: &skin.anchor_weights,
            warp: request.warp,
            guard: effective_guard,
        })?;
        let mut vertices = canonical.clone();
        for (&global, vertex) in skin.global_vertex_indices.iter().zip(&skin_warp.vertices) {
            vertices[global] = *vertex;
        }
        LandmarkWarpResult {
            vertices,
            report: skin_warp.report,
        }
    } else {
        let triangles = template_surface
            .triangles
            .iter()
            .map(|triangle| triangle.map(|index| index as usize))
            .collect::<Vec<_>>();
        let anchor_weights = if request.anchor_weights.is_empty() {
            vec![0.0; canonical.len()]
        } else {
            request.anchor_weights.clone()
        };
        guarded_landmark_warp(GuardedWarpInput {
            base_vertices: &canonical,
            triangles: &triangles,
            triangle_arrays: &template_surface.triangles,
            constraints: &constraints,
            seam: &request.seam_vertices,
            anchor_weights: &anchor_weights,
            warp: request.warp,
            guard: effective_guard,
        })?
    };

    let mut neck_blend = None;
    let (mut fitted, dense_registration) = if is_canonical_g2 {
        checkpoint(
            PipelineStage::DenseRegistration,
            650,
            progress,
            is_cancelled,
        )?;
        let skin = compact_skin
            .as_ref()
            .expect("canonical G2 branch constructed a compact head skin");
        let initial_skin = skin
            .global_vertex_indices
            .iter()
            .map(|&index| warp.vertices[index])
            .collect::<Vec<_>>();
        let dense_options = DenseRegistrationOptions {
            minimum_orientation_cosine: effective_guard.minimum_orientation_cosine,
            minimum_area_ratio: effective_guard.minimum_area_ratio,
            maximum_area_ratio: effective_guard.maximum_area_ratio,
            strict_landmark_count,
            stages: match dense_profile {
                DenseSolveProfile::PythonParity => PYTHON_PARITY_DENSE_REGISTRATION_STAGES.to_vec(),
                DenseSolveProfile::Interactive { fidelity } => dense_stages_at_fidelity(fidelity),
            },
            graph_regularization: dense_profile.is_interactive(),
            adaptive_stiffness: dense_profile.is_interactive(),
            ..Default::default()
        };
        let dense = nonrigid_skin_registration(
            SkinRegistrationInputs {
                initial_vertices: &initial_skin,
                base_vertices: &skin.base_vertices,
                triangles: &skin.triangles,
                constraints: &skin.constraints,
                seam: &skin.seam,
                anchor_weights: &skin.anchor_weights,
                fade_end: skin.fade_end,
            },
            &aligned_scan,
            &dense_options,
            |dense_progress| {
                let span = 160_u32;
                let completed = dense_progress.completed_iterations as u32;
                let total = dense_progress.total_iterations.max(1) as u32;
                let units = 650_u32 + span * completed / total;
                (*progress)(PipelineProgress {
                    stage: PipelineStage::DenseRegistration,
                    completed_units: units.min(810) as u16,
                    total_units: PipelineProgress::TOTAL_UNITS,
                });
            },
            &mut *is_cancelled,
        );
        let dense = match dense {
            Err(DenseRegistrationError::Cancelled) => {
                return Err(PipelineError::Cancelled(PipelineStage::DenseRegistration));
            }
            result => result?,
        };

        let mut dense_vertices = dense.vertices.clone();

        if output_symmetry_mode != SymmetryMode::Off && dense_profile.is_interactive() {
            symmetrize_canonical_skin_displacements(
                &skin.base_vertices,
                &skin.triangles,
                &mut dense_vertices,
                output_symmetry_mode,
                effective_guard,
            )?;
        }

        if dense_profile.is_interactive() {
            neck_blend = Some(apply_canonical_neck_blend(
                &skin.base_vertices,
                &skin.triangles,
                &skin.seam,
                &mut dense_vertices,
                effective_guard,
            )?);
        }
        let mut fitted = canonical.clone();
        for (&global, vertex) in skin.global_vertex_indices.iter().zip(&dense_vertices) {
            fitted[global] = *vertex;
        }
        (fitted, Some(dense.report))
    } else {
        (warp.vertices.clone(), None)
    };

    checkpoint(
        PipelineStage::AnatomyPreservation,
        820,
        progress,
        is_cancelled,
    )?;
    let pre_anatomy_fitted = fitted.clone();
    let mut protected_anatomy_vertices = Vec::new();
    let mut anatomy = if is_canonical_g2 {
        let propagated = propagate_head_anatomy_auto_with_topology_guard(
            &request.template,
            &fitted,
            effective_guard.minimum_orientation_cosine,
            effective_guard.minimum_area_ratio,
            effective_guard.maximum_area_ratio,
        )
        .map_err(|source| PipelineError::AnatomyStage {
            context: anatomy_failure_context(
                AnatomyPipelineSubstage::HeadPropagation,
                dense_registration.as_ref(),
                strict_landmark_count,
                constraints.len(),
                effective_guard,
            ),
            source: Box::new(source),
        })?;
        protected_anatomy_vertices = merge_protected_vertices(
            propagated.components.protected_vertices(),
            &propagated.transition_collar,
        );
        fitted = propagated.vertices;
        AnatomyReceipt {
            head: Some(propagated.receipt),
            ..Default::default()
        }
    } else {
        preserve_anatomy(&canonical, &mut fitted, &request.anatomy)?
    };

    if is_canonical_g2 {
        let skin = compact_skin
            .as_ref()
            .expect("canonical G2 branch constructed a compact head skin");
        anatomy.final_skin_safety_repair = Some(
            repair_final_skin_safety(
                &canonical,
                &pre_anatomy_fitted,
                &mut fitted,
                &template_surface.triangles,
                &skin.global_vertex_indices,
                &protected_anatomy_vertices,
                effective_guard,
            )
            .map_err(|source| PipelineError::GuardStage {
                context: GuardFailureContext {
                    substage: GuardPipelineSubstage::FinalSkinSafetyRepair,
                    topology_guard: effective_guard,
                },
                source,
            })?,
        );

        let pinned_lid_points = if dense_profile.is_interactive() {
            fitted_pin_points(&resolved, &pre_anatomy_fitted)
        } else {
            Vec::new()
        };
        let fitted_eyelid_conformation =
            EyelidConformationPlan::build(&request.template).and_then(|plan| {
                plan.conform_bound_vertices_holding_pins(
                    &pre_anatomy_fitted,
                    &mut fitted,
                    EyelidCanonicalGuard {
                        canonical_vertices: &canonical,
                        minimum_orientation_cosine: effective_guard.minimum_orientation_cosine,
                        minimum_area_ratio: effective_guard.minimum_area_ratio,
                        maximum_area_ratio: effective_guard.maximum_area_ratio,
                    },
                    &pinned_lid_points,
                )
            });
        let (eyelid_conformation, eyelid_conformation_skipped, eyelid_skip_reason) =
            optional_eyelid_conformation(fitted_eyelid_conformation);
        anatomy.eyelid_conformation = eyelid_conformation;
        anatomy.eyelid_conformation_skipped = eyelid_conformation_skipped;
        anatomy.eyelid_conformation_skip_reason = eyelid_skip_reason;
    }

    checkpoint(PipelineStage::OutputAssembly, 900, progress, is_cancelled)?;
    let canonical_arrays = canonical
        .iter()
        .copied()
        .map(Vec3::to_array)
        .collect::<Vec<_>>();
    let triangle_arrays = template_surface.triangles.clone();
    let guard_options = effective_guard;
    let final_arrays: Vec<[f64; 3]> = fitted.iter().copied().map(Vec3::to_array).collect();
    let final_unsafe_mask = deformation_safety_mask(
        &canonical_arrays,
        &final_arrays,
        &triangle_arrays,
        guard_options.minimum_orientation_cosine,
        guard_options.minimum_area_ratio,
        guard_options.maximum_area_ratio,
    )
    .map_err(|source| PipelineError::GuardStage {
        context: GuardFailureContext {
            substage: GuardPipelineSubstage::FinalGeometryValidation,
            topology_guard: guard_options,
        },
        source,
    })?;
    let triangle_ids = final_unsafe_mask
        .iter()
        .enumerate()
        .filter_map(|(index, &unsafe_triangle)| unsafe_triangle.then_some(index))
        .collect::<Vec<_>>();
    let unsafe_triangles = triangle_ids.len();
    if unsafe_triangles != 0 {
        return Err(PipelineError::UnsafeFinalGeometry {
            unsafe_triangles,
            triangle_ids,
        });
    }
    let mut output = request.template.to_ordered_obj(None)?;
    output.vertices = final_arrays;
    output.validate()?;

    checkpoint(
        PipelineStage::QualityValidation,
        960,
        progress,
        is_cancelled,
    )?;
    let topology = output_topology_quality(&request.template, &output);
    if !topology.valid {
        return Err(PipelineError::OutputTopologyViolation { report: topology });
    }

    (*progress)(PipelineProgress {
        stage: PipelineStage::Complete,
        completed_units: PipelineProgress::TOTAL_UNITS,
        total_units: PipelineProgress::TOTAL_UNITS,
    });
    Ok(ManualFitResult {
        output,
        prepared_scan: symmetrized.mesh,
        aligned_scan,
        alignment,
        symmetry: symmetrized.receipt,
        warp: warp.report,
        dense_registration,
        neck_blend,
        anatomy,
        topology,
    })
}

fn optional_eyelid_conformation(
    result: Result<EyelidConformationReceipt, AnatomyError>,
) -> (Option<EyelidConformationReceipt>, bool, Option<String>) {
    match result {
        Ok(receipt) => (Some(receipt), false, None),
        Err(error) => (None, true, Some(error.to_string())),
    }
}

fn anatomy_failure_context(
    substage: AnatomyPipelineSubstage,
    dense_registration: Option<&DenseRegistrationReport>,
    strict_landmark_count: usize,
    total_landmark_count: usize,
    topology_guard: TopologyGuardOptions,
) -> AnatomyFailureContext {
    let automatic_landmark_count = total_landmark_count.saturating_sub(strict_landmark_count);
    AnatomyFailureContext {
        substage,
        strict_user_landmark_count: dense_registration.map_or(strict_landmark_count, |report| {
            report.strict_user_landmark_count
        }),
        automatic_landmark_count: dense_registration.map_or(automatic_landmark_count, |report| {
            report.automatic_landmark_count
        }),
        automatic_landmark_guard_fallback: dense_registration.map_or(
            strict_landmark_count == 0 && automatic_landmark_count != 0,
            |report| report.automatic_landmark_guard_fallback,
        ),
        landmark_guard_limit_cm: dense_registration.map(|report| report.landmark_guard_limit_cm),
        topology_guard,
    }
}

fn checkpoint<P, C>(
    stage: PipelineStage,
    completed_units: u16,
    progress: &mut P,
    is_cancelled: &mut C,
) -> Result<(), PipelineError>
where
    P: FnMut(PipelineProgress),
    C: FnMut() -> bool,
{
    progress(PipelineProgress {
        stage,
        completed_units,
        total_units: PipelineProgress::TOTAL_UNITS,
    });
    if is_cancelled() {
        Err(PipelineError::Cancelled(stage))
    } else {
        Ok(())
    }
}

fn supports_g2_head_fit(template: &DazGeometry) -> Result<bool, FormatError> {
    if matches_canonical_g2f_topology(template.vertices.len(), &template.faces)? {
        return Ok(true);
    }
    Ok(template.vertices.len() == crate::G2F_VERTEX_COUNT
        && template
            .root_region
            .pointer("/vkit_import/source")
            .and_then(serde_json::Value::as_str)
            == Some("VaM-runtime-base"))
}

fn validate_request(
    request: &ManualFitRequest,
    minimums: ValidationMinimums,
) -> Result<(), PipelineError> {
    request.scan.require_surface()?;
    request.template.validate()?;
    validate_topology_guard(request.topology_guard)?;
    if !request.anchor_weights.is_empty()
        && request.anchor_weights.len() != request.template.vertices.len()
    {
        return Err(PipelineError::AnchorCountMismatch {
            expected: request.template.vertices.len(),
            actual: request.anchor_weights.len(),
        });
    }
    let mut alignment_count = 0usize;
    let mut fit_count = 0usize;
    for (position, pin) in request.pins.iter().enumerate() {
        if pin.pair_index as usize != position {
            return Err(PipelineError::PinIndexMismatch {
                position,
                actual: pin.pair_index,
            });
        }
        for (field, value) in [
            ("alignment_weight", pin.alignment_weight),
            ("fit_weight", pin.fit_weight),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(PipelineError::InvalidPinScalar {
                    pin: position,
                    field,
                });
            }
        }
        if !pin.confidence.is_finite() || pin.confidence <= 0.0 || pin.confidence > 1.0 {
            return Err(PipelineError::InvalidPinScalar {
                pin: position,
                field: "confidence",
            });
        }
        if pin.alignment_weight == 0.0 && pin.fit_weight == 0.0 {
            return Err(PipelineError::InactivePin(position));
        }
        alignment_count += usize::from(pin.alignment_weight > 0.0);
        fit_count += usize::from(pin.fit_weight > 0.0);
    }
    if alignment_count < minimums.alignment {
        return Err(PipelineError::TooFewAlignmentPins {
            required: minimums.alignment,
            actual: alignment_count,
        });
    }
    if fit_count < minimums.fit {
        return Err(PipelineError::TooFewFitPins {
            required: minimums.fit,
            actual: fit_count,
        });
    }
    validate_anatomy_layout(&request.anatomy)?;
    Ok(())
}

fn validate_topology_guard(options: TopologyGuardOptions) -> Result<(), PipelineError> {
    if !options.minimum_orientation_cosine.is_finite()
        || !(-1.0..=1.0).contains(&options.minimum_orientation_cosine)
        || !options.minimum_area_ratio.is_finite()
        || !options.maximum_area_ratio.is_finite()
        || options.minimum_area_ratio <= 0.0
        || options.minimum_area_ratio > options.maximum_area_ratio
    {
        Err(PipelineError::InvalidTopologyGuard)
    } else {
        Ok(())
    }
}

fn repair_final_skin_safety(
    canonical: &[Vec3],
    scan_fitted: &[Vec3],
    result: &mut [Vec3],
    triangles: &[[u32; 3]],
    skin_vertex_indices: &[usize],
    protected_vertex_indices: &[usize],
    guard: TopologyGuardOptions,
) -> Result<FinalSkinSafetyRepairReceipt, GeometryKernelError> {
    const MAX_ITERATIONS: usize = 16;
    let canonical_arrays = canonical
        .iter()
        .copied()
        .map(Vec3::to_array)
        .collect::<Vec<_>>();
    let mut result_arrays = result
        .iter()
        .copied()
        .map(Vec3::to_array)
        .collect::<Vec<_>>();
    let initial_mask = deformation_safety_mask(
        &canonical_arrays,
        &result_arrays,
        triangles,
        guard.minimum_orientation_cosine,
        guard.minimum_area_ratio,
        guard.maximum_area_ratio,
    )?;
    let initial_unsafe_triangles = initial_mask.iter().filter(|unsafe_| **unsafe_).count();
    if initial_unsafe_triangles == 0 {
        return Ok(FinalSkinSafetyRepairReceipt {
            minimum_retained_anatomy_displacement_fraction: 1.0,
            ..Default::default()
        });
    }

    let mut skin_mask = vec![false; result.len()];
    for &index in skin_vertex_indices {
        if index < skin_mask.len() {
            skin_mask[index] = true;
        }
    }
    let mut protected_mask = vec![false; result.len()];
    for &index in protected_vertex_indices {
        if index < protected_mask.len() {
            protected_mask[index] = true;
        }
    }
    let desired = result.to_vec();
    let displacement = desired
        .iter()
        .copied()
        .zip(scan_fitted.iter().copied())
        .map(|(desired, scan_fit)| desired - scan_fit)
        .collect::<Vec<_>>();
    let mut retained = vec![1.0_f64; result.len()];
    let mut adjusted = BTreeSet::new();
    let mut iterations = 0;
    let mut unsafe_mask = initial_mask;
    for pass in 1..=MAX_ITERATIONS {
        if unsafe_mask.iter().all(|unsafe_| !unsafe_) {
            break;
        }
        let mut candidates = BTreeSet::new();
        for (triangle, unsafe_triangle) in triangles.iter().zip(&unsafe_mask) {
            if *unsafe_triangle {
                candidates.extend(
                    triangle
                        .iter()
                        .map(|index| *index as usize)
                        .filter(|&index| {
                            skin_mask.get(index).copied().unwrap_or(false)
                                && !protected_mask.get(index).copied().unwrap_or(true)
                                && displacement[index].norm() > 1.0e-12
                        }),
                );
            }
        }
        if candidates.is_empty() {
            break;
        }
        iterations = pass;
        for index in candidates {
            retained[index] *= 0.5;
            result[index] = scan_fitted[index] + displacement[index] * retained[index];
            result_arrays[index] = result[index].to_array();
            adjusted.insert(index);
        }
        unsafe_mask = deformation_safety_mask(
            &canonical_arrays,
            &result_arrays,
            triangles,
            guard.minimum_orientation_cosine,
            guard.minimum_area_ratio,
            guard.maximum_area_ratio,
        )?;
    }
    let final_unsafe_triangles = unsafe_mask.iter().filter(|unsafe_| **unsafe_).count();
    let minimum_retained_anatomy_displacement_fraction = adjusted
        .iter()
        .map(|&index| retained[index])
        .reduce(f64::min)
        .unwrap_or(1.0);
    let protected_vertices_changed = protected_mask
        .iter()
        .enumerate()
        .any(|(index, &protected)| protected && result[index] != desired[index]);
    Ok(FinalSkinSafetyRepairReceipt {
        attempted: true,
        initial_unsafe_triangles,
        final_unsafe_triangles,
        adjusted_skin_vertices: adjusted.len(),
        iterations,
        minimum_retained_anatomy_displacement_fraction,
        protected_vertices_changed,
    })
}

fn merge_protected_vertices(mut anatomy: Vec<usize>, transition_collar: &[usize]) -> Vec<usize> {
    anatomy.extend(transition_collar.iter().copied());
    anatomy.sort_unstable();
    anatomy.dedup();
    anatomy
}

fn effective_topology_guard(
    is_canonical_g2: bool,
    requested: TopologyGuardOptions,
) -> Result<TopologyGuardOptions, PipelineError> {
    let effective = if is_canonical_g2 {
        requested.tightened_for_canonical_g2()
    } else {
        requested
    };
    validate_topology_guard(effective)?;
    Ok(effective)
}

fn validate_anatomy_layout(anatomy: &AnatomyPreservation) -> Result<(), PipelineError> {
    let mut protected = BTreeSet::new();
    for vertices in anatomy
        .rigid_components
        .iter()
        .map(|component| component.vertices.as_slice())
        .chain(
            anatomy
                .similarity_components
                .iter()
                .map(|component| component.vertices.as_slice()),
        )
    {
        for &vertex in vertices {
            if !protected.insert(vertex) {
                return Err(PipelineError::OverlappingAnatomyVertex { vertex });
            }
        }
    }
    Ok(())
}

fn resolve_pins(
    scan: &Mesh,
    template: &Mesh,
    pins: &[NumericSurfacePin],
) -> Result<Vec<ResolvedPin>, PipelineError> {
    pins.iter()
        .enumerate()
        .map(|(index, pin)| {
            let scan_attachment = resolve_attachment(scan, &pin.scan, index, PinSide::Scan)?;
            let template_attachment =
                resolve_attachment(template, &pin.template, index, PinSide::Template)?;
            Ok(ResolvedPin {
                scan: scan_attachment.point,
                template: template_attachment.surface,
                template_point: template_attachment.point,
                alignment_weight: pin.alignment_weight,
                fit_weight: pin.fit_weight,
                confidence: pin.confidence,
            })
        })
        .collect()
}

fn resolve_attachment(
    mesh: &Mesh,
    attachment: &SurfaceAttachment,
    pin: usize,
    side: PinSide,
) -> Result<ResolvedAttachment, PipelineError> {
    let mut normalized = attachment.clone();
    normalized.validate_and_normalize()?;
    let primitive = if let Some(primitive) = normalized.primitive_id {
        let triangle = mesh.triangles.get(primitive as usize).ok_or(
            PipelineError::PinPrimitiveOutOfBounds {
                pin,
                side,
                primitive,
                triangle_count: mesh.triangles.len(),
            },
        )?;
        if triangle != &normalized.triangle_vertex_ids {
            return Err(PipelineError::PinTriangleMismatch { pin, side });
        }
        primitive
    } else {
        mesh.triangles
            .iter()
            .position(|triangle| triangle == &normalized.triangle_vertex_ids)
            .ok_or(PipelineError::PinTriangleMismatch { pin, side })? as u32
    };
    normalized.primitive_id = Some(primitive);
    let point = Vec3::from(normalized.resolve(&mesh.vertices)?);
    Ok(ResolvedAttachment {
        point,
        surface: normalized,
    })
}

fn fitted_pin_points(resolved: &[ResolvedPin], fitted: &[Vec3]) -> Vec<Vec3> {
    resolved
        .iter()
        .filter(|pin| pin.fit_weight > 0.0)
        .filter_map(|pin| {
            let mut point = Vec3::ZERO;
            for (&vertex_id, &weight) in pin
                .template
                .triangle_vertex_ids
                .iter()
                .zip(pin.template.barycentric.iter())
            {
                point += *fitted.get(vertex_id as usize)? * weight;
            }
            Some(point)
        })
        .collect()
}

fn estimate_alignment(resolved: &[ResolvedPin]) -> Result<SimilarityTransform, PipelineError> {
    let selected: Vec<_> = resolved
        .iter()
        .filter(|pin| pin.alignment_weight > 0.0)
        .collect();
    let source: Vec<_> = selected.iter().map(|pin| pin.scan).collect();
    let target: Vec<_> = selected.iter().map(|pin| pin.template_point).collect();
    let weights: Vec<_> = selected
        .iter()
        .map(|pin| pin.alignment_weight * pin.confidence)
        .collect();
    Ok(estimate_similarity(&source, &target, Some(&weights))?)
}

fn build_warp_constraints(
    resolved: &[ResolvedPin],
    alignment: SimilarityTransform,
    automatic_pin_start: Option<usize>,
) -> Result<Vec<BarycentricConstraint>, PipelineError> {
    resolved
        .iter()
        .enumerate()
        .filter(|(_, pin)| pin.fit_weight > 0.0)
        .map(|(pin_index, pin)| {
            let mut constraint = BarycentricConstraint::new(
                pin.template.triangle_vertex_ids.map(|index| index as usize),
                pin.template.barycentric,
                alignment.apply(pin.scan),
                pin.fit_weight,
                pin.confidence,
            )?;
            if automatic_pin_start.is_some_and(|start| pin_index >= start) {
                constraint.effective_weight = pin.fit_weight * pin.confidence;
            }
            Ok(constraint)
        })
        .collect()
}

fn symmetrize_canonical_skin_displacements(
    canonical: &[Vec3],
    triangles: &[[u32; 3]],
    fitted: &mut [Vec3],
    mode: SymmetryMode,
    guard: TopologyGuardOptions,
) -> Result<(), PipelineError> {
    if canonical.len() != fitted.len() || canonical.is_empty() {
        return Err(PipelineError::CanonicalSkinSymmetryMap {
            detail: format!(
                "canonical/fitted vertex counts differ ({} vs {})",
                canonical.len(),
                fitted.len()
            ),
        });
    }
    let min = canonical.iter().copied().fold(
        Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
        |bounds, point| {
            Vec3::new(
                bounds.x.min(point.x),
                bounds.y.min(point.y),
                bounds.z.min(point.z),
            )
        },
    );
    let max = canonical.iter().copied().fold(
        Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        |bounds, point| {
            Vec3::new(
                bounds.x.max(point.x),
                bounds.y.max(point.y),
                bounds.z.max(point.z),
            )
        },
    );
    let center_x = (min.x + max.x) * 0.5;
    let tolerance = ((max - min).norm() * 1.0e-4).max(1.0e-6);
    let inverse_cell = 1.0 / tolerance;
    let key = |point: Vec3| {
        (
            (point.x * inverse_cell).round() as i64,
            (point.y * inverse_cell).round() as i64,
            (point.z * inverse_cell).round() as i64,
        )
    };
    let mut cells = BTreeMap::<(i64, i64, i64), Vec<usize>>::new();
    for (index, &point) in canonical.iter().enumerate() {
        cells.entry(key(point)).or_default().push(index);
    }
    let mut mirror = vec![usize::MAX; canonical.len()];
    for (index, &point) in canonical.iter().enumerate() {
        let reflected = Vec3::new(2.0 * center_x - point.x, point.y, point.z);
        let base = key(reflected);
        let mut best: Option<(f64, usize)> = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(candidates) = cells.get(&(base.0 + dx, base.1 + dy, base.2 + dz))
                    else {
                        continue;
                    };
                    for &candidate in candidates {
                        let distance = (canonical[candidate] - reflected).norm();
                        if distance <= tolerance
                            && best.is_none_or(|(best_distance, best_index)| {
                                distance < best_distance - 1.0e-12
                                    || ((distance - best_distance).abs() <= 1.0e-12
                                        && candidate < best_index)
                            })
                        {
                            best = Some((distance, candidate));
                        }
                    }
                }
            }
        }
        mirror[index] = best.map(|(_, candidate)| candidate).ok_or_else(|| {
            PipelineError::CanonicalSkinSymmetryMap {
                detail: format!("vertex {index} has no reflected canonical counterpart"),
            }
        })?;
    }
    for (index, &counterpart) in mirror.iter().enumerate() {
        if mirror.get(counterpart).copied() != Some(index) {
            return Err(PipelineError::CanonicalSkinSymmetryMap {
                detail: format!("mapping {index}->{counterpart} is not involutive"),
            });
        }
    }
    let canonical_faces = triangles
        .iter()
        .map(|triangle| {
            let mut face = triangle.map(|index| index as usize);
            face.sort_unstable();
            face
        })
        .collect::<BTreeSet<_>>();
    for triangle in triangles {
        let mut reflected = triangle.map(|index| mirror[index as usize]);
        reflected.sort_unstable();
        if !canonical_faces.contains(&reflected) {
            return Err(PipelineError::CanonicalSkinSymmetryMap {
                detail: "reflected triangle is absent from canonical topology".to_owned(),
            });
        }
    }

    let reflect_vector = |delta: Vec3| Vec3::new(-delta.x, delta.y, delta.z);

    let keep_sign = match mode {
        SymmetryMode::PositiveX => 1.0,
        SymmetryMode::NegativeX => -1.0,

        SymmetryMode::Off => 0.0,
    };
    let original = fitted.to_vec();
    let mut symmetric = original.clone();
    for index in 0..canonical.len() {
        let counterpart = mirror[index];
        if index > counterpart {
            continue;
        }
        if index == counterpart {
            let delta = original[index] - canonical[index];
            symmetric[index] = canonical[index] + Vec3::new(0.0, delta.y, delta.z);
            continue;
        }

        let source = if (canonical[index].x - center_x) * keep_sign >= 0.0 {
            index
        } else {
            counterpart
        };
        let other = if source == index { counterpart } else { index };
        let delta = original[source] - canonical[source];
        symmetric[source] = canonical[source] + delta;
        symmetric[other] = canonical[other] + reflect_vector(delta);
    }

    let canonical_arrays = canonical
        .iter()
        .copied()
        .map(Vec3::to_array)
        .collect::<Vec<_>>();
    let symmetric_delta = symmetric
        .iter()
        .zip(canonical)
        .map(|(candidate, base)| *candidate - *base)
        .collect::<Vec<_>>();
    let mut factor = 1.0;
    for _ in 0..10 {
        let candidate = canonical
            .iter()
            .zip(&symmetric_delta)
            .map(|(base, delta)| *base + *delta * factor)
            .collect::<Vec<_>>();
        let candidate_arrays = candidate
            .iter()
            .copied()
            .map(Vec3::to_array)
            .collect::<Vec<_>>();
        let unsafe_mask = deformation_safety_mask(
            &canonical_arrays,
            &candidate_arrays,
            triangles,
            guard.minimum_orientation_cosine,
            guard.minimum_area_ratio,
            guard.maximum_area_ratio,
        )?;
        if !unsafe_mask.iter().any(|unsafe_triangle| *unsafe_triangle) {
            fitted.clone_from_slice(&candidate);
            return Ok(());
        }
        factor *= 0.5;
    }
    Err(PipelineError::CanonicalSkinSymmetryMap {
        detail: "topology-safe symmetric displacement could not be found".to_owned(),
    })
}

fn guarded_landmark_warp(input: GuardedWarpInput<'_>) -> Result<LandmarkWarpResult, PipelineError> {
    let guard_error = Cell::new(None);
    let canonical_arrays = input
        .base_vertices
        .iter()
        .copied()
        .map(Vec3::to_array)
        .collect::<Vec<_>>();
    let warp = landmark_laplacian_warp(
        input.base_vertices,
        input.triangles,
        input.constraints,
        input.seam,
        input.anchor_weights,
        input.warp,
        |candidate| {
            let candidate = candidate
                .iter()
                .copied()
                .map(Vec3::to_array)
                .collect::<Vec<_>>();
            match deformation_safety_mask(
                &canonical_arrays,
                &candidate,
                input.triangle_arrays,
                input.guard.minimum_orientation_cosine,
                input.guard.minimum_area_ratio,
                input.guard.maximum_area_ratio,
            ) {
                Ok(unsafe_mask) => unsafe_mask
                    .into_iter()
                    .all(|unsafe_triangle| !unsafe_triangle),
                Err(error) => {
                    guard_error.set(Some(error));
                    false
                }
            }
        },
    );
    if let Some(error) = guard_error.get() {
        return Err(PipelineError::Geometry(error));
    }
    Ok(warp?)
}

fn triangulated_selected_faces(template: &DazGeometry, face_mask: &[bool]) -> Vec<[usize; 3]> {
    let mut triangles = Vec::new();
    for (face, selected) in template.faces.iter().zip(face_mask) {
        if !selected {
            continue;
        }
        triangles.push([face[0] as usize, face[1] as usize, face[2] as usize]);
        if face.len() == 4 {
            triangles.push([face[0] as usize, face[2] as usize, face[3] as usize]);
        }
    }
    triangles
}

fn triangulated_selected_face_ids(template: &DazGeometry, face_mask: &[bool]) -> Vec<u32> {
    let mut selected_triangles = Vec::new();
    let mut triangle_id = 0_u32;
    for (face, selected) in template.faces.iter().zip(face_mask) {
        if *selected {
            selected_triangles.push(triangle_id);
            if face.len() == 4 {
                selected_triangles.push(triangle_id + 1);
            }
        }
        triangle_id += if face.len() == 4 { 2 } else { 1 };
    }
    selected_triangles
}

fn compact_selected_triangle_surface(
    mesh: &Mesh,
    selected_triangle_ids: &[u32],
) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let selected_vertices = selected_triangle_ids
        .iter()
        .flat_map(|&triangle| mesh.triangles[triangle as usize])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut global_to_local = vec![u32::MAX; mesh.vertices.len()];
    for (local, &global) in selected_vertices.iter().enumerate() {
        global_to_local[global as usize] = local as u32;
    }
    let vertices = selected_vertices
        .iter()
        .map(|&global| Vec3::from(mesh.vertices[global as usize]))
        .collect();
    let triangles = selected_triangle_ids
        .iter()
        .map(|&triangle| {
            mesh.triangles[triangle as usize].map(|global| global_to_local[global as usize])
        })
        .collect();
    (vertices, triangles)
}

fn compact_head_skin(
    request: &ManualFitRequest,
    constraints: &[BarycentricConstraint],
) -> Result<CompactHeadSkin, PipelineError> {
    let face_mask = request
        .template
        .face_mask_for_materials(HEAD_SKIN_MATERIALS.iter().copied());
    let global_triangles = triangulated_selected_faces(&request.template, &face_mask);
    let global_vertex_indices = global_triangles
        .iter()
        .flat_map(|triangle| triangle.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if global_vertex_indices.len() != G2F_HEAD_SKIN_VERTEX_COUNT
        || global_triangles.len() != G2F_HEAD_SKIN_TRIANGLE_COUNT
        || request.seam_vertices.len() != G2F_NECK_SEAM_VERTEX_COUNT
    {
        return Err(PipelineError::CanonicalSkinContract {
            vertices: global_vertex_indices.len(),
            triangles: global_triangles.len(),
            seam_vertices: request.seam_vertices.len(),
        });
    }
    let mut global_to_local = vec![usize::MAX; request.template.vertices.len()];
    for (local, &global) in global_vertex_indices.iter().enumerate() {
        global_to_local[global] = local;
    }
    let triangles = global_triangles
        .iter()
        .map(|triangle| triangle.map(|global| global_to_local[global] as u32))
        .collect::<Vec<_>>();
    let base_vertices = global_vertex_indices
        .iter()
        .map(|&global| Vec3::from(request.template.vertices[global]))
        .collect::<Vec<_>>();
    let neck = canonical_g2_neck_constraints(&request.template)?;
    let mut requested_seam = request.seam_vertices.clone();
    requested_seam.sort_unstable();
    if requested_seam != neck.seam_vertices {
        return Err(PipelineError::CanonicalNeckSeamMismatch);
    }
    let seam = neck
        .seam_vertices
        .iter()
        .map(|&global| {
            global_to_local
                .get(global)
                .copied()
                .filter(|local| *local != usize::MAX)
                .ok_or(PipelineError::CanonicalSkinContract {
                    vertices: global_vertex_indices.len(),
                    triangles: global_triangles.len(),
                    seam_vertices: 0,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let anchor_weights = global_vertex_indices
        .iter()
        .map(|&global| neck.anchor_weights[global])
        .collect::<Vec<_>>();
    let constraints = constraints
        .iter()
        .enumerate()
        .map(|(constraint_index, constraint)| {
            let mut local = [0; 3];
            for (corner, &global) in constraint.vertex_indices.iter().enumerate() {
                let mapped = global_to_local
                    .get(global)
                    .copied()
                    .filter(|local| *local != usize::MAX)
                    .ok_or(PipelineError::LandmarkOutsideHeadSkin {
                        constraint: constraint_index,
                    })?;
                local[corner] = mapped;
            }
            Ok(BarycentricConstraint {
                vertex_indices: local,
                barycentric: constraint.barycentric,
                target: constraint.target,
                effective_weight: constraint.effective_weight,
            })
        })
        .collect::<Result<Vec<_>, PipelineError>>()?;
    Ok(CompactHeadSkin {
        global_vertex_indices,
        base_vertices,
        triangles,
        constraints,
        seam,
        anchor_weights,
        fade_end: neck.fade_end,
    })
}

fn preserve_anatomy(
    canonical: &[Vec3],
    fitted: &mut [Vec3],
    anatomy: &AnatomyPreservation,
) -> Result<AnatomyReceipt, PipelineError> {
    let mut receipt = AnatomyReceipt::default();
    for component in &anatomy.rigid_components {
        let translation =
            mean_control_displacement(canonical, fitted, &component.control_vertices)?;
        apply_rigid_translation(canonical, fitted, &component.vertices, translation)?;
        receipt.rigid_translations.push(translation);
    }
    for component in &anatomy.similarity_components {
        let transform = bounded_control_similarity(
            canonical,
            fitted,
            &component.control_vertices,
            component.minimum_scale,
            component.maximum_scale,
        )?;
        apply_component_similarity(canonical, fitted, &component.vertices, transform)?;
        receipt.similarity_transforms.push(transform);
    }
    Ok(receipt)
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    use crate::anatomy::{EyeSide, EyelidSideReceipt};

    #[test]
    fn canonical_g2_guard_cannot_be_weakened() {
        let weak = TopologyGuardOptions::default();
        assert_eq!(
            effective_topology_guard(true, weak).unwrap(),
            TopologyGuardOptions::canonical_g2()
        );
    }

    #[test]
    fn canonical_g2_guard_preserves_stronger_caller_values() {
        let strong = TopologyGuardOptions {
            minimum_orientation_cosine: 0.08,
            minimum_area_ratio: 0.15,
            maximum_area_ratio: 2.5,
        };
        assert_eq!(effective_topology_guard(true, strong).unwrap(), strong);
    }

    #[test]
    fn generic_mesh_guard_is_unchanged() {
        let generic = TopologyGuardOptions::default();
        assert_eq!(effective_topology_guard(false, generic).unwrap(), generic);
    }

    #[test]
    fn automatic_fit_weight_is_full_only_without_user_constraints() {
        assert_eq!(effective_automatic_fit_weight(0, 0.0, 0.2, 0.65), 0.2);
        assert_eq!(effective_automatic_fit_weight(4, 16.0, 0.2, 0.65), 0.0);
    }

    #[test]
    fn sparse_user_constraints_cap_the_total_automatic_fit_vote() {
        let weight = effective_automatic_fit_weight(2, 8.0, 0.2, 0.65);
        let automatic_total = weight * 0.65;
        assert!((automatic_total - 0.13).abs() <= 1.0e-12);
        assert!(automatic_total <= 8.0 * 0.25);
    }

    #[test]
    fn sparse_user_constraints_also_dominate_semantic_alignment_vote() {
        let weight = effective_automatic_alignment_weight(1, 4.0, 0.25, 0.65);
        let automatic_total = weight * 0.65;
        assert!((automatic_total - 0.1625).abs() <= 1.0e-12);
        assert!(automatic_total <= 4.0 * 0.25);
        assert_eq!(
            effective_automatic_alignment_weight(4, 16.0, 0.25, 0.65),
            0.0
        );
    }

    #[test]
    fn automatic_constraints_bypass_the_user_pin_floor_without_weakening_users() {
        let attachment = SurfaceAttachment {
            triangle_vertex_ids: [0, 1, 2],
            barycentric: [1.0, 0.0, 0.0],
            primitive_id: Some(0),
        };
        let pin = |fit_weight| ResolvedPin {
            scan: Vec3::ZERO,
            template: attachment.clone(),
            template_point: Vec3::ZERO,
            alignment_weight: 1.0,
            fit_weight,
            confidence: 1.0,
        };
        let constraints = build_warp_constraints(
            &[pin(0.01), pin(0.001)],
            SimilarityTransform::IDENTITY,
            Some(1),
        )
        .unwrap();
        assert_eq!(constraints[0].effective_weight, 0.05);
        assert_eq!(constraints[1].effective_weight, 0.001);
    }

    #[test]
    fn geometry_assisted_failure_includes_automatic_guard_and_anatomy_substage_context() {
        let source = PipelineError::AnatomyStage {
            context: AnatomyFailureContext {
                substage: AnatomyPipelineSubstage::EyelidConformation,
                strict_user_landmark_count: 0,
                automatic_landmark_count: 120,
                automatic_landmark_guard_fallback: true,
                landmark_guard_limit_cm: Some(0.42),
                topology_guard: TopologyGuardOptions::canonical_g2(),
            },
            source: Box::new(AnatomyError::InvalidTransitionConfiguration),
        };
        let error = PipelineError::GeometryAssistedGuardedFit {
            context: Box::new(GeometryAssistedFailureContext {
                user_pair_count: 0,
                automatic_pair_count: 120,
                automatic_pair_source: AutomaticPairSource::ClosestSurfaceProjection,
                automatic_constraints: AutomaticSurfaceConstraintReceipt {
                    provenance: crate::fit::AutomaticConstraintProvenance::ClosestSurfaceProjection,
                    sampled_triangles: 160,
                    projected_candidates: 120,
                    duplicate_scan_primitives_removed: 0,
                    accepted_constraints: 120,
                    coverage: 0.75,
                    rms_distance_cm: Some(0.2),
                    maximum_accepted_distance_cm: Some(0.4),
                    normals_were_flipped: false,
                    regional_constraint_counts: [20; 6],
                    regional_alignment_shares: [1.0 / 6.0; 6],
                    regional_fit_shares: [1.0 / 6.0; 6],
                    rejection_reason: None,
                },
                semantic_constraints: None,
                topology_guard: TopologyGuardOptions::canonical_g2(),
            }),
            source: Box::new(source),
        };
        let diagnostic = error.to_string();
        for expected in [
            "automatic_pairs=120",
            "semantic_pairs=0",
            "substage=eyelid_conformation",
            "landmark_guard=automatic_fallback",
            "topology_guard=orientation>=0.030000,area=0.080000..4.000000",
        ] {
            assert!(
                diagnostic.contains(expected),
                "missing {expected}: {diagnostic}"
            );
        }
    }

    #[test]
    fn every_eyelid_conformation_failure_degrades_to_a_skip_with_a_reason() {
        let failures: Vec<AnatomyError> = vec![
            AnatomyError::InvalidTransitionConfiguration,
            AnatomyError::MissingComponent { component: "lEye" },
            AnatomyError::MissingComponent {
                component: "21-vertex eye apertures",
            },
            AnatomyError::VertexCountMismatch {
                base: 21_556,
                fitted: 21_000,
            },
            AnatomyError::NonFiniteVertex {
                name: "eyelid candidate",
                index: 7,
            },
        ];
        for failure in failures {
            let reason = failure.to_string();
            let (receipt, skipped, skip_reason) = optional_eyelid_conformation(Err(failure));
            assert!(receipt.is_none());
            assert!(skipped);
            assert_eq!(skip_reason.as_deref(), Some(reason.as_str()));
        }
    }

    #[test]
    fn successful_eyelid_conformation_is_not_reported_as_skipped() {
        let side = |side| EyelidSideReceipt {
            side,
            aperture_vertex_count: 21,
            affected_skin_vertex_count: 100,
            transition_ring_count: 4,
            globe_vertex_count: 571,
            globe_radius_cm: 1.6,
            globe_fit_rms_cm: 0.01,
            globe_translation: Vec3::ZERO,
            globe_scale: 1.0,
            maximum_globe_rigidity_error_cm: 0.0,
            maximum_rest_error_before_cm: 0.05,
            maximum_rest_error_after_cm: 0.01,
            maximum_requested_displacement_cm: 0.04,
            maximum_applied_displacement_cm: 0.04,
            mean_lower_strength: 0.7,
            mean_upper_inner_strength: 0.6,
        };
        let receipt = EyelidConformationReceipt {
            left: side(EyeSide::Left),
            right: side(EyeSide::Right),
            accepted_scale: 1.0,
            line_search_steps: 0,
            exact_noop: false,
            protected_vertex_count: 1_500,
            protected_vertices_changed: false,
            minimum_edge_ratio: 1.0,
            maximum_edge_ratio: 1.0,
            minimum_area_ratio: 1.0,
            maximum_area_ratio: 1.0,
            minimum_orientation_cosine: 1.0,
        };
        let (kept, skipped, skip_reason) = optional_eyelid_conformation(Ok(receipt.clone()));
        assert_eq!(kept, Some(receipt));
        assert!(!skipped);
        assert_eq!(skip_reason, None);
    }

    #[test]
    fn final_skin_repair_backtracks_only_selected_skin_vertices() {
        let canonical = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(4.0, 4.0, 4.0),
        ];
        let scan_fitted = canonical.clone();
        let protected_before = Vec3::new(5.0, 4.0, 4.0);
        let mut result = vec![
            canonical[0],
            canonical[1],
            Vec3::new(0.95, 0.001, 0.0),
            protected_before,
        ];
        let receipt = repair_final_skin_safety(
            &canonical,
            &scan_fitted,
            &mut result,
            &[[0, 1, 2]],
            &[0, 1, 2],
            &[],
            TopologyGuardOptions::canonical_g2(),
        )
        .unwrap();
        assert!(receipt.attempted);
        assert_eq!(receipt.initial_unsafe_triangles, 1);
        assert_eq!(receipt.final_unsafe_triangles, 0);
        assert!(receipt.adjusted_skin_vertices > 0);
        assert!(!receipt.protected_vertices_changed);
        assert_eq!(result[3], protected_before);
    }

    #[test]
    fn final_skin_repair_never_moves_a_skin_anatomy_overlap_vertex() {
        let canonical = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let scan_fitted = canonical.clone();
        let protected_before = Vec3::new(0.0, -1.0, 0.0);
        let mut result = vec![canonical[0], canonical[1], protected_before];
        let receipt = repair_final_skin_safety(
            &canonical,
            &scan_fitted,
            &mut result,
            &[[0, 1, 2]],
            &[0, 1, 2],
            &[2],
            TopologyGuardOptions::canonical_g2(),
        )
        .unwrap();
        assert!(receipt.attempted);
        assert_eq!(receipt.final_unsafe_triangles, 1);
        assert_eq!(receipt.adjusted_skin_vertices, 0);
        assert!(!receipt.protected_vertices_changed);
        assert_eq!(result[2], protected_before);
    }

    #[test]
    fn transition_collar_is_merged_into_final_repair_protection() {
        assert_eq!(
            merge_protected_vertices(vec![9, 3, 9, 1], &[4, 3, 8]),
            vec![1, 3, 4, 8, 9]
        );
    }

    #[test]
    fn canonical_skin_symmetry_mirrors_the_kept_half_at_full_amplitude() {
        let canonical = vec![
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(6.0, 0.0, 0.0),
            Vec3::new(5.0, 1.0, 0.0),
            Vec3::new(5.0, 0.0, 1.0),
        ];
        let triangles = vec![[0, 2, 3], [1, 3, 2]];

        let kept = Vec3::new(-0.02, 0.04, 0.01);
        let discarded = Vec3::new(0.10, 0.02, 0.01);
        let mut fitted = vec![
            canonical[0] + discarded,
            canonical[1] + kept,
            canonical[2] + Vec3::new(0.10, 0.01, 0.00),
            canonical[3] + Vec3::new(-0.10, 0.02, 0.00),
        ];

        symmetrize_canonical_skin_displacements(
            &canonical,
            &triangles,
            &mut fitted,
            SymmetryMode::PositiveX,
            TopologyGuardOptions::default(),
        )
        .unwrap();

        let right_delta = fitted[1] - canonical[1];
        assert!((right_delta - kept).norm() <= 1.0e-12, "{right_delta:?}");

        let mut other_run = vec![
            canonical[0] + Vec3::new(-0.40, 0.90, -0.30),
            canonical[1] + kept,
            fitted[2],
            fitted[3],
        ];
        symmetrize_canonical_skin_displacements(
            &canonical,
            &triangles,
            &mut other_run,
            SymmetryMode::PositiveX,
            TopologyGuardOptions::default(),
        )
        .unwrap();
        assert!(
            (other_run[1] - fitted[1]).norm() <= 1.0e-12
                && (other_run[0] - fitted[0]).norm() <= 1.0e-12,
            "the discarded half reached the result"
        );

        let left_delta = fitted[0] - canonical[0];
        assert!((left_delta.x + right_delta.x).abs() <= 1.0e-12);
        assert!((left_delta.y - right_delta.y).abs() <= 1.0e-12);
        assert!((left_delta.z - right_delta.z).abs() <= 1.0e-12);

        assert!((fitted[2].x - canonical[2].x).abs() <= 1.0e-12);
        assert!((fitted[3].x - canonical[3].x).abs() <= 1.0e-12);
        assert!((fitted[2].y - canonical[2].y - 0.01).abs() <= 1.0e-12);
    }

    #[test]
    fn the_other_side_is_kept_when_the_other_side_is_chosen() {
        let canonical = vec![
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(6.0, 0.0, 0.0),
            Vec3::new(5.0, 1.0, 0.0),
            Vec3::new(5.0, 0.0, 1.0),
        ];
        let triangles = vec![[0, 2, 3], [1, 3, 2]];
        let left = Vec3::new(0.10, 0.02, 0.01);
        let mut fitted = vec![
            canonical[0] + left,
            canonical[1] + Vec3::new(-0.02, 0.04, 0.01),
            canonical[2],
            canonical[3],
        ];

        symmetrize_canonical_skin_displacements(
            &canonical,
            &triangles,
            &mut fitted,
            SymmetryMode::NegativeX,
            TopologyGuardOptions::default(),
        )
        .unwrap();

        assert!((fitted[0] - canonical[0] - left).norm() <= 1.0e-12);
    }

    fn neck_grid(columns: &[f64], rows: usize) -> (Vec<Vec3>, Vec<[u32; 3]>, Vec<usize>) {
        let mut vertices = Vec::new();
        for row in 0..rows {
            for &x in columns {
                vertices.push(Vec3::new(x, row as f64, 0.0));
            }
        }
        let stride = columns.len();
        let mut triangles = Vec::new();
        for row in 0..rows - 1 {
            for column in 0..stride - 1 {
                let a = (row * stride + column) as u32;
                let b = (row * stride + column + 1) as u32;
                let c = ((row + 1) * stride + column) as u32;
                let d = ((row + 1) * stride + column + 1) as u32;
                triangles.push([a, b, c]);
                triangles.push([b, d, c]);
            }
        }
        let seam = (0..stride).collect();
        (vertices, triangles, seam)
    }

    #[test]
    fn neck_blend_weights_are_monotone_seam_pinned_and_symmetric() {
        let columns = [-1.5, -0.5, 0.5, 1.5];
        let rows = 13;
        let (vertices, triangles, seam) = neck_grid(&columns, rows);
        let weights = neck_blend_weights(&vertices, &triangles, &seam);
        let stride = columns.len();
        for column in 0..stride {
            let weight = |row: usize| weights[row * stride + column];
            assert_eq!(weight(0), 1.0, "seam ring keeps the authored geometry");
            assert_eq!(weight(1), 1.0, "the authored collar spans one centimetre");
            for row in 1..rows - 1 {
                assert!(
                    weight(row) >= weight(row + 1),
                    "weights must fall monotonically away from the seam"
                );
            }
            for row in 2..9 {
                assert!(
                    weight(row) > 0.0 && weight(row) < 1.0,
                    "row {row} must blend partially"
                );
            }
            assert_eq!(weight(9), 0.0, "pure fit from nine centimetres");
            assert_eq!(weight(12), 0.0);
        }

        for row in 0..rows {
            assert_eq!(weights[row * stride], weights[row * stride + 3]);
            assert_eq!(weights[row * stride + 1], weights[row * stride + 2]);
        }
    }

    #[test]
    fn neck_blend_restores_the_band_and_leaves_the_face_untouched() {
        let columns = [-1.5, -0.5, 0.5, 1.5];
        let rows = 13;
        let (rest, triangles, seam) = neck_grid(&columns, rows);

        let mut fitted = rest
            .iter()
            .map(|vertex| *vertex + Vec3::new(0.8, 0.0, 0.0))
            .collect::<Vec<_>>();
        let receipt = apply_canonical_neck_blend(
            &rest,
            &triangles,
            &seam,
            &mut fitted,
            TopologyGuardOptions::canonical_g2(),
        )
        .unwrap();
        let stride = columns.len();
        for column in 0..stride {
            assert_eq!(fitted[column], rest[column], "seam is bit-exact rest");
            let far = 12 * stride + column;
            assert_eq!(
                fitted[far],
                rest[far] + Vec3::new(0.8, 0.0, 0.0),
                "vertices past the falloff keep the pure fit"
            );
            let mid = 5 * stride + column;
            let offset = (fitted[mid] - rest[mid]).x;
            assert!(
                offset > 0.0 && offset < 0.8,
                "mid-band keeps a partial fit ({offset})"
            );
        }
        assert_eq!(receipt.guard_relaxed_vertices, 0);
        assert_eq!(receipt.minimum_weight, 0.0);
        assert_eq!(receipt.maximum_weight, 1.0);
        assert_eq!(
            receipt.falloff_start_cm,
            NECK_BLEND_FULL_RESTORE_GEODESIC_CM
        );
        assert_eq!(receipt.falloff_span_cm, NECK_BLEND_FALLOFF_SPAN_CM);
        assert!(receipt.band_vertex_count > receipt.full_weight_vertex_count);
        assert_eq!(receipt.full_weight_vertex_count, 2 * stride);
    }

    #[test]
    fn neck_blend_guard_fails_closed_when_a_blend_would_fold_the_band() {
        let columns = [-1.5, -0.5, 0.5, 1.5];
        let rows = 13;
        let (rest, triangles, seam) = neck_grid(&columns, rows);

        let mut fitted = rest
            .iter()
            .map(|vertex| *vertex + Vec3::new(0.0, -12.0, 0.0))
            .collect::<Vec<_>>();
        let rest_arrays = rest.iter().copied().map(Vec3::to_array).collect::<Vec<_>>();
        let receipt = apply_canonical_neck_blend(
            &rest,
            &triangles,
            &seam,
            &mut fitted,
            TopologyGuardOptions::canonical_g2(),
        )
        .unwrap();
        assert!(receipt.guard_relaxed_vertices > 0);
        let blended_arrays = fitted
            .iter()
            .copied()
            .map(Vec3::to_array)
            .collect::<Vec<_>>();
        let guard = TopologyGuardOptions::canonical_g2();
        let unsafe_after = crate::spatial::deformation_safety_mask(
            &rest_arrays,
            &blended_arrays,
            &triangles,
            guard.minimum_orientation_cosine,
            guard.minimum_area_ratio,
            guard.maximum_area_ratio,
        )
        .unwrap();
        let before_arrays = rest
            .iter()
            .map(|vertex| (*vertex + Vec3::new(0.0, -12.0, 0.0)).to_array())
            .collect::<Vec<_>>();
        let unsafe_before = crate::spatial::deformation_safety_mask(
            &rest_arrays,
            &before_arrays,
            &triangles,
            guard.minimum_orientation_cosine,
            guard.minimum_area_ratio,
            guard.maximum_area_ratio,
        )
        .unwrap();
        for (after, before) in unsafe_after.iter().zip(&unsafe_before) {
            assert!(!after || *before, "the blend may never introduce unsafety");
        }
    }
}
