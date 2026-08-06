use std::collections::BTreeSet;

use thiserror::Error;

use crate::math::Vec3;

use super::{
    LinearOperator, LsmrError, LsmrOptions, LsmrStop, OperatorError, StackedOperator, lsmr,
};

pub const MINIMUM_EFFECTIVE_LANDMARK_WEIGHT: f64 = 0.05;
pub const MAXIMUM_EFFECTIVE_LANDMARK_WEIGHT: f64 = 25.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarycentricConstraint {
    pub vertex_indices: [usize; 3],
    pub barycentric: [f64; 3],
    pub target: Vec3,
    pub effective_weight: f64,
}

impl BarycentricConstraint {
    pub fn new(
        vertex_indices: [usize; 3],
        barycentric: [f64; 3],
        target: Vec3,
        fit_weight: f64,
        confidence: f64,
    ) -> Result<Self, LandmarkWarpError> {
        if !target.is_finite() {
            return Err(LandmarkWarpError::NonFiniteConstraintTarget);
        }
        if !fit_weight.is_finite() || fit_weight <= 0.0 {
            return Err(LandmarkWarpError::InvalidFitWeight);
        }
        if !confidence.is_finite() || confidence <= 0.0 {
            return Err(LandmarkWarpError::InvalidConfidence);
        }
        if barycentric.iter().any(|value| !value.is_finite()) {
            return Err(LandmarkWarpError::NonFiniteBarycentric);
        }
        if barycentric.iter().any(|value| *value < 0.0) {
            return Err(LandmarkWarpError::NegativeBarycentric);
        }
        let barycentric_sum: f64 = barycentric.iter().sum();
        if (barycentric_sum - 1.0).abs() > 1.0e-6 {
            return Err(LandmarkWarpError::BarycentricSum(barycentric_sum));
        }
        if vertex_indices[0] == vertex_indices[1]
            || vertex_indices[0] == vertex_indices[2]
            || vertex_indices[1] == vertex_indices[2]
        {
            return Err(LandmarkWarpError::DuplicateConstraintVertex);
        }

        let mut pairs = [
            (vertex_indices[0], barycentric[0]),
            (vertex_indices[1], barycentric[1]),
            (vertex_indices[2], barycentric[2]),
        ];
        pairs.sort_by_key(|pair| pair.0);

        let raw_weight = fit_weight * confidence;
        Ok(Self {
            vertex_indices: [pairs[0].0, pairs[1].0, pairs[2].0],
            barycentric: [pairs[0].1, pairs[1].1, pairs[2].1],
            target,
            effective_weight: raw_weight.clamp(
                MINIMUM_EFFECTIVE_LANDMARK_WEIGHT,
                MAXIMUM_EFFECTIVE_LANDMARK_WEIGHT,
            ),
        })
    }

    fn interpolate(self, vertices: &[Vec3]) -> Vec3 {
        vertices[self.vertex_indices[0]] * self.barycentric[0]
            + vertices[self.vertex_indices[1]] * self.barycentric[1]
            + vertices[self.vertex_indices[2]] * self.barycentric[2]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UniformLaplacianOperator {
    neighbors: Vec<Vec<usize>>,
    scale: f64,
}

impl UniformLaplacianOperator {
    pub fn new(
        vertex_count: usize,
        triangles: &[[usize; 3]],
        scale: f64,
    ) -> Result<Self, LandmarkWarpError> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err(LandmarkWarpError::InvalidOperatorScale);
        }
        let mut adjacency = vec![BTreeSet::new(); vertex_count];
        for (triangle_index, triangle) in triangles.iter().copied().enumerate() {
            for vertex in triangle {
                if vertex >= vertex_count {
                    return Err(LandmarkWarpError::TriangleVertexOutOfBounds {
                        triangle: triangle_index,
                        vertex,
                        vertex_count,
                    });
                }
            }
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
        Ok(Self {
            neighbors: adjacency
                .into_iter()
                .map(|neighbors| neighbors.into_iter().collect())
                .collect(),
            scale,
        })
    }

    #[must_use]
    pub fn neighbors(&self, vertex: usize) -> Option<&[usize]> {
        self.neighbors.get(vertex).map(Vec::as_slice)
    }
}

impl LinearOperator for UniformLaplacianOperator {
    fn rows(&self) -> usize {
        self.neighbors.len()
    }

    fn columns(&self) -> usize {
        self.neighbors.len()
    }

