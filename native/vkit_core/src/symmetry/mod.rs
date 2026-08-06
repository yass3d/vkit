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
    SymmetrizedWithSkippedVertices,
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
    let mut projections = Vec::with_capacity(target_ids.len());
    let mut errors = Vec::with_capacity(target_ids.len());
    let mut accepted_count = 0usize;
    for &target_id in &target_ids {
        let mut reflected = mesh.vertices[target_id];
        reflected[0] = 2.0 * options.center_x - reflected[0];
        let projection = projector.project(reflected)?;
        let error = projection.distance_squared.sqrt();
        if error <= tolerance {
            accepted_count += 1;
        }
        errors.push(error);
        projections.push(projection);
    }
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

    let mut result = mesh.clone();
    for ((&target_id, projection), error) in target_ids
        .iter()
        .zip(projections.iter())
        .zip(errors.iter().copied())
    {
        if error > tolerance {
            continue;
        }
        let original = mesh.vertices[target_id];
        let mut candidate = projection.point;
        candidate[0] = 2.0 * options.center_x - candidate[0];
        let distance_from_plane = (original[0] - options.center_x).abs();
        let blend = if seam_width <= plane_epsilon {
            1.0
        } else {
            let linear = (distance_from_plane / seam_width).clamp(0.0, 1.0);
            linear * linear * (3.0 - 2.0 * linear)
        };
        for axis in 0..3 {
            result.vertices[target_id][axis] =
                original[axis] * (1.0 - blend) + candidate[axis] * blend;
        }
    }
    let reason = if accepted_count == target_ids.len() {
        SymmetryReason::Symmetrized
    } else {
        SymmetryReason::SymmetrizedWithSkippedVertices
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

    #[test]
    fn positive_side_reconstructs_negative_without_index_pairing() {
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
        assert!((result.mesh.vertices[3][0] + 1.0).abs() < 1e-12);
        assert!((result.mesh.vertices[3][1] - 0.2).abs() < 1e-12);
        assert!((result.mesh.vertices[3][2] - 0.1).abs() < 1e-12);
        assert_eq!(result.mesh.triangles, mesh.triangles);
    }
}
