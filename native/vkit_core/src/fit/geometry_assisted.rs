use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use nalgebra::UnitQuaternion;

use crate::formats::{Mesh, SurfaceAttachment};
use crate::math::{Mat3, SimilarityTransform, Vec3, estimate_similarity};
use crate::spatial::SurfaceProjector;

use super::assisted_alignment::{
    AssistedAlignmentRejection, AssistedFitOptions, AssistedFitReceipt,
    AssistedIcpIterationReceipt, AssistedIcpSkipReason, clamp_similarity_delta, compose_similarity,
    estimate_assisted_similarity, refine_assisted_similarity_icp, valid_options, valid_similarity,
};
use super::dense_registration::largest_component_triangle_ids;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryInitializationMode {
    ManualPrior,

    PriorWithOnePair,

    PriorWithTwoPairs,

    PriorWithThreePairs,

    RobustLandmarkPairs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SparseInitializationFallback {
    DegeneratePairToTranslation,
    DegenerateTripletToPair,
    PairRotationUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RobustInitializationFallback {
    Rejected(AssistedAlignmentRejection),
    InvalidDelta,
    NoPinRmsImprovement,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryInitializationRejection {
    InvalidPrior,
    InvalidInput,
    InvalidOptions,
    InvalidSurface,
    RobustLandmarkInitialization(AssistedAlignmentRejection),
}

pub struct GeometryInitializationRequest<'a> {
    pub prior: SimilarityTransform,
    pub scan: &'a Mesh,
    pub template_vertices: &'a [Vec3],
    pub template_triangles: &'a [[u32; 3]],

    pub scan_points: &'a [Vec3],
    pub template_points: &'a [Vec3],
    pub weights: &'a [f64],
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeometryInitializationReceipt {
    pub pair_count: usize,
    pub mode: GeometryInitializationMode,
    pub sparse_fallback: Option<SparseInitializationFallback>,
    pub sparse_step_was_clamped: bool,

    pub extent_scale_ratio: Option<f64>,
    pub extent_translation_delta_cm: Option<Vec3>,
    pub extent_step_was_clamped: bool,
    pub robust_fallback: Option<RobustInitializationFallback>,
    pub robust_step_was_clamped: bool,
    pub pin_rms_before_cm: Option<f64>,
    pub pin_rms_after_cm: Option<f64>,

    pub robust_landmarks: Option<Box<AssistedFitReceipt>>,
    pub icp_iterations: Vec<AssistedIcpIterationReceipt>,
    pub icp_skip_reason: Option<AssistedIcpSkipReason>,
    pub rejection_reason: Option<GeometryInitializationRejection>,
}

impl GeometryInitializationReceipt {
    fn new(pair_count: usize) -> Self {
        Self {
            pair_count,
            mode: GeometryInitializationMode::ManualPrior,
            sparse_fallback: None,
            sparse_step_was_clamped: false,
            extent_scale_ratio: None,
            extent_translation_delta_cm: None,
            extent_step_was_clamped: false,
            robust_fallback: None,
            robust_step_was_clamped: false,
            pin_rms_before_cm: None,
            pin_rms_after_cm: None,
            robust_landmarks: None,
            icp_iterations: Vec::new(),
            icp_skip_reason: None,
            rejection_reason: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeometryInitializationResult {
    pub transform: SimilarityTransform,
    pub pin_weight_multipliers: Vec<f64>,
    pub receipt: GeometryInitializationReceipt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeometryInitializationError {
    pub receipt: Box<GeometryInitializationReceipt>,
}

impl fmt::Display for GeometryInitializationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "geometry-assisted initialization rejected: {:?}",
            self.receipt.rejection_reason
        )
    }
}

impl std::error::Error for GeometryInitializationError {}

pub fn initialize_geometry_assisted_similarity(
    request: GeometryInitializationRequest<'_>,
    options: &AssistedFitOptions,
) -> Result<GeometryInitializationResult, GeometryInitializationError> {
    let pair_count = request.scan_points.len();
    let mut receipt = GeometryInitializationReceipt::new(pair_count);
    if !valid_geometry_similarity(request.prior) {
        return initialization_rejection(receipt, GeometryInitializationRejection::InvalidPrior);
    }
    if !valid_options(options) {
        return initialization_rejection(receipt, GeometryInitializationRejection::InvalidOptions);
    }
    if pair_count != request.template_points.len()
        || pair_count != request.weights.len()
        || request.scan_points.iter().any(|point| !point.is_finite())
        || request
            .template_points
            .iter()
            .any(|point| !point.is_finite())
        || request
            .weights
            .iter()
            .any(|weight| !weight.is_finite() || *weight <= 0.0)
    {
        return initialization_rejection(receipt, GeometryInitializationRejection::InvalidInput);
    }
    if !valid_surface(
        request.scan,
        request.template_vertices,
        request.template_triangles,
    ) {
        return initialization_rejection(receipt, GeometryInitializationRejection::InvalidSurface);
    }

    receipt.pin_rms_before_cm =
        paired_rms(request.prior, request.scan_points, request.template_points);
    let (initial, multipliers, pin_anchored_initialization) = if pair_count >= 4 {
        receipt.mode = GeometryInitializationMode::RobustLandmarkPairs;
        let transformed = request.prior.apply_slice(request.scan_points);
        match estimate_assisted_similarity(
            &transformed,
            request.template_points,
            request.weights,
            options,
        ) {
            Ok(robust) => {
                receipt.robust_landmarks = Some(Box::new(robust.receipt.clone()));
                let (delta, clamped) = clamp_similarity_delta(robust.transform, options);
                receipt.robust_step_was_clamped = clamped;
                let candidate = compose_similarity(delta, request.prior);
                let before =
                    paired_rms(request.prior, request.scan_points, request.template_points);
                let after = paired_rms(candidate, request.scan_points, request.template_points);
                if !valid_geometry_similarity(delta) || !valid_geometry_similarity(candidate) {
                    receipt.robust_fallback = Some(RobustInitializationFallback::InvalidDelta);
                    (request.prior, vec![1.0; pair_count], false)
                } else if rms_is_worse(after, before) {
                    receipt.robust_fallback =
                        Some(RobustInitializationFallback::NoPinRmsImprovement);
                    (request.prior, vec![1.0; pair_count], false)
                } else {
                    (candidate, robust.pin_weight_multipliers, true)
                }
            }
            Err(error) => {
                let reason = error
                    .receipt
                    .rejection_reason
                    .unwrap_or(AssistedAlignmentRejection::InvalidInput);
                receipt.robust_landmarks = Some(error.receipt);
                receipt.robust_fallback = Some(RobustInitializationFallback::Rejected(reason));
                (request.prior, vec![1.0; pair_count], false)
            }
        }
    } else {
        let sparse_prior = if pair_count == 0 {
            let extent = refine_zero_pin_extent_prior(
                request.prior,
                request.scan,
                request.template_vertices,
                options,
            );
            receipt.extent_scale_ratio = Some(extent.scale_ratio);
            receipt.extent_translation_delta_cm = Some(extent.translation_delta);
            receipt.extent_step_was_clamped = extent.was_clamped;
            extent.transform
        } else {
            request.prior
        };
        let transformed = sparse_prior.apply_slice(request.scan_points);
        let (delta, mode, fallback) =
            sparse_prior_delta(&transformed, request.template_points, request.weights);
        let (delta, clamped) = clamp_similarity_delta(delta, options);
        receipt.mode = mode;
        receipt.sparse_fallback = fallback;
        receipt.sparse_step_was_clamped = clamped;
        (
            compose_similarity(delta, sparse_prior),
            vec![1.0; pair_count],
            false,
        )
    };

    let transform = if pin_anchored_initialization {
        receipt.icp_skip_reason = Some(AssistedIcpSkipReason::AuthoritativeUserPins);
        initial
    } else {
        let (transform, iterations, skip_reason) = refine_assisted_similarity_icp(
            initial,
            request.scan,
            request.template_vertices,
            request.template_triangles,
            options,
        );
        receipt.icp_iterations = iterations;
        receipt.icp_skip_reason = skip_reason;
        transform
    };
    receipt.pin_rms_after_cm = paired_rms(transform, request.scan_points, request.template_points);
    Ok(GeometryInitializationResult {
        transform,
        pin_weight_multipliers: multipliers,
        receipt,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometryExtentPriorResult {
    pub transform: SimilarityTransform,
    pub scale_ratio: f64,
    pub translation_delta: Vec3,
    pub was_clamped: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct GeometryExtentMeasurements {
    scan_bounds: AxisBounds,
    template_bounds: AxisBounds,
    scan_width: f64,
    template_width: f64,
}

pub fn suggest_zero_pin_extent_similarity(
    rotation: Mat3,
    scan: &Mesh,
    template_vertices: &[Vec3],
) -> Option<SimilarityTransform> {
    if !valid_geometry_similarity(SimilarityTransform {
        scale: 1.0,
        rotation,
        translation: Vec3::ZERO,
    }) {
        return None;
    }
    let measurements = geometry_extent_measurements(rotation, scan, template_vertices)?;
    let scale = measurements.template_width / measurements.scan_width;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let scaled_scan = measurements.scan_bounds.scaled(scale);
    let transform = SimilarityTransform {
        scale,
        rotation,
        translation: Vec3::new(
            measurements.template_bounds.center().x - scaled_scan.center().x,
            measurements.template_bounds.maximum.y - scaled_scan.maximum.y,
            measurements.template_bounds.maximum.z - scaled_scan.maximum.z,
        ),
    };
    valid_geometry_similarity(transform).then_some(transform)
}

pub fn refine_zero_pin_extent_prior(
    prior: SimilarityTransform,
    scan: &Mesh,
    template_vertices: &[Vec3],
    options: &AssistedFitOptions,
) -> GeometryExtentPriorResult {
    let Some(measurements) = geometry_extent_measurements(prior.rotation, scan, template_vertices)
    else {
        return GeometryExtentPriorResult {
            transform: prior,
            scale_ratio: 1.0,
            translation_delta: Vec3::ZERO,
            was_clamped: false,
        };
    };
    let scan_bounds = measurements.scan_bounds;
    let template_bounds = measurements.template_bounds;
    let scan_width = measurements.scan_width;
    let template_width = measurements.template_width;

    let requested_scale_ratio = template_width / (scan_width * prior.scale);
    let scale_limit = (options.icp_max_scale_step * 3.0).clamp(0.08, 0.15);
    let scale_ratio = requested_scale_ratio.clamp(1.0 - scale_limit, 1.0 + scale_limit);
    let scale = prior.scale * scale_ratio;
    let scaled_scan = scan_bounds.scaled(scale);
    let extent_mismatch = (requested_scale_ratio - 1.0).abs();
    let front_clearance = 0.35 * (extent_mismatch / 0.05).clamp(0.0, 1.0);
    let desired_translation = Vec3::new(
        template_bounds.center().x - scaled_scan.center().x,
        template_bounds.maximum.y - scaled_scan.maximum.y,
        template_bounds.maximum.z - scaled_scan.maximum.z - front_clearance,
    );
    let requested_translation_delta = desired_translation - prior.translation;
    let translation_limit = (options.icp_max_translation_step_cm * 2.5).max(1.0);
    let translation_length = requested_translation_delta.norm();
    let translation_delta = if translation_length > translation_limit {
        requested_translation_delta * (translation_limit / translation_length)
    } else {
        requested_translation_delta
    };
    GeometryExtentPriorResult {
        transform: SimilarityTransform {
            scale,
            rotation: prior.rotation,
            translation: prior.translation + translation_delta,
        },
        scale_ratio,
        translation_delta,
        was_clamped: (scale_ratio - requested_scale_ratio).abs() > 1.0e-12
            || translation_length > translation_limit,
    }
}

fn geometry_extent_measurements(
    rotation: Mat3,
    scan: &Mesh,
    template_vertices: &[Vec3],
) -> Option<GeometryExtentMeasurements> {
    let component_triangles = largest_component_triangle_ids(scan);
    let mut referenced = vec![false; scan.vertices.len()];
    for primitive in component_triangles {
        for vertex in scan.triangles[primitive as usize] {
            referenced[vertex as usize] = true;
        }
    }
    let rotated_scan = scan
        .vertices
        .iter()
        .zip(referenced)
        .filter(|(_, referenced)| *referenced)
        .map(|(point, _)| rotation.transform_vector(Vec3::from(*point)))
        .collect::<Vec<_>>();
    let scan_bounds = AxisBounds::from_points(rotated_scan.iter().copied())?;
    let template_bounds = AxisBounds::from_points(template_vertices.iter().copied())?;

    let scan_width = equal_spatial_support_axis_span(&rotated_scan, 0, 0.10)
        .or_else(|| trimmed_axis_span(&rotated_scan, 0, 0.10))
        .unwrap_or(scan_bounds.maximum.x - scan_bounds.minimum.x);
    let template_width = equal_spatial_support_axis_span(template_vertices, 0, 0.10)
        .or_else(|| trimmed_axis_span(template_vertices, 0, 0.10))
        .unwrap_or(template_bounds.maximum.x - template_bounds.minimum.x);
    if scan_width <= 1.0e-9 || template_width <= 1.0e-9 {
        return None;
    }
    Some(GeometryExtentMeasurements {
        scan_bounds,
        template_bounds,
        scan_width,
        template_width,
    })
}

fn trimmed_axis_span(points: &[Vec3], axis: usize, trim_fraction: f64) -> Option<f64> {
    let mut coordinates = points
        .iter()
        .map(|point| match axis {
            0 => point.x,
            1 => point.y,
            2 => point.z,
            _ => unreachable!("three-dimensional axis"),
        })
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if coordinates.len() < 3 || !(0.0..0.5).contains(&trim_fraction) {
        return None;
    }
    coordinates.sort_by(f64::total_cmp);
    let quantile = |fraction: f64| {
        let position = fraction * (coordinates.len() - 1) as f64;
        let lower = position.floor() as usize;
        let upper = position.ceil() as usize;
        let blend = position - lower as f64;
        coordinates[lower] * (1.0 - blend) + coordinates[upper] * blend
    };
    let span = quantile(1.0 - trim_fraction) - quantile(trim_fraction);
    (span.is_finite() && span > 1.0e-9).then_some(span)
}

const EQUAL_SUPPORT_VOXEL_RESOLUTION: u32 = 24;
const EQUAL_SUPPORT_MIN_OCCUPIED_VOXELS: usize = 12;

fn equal_spatial_support_axis_span(
    points: &[Vec3],
    axis: usize,
    trim_fraction: f64,
) -> Option<f64> {
    if !(0.0..0.5).contains(&trim_fraction) {
        return None;
    }
    let bounds = AxisBounds::from_points(points.iter().copied())?;
    let extent = bounds.maximum - bounds.minimum;
    let mut occupied = BTreeMap::<(u32, u32, u32), (Vec3, usize)>::new();
    for point in points.iter().copied().filter(|point| point.is_finite()) {
        let key = (
            normalized_voxel_coordinate(point.x, bounds.minimum.x, extent.x),
            normalized_voxel_coordinate(point.y, bounds.minimum.y, extent.y),
            normalized_voxel_coordinate(point.z, bounds.minimum.z, extent.z),
        );
        let entry = occupied.entry(key).or_insert((Vec3::ZERO, 0));
        entry.0 += point;
        entry.1 += 1;
    }
    if occupied.len() < EQUAL_SUPPORT_MIN_OCCUPIED_VOXELS {
        return None;
    }
    let representatives = occupied
        .into_values()
        .map(|(sum, count)| sum / count as f64)
        .collect::<Vec<_>>();
    trimmed_axis_span(&representatives, axis, trim_fraction)
}

fn normalized_voxel_coordinate(value: f64, minimum: f64, extent: f64) -> u32 {
    if !value.is_finite() || !minimum.is_finite() || !extent.is_finite() || extent <= 1.0e-12 {
        return 0;
    }
    (((value - minimum) / extent * (EQUAL_SUPPORT_VOXEL_RESOLUTION - 1) as f64).floor() as i64)
        .clamp(0, i64::from(EQUAL_SUPPORT_VOXEL_RESOLUTION - 1)) as u32
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AxisBounds {
    minimum: Vec3,
    maximum: Vec3,
}

impl AxisBounds {
    fn from_points(points: impl IntoIterator<Item = Vec3>) -> Option<Self> {
        let mut minimum = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut maximum = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        let mut count = 0_usize;
        for point in points {
            if !point.is_finite() {
                continue;
            }
            minimum.x = minimum.x.min(point.x);
            minimum.y = minimum.y.min(point.y);
            minimum.z = minimum.z.min(point.z);
            maximum.x = maximum.x.max(point.x);
            maximum.y = maximum.y.max(point.y);
            maximum.z = maximum.z.max(point.z);
            count += 1;
        }
        (count >= 3).then_some(Self { minimum, maximum })
    }

    fn center(self) -> Vec3 {
        (self.minimum + self.maximum) * 0.5
    }

    fn scaled(self, scale: f64) -> Self {
        Self {
            minimum: self.minimum * scale,
            maximum: self.maximum * scale,
        }
    }
}

fn rms_is_worse(after: Option<f64>, before: Option<f64>) -> bool {
    matches!((after, before), (Some(after), Some(before)) if after > before + 1.0e-10)
}

fn valid_geometry_similarity(transform: SimilarityTransform) -> bool {
    if !valid_similarity(transform) {
        return false;
    }
    let rows = transform.rotation.rows();
    for column in 0..3 {
        for other in 0..3 {
            let dot = (0..3)
                .map(|row| rows[row][column] * rows[row][other])
                .sum::<f64>();
            let expected = if column == other { 1.0 } else { 0.0 };
            if (dot - expected).abs() > 1.0e-6 {
                return false;
            }
        }
    }
    true
}

fn initialization_rejection(
    mut receipt: GeometryInitializationReceipt,
    reason: GeometryInitializationRejection,
) -> Result<GeometryInitializationResult, GeometryInitializationError> {
    receipt.rejection_reason = Some(reason);
    Err(GeometryInitializationError {
        receipt: Box::new(receipt),
    })
}

fn sparse_prior_delta(
    source: &[Vec3],
    target: &[Vec3],
    weights: &[f64],
) -> (
    SimilarityTransform,
    GeometryInitializationMode,
    Option<SparseInitializationFallback>,
) {
    match source.len() {
        0 => (
            SimilarityTransform::IDENTITY,
            GeometryInitializationMode::ManualPrior,
            None,
        ),
        1 => (
            translation_delta(source[0], target[0]),
            GeometryInitializationMode::PriorWithOnePair,
            None,
        ),
        2 => pair_delta(source, target, weights),
        3 => {
            let source_span = maximum_pair_distance(source);
            let target_span = maximum_pair_distance(target);
            let source_area = normalized_triangle_area(source, source_span);
            let target_area = normalized_triangle_area(target, target_span);
            if source_span > 1.0e-9
                && target_span > 1.0e-9
                && source_area.min(target_area) > 1.0e-5
                && let Ok(delta) = estimate_similarity(source, target, Some(weights))
                && valid_similarity(delta)
            {
                return (delta, GeometryInitializationMode::PriorWithThreePairs, None);
            }
            let (first, second) = farthest_pair(source, target);
            let pair_source = [source[first], source[second]];
            let pair_target = [target[first], target[second]];
            let pair_weights = [weights[first], weights[second]];
            let (delta, _, fallback) = pair_delta(&pair_source, &pair_target, &pair_weights);
            (
                delta,
                GeometryInitializationMode::PriorWithTwoPairs,
                Some(fallback.unwrap_or(SparseInitializationFallback::DegenerateTripletToPair)),
            )
        }
        _ => unreachable!("robust branch handles four or more pairs"),
    }
}

fn pair_delta(
    source: &[Vec3],
    target: &[Vec3],
    weights: &[f64],
) -> (
    SimilarityTransform,
    GeometryInitializationMode,
    Option<SparseInitializationFallback>,
) {
    let source_vector = source[1] - source[0];
    let target_vector = target[1] - target[0];
    let source_length = source_vector.norm();
    let target_length = target_vector.norm();
    if source_length <= 1.0e-9 || target_length <= 1.0e-9 {
        return (
            translation_delta(
                weighted_mean(source, weights),
                weighted_mean(target, weights),
            ),
            GeometryInitializationMode::PriorWithOnePair,
            Some(SparseInitializationFallback::DegeneratePairToTranslation),
        );
    }
    let source_direction = (source_vector / source_length).to_na();
    let target_direction = (target_vector / target_length).to_na();
    let Some(rotation) = UnitQuaternion::rotation_between(&source_direction, &target_direction)
    else {
        return (
            translation_delta(
                weighted_mean(source, weights),
                weighted_mean(target, weights),
            ),
            GeometryInitializationMode::PriorWithOnePair,
            Some(SparseInitializationFallback::PairRotationUnavailable),
        );
    };
    let scale = target_length / source_length;
    let rotation = Mat3::from_na(*rotation.to_rotation_matrix().matrix());
    let source_center = weighted_mean(source, weights);
    let target_center = weighted_mean(target, weights);
    let translation = target_center - rotation.transform_vector(source_center) * scale;
    (
        SimilarityTransform {
            scale,
            rotation,
            translation,
        },
        GeometryInitializationMode::PriorWithTwoPairs,
        None,
    )
}

fn translation_delta(source: Vec3, target: Vec3) -> SimilarityTransform {
    SimilarityTransform {
        translation: target - source,
        ..SimilarityTransform::IDENTITY
    }
}

fn weighted_mean(points: &[Vec3], weights: &[f64]) -> Vec3 {
    let total = weights.iter().sum::<f64>();
    points
        .iter()
        .copied()
        .zip(weights.iter().copied())
        .fold(Vec3::ZERO, |sum, (point, weight)| sum + point * weight)
        / total
}

fn farthest_pair(source: &[Vec3], target: &[Vec3]) -> (usize, usize) {
    let mut best = (0, 1);
    let mut best_score = -1.0_f64;
    for first in 0..source.len() - 1 {
        for second in first + 1..source.len() {
            let score = (source[second] - source[first])
                .norm()
                .min((target[second] - target[first]).norm());
            if score > best_score {
                best = (first, second);
                best_score = score;
            }
        }
    }
    best
}

fn maximum_pair_distance(points: &[Vec3]) -> f64 {
    let mut maximum = 0.0_f64;
    for first in 0..points.len() {
        for second in first + 1..points.len() {
            maximum = maximum.max((points[second] - points[first]).norm());
        }
    }
    maximum
}

fn normalized_triangle_area(points: &[Vec3], span: f64) -> f64 {
    if span <= 1.0e-12 {
        return 0.0;
    }
    cross(points[1] - points[0], points[2] - points[0]).norm() / span.powi(2)
}

fn paired_rms(transform: SimilarityTransform, source: &[Vec3], target: &[Vec3]) -> Option<f64> {
    if source.is_empty() {
        return None;
    }
    Some(
        (source
            .iter()
            .copied()
            .zip(target.iter().copied())
            .map(|(source, target)| (transform.apply(source) - target).norm_squared())
            .sum::<f64>()
            / source.len() as f64)
            .sqrt(),
    )
}

fn valid_surface(scan: &Mesh, vertices: &[Vec3], triangles: &[[u32; 3]]) -> bool {
    scan.require_surface().is_ok()
        && vertices.len() >= 3
        && !triangles.is_empty()
        && vertices.iter().all(|vertex| vertex.is_finite())
        && triangles
            .iter()
            .flatten()
            .all(|index| (*index as usize) < vertices.len())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutomaticSurfaceConstraintOptions {
    pub sample_count: usize,
    pub maximum_distance_cm: f64,
    pub minimum_normal_dot: f64,
    pub trim_fraction: f64,
    pub minimum_constraints: usize,
    pub minimum_coverage: f64,
    pub alignment_weight: f64,
    pub fit_weight: f64,
    pub confidence: f64,
}

impl Default for AutomaticSurfaceConstraintOptions {
    fn default() -> Self {
        Self {
            sample_count: 160,
            maximum_distance_cm: 1.75,
            minimum_normal_dot: -0.10,
            trim_fraction: 0.15,
            minimum_constraints: 24,
            minimum_coverage: 0.20,
            alignment_weight: 0.25,
            fit_weight: 0.20,
            confidence: 0.65,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomaticConstraintProvenance {
    ClosestSurfaceProjection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AutomaticConstraintRegion {
    LowerFaceJawChin,
    ExteriorEnvelope,
    BroadSurface,
    InteriorEye,
    InteriorNose,
    InteriorLip,
}

impl AutomaticConstraintRegion {
    const fn alignment_budget(self) -> f64 {
        match self {
            Self::LowerFaceJawChin => 0.36,
            Self::ExteriorEnvelope => 0.26,
            Self::BroadSurface => 0.23,
            Self::InteriorEye => 0.06,
            Self::InteriorNose => 0.06,
            Self::InteriorLip => 0.03,
        }
    }

    const fn fit_budget(self) -> f64 {
        match self {
            Self::LowerFaceJawChin => 0.45,
            Self::ExteriorEnvelope => 0.30,
            Self::BroadSurface => 0.20,
            Self::InteriorNose => 0.05,
            Self::InteriorEye | Self::InteriorLip => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutomaticConstraintRejection {
    InvalidOptions,
    InvalidSurface,
    ProjectionUnavailable,
    InsufficientCorrespondences,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomaticSurfaceConstraintReceipt {
    pub provenance: AutomaticConstraintProvenance,
    pub sampled_triangles: usize,
    pub projected_candidates: usize,
    pub duplicate_scan_primitives_removed: usize,
    pub accepted_constraints: usize,
    pub coverage: f64,
    pub rms_distance_cm: Option<f64>,
    pub maximum_accepted_distance_cm: Option<f64>,
    pub normals_were_flipped: bool,

    pub regional_constraint_counts: [usize; 6],

    pub regional_alignment_shares: [f64; 6],

    pub regional_fit_shares: [f64; 6],
    pub rejection_reason: Option<AutomaticConstraintRejection>,
}

impl AutomaticSurfaceConstraintReceipt {
    fn new() -> Self {
        Self {
            provenance: AutomaticConstraintProvenance::ClosestSurfaceProjection,
            sampled_triangles: 0,
            projected_candidates: 0,
            duplicate_scan_primitives_removed: 0,
            accepted_constraints: 0,
            coverage: 0.0,
            rms_distance_cm: None,
            maximum_accepted_distance_cm: None,
            normals_were_flipped: false,
            regional_constraint_counts: [0; 6],
            regional_alignment_shares: [0.0; 6],
            regional_fit_shares: [0.0; 6],
            rejection_reason: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomaticSurfaceCorrespondence {
    pub scan: SurfaceAttachment,
    pub template: SurfaceAttachment,
    pub distance_cm: f64,
    pub normal_dot: f64,
    pub(crate) region: AutomaticConstraintRegion,

    pub alignment_share: f64,

    pub fit_share: f64,
}

impl AutomaticSurfaceCorrespondence {
    pub(crate) fn normalize_weights(correspondences: &mut [Self]) {
        normalize_automatic_correspondence_weights(correspondences);
    }

    pub(crate) fn regional_receipt(correspondences: &[Self]) -> ([usize; 6], [f64; 6], [f64; 6]) {
        automatic_regional_receipt(correspondences)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomaticSurfaceConstraintResult {
    pub correspondences: Vec<AutomaticSurfaceCorrespondence>,
    pub receipt: AutomaticSurfaceConstraintReceipt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AutomaticSurfaceConstraintError {
    pub receipt: Box<AutomaticSurfaceConstraintReceipt>,
}

impl fmt::Display for AutomaticSurfaceConstraintError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "automatic surface constraints rejected: {:?}",
            self.receipt.rejection_reason
        )
    }
}

impl std::error::Error for AutomaticSurfaceConstraintError {}

#[derive(Clone)]
struct Candidate {
    template_primitive: u32,
    scan_projection: crate::spatial::SurfaceProjection,
    distance_cm: f64,
    normal_dot: f64,
}

pub fn build_automatic_surface_correspondences(
    scan: &Mesh,
    template: &Mesh,
    template_candidate_triangles: &[u32],
    scan_to_template: SimilarityTransform,
    options: &AutomaticSurfaceConstraintOptions,
) -> Result<AutomaticSurfaceConstraintResult, AutomaticSurfaceConstraintError> {
    let mut receipt = AutomaticSurfaceConstraintReceipt::new();
    if !valid_automatic_options(options) || !valid_geometry_similarity(scan_to_template) {
        return automatic_rejection(receipt, AutomaticConstraintRejection::InvalidOptions);
    }
    if scan.require_surface().is_err()
        || template.require_surface().is_err()
        || template_candidate_triangles.is_empty()
        || template_candidate_triangles
            .iter()
            .any(|index| (*index as usize) >= template.triangles.len())
    {
        return automatic_rejection(receipt, AutomaticConstraintRejection::InvalidSurface);
    }
    let scan_candidates = largest_component_triangle_ids(scan);
    let Ok(projector) =
        SurfaceProjector::new(&scan.vertices, &scan.triangles, Some(&scan_candidates), 0.0)
    else {
        return automatic_rejection(receipt, AutomaticConstraintRejection::ProjectionUnavailable);
    };
    let samples =
        deterministic_spatial_samples(template, template_candidate_triangles, options.sample_count);
    let region_bounds = surface_bounds(template, template_candidate_triangles);
    receipt.sampled_triangles = samples.len();
    let mut candidates = Vec::with_capacity(samples.len());
    for &primitive in &samples {
        let triangle = template.triangles[primitive as usize];
        let points = triangle.map(|index| Vec3::from(template.vertices[index as usize]));
        let template_point = (points[0] + points[1] + points[2]) / 3.0;
        let template_normal =
            normalize_or_zero(cross(points[1] - points[0], points[2] - points[0]));
        if template_normal.norm_squared() <= 1.0e-12 {
            continue;
        }
        let query = inverse_apply(scan_to_template, template_point);
        let Ok(projection) = projector.project(query.to_array()) else {
            continue;
        };
        let scan_point = Vec3::from(projection.point);
        let transformed = scan_to_template.apply(scan_point);
        let scan_normal = normalize_or_zero(
            scan_to_template
                .rotation
                .transform_vector(Vec3::from(projection.normal)),
        );
        candidates.push(Candidate {
            template_primitive: primitive,
            scan_projection: projection,
            distance_cm: (transformed - template_point).norm(),
            normal_dot: scan_normal.dot(template_normal),
        });
    }
    receipt.projected_candidates = candidates.len();
    if candidates.is_empty() {
        return automatic_rejection(receipt, AutomaticConstraintRejection::ProjectionUnavailable);
    }
    let dots = candidates
        .iter()
        .map(|candidate| candidate.normal_dot)
        .collect::<Vec<_>>();
    let flip = median(&dots).is_some_and(|value| value < 0.0);
    receipt.normals_were_flipped = flip;
    if flip {
        for candidate in &mut candidates {
            candidate.normal_dot *= -1.0;
        }
    }
    candidates.retain(|candidate| {
        candidate.distance_cm <= options.maximum_distance_cm
            && candidate.normal_dot >= options.minimum_normal_dot
    });
    candidates.sort_by(|left, right| {
        left.distance_cm
            .total_cmp(&right.distance_cm)
            .then_with(|| left.template_primitive.cmp(&right.template_primitive))
    });
    let keep = ((candidates.len() as f64) * (1.0 - options.trim_fraction)).ceil() as usize;
    candidates.truncate(keep);

    let before_deduplication = candidates.len();
    let mut scan_primitives = BTreeSet::new();
    candidates.retain(|candidate| scan_primitives.insert(candidate.scan_projection.primitive_id));
    receipt.duplicate_scan_primitives_removed = before_deduplication - candidates.len();
    receipt.accepted_constraints = candidates.len();
    receipt.coverage = candidates.len() as f64 / samples.len() as f64;
    if candidates.len() < options.minimum_constraints || receipt.coverage < options.minimum_coverage
    {
        return automatic_rejection(
            receipt,
            AutomaticConstraintRejection::InsufficientCorrespondences,
        );
    }
    receipt.rms_distance_cm = Some(
        (candidates
            .iter()
            .map(|candidate| candidate.distance_cm.powi(2))
            .sum::<f64>()
            / candidates.len() as f64)
            .sqrt(),
    );
    receipt.maximum_accepted_distance_cm = candidates
        .iter()
        .map(|candidate| candidate.distance_cm)
        .max_by(f64::total_cmp);
    candidates.sort_by_key(|candidate| candidate.template_primitive);
    let correspondences = candidates
        .into_iter()
        .map(|candidate| {
            let scan_primitive = candidate.scan_projection.primitive_id;
            let template_primitive = candidate.template_primitive;
            let template_triangle = template.triangles[template_primitive as usize];
            let template_point = template_triangle
                .map(|vertex| Vec3::from(template.vertices[vertex as usize]))
                .into_iter()
                .fold(Vec3::ZERO, |sum, point| sum + point)
                / 3.0;
            AutomaticSurfaceCorrespondence {
                scan: SurfaceAttachment {
                    triangle_vertex_ids: scan.triangles[scan_primitive as usize],
                    barycentric: candidate.scan_projection.barycentric,
                    primitive_id: Some(scan_primitive),
                },
                template: SurfaceAttachment {
                    triangle_vertex_ids: template.triangles[template_primitive as usize],
                    barycentric: [1.0 / 3.0; 3],
                    primitive_id: Some(template_primitive),
                },
                distance_cm: candidate.distance_cm,
                normal_dot: candidate.normal_dot,
                region: classify_geometry_region(template_point, region_bounds),
                alignment_share: 0.0,
                fit_share: 0.0,
            }
        })
        .collect::<Vec<_>>();
    let mut correspondences = correspondences;
    normalize_automatic_correspondence_weights(&mut correspondences);
    (
        receipt.regional_constraint_counts,
        receipt.regional_alignment_shares,
        receipt.regional_fit_shares,
    ) = automatic_regional_receipt(&correspondences);
    Ok(AutomaticSurfaceConstraintResult {
        correspondences,
        receipt,
    })
}

const AUTOMATIC_REGIONS: [AutomaticConstraintRegion; 6] = [
    AutomaticConstraintRegion::LowerFaceJawChin,
    AutomaticConstraintRegion::ExteriorEnvelope,
    AutomaticConstraintRegion::BroadSurface,
    AutomaticConstraintRegion::InteriorEye,
    AutomaticConstraintRegion::InteriorNose,
    AutomaticConstraintRegion::InteriorLip,
];

pub(crate) fn normalize_automatic_correspondence_weights(
    correspondences: &mut [AutomaticSurfaceCorrespondence],
) {
    let mut counts = BTreeMap::<AutomaticConstraintRegion, usize>::new();
    for pair in correspondences.iter() {
        *counts.entry(pair.region).or_default() += 1;
    }
    let alignment_total = AUTOMATIC_REGIONS
        .iter()
        .filter(|region| counts.contains_key(region))
        .map(|region| region.alignment_budget())
        .sum::<f64>();
    let fit_total = AUTOMATIC_REGIONS
        .iter()
        .filter(|region| counts.contains_key(region))
        .map(|region| region.fit_budget())
        .sum::<f64>();
    for pair in correspondences {
        let count = counts[&pair.region] as f64;
        pair.alignment_share = if alignment_total > 0.0 {
            pair.region.alignment_budget() / alignment_total / count
        } else {
            0.0
        };
        pair.fit_share = if fit_total > 0.0 {
            pair.region.fit_budget() / fit_total / count
        } else {
            0.0
        };
    }
}

fn automatic_regional_receipt(
    correspondences: &[AutomaticSurfaceCorrespondence],
) -> ([usize; 6], [f64; 6], [f64; 6]) {
    let mut counts = [0; 6];
    let mut alignment = [0.0; 6];
    let mut fit = [0.0; 6];
    for (index, region) in AUTOMATIC_REGIONS.iter().enumerate() {
        for pair in correspondences.iter().filter(|pair| pair.region == *region) {
            counts[index] += 1;
            alignment[index] += pair.alignment_share;
            fit[index] += pair.fit_share;
        }
    }
    (counts, alignment, fit)
}

fn surface_bounds(mesh: &Mesh, triangles: &[u32]) -> (Vec3, Vec3) {
    triangles
        .iter()
        .flat_map(|&primitive| mesh.triangles[primitive as usize])
        .fold(
            (
                Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
                Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
            ),
            |(mut minimum, mut maximum), vertex| {
                let point = Vec3::from(mesh.vertices[vertex as usize]);
                minimum.x = minimum.x.min(point.x);
                minimum.y = minimum.y.min(point.y);
                minimum.z = minimum.z.min(point.z);
                maximum.x = maximum.x.max(point.x);
                maximum.y = maximum.y.max(point.y);
                maximum.z = maximum.z.max(point.z);
                (minimum, maximum)
            },
        )
}

fn classify_geometry_region(
    point: Vec3,
    (minimum, maximum): (Vec3, Vec3),
) -> AutomaticConstraintRegion {
    let height = (maximum.y - minimum.y).max(1.0e-12);
    let half_width = ((maximum.x - minimum.x) * 0.5).max(1.0e-12);
    let center_x = (minimum.x + maximum.x) * 0.5;
    let normalized_y = (point.y - minimum.y) / height;
    let lateral = (point.x - center_x).abs() / half_width;
    if normalized_y <= 0.45 {
        AutomaticConstraintRegion::LowerFaceJawChin
    } else if lateral >= 0.72 {
        AutomaticConstraintRegion::ExteriorEnvelope
    } else {
        AutomaticConstraintRegion::BroadSurface
    }
}

fn automatic_rejection(
    mut receipt: AutomaticSurfaceConstraintReceipt,
    reason: AutomaticConstraintRejection,
) -> Result<AutomaticSurfaceConstraintResult, AutomaticSurfaceConstraintError> {
    receipt.rejection_reason = Some(reason);
    Err(AutomaticSurfaceConstraintError {
        receipt: Box::new(receipt),
    })
}

fn valid_automatic_options(options: &AutomaticSurfaceConstraintOptions) -> bool {
    options.sample_count >= 3
        && options.maximum_distance_cm.is_finite()
        && options.maximum_distance_cm > 0.0
        && options.minimum_normal_dot.is_finite()
        && (-1.0..=1.0).contains(&options.minimum_normal_dot)
        && options.trim_fraction.is_finite()
        && (0.0..1.0).contains(&options.trim_fraction)
        && options.minimum_constraints >= 3
        && options.minimum_constraints <= options.sample_count
        && options.minimum_coverage.is_finite()
        && (0.0..=1.0).contains(&options.minimum_coverage)
        && options.alignment_weight.is_finite()
        && options.alignment_weight > 0.0
        && options.fit_weight.is_finite()
        && options.fit_weight > 0.0
        && options.confidence.is_finite()
        && (0.0..=1.0).contains(&options.confidence)
        && options.confidence > 0.0
}

#[derive(Clone, Copy, Debug)]
struct SpatialSample {
    primitive: u32,
    centroid: Vec3,
    normal: Vec3,
    double_area: f64,
    feature_salience: f64,
}

fn deterministic_spatial_samples(mesh: &Mesh, ids: &[u32], requested: usize) -> Vec<u32> {
    let count = ids.len().min(requested);
    if count == 0 {
        return Vec::new();
    }
    if count == ids.len() {
        return ids.to_vec();
    }
    let mut samples = ids
        .iter()
        .map(|&primitive| {
            let triangle = mesh.triangles[primitive as usize];
            let points = triangle.map(|vertex| Vec3::from(mesh.vertices[vertex as usize]));
            let cross = cross(points[1] - points[0], points[2] - points[0]);
            SpatialSample {
                primitive,
                centroid: (points[0] + points[1] + points[2]) / 3.0,
                normal: normalize_or_zero(cross),
                double_area: cross.norm(),
                feature_salience: 0.0,
            }
        })
        .collect::<Vec<_>>();
    assign_feature_salience(mesh, &mut samples);

    let detail_target = if count >= 6 { count / 3 } else { 0 };
    let broad_target = count - detail_target;
    let mut selected = vec![false; samples.len()];
    extend_spatial_coverage(&samples, &mut selected, broad_target);
    select_spatially_diverse_features(&samples, &mut selected, detail_target);

    extend_spatial_coverage(&samples, &mut selected, count);

    let mut primitives = samples
        .iter()
        .zip(selected)
        .filter_map(|(sample, selected)| selected.then_some(sample.primitive))
        .collect::<Vec<_>>();
    primitives.sort_unstable();
    primitives
}

fn assign_feature_salience(mesh: &Mesh, samples: &mut [SpatialSample]) {
    let mut shared_edges = BTreeMap::<(u32, u32), Vec<usize>>::new();
    for (sample_index, sample) in samples.iter().enumerate() {
        let triangle = mesh.triangles[sample.primitive as usize];
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
            shared_edges.entry(edge).or_default().push(sample_index);
        }
    }

    let mut normal_variation = vec![0.0_f64; samples.len()];
    for neighbors in shared_edges.values() {
        for left in 0..neighbors.len() {
            for right in left + 1..neighbors.len() {
                let first = neighbors[left];
                let second = neighbors[right];
                if samples[first].normal.norm_squared() <= 1.0e-12
                    || samples[second].normal.norm_squared() <= 1.0e-12
                {
                    continue;
                }

                let dot = samples[first]
                    .normal
                    .dot(samples[second].normal)
                    .abs()
                    .clamp(0.0, 1.0);
                let bend = 1.0 - dot;
                normal_variation[first] = normal_variation[first].max(bend);
                normal_variation[second] = normal_variation[second].max(bend);
            }
        }
    }

    let curvature_reference = upper_decile(&normal_variation).unwrap_or(0.0);
    let area_reference = median_positive(
        &samples
            .iter()
            .map(|sample| sample.double_area)
            .collect::<Vec<_>>(),
    )
    .unwrap_or(0.0);
    for (sample, curvature) in samples.iter_mut().zip(normal_variation) {
        let curvature = if curvature_reference > 1.0e-12 {
            (curvature / curvature_reference).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let density = if area_reference > 1.0e-12 && sample.double_area > 1.0e-12 {
            (1.0 - sample.double_area / area_reference).clamp(0.0, 1.0)
        } else {
            0.0
        };
        sample.feature_salience = curvature * 0.8 + density * 0.2;
    }
}

fn extend_spatial_coverage(samples: &[SpatialSample], selected: &mut [bool], target_count: usize) {
    let mut selected_count = selected.iter().filter(|selected| **selected).count();
    if selected_count >= target_count {
        return;
    }
    if selected_count == 0 {
        let mean = samples
            .iter()
            .map(|sample| sample.centroid)
            .fold(Vec3::ZERO, |sum, point| sum + point)
            / samples.len() as f64;
        let first = best_index(samples, selected, |_, sample| {
            (sample.centroid - mean).norm_squared()
        })
        .expect("validated candidate list is non-empty");
        selected[first] = true;
        selected_count = 1;
    }

    let mut minimum_distance_squared = samples
        .iter()
        .map(|sample| {
            samples
                .iter()
                .zip(selected.iter().copied())
                .filter(|(_, selected)| *selected)
                .map(|(chosen, _)| (sample.centroid - chosen.centroid).norm_squared())
                .min_by(f64::total_cmp)
                .unwrap_or(f64::INFINITY)
        })
        .collect::<Vec<_>>();
    while selected_count < target_count {
        let next = best_index(samples, selected, |index, _| {
            minimum_distance_squared[index]
        })
        .expect("requested sample count does not exceed candidate count");
        selected[next] = true;
        selected_count += 1;
        for (index, distance) in minimum_distance_squared.iter_mut().enumerate() {
            if !selected[index] {
                *distance =
                    distance.min((samples[index].centroid - samples[next].centroid).norm_squared());
            }
        }
    }
}

fn select_spatially_diverse_features(
    samples: &[SpatialSample],
    selected: &mut [bool],
    requested: usize,
) {
    if requested == 0 {
        return;
    }
    let (minimum, maximum) = centroid_bounds(samples);
    let diagonal = (maximum - minimum).norm().max(1.0e-12);
    let mut detail = Vec::<usize>::new();
    for _ in 0..requested {
        let next = best_index(samples, selected, |_, sample| {
            if sample.feature_salience <= 1.0e-12 {
                return -1.0;
            }
            let spread = detail
                .iter()
                .map(|&index| (sample.centroid - samples[index].centroid).norm())
                .min_by(f64::total_cmp)
                .map_or(1.0, |distance| (distance / diagonal).clamp(0.0, 1.0));
            sample.feature_salience * (0.25 + spread * 0.75)
        });
        let Some(next) = next.filter(|&index| samples[index].feature_salience > 1.0e-12) else {
            break;
        };
        selected[next] = true;
        detail.push(next);
    }
}

fn best_index(
    samples: &[SpatialSample],
    selected: &[bool],
    mut score: impl FnMut(usize, &SpatialSample) -> f64,
) -> Option<usize> {
    let mut best = None::<(usize, f64)>;
    for (index, sample) in samples.iter().enumerate() {
        if selected[index] {
            continue;
        }
        let candidate_score = score(index, sample);
        match best {
            None => best = Some((index, candidate_score)),
            Some((best_index, best_score)) => {
                if candidate_score.total_cmp(&best_score).is_gt()
                    || (candidate_score.total_cmp(&best_score).is_eq()
                        && sample.primitive < samples[best_index].primitive)
                {
                    best = Some((index, candidate_score));
                }
            }
        }
    }
    best.map(|(index, _)| index)
}

fn centroid_bounds(samples: &[SpatialSample]) -> (Vec3, Vec3) {
    samples.iter().fold(
        (
            Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
            Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        ),
        |(mut minimum, mut maximum), sample| {
            minimum.x = minimum.x.min(sample.centroid.x);
            minimum.y = minimum.y.min(sample.centroid.y);
            minimum.z = minimum.z.min(sample.centroid.z);
            maximum.x = maximum.x.max(sample.centroid.x);
            maximum.y = maximum.y.max(sample.centroid.y);
            maximum.z = maximum.z.max(sample.centroid.z);
            (minimum, maximum)
        },
    )
}

fn upper_decile(values: &[f64]) -> Option<f64> {
    let mut values = values
        .iter()
        .copied()
        .filter(|value| value.is_finite() && *value > 0.0)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(values[(values.len() - 1) * 9 / 10])
}

fn median_positive(values: &[f64]) -> Option<f64> {
    let mut values = values
        .iter()
        .copied()
        .filter(|value| value.is_finite() && *value > 0.0)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[middle - 1] + values[middle]) * 0.5)
    } else {
        Some(values[middle])
    }
}

fn inverse_apply(transform: SimilarityTransform, point: Vec3) -> Vec3 {
    transform
        .rotation
        .transpose()
        .transform_vector((point - transform.translation) / transform.scale)
}

fn normalize_or_zero(value: Vec3) -> Vec3 {
    let length = value.norm();
    if length > 1.0e-12 {
        value / length
    } else {
        Vec3::ZERO
    }
}

fn cross(first: Vec3, second: Vec3) -> Vec3 {
    Vec3::new(
        first.y * second.z - first.z * second.y,
        first.z * second.x - first.x * second.z,
        first.x * second.y - first.y * second.x,
    )
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[middle - 1] + values[middle]) * 0.5)
    } else {
        Some(values[middle])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ReliefFixture {
        mesh: Mesh,
        candidate_ids: Vec<u32>,
        left_detail: BTreeSet<u32>,
        right_detail: BTreeSet<u32>,
    }

    fn relief_fixture() -> ReliefFixture {
        const SIDE: usize = 9;
        let left_peak = 4 * SIDE + 2;
        let right_peak = 4 * SIDE + 6;
        let vertices = (0..SIDE)
            .flat_map(|row| {
                (0..SIDE).map(move |column| {
                    let index = row * SIDE + column;
                    let relief = if index == left_peak {
                        1.4
                    } else if index == right_peak {
                        1.15
                    } else {
                        0.0
                    };
                    [column as f64, row as f64, relief]
                })
            })
            .collect::<Vec<_>>();
        let mut triangles = Vec::<[u32; 3]>::new();
        for row in 0..SIDE - 1 {
            for column in 0..SIDE - 1 {
                let lower_left = (row * SIDE + column) as u32;
                let lower_right = lower_left + 1;
                let upper_left = lower_left + SIDE as u32;
                let upper_right = upper_left + 1;
                triangles.push([lower_left, lower_right, upper_right]);
                triangles.push([lower_left, upper_right, upper_left]);
            }
        }
        let detail_triangles = |vertex: usize| {
            triangles
                .iter()
                .enumerate()
                .filter_map(|(primitive, triangle)| {
                    triangle
                        .contains(&(vertex as u32))
                        .then_some(primitive as u32)
                })
                .collect::<BTreeSet<_>>()
        };
        let left_detail = detail_triangles(left_peak);
        let right_detail = detail_triangles(right_peak);
        let candidate_ids = (0..triangles.len() as u32).collect::<Vec<_>>();
        ReliefFixture {
            mesh: Mesh::new(vertices, triangles).unwrap(),
            candidate_ids,
            left_detail,
            right_detail,
        }
    }

    #[test]
    fn feature_aware_samples_are_deterministic_and_cover_both_relief_regions() {
        let fixture = relief_fixture();
        let selected = deterministic_spatial_samples(&fixture.mesh, &fixture.candidate_ids, 18);
        let mut reversed = fixture.candidate_ids.clone();
        reversed.reverse();
        assert_eq!(
            deterministic_spatial_samples(&fixture.mesh, &reversed, 18),
            selected
        );
        assert_eq!(selected.len(), 18);
        assert!(
            selected
                .iter()
                .filter(|primitive| fixture.left_detail.contains(primitive))
                .count()
                >= 2,
            "the left high-curvature relief must receive multiple samples"
        );
        assert!(
            selected
                .iter()
                .filter(|primitive| fixture.right_detail.contains(primitive))
                .count()
                >= 2,
            "the right high-curvature relief must receive multiple samples"
        );
        let selected_centroids = selected
            .iter()
            .map(|&primitive| {
                let triangle = fixture.mesh.triangles[primitive as usize];
                triangle
                    .map(|vertex| Vec3::from(fixture.mesh.vertices[vertex as usize]))
                    .into_iter()
                    .fold(Vec3::ZERO, |sum, point| sum + point)
                    / 3.0
            })
            .collect::<Vec<_>>();
        assert!(selected_centroids.iter().any(|point| point.x < 1.0));
        assert!(selected_centroids.iter().any(|point| point.x > 7.0));
        assert!(selected_centroids.iter().any(|point| point.y < 1.0));
        assert!(selected_centroids.iter().any(|point| point.y > 7.0));
    }

    #[test]
    fn feature_aware_zero_pin_constraints_remain_valid_on_identity_surface() {
        let fixture = relief_fixture();
        let options = AutomaticSurfaceConstraintOptions {
            sample_count: 24,
            maximum_distance_cm: 1.0e-6,
            minimum_normal_dot: 0.99,
            trim_fraction: 0.0,
            minimum_constraints: 20,
            minimum_coverage: 0.80,
            ..Default::default()
        };
        let first = build_automatic_surface_correspondences(
            &fixture.mesh,
            &fixture.mesh,
            &fixture.candidate_ids,
            SimilarityTransform::IDENTITY,
            &options,
        )
        .unwrap();
        let second = build_automatic_surface_correspondences(
            &fixture.mesh,
            &fixture.mesh,
            &fixture.candidate_ids,
            SimilarityTransform::IDENTITY,
            &options,
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.receipt.sampled_triangles, 24);
        assert_eq!(first.receipt.projected_candidates, 24);
        assert!(first.receipt.accepted_constraints >= 20);
        assert!(first.receipt.coverage >= 0.80);
        assert_eq!(first.receipt.rejection_reason, None);
        let template_primitives = first
            .correspondences
            .iter()
            .filter_map(|pair| pair.template.primitive_id)
            .collect::<BTreeSet<_>>();
        assert!(!template_primitives.is_disjoint(&fixture.left_detail));
        assert!(!template_primitives.is_disjoint(&fixture.right_detail));
    }

    #[test]
    fn automatic_default_reserves_a_quality_first_projection_budget() {
        assert_eq!(
            AutomaticSurfaceConstraintOptions::default().sample_count,
            160
        );
    }

    fn regional_pair(
        region: AutomaticConstraintRegion,
        primitive: u32,
    ) -> AutomaticSurfaceCorrespondence {
        let attachment = SurfaceAttachment {
            triangle_vertex_ids: [0, 1, 2],
            barycentric: [1.0 / 3.0; 3],
            primitive_id: Some(primitive),
        };
        AutomaticSurfaceCorrespondence {
            scan: attachment.clone(),
            template: attachment,
            distance_cm: 0.0,
            normal_dot: 1.0,
            region,
            alignment_share: 0.0,
            fit_share: 0.0,
        }
    }

    #[test]
    fn automatic_regional_weight_totals_are_count_invariant() {
        let mut sparse = AUTOMATIC_REGIONS
            .iter()
            .enumerate()
            .map(|(index, &region)| regional_pair(region, index as u32))
            .collect::<Vec<_>>();
        let mut dense = sparse.clone();
        dense.extend(
            (0..300)
                .map(|index| regional_pair(AutomaticConstraintRegion::BroadSurface, 100 + index)),
        );
        dense.extend(
            (0..200)
                .map(|index| regional_pair(AutomaticConstraintRegion::InteriorEye, 500 + index)),
        );
        AutomaticSurfaceCorrespondence::normalize_weights(&mut sparse);
        AutomaticSurfaceCorrespondence::normalize_weights(&mut dense);
        let (_, sparse_alignment, sparse_fit) =
            AutomaticSurfaceCorrespondence::regional_receipt(&sparse);
        let (_, dense_alignment, dense_fit) =
            AutomaticSurfaceCorrespondence::regional_receipt(&dense);
        for index in 0..AUTOMATIC_REGIONS.len() {
            assert!((sparse_alignment[index] - dense_alignment[index]).abs() <= 1.0e-12);
            assert!((sparse_fit[index] - dense_fit[index]).abs() <= 1.0e-12);
        }
        assert!(
            (dense.iter().map(|pair| pair.alignment_share).sum::<f64>() - 1.0).abs() <= 1.0e-12
        );
        assert!((dense.iter().map(|pair| pair.fit_share).sum::<f64>() - 1.0).abs() <= 1.0e-12);
        assert_eq!(
            dense_fit[3], 0.0,
            "eye points must not force local skin fit"
        );
        assert_eq!(
            dense_fit[5], 0.0,
            "lip points must not force local skin fit"
        );
    }

    fn box_mesh(minimum: Vec3, maximum: Vec3) -> Mesh {
        let vertices = vec![
            [minimum.x, minimum.y, minimum.z],
            [maximum.x, minimum.y, minimum.z],
            [minimum.x, maximum.y, minimum.z],
            [maximum.x, maximum.y, minimum.z],
            [minimum.x, minimum.y, maximum.z],
            [maximum.x, minimum.y, maximum.z],
            [minimum.x, maximum.y, maximum.z],
            [maximum.x, maximum.y, maximum.z],
        ];
        Mesh::new(
            vertices,
            vec![
                [0, 1, 2],
                [1, 3, 2],
                [4, 6, 5],
                [5, 6, 7],
                [0, 4, 1],
                [1, 4, 5],
                [2, 3, 6],
                [3, 7, 6],
                [0, 2, 4],
                [2, 6, 4],
                [1, 5, 3],
                [3, 5, 7],
            ],
        )
        .unwrap()
    }

    fn cropped_template() -> [Vec3; 4] {
        [
            Vec3::new(-5.0, 100.0, -2.0),
            Vec3::new(5.0, 100.0, -2.0),
            Vec3::new(-5.0, 130.0, 3.0),
            Vec3::new(5.0, 130.0, 3.0),
        ]
    }

    fn face_plane_mesh(central_duplicates: usize) -> Mesh {
        const COLUMNS: usize = 25;
        const ROWS: usize = 25;
        let mut vertices = Vec::with_capacity(COLUMNS * ROWS + central_duplicates);
        for row in 0..ROWS {
            for column in 0..COLUMNS {
                vertices.push([
                    -1.0 + 2.0 * column as f64 / (COLUMNS - 1) as f64,
                    -2.0 + 4.0 * row as f64 / (ROWS - 1) as f64,
                    0.0,
                ]);
            }
        }
        let index = |row: usize, column: usize| (row * COLUMNS + column) as u32;
        let mut triangles = Vec::with_capacity((COLUMNS - 1) * (ROWS - 1) * 2 + central_duplicates);
        for row in 0..ROWS - 1 {
            for column in 0..COLUMNS - 1 {
                let lower_left = index(row, column);
                let lower_right = index(row, column + 1);
                let upper_left = index(row + 1, column);
                let upper_right = index(row + 1, column + 1);
                triangles.push([lower_left, lower_right, upper_left]);
                triangles.push([lower_right, upper_right, upper_left]);
            }
        }

        let center = index(ROWS / 2, COLUMNS / 2);
        let neighbor = index(ROWS / 2, COLUMNS / 2 + 1);
        let center_position = vertices[center as usize];
        for _ in 0..central_duplicates {
            let duplicate = vertices.len() as u32;
            vertices.push(center_position);
            triangles.push([center, duplicate, neighbor]);
        }
        Mesh::new(vertices, triangles).unwrap()
    }

    #[test]
    fn zero_pin_extent_scale_ignores_nonuniform_facial_tessellation_density() {
        let uniform = face_plane_mesh(0);
        let dense_center = face_plane_mesh(640);
        let template = uniform
            .vertices
            .iter()
            .map(|point| Vec3::new(point[0] * 5.0, point[1] * 5.0, point[2] * 5.0))
            .collect::<Vec<_>>();

        let uniform_suggestion =
            suggest_zero_pin_extent_similarity(Mat3::IDENTITY, &uniform, &template).unwrap();
        let dense_suggestion =
            suggest_zero_pin_extent_similarity(Mat3::IDENTITY, &dense_center, &template).unwrap();

        assert!((uniform_suggestion.scale - 5.0).abs() <= 1.0e-12);
        assert_eq!(dense_suggestion, uniform_suggestion);
    }

    #[test]
    fn zero_pin_extent_prior_uses_width_and_ignores_neck_crop_height() {
        let scan = box_mesh(Vec3::new(-1.0, -2.0, -0.5), Vec3::new(1.0, 2.0, 0.5));
        let shorter_neck = box_mesh(Vec3::new(-1.0, -1.0, -0.5), Vec3::new(1.0, 2.0, 0.5));

        let template = cropped_template();
        let prior = SimilarityTransform {
            scale: 5.5,
            rotation: Mat3::IDENTITY,
            translation: Vec3::new(0.2, 119.0, -0.2),
        };
        let result =
            refine_zero_pin_extent_prior(prior, &scan, &template, &AssistedFitOptions::default());
        assert!((result.transform.scale - 5.0).abs() <= 1.0e-12);
        assert!((result.transform.translation.x - 0.0).abs() <= 1.0e-12);
        assert!((result.transform.translation.y - 120.0).abs() <= 1.0e-12);
        assert!((result.transform.translation.z - 0.15).abs() <= 1.0e-12);
        assert!(!result.was_clamped);
        assert_eq!(
            refine_zero_pin_extent_prior(
                prior,
                &shorter_neck,
                &template,
                &AssistedFitOptions::default()
            ),
            result,
            "changing only the lower neck crop must not change face alignment"
        );

        let suggested = suggest_zero_pin_extent_similarity(Mat3::IDENTITY, &scan, &template)
            .expect("valid head extents produce an alignment suggestion");
        let shorter_suggested =
            suggest_zero_pin_extent_similarity(Mat3::IDENTITY, &shorter_neck, &template)
                .expect("a different lower neck crop remains alignable");
        assert_eq!(suggested, shorter_suggested);
        assert!((suggested.scale - 5.0).abs() <= 1.0e-12);
        assert_eq!(suggested.translation, Vec3::new(0.0, 120.0, 0.5));
    }

    #[test]
    fn zero_pin_extent_prior_adapts_to_face_width_and_bounds_large_errors() {
        let narrow = box_mesh(Vec3::new(-0.8, -2.0, -0.5), Vec3::new(0.8, 2.0, 0.5));
        let template = cropped_template();
        let options = AssistedFitOptions::default();
        let adapted = refine_zero_pin_extent_prior(
            SimilarityTransform {
                scale: 7.0,
                rotation: Mat3::IDENTITY,
                translation: Vec3::new(0.0, 115.0, 0.0),
            },
            &narrow,
            &template,
            &options,
        );
        assert!((adapted.transform.scale - 6.25).abs() <= 1.0e-12);

        let bounded = refine_zero_pin_extent_prior(
            SimilarityTransform {
                scale: 20.0,
                rotation: Mat3::IDENTITY,
                translation: Vec3::ZERO,
            },
            &narrow,
            &template,
            &options,
        );
        assert!((bounded.scale_ratio - 0.88).abs() <= 1.0e-12);
        assert!(bounded.translation_delta.norm() <= 1.875 + 1.0e-12);
        assert!(bounded.was_clamped);
    }

    #[test]
    fn zero_pin_extent_prior_is_noop_for_correct_rotated_alignment() {
        let canonical = box_mesh(Vec3::new(-1.0, -2.0, -0.5), Vec3::new(1.0, 2.0, 0.5));
        let rotation = Mat3::from_rows([[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]]);
        let inverse = rotation.transpose();
        let rotated = Mesh::new(
            canonical
                .vertices
                .iter()
                .map(|point| inverse.transform_vector(Vec3::from(*point)).to_array())
                .collect(),
            canonical.triangles.clone(),
        )
        .unwrap();
        let prior = SimilarityTransform {
            scale: 2.0,
            rotation,
            translation: Vec3::new(3.0, 100.0, 4.0),
        };
        let template = rotated
            .vertices
            .iter()
            .map(|point| prior.apply(Vec3::from(*point)))
            .collect::<Vec<_>>();
        let result = refine_zero_pin_extent_prior(
            prior,
            &rotated,
            &template,
            &AssistedFitOptions::default(),
        );
        assert_eq!(result.transform, prior);
        assert!((result.scale_ratio - 1.0).abs() <= 1.0e-12);
        assert_eq!(result.translation_delta, Vec3::ZERO);
        assert!(!result.was_clamped);
    }
}
