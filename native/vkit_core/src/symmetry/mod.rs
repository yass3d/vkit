mod mirror_map;

pub use mirror_map::MirrorMap;

use thiserror::Error;

use crate::formats::Mesh;
use crate::spatial::{SurfaceProjector, SurfaceProjectorError};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SymmetryMode {
    #[default]
    Off,
    PositiveX,
    NegativeX,
}

#[derive(Clone, Copy, Debug)]
pub struct SymmetryOptions {
    pub mode: SymmetryMode,
    pub center_x: f64,
    pub seam_width: Option<f64>,
    pub tolerance: Option<f64>,
    pub minimum_coverage: f64,
}

impl Default for SymmetryOptions {
    fn default() -> Self {
        Self {
            mode: SymmetryMode::Off,
            center_x: 0.0,
            seam_width: None,
            tolerance: None,
            minimum_coverage: 0.95,
        }
    }
}

impl SymmetryOptions {
    pub fn preapplied(mode: SymmetryMode, center_x: f64) -> Self {
        Self {
            mode,
            center_x,
            seam_width: Some(f64::NEG_INFINITY),
            ..Self::default()
        }
    }

    pub fn source_preapplied(self) -> bool {
        self.mode != SymmetryMode::Off && self.seam_width == Some(f64::NEG_INFINITY)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymmetryReason {
    ModeOff,
    SourcePreapplied,
    NoTargetVertices,
    NoBaseSurface,
    InsufficientCounterpartCoverage,
    Symmetrized,
    SymmetrizedBeyondTolerance,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SymmetryReceipt {
    pub accepted: bool,
    pub applied: bool,
    pub reason: SymmetryReason,
    pub mode: SymmetryMode,
    pub center_x: f64,
    pub seam_width: f64,
    pub tolerance: f64,
    pub minimum_coverage: f64,
    pub target_vertex_count: usize,
    pub projected_vertex_count: usize,
    pub rejected_vertex_count: usize,
    pub coverage: f64,
    pub mean_symmetry_error: Option<f64>,
    pub max_symmetry_error: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SymmetrizedMesh {
    pub mesh: Mesh,
    pub receipt: SymmetryReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MirrorPinOptions {
    pub center_x: f64,
    pub centerline_tolerance_ratio: f64,
    pub counterpart_tolerance_ratio: f64,
}

impl Default for MirrorPinOptions {
    fn default() -> Self {
        Self {
            center_x: 0.0,
            centerline_tolerance_ratio: 0.001,
            counterpart_tolerance_ratio: 0.04,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MirrorPinReason {
    CounterpartAccepted,
    CenterlineSingle,
    RejectedNonFiniteInput,
    RejectedInvalidTolerance,
    RejectedInvalidSurface,
    RejectedNoOppositeSurface,
    RejectedProjectionUnavailable,
    RejectedWrongSide,
    RejectedAsymmetricCounterpart,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MirrorPinProjection {
    pub accepted: bool,
    pub create_counterpart: bool,
    pub reason: MirrorPinReason,
    pub triangle_id: Option<u32>,
    pub barycentric: Option<[f64; 3]>,
    pub projected_point: Option<[f64; 3]>,
    pub reflected_point: Option<[f64; 3]>,
    pub projection_error: Option<f64>,
    pub tolerance: Option<f64>,
    pub centerline_tolerance: Option<f64>,
}

impl MirrorPinProjection {
    fn rejected(reason: MirrorPinReason) -> Self {
        Self {
            accepted: false,
            create_counterpart: false,
            reason,
            triangle_id: None,
            barycentric: None,
            projected_point: None,
            reflected_point: None,
            projection_error: None,
            tolerance: None,
            centerline_tolerance: None,
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum SymmetryError {
    #[error("center X, seam width, tolerance, and coverage must be finite")]
    NonFiniteOption,
    #[error("minimum coverage must lie between zero and one")]
    InvalidCoverage,
    #[error("seam width must be non-negative")]
    InvalidSeamWidth,
    #[error("tolerance must be positive")]
    InvalidTolerance,
    #[error("the mesh has no surface on the side being kept")]
    NothingToMirror,
    #[error("the mirrored mesh could not be assembled: {0}")]
    Rebuild(String),
    #[error("the mirrored mesh needs more vertices than an index can hold")]
    MeshTooLarge,
    #[error(transparent)]
    Projector(#[from] SurfaceProjectorError),
}

pub fn project_mirrored_surface_pin_x(
    mesh: &Mesh,
    picked_point: [f64; 3],
    options: MirrorPinOptions,
) -> MirrorPinProjection {
    if picked_point
        .iter()
        .any(|coordinate| !coordinate.is_finite())
        || !options.center_x.is_finite()
        || !options.centerline_tolerance_ratio.is_finite()
        || !options.counterpart_tolerance_ratio.is_finite()
    {
        return MirrorPinProjection::rejected(MirrorPinReason::RejectedNonFiniteInput);
    }
    if options.centerline_tolerance_ratio < 0.0 || options.counterpart_tolerance_ratio <= 0.0 {
        return MirrorPinProjection::rejected(MirrorPinReason::RejectedInvalidTolerance);
    }
    if mesh.vertices.is_empty()
        || mesh.triangles.is_empty()
        || mesh
            .vertices
            .iter()
            .flatten()
            .any(|coordinate| !coordinate.is_finite())
        || mesh.triangles.iter().flatten().any(|&vertex_id| {
            usize::try_from(vertex_id)
                .map(|index| index >= mesh.vertices.len())
                .unwrap_or(true)
        })
    {
        return MirrorPinProjection::rejected(MirrorPinReason::RejectedInvalidSurface);
    }

    let scale = characteristic_diagonal(&mesh.vertices);
    if !scale.is_finite() || scale <= f64::EPSILON {
        return MirrorPinProjection::rejected(MirrorPinReason::RejectedInvalidSurface);
    }
    let numerical_epsilon = (scale * 1e-12).max(f64::EPSILON * 64.0);
    let centerline_tolerance = (scale * options.centerline_tolerance_ratio).max(numerical_epsilon);
    let tolerance = (scale * options.counterpart_tolerance_ratio).max(numerical_epsilon);
    if !centerline_tolerance.is_finite() || !tolerance.is_finite() {
        return MirrorPinProjection::rejected(MirrorPinReason::RejectedInvalidTolerance);
    }
    let source_offset = picked_point[0] - options.center_x;
    if source_offset.abs() <= centerline_tolerance {
        return MirrorPinProjection {
            accepted: true,
            create_counterpart: false,
            reason: MirrorPinReason::CenterlineSingle,
            triangle_id: None,
            barycentric: None,
            projected_point: None,
            reflected_point: None,
            projection_error: None,
            tolerance: Some(tolerance),
            centerline_tolerance: Some(centerline_tolerance),
        };
    }

    let target_sign = -source_offset.signum();
    let candidate_ids: Vec<u32> = mesh
        .triangles
        .iter()
        .enumerate()
        .filter_map(|(triangle_id, triangle)| {
            let reaches_target_half = triangle.iter().copied().any(|vertex_id| {
                target_sign * (mesh.vertices[vertex_id as usize][0] - options.center_x)
                    > numerical_epsilon
            });
            reaches_target_half.then(|| u32::try_from(triangle_id).ok())?
        })
        .collect();
    if candidate_ids.is_empty() {
        return MirrorPinProjection {
            reflected_point: Some(reflect_point_x(picked_point, options.center_x)),
            tolerance: Some(tolerance),
            centerline_tolerance: Some(centerline_tolerance),
            ..MirrorPinProjection::rejected(MirrorPinReason::RejectedNoOppositeSurface)
        };
    }

    let reflected_point = reflect_point_x(picked_point, options.center_x);
    let Ok(projector) =
        SurfaceProjector::new(&mesh.vertices, &mesh.triangles, Some(&candidate_ids), 0.0)
    else {
        return MirrorPinProjection {
            reflected_point: Some(reflected_point),
            tolerance: Some(tolerance),
            centerline_tolerance: Some(centerline_tolerance),
            ..MirrorPinProjection::rejected(MirrorPinReason::RejectedProjectionUnavailable)
        };
    };
    let Ok(projection) = projector.project(reflected_point) else {
        return MirrorPinProjection {
            reflected_point: Some(reflected_point),
            tolerance: Some(tolerance),
            centerline_tolerance: Some(centerline_tolerance),
            ..MirrorPinProjection::rejected(MirrorPinReason::RejectedProjectionUnavailable)
        };
    };
    let projection_error = projection.distance_squared.sqrt();
    let projected_to_opposite_half =
        target_sign * (projection.point[0] - options.center_x) > numerical_epsilon;
    let reason = if !projected_to_opposite_half {
        MirrorPinReason::RejectedWrongSide
    } else if !projection_error.is_finite() || projection_error > tolerance {
        MirrorPinReason::RejectedAsymmetricCounterpart
    } else {
        MirrorPinReason::CounterpartAccepted
    };
    let accepted = reason == MirrorPinReason::CounterpartAccepted;
    MirrorPinProjection {
        accepted,
        create_counterpart: accepted,
        reason,
        triangle_id: Some(projection.primitive_id),
        barycentric: Some(projection.barycentric),
        projected_point: Some(projection.point),
        reflected_point: Some(reflected_point),
        projection_error: Some(projection_error),
        tolerance: Some(tolerance),
        centerline_tolerance: Some(centerline_tolerance),
    }
}

fn reflect_point_x(point: [f64; 3], center_x: f64) -> [f64; 3] {
    [2.0 * center_x - point[0], point[1], point[2]]
}

fn relax_seam(mesh: &mut Mesh, center_x: f64, seam_width: f64, plane_epsilon: f64) {
    if seam_width <= plane_epsilon || !seam_width.is_finite() {
        return;
    }
    let mut neighbours: Vec<Vec<u32>> = vec![Vec::new(); mesh.vertices.len()];
    let mut incident: Vec<Vec<u32>> = vec![Vec::new(); mesh.vertices.len()];
    for (index, triangle) in mesh.triangles.iter().enumerate() {
        for corner in 0..3 {
            let here = triangle[corner] as usize;
            let next = triangle[(corner + 1) % 3];
            if !neighbours[here].contains(&next) {
                neighbours[here].push(next);
            }
            if !neighbours[next as usize].contains(&triangle[corner]) {
                neighbours[next as usize].push(triangle[corner]);
            }
            #[expect(clippy::cast_possible_truncation, reason = "triangle ids fit u32")]
            incident[here].push(index as u32);
        }
    }

    let weights: Vec<f64> = mesh
        .vertices
        .iter()
        .map(|vertex| {
            let distance = (vertex[0] - center_x).abs();
            if distance >= seam_width {
                return 0.0;
            }
            let t = (distance / seam_width).clamp(0.0, 1.0);
            1.0 - ((3.0 - 2.0 * t) * t * t)
        })
        .collect();

    for _ in 0..SEAM_RELAX_PASSES {
        let previous = mesh.vertices.clone();
        for (index, weight) in weights.iter().copied().enumerate() {
            if weight <= 0.0 || neighbours[index].is_empty() {
                continue;
            }
            let mut average = [0.0_f64; 3];
            for &neighbour in &neighbours[index] {
                for axis in 0..3 {
                    average[axis] += previous[neighbour as usize][axis];
                }
            }
            #[expect(clippy::cast_precision_loss, reason = "neighbour counts")]
            let count = neighbours[index].len() as f64;
            let delta = [0, 1, 2].map(|axis| average[axis] / count - previous[index][axis]);

            let normal = vertex_normal(&previous, &mesh.triangles, &incident[index]);
            let Some(normal) = normal else {
                continue;
            };
            let along = (0..3).map(|axis| delta[axis] * normal[axis]).sum::<f64>();
            let strength = weight * SEAM_RELAX_RATE;
            for axis in 0..3 {
                mesh.vertices[index][axis] =
                    (along * normal[axis]).mul_add(strength, previous[index][axis]);
            }
        }
    }

    for vertex in &mut mesh.vertices {
        if (vertex[0] - center_x).abs() <= plane_epsilon {
            vertex[0] = center_x;
        }
    }
}

fn vertex_normal(
    vertices: &[[f64; 3]],
    triangles: &[[u32; 3]],
    incident: &[u32],
) -> Option<[f64; 3]> {
    let mut sum = [0.0_f64; 3];
    for &triangle_id in incident {
        let triangle = triangles[triangle_id as usize];
        let [a, b, c] = triangle.map(|index| vertices[index as usize]);
        let first = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let second = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        sum[0] += first[1] * second[2] - first[2] * second[1];
        sum[1] += first[2] * second[0] - first[0] * second[2];
        sum[2] += first[0] * second[1] - first[1] * second[0];
    }
    let length = sum[2]
        .mul_add(sum[2], sum[1].mul_add(sum[1], sum[0] * sum[0]))
        .sqrt();
    (length > f64::EPSILON).then(|| [sum[0] / length, sum[1] / length, sum[2] / length])
}

const SEAM_RELAX_PASSES: usize = 8;
const SEAM_RELAX_RATE: f64 = 0.6;

fn mirror_mesh_x(mesh: &Mesh, center_x: f64, keep_sign: f64) -> Result<Mesh, SymmetryError> {
    let epsilon = (characteristic_diagonal(&mesh.vertices) * 1e-7).max(f64::EPSILON * 64.0);
    let side = |vertex: [f64; 3]| {
        let offset = (vertex[0] - center_x) * keep_sign;
        if offset > epsilon {
            1_i8
        } else if offset < -epsilon {
            -1
        } else {
            0
        }
    };

    let mut kept_vertices: Vec<[f64; 3]> = Vec::new();
    let mut kept_triangles: Vec<[u32; 3]> = Vec::new();
    let mut remap = vec![u32::MAX; mesh.vertices.len()];
    let keep_vertex = |index: usize,
                       kept_vertices: &mut Vec<[f64; 3]>,
                       remap: &mut Vec<u32>|
     -> Result<u32, SymmetryError> {
        if remap[index] != u32::MAX {
            return Ok(remap[index]);
        }
        let mut point = mesh.vertices[index];
        if side(point) == 0 {
            point[0] = center_x;
        }
        let id = u32::try_from(kept_vertices.len()).map_err(|_| SymmetryError::MeshTooLarge)?;
        kept_vertices.push(point);
        remap[index] = id;
        Ok(id)
    };

    for triangle in &mesh.triangles {
        let points = triangle.map(|index| mesh.vertices[index as usize]);
        let sides = points.map(side);
        if sides.iter().all(|&value| value <= 0) {
            continue;
        }
        if sides.iter().all(|&value| value >= 0) {
            let mut corners = [0_u32; 3];
            for (slot, &index) in corners.iter_mut().zip(triangle.iter()) {
                *slot = keep_vertex(index as usize, &mut kept_vertices, &mut remap)?;
            }
            kept_triangles.push(corners);
            continue;
        }

        let mut polygon: Vec<u32> = Vec::with_capacity(4);
        for corner in 0..3 {
            let next = (corner + 1) % 3;
            if sides[corner] >= 0 {
                polygon.push(keep_vertex(
                    triangle[corner] as usize,
                    &mut kept_vertices,
                    &mut remap,
                )?);
            }
            if sides[corner] * sides[next] < 0 {
                let (from, to) = (points[corner], points[next]);
                let span = to[0] - from[0];
                let ratio = if span.abs() <= f64::MIN_POSITIVE {
                    0.5
                } else {
                    ((center_x - from[0]) / span).clamp(0.0, 1.0)
                };
                let mut cut = [0.0_f64; 3];
                for axis in 0..3 {
                    cut[axis] = (to[axis] - from[axis]).mul_add(ratio, from[axis]);
                }
                cut[0] = center_x;
                let id =
                    u32::try_from(kept_vertices.len()).map_err(|_| SymmetryError::MeshTooLarge)?;
                kept_vertices.push(cut);
                polygon.push(id);
            }
        }
        for corner in 1..polygon.len().saturating_sub(1) {
            kept_triangles.push([polygon[0], polygon[corner], polygon[corner + 1]]);
        }
    }

    if kept_triangles.is_empty() {
        return Err(SymmetryError::NothingToMirror);
    }

    let mut vertices = kept_vertices.clone();
    let mut mirrored_of = vec![u32::MAX; kept_vertices.len()];
    for (index, point) in kept_vertices.iter().enumerate() {
        if (point[0] - center_x).abs() <= epsilon {
            mirrored_of[index] = u32::try_from(index).map_err(|_| SymmetryError::MeshTooLarge)?;
            continue;
        }
        let id = u32::try_from(vertices.len()).map_err(|_| SymmetryError::MeshTooLarge)?;
        vertices.push([2.0 * center_x - point[0], point[1], point[2]]);
        mirrored_of[index] = id;
    }
    let mut triangles = kept_triangles.clone();
    for triangle in &kept_triangles {
        triangles.push([
            mirrored_of[triangle[2] as usize],
            mirrored_of[triangle[1] as usize],
            mirrored_of[triangle[0] as usize],
        ]);
    }

    Mesh::new(vertices, triangles).map_err(|error| SymmetryError::Rebuild(error.to_string()))
}

pub fn symmetrize_mesh_x(
    mesh: &Mesh,
    options: SymmetryOptions,
) -> Result<SymmetrizedMesh, SymmetryError> {
    if !options.center_x.is_finite() || !options.minimum_coverage.is_finite() {
        return Err(SymmetryError::NonFiniteOption);
    }
    if !(0.0..=1.0).contains(&options.minimum_coverage) {
        return Err(SymmetryError::InvalidCoverage);
    }
    let diagonal = characteristic_diagonal(&mesh.vertices);
    let source_preapplied = options.source_preapplied();
    let seam_width = if source_preapplied {
        diagonal * 0.015
    } else {
        options.seam_width.unwrap_or(diagonal * 0.015)
    };
    if !seam_width.is_finite() {
        return Err(SymmetryError::NonFiniteOption);
    }
    if seam_width < 0.0 {
        return Err(SymmetryError::InvalidSeamWidth);
    }
    let tolerance = options
        .tolerance
        .unwrap_or((diagonal * 0.01).max(f64::EPSILON * 64.0));
    if !tolerance.is_finite() {
        return Err(SymmetryError::NonFiniteOption);
    }
    if tolerance <= 0.0 {
        return Err(SymmetryError::InvalidTolerance);
    }

    if options.mode == SymmetryMode::Off {
        return Ok(result_without_change(
            mesh,
            SymmetryVerdict {
                options,
                seam_width,
                tolerance,
                accepted: true,
                reason: SymmetryReason::ModeOff,
                target_count: 0,
                errors: &[],
            },
        ));
    }

    if source_preapplied {
        return Ok(result_without_change(
            mesh,
            SymmetryVerdict {
                options,
                seam_width,
                tolerance,
                accepted: true,
                reason: SymmetryReason::SourcePreapplied,
                target_count: 0,
                errors: &[],
            },
        ));
    }

    let plane_epsilon = (diagonal * 1e-9).max(f64::EPSILON * 32.0);
    let base_sign = match options.mode {
        SymmetryMode::PositiveX => 1.0,
        SymmetryMode::NegativeX => -1.0,
        SymmetryMode::Off => unreachable!(),
    };
    let target_ids: Vec<usize> = mesh
        .vertices
        .iter()
        .enumerate()
        .filter_map(|(index, vertex)| {
            (((vertex[0] - options.center_x) * base_sign) < -plane_epsilon).then_some(index)
        })
        .collect();
    if target_ids.is_empty() {
        return Ok(result_without_change(
            mesh,
            SymmetryVerdict {
                options,
                seam_width,
                tolerance,
                accepted: false,
                reason: SymmetryReason::NoTargetVertices,
                target_count: 0,
                errors: &[],
            },
        ));
    }

    let base_triangle_ids: Vec<u32> = mesh
        .triangles
        .iter()
        .enumerate()
        .filter_map(|(triangle_id, triangle)| {
            let xs = triangle.map(|index| mesh.vertices[index as usize][0]);
            let on_side = match options.mode {
                SymmetryMode::PositiveX => {
                    xs.into_iter().fold(f64::NEG_INFINITY, f64::max)
                        > options.center_x + plane_epsilon
                }
                SymmetryMode::NegativeX => {
                    xs.into_iter().fold(f64::INFINITY, f64::min) < options.center_x - plane_epsilon
                }
                SymmetryMode::Off => unreachable!(),
            };
            on_side.then_some(triangle_id as u32)
        })
        .collect();
    if base_triangle_ids.is_empty() {
        return Ok(result_without_change(
            mesh,
            SymmetryVerdict {
                options,
                seam_width,
                tolerance,
                accepted: false,
                reason: SymmetryReason::NoBaseSurface,
                target_count: target_ids.len(),
                errors: &[],
            },
        ));
    }

    let projector = SurfaceProjector::new(
        &mesh.vertices,
        &mesh.triangles,
        Some(&base_triangle_ids),
        0.0,
    )?;
    let mut errors = Vec::with_capacity(target_ids.len());
    let mut accepted_count = 0usize;
    for &target_id in &target_ids {
        let mut reflected = mesh.vertices[target_id];
        reflected[0] = 2.0 * options.center_x - reflected[0];
        let error = projector.project(reflected)?.distance_squared.sqrt();
        if error <= tolerance {
            accepted_count += 1;
        }
        errors.push(error);
    }
    #[expect(clippy::cast_precision_loss, reason = "vertex counts")]
    let coverage = accepted_count as f64 / target_ids.len() as f64;
    if coverage < options.minimum_coverage {
        return Ok(result_without_change(
            mesh,
            SymmetryVerdict {
                options,
                seam_width,
                tolerance,
                accepted: false,
                reason: SymmetryReason::InsufficientCounterpartCoverage,
                target_count: target_ids.len(),
                errors: &errors,
            },
        ));
    }

    let mut result = mirror_mesh_x(mesh, options.center_x, base_sign)?;
    relax_seam(&mut result, options.center_x, seam_width, plane_epsilon);

    let reason = if accepted_count == target_ids.len() {
        SymmetryReason::Symmetrized
    } else {
        SymmetryReason::SymmetrizedBeyondTolerance
    };
    Ok(SymmetrizedMesh {
        mesh: result,
        receipt: receipt(
            SymmetryVerdict {
                options,
                seam_width,
                tolerance,
                accepted: true,
                reason,
                target_count: target_ids.len(),
                errors: &errors,
            },
            true,
            accepted_count,
        ),
    })
}

struct SymmetryVerdict<'a> {
    options: SymmetryOptions,
    seam_width: f64,
    tolerance: f64,
    accepted: bool,
    reason: SymmetryReason,
    target_count: usize,
    errors: &'a [f64],
}

fn receipt(verdict: SymmetryVerdict<'_>, applied: bool, accepted_count: usize) -> SymmetryReceipt {
    let SymmetryVerdict {
        options,
        seam_width,
        tolerance,
        accepted,
        reason,
        target_count,
        errors,
    } = verdict;
    let finite: Vec<f64> = errors
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    SymmetryReceipt {
        accepted,
        applied,
        reason,
        mode: options.mode,
        center_x: options.center_x,
        seam_width,
        tolerance,
        minimum_coverage: options.minimum_coverage,
        target_vertex_count: target_count,
        projected_vertex_count: accepted_count,
        rejected_vertex_count: target_count.saturating_sub(accepted_count),
        coverage: if target_count == 0 {
            0.0
        } else {
            accepted_count as f64 / target_count as f64
        },
        mean_symmetry_error: (!finite.is_empty())
            .then(|| finite.iter().sum::<f64>() / finite.len() as f64),
        max_symmetry_error: finite.into_iter().reduce(f64::max),
    }
}

fn result_without_change(mesh: &Mesh, verdict: SymmetryVerdict<'_>) -> SymmetrizedMesh {
    SymmetrizedMesh {
        mesh: mesh.clone(),

        receipt: receipt(verdict, false, 0),
    }
}

fn characteristic_diagonal(vertices: &[[f64; 3]]) -> f64 {
    let bounds_diagonal = |points: &[[f64; 3]]| {
        let mut minimum = [f64::INFINITY; 3];
        let mut maximum = [f64::NEG_INFINITY; 3];
        for point in points {
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min(point[axis]);
                maximum[axis] = maximum[axis].max(point[axis]);
            }
        }
        (0..3)
            .map(|axis| (maximum[axis] - minimum[axis]).powi(2))
            .sum::<f64>()
            .sqrt()
    };
    let full = bounds_diagonal(vertices);
    if vertices.len() < 20 {
        return full;
    }
    let mut low = [0.0; 3];
    let mut high = [0.0; 3];
    for axis in 0..3 {
        let mut values: Vec<f64> = vertices.iter().map(|point| point[axis]).collect();
        values.sort_by(f64::total_cmp);
        low[axis] = linear_quantile(&values, 0.02);
        high[axis] = linear_quantile(&values, 0.98);
    }
    let robust = (0..3)
        .map(|axis| (high[axis] - low[axis]).powi(2))
        .sum::<f64>()
        .sqrt();
    if robust > f64::EPSILON { robust } else { full }
}

fn linear_quantile(sorted: &[f64], fraction: f64) -> f64 {
    let position = (sorted.len() - 1) as f64 * fraction;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let blend = position - lower as f64;
        sorted[lower] * (1.0 - blend) + sorted[upper] * blend
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paired_x_planes(positive_x: f64, negative_x: f64) -> Mesh {
        Mesh::new(
            vec![
                [positive_x, 0.0, 0.0],
                [positive_x, 1.0, 0.0],
                [positive_x, 0.0, 1.0],
                [negative_x, 0.0, 0.0],
                [negative_x, 1.0, 0.0],
                [negative_x, 0.0, 1.0],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .unwrap()
    }

    fn asymmetric_grid(bump: f64) -> Mesh {
        const COLUMNS: usize = 21;
        const ROWS: usize = 11;
        let mut vertices = Vec::new();
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                #[expect(clippy::cast_precision_loss, reason = "small grid")]
                let x = column as f64 - 10.0;
                #[expect(clippy::cast_precision_loss, reason = "small grid")]
                let y = row as f64;
                let z = if x < -0.5 && row == 5 && column == 4 {
                    bump
                } else {
                    0.0
                };
                vertices.push([x, y, z]);
            }
        }
        let mut triangles = Vec::new();
        for row in 0..ROWS - 1 {
            for column in 0..COLUMNS - 1 {
                let base = row * COLUMNS + column;
                triangles.push([base as u32, (base + 1) as u32, (base + COLUMNS) as u32]);
                triangles.push([
                    (base + 1) as u32,
                    (base + COLUMNS + 1) as u32,
                    (base + COLUMNS) as u32,
                ]);
            }
        }
        Mesh::new(vertices, triangles).unwrap()
    }

    #[test]
    fn mirroring_leaves_no_vertex_behind_on_the_half_it_replaced() {
        let bump = 2.0;
        let mesh = asymmetric_grid(bump);
        let result = symmetrize_mesh_x(
            &mesh,
            SymmetryOptions {
                mode: SymmetryMode::PositiveX,
                center_x: 0.0,
                seam_width: Some(0.0),
                ..Default::default()
            },
        )
        .expect("the grid symmetrizes");

        assert!(result.receipt.accepted, "{:?}", result.receipt);
        let worst = result
            .mesh
            .vertices
            .iter()
            .filter(|vertex| vertex[0] < 0.0)
            .map(|vertex| vertex[2].abs())
            .fold(0.0_f64, f64::max);
        assert!(
            worst <= 1e-9,
            "the replaced half kept {worst} of its own ridge; \
             it should be a copy of the side that had none"
        );
    }

    #[test]
    fn mirror_pin_projects_to_symmetric_counterpart() {
        let mesh = paired_x_planes(3.0, 1.0);
        let result = project_mirrored_surface_pin_x(
            &mesh,
            [3.0, 0.25, 0.25],
            MirrorPinOptions {
                center_x: 2.0,
                ..Default::default()
            },
        );

        assert!(result.accepted);
        assert!(result.create_counterpart);
        assert_eq!(result.reason, MirrorPinReason::CounterpartAccepted);
        assert_eq!(result.triangle_id, Some(1));
        assert_eq!(result.projected_point, Some([1.0, 0.25, 0.25]));
        assert_eq!(result.reflected_point, result.projected_point);
        assert!(result.projection_error.unwrap() <= 1e-12);
        let barycentric = result.barycentric.unwrap();
        assert!((barycentric.iter().sum::<f64>() - 1.0).abs() <= 1e-12);
    }

    #[test]
    fn mirror_pin_rejects_implausibly_asymmetric_counterpart() {
        let mesh = paired_x_planes(1.0, -3.0);
        let result = project_mirrored_surface_pin_x(
            &mesh,
            [1.0, 0.25, 0.25],
            MirrorPinOptions {
                counterpart_tolerance_ratio: 0.01,
                ..Default::default()
            },
        );

        assert!(!result.accepted);
        assert!(!result.create_counterpart);
        assert_eq!(
            result.reason,
            MirrorPinReason::RejectedAsymmetricCounterpart
        );
        assert_eq!(result.triangle_id, Some(1));
        assert_eq!(result.projected_point, Some([-3.0, 0.25, 0.25]));
        assert!(result.projection_error.unwrap() > result.tolerance.unwrap());
    }

    #[test]
    fn mirror_pin_keeps_centerline_as_one_pin() {
        let mesh = Mesh::new(
            vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let result =
            project_mirrored_surface_pin_x(&mesh, [0.0, 0.25, 0.25], MirrorPinOptions::default());

        assert!(result.accepted);
        assert!(!result.create_counterpart);
        assert_eq!(result.reason, MirrorPinReason::CenterlineSingle);
        assert_eq!(result.triangle_id, None);
        assert_eq!(result.projected_point, None);
    }

    #[test]
    fn mirror_pin_rejects_non_finite_inputs_without_panicking() {
        let mesh = paired_x_planes(1.0, -1.0);
        for (point, options) in [
            ([f64::NAN, 0.0, 0.0], MirrorPinOptions::default()),
            (
                [1.0, 0.0, 0.0],
                MirrorPinOptions {
                    center_x: f64::INFINITY,
                    ..Default::default()
                },
            ),
            (
                [1.0, 0.0, 0.0],
                MirrorPinOptions {
                    counterpart_tolerance_ratio: f64::NAN,
                    ..Default::default()
                },
            ),
        ] {
            let result = project_mirrored_surface_pin_x(&mesh, point, options);
            assert!(!result.accepted);
            assert_eq!(result.reason, MirrorPinReason::RejectedNonFiniteInput);
        }
    }

    #[test]
    fn mirror_pin_rejects_scale_tolerance_overflow() {
        let mesh = paired_x_planes(1.0, -1.0);
        let result = project_mirrored_surface_pin_x(
            &mesh,
            [1.0, 0.0, 0.0],
            MirrorPinOptions {
                counterpart_tolerance_ratio: f64::MAX,
                ..Default::default()
            },
        );
        assert!(!result.accepted);
        assert_eq!(result.reason, MirrorPinReason::RejectedInvalidTolerance);
    }

    #[test]
    fn off_mode_is_byte_equivalent_geometry() {
        let mesh = Mesh::new(
            vec![[-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let result = symmetrize_mesh_x(&mesh, SymmetryOptions::default()).unwrap();
        assert_eq!(result.mesh, mesh);
        assert_eq!(result.receipt.reason, SymmetryReason::ModeOff);
    }

    #[test]
    fn preapplied_source_is_not_processed_twice_and_retains_output_mode() {
        let mesh = Mesh::new(
            vec![[-1.0, 0.0, 0.3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        )
        .unwrap();
        let options = SymmetryOptions::preapplied(SymmetryMode::PositiveX, 0.25);
        assert!(options.source_preapplied());

        let result = symmetrize_mesh_x(&mesh, options).unwrap();

        assert_eq!(result.mesh, mesh);
        assert_eq!(result.receipt.mode, SymmetryMode::PositiveX);
        assert_eq!(result.receipt.center_x, 0.25);
        assert_eq!(result.receipt.reason, SymmetryReason::SourcePreapplied);
        assert!(!result.receipt.applied);
    }

    fn sloped_grid(slope: f64) -> Mesh {
        const COLUMNS: usize = 21;
        const ROWS: usize = 5;
        let mut vertices = Vec::new();
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                #[expect(clippy::cast_precision_loss, reason = "small grid")]
                let x = column as f64 - 10.0;
                #[expect(clippy::cast_precision_loss, reason = "small grid")]
                let y = row as f64;
                vertices.push([x, y, x * slope]);
            }
        }
        let mut triangles = Vec::new();
        for row in 0..ROWS - 1 {
            for column in 0..COLUMNS - 1 {
                let base = row * COLUMNS + column;
                triangles.push([base as u32, (base + 1) as u32, (base + COLUMNS) as u32]);
                triangles.push([
                    (base + 1) as u32,
                    (base + COLUMNS + 1) as u32,
                    (base + COLUMNS) as u32,
                ]);
            }
        }
        Mesh::new(vertices, triangles).unwrap()
    }

    fn seam_turn(mesh: &Mesh, center_x: f64) -> f64 {
        let mut row: Vec<[f64; 3]> = mesh
            .vertices
            .iter()
            .copied()
            .filter(|vertex| (vertex[1] - 2.0).abs() < 0.25)
            .collect();
        row.sort_by(|left, right| left[0].partial_cmp(&right[0]).unwrap());
        let seam = row
            .iter()
            .position(|vertex| (vertex[0] - center_x).abs() < 1.0e-9)
            .expect("the row crosses the plane");
        let before = row[seam - 1];
        let after = row[seam + 1];
        let arm = |from: [f64; 3], to: [f64; 3]| {
            let delta = [to[0] - from[0], to[2] - from[2]];
            let length = delta[0].hypot(delta[1]).max(f64::EPSILON);
            [delta[0] / length, delta[1] / length]
        };
        let incoming = arm(before, row[seam]);
        let outgoing = arm(row[seam], after);
        incoming
            .iter()
            .zip(outgoing)
            .map(|(left, right)| left * right)
            .sum::<f64>()
            .clamp(-1.0, 1.0)
            .acos()
    }

    #[test]
    fn the_seam_is_rounded_rather_than_left_as_a_ridge() {
        let mesh = sloped_grid(0.6);
        let mirror = |seam_width: f64| {
            symmetrize_mesh_x(
                &mesh,
                SymmetryOptions {
                    mode: SymmetryMode::PositiveX,
                    center_x: 0.0,
                    seam_width: Some(seam_width),
                    minimum_coverage: 0.0,
                    tolerance: Some(1.0e6),
                },
            )
            .expect("it mirrors")
            .mesh
        };

        let sharp = seam_turn(&mirror(0.0), 0.0);
        let rounded = seam_turn(&mirror(4.0), 0.0);
        assert!(
            sharp > 0.9,
            "the wedge should close on a real ridge, got {sharp} rad"
        );
        assert!(
            rounded < sharp * 0.5,
            "the seam is still {rounded} rad against {sharp} unsmoothed"
        );

        let smoothed = mirror(4.0);
        for vertex in &smoothed.vertices {
            let reflected = [-vertex[0], vertex[1], vertex[2]];
            assert!(
                smoothed.vertices.iter().any(|candidate| {
                    (0..3).all(|axis| (candidate[axis] - reflected[axis]).abs() < 1.0e-9)
                }),
                "{vertex:?} lost its counterpart to the smoothing"
            );
        }
    }

    #[test]
    fn the_result_is_the_kept_half_and_its_reflection() {
        let mesh = Mesh::new(
            vec![
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [1.0, 0.0, 1.0],
                [-1.0, 0.2, 0.1],
            ],
            vec![[0, 1, 2], [3, 2, 1]],
        )
        .unwrap();
        let result = symmetrize_mesh_x(
            &mesh,
            SymmetryOptions {
                mode: SymmetryMode::PositiveX,
                tolerance: Some(2.0),
                seam_width: Some(0.0),
                minimum_coverage: 1.0,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(result.receipt.applied);

        for vertex in &result.mesh.vertices {
            let reflected = [-vertex[0], vertex[1], vertex[2]];
            assert!(
                result.mesh.vertices.iter().any(|candidate| {
                    (0..3).all(|axis| (candidate[axis] - reflected[axis]).abs() < 1e-9)
                }),
                "{vertex:?} has no counterpart across the plane"
            );
        }
        assert!(
            result
                .mesh
                .vertices
                .iter()
                .filter(|vertex| vertex[0] < 0.0)
                .all(|vertex| (vertex[0] + 1.0).abs() < 1e-9),
            "the replaced half should be a mirror of the plane at x = 1"
        );
        assert!(!result.mesh.triangles.is_empty());
    }
}