    fn apply(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        validate_vectors(self, input, output, "laplacian")?;
        for (vertex, result) in output.iter_mut().enumerate() {
            let neighbors = &self.neighbors[vertex];
            let inverse_degree = if neighbors.is_empty() {
                0.0
            } else {
                1.0 / neighbors.len() as f64
            };

            let mut value = 0.0;
            for &neighbor in neighbors {
                value -= inverse_degree * input[neighbor];
            }
            value += input[vertex];
            *result = self.scale * value;
        }
        Ok(())
    }

    fn apply_transpose(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        validate_transpose_vectors(self, input, output, "laplacian")?;
        output.fill(0.0);
        self.apply_transpose_add(input, output)
    }

    fn apply_transpose_add(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        validate_transpose_vectors(self, input, output, "laplacian accumulation")?;
        for (vertex, &value) in input.iter().enumerate() {
            let neighbors = &self.neighbors[vertex];
            let scaled = self.scale * value;
            let inverse_degree = if neighbors.is_empty() {
                0.0
            } else {
                1.0 / neighbors.len() as f64
            };
            for &neighbor in neighbors {
                output[neighbor] -= inverse_degree * scaled;
            }
            output[vertex] += scaled;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BarycentricLandmarkOperator {
    vertex_count: usize,
    constraints: Vec<BarycentricConstraint>,
    row_scales: Vec<f64>,
}

impl BarycentricLandmarkOperator {
    pub fn new(
        vertex_count: usize,
        constraints: &[BarycentricConstraint],
        landmark_weight: f64,
    ) -> Result<Self, LandmarkWarpError> {
        if constraints.is_empty() {
            return Err(LandmarkWarpError::NoLandmarkConstraints);
        }
        if !landmark_weight.is_finite() || landmark_weight <= 0.0 {
            return Err(LandmarkWarpError::InvalidLandmarkWeight);
        }
        for (constraint_index, constraint) in constraints.iter().enumerate() {
            for &vertex in &constraint.vertex_indices {
                if vertex >= vertex_count {
                    return Err(LandmarkWarpError::ConstraintVertexOutOfBounds {
                        constraint: constraint_index,
                        vertex,
                        vertex_count,
                    });
                }
            }
        }
        Ok(Self {
            vertex_count,
            constraints: constraints.to_vec(),
            row_scales: constraints
                .iter()
                .map(|constraint| (landmark_weight * constraint.effective_weight).sqrt())
                .collect(),
        })
    }

    fn coordinate_right_hand_side(&self, base: &[Vec3], coordinate: usize) -> Vec<f64> {
        self.constraints
            .iter()
            .zip(self.row_scales.iter().copied())
            .map(|(constraint, scale)| {
                let source = coordinate_value(constraint.interpolate(base), coordinate);
                let target = coordinate_value(constraint.target, coordinate);
                (target - source) * scale
            })
            .collect()
    }
}

impl LinearOperator for BarycentricLandmarkOperator {
    fn rows(&self) -> usize {
        self.constraints.len()
    }

    fn columns(&self) -> usize {
        self.vertex_count
    }

    fn apply(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        validate_vectors(self, input, output, "landmark")?;
        for (row, result) in output.iter_mut().enumerate() {
            let constraint = self.constraints[row];
            let mut value = 0.0;
            for corner in 0..3 {
                value += constraint.barycentric[corner] * input[constraint.vertex_indices[corner]];
            }
            *result = self.row_scales[row] * value;
        }
        Ok(())
    }

    fn apply_transpose(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        validate_transpose_vectors(self, input, output, "landmark")?;
        output.fill(0.0);
        self.apply_transpose_add(input, output)
    }

    fn apply_transpose_add(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        validate_transpose_vectors(self, input, output, "landmark accumulation")?;
        for (row, &input_value) in input.iter().enumerate() {
            let constraint = self.constraints[row];
            let scaled = self.row_scales[row] * input_value;
            for corner in 0..3 {
                output[constraint.vertex_indices[corner]] +=
                    constraint.barycentric[corner] * scaled;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnchorOperator {
    vertex_count: usize,
    indices: Vec<usize>,
    scales: Vec<f64>,
}

impl AnchorOperator {
    pub fn from_weights(weights: &[f64]) -> Result<Self, LandmarkWarpError> {
        let mut indices = Vec::new();
        let mut scales = Vec::new();
        for (index, weight) in weights.iter().copied().enumerate() {
            if !weight.is_finite() || weight < 0.0 {
                return Err(LandmarkWarpError::InvalidAnchorWeight(index));
            }
            if weight > 0.0 {
                indices.push(index);
                scales.push(weight.sqrt());
            }
        }
        Ok(Self {
            vertex_count: weights.len(),
            indices,
            scales,
        })
    }

    #[must_use]
    pub fn indices(&self) -> &[usize] {
        &self.indices
    }
}

impl LinearOperator for AnchorOperator {
    fn rows(&self) -> usize {
        self.indices.len()
    }

    fn columns(&self) -> usize {
        self.vertex_count
    }

    fn apply(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        validate_vectors(self, input, output, "anchor")?;
        for (row, result) in output.iter_mut().enumerate() {
            *result = self.scales[row] * input[self.indices[row]];
        }
        Ok(())
    }

    fn apply_transpose(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        validate_transpose_vectors(self, input, output, "anchor")?;
        output.fill(0.0);
        self.apply_transpose_add(input, output)
    }

    fn apply_transpose_add(&self, input: &[f64], output: &mut [f64]) -> Result<(), OperatorError> {
        validate_transpose_vectors(self, input, output, "anchor accumulation")?;
        for (row, &value) in input.iter().enumerate() {
            output[self.indices[row]] += self.scales[row] * value;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LandmarkWarpOptions {
    pub landmark_weight: f64,
    pub laplacian_weight: f64,
    pub minimum_line_search_tau: f64,
    pub lsmr: LsmrOptions,
}

impl Default for LandmarkWarpOptions {
    fn default() -> Self {
        Self {
            landmark_weight: 100.0,
            laplacian_weight: 20.0,
            minimum_line_search_tau: 1.0 / 64.0,
            lsmr: LsmrOptions {
                damping: 0.0,
                absolute_tolerance: 1.0e-8,
                relative_tolerance: 1.0e-8,
                condition_limit: 1.0e10,
                max_iterations: Some(600),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WarpSolveReport {
    pub stop: LsmrStop,
    pub iterations: usize,
    pub condition_estimate: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LandmarkWarpReport {
    pub line_search_tau: f64,
    pub landmark_rms: f64,
    pub landmark_p95: f64,
    pub landmark_weighted_rms: f64,
    pub max_displacement: f64,
    pub solves: [WarpSolveReport; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct LandmarkWarpResult {
    pub vertices: Vec<Vec3>,
    pub report: LandmarkWarpReport,
}

#[derive(Debug, Error, PartialEq)]
pub enum LandmarkWarpError {
    #[error("the warp requires at least one base vertex")]
    EmptyVertices,
    #[error("the warp requires at least one barycentric landmark constraint")]
    NoLandmarkConstraints,
    #[error("constraint target is non-finite")]
    NonFiniteConstraintTarget,
    #[error("fit weight must be finite and positive")]
    InvalidFitWeight,
    #[error("confidence must be finite and positive")]
    InvalidConfidence,
    #[error("barycentric coordinates must be finite")]
    NonFiniteBarycentric,
    #[error("barycentric coordinates must be non-negative")]
    NegativeBarycentric,
    #[error("barycentric coordinates sum to {0}; expected one")]
    BarycentricSum(f64),
    #[error("a barycentric constraint repeats one of its triangle vertices")]
    DuplicateConstraintVertex,
    #[error("landmark weight must be finite and positive")]
    InvalidLandmarkWeight,
    #[error("Laplacian weight must be finite and positive")]
    InvalidLaplacianWeight,
    #[error("operator scale must be finite and positive")]
    InvalidOperatorScale,
    #[error("minimum line-search tau must be finite and in (0, 1]")]
    InvalidMinimumTau,
    #[error("anchor weight {0} must be finite and non-negative")]
    InvalidAnchorWeight(usize),
    #[error("anchor weight count {actual} does not match vertex count {expected}")]
    AnchorCountMismatch { expected: usize, actual: usize },
    #[error("triangle {triangle} references vertex {vertex}, but there are {vertex_count}")]
    TriangleVertexOutOfBounds {
        triangle: usize,
        vertex: usize,
        vertex_count: usize,
    },
    #[error(
        "landmark constraint {constraint} references vertex {vertex}, but there are {vertex_count}"
    )]
    ConstraintVertexOutOfBounds {
        constraint: usize,
        vertex: usize,
        vertex_count: usize,
    },
    #[error("seam vertex {0} is outside the vertex array")]
    SeamVertexOutOfBounds(usize),
    #[error("base and delta vertex counts differ ({base} != {delta})")]
    DeltaCountMismatch { base: usize, delta: usize },
    #[error("base vertex {0} is non-finite")]
    NonFiniteBaseVertex(usize),
    #[error("delta vertex {0} is non-finite")]
    NonFiniteDeltaVertex(usize),
    #[error("topology guard rejected every step through the minimum tau")]
    TopologyLineSearchFailed,
    #[error(transparent)]
    Operator(#[from] OperatorError),
    #[error(transparent)]
    Lsmr(#[from] LsmrError),
}

pub fn landmark_laplacian_warp<F>(
    base_vertices: &[Vec3],
    triangles: &[[usize; 3]],
    constraints: &[BarycentricConstraint],
    seam: &[usize],
    anchor_weights: &[f64],
    options: LandmarkWarpOptions,
    topology_guard: F,
) -> Result<LandmarkWarpResult, LandmarkWarpError>
where
    F: FnMut(&[Vec3]) -> bool,
{
    if base_vertices.is_empty() {
        return Err(LandmarkWarpError::EmptyVertices);
    }
    validate_finite_vertices(base_vertices, true)?;
    if anchor_weights.len() != base_vertices.len() {
        return Err(LandmarkWarpError::AnchorCountMismatch {
            expected: base_vertices.len(),
            actual: anchor_weights.len(),
        });
    }
    if !options.landmark_weight.is_finite() || options.landmark_weight <= 0.0 {
        return Err(LandmarkWarpError::InvalidLandmarkWeight);
    }
    if !options.laplacian_weight.is_finite() || options.laplacian_weight <= 0.0 {
        return Err(LandmarkWarpError::InvalidLaplacianWeight);
    }
    validate_minimum_tau(options.minimum_line_search_tau)?;
    validate_seam(seam, base_vertices.len())?;

    let landmark_operator = BarycentricLandmarkOperator::new(
        base_vertices.len(),
        constraints,
        options.landmark_weight,
    )?;
    let laplacian_operator = UniformLaplacianOperator::new(
        base_vertices.len(),
        triangles,
        options.laplacian_weight.sqrt(),
    )?;
    let anchor_operator = AnchorOperator::from_weights(anchor_weights)?;
    let system = StackedOperator::new(vec![
        &landmark_operator,
        &laplacian_operator,
        &anchor_operator,
    ])?;

    let mut deltas = vec![Vec3::ZERO; base_vertices.len()];
    let mut solve_reports = Vec::with_capacity(3);
    for coordinate in 0..3 {
        let mut right_hand = vec![0.0; system.rows()];
        let landmark_rhs = landmark_operator.coordinate_right_hand_side(base_vertices, coordinate);
        right_hand[..landmark_rhs.len()].copy_from_slice(&landmark_rhs);
        let solve = lsmr(&system, &right_hand, options.lsmr)?;
        for (vertex, value) in solve.solution.iter().copied().enumerate() {
            set_coordinate(&mut deltas[vertex], coordinate, value);
        }
        solve_reports.push(WarpSolveReport {
            stop: solve.stop,
            iterations: solve.iterations,
            condition_estimate: solve.condition_estimate,
        });
    }
    for &vertex in seam {
        deltas[vertex] = Vec3::ZERO;
    }
    let solves: [WarpSolveReport; 3] = solve_reports
        .try_into()
        .expect("exactly three coordinate solves are produced");
    let (vertices, line_search_tau) = topology_safe_step(
        base_vertices,
        &deltas,
        seam,
        options.minimum_line_search_tau,
        topology_guard,
    )?;

    let residuals: Vec<_> = constraints
        .iter()
        .map(|constraint| (constraint.interpolate(&vertices) - constraint.target).norm())
        .collect();
    let landmark_rms =
        (residuals.iter().map(|value| value * value).sum::<f64>() / residuals.len() as f64).sqrt();
    let total_weight: f64 = constraints
        .iter()
        .map(|constraint| constraint.effective_weight)
        .sum();
    let landmark_weighted_rms = (constraints
        .iter()
        .zip(&residuals)
        .map(|(constraint, residual)| constraint.effective_weight * residual * residual)
        .sum::<f64>()
        / total_weight)
        .sqrt();
    let max_displacement = vertices
        .iter()
        .zip(base_vertices)
        .map(|(candidate, base)| (*candidate - *base).norm())
        .fold(0.0_f64, f64::max);

    Ok(LandmarkWarpResult {
        vertices,
        report: LandmarkWarpReport {
            line_search_tau,
            landmark_rms,
            landmark_p95: linear_quantile(&residuals, 0.95),
            landmark_weighted_rms,
            max_displacement,
            solves,
        },
    })
}

pub fn topology_safe_step<F>(
    base_vertices: &[Vec3],
    deltas: &[Vec3],
    seam: &[usize],
    minimum_tau: f64,
    mut topology_guard: F,
) -> Result<(Vec<Vec3>, f64), LandmarkWarpError>
where
    F: FnMut(&[Vec3]) -> bool,
{
    if base_vertices.len() != deltas.len() {
        return Err(LandmarkWarpError::DeltaCountMismatch {
            base: base_vertices.len(),
            delta: deltas.len(),
        });
    }
    validate_finite_vertices(base_vertices, true)?;
    validate_finite_vertices(deltas, false)?;
    validate_minimum_tau(minimum_tau)?;
    validate_seam(seam, base_vertices.len())?;
    let mut tau = 1.0;
    while tau >= minimum_tau {
        let mut candidate: Vec<_> = base_vertices
            .iter()
            .copied()
            .zip(deltas.iter().copied())
            .map(|(base, delta)| base + delta * tau)
            .collect();
        for &vertex in seam {
            candidate[vertex] = base_vertices[vertex];
        }
        if topology_guard(&candidate) {
            return Ok((candidate, tau));
        }
        tau *= 0.5;
    }
    Err(LandmarkWarpError::TopologyLineSearchFailed)
}

fn validate_vectors<O: LinearOperator + ?Sized>(
    operator: &O,
    input: &[f64],
    output: &[f64],
    label: &'static str,
) -> Result<(), OperatorError> {
    if input.len() != operator.columns() {
        return Err(OperatorError::VectorLength {
            kind: label,
            expected: operator.columns(),
            actual: input.len(),
        });
    }
    if output.len() != operator.rows() {
        return Err(OperatorError::VectorLength {
            kind: label,
            expected: operator.rows(),
            actual: output.len(),
        });
    }
    Ok(())
}

fn validate_transpose_vectors<O: LinearOperator + ?Sized>(
    operator: &O,
    input: &[f64],
    output: &[f64],
    label: &'static str,
) -> Result<(), OperatorError> {
    if input.len() != operator.rows() {
        return Err(OperatorError::VectorLength {
            kind: label,
            expected: operator.rows(),
            actual: input.len(),
        });
    }
    if output.len() != operator.columns() {
        return Err(OperatorError::VectorLength {
            kind: label,
            expected: operator.columns(),
            actual: output.len(),
        });
    }
    Ok(())
}

fn validate_seam(seam: &[usize], vertex_count: usize) -> Result<(), LandmarkWarpError> {
    if let Some(vertex) = seam.iter().copied().find(|vertex| *vertex >= vertex_count) {
        return Err(LandmarkWarpError::SeamVertexOutOfBounds(vertex));
    }
    Ok(())
}

fn validate_minimum_tau(value: f64) -> Result<(), LandmarkWarpError> {
    if !value.is_finite() || value <= 0.0 || value > 1.0 {
        return Err(LandmarkWarpError::InvalidMinimumTau);
    }
    Ok(())
}

fn validate_finite_vertices(vertices: &[Vec3], base: bool) -> Result<(), LandmarkWarpError> {
    if let Some(index) = vertices.iter().position(|vertex| !vertex.is_finite()) {
        if base {
            Err(LandmarkWarpError::NonFiniteBaseVertex(index))
        } else {
            Err(LandmarkWarpError::NonFiniteDeltaVertex(index))
        }
    } else {
        Ok(())
    }
}

fn coordinate_value(value: Vec3, coordinate: usize) -> f64 {
    match coordinate {
        0 => value.x,
        1 => value.y,
        2 => value.z,
        _ => unreachable!("the warp has exactly three coordinates"),
    }
}

fn set_coordinate(value: &mut Vec3, coordinate: usize, coordinate_value: f64) {
    match coordinate {
        0 => value.x = coordinate_value,
        1 => value.y = coordinate_value,
        2 => value.z = coordinate_value,
        _ => unreachable!("the warp has exactly three coordinates"),
    }
}

fn linear_quantile(values: &[f64], quantile: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let position = quantile * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let fraction = position - lower as f64;
        sorted[lower] * (1.0 - fraction) + sorted[upper] * fraction
    }
}
