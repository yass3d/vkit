use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use crate::formats::Mesh;
use crate::math::Vec3;
use crate::spatial::{
    GeometryKernelError, SurfaceProjector, SurfaceProjectorError, deformation_safety_mask,
};

use super::deformation_graph::{DeformationGraphSchedule, regularize_step};
use super::{
    BarycentricConstraint, LinearOperator, LsmrError, LsmrOptions, LsmrStop, OperatorError, lsmr,
};

pub const G2F_HEAD_SKIN_VERTEX_COUNT: usize = 3_841;
pub const G2F_HEAD_SKIN_TRIANGLE_COUNT: usize = 7_536;
pub const G2F_NECK_SEAM_VERTEX_COUNT: usize = 52;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DenseStageKind {
    Envelope,
    Coarse,
    Medium,
    Fine,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DenseRegistrationStage {
    pub kind: DenseStageKind,
    pub max_distance: f64,
    pub min_normal_dot: f64,
    pub trim_fraction: f64,
    pub huber_delta: f64,

    pub correspondence_weight: f64,
    pub smoothness: f64,
    pub point_to_point: f64,
    pub landmark_weight: f64,
    pub max_step: f64,
    pub iterations: usize,
}

pub const DEFAULT_DENSE_REGISTRATION_STAGES: [DenseRegistrationStage; 4] = [
    DenseRegistrationStage {
        kind: DenseStageKind::Envelope,
        max_distance: 4.50,
        min_normal_dot: -0.35,
        trim_fraction: 0.00,
        huber_delta: 1.20,
        correspondence_weight: 10.0,

        smoothness: 22.0,
        point_to_point: 0.30,
        landmark_weight: 180.0,
        max_step: 0.35,
        iterations: 9,
    },
    DenseRegistrationStage {
        kind: DenseStageKind::Coarse,
        max_distance: 3.00,
        min_normal_dot: -0.20,
        trim_fraction: 0.00,
        huber_delta: 1.00,
        correspondence_weight: 8.0,

        smoothness: 20.0,
        point_to_point: 0.12,
        landmark_weight: 160.0,
        max_step: 0.35,
        iterations: 8,
    },
    DenseRegistrationStage {
        kind: DenseStageKind::Medium,
        max_distance: 2.00,
        min_normal_dot: 0.00,
        trim_fraction: 0.01,
        huber_delta: 0.60,
        correspondence_weight: 6.0,
        smoothness: 14.0,
        point_to_point: 0.08,
        landmark_weight: 140.0,
        max_step: 0.18,
        iterations: 6,
    },
    DenseRegistrationStage {
        kind: DenseStageKind::Fine,
        max_distance: 1.00,
        min_normal_dot: 0.30,
        trim_fraction: 0.02,
        huber_delta: 0.30,
        correspondence_weight: 4.0,

        smoothness: 8.0,
        point_to_point: 0.04,
        landmark_weight: 140.0,
        max_step: 0.08,
        iterations: 6,
    },
];

pub const SCAN_FIDELITY_RANGE: std::ops::RangeInclusive<f64> = 0.4..=5.0;

pub const SCAN_FIDELITY_CLEAN_LIMIT: f64 = 2.0;

#[must_use]
pub fn dense_stages_at_fidelity(fidelity: f64) -> Vec<DenseRegistrationStage> {
    let fidelity = if fidelity.is_finite() {
        fidelity.clamp(*SCAN_FIDELITY_RANGE.start(), *SCAN_FIDELITY_RANGE.end())
    } else {
        1.0
    };
    DEFAULT_DENSE_REGISTRATION_STAGES
        .iter()
        .map(|stage| DenseRegistrationStage {
            smoothness: stage.smoothness / fidelity,

            correspondence_weight: stage.correspondence_weight * fidelity.sqrt(),
            ..*stage
        })
        .collect()
}

pub const PYTHON_PARITY_DENSE_REGISTRATION_STAGES: [DenseRegistrationStage; 3] = [
    DenseRegistrationStage {
        kind: DenseStageKind::Coarse,
        max_distance: 3.00,
        min_normal_dot: -0.20,
        trim_fraction: 0.00,
        huber_delta: 1.00,
        correspondence_weight: 8.0,
        smoothness: 120.0,
        point_to_point: 0.12,
        landmark_weight: 160.0,
        max_step: 0.35,
        iterations: 8,
    },
    DenseRegistrationStage {
        kind: DenseStageKind::Medium,
        max_distance: 2.00,
        min_normal_dot: 0.00,
        trim_fraction: 0.01,
        huber_delta: 0.60,
        correspondence_weight: 6.0,
        smoothness: 60.0,
        point_to_point: 0.08,
        landmark_weight: 140.0,
        max_step: 0.18,
        iterations: 6,
    },
    DenseRegistrationStage {
        kind: DenseStageKind::Fine,
        max_distance: 1.00,
        min_normal_dot: 0.30,
        trim_fraction: 0.02,
        huber_delta: 0.30,
        correspondence_weight: 4.0,
        smoothness: 30.0,
        point_to_point: 0.04,
        landmark_weight: 120.0,
        max_step: 0.08,
        iterations: 2,
    },
];

#[derive(Clone, Debug, PartialEq)]
pub struct DenseRegistrationOptions {
    pub stages: Vec<DenseRegistrationStage>,
    pub minimum_correspondence_coverage: f64,
    pub eligible_fade_margin: f64,
    pub minimum_orientation_cosine: f64,
    pub minimum_area_ratio: f64,
    pub maximum_area_ratio: f64,

    pub strict_landmark_count: usize,

    pub graph_regularization: bool,

    pub adaptive_stiffness: bool,
    pub lsmr: LsmrOptions,
}

impl Default for DenseRegistrationOptions {
    fn default() -> Self {
        Self {
            stages: DEFAULT_DENSE_REGISTRATION_STAGES.to_vec(),
            minimum_correspondence_coverage: 0.45,
            eligible_fade_margin: 0.25,
            minimum_orientation_cosine: 0.03,
            minimum_area_ratio: 0.08,
            maximum_area_ratio: 4.0,
            strict_landmark_count: usize::MAX,
            graph_regularization: true,
            adaptive_stiffness: true,
            lsmr: LsmrOptions {
                damping: 0.0,
                absolute_tolerance: 2.0e-6,
                relative_tolerance: 2.0e-6,
                condition_limit: 1.0e10,
                max_iterations: Some(350),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DenseIterationOutcome {
    Accepted,
    InsufficientCorrespondenceCoverage,
    LineSearchFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DenseRegistrationProgress {
    pub stage: DenseStageKind,
    pub stage_iteration: usize,
    pub completed_iterations: usize,
    pub total_iterations: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DenseIterationReceipt {
    pub stage: DenseStageKind,
    pub iteration: usize,
    pub outcome: DenseIterationOutcome,
    pub correspondences: usize,
    pub coverage: f64,
    pub median_before: Option<f64>,
    pub p95_before: Option<f64>,
    pub point_plane_rms_before: Option<f64>,
    pub landmark_rms_before: Option<f64>,
    pub strict_user_landmark_rms_before: Option<f64>,
    pub automatic_landmark_rms_before: Option<f64>,
    pub exterior_lower_p95_before: Option<f64>,
    pub linear_energy_before: Option<f64>,
    pub raw_max_step: Option<f64>,

    pub graph_blend_lambda: Option<f64>,
    pub topology_guard_vertex_count: usize,
    pub line_search_tau: Option<f64>,
    pub lsmr_stop: Option<LsmrStop>,
    pub lsmr_iterations: Option<usize>,
    pub lsmr_condition: Option<f64>,
    pub median_after: Option<f64>,
    pub p95_after: Option<f64>,
    pub point_plane_rms_after: Option<f64>,
    pub landmark_rms_after: Option<f64>,
    pub strict_user_landmark_rms_after: Option<f64>,
    pub automatic_landmark_rms_after: Option<f64>,
    pub exterior_lower_p95_after: Option<f64>,
    pub linear_energy_after: Option<f64>,
    pub score_improvement: Option<f64>,
}

impl DenseIterationReceipt {
    fn insufficient(
        stage: DenseStageKind,
        iteration: usize,
        correspondences: usize,
        coverage: f64,
    ) -> Self {
        Self {
            stage,
            iteration,
            outcome: DenseIterationOutcome::InsufficientCorrespondenceCoverage,
            correspondences,
            coverage,
            median_before: None,
            p95_before: None,
            point_plane_rms_before: None,
            landmark_rms_before: None,
            strict_user_landmark_rms_before: None,
            automatic_landmark_rms_before: None,
            exterior_lower_p95_before: None,
            linear_energy_before: None,
            raw_max_step: None,
            graph_blend_lambda: None,
            topology_guard_vertex_count: 0,
            line_search_tau: None,
            lsmr_stop: None,
            lsmr_iterations: None,
            lsmr_condition: None,
            median_after: None,
            p95_after: None,
            point_plane_rms_after: None,
            landmark_rms_after: None,
            strict_user_landmark_rms_after: None,
            automatic_landmark_rms_after: None,
            exterior_lower_p95_after: None,
            linear_energy_after: None,
            score_improvement: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DenseRegistrationReport {
    pub eligible_vertex_count: usize,
    pub target_component_vertex_count: usize,
    pub target_component_triangle_count: usize,

    pub strict_user_landmark_count: usize,

    pub automatic_landmark_count: usize,

    pub automatic_landmark_guard_fallback: bool,
    pub landmark_guard_limit_cm: f64,
    pub scheduled_iterations: usize,
    pub attempted_iterations: usize,
    pub accepted_iterations: usize,

    pub lsmr_iteration_limit_terminations: usize,
    pub landmark_rms: f64,
    pub landmark_weighted_rms: f64,
    pub iterations: Vec<DenseIterationReceipt>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DenseRegistrationResult {
    pub vertices: Vec<Vec3>,
    pub report: DenseRegistrationReport,
}

#[derive(Debug, Error)]
pub enum DenseRegistrationError {
    #[error("dense registration requires a non-empty skin mesh")]
    EmptySkin,
    #[error("initial and canonical skin vertex counts differ ({initial} != {canonical})")]
    VertexCountMismatch { initial: usize, canonical: usize },
    #[error("anchor weight count {actual} does not match skin vertex count {expected}")]
    AnchorCountMismatch { expected: usize, actual: usize },
    #[error("dense registration requires at least one landmark constraint")]
    NoLandmarkConstraints,
    #[error("dense registration has no vertices above the neck fade")]
    NoEligibleVertices,
    #[error(
        "triangle {triangle} references vertex {vertex}, but the skin has {vertex_count} vertices"
    )]
    TriangleVertexOutOfBounds {
        triangle: usize,
        vertex: usize,
        vertex_count: usize,
    },
    #[error("seam vertex {0} is outside the skin vertex array")]
    SeamVertexOutOfBounds(usize),
    #[error(
        "landmark constraint {constraint} references vertex {vertex}, but the skin has {vertex_count} vertices"
    )]
    ConstraintVertexOutOfBounds {
        constraint: usize,
        vertex: usize,
        vertex_count: usize,
    },
    #[error("dense registration option {0} is invalid")]
    InvalidOption(&'static str),
    #[error("dense registration was cancelled")]
    Cancelled,
    #[error(transparent)]
    Surface(#[from] SurfaceProjectorError),
    #[error(transparent)]
    Geometry(#[from] GeometryKernelError),
    #[error(transparent)]
    Operator(#[from] OperatorError),
    #[error(transparent)]
    Lsmr(#[from] LsmrError),
}

#[derive(Clone, Copy)]
struct Projection {
    point: Vec3,
    normal: Vec3,
    distance: f64,
    primitive_id: u32,
    barycentric: [f64; 3],
}

#[derive(Clone, Copy)]
struct CorrespondenceRow {
    vertex: usize,
    normal: Vec3,
    plane_scale: f64,
    point_scale: f64,
}

const SUPPORTED_STIFFNESS: f64 = 0.22;

const SUPPORT_SMOOTHING_PASSES: usize = 3;

fn laplacian_scales(
    vertex_count: usize,
    smoothness: f64,
    scan_support: &[f64],
    strict_constraints: &[BarycentricConstraint],
    neighbors: &[Vec<usize>],
) -> Vec<f64> {
    let blur = |seed: Vec<f64>| {
        let mut field = seed;
        let mut scratch = field.clone();
        for _ in 0..SUPPORT_SMOOTHING_PASSES {
            for (index, value) in scratch.iter_mut().enumerate() {
                let ring = &neighbors[index];
                *value = if ring.is_empty() {
                    field[index]
                } else {
                    let sum: f64 = ring.iter().map(|&other| field[other]).sum();

                    0.5f64.mul_add(field[index], 0.5 * sum / ring.len() as f64)
                };
            }
            field.copy_from_slice(&scratch);
        }
        field
    };

    let mut pin_seed = vec![0.0_f64; vertex_count];
    for constraint in strict_constraints {
        for vertex in constraint.vertex_indices {
            if let Some(value) = pin_seed.get_mut(vertex) {
                *value = 1.0;
            }
        }
    }
    let scan = blur(scan_support.to_vec());
    let pins = blur(pin_seed);

    scan.into_iter()
        .zip(pins)
        .map(|(scan, pin)| {
            let carry = 1.0 - pin.clamp(0.0, 1.0);
            let relief = 1.0 - (1.0 - SUPPORTED_STIFFNESS) * scan.clamp(0.0, 1.0) * carry;
            (smoothness * relief).max(0.0).sqrt()
        })
        .collect()
}

struct DenseSystem<'a> {
    vertex_count: usize,
    correspondences: Vec<CorrespondenceRow>,
    neighbors: &'a [Vec<usize>],
    neighbor_weights: &'a [Vec<f64>],
    laplacian_scales: Vec<f64>,
    constraints: &'a [BarycentricConstraint],
    landmark_scales: Vec<f64>,
    anchor_indices: Vec<usize>,
    anchor_scales: Vec<f64>,
    rows: usize,
}

struct DenseSystemSpec<'a> {
    vertex_count: usize,
    correspondence_indices: &'a [usize],
    target_normals: &'a [Vec3],
    data_weights: &'a [f64],
    point_to_point: f64,
    neighbors: &'a [Vec<usize>],

    neighbor_weights: &'a [Vec<f64>],
    smoothness: f64,
    adaptive_stiffness: bool,

    scan_support: &'a [f64],

    strict_constraints: &'a [BarycentricConstraint],
    constraints: &'a [BarycentricConstraint],
    landmark_weight: f64,
    anchor_weights: &'a [f64],
}

impl<'a> DenseSystem<'a> {
    fn new(spec: DenseSystemSpec<'a>) -> Self {
        debug_assert_eq!(spec.neighbor_weights.len(), spec.vertex_count);
        let correspondences = spec
            .correspondence_indices
            .iter()
            .copied()
            .zip(spec.target_normals.iter().copied())
            .zip(spec.data_weights.iter().copied())
            .map(|((vertex, normal), weight)| CorrespondenceRow {
                vertex,
                normal,
                plane_scale: weight.sqrt(),
                point_scale: (weight * spec.point_to_point).sqrt(),
            })
            .collect::<Vec<_>>();
        let landmark_scales = spec
            .constraints
            .iter()
            .map(|constraint| (constraint.effective_weight * spec.landmark_weight).sqrt())
            .collect::<Vec<_>>();
        let mut anchor_indices = Vec::new();
        let mut anchor_scales = Vec::new();
        for (index, weight) in spec.anchor_weights.iter().copied().enumerate() {
            if weight > 0.0 {
                anchor_indices.push(index);
                anchor_scales.push(weight.sqrt());
            }
        }
        let rows = correspondences.len()
            + correspondences.len() * 3
            + spec.vertex_count * 3
            + spec.constraints.len() * 3
            + anchor_indices.len() * 3;
        let laplacian_scales = if spec.adaptive_stiffness {
            laplacian_scales(
                spec.vertex_count,
                spec.smoothness,
                spec.scan_support,
                spec.strict_constraints,
                spec.neighbors,
            )
        } else {
            vec![spec.smoothness.max(0.0).sqrt(); spec.vertex_count]
        };
        Self {
            vertex_count: spec.vertex_count,
            correspondences,
            neighbors: spec.neighbors,
            neighbor_weights: spec.neighbor_weights,
            laplacian_scales,
            constraints: spec.constraints,
            landmark_scales,
            anchor_indices,
            anchor_scales,
            rows,
        }
    }

    fn right_hand_side(
        &self,
        current: &[Vec3],
        closest: &[Vec3],
        landmark_targets: &[Vec3],
    ) -> Vec<f64> {
        let mut right_hand = Vec::with_capacity(self.rows);
        for row in &self.correspondences {
            let displacement = closest[row.vertex] - current[row.vertex];
            right_hand.push(row.normal.dot(displacement) * row.plane_scale);
        }
        for row in &self.correspondences {
            let displacement = closest[row.vertex] - current[row.vertex];
            right_hand.extend_from_slice(&[
                displacement.x * row.point_scale,
                displacement.y * row.point_scale,
                displacement.z * row.point_scale,
            ]);
        }
        right_hand.resize(right_hand.len() + self.vertex_count * 3, 0.0);
        for ((constraint, target), scale) in self
            .constraints
            .iter()
            .zip(landmark_targets)
            .zip(self.landmark_scales.iter().copied())
        {
            let displacement = *target - interpolate(*constraint, current);
            right_hand.extend_from_slice(&[
                displacement.x * scale,
                displacement.y * scale,
                displacement.z * scale,
            ]);
        }
        right_hand.resize(right_hand.len() + self.anchor_indices.len() * 3, 0.0);
        debug_assert_eq!(right_hand.len(), self.rows);
        right_hand
    }
}

impl LinearOperator for DenseSystem<'_> {
    fn rows(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.vertex_count * 3
    }

    fn apply(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        validate_operator_vectors(self, input, output, "dense registration")?;
        let mut row_offset = 0;
        for row in &self.correspondences {
            let offset = row.vertex * 3;
            output[row_offset] = row.plane_scale
                * (row.normal.x * input[offset]
                    + row.normal.y * input[offset + 1]
                    + row.normal.z * input[offset + 2]);
            row_offset += 1;
        }
        for row in &self.correspondences {
            let offset = row.vertex * 3;
            for coordinate in 0..3 {
                output[row_offset] = row.point_scale * input[offset + coordinate];
                row_offset += 1;
            }
        }
        for vertex in 0..self.vertex_count {
            let mut value = [0.0; 3];
            for (&neighbor, &weight) in self.neighbors[vertex]
                .iter()
                .zip(&self.neighbor_weights[vertex])
            {
                let neighbor_offset = neighbor * 3;
                value[0] -= weight * input[neighbor_offset];
                value[1] -= weight * input[neighbor_offset + 1];
                value[2] -= weight * input[neighbor_offset + 2];
            }
            let vertex_offset = vertex * 3;
            value[0] += input[vertex_offset];
            value[1] += input[vertex_offset + 1];
            value[2] += input[vertex_offset + 2];
            let scale = self.laplacian_scales[vertex];
            output[row_offset] = scale * value[0];
            output[row_offset + 1] = scale * value[1];
            output[row_offset + 2] = scale * value[2];
            row_offset += 3;
        }
        for (constraint_index, constraint) in self.constraints.iter().copied().enumerate() {
            let scale = self.landmark_scales[constraint_index];
            for coordinate in 0..3 {
                let mut value = 0.0;
                for corner in 0..3 {
                    value += constraint.barycentric[corner]
                        * input[constraint.vertex_indices[corner] * 3 + coordinate];
                }
                output[row_offset] = scale * value;
                row_offset += 1;
            }
        }
        for (&vertex, &scale) in self.anchor_indices.iter().zip(&self.anchor_scales) {
            for coordinate in 0..3 {
                output[row_offset] = scale * input[vertex * 3 + coordinate];
                row_offset += 1;
            }
        }
        debug_assert_eq!(row_offset, output.len());
        Ok(())
    }

    fn apply_transpose(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        validate_transpose_vectors(self, input, output, "dense registration transpose")?;
        output.fill(0.0);
        self.apply_transpose_add(input, output)
    }

    fn apply_transpose_add(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        validate_transpose_vectors(self, input, output, "dense registration accumulation")?;
        let mut row_offset = 0;
        for row in &self.correspondences {
            let value = row.plane_scale * input[row_offset];
            let offset = row.vertex * 3;
            output[offset] += row.normal.x * value;
            output[offset + 1] += row.normal.y * value;
            output[offset + 2] += row.normal.z * value;
            row_offset += 1;
        }
        for row in &self.correspondences {
            let offset = row.vertex * 3;
            for coordinate in 0..3 {
                output[offset + coordinate] += row.point_scale * input[row_offset];
                row_offset += 1;
            }
        }
        for vertex in 0..self.vertex_count {
            let scaled = [
                self.laplacian_scales[vertex] * input[row_offset],
                self.laplacian_scales[vertex] * input[row_offset + 1],
                self.laplacian_scales[vertex] * input[row_offset + 2],
            ];
            for (&neighbor, &weight) in self.neighbors[vertex]
                .iter()
                .zip(&self.neighbor_weights[vertex])
            {
                let neighbor_offset = neighbor * 3;
                output[neighbor_offset] -= weight * scaled[0];
                output[neighbor_offset + 1] -= weight * scaled[1];
                output[neighbor_offset + 2] -= weight * scaled[2];
            }
            let vertex_offset = vertex * 3;
            output[vertex_offset] += scaled[0];
            output[vertex_offset + 1] += scaled[1];
            output[vertex_offset + 2] += scaled[2];
            row_offset += 3;
        }
        for (constraint_index, constraint) in self.constraints.iter().copied().enumerate() {
            let scale = self.landmark_scales[constraint_index];
            for coordinate in 0..3 {
                let scaled = scale * input[row_offset];
                for corner in 0..3 {
                    output[constraint.vertex_indices[corner] * 3 + coordinate] +=
                        constraint.barycentric[corner] * scaled;
                }
                row_offset += 1;
            }
        }
        for (&vertex, &scale) in self.anchor_indices.iter().zip(&self.anchor_scales) {
            for coordinate in 0..3 {
                output[vertex * 3 + coordinate] += scale * input[row_offset];
                row_offset += 1;
            }
        }
        debug_assert_eq!(row_offset, input.len());
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
pub struct SkinRegistrationInputs<'a> {
    pub initial_vertices: &'a [Vec3],
    pub base_vertices: &'a [Vec3],
    pub triangles: &'a [[u32; 3]],
    pub constraints: &'a [BarycentricConstraint],
    pub seam: &'a [usize],
    pub anchor_weights: &'a [f64],

    pub fade_end: f64,
}

pub fn nonrigid_skin_registration<P, C>(
    inputs: SkinRegistrationInputs<'_>,
    target_mesh: &Mesh,
    options: &DenseRegistrationOptions,
    mut progress: P,
    mut is_cancelled: C,
) -> Result<DenseRegistrationResult, DenseRegistrationError>
where
    P: FnMut(DenseRegistrationProgress),
    C: FnMut() -> bool,
{
    let SkinRegistrationInputs {
        initial_vertices,
        base_vertices,
        triangles,
        constraints,
        seam,
        anchor_weights,
        fade_end,
    } = inputs;
    validate_inputs(inputs, options)?;
    target_mesh.require_surface().map_err(|_| {
        DenseRegistrationError::InvalidOption("target mesh must be a finite triangle surface")
    })?;

    let scan_connectivity = analyze_scan_connectivity(target_mesh);
    let target_triangle_ids = scan_connectivity.largest_component.clone();
    let target_component_vertex_count = target_triangle_ids
        .iter()
        .flat_map(|&triangle_id| target_mesh.triangles[triangle_id as usize])
        .collect::<BTreeSet<_>>()
        .len();
    let target_vertices = target_mesh
        .vertices
        .iter()
        .map(|vertex| vertex.map(|coordinate| coordinate as f32 as f64))
        .collect::<Vec<_>>();
    let target_projector = SurfaceProjector::new(
        &target_vertices,
        &target_mesh.triangles,
        Some(&target_triangle_ids),
        0.0,
    )?;
    let neighbors = build_neighbors(base_vertices.len(), triangles);

    let neighbor_weights: Vec<Vec<f64>> = if options.adaptive_stiffness {
        build_cotangent_weights(base_vertices, triangles, &neighbors)
    } else {
        build_inverse_degrees(&neighbors)
            .into_iter()
            .zip(&neighbors)
            .map(|(uniform, ring)| vec![uniform; ring.len()])
            .collect()
    };
    let areas = vertex_areas(base_vertices, triangles);
    let eligible = base_vertices
        .iter()
        .map(|vertex| vertex.y >= fade_end - options.eligible_fade_margin)
        .collect::<Vec<_>>();
    let eligible_count = eligible.iter().filter(|value| **value).count();
    if eligible_count == 0 {
        return Err(DenseRegistrationError::NoEligibleVertices);
    }
    let landmark_targets = constraints
        .iter()
        .map(|constraint| constraint.target)
        .collect::<Vec<_>>();
    let strict_landmark_count = options.strict_landmark_count.min(constraints.len());
    let (strict_constraints, automatic_constraints) = constraints.split_at(strict_landmark_count);

    let (landmark_guard_constraints, automatic_landmark_guard_fallback) =
        if strict_constraints.is_empty() {
            (automatic_constraints, true)
        } else {
            (strict_constraints, false)
        };
    let exterior_lower = exterior_lower_mask(base_vertices, &eligible);
    let scheduled_iterations = options.stages.iter().map(|stage| stage.iterations).sum();

    let mut scan_support = vec![0.0_f64; base_vertices.len()];
    let mut completed_iterations = 0;
    let mut current = initial_vertices.to_vec();

    let initial_landmark_guard_rms = weighted_landmark_rms(&current, landmark_guard_constraints);
    let landmark_drift_limit = (initial_landmark_guard_rms * 1.02).max(0.005);
    if !landmark_drift_limit.is_finite() {
        return Err(DenseRegistrationError::InvalidOption(
            "finite landmark drift guard",
        ));
    }
    let mut logs = Vec::new();

    for stage in &options.stages {
        let mut stale_iterations = 0;
        for iteration in 0..stage.iterations {
            if is_cancelled() {
                return Err(DenseRegistrationError::Cancelled);
            }
            progress(DenseRegistrationProgress {
                stage: stage.kind,
                stage_iteration: iteration,
                completed_iterations,
                total_iterations: scheduled_iterations,
            });
            let projections = project_all(&target_projector, &current)?;
            let closest = projections
                .iter()
                .map(|projection| projection.point)
                .collect::<Vec<_>>();
            let distances = projections
                .iter()
                .map(|projection| projection.distance)
                .collect::<Vec<_>>();
            let touches_boundary = projections
                .iter()
                .map(|projection| {
                    projection_touches_boundary(
                        projection,
                        &target_mesh.triangles,
                        &scan_connectivity,
                    )
                })
                .collect::<Vec<_>>();

            let mut target_normals = projections
                .iter()
                .map(|projection| {
                    if scan_connectivity.winding_consistent
                        && scan_connectivity.triangle_flipped[projection.primitive_id as usize]
                    {
                        projection.normal * -1.0
                    } else {
                        projection.normal
                    }
                })
                .collect::<Vec<_>>();
            let source_normals = vertex_normals(&current, triangles);
            let mut normal_dot = source_normals
                .iter()
                .zip(&target_normals)
                .map(|(source, target)| source.dot(*target))
                .collect::<Vec<_>>();
            let eligible_normal_dot = normal_dot
                .iter()
                .copied()
                .zip(&eligible)
                .filter_map(|(value, selected)| selected.then_some(value))
                .collect::<Vec<_>>();
            if scan_connectivity.winding_consistent
                && quantile(&eligible_normal_dot, 0.5).unwrap_or(0.0) < 0.0
            {
                for normal in &mut target_normals {
                    *normal = *normal * -1.0;
                }
                for value in &mut normal_dot {
                    *value *= -1.0;
                }
            }
            let mut valid_indices = (0..current.len())
                .filter(|&index| {
                    let oriented_dot = if scan_connectivity.winding_consistent {
                        normal_dot[index]
                    } else {
                        normal_dot[index].abs()
                    };
                    eligible[index]
                        && distances[index] <= stage.max_distance
                        && oriented_dot >= stage.min_normal_dot
                        && !touches_boundary[index]
                })
                .collect::<Vec<_>>();
            let pretrim_coverage = valid_indices.len() as f64 / eligible_count as f64;
            if valid_indices.is_empty()
                || pretrim_coverage < options.minimum_correspondence_coverage
            {
                logs.push(DenseIterationReceipt::insufficient(
                    stage.kind,
                    iteration,
                    valid_indices.len(),
                    pretrim_coverage,
                ));
                completed_iterations += 1;
                break;
            }
            let valid_distances = valid_indices
                .iter()
                .map(|&index| distances[index])
                .collect::<Vec<_>>();
            let trim_cutoff = quantile(&valid_distances, 1.0 - stage.trim_fraction)
                .expect("valid correspondence distances are non-empty");
            valid_indices.retain(|&index| distances[index] <= trim_cutoff);
            for &index in &valid_indices {
                if let Some(value) = scan_support.get_mut(index) {
                    *value = 1.0;
                }
            }
            let selected_normals = valid_indices
                .iter()
                .map(|&index| target_normals[index])
                .collect::<Vec<_>>();
            let plane_residual = valid_indices
                .iter()
                .map(|&index| target_normals[index].dot(closest[index] - current[index]))
                .collect::<Vec<_>>();
            let data_weights = valid_indices
                .iter()
                .zip(&plane_residual)
                .map(|(&index, &residual)| {
                    let absolute = residual.abs();
                    let robust = if absolute > stage.huber_delta {
                        stage.huber_delta / absolute.max(1.0e-12)
                    } else {
                        1.0
                    };
                    (areas[index] * robust * stage.correspondence_weight).max(1.0e-8)
                })
                .collect::<Vec<_>>();
            let system = DenseSystem::new(DenseSystemSpec {
                vertex_count: current.len(),
                correspondence_indices: &valid_indices,
                target_normals: &selected_normals,
                data_weights: &data_weights,
                point_to_point: stage.point_to_point,
                neighbors: &neighbors,
                neighbor_weights: &neighbor_weights,
                smoothness: stage.smoothness,
                adaptive_stiffness: options.adaptive_stiffness,
                scan_support: &scan_support,
                strict_constraints,
                constraints,
                landmark_weight: stage.landmark_weight,
                anchor_weights,
            });
            let right_hand = system.right_hand_side(&current, &closest, &landmark_targets);
            let before_linear_energy = squared_norm(&right_hand);
            let solve = lsmr(&system, &right_hand, options.lsmr)?;
            let raw_delta = solve
                .solution
                .chunks_exact(3)
                .map(|value| Vec3::new(value[0], value[1], value[2]))
                .collect::<Vec<_>>();

            let (mut delta, graph_blend_lambda) = if options.graph_regularization {
                let projected_delta = regularize_step(
                    &current,
                    &raw_delta,
                    &eligible,
                    seam,
                    anchor_weights,
                    strict_constraints,
                    deformation_graph_schedule(stage.kind),
                );
                let (delta, lambda) = energy_aware_graph_blend(
                    &system,
                    &right_hand,
                    before_linear_energy,
                    &raw_delta,
                    &projected_delta,
                )?;
                (delta, Some(lambda))
            } else {
                (raw_delta, None)
            };
            for &vertex in seam {
                delta[vertex] = Vec3::ZERO;
            }
            let mut guarded_vertices = BTreeSet::new();
            for _ in 0..8 {
                let maximum = maximum_norm(&delta);
                let scale = if maximum > 0.0 {
                    (stage.max_step / maximum).min(1.0)
                } else {
                    1.0
                };
                let guarded_trial =
                    candidate_vertices(&current, &delta, scale, seam, base_vertices);
                let unsafe_mask = safety_mask(base_vertices, &guarded_trial, triangles, options)?;
                if unsafe_mask.iter().all(|unsafe_triangle| !unsafe_triangle) {
                    break;
                }
                let mut newly_guarded = BTreeSet::new();
                for (triangle, unsafe_triangle) in triangles.iter().zip(unsafe_mask) {
                    if unsafe_triangle {
                        for &vertex in triangle {
                            let vertex = vertex as usize;
                            if !guarded_vertices.contains(&vertex) {
                                newly_guarded.insert(vertex);
                            }
                        }
                    }
                }
                if newly_guarded.is_empty() {
                    break;
                }
                for vertex in newly_guarded {
                    guarded_vertices.insert(vertex);
                    delta[vertex] = Vec3::ZERO;
                }
            }
            let raw_max_step = maximum_norm(&delta);
            let scale = if raw_max_step > 0.0 {
                (stage.max_step / raw_max_step).min(1.0)
            } else {
                1.0
            };
            let before_distances = distances
                .iter()
                .copied()
                .zip(&eligible)
                .filter_map(|(distance, selected)| selected.then_some(distance))
                .collect::<Vec<_>>();
            let before_median = quantile(&before_distances, 0.5)
                .expect("at least one eligible vertex is guaranteed above");
            let before_p95 = quantile(&before_distances, 0.95)
                .expect("at least one eligible vertex is guaranteed above");
            let before_exterior_lower_p95 = quantile(
                &distances
                    .iter()
                    .copied()
                    .zip(&exterior_lower)
                    .filter_map(|(distance, selected)| selected.then_some(distance))
                    .collect::<Vec<_>>(),
                0.95,
            )
            .expect("the exterior-lower mask falls back to the eligible mask");
            let before_score = before_median + 0.5 * before_p95;
            let before_plane_rms = rms(&plane_residual);
            let before_landmark_rms = weighted_landmark_rms(&current, constraints);
            let before_strict_landmark_rms = weighted_landmark_rms(&current, strict_constraints);
            let before_automatic_landmark_rms =
                weighted_landmark_rms(&current, automatic_constraints);
            let mut tau = scale;
            let minimum_tau = scale / 32.0;
            let mut accepted_candidate = None;
            while tau >= minimum_tau {
                if is_cancelled() {
                    return Err(DenseRegistrationError::Cancelled);
                }
                let candidate = candidate_vertices(&current, &delta, tau, seam, base_vertices);
                let unsafe_mask = safety_mask(base_vertices, &candidate, triangles, options)?;
                if unsafe_mask.iter().any(|unsafe_triangle| *unsafe_triangle) {
                    tau *= 0.5;
                    continue;
                }

                let candidate_strict_landmark_rms =
                    weighted_landmark_rms(&candidate, strict_constraints);
                let candidate_landmark_guard_rms =
                    weighted_landmark_rms(&candidate, landmark_guard_constraints);
                if candidate_landmark_guard_rms > landmark_drift_limit {
                    tau *= 0.5;
                    continue;
                }
                let candidate_automatic_landmark_rms =
                    weighted_landmark_rms(&candidate, automatic_constraints);
                let candidate_projections = project_all(&target_projector, &candidate)?;
                let candidate_distances = candidate_projections
                    .iter()
                    .map(|projection| projection.distance)
                    .zip(&eligible)
                    .filter_map(|(distance, selected)| selected.then_some(distance))
                    .collect::<Vec<_>>();
                let candidate_median = quantile(&candidate_distances, 0.5)
                    .expect("at least one eligible vertex is guaranteed above");
                let candidate_p95 = quantile(&candidate_distances, 0.95)
                    .expect("at least one eligible vertex is guaranteed above");
                let candidate_exterior_lower_p95 = quantile(
                    &candidate_projections
                        .iter()
                        .map(|projection| projection.distance)
                        .zip(&exterior_lower)
                        .filter_map(|(distance, selected)| selected.then_some(distance))
                        .collect::<Vec<_>>(),
                    0.95,
                )
                .expect("the exterior-lower mask falls back to the eligible mask");
                let candidate_score = candidate_median + 0.5 * candidate_p95;
                let fixed_candidate_plane = valid_indices
                    .iter()
                    .map(|&index| target_normals[index].dot(closest[index] - candidate[index]))
                    .collect::<Vec<_>>();
                let candidate_plane_rms = rms(&fixed_candidate_plane);
                let candidate_landmark_rms = weighted_landmark_rms(&candidate, constraints);
                let scaled_delta = flatten_scaled(&delta, tau);
                let mut linear_residual = vec![0.0; system.rows()];
                system.apply(&scaled_delta, &mut linear_residual)?;
                for (residual, rhs) in linear_residual.iter_mut().zip(&right_hand) {
                    *residual -= rhs;
                }
                let candidate_linear_energy = squared_norm(&linear_residual);
                let landmark_guard = candidate_landmark_guard_rms <= landmark_drift_limit;
                let metric_descent = candidate_score <= before_score * 1.002 && landmark_guard;
                let energy_descent = candidate_linear_energy
                    < before_linear_energy * ENERGY_DESCENT_MARGIN
                    && landmark_guard
                    && candidate_score <= before_score * 1.02;

                let exterior_lower_descent = candidate_exterior_lower_p95
                    <= before_exterior_lower_p95 * 0.995
                    && candidate_score <= before_score * 1.02
                    && landmark_guard;
                if metric_descent || energy_descent || exterior_lower_descent {
                    accepted_candidate = Some((
                        candidate,
                        candidate_median,
                        candidate_p95,
                        candidate_plane_rms,
                        candidate_landmark_rms,
                        candidate_strict_landmark_rms,
                        candidate_automatic_landmark_rms,
                        candidate_exterior_lower_p95,
                        candidate_linear_energy,
                        candidate_score,
                    ));
                    break;
                }
                tau *= 0.5;
            }
            completed_iterations += 1;
            let coverage = valid_indices.len() as f64 / eligible_count as f64;
            let base_receipt = DenseIterationReceipt {
                stage: stage.kind,
                iteration,
                outcome: DenseIterationOutcome::LineSearchFailed,
                correspondences: valid_indices.len(),
                coverage,
                median_before: Some(before_median),
                p95_before: Some(before_p95),
                point_plane_rms_before: Some(before_plane_rms),
                landmark_rms_before: Some(before_landmark_rms),
                strict_user_landmark_rms_before: Some(before_strict_landmark_rms),
                automatic_landmark_rms_before: Some(before_automatic_landmark_rms),
                exterior_lower_p95_before: Some(before_exterior_lower_p95),
                linear_energy_before: Some(before_linear_energy),
                raw_max_step: Some(raw_max_step),
                graph_blend_lambda,
                topology_guard_vertex_count: guarded_vertices.len(),
                line_search_tau: Some(tau),
                lsmr_stop: Some(solve.stop),
                lsmr_iterations: Some(solve.iterations),
                lsmr_condition: Some(solve.condition_estimate),
                median_after: None,
                p95_after: None,
                point_plane_rms_after: None,
                landmark_rms_after: None,
                strict_user_landmark_rms_after: None,
                automatic_landmark_rms_after: None,
                exterior_lower_p95_after: None,
                linear_energy_after: None,
                score_improvement: None,
            };
            let Some((
                candidate,
                candidate_median,
                candidate_p95,
                candidate_plane_rms,
                candidate_landmark_rms,
                candidate_strict_landmark_rms,
                candidate_automatic_landmark_rms,
                candidate_exterior_lower_p95,
                candidate_linear_energy,
                candidate_score,
            )) = accepted_candidate
            else {
                logs.push(base_receipt);
                break;
            };
            let improvement = before_score - candidate_score;
            logs.push(DenseIterationReceipt {
                outcome: DenseIterationOutcome::Accepted,
                median_after: Some(candidate_median),
                p95_after: Some(candidate_p95),
                point_plane_rms_after: Some(candidate_plane_rms),
                landmark_rms_after: Some(candidate_landmark_rms),
                strict_user_landmark_rms_after: Some(candidate_strict_landmark_rms),
                automatic_landmark_rms_after: Some(candidate_automatic_landmark_rms),
                exterior_lower_p95_after: Some(candidate_exterior_lower_p95),
                linear_energy_after: Some(candidate_linear_energy),
                score_improvement: Some(improvement),
                ..base_receipt
            });
            current = candidate;
            if improvement < before_score * 0.001 || tau * raw_max_step < 1.0e-4 {
                stale_iterations += 1;
            } else {
                stale_iterations = 0;
            }
            if stale_iterations >= 2 {
                break;
            }
        }
    }
    for &vertex in seam {
        current[vertex] = base_vertices[vertex];
    }
    let residuals = landmark_residuals(&current, constraints);
    let total_weight = constraints
        .iter()
        .map(|constraint| constraint.effective_weight)
        .sum::<f64>();
    let landmark_weighted_rms = (constraints
        .iter()
        .zip(&residuals)
        .map(|(constraint, residual)| constraint.effective_weight * residual * residual)
        .sum::<f64>()
        / total_weight)
        .sqrt();
    progress(DenseRegistrationProgress {
        stage: options
            .stages
            .last()
            .map_or(DenseStageKind::Fine, |stage| stage.kind),
        stage_iteration: options.stages.last().map_or(0, |stage| stage.iterations),
        completed_iterations,
        total_iterations: scheduled_iterations,
    });
    Ok(DenseRegistrationResult {
        vertices: current,
        report: DenseRegistrationReport {
            eligible_vertex_count: eligible_count,
            target_component_vertex_count,
            target_component_triangle_count: target_triangle_ids.len(),
            strict_user_landmark_count: strict_constraints.len(),
            automatic_landmark_count: automatic_constraints.len(),
            automatic_landmark_guard_fallback,
            landmark_guard_limit_cm: landmark_drift_limit,
            scheduled_iterations,
            attempted_iterations: logs.len(),
            accepted_iterations: logs
                .iter()
                .filter(|log| log.outcome == DenseIterationOutcome::Accepted)
                .count(),
            lsmr_iteration_limit_terminations: logs
                .iter()
                .filter(|log| log.lsmr_stop == Some(LsmrStop::IterationLimit))
                .count(),
            landmark_rms: rms(&residuals),
            landmark_weighted_rms,
            iterations: logs,
        },
    })
}

fn validate_inputs(
    inputs: SkinRegistrationInputs<'_>,
    options: &DenseRegistrationOptions,
) -> Result<(), DenseRegistrationError> {
    let SkinRegistrationInputs {
        initial_vertices,
        base_vertices,
        triangles,
        constraints,
        seam,
        anchor_weights,
        fade_end,
    } = inputs;
    if base_vertices.is_empty() || triangles.is_empty() {
        return Err(DenseRegistrationError::EmptySkin);
    }
    if initial_vertices.len() != base_vertices.len() {
        return Err(DenseRegistrationError::VertexCountMismatch {
            initial: initial_vertices.len(),
            canonical: base_vertices.len(),
        });
    }
    if anchor_weights.len() != base_vertices.len() {
        return Err(DenseRegistrationError::AnchorCountMismatch {
            expected: base_vertices.len(),
            actual: anchor_weights.len(),
        });
    }
    if constraints.is_empty() {
        return Err(DenseRegistrationError::NoLandmarkConstraints);
    }
    if !fade_end.is_finite() {
        return Err(DenseRegistrationError::InvalidOption("fade_end"));
    }
    if initial_vertices
        .iter()
        .chain(base_vertices)
        .any(|vertex| !vertex.is_finite())
    {
        return Err(DenseRegistrationError::InvalidOption("finite vertices"));
    }
    for (triangle_index, triangle) in triangles.iter().enumerate() {
        for &vertex in triangle {
            if vertex as usize >= base_vertices.len() {
                return Err(DenseRegistrationError::TriangleVertexOutOfBounds {
                    triangle: triangle_index,
                    vertex: vertex as usize,
                    vertex_count: base_vertices.len(),
                });
            }
        }
    }
    for &vertex in seam {
        if vertex >= base_vertices.len() {
            return Err(DenseRegistrationError::SeamVertexOutOfBounds(vertex));
        }
    }
    for (constraint_index, constraint) in constraints.iter().enumerate() {
        for &vertex in &constraint.vertex_indices {
            if vertex >= base_vertices.len() {
                return Err(DenseRegistrationError::ConstraintVertexOutOfBounds {
                    constraint: constraint_index,
                    vertex,
                    vertex_count: base_vertices.len(),
                });
            }
        }
    }
    if anchor_weights
        .iter()
        .any(|weight| !weight.is_finite() || *weight < 0.0)
    {
        return Err(DenseRegistrationError::InvalidOption("anchor weights"));
    }
    if options.stages.is_empty() {
        return Err(DenseRegistrationError::InvalidOption("stages"));
    }
    if !options.minimum_correspondence_coverage.is_finite()
        || !(0.0..=1.0).contains(&options.minimum_correspondence_coverage)
    {
        return Err(DenseRegistrationError::InvalidOption(
            "minimum correspondence coverage",
        ));
    }
    if !options.eligible_fade_margin.is_finite() || options.eligible_fade_margin < 0.0 {
        return Err(DenseRegistrationError::InvalidOption(
            "eligible fade margin",
        ));
    }
    if !options.minimum_orientation_cosine.is_finite()
        || !(-1.0..=1.0).contains(&options.minimum_orientation_cosine)
        || !options.minimum_area_ratio.is_finite()
        || !options.maximum_area_ratio.is_finite()
        || options.minimum_area_ratio < 0.0
        || options.maximum_area_ratio < options.minimum_area_ratio
    {
        return Err(DenseRegistrationError::InvalidOption("topology thresholds"));
    }
    for stage in &options.stages {
        let finite = [
            stage.max_distance,
            stage.min_normal_dot,
            stage.trim_fraction,
            stage.huber_delta,
            stage.correspondence_weight,
            stage.smoothness,
            stage.point_to_point,
            stage.landmark_weight,
            stage.max_step,
        ]
        .into_iter()
        .all(f64::is_finite);
        if !finite
            || stage.max_distance <= 0.0
            || !(-1.0..=1.0).contains(&stage.min_normal_dot)
            || !(0.0..1.0).contains(&stage.trim_fraction)
            || stage.huber_delta <= 0.0
            || stage.correspondence_weight <= 0.0
            || stage.smoothness <= 0.0
            || stage.point_to_point < 0.0
            || stage.landmark_weight <= 0.0
            || stage.max_step <= 0.0
            || stage.iterations == 0
        {
            return Err(DenseRegistrationError::InvalidOption("registration stage"));
        }
    }
    Ok(())
}

fn project_one(
    projector: &SurfaceProjector,
    point: Vec3,
) -> Result<Projection, SurfaceProjectorError> {
    let query = point.to_array().map(|coordinate| coordinate as f32 as f64);
    let projection = projector.project(query)?;
    let projected_point = projection.point.map(|coordinate| coordinate as f32 as f64);
    let projected_normal = projection.normal.map(|coordinate| coordinate as f32 as f64);
    Ok(Projection {
        point: Vec3::from(projected_point),
        normal: Vec3::from(projected_normal),
        distance: (Vec3::from(projected_point) - point).norm(),
        primitive_id: projection.primitive_id,
        barycentric: projection.barycentric,
    })
}

const PARALLEL_PROJECTION_THRESHOLD: usize = 1_024;

fn project_all(
    projector: &SurfaceProjector,
    points: &[Vec3],
) -> Result<Vec<Projection>, SurfaceProjectorError> {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    if threads <= 1 || points.len() < PARALLEL_PROJECTION_THRESHOLD {
        return points
            .iter()
            .map(|&point| project_one(projector, point))
            .collect();
    }
    let chunk_size = points.len().div_ceil(threads);
    let mut slots: Vec<Result<Projection, SurfaceProjectorError>> =
        vec![Err(SurfaceProjectorError::InvalidArgument); points.len()];
    std::thread::scope(|scope| {
        for (chunk_points, chunk_slots) in
            points.chunks(chunk_size).zip(slots.chunks_mut(chunk_size))
        {
            scope.spawn(move || {
                for (point, slot) in chunk_points.iter().zip(chunk_slots.iter_mut()) {
                    *slot = project_one(projector, *point);
                }
            });
        }
    });
    slots.into_iter().collect()
}

pub(crate) struct ScanConnectivity {
    pub weld_map: Vec<u32>,

    pub largest_component: Vec<u32>,

    pub boundary_edges: BTreeSet<(u32, u32)>,

    pub boundary_vertices: BTreeSet<u32>,

    pub triangle_flipped: Vec<bool>,

    pub winding_consistent: bool,
}

fn weld_vertex_map(vertices: &[[f64; 3]]) -> Vec<u32> {
    if vertices.is_empty() {
        return Vec::new();
    }
    let mut minimum = [f64::INFINITY; 3];
    let mut maximum = [f64::NEG_INFINITY; 3];
    for vertex in vertices {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(vertex[axis]);
            maximum[axis] = maximum[axis].max(vertex[axis]);
        }
    }
    let diagonal = (0..3)
        .map(|axis| (maximum[axis] - minimum[axis]).powi(2))
        .sum::<f64>()
        .sqrt();

    let tolerance = (diagonal * 1.0e-6).max(1.0e-9);
    let inverse_cell = 1.0 / tolerance;
    let cell_key = |vertex: &[f64; 3]| {
        (
            (vertex[0] * inverse_cell).round() as i64,
            (vertex[1] * inverse_cell).round() as i64,
            (vertex[2] * inverse_cell).round() as i64,
        )
    };
    let tolerance_squared = tolerance * tolerance;
    let mut cells = std::collections::BTreeMap::<(i64, i64, i64), Vec<u32>>::new();
    let mut weld_map = vec![0_u32; vertices.len()];
    for (index, vertex) in vertices.iter().enumerate() {
        let base = cell_key(vertex);
        let mut best: Option<(f64, u32)> = None;
        for dx in -1..=1_i64 {
            for dy in -1..=1_i64 {
                for dz in -1..=1_i64 {
                    let Some(candidates) = cells.get(&(base.0 + dx, base.1 + dy, base.2 + dz))
                    else {
                        continue;
                    };
                    for &candidate in candidates {
                        let other = vertices[candidate as usize];
                        let distance_squared = (0..3)
                            .map(|axis| (vertex[axis] - other[axis]).powi(2))
                            .sum::<f64>();
                        if distance_squared <= tolerance_squared
                            && best.is_none_or(|(best_distance, best_index)| {
                                distance_squared < best_distance
                                    || (distance_squared == best_distance && candidate < best_index)
                            })
                        {
                            best = Some((distance_squared, candidate));
                        }
                    }
                }
            }
        }
        match best {
            Some((_, representative)) => weld_map[index] = representative,
            None => {
                weld_map[index] = index as u32;
                cells.entry(base).or_default().push(index as u32);
            }
        }
    }
    weld_map
}

pub(crate) fn analyze_scan_connectivity(mesh: &Mesh) -> ScanConnectivity {
    let triangle_count = mesh.triangles.len();
    let weld_map = weld_vertex_map(&mesh.vertices);

    let mut edge_records = Vec::with_capacity(triangle_count * 3);
    for (triangle_id, triangle) in mesh.triangles.iter().enumerate() {
        let welded = triangle.map(|vertex| weld_map[vertex as usize]);
        for (first, second) in [
            (welded[0], welded[1]),
            (welded[1], welded[2]),
            (welded[2], welded[0]),
        ] {
            if first == second {
                continue;
            }
            let forward = first < second;
            let (low, high) = if forward {
                (first, second)
            } else {
                (second, first)
            };
            edge_records.push((low, high, triangle_id as u32, forward));
        }
    }
    edge_records.sort_unstable();
    let mut adjacency = vec![Vec::new(); triangle_count];
    let mut orientation_constraints = vec![Vec::new(); triangle_count];
    let mut boundary_edges = BTreeSet::new();
    let mut boundary_vertices = BTreeSet::new();
    let mut run_start = 0;
    while run_start < edge_records.len() {
        let (low, high, _, _) = edge_records[run_start];
        let mut run_end = run_start + 1;
        while run_end < edge_records.len()
            && edge_records[run_end].0 == low
            && edge_records[run_end].1 == high
        {
            run_end += 1;
        }
        match run_end - run_start {
            1 => {
                boundary_edges.insert((low, high));
                boundary_vertices.insert(low);
                boundary_vertices.insert(high);
            }
            2 => {
                let (_, _, first_triangle, first_forward) = edge_records[run_start];
                let (_, _, second_triangle, second_forward) = edge_records[run_start + 1];
                adjacency[first_triangle as usize].push(second_triangle as usize);
                adjacency[second_triangle as usize].push(first_triangle as usize);

                let requires_flip = first_forward == second_forward;
                orientation_constraints[first_triangle as usize]
                    .push((second_triangle as usize, requires_flip));
                orientation_constraints[second_triangle as usize]
                    .push((first_triangle as usize, requires_flip));
            }
            _ => {}
        }
        run_start = run_end;
    }
    for neighbors in &mut adjacency {
        neighbors.sort_unstable();
        neighbors.dedup();
    }
    let mut seen = vec![false; triangle_count];
    let mut best_triangles = Vec::new();
    let mut best_triangle_count = 0;
    for start in 0..triangle_count {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let mut component = Vec::new();
        while let Some(triangle_id) = stack.pop() {
            component.push(triangle_id as u32);
            for &neighbor in &adjacency[triangle_id] {
                if !seen[neighbor] {
                    seen[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        component.sort_unstable();
        if component.len() > best_triangle_count {
            best_triangle_count = component.len();
            best_triangles = component;
        }
    }

    let mut triangle_flipped = vec![false; triangle_count];
    let mut winding_consistent = true;
    let mut visited = vec![false; triangle_count];
    for &start in &best_triangles {
        let start = start as usize;
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(triangle_id) = queue.pop_front() {
            for &(neighbor, requires_flip) in &orientation_constraints[triangle_id] {
                let expected = triangle_flipped[triangle_id] ^ requires_flip;
                if visited[neighbor] {
                    if triangle_flipped[neighbor] != expected {
                        winding_consistent = false;
                    }
                } else {
                    visited[neighbor] = true;
                    triangle_flipped[neighbor] = expected;
                    queue.push_back(neighbor);
                }
            }
        }
    }

    ScanConnectivity {
        weld_map,
        largest_component: best_triangles,
        boundary_edges,
        boundary_vertices,
        triangle_flipped,
        winding_consistent,
    }
}

pub(crate) fn largest_component_triangle_ids(mesh: &Mesh) -> Vec<u32> {
    analyze_scan_connectivity(mesh).largest_component
}

fn build_neighbors(vertex_count: usize, triangles: &[[u32; 3]]) -> Vec<Vec<usize>> {
    let mut adjacency = vec![BTreeSet::new(); vertex_count];
    for triangle in triangles {
        let triangle = triangle.map(|vertex| vertex as usize);
        for (first, second) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            if first != second {
                adjacency[first].insert(second);
                adjacency[second].insert(first);
            }
        }
    }
    adjacency
        .into_iter()
        .map(|neighbors| neighbors.into_iter().collect())
        .collect()
}

fn build_inverse_degrees(neighbors: &[Vec<usize>]) -> Vec<f64> {
    neighbors
        .iter()
        .map(|neighbors| {
            if neighbors.is_empty() {
                0.0
            } else {
                1.0 / neighbors.len() as f64
            }
        })
        .collect()
}

fn build_cotangent_weights(
    vertices: &[Vec3],
    triangles: &[[u32; 3]],
    neighbors: &[Vec<usize>],
) -> Vec<Vec<f64>> {
    let mut accumulated: Vec<BTreeMap<usize, f64>> = vec![BTreeMap::new(); neighbors.len()];
    for triangle in triangles {
        let corners = triangle.map(|index| index as usize);
        for corner in 0..3 {
            let (apex, first, second) = (
                corners[corner],
                corners[(corner + 1) % 3],
                corners[(corner + 2) % 3],
            );
            let Some(&origin) = vertices.get(apex) else {
                continue;
            };
            let (Some(&left), Some(&right)) = (vertices.get(first), vertices.get(second)) else {
                continue;
            };

            let to_left = left - origin;
            let to_right = right - origin;

            let area = Vec3::new(
                to_left.y.mul_add(to_right.z, -(to_left.z * to_right.y)),
                to_left.z.mul_add(to_right.x, -(to_left.x * to_right.z)),
                to_left.x.mul_add(to_right.y, -(to_left.y * to_right.x)),
            )
            .norm();

            if area.is_nan() || area <= 1.0e-20 {
                continue;
            }
            let cotangent = to_left.dot(to_right) / area;
            if !cotangent.is_finite() {
                continue;
            }
            *accumulated[first].entry(second).or_insert(0.0) += 0.5 * cotangent;
            *accumulated[second].entry(first).or_insert(0.0) += 0.5 * cotangent;
        }
    }
    neighbors
        .iter()
        .enumerate()
        .map(|(vertex, ring)| {
            let raw: Vec<f64> = ring
                .iter()
                .map(|neighbor| {
                    accumulated[vertex]
                        .get(neighbor)
                        .copied()
                        .unwrap_or(0.0)
                        .max(0.0)
                })
                .collect();
            let total: f64 = raw.iter().sum();
            if total > 1.0e-12 && total.is_finite() {
                raw.into_iter().map(|weight| weight / total).collect()
            } else if ring.is_empty() {
                Vec::new()
            } else {
                vec![1.0 / ring.len() as f64; ring.len()]
            }
        })
        .collect()
}

pub(crate) fn vertex_normals(vertices: &[Vec3], triangles: &[[u32; 3]]) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; vertices.len()];
    for triangle in triangles {
        let [a, b, c] = triangle.map(|vertex| vertex as usize);
        let face_normal = cross(vertices[b] - vertices[a], vertices[c] - vertices[a]);
        normals[a] += face_normal;
        normals[b] += face_normal;
        normals[c] += face_normal;
    }
    for normal in &mut normals {
        let length = normal.norm();
        if length > 1.0e-12 {
            *normal = *normal / length;
        }
    }
    normals
}

fn vertex_areas(vertices: &[Vec3], triangles: &[[u32; 3]]) -> Vec<f64> {
    let mut areas = vec![0.0; vertices.len()];
    for triangle in triangles {
        let [a, b, c] = triangle.map(|vertex| vertex as usize);
        let area = 0.5 * cross(vertices[b] - vertices[a], vertices[c] - vertices[a]).norm();
        let contribution = area / 3.0;
        areas[a] += contribution;
        areas[b] += contribution;
        areas[c] += contribution;
    }
    let positive_count = areas.iter().filter(|area| **area > 0.0).count();
    let positive_mean =
        areas.iter().filter(|area| **area > 0.0).sum::<f64>() / positive_count as f64;
    for area in &mut areas {
        if *area > 0.0 {
            *area /= positive_mean;
        }
    }
    areas
}

fn cross(first: Vec3, second: Vec3) -> Vec3 {
    Vec3::new(
        first.y * second.z - first.z * second.y,
        first.z * second.x - first.x * second.z,
        first.x * second.y - first.y * second.x,
    )
}

fn interpolate(constraint: BarycentricConstraint, vertices: &[Vec3]) -> Vec3 {
    vertices[constraint.vertex_indices[0]] * constraint.barycentric[0]
        + vertices[constraint.vertex_indices[1]] * constraint.barycentric[1]
        + vertices[constraint.vertex_indices[2]] * constraint.barycentric[2]
}

fn landmark_residuals(vertices: &[Vec3], constraints: &[BarycentricConstraint]) -> Vec<f64> {
    constraints
        .iter()
        .map(|constraint| (interpolate(*constraint, vertices) - constraint.target).norm())
        .collect()
}

fn weighted_landmark_rms(vertices: &[Vec3], constraints: &[BarycentricConstraint]) -> f64 {
    if constraints.is_empty() {
        return 0.0;
    }
    let residuals = landmark_residuals(vertices, constraints);
    let total_weight = constraints
        .iter()
        .map(|constraint| constraint.effective_weight)
        .sum::<f64>();
    (constraints
        .iter()
        .zip(residuals)
        .map(|(constraint, residual)| constraint.effective_weight * residual * residual)
        .sum::<f64>()
        / total_weight)
        .sqrt()
}

fn exterior_lower_mask(vertices: &[Vec3], eligible: &[bool]) -> Vec<bool> {
    let selected = vertices
        .iter()
        .zip(eligible)
        .filter_map(|(vertex, selected)| selected.then_some(*vertex))
        .collect::<Vec<_>>();
    let minimum_x = selected
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let maximum_x = selected
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let minimum_y = selected
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let maximum_y = selected
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let center_x = (minimum_x + maximum_x) * 0.5;
    let half_width = ((maximum_x - minimum_x) * 0.5).max(1.0e-12);
    let height = (maximum_y - minimum_y).max(1.0e-12);
    let mut mask = vertices
        .iter()
        .zip(eligible)
        .map(|(point, selected)| {
            *selected
                && ((point.y - minimum_y) / height <= 0.45
                    || (point.x - center_x).abs() / half_width >= 0.72)
        })
        .collect::<Vec<_>>();
    if !mask.iter().any(|selected| *selected) {
        mask.clone_from_slice(eligible);
    }
    mask
}

fn candidate_vertices(
    current: &[Vec3],
    delta: &[Vec3],
    tau: f64,
    seam: &[usize],
    base: &[Vec3],
) -> Vec<Vec3> {
    let mut candidate = current
        .iter()
        .zip(delta)
        .map(|(vertex, step)| *vertex + *step * tau)
        .collect::<Vec<_>>();
    for &vertex in seam {
        candidate[vertex] = base[vertex];
    }
    candidate
}

const ENERGY_DESCENT_MARGIN: f64 = 1.0 - 1.0e-7;

fn energy_aware_graph_blend(
    system: &DenseSystem<'_>,
    right_hand: &[f64],
    before_linear_energy: f64,
    raw: &[Vec3],
    projected: &[Vec3],
) -> Result<(Vec<Vec3>, f64), DenseRegistrationError> {
    let difference = raw
        .iter()
        .zip(projected)
        .flat_map(|(raw_value, projected_value)| {
            let step = *projected_value - *raw_value;
            [step.x, step.y, step.z]
        })
        .collect::<Vec<_>>();
    if difference.iter().all(|value| *value == 0.0) {
        return Ok((projected.to_vec(), 1.0));
    }
    let raw_flat = flatten_scaled(raw, 1.0);
    let mut raw_residual = vec![0.0; system.rows()];
    system.apply(&raw_flat, &mut raw_residual)?;
    for (residual, rhs) in raw_residual.iter_mut().zip(right_hand) {
        *residual -= rhs;
    }
    let mut difference_image = vec![0.0; system.rows()];
    system.apply(&difference, &mut difference_image)?;
    let raw_energy = squared_norm(&raw_residual);
    let cross = raw_residual
        .iter()
        .zip(&difference_image)
        .map(|(left, right)| left * right)
        .sum::<f64>();
    let curvature = squared_norm(&difference_image);
    let projected_energy = raw_energy + 2.0 * cross + curvature;
    let bar = before_linear_energy * ENERGY_DESCENT_MARGIN;
    if projected_energy <= bar || raw_energy > bar || curvature <= 1.0e-30 {
        return Ok((projected.to_vec(), 1.0));
    }

    let discriminant = (cross * cross + curvature * (bar - raw_energy)).max(0.0);
    let lambda = ((-cross + discriminant.sqrt()) / curvature).clamp(0.0, 1.0);
    let blended = raw
        .iter()
        .zip(projected)
        .map(|(raw_value, projected_value)| *raw_value * (1.0 - lambda) + *projected_value * lambda)
        .collect();
    Ok((blended, lambda))
}

const fn deformation_graph_schedule(kind: DenseStageKind) -> DeformationGraphSchedule {
    match kind {
        DenseStageKind::Envelope => DeformationGraphSchedule {
            divisor: 160,
            minimum_nodes: 24,
            maximum_nodes: 48,
            graph_blend: 0.90,
        },
        DenseStageKind::Coarse => DeformationGraphSchedule {
            divisor: 80,
            minimum_nodes: 32,
            maximum_nodes: 72,
            graph_blend: 0.72,
        },
        DenseStageKind::Medium => DeformationGraphSchedule {
            divisor: 40,
            minimum_nodes: 48,
            maximum_nodes: 112,
            graph_blend: 0.34,
        },
        DenseStageKind::Fine => DeformationGraphSchedule {
            divisor: 20,
            minimum_nodes: 72,
            maximum_nodes: 192,
            graph_blend: 0.08,
        },
    }
}

fn safety_mask(
    base: &[Vec3],
    candidate: &[Vec3],
    triangles: &[[u32; 3]],
    options: &DenseRegistrationOptions,
) -> Result<Vec<bool>, GeometryKernelError> {
    let base = base.iter().copied().map(Vec3::to_array).collect::<Vec<_>>();
    let candidate = candidate
        .iter()
        .copied()
        .map(Vec3::to_array)
        .collect::<Vec<_>>();
    deformation_safety_mask(
        &base,
        &candidate,
        triangles,
        options.minimum_orientation_cosine,
        options.minimum_area_ratio,
        options.maximum_area_ratio,
    )
}

fn maximum_norm(values: &[Vec3]) -> f64 {
    values.iter().map(|value| value.norm()).fold(0.0, f64::max)
}

fn flatten_scaled(values: &[Vec3], scale: f64) -> Vec<f64> {
    values
        .iter()
        .flat_map(|value| [value.x * scale, value.y * scale, value.z * scale])
        .collect()
}

fn squared_norm(values: &[f64]) -> f64 {
    values.iter().map(|value| value * value).sum()
}

fn rms(values: &[f64]) -> f64 {
    (squared_norm(values) / values.len() as f64).sqrt()
}

fn quantile(values: &[f64], fraction: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let position = fraction * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    Some(if lower == upper {
        sorted[lower]
    } else {
        let t = position - lower as f64;
        sorted[lower] * (1.0 - t) + sorted[upper] * t
    })
}

const BOUNDARY_BARYCENTRIC_EPSILON: f64 = 1.0e-6;

fn projection_touches_boundary(
    projection: &Projection,
    triangles: &[[u32; 3]],
    connectivity: &ScanConnectivity,
) -> bool {
    if connectivity.boundary_edges.is_empty() {
        return false;
    }
    let triangle = triangles[projection.primitive_id as usize];
    let welded = triangle.map(|vertex| connectivity.weld_map[vertex as usize]);
    let near_zero = projection
        .barycentric
        .map(|coordinate| coordinate <= BOUNDARY_BARYCENTRIC_EPSILON);
    match near_zero.iter().filter(|flag| **flag).count() {
        0 => false,
        1 => {
            let opposite = near_zero
                .iter()
                .position(|flag| *flag)
                .expect("exactly one barycentric coordinate is near zero");
            let first = welded[(opposite + 1) % 3];
            let second = welded[(opposite + 2) % 3];
            let edge = if first < second {
                (first, second)
            } else {
                (second, first)
            };
            first == second || connectivity.boundary_edges.contains(&edge)
        }
        _ => {
            let corner = (0..3)
                .max_by(|&left, &right| {
                    projection.barycentric[left].total_cmp(&projection.barycentric[right])
                })
                .expect("triangles have three corners");
            connectivity.boundary_vertices.contains(&welded[corner])
        }
    }
}

fn validate_operator_vectors<O: LinearOperator>(
    operator: &O,
    input: &[f64],
    output: &[f64],
    kind: &'static str,
) -> Result<(), OperatorError> {
    if input.len() != operator.columns() {
        return Err(OperatorError::VectorLength {
            kind,
            expected: operator.columns(),
            actual: input.len(),
        });
    }
    if output.len() != operator.rows() {
        return Err(OperatorError::VectorLength {
            kind,
            expected: operator.rows(),
            actual: output.len(),
        });
    }
    Ok(())
}

fn validate_transpose_vectors<O: LinearOperator>(
    operator: &O,
    input: &[f64],
    output: &[f64],
    kind: &'static str,
) -> Result<(), OperatorError> {
    if input.len() != operator.rows() {
        return Err(OperatorError::VectorLength {
            kind,
            expected: operator.rows(),
            actual: input.len(),
        });
    }
    if output.len() != operator.columns() {
        return Err(OperatorError::VectorLength {
            kind,
            expected: operator.columns(),
            actual: output.len(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane_grid(size: usize, z: f64) -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let mut vertices = Vec::new();
        for y in 0..size {
            for x in 0..size {
                vertices.push(Vec3::new(x as f64, y as f64, z));
            }
        }
        let mut triangles = Vec::new();
        for y in 0..size - 1 {
            for x in 0..size - 1 {
                let a = (y * size + x) as u32;
                let b = a + 1;
                let c = a + size as u32;
                let d = c + 1;
                triangles.push([a, b, d]);
                triangles.push([a, d, c]);
            }
        }
        (vertices, triangles)
    }

    fn enlarged_plane_target(size: usize, z: f64) -> Mesh {
        let (vertices, triangles) = plane_grid(size + 2, z);
        Mesh::new(
            vertices
                .into_iter()
                .map(|vertex| [vertex.x - 1.0, vertex.y - 1.0, vertex.z])
                .collect(),
            triangles,
        )
        .unwrap()
    }

    fn legacy_canonical_biased_stages() -> Vec<DenseRegistrationStage> {
        vec![
            DenseRegistrationStage {
                kind: DenseStageKind::Envelope,
                max_distance: 4.50,
                min_normal_dot: -0.35,
                trim_fraction: 0.05,
                huber_delta: 1.20,
                correspondence_weight: 1.0,
                smoothness: 160.0,
                point_to_point: 0.20,
                landmark_weight: 90.0,
                max_step: 0.30,
                iterations: 6,
            },
            DenseRegistrationStage {
                kind: DenseStageKind::Coarse,
                max_distance: 3.00,
                min_normal_dot: -0.20,
                trim_fraction: 0.00,
                huber_delta: 1.00,
                correspondence_weight: 1.0,
                smoothness: 350.0,
                point_to_point: 0.04,
                landmark_weight: 75.0,
                max_step: 0.35,
                iterations: 8,
            },
            DenseRegistrationStage {
                kind: DenseStageKind::Medium,
                max_distance: 2.00,
                min_normal_dot: 0.00,
                trim_fraction: 0.02,
                huber_delta: 0.60,
                correspondence_weight: 1.0,
                smoothness: 150.0,
                point_to_point: 0.04,
                landmark_weight: 55.0,
                max_step: 0.18,
                iterations: 6,
            },
            DenseRegistrationStage {
                kind: DenseStageKind::Fine,
                max_distance: 1.00,
                min_normal_dot: 0.30,
                trim_fraction: 0.05,
                huber_delta: 0.30,
                correspondence_weight: 1.0,
                smoothness: 65.0,
                point_to_point: 0.02,
                landmark_weight: 40.0,
                max_step: 0.08,
                iterations: 6,
            },
        ]
    }

    fn vertex_constraint(
        vertex: usize,
        target: Vec3,
        triangles: &[[u32; 3]],
    ) -> BarycentricConstraint {
        let triangle = triangles
            .iter()
            .find(|triangle| triangle.contains(&(vertex as u32)))
            .expect("the grid vertex belongs to a triangle");
        let corner = triangle
            .iter()
            .position(|index| *index as usize == vertex)
            .expect("the selected triangle contains the vertex");
        let mut barycentric = [0.0; 3];
        barycentric[corner] = 1.0;
        BarycentricConstraint::new(
            triangle.map(|index| index as usize),
            barycentric,
            target,
            2.0,
            1.0,
        )
        .unwrap()
    }

    fn indexed_rms(actual: &[Vec3], expected: &[Vec3], indices: &[usize]) -> f64 {
        (indices
            .iter()
            .map(|&index| (actual[index] - expected[index]).norm_squared())
            .sum::<f64>()
            / indices.len() as f64)
            .sqrt()
    }

    #[test]
    fn fidelity_moves_only_the_balance_between_scan_and_smoothness() {
        let tuned = dense_stages_at_fidelity(1.0);
        assert_eq!(tuned, DEFAULT_DENSE_REGISTRATION_STAGES.to_vec());

        for fidelity in [0.4, 0.7, 1.5, 2.0, 3.5, 5.0] {
            let stages = dense_stages_at_fidelity(fidelity);
            assert_eq!(stages.len(), DEFAULT_DENSE_REGISTRATION_STAGES.len());
            for (stage, tuned) in stages.iter().zip(&DEFAULT_DENSE_REGISTRATION_STAGES) {
                assert_eq!(stage.kind, tuned.kind);
                assert_eq!(stage.iterations, tuned.iterations);
                assert_eq!(stage.max_distance, tuned.max_distance);
                assert_eq!(stage.min_normal_dot, tuned.min_normal_dot);
                assert_eq!(stage.trim_fraction, tuned.trim_fraction);
                assert_eq!(stage.max_step, tuned.max_step);
                assert_eq!(stage.landmark_weight, tuned.landmark_weight);

                let was = tuned.correspondence_weight / tuned.smoothness;
                let now = stage.correspondence_weight / stage.smoothness;
                assert_eq!(now > was, fidelity > 1.0, "{fidelity} moved the wrong way");
            }
        }
    }

    #[test]
    fn an_impossible_fidelity_falls_back_to_the_tuned_schedule() {
        for bad in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
            let stages = dense_stages_at_fidelity(bad);
            assert!(stages.iter().all(|stage| stage.smoothness.is_finite()
                && stage.smoothness > 0.0
                && stage.correspondence_weight > 0.0));
        }
    }

    #[test]
    fn default_schedule_starts_with_a_broad_outer_envelope() {
        assert_eq!(DEFAULT_DENSE_REGISTRATION_STAGES.len(), 4);
        assert_eq!(
            DEFAULT_DENSE_REGISTRATION_STAGES[0].kind,
            DenseStageKind::Envelope
        );

        assert!(DEFAULT_DENSE_REGISTRATION_STAGES[0].iterations >= 6);
        assert_eq!(DEFAULT_DENSE_REGISTRATION_STAGES[0].max_distance, 4.5);
        assert_eq!(
            DEFAULT_DENSE_REGISTRATION_STAGES[0].correspondence_weight,
            10.0
        );
        assert_eq!(DEFAULT_DENSE_REGISTRATION_STAGES[0].trim_fraction, 0.0);
        assert_eq!(DEFAULT_DENSE_REGISTRATION_STAGES[1].iterations, 8);

        assert_eq!(DEFAULT_DENSE_REGISTRATION_STAGES[3].landmark_weight, 140.0);
        assert_eq!(DEFAULT_DENSE_REGISTRATION_STAGES[3].max_step, 0.08);
        assert!(
            DEFAULT_DENSE_REGISTRATION_STAGES
                .iter()
                .map(|stage| stage.iterations)
                .sum::<usize>()
                >= 26
        );
    }

    #[test]
    fn python_parity_schedule_stays_frozen_to_the_captured_oracle() {
        let parity = &PYTHON_PARITY_DENSE_REGISTRATION_STAGES;
        assert_eq!(parity.len(), 3);
        assert_eq!(
            parity.iter().map(|stage| stage.kind).collect::<Vec<_>>(),
            [
                DenseStageKind::Coarse,
                DenseStageKind::Medium,
                DenseStageKind::Fine
            ]
        );
        assert_eq!(
            parity
                .iter()
                .map(|stage| stage.smoothness)
                .collect::<Vec<_>>(),
            [120.0, 60.0, 30.0]
        );
        assert_eq!(
            parity
                .iter()
                .map(|stage| stage.landmark_weight)
                .collect::<Vec<_>>(),
            [160.0, 140.0, 120.0]
        );
        assert_eq!(
            parity
                .iter()
                .map(|stage| stage.iterations)
                .collect::<Vec<_>>(),
            [8, 6, 2]
        );

        assert!(DenseRegistrationOptions::default().graph_regularization);
    }

    fn uniform_weights(neighbors: &[Vec<usize>]) -> Vec<Vec<f64>> {
        build_inverse_degrees(neighbors)
            .into_iter()
            .zip(neighbors)
            .map(|(uniform, ring)| vec![uniform; ring.len()])
            .collect()
    }

    fn chain(count: usize) -> Vec<Vec<usize>> {
        (0..count)
            .map(|index| {
                let mut ring = Vec::new();
                if index > 0 {
                    ring.push(index - 1);
                }
                if index + 1 < count {
                    ring.push(index + 1);
                }
                ring
            })
            .collect()
    }

    fn support_over(count: usize, vertices: &[usize]) -> Vec<f64> {
        let mut field = vec![0.0; count];
        for &vertex in vertices {
            field[vertex] = 1.0;
        }
        field
    }

    #[test]
    fn a_right_angled_grid_reduces_to_the_five_point_stencil() {
        let (vertices, triangles) = plane_grid(5, 0.0);
        let neighbors = build_neighbors(vertices.len(), &triangles);
        let cotangent = build_cotangent_weights(&vertices, &triangles, &neighbors);

        let interior = 2 * 5 + 2;
        let ring = &neighbors[interior];
        let weights = &cotangent[interior];
        let carrying: Vec<f64> = weights.iter().copied().filter(|w| *w > 1.0e-9).collect();

        assert_eq!(
            carrying.len(),
            4,
            "only the four edge neighbours should carry: {weights:?}"
        );
        for weight in &carrying {
            assert!((weight - 0.25).abs() < 1.0e-9, "{weights:?}");
        }
        assert_eq!(ring.len(), 6, "and the diagonals are still neighbours");
    }

    #[test]
    fn every_smoothness_row_still_sums_to_one() {
        let (mut vertices, triangles) = plane_grid(5, 0.0);

        for (index, vertex) in vertices.iter_mut().enumerate() {
            vertex.x *= 0.08;
            vertex.z += 0.4 * (index % 3) as f64;
        }
        let neighbors = build_neighbors(vertices.len(), &triangles);
        let cotangent = build_cotangent_weights(&vertices, &triangles, &neighbors);

        for (vertex, ring) in cotangent.iter().enumerate() {
            if ring.is_empty() {
                continue;
            }
            let total: f64 = ring.iter().sum();
            assert!(
                (total - 1.0).abs() < 1.0e-9,
                "vertex {vertex} sums to {total}"
            );

            assert!(ring.iter().all(|weight| *weight >= 0.0), "{ring:?}");
        }
    }

    #[test]
    fn a_stretched_mesh_weights_its_neighbours_by_shape_not_by_count() {
        let (mut vertices, triangles) = plane_grid(5, 0.0);

        for vertex in &mut vertices {
            vertex.x *= 0.1;
        }
        let neighbors = build_neighbors(vertices.len(), &triangles);
        let cotangent = build_cotangent_weights(&vertices, &triangles, &neighbors);

        let interior = 2 * 5 + 2;
        let ring = &neighbors[interior];
        let weights = &cotangent[interior];
        let uniform = 1.0 / ring.len() as f64;
        let spread = weights
            .iter()
            .map(|weight| (weight - uniform).abs())
            .fold(0.0_f64, f64::max);
        assert!(
            spread > 0.1,
            "on a skewed mesh the neighbours must not all count the same: {weights:?}"
        );
    }

    #[test]
    fn the_shell_is_compliant_where_the_scan_has_something_to_say() {
        let neighbors = chain(40);

        let scales = laplacian_scales(
            40,
            100.0,
            &support_over(40, &(0..20).collect::<Vec<_>>()),
            &[],
            &neighbors,
        );

        let supported = scales[4];
        let unsupported = scales[35];
        assert!(
            supported < unsupported * 0.75,
            "the covered half must be markedly freer: {supported} vs {unsupported}"
        );

        assert!(supported > 0.0);
        assert!(
            unsupported > (100.0_f64 * 0.9).sqrt(),
            "an uncovered vertex keeps essentially the whole prior"
        );
    }

    #[test]
    fn a_region_the_scan_reached_once_does_not_stiffen_again() {
        let neighbors = chain(40);

        let early = support_over(40, &(0..40).collect::<Vec<_>>());
        let loose = laplacian_scales(40, 100.0, &early, &[], &neighbors);

        let mut accumulated = early.clone();
        for (index, value) in accumulated.iter_mut().enumerate() {
            if (15..25).contains(&index) {
                *value = 1.0;
            }
        }
        let later = laplacian_scales(40, 100.0, &accumulated, &[], &neighbors);
        for (index, (before, after)) in loose.iter().zip(&later).enumerate() {
            assert!(
                after <= &(before + 1.0e-9),
                "vertex {index} stiffened between stages: {before} -> {after}"
            );
        }

        let narrow_only = laplacian_scales(
            40,
            100.0,
            &support_over(40, &(15..25).collect::<Vec<_>>()),
            &[],
            &neighbors,
        );
        assert!(
            narrow_only[0] > loose[0] * 1.5,
            "the test's own premise: a narrow window would have re-stiffened the ends"
        );
    }

    #[test]
    fn a_pin_keeps_the_shell_that_carries_it() {
        let neighbors = chain(40);

        let everywhere = support_over(40, &(0..40).collect::<Vec<_>>());
        let free = laplacian_scales(40, 100.0, &everywhere, &[], &neighbors);

        let pin = BarycentricConstraint::new(
            [20, 21, 22],
            [0.5, 0.3, 0.2],
            Vec3::new(0.0, 0.0, 0.0),
            1.0,
            1.0,
        )
        .unwrap();
        let pinned = laplacian_scales(40, 100.0, &everywhere, &[pin], &neighbors);

        assert!(
            pinned[21] > free[21] * 1.5,
            "the shell at the pin must come back: {} vs {}",
            pinned[21],
            free[21]
        );

        assert!((pinned[2] - free[2]).abs() <= 1.0e-9);

        assert!(pinned[21] > pinned[24] && pinned[24] > pinned[30] - 1.0e-9);
    }

    #[test]
    fn the_stiffness_field_has_no_step_in_it() {
        let neighbors = chain(40);
        let scales = laplacian_scales(
            40,
            100.0,
            &support_over(40, &(0..20).collect::<Vec<_>>()),
            &[],
            &neighbors,
        );
        let span = scales.iter().copied().fold(0.0_f64, f64::max)
            - scales.iter().copied().fold(f64::MAX, f64::min);
        for pair in scales.windows(2) {
            assert!(
                (pair[0] - pair[1]).abs() < span * 0.5,
                "the field steps between neighbours: {scales:?}"
            );
        }
    }

    #[test]
    fn no_correspondences_leaves_the_authored_smoothness_untouched() {
        let neighbors = chain(12);
        let scales = laplacian_scales(12, 49.0, &[], &[], &neighbors);
        assert!(scales.iter().all(|scale| (scale - 7.0).abs() < 1.0e-12));
    }

    #[test]
    fn the_parity_shell_is_uniform_and_the_interactive_one_is_not() {
        let neighbors = chain(20);
        let rows = support_over(20, &[0, 1, 2, 3]);
        let weights = uniform_weights(&neighbors);

        let mut uniform = DenseSystem::new(DenseSystemSpec {
            vertex_count: 20,
            correspondence_indices: &[],
            target_normals: &[],
            data_weights: &[],
            point_to_point: 0.1,
            neighbors: &neighbors,
            neighbor_weights: &weights,
            smoothness: 100.0,
            adaptive_stiffness: false,
            scan_support: &[],
            strict_constraints: &[],
            constraints: &[],
            landmark_weight: 1.0,
            anchor_weights: &[0.0; 20],
        })
        .laplacian_scales;
        uniform.dedup();
        assert_eq!(uniform.len(), 1, "the parity shell must be one number");

        let adaptive = laplacian_scales(20, 100.0, &rows, &[], &neighbors);
        assert!(
            adaptive
                .iter()
                .any(|scale| (scale - uniform[0]).abs() > 1.0e-9),
            "and the interactive one must actually vary"
        );
    }

    #[test]
    fn the_shell_loosens_as_the_window_narrows() {
        let stages = DEFAULT_DENSE_REGISTRATION_STAGES;
        for pair in stages.windows(2) {
            assert!(
                pair[0].smoothness > pair[1].smoothness,
                "smoothness must never rise: {:?}",
                stages.map(|stage| stage.smoothness)
            );
            assert!(
                pair[0].max_distance > pair[1].max_distance,
                "the correspondence window must never widen: {:?}",
                stages.map(|stage| stage.max_distance)
            );
        }

        assert!(
            stages[0].smoothness < PYTHON_PARITY_DENSE_REGISTRATION_STAGES[0].smoothness * 0.25,
            "the envelope stage is the only chance a different face shape gets"
        );
    }

    #[test]
    fn the_graph_lets_go_by_the_finishing_stage() {
        let blends = [
            DenseStageKind::Envelope,
            DenseStageKind::Coarse,
            DenseStageKind::Medium,
            DenseStageKind::Fine,
        ]
        .map(|kind| deformation_graph_schedule(kind).graph_blend);

        for pair in blends.windows(2) {
            assert!(
                pair[0] > pair[1],
                "the graph must loosen monotonically: {blends:?}"
            );
        }
        assert!(
            blends[0] >= 0.85,
            "the envelope stage is what keeps a scan from tearing the shell"
        );
        assert!(
            blends[3] <= 0.15,
            "the finishing stage is where a face stops being generic, and it              cannot do that through a rigid projection: {}",
            blends[3]
        );

        let nodes = [
            DenseStageKind::Envelope,
            DenseStageKind::Coarse,
            DenseStageKind::Medium,
            DenseStageKind::Fine,
        ]
        .map(|kind| deformation_graph_schedule(kind).maximum_nodes);
        for pair in nodes.windows(2) {
            assert!(pair[0] < pair[1], "{nodes:?}");
        }
    }

    #[test]
    fn matrix_free_dense_system_has_a_consistent_transpose() {
        let (vertices, triangles) = plane_grid(3, 0.0);
        let neighbors = build_neighbors(vertices.len(), &triangles);
        let constraint = BarycentricConstraint::new(
            [0, 1, 4],
            [0.2, 0.3, 0.5],
            Vec3::new(0.8, 0.5, 0.2),
            2.0,
            0.75,
        )
        .unwrap();
        let constraints = [constraint];
        let correspondence_indices = [0, 4, 8];
        let target_normals = [Vec3::new(0.0, 0.0, 1.0); 3];
        let data_weights = [1.0, 0.5, 2.0];
        let anchor_weights = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0];
        let weights = uniform_weights(&neighbors);
        let system = DenseSystem::new(DenseSystemSpec {
            vertex_count: vertices.len(),
            correspondence_indices: &correspondence_indices,
            target_normals: &target_normals,
            data_weights: &data_weights,
            point_to_point: 0.04,
            neighbors: &neighbors,
            neighbor_weights: &weights,
            smoothness: 65.0,
            adaptive_stiffness: false,
            scan_support: &[],
            strict_constraints: &[],
            constraints: &constraints,
            landmark_weight: 40.0,
            anchor_weights: &anchor_weights,
        });
        let x = (0..system.columns())
            .map(|index| (index as f64 * 0.17).sin())
            .collect::<Vec<_>>();
        let y = (0..system.rows())
            .map(|index| (index as f64 * 0.11).cos())
            .collect::<Vec<_>>();
        let mut ax = vec![0.0; system.rows()];
        let mut aty = vec![0.0; system.columns()];
        system.apply(&x, &mut ax).unwrap();
        system.apply_transpose(&y, &mut aty).unwrap();
        let left = ax.iter().zip(&y).map(|(a, b)| a * b).sum::<f64>();
        let right = x.iter().zip(&aty).map(|(a, b)| a * b).sum::<f64>();
        assert!((left - right).abs() <= 1.0e-10, "{left} != {right}");
    }

    #[test]
    fn fused_xyz_laplacian_is_bit_exact_to_coordinate_major_reference() {
        let neighbors = vec![vec![1, 2], vec![0, 2], vec![0, 1], Vec::new()];
        let empty_indices = [];
        let empty_normals = [];
        let empty_weights = [];
        let empty_constraints = [];
        let anchor_weights = [0.0; 4];
        let weights = uniform_weights(&neighbors);
        let system = DenseSystem::new(DenseSystemSpec {
            vertex_count: neighbors.len(),
            correspondence_indices: &empty_indices,
            target_normals: &empty_normals,
            data_weights: &empty_weights,
            point_to_point: 0.04,
            neighbors: &neighbors,
            neighbor_weights: &weights,
            smoothness: 65.0,
            adaptive_stiffness: false,
            scan_support: &[],
            strict_constraints: &[],
            constraints: &empty_constraints,
            landmark_weight: 40.0,
            anchor_weights: &anchor_weights,
        });
        let input = (0..system.columns())
            .map(|index| ((index as f64 + 0.25) * 0.37).sin() * 10.0_f64.powi(index as i32 % 5 - 2))
            .collect::<Vec<_>>();

        let mut actual = vec![0.0; system.rows()];
        system.apply(&input, &mut actual).unwrap();
        let mut expected = vec![0.0; system.rows()];
        let mut row = 0;
        for (vertex, vertex_neighbors) in neighbors.iter().enumerate() {
            let inverse_degree = if vertex_neighbors.is_empty() {
                0.0
            } else {
                1.0 / vertex_neighbors.len() as f64
            };
            for coordinate in 0..3 {
                let mut value = 0.0;
                for &neighbor in vertex_neighbors {
                    value -= inverse_degree * input[neighbor * 3 + coordinate];
                }
                value += input[vertex * 3 + coordinate];
                expected[row] = system.laplacian_scales[vertex] * value;
                row += 1;
            }
        }
        assert_eq!(
            actual
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        );

        let transpose_input = (0..system.rows())
            .map(|index| ((index as f64 + 0.75) * 0.23).cos())
            .collect::<Vec<_>>();
        let mut actual_transpose = vec![0.0; system.columns()];
        system
            .apply_transpose(&transpose_input, &mut actual_transpose)
            .unwrap();
        let mut expected_transpose = vec![0.0; system.columns()];
        row = 0;
        for (vertex, vertex_neighbors) in neighbors.iter().enumerate() {
            let inverse_degree = if vertex_neighbors.is_empty() {
                0.0
            } else {
                1.0 / vertex_neighbors.len() as f64
            };
            for coordinate in 0..3 {
                let scaled = system.laplacian_scales[vertex] * transpose_input[row];
                for &neighbor in vertex_neighbors {
                    expected_transpose[neighbor * 3 + coordinate] -= inverse_degree * scaled;
                }
                expected_transpose[vertex * 3 + coordinate] += scaled;
                row += 1;
            }
        }
        assert_eq!(
            actual_transpose
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>(),
            expected_transpose
                .iter()
                .map(|value| value.to_bits())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn quantile_of_an_empty_slice_is_none() {
        assert_eq!(quantile(&[], 0.5), None);
        assert_eq!(quantile(&[2.0], 0.95), Some(2.0));
        assert_eq!(quantile(&[1.0, 3.0], 0.5), Some(2.0));
    }

    #[test]
    fn duplicated_seam_positions_weld_into_one_component() {
        let mut vertices: Vec<[f64; 3]> = Vec::new();
        for y in 0..3 {
            for x in 0..3 {
                vertices.push([x as f64, y as f64, 0.0]);
            }
        }
        for y in 0..3 {
            vertices.push([1.0, y as f64, 0.0]);
        }
        let orig = |y: usize, x: usize| (y * 3 + x) as u32;
        let dup = |y: usize| (9 + y) as u32;
        let mut triangles = Vec::new();
        for y in 0..2 {
            let (a, b, c, d) = (orig(y, 0), orig(y, 1), orig(y + 1, 0), orig(y + 1, 1));
            triangles.push([a, b, d]);
            triangles.push([a, d, c]);
        }
        for y in 0..2 {
            let (a, b, c, d) = (dup(y), orig(y, 2), dup(y + 1), orig(y + 1, 2));
            triangles.push([a, b, d]);
            triangles.push([a, d, c]);
        }
        let mesh = Mesh::new(vertices, triangles).unwrap();
        let connectivity = analyze_scan_connectivity(&mesh);
        assert_eq!(
            connectivity.largest_component,
            (0..8).collect::<Vec<u32>>(),
            "the duplicated seam must not fragment the scan into islands"
        );
        assert_eq!(connectivity.weld_map[9], 1);
        assert_eq!(connectivity.weld_map[10], 4);
        assert_eq!(connectivity.weld_map[11], 7);
        assert!(connectivity.winding_consistent);
        assert!(connectivity.triangle_flipped.iter().all(|flip| !flip));

        assert!(!connectivity.boundary_edges.contains(&(1, 4)));
        assert!(!connectivity.boundary_edges.contains(&(4, 7)));
        assert!(connectivity.boundary_edges.contains(&(0, 1)));
        assert!(connectivity.boundary_edges.contains(&(0, 3)));
    }

    #[test]
    fn mixed_winding_triangles_are_oriented_coherently() {
        let quad_vertices = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ];

        let mixed = Mesh::new(quad_vertices.clone(), vec![[0, 1, 2], [1, 2, 3]]).unwrap();
        let connectivity = analyze_scan_connectivity(&mixed);
        assert!(connectivity.winding_consistent);
        assert_eq!(connectivity.triangle_flipped, vec![false, true]);

        let consistent = Mesh::new(quad_vertices, vec![[0, 1, 2], [1, 3, 2]]).unwrap();
        let connectivity = analyze_scan_connectivity(&consistent);
        assert!(connectivity.winding_consistent);
        assert_eq!(connectivity.triangle_flipped, vec![false, false]);
    }

    #[test]
    fn moebius_identification_reports_an_inconsistent_winding() {
        let vertices = (0..5)
            .map(|index| {
                let angle = std::f64::consts::TAU * index as f64 / 5.0;
                [angle.cos(), angle.sin(), (index % 2) as f64 * 0.2]
            })
            .collect::<Vec<_>>();
        let mesh = Mesh::new(
            vertices,
            vec![[0, 1, 2], [1, 2, 3], [2, 3, 4], [3, 4, 0], [4, 0, 1]],
        )
        .unwrap();
        let connectivity = analyze_scan_connectivity(&mesh);
        assert_eq!(connectivity.largest_component.len(), 5);
        assert!(!connectivity.winding_consistent);
    }

    #[test]
    fn hole_rim_projections_are_rejected() {
        let (vertices, triangles) = plane_grid(4, 0.0);
        let kept = triangles
            .iter()
            .enumerate()
            .filter_map(|(index, triangle)| (index != 8 && index != 9).then_some(*triangle))
            .collect::<Vec<_>>();
        let mesh = Mesh::new(vertices.iter().copied().map(Vec3::to_array).collect(), kept).unwrap();
        let connectivity = analyze_scan_connectivity(&mesh);
        assert_eq!(connectivity.largest_component.len(), 16);
        let projector = SurfaceProjector::new(
            &mesh.vertices,
            &mesh.triangles,
            Some(&connectivity.largest_component),
            0.0,
        )
        .unwrap();
        let over_hole = project_one(&projector, Vec3::new(1.5, 1.5, 0.4)).unwrap();
        assert!(
            projection_touches_boundary(&over_hole, &mesh.triangles, &connectivity),
            "a projection over the hole must land on the rim and be rejected"
        );
        let beyond_rim = project_one(&projector, Vec3::new(1.5, -0.7, 0.0)).unwrap();
        assert!(
            projection_touches_boundary(&beyond_rim, &mesh.triangles, &connectivity),
            "a projection beyond the open scan edge must be rejected"
        );
        let interior = project_one(&projector, Vec3::new(0.5, 0.25, 0.4)).unwrap();
        assert!(
            !projection_touches_boundary(&interior, &mesh.triangles, &connectivity),
            "interior surface hits must keep their correspondences"
        );
    }

    #[test]
    fn plane_with_a_hole_keeps_template_vertices_off_the_rim() {
        let (base, triangles) = plane_grid(5, 0.0);
        let (target_vertices, target_triangles) = plane_grid(7, 0.35);

        let hole_triangles: BTreeSet<usize> =
            [28, 29, 30, 31, 40, 41, 42, 43].into_iter().collect();
        let kept = target_triangles
            .iter()
            .enumerate()
            .filter_map(|(index, triangle)| (!hole_triangles.contains(&index)).then_some(*triangle))
            .collect::<Vec<_>>();
        let target = Mesh::new(
            target_vertices
                .iter()
                .map(|vertex| [vertex.x - 1.0, vertex.y - 1.0, vertex.z])
                .collect(),
            kept,
        )
        .unwrap();
        let constraints = vec![
            vertex_constraint(6, Vec3::new(1.0, 1.0, 0.35), &triangles),
            vertex_constraint(8, Vec3::new(3.0, 1.0, 0.35), &triangles),
            vertex_constraint(16, Vec3::new(1.0, 3.0, 0.35), &triangles),
            vertex_constraint(18, Vec3::new(3.0, 3.0, 0.35), &triangles),
        ];
        let options = DenseRegistrationOptions {
            stages: vec![DenseRegistrationStage {
                kind: DenseStageKind::Coarse,
                iterations: 4,

                max_distance: 2.0,
                min_normal_dot: -1.0,
                trim_fraction: 0.0,
                huber_delta: 1.0,
                correspondence_weight: 1.0,
                smoothness: 10.0,
                point_to_point: 0.1,
                landmark_weight: 30.0,
                max_step: 0.2,
            }],
            ..Default::default()
        };
        let result = nonrigid_skin_registration(
            SkinRegistrationInputs {
                initial_vertices: &base,
                base_vertices: &base,
                triangles: &triangles,
                constraints: &constraints,
                seam: &[],
                anchor_weights: &vec![0.0; base.len()],
                fade_end: 0.0,
            },
            &target,
            &options,
            |_| {},
            || false,
        )
        .unwrap();
        assert!(result.report.accepted_iterations > 0);

        let center = result.vertices[12];
        assert!(
            (center.x - 2.0).abs() < 0.15 && (center.y - 2.0).abs() < 0.15,
            "hole-center vertex drifted to the rim: {center:?}"
        );
    }

    #[test]
    fn zero_minimum_coverage_with_an_unreachable_target_completes_without_panic() {
        let (base, triangles) = plane_grid(3, 0.0);
        let target = Mesh::new(
            vec![
                [100.0, 100.0, 100.0],
                [101.0, 100.0, 100.0],
                [100.0, 101.0, 100.0],
            ],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let constraints = vec![vertex_constraint(4, Vec3::new(1.0, 1.0, 0.0), &triangles)];
        let options = DenseRegistrationOptions {
            minimum_correspondence_coverage: 0.0,
            stages: vec![DenseRegistrationStage {
                kind: DenseStageKind::Coarse,
                iterations: 2,
                max_distance: 1.0,
                min_normal_dot: -1.0,
                trim_fraction: 0.0,
                huber_delta: 1.0,
                correspondence_weight: 1.0,
                smoothness: 10.0,
                point_to_point: 0.1,
                landmark_weight: 30.0,
                max_step: 0.2,
            }],
            ..Default::default()
        };
        let result = nonrigid_skin_registration(
            SkinRegistrationInputs {
                initial_vertices: &base,
                base_vertices: &base,
                triangles: &triangles,
                constraints: &constraints,
                seam: &[],
                anchor_weights: &vec![0.0; base.len()],
                fade_end: 0.0,
            },
            &target,
            &options,
            |_| {},
            || false,
        )
        .expect("an empty correspondence set must degrade, not panic");
        assert_eq!(result.report.accepted_iterations, 0);
        assert!(result.report.iterations.iter().all(|iteration| {
            iteration.outcome == DenseIterationOutcome::InsufficientCorrespondenceCoverage
        }));
        assert_eq!(result.vertices, base);
    }

    #[test]
    fn energy_aware_blend_direction_never_violates_the_descent_bar() {
        let (vertices, triangles) = plane_grid(3, 0.0);
        let neighbors = build_neighbors(vertices.len(), &triangles);
        let correspondence_indices = (0..vertices.len()).collect::<Vec<_>>();
        let target_normals = vec![Vec3::new(0.0, 0.0, 1.0); vertices.len()];
        let data_weights = vec![1.0; vertices.len()];
        let anchor_weights = vec![0.0; vertices.len()];
        let empty_constraints: [BarycentricConstraint; 0] = [];
        let weights = uniform_weights(&neighbors);
        let system = DenseSystem::new(DenseSystemSpec {
            vertex_count: vertices.len(),
            correspondence_indices: &correspondence_indices,
            target_normals: &target_normals,
            data_weights: &data_weights,
            point_to_point: 0.1,
            neighbors: &neighbors,
            neighbor_weights: &weights,
            smoothness: 5.0,
            adaptive_stiffness: false,
            scan_support: &[],
            strict_constraints: &[],
            constraints: &empty_constraints,
            landmark_weight: 1.0,
            anchor_weights: &anchor_weights,
        });
        let closest = vertices
            .iter()
            .map(|vertex| *vertex + Vec3::new(0.0, 0.0, 0.3))
            .collect::<Vec<_>>();
        let right_hand = system.right_hand_side(&vertices, &closest, &[]);
        let before_linear_energy = squared_norm(&right_hand);
        let solve = lsmr(&system, &right_hand, LsmrOptions::default()).unwrap();
        let raw = solve
            .solution
            .chunks_exact(3)
            .map(|value| Vec3::new(value[0], value[1], value[2]))
            .collect::<Vec<_>>();

        let projected = raw.iter().map(|value| *value * 3.0).collect::<Vec<_>>();
        let (blended, lambda) =
            energy_aware_graph_blend(&system, &right_hand, before_linear_energy, &raw, &projected)
                .unwrap();
        assert!(lambda > 0.0 && lambda < 1.0, "lambda={lambda}");
        let flat = flatten_scaled(&blended, 1.0);
        let mut residual = vec![0.0; system.rows()];
        system.apply(&flat, &mut residual).unwrap();
        for (value, rhs) in residual.iter_mut().zip(&right_hand) {
            *value -= rhs;
        }
        let blended_energy = squared_norm(&residual);
        let bar = before_linear_energy * ENERGY_DESCENT_MARGIN;
        assert!(
            blended_energy <= bar + 1.0e-9,
            "blended energy {blended_energy} exceeds the descent bar {bar}"
        );

        let (kept, kept_lambda) =
            energy_aware_graph_blend(&system, &right_hand, before_linear_energy, &raw, &raw)
                .unwrap();
        assert_eq!(kept_lambda, 1.0);
        assert_eq!(kept, raw);
    }

    #[test]
    fn detached_scan_fragments_are_excluded_deterministically() {
        let mesh = Mesh::new(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
                [10.0, 0.0, 0.0],
                [11.0, 0.0, 0.0],
                [10.0, 1.0, 0.0],
            ],
            vec![[0, 1, 2], [1, 3, 2], [4, 5, 6]],
        )
        .unwrap();
        assert_eq!(largest_component_triangle_ids(&mesh), vec![0, 1]);
    }

    fn scan_with_a_feature(size: usize, height: f64) -> (Mesh, Vec<Vec3>) {
        let centre = (size - 1) as f64 * 0.5;
        let radius = (size - 1) as f64 * 0.30;
        let vertices: Vec<Vec3> = (0..size)
            .flat_map(|y| {
                (0..size).map(move |x| {
                    let (fx, fy) = (x as f64, y as f64);
                    let distance = (fx - centre).hypot(fy - centre);
                    let bump = if distance < radius {
                        height * 0.5 * (1.0 + (std::f64::consts::PI * distance / radius).cos())
                    } else {
                        0.0
                    };

                    let lean = if fy > centre {
                        height * 0.35 * (fy - centre) / centre
                    } else {
                        0.0
                    };
                    Vec3::new(fx, fy, bump + lean)
                })
            })
            .collect();
        let (_, triangles) = plane_grid(size, 0.0);
        let mesh = Mesh::new(
            vertices.iter().copied().map(Vec3::to_array).collect(),
            triangles,
        )
        .unwrap();
        (mesh, vertices)
    }

    #[test]
    fn the_fit_reproduces_a_feature_the_template_does_not_have() {
        const SIZE: usize = 11;
        const HEIGHT: f64 = 1.4;
        let (base, triangles) = plane_grid(SIZE, 0.0);
        let (target, wanted) = scan_with_a_feature(SIZE, HEIGHT);
        let fit_with = |adaptive: bool| {
            let options = DenseRegistrationOptions {
                stages: dense_stages_at_fidelity(1.0),
                adaptive_stiffness: adaptive,
                ..Default::default()
            };

            let corner = |x: usize, y: usize, along: usize, down: usize| {
                let vertex = y * SIZE + x;
                BarycentricConstraint::new(
                    [vertex, along, down],
                    [1.0, 0.0, 0.0],
                    wanted[vertex],
                    1.0,
                    1.0,
                )
                .unwrap()
            };
            let constraints = [
                corner(0, 0, 1, SIZE),
                corner(SIZE - 1, 0, SIZE - 2, 2 * SIZE - 1),
                corner(0, SIZE - 1, (SIZE - 1) * SIZE + 1, (SIZE - 2) * SIZE),
                corner(SIZE - 1, SIZE - 1, SIZE * SIZE - 2, (SIZE - 1) * SIZE - 1),
            ];
            nonrigid_skin_registration(
                SkinRegistrationInputs {
                    initial_vertices: &base,
                    base_vertices: &base,
                    triangles: &triangles,
                    constraints: &constraints,
                    seam: &[],
                    anchor_weights: &vec![0.0; base.len()],
                    fade_end: 0.0,
                },
                &target,
                &options,
                |_| {},
                || false,
            )
            .expect("the schedule must run on a clean synthetic pair")
        };

        let apex = SIZE / 2 * SIZE + SIZE / 2;
        let result = fit_with(true);
        let uniform = fit_with(false);
        let reproduced = result.vertices[apex].z / wanted[apex].z;

        let with_uniform = uniform.vertices[apex].z / wanted[apex].z;
        assert!(
            reproduced > with_uniform,
            "adaptive stiffness recovered {:.1}% against {:.1}% uniform, so it              is not earning its place",
            reproduced * 100.0,
            with_uniform * 100.0
        );
        assert!(
            reproduced > 0.80,
            "only {:.0}% of the feature survived the fit ({:.3} of {:.3})",
            reproduced * 100.0,
            result.vertices[apex].z,
            wanted[apex].z
        );

        let squared: f64 = result
            .vertices
            .iter()
            .zip(&wanted)
            .map(|(fitted, want)| {
                let delta = *fitted - *want;
                delta.dot(delta)
            })
            .sum();
        let rms = (squared / result.vertices.len() as f64).sqrt();
        assert!(
            rms < HEIGHT * 0.12,
            "the fitted surface is {rms:.4} from the scan, which is not a fit"
        );
    }

    #[test]
    fn synthetic_skin_moves_toward_scan_without_moving_the_seam() {
        let (base, triangles) = plane_grid(5, 0.0);
        let initial = base.clone();
        let target = enlarged_plane_target(5, 0.35);
        let constraints = vec![
            BarycentricConstraint::new(
                [18, 19, 24],
                [0.2, 0.3, 0.5],
                Vec3::new(3.3, 3.5, 0.35),
                1.0,
                1.0,
            )
            .unwrap(),
            BarycentricConstraint::new(
                [15, 16, 21],
                [0.2, 0.3, 0.5],
                Vec3::new(0.8, 3.5, 0.35),
                1.0,
                1.0,
            )
            .unwrap(),
        ];
        let seam = (0..5).collect::<Vec<_>>();
        let mut anchors = vec![0.0; base.len()];
        for &vertex in &seam {
            anchors[vertex] = 1_000_000.0;
        }
        let options = DenseRegistrationOptions {
            stages: vec![DenseRegistrationStage {
                iterations: 3,
                max_distance: 1.0,
                min_normal_dot: -1.0,
                trim_fraction: 0.0,
                huber_delta: 1.0,
                correspondence_weight: 1.0,
                smoothness: 10.0,
                point_to_point: 0.1,
                landmark_weight: 30.0,
                max_step: 0.2,
                kind: DenseStageKind::Coarse,
            }],
            ..Default::default()
        };
        let result = nonrigid_skin_registration(
            SkinRegistrationInputs {
                initial_vertices: &initial,
                base_vertices: &base,
                triangles: &triangles,
                constraints: &constraints,
                seam: &seam,
                anchor_weights: &anchors,
                fade_end: 0.0,
            },
            &target,
            &options,
            |_| {},
            || false,
        )
        .unwrap();
        assert!(result.report.accepted_iterations > 0);
        assert!(result.vertices[24].z > 0.05);
        for &vertex in &seam {
            assert_eq!(result.vertices[vertex], base[vertex]);
        }
        let unsafe_mask = safety_mask(&base, &result.vertices, &triangles, &options).unwrap();
        assert!(unsafe_mask.iter().all(|unsafe_triangle| !unsafe_triangle));
    }

    #[test]
    fn zero_through_three_user_pins_always_select_a_finite_cumulative_landmark_guard() {
        let (base, triangles) = plane_grid(5, 0.0);
        let target_vertices = base
            .iter()
            .map(|vertex| Vec3::new(vertex.x, vertex.y, 0.35))
            .collect::<Vec<_>>();
        let target = enlarged_plane_target(5, 0.35);
        let constraints = [6, 8, 16, 18]
            .into_iter()
            .map(|vertex| vertex_constraint(vertex, target_vertices[vertex], &triangles))
            .collect::<Vec<_>>();
        let seam = (0..5).collect::<Vec<_>>();
        let mut anchors = vec![0.0; base.len()];
        for &vertex in &seam {
            anchors[vertex] = 1_000_000.0;
        }

        for user_pin_count in 0..=3 {
            let options = DenseRegistrationOptions {
                strict_landmark_count: user_pin_count,
                stages: vec![DenseRegistrationStage {
                    kind: DenseStageKind::Coarse,
                    iterations: 2,
                    max_distance: 1.0,
                    min_normal_dot: -1.0,
                    trim_fraction: 0.0,
                    huber_delta: 1.0,
                    correspondence_weight: 1.0,
                    smoothness: 10.0,
                    point_to_point: 0.1,
                    landmark_weight: 30.0,
                    max_step: 0.2,
                }],
                ..Default::default()
            };
            let result = nonrigid_skin_registration(
                SkinRegistrationInputs {
                    initial_vertices: &base,
                    base_vertices: &base,
                    triangles: &triangles,
                    constraints: &constraints,
                    seam: &seam,
                    anchor_weights: &anchors,
                    fade_end: 0.0,
                },
                &target,
                &options,
                |_| {},
                || false,
            )
            .unwrap();
            let report = &result.report;
            assert_eq!(report.strict_user_landmark_count, user_pin_count);
            assert_eq!(
                report.automatic_landmark_count,
                constraints.len() - user_pin_count
            );
            assert_eq!(
                report.automatic_landmark_guard_fallback,
                user_pin_count == 0
            );
            assert!(report.landmark_guard_limit_cm.is_finite());
            assert!((report.landmark_guard_limit_cm - 0.357).abs() <= 1.0e-12);
            for iteration in report
                .iterations
                .iter()
                .filter(|iteration| iteration.outcome == DenseIterationOutcome::Accepted)
            {
                let guarded_rms = if user_pin_count == 0 {
                    iteration.automatic_landmark_rms_after.unwrap()
                } else {
                    iteration.strict_user_landmark_rms_after.unwrap()
                };
                assert!(guarded_rms <= report.landmark_guard_limit_cm + 1.0e-12);
            }
        }
    }

    #[test]
    fn distinctive_lower_face_beats_the_legacy_canonical_biased_schedule() {
        const SIZE: usize = 7;
        let (base, triangles) = plane_grid(SIZE, 0.0);
        let center_x = (SIZE - 1) as f64 * 0.5;
        let deform = |x: f64, y: f64| {
            let lower_face = (1.0 - (y - 2.0).abs() / 2.0).max(0.0);
            let lateral = (x - center_x).abs() / center_x;
            Vec3::new(
                center_x + (x - center_x) * (1.0 + 0.32 * lower_face),
                y,
                0.72 * lower_face * (1.0 - 0.12 * lateral),
            )
        };
        let target_vertices = (0..base.len())
            .map(|index| deform((index % SIZE) as f64, (index / SIZE) as f64))
            .collect::<Vec<_>>();

        let (extended, extended_triangles) = plane_grid(SIZE + 2, 0.0);
        let target = Mesh::new(
            extended
                .iter()
                .map(|vertex| deform(vertex.x - 1.0, vertex.y - 1.0).to_array())
                .collect(),
            extended_triangles,
        )
        .unwrap();
        let pin_vertices = [2 * SIZE, 2 * SIZE + 3, 2 * SIZE + 6, 3 * SIZE + 3];
        let constraints = pin_vertices
            .iter()
            .map(|&vertex| vertex_constraint(vertex, target_vertices[vertex], &triangles))
            .collect::<Vec<_>>();
        let seam = (0..SIZE).collect::<Vec<_>>();
        let mut anchors = vec![0.0; base.len()];
        for &vertex in &seam {
            anchors[vertex] = 1_000_000.0;
        }

        let improved = nonrigid_skin_registration(
            SkinRegistrationInputs {
                initial_vertices: &base,
                base_vertices: &base,
                triangles: &triangles,
                constraints: &constraints,
                seam: &seam,
                anchor_weights: &anchors,
                fade_end: 0.0,
            },
            &target,
            &DenseRegistrationOptions::default(),
            |_| {},
            || false,
        )
        .unwrap();
        let legacy = nonrigid_skin_registration(
            SkinRegistrationInputs {
                initial_vertices: &base,
                base_vertices: &base,
                triangles: &triangles,
                constraints: &constraints,
                seam: &seam,
                anchor_weights: &anchors,
                fade_end: 0.0,
            },
            &target,
            &DenseRegistrationOptions {
                stages: legacy_canonical_biased_stages(),
                ..Default::default()
            },
            |_| {},
            || false,
        )
        .unwrap();

        let lower_face = (SIZE..4 * SIZE).collect::<Vec<_>>();
        let canonical_error = indexed_rms(&base, &target_vertices, &lower_face);
        let improved_error = indexed_rms(&improved.vertices, &target_vertices, &lower_face);
        let legacy_error = indexed_rms(&legacy.vertices, &target_vertices, &lower_face);
        assert!(
            improved_error < canonical_error * 0.80,
            "custom lower face remained too canonical: {improved_error} vs {canonical_error}"
        );
        assert!(
            improved_error < legacy_error * 0.92,
            "new schedule {improved_error} did not beat legacy {legacy_error}"
        );
        assert!(
            weighted_landmark_rms(&improved.vertices, &constraints)
                <= weighted_landmark_rms(&legacy.vertices, &constraints) + 1.0e-6
        );
        let strict_limit = (weighted_landmark_rms(&base, &constraints) * 1.02).max(0.005);
        for iteration in improved
            .report
            .iterations
            .iter()
            .filter(|iteration| iteration.outcome == DenseIterationOutcome::Accepted)
        {
            assert!(iteration.strict_user_landmark_rms_after.unwrap() <= strict_limit + 1.0e-12);
            assert!(iteration.exterior_lower_p95_before.unwrap().is_finite());
            assert!(iteration.exterior_lower_p95_after.unwrap().is_finite());
        }
        assert!(improved.vertices.iter().all(|vertex| vertex.is_finite()));
        for &vertex in &seam {
            assert_eq!(improved.vertices[vertex], base[vertex]);
        }
        assert!(
            safety_mask(
                &base,
                &improved.vertices,
                &triangles,
                &DenseRegistrationOptions::default()
            )
            .unwrap()
            .iter()
            .all(|unsafe_triangle| !unsafe_triangle)
        );
    }
}
