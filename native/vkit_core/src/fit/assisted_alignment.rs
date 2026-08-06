use std::fmt;

use nalgebra::{Matrix3, Rotation3, UnitQuaternion};

use crate::formats::Mesh;
use crate::math::{Mat3, SimilarityTransform, Vec3, estimate_similarity};
use crate::spatial::SurfaceProjector;

use super::dense_registration::{largest_component_triangle_ids, vertex_normals};

pub const MIN_ASSISTED_FIT_PAIRS: usize = 4;

pub const RECOMMENDED_ASSISTED_FIT_PAIRS: usize = 6;

const MAXIMUM_ASSISTED_HYPOTHESES: usize = 4_096;
const MAXIMUM_ASSISTED_IRLS_ITERATIONS: usize = 8;
const MAXIMUM_ASSISTED_ICP_ITERATIONS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssistedFitOptions {
    pub maximum_hypotheses: usize,
    pub pin_inlier_distance_cm: f64,
    pub pin_inlier_span_fraction: f64,
    pub minimum_inlier_fraction: f64,
    pub minimum_layout_spread: f64,
    pub irls_iterations: usize,
    pub mad_multiplier: f64,
    pub outlier_weight_floor: f64,
    pub icp_iterations: usize,
    pub icp_sample_count: usize,
    pub icp_max_distance_cm: f64,
    pub icp_min_normal_dot: f64,
    pub icp_trim_fraction: f64,
    pub icp_huber_delta_cm: f64,
    pub icp_minimum_correspondences: usize,
    pub icp_minimum_coverage: f64,
    pub icp_max_translation_step_cm: f64,
    pub icp_max_rotation_step_radians: f64,
    pub icp_max_scale_step: f64,
    pub icp_minimum_rms_improvement_cm: f64,
}

impl Default for AssistedFitOptions {
    fn default() -> Self {
        Self {
            maximum_hypotheses: 512,
            pin_inlier_distance_cm: 0.35,
            pin_inlier_span_fraction: 0.025,
            minimum_inlier_fraction: 0.60,
            minimum_layout_spread: 0.015,
            irls_iterations: 4,
            mad_multiplier: 2.5,
            outlier_weight_floor: 1.0e-8,
            icp_iterations: 3,
            icp_sample_count: 768,
            icp_max_distance_cm: 2.5,
            icp_min_normal_dot: -0.10,
            icp_trim_fraction: 0.10,
            icp_huber_delta_cm: 0.75,
            icp_minimum_correspondences: 48,
            icp_minimum_coverage: 0.15,
            icp_max_translation_step_cm: 0.75,
            icp_max_rotation_step_radians: 3.0_f64.to_radians(),
            icp_max_scale_step: 0.04,
            icp_minimum_rms_improvement_cm: 1.0e-5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistedAlignmentRejection {
    TooFewPairs,
    InvalidInput,
    InvalidOptions,
    DegeneratePinLayout,
    InsufficientRobustInliers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistedInitializationMode {
    RobustHypotheses,
    GuardedWeightedPrior,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardedWeightedFallbackRejection {
    InvalidPrior,
    InvalidCandidate,
    InsufficientEffectiveSupport,
    ExcessivePriorStep,
    NoResidualImprovement,
    ExcessiveResidual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistedIcpSkipReason {
    AuthoritativeUserPins,
    NonCanonicalTemplate,
    InvalidOptions,
    InvalidSurface,
    ProjectionUnavailable,
    InsufficientCorrespondences,
    NoImprovement,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssistedIcpIterationReceipt {
    pub correspondences: usize,
    pub coverage: f64,
    pub rms_before_cm: f64,
    pub rms_after_cm: f64,
    pub accepted: bool,
    pub step_was_clamped: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssistedFitReceipt {
    pub pair_count: usize,
    pub inlier_count: usize,
    pub outlier_count: usize,
    pub hypothesis_count: usize,
    pub irls_iterations: usize,
    pub source_spread: f64,
    pub target_spread: f64,
    pub pin_inlier_threshold_cm: f64,
    pub pin_rms_cm: Option<f64>,
    pub pin_all_rms_cm: Option<f64>,
    pub robust_scale_cm: Option<f64>,
    pub initialization_mode: AssistedInitializationMode,
    pub fallback_rejection_reason: Option<GuardedWeightedFallbackRejection>,
    pub fallback_prior_rms_cm: Option<f64>,
    pub fallback_candidate_rms_cm: Option<f64>,
    pub fallback_maximum_rms_cm: Option<f64>,
    pub rejection_reason: Option<AssistedAlignmentRejection>,
    pub icp_skip_reason: Option<AssistedIcpSkipReason>,
    pub icp_iterations: Vec<AssistedIcpIterationReceipt>,
}

impl AssistedFitReceipt {
    fn empty(pair_count: usize) -> Self {
        Self {
            pair_count,
            inlier_count: 0,
            outlier_count: pair_count,
            hypothesis_count: 0,
            irls_iterations: 0,
            source_spread: 0.0,
            target_spread: 0.0,
            pin_inlier_threshold_cm: 0.0,
            pin_rms_cm: None,
            pin_all_rms_cm: None,
            robust_scale_cm: None,
            initialization_mode: AssistedInitializationMode::RobustHypotheses,
            fallback_rejection_reason: None,
            fallback_prior_rms_cm: None,
            fallback_candidate_rms_cm: None,
            fallback_maximum_rms_cm: None,
            rejection_reason: None,
            icp_skip_reason: None,
            icp_iterations: Vec::new(),
        }
    }
}

pub fn estimate_assisted_similarity_with_prior(
    source: &[Vec3],
    target: &[Vec3],
    base_weights: &[f64],
    prior: SimilarityTransform,
    options: &AssistedFitOptions,
) -> Result<AssistedAlignmentResult, AssistedAlignmentError> {
    match estimate_assisted_similarity(source, target, base_weights, options) {
        Ok(result) => Ok(result),
        Err(error)
            if error.receipt.rejection_reason
                == Some(AssistedAlignmentRejection::InsufficientRobustInliers) =>
        {
            guarded_weighted_prior_fallback(
                *error.receipt,
                source,
                target,
                base_weights,
                prior,
                options,
            )
        }
        Err(error) => Err(error),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssistedAlignmentResult {
    pub transform: SimilarityTransform,

    pub pin_weight_multipliers: Vec<f64>,
    pub receipt: AssistedFitReceipt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AssistedAlignmentError {
    pub receipt: Box<AssistedFitReceipt>,
}

impl fmt::Display for AssistedAlignmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "assisted similarity initialization rejected: {:?}",
            self.receipt.rejection_reason
        )?;
        if let Some(reason) = self.receipt.fallback_rejection_reason {
            write!(
                formatter,
                "; guarded weighted fallback rejected: {reason:?} (prior_rms_cm={:?}, candidate_rms_cm={:?}, maximum_rms_cm={:?})",
                self.receipt.fallback_prior_rms_cm,
                self.receipt.fallback_candidate_rms_cm,
                self.receipt.fallback_maximum_rms_cm,
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for AssistedAlignmentError {}

#[derive(Clone, Copy)]
struct HypothesisScore {
    transform: SimilarityTransform,
    inlier_count: usize,
    inlier_weight: f64,
    median_residual: f64,
    rms_residual: f64,
}

pub fn estimate_assisted_similarity(
    source: &[Vec3],
    target: &[Vec3],
    base_weights: &[f64],
    options: &AssistedFitOptions,
) -> Result<AssistedAlignmentResult, AssistedAlignmentError> {
    let mut receipt = AssistedFitReceipt::empty(source.len());
    if source.len() != target.len()
        || source.len() != base_weights.len()
        || source.iter().any(|point| !point.is_finite())
        || target.iter().any(|point| !point.is_finite())
        || base_weights
            .iter()
            .any(|weight| !weight.is_finite() || *weight <= 0.0)
    {
        return reject(receipt, AssistedAlignmentRejection::InvalidInput);
    }
    if source.len() < MIN_ASSISTED_FIT_PAIRS {
        return reject(receipt, AssistedAlignmentRejection::TooFewPairs);
    }
    if !valid_options(options) {
        return reject(receipt, AssistedAlignmentRejection::InvalidOptions);
    }

    let source_span = maximum_pair_distance(source);
    let target_span = maximum_pair_distance(target);
    receipt.source_spread = layout_spread(source, source_span);
    receipt.target_spread = layout_spread(target, target_span);
    if source_span <= f64::EPSILON
        || target_span <= f64::EPSILON
        || receipt.source_spread < options.minimum_layout_spread
        || receipt.target_spread < options.minimum_layout_spread
    {
        return reject(receipt, AssistedAlignmentRejection::DegeneratePinLayout);
    }

    let threshold = options
        .pin_inlier_distance_cm
        .max(target_span * options.pin_inlier_span_fraction);
    receipt.pin_inlier_threshold_cm = threshold;
    let triples = bounded_ranked_triples(
        source,
        target,
        source_span,
        target_span,
        options.maximum_hypotheses,
    )
    .triples;
    receipt.hypothesis_count = triples.len();
    if triples.is_empty() {
        return reject(receipt, AssistedAlignmentRejection::DegeneratePinLayout);
    }

    let mut best: Option<HypothesisScore> = None;
    for (_, [first, second, third]) in triples {
        let indices = [first, second, third];
        let hypothesis_source = indices.map(|index| source[index]);
        let hypothesis_target = indices.map(|index| target[index]);
        let hypothesis_weights = indices.map(|index| base_weights[index]);
        let Ok(transform) = estimate_similarity(
            &hypothesis_source,
            &hypothesis_target,
            Some(&hypothesis_weights),
        ) else {
            continue;
        };
        if !valid_similarity(transform) {
            continue;
        }
        let residuals = similarity_residuals(transform, source, target);
        let inliers = residuals
            .iter()
            .map(|residual| *residual <= threshold)
            .collect::<Vec<_>>();
        let selected = residuals
            .iter()
            .copied()
            .zip(&inliers)
            .filter_map(|(residual, inlier)| inlier.then_some(residual))
            .collect::<Vec<_>>();
        let score = HypothesisScore {
            transform,
            inlier_count: selected.len(),
            inlier_weight: inliers
                .iter()
                .zip(base_weights)
                .filter_map(|(inlier, weight)| inlier.then_some(*weight))
                .sum(),
            median_residual: median(&selected).unwrap_or(f64::INFINITY),
            rms_residual: rms(&selected).unwrap_or(f64::INFINITY),
        };
        if best.is_none_or(|current| hypothesis_is_better(score, current)) {
            best = Some(score);
        }
    }
    let Some(best) = best else {
        return reject(receipt, AssistedAlignmentRejection::DegeneratePinLayout);
    };
    let minimum_inliers = MIN_ASSISTED_FIT_PAIRS
        .max((source.len() as f64 * options.minimum_inlier_fraction).ceil() as usize);
    if best.inlier_count < minimum_inliers {
        receipt.inlier_count = best.inlier_count;
        receipt.outlier_count = source.len() - best.inlier_count;
        return reject(
            receipt,
            AssistedAlignmentRejection::InsufficientRobustInliers,
        );
    }

    let mut transform = best.transform;
    let mut multipliers = vec![options.outlier_weight_floor; source.len()];
    let mut robust_scale = threshold;
    for iteration in 0..options.irls_iterations {
        let residuals = similarity_residuals(transform, source, target);
        let seed_inliers = residuals
            .iter()
            .copied()
            .filter(|residual| *residual <= threshold)
            .collect::<Vec<_>>();
        if seed_inliers.len() < minimum_inliers {
            break;
        }
        let center = median(&seed_inliers).unwrap_or(0.0);
        let deviations = seed_inliers
            .iter()
            .map(|residual| (*residual - center).abs())
            .collect::<Vec<_>>();
        robust_scale = (1.4826 * median(&deviations).unwrap_or(0.0))
            .max(threshold * 0.10)
            .min(threshold);
        let huber_delta = (options.mad_multiplier * robust_scale)
            .max(threshold * 0.25)
            .min(threshold);
        for (index, residual) in residuals.iter().copied().enumerate() {
            multipliers[index] = if residual <= threshold {
                huber_weight(residual, huber_delta)
            } else {
                options.outlier_weight_floor
            };
        }
        let weights = base_weights
            .iter()
            .zip(&residuals)
            .map(|(base, residual)| {
                if *residual <= threshold {
                    base * huber_weight(*residual, huber_delta)
                } else {
                    0.0
                }
            })
            .collect::<Vec<_>>();
        let Ok(candidate) = estimate_similarity(source, target, Some(&weights)) else {
            break;
        };
        if !valid_similarity(candidate) {
            break;
        }
        transform = candidate;
        receipt.irls_iterations = iteration + 1;
    }

    let residuals = similarity_residuals(transform, source, target);
    let inlier_mask = residuals
        .iter()
        .map(|residual| *residual <= threshold)
        .collect::<Vec<_>>();
    receipt.inlier_count = inlier_mask.iter().filter(|inlier| **inlier).count();
    receipt.outlier_count = source.len() - receipt.inlier_count;
    if receipt.inlier_count < minimum_inliers {
        return reject(
            receipt,
            AssistedAlignmentRejection::InsufficientRobustInliers,
        );
    }
    for (index, inlier) in inlier_mask.iter().copied().enumerate() {
        if !inlier {
            multipliers[index] = options.outlier_weight_floor;
        }
    }
    let inlier_residuals = residuals
        .iter()
        .copied()
        .zip(&inlier_mask)
        .filter_map(|(residual, inlier)| inlier.then_some(residual))
        .collect::<Vec<_>>();
    receipt.pin_rms_cm = rms(&inlier_residuals);
    receipt.pin_all_rms_cm = rms(&residuals);
    receipt.robust_scale_cm = Some(robust_scale);
    Ok(AssistedAlignmentResult {
        transform,
        pin_weight_multipliers: multipliers,
        receipt,
    })
}

fn guarded_weighted_prior_fallback(
    mut receipt: AssistedFitReceipt,
    source: &[Vec3],
    target: &[Vec3],
    base_weights: &[f64],
    prior: SimilarityTransform,
    options: &AssistedFitOptions,
) -> Result<AssistedAlignmentResult, AssistedAlignmentError> {
    if !valid_rigid_similarity(prior) {
        receipt.fallback_rejection_reason = Some(GuardedWeightedFallbackRejection::InvalidPrior);
        return Err(AssistedAlignmentError {
            receipt: Box::new(receipt),
        });
    }

    let mut transform = match estimate_similarity(source, target, Some(base_weights)) {
        Ok(candidate) if valid_rigid_similarity(candidate) => candidate,
        _ => {
            receipt.fallback_rejection_reason =
                Some(GuardedWeightedFallbackRejection::InvalidCandidate);
            return Err(AssistedAlignmentError {
                receipt: Box::new(receipt),
            });
        }
    };
    let threshold = receipt.pin_inlier_threshold_cm;
    let target_span = maximum_pair_distance(target);
    let maximum_rms = (threshold * 3.0).max(target_span * 0.10);
    receipt.fallback_maximum_rms_cm = Some(maximum_rms);

    let mut multipliers = vec![1.0; source.len()];
    for _ in 0..options.irls_iterations {
        let residuals = similarity_residuals(transform, source, target);
        multipliers = soft_fallback_multipliers(&residuals, threshold, options);
        let effective_weights = base_weights
            .iter()
            .zip(&multipliers)
            .map(|(base, multiplier)| base * multiplier)
            .collect::<Vec<_>>();
        let Ok(candidate) = estimate_similarity(source, target, Some(&effective_weights)) else {
            break;
        };
        if !valid_rigid_similarity(candidate) {
            break;
        }
        transform = candidate;
    }

    let residuals = similarity_residuals(transform, source, target);
    multipliers = soft_fallback_multipliers(&residuals, threshold, options);
    let effective_weights = base_weights
        .iter()
        .zip(&multipliers)
        .map(|(base, multiplier)| base * multiplier)
        .collect::<Vec<_>>();
    let base_weight = base_weights.iter().sum::<f64>();
    let effective_weight = effective_weights.iter().sum::<f64>();
    if !effective_weight.is_finite() || effective_weight < base_weight * 0.35 {
        receipt.fallback_rejection_reason =
            Some(GuardedWeightedFallbackRejection::InsufficientEffectiveSupport);
        return Err(AssistedAlignmentError {
            receipt: Box::new(receipt),
        });
    }

    let prior_residuals = similarity_residuals(prior, source, target);
    let prior_rms = weighted_rms(&prior_residuals, &effective_weights);
    let candidate_rms = weighted_rms(&residuals, &effective_weights);
    receipt.fallback_prior_rms_cm = prior_rms;
    receipt.fallback_candidate_rms_cm = candidate_rms;
    let (Some(prior_rms), Some(candidate_rms)) = (prior_rms, candidate_rms) else {
        receipt.fallback_rejection_reason =
            Some(GuardedWeightedFallbackRejection::InvalidCandidate);
        return Err(AssistedAlignmentError {
            receipt: Box::new(receipt),
        });
    };

    if !fallback_step_is_conservative(prior, transform, source, base_weights, target_span) {
        receipt.fallback_rejection_reason =
            Some(GuardedWeightedFallbackRejection::ExcessivePriorStep);
        return Err(AssistedAlignmentError {
            receipt: Box::new(receipt),
        });
    }
    let minimum_improvement = (prior_rms * 1.0e-3).max(1.0e-6);
    if candidate_rms + minimum_improvement >= prior_rms {
        receipt.fallback_rejection_reason =
            Some(GuardedWeightedFallbackRejection::NoResidualImprovement);
        return Err(AssistedAlignmentError {
            receipt: Box::new(receipt),
        });
    }
    if candidate_rms > maximum_rms {
        receipt.fallback_rejection_reason =
            Some(GuardedWeightedFallbackRejection::ExcessiveResidual);
        return Err(AssistedAlignmentError {
            receipt: Box::new(receipt),
        });
    }

    receipt.initialization_mode = AssistedInitializationMode::GuardedWeightedPrior;
    receipt.fallback_rejection_reason = None;
    receipt.rejection_reason = None;
    receipt.pin_all_rms_cm = Some(candidate_rms);
    receipt.pin_rms_cm = Some(candidate_rms);
    receipt.robust_scale_cm = Some(threshold);
    Ok(AssistedAlignmentResult {
        transform,
        pin_weight_multipliers: multipliers,
        receipt,
    })
}

fn soft_fallback_multipliers(
    residuals: &[f64],
    threshold: f64,
    options: &AssistedFitOptions,
) -> Vec<f64> {
    let center = median(residuals).unwrap_or(threshold);
    let deviations = residuals
        .iter()
        .map(|residual| (*residual - center).abs())
        .collect::<Vec<_>>();
    let robust_scale = (1.4826 * median(&deviations).unwrap_or(0.0))
        .max(threshold * 0.10)
        .min(threshold * 3.0);
    let huber_delta = (options.mad_multiplier * robust_scale)
        .max(threshold)
        .min(threshold * 3.0);
    residuals
        .iter()
        .map(|residual| huber_weight(*residual, huber_delta).max(options.outlier_weight_floor))
        .collect()
}

fn weighted_rms(residuals: &[f64], weights: &[f64]) -> Option<f64> {
    if residuals.len() != weights.len() || residuals.is_empty() {
        return None;
    }
    let total_weight = weights.iter().sum::<f64>();
    if !total_weight.is_finite() || total_weight <= 0.0 {
        return None;
    }
    let squared = residuals
        .iter()
        .zip(weights)
        .map(|(residual, weight)| residual.powi(2) * weight)
        .sum::<f64>();
    let result = (squared / total_weight).sqrt();
    result.is_finite().then_some(result)
}

fn fallback_step_is_conservative(
    prior: SimilarityTransform,
    candidate: SimilarityTransform,
    source: &[Vec3],
    weights: &[f64],
    target_span: f64,
) -> bool {
    let scale_ratio = candidate.scale / prior.scale;
    if !(0.67..=1.50).contains(&scale_ratio) {
        return false;
    }
    let prior_rotation = matrix_to_na(prior.rotation);
    let candidate_rotation = matrix_to_na(candidate.rotation);
    let relative =
        Rotation3::from_matrix_unchecked(candidate_rotation * prior_rotation.transpose());
    if UnitQuaternion::from_rotation_matrix(&relative).angle() > 35.0_f64.to_radians() {
        return false;
    }
    let center = weighted_mean(source, weights);
    let center_step = (candidate.apply(center) - prior.apply(center)).norm();
    center_step <= (target_span * 0.35).max(2.5)
}

fn weighted_mean(points: &[Vec3], weights: &[f64]) -> Vec3 {
    let total = weights.iter().sum::<f64>();
    points
        .iter()
        .zip(weights)
        .fold(Vec3::ZERO, |sum, (point, weight)| sum + *point * *weight)
        / total
}

fn valid_rigid_similarity(transform: SimilarityTransform) -> bool {
    if !valid_similarity(transform) {
        return false;
    }
    let rotation = transform.rotation.rows();
    for column in 0..3 {
        for other in 0..3 {
            let dot = (0..3)
                .map(|row| rotation[row][column] * rotation[row][other])
                .sum::<f64>();
            let expected = if column == other { 1.0 } else { 0.0 };
            if (dot - expected).abs() > 1.0e-6 {
                return false;
            }
        }
    }
    true
}

pub fn refine_assisted_similarity_icp(
    initial: SimilarityTransform,
    scan: &Mesh,
    template_vertices: &[Vec3],
    template_triangles: &[[u32; 3]],
    options: &AssistedFitOptions,
) -> (
    SimilarityTransform,
    Vec<AssistedIcpIterationReceipt>,
    Option<AssistedIcpSkipReason>,
) {
    if !valid_options(options) {
        return (
            initial,
            Vec::new(),
            Some(AssistedIcpSkipReason::InvalidOptions),
        );
    }
    if !valid_similarity(initial)
        || scan.vertices.is_empty()
        || scan.triangles.is_empty()
        || template_vertices.len() < 3
        || template_triangles.is_empty()
        || template_triangles.iter().flatten().any(|index| {
            usize::try_from(*index)
                .map(|index| index >= template_vertices.len())
                .unwrap_or(true)
        })
    {
        return (
            initial,
            Vec::new(),
            Some(AssistedIcpSkipReason::InvalidSurface),
        );
    }
    let target_ids = largest_component_triangle_ids(scan);
    let Ok(projector) =
        SurfaceProjector::new(&scan.vertices, &scan.triangles, Some(&target_ids), 0.0)
    else {
        return (
            initial,
            Vec::new(),
            Some(AssistedIcpSkipReason::ProjectionUnavailable),
        );
    };
    let template_normals = vertex_normals(template_vertices, template_triangles);
    let sample_indices = deterministic_samples(template_vertices.len(), options.icp_sample_count);
    if sample_indices.len() < 3 {
        return (
            initial,
            Vec::new(),
            Some(AssistedIcpSkipReason::InvalidSurface),
        );
    }

    let mut current = initial;
    let mut receipts = Vec::new();
    let mut skip_reason = None;
    for _ in 0..options.icp_iterations {
        let mut candidates = Vec::with_capacity(sample_indices.len());
        for &index in &sample_indices {
            let query = inverse_apply(current, template_vertices[index]);
            let Ok(projection) = projector.project(query.to_array()) else {
                continue;
            };
            let scan_point = Vec3::from(projection.point);
            let transformed = current.apply(scan_point);
            let distance = (transformed - template_vertices[index]).norm();
            let scan_normal = normalize_or_zero(
                current
                    .rotation
                    .transform_vector(Vec3::from(projection.normal)),
            );
            let template_normal = template_normals[index];
            candidates.push((
                scan_point,
                template_vertices[index],
                distance,
                scan_normal.dot(template_normal),
            ));
        }
        if candidates.len() < 3 {
            skip_reason = Some(AssistedIcpSkipReason::ProjectionUnavailable);
            break;
        }
        let dots = candidates
            .iter()
            .map(|candidate| candidate.3)
            .collect::<Vec<_>>();
        let flip = median(&dots).is_some_and(|value| value < 0.0);
        for candidate in &mut candidates {
            if flip {
                candidate.3 *= -1.0;
            }
        }
        let mut selected = candidates
            .into_iter()
            .filter(|candidate| {
                candidate.2 <= options.icp_max_distance_cm
                    && candidate.3 >= options.icp_min_normal_dot
            })
            .collect::<Vec<_>>();
        selected.sort_by(|left, right| left.2.total_cmp(&right.2));
        let keep = ((selected.len() as f64) * (1.0 - options.icp_trim_fraction))
            .ceil()
            .max(0.0) as usize;
        selected.truncate(keep);
        let coverage = selected.len() as f64 / sample_indices.len() as f64;
        if selected.len() < options.icp_minimum_correspondences
            || coverage < options.icp_minimum_coverage
        {
            skip_reason = Some(AssistedIcpSkipReason::InsufficientCorrespondences);
            break;
        }
        let source = selected
            .iter()
            .map(|candidate| current.apply(candidate.0))
            .collect::<Vec<_>>();
        let target = selected
            .iter()
            .map(|candidate| candidate.1)
            .collect::<Vec<_>>();
        let residuals = selected
            .iter()
            .map(|candidate| candidate.2)
            .collect::<Vec<_>>();
        let weights = residuals
            .iter()
            .map(|residual| huber_weight(*residual, options.icp_huber_delta_cm))
            .collect::<Vec<_>>();
        let rms_before = rms(&residuals).unwrap_or(f64::INFINITY);
        let Ok(delta) = estimate_similarity(&source, &target, Some(&weights)) else {
            skip_reason = Some(AssistedIcpSkipReason::NoImprovement);
            break;
        };
        let (delta, step_was_clamped) = clamp_similarity_delta(delta, options);
        let candidate = compose_similarity(delta, current);
        let after_residuals = selected
            .iter()
            .map(|correspondence| (candidate.apply(correspondence.0) - correspondence.1).norm())
            .collect::<Vec<_>>();
        let rms_after = rms(&after_residuals).unwrap_or(f64::INFINITY);
        let accepted = rms_after + options.icp_minimum_rms_improvement_cm < rms_before;
        receipts.push(AssistedIcpIterationReceipt {
            correspondences: selected.len(),
            coverage,
            rms_before_cm: rms_before,
            rms_after_cm: rms_after,
            accepted,
            step_was_clamped,
        });
        if !accepted {
            skip_reason = Some(AssistedIcpSkipReason::NoImprovement);
            break;
        }
        current = candidate;
    }
    (current, receipts, skip_reason)
}

#[must_use]
pub fn compose_similarity(
    after: SimilarityTransform,
    before: SimilarityTransform,
) -> SimilarityTransform {
    let after_rotation = matrix_to_na(after.rotation);
    let before_rotation = matrix_to_na(before.rotation);
    SimilarityTransform {
        scale: after.scale * before.scale,
        rotation: Mat3::from_na(after_rotation * before_rotation),
        translation: after.rotation.transform_vector(before.translation) * after.scale
            + after.translation,
    }
}

fn reject<T>(
    mut receipt: AssistedFitReceipt,
    reason: AssistedAlignmentRejection,
) -> Result<T, AssistedAlignmentError> {
    receipt.rejection_reason = Some(reason);
    Err(AssistedAlignmentError {
        receipt: Box::new(receipt),
    })
}

pub(super) fn valid_options(options: &AssistedFitOptions) -> bool {
    options.maximum_hypotheses > 0
        && options.maximum_hypotheses <= MAXIMUM_ASSISTED_HYPOTHESES
        && options.pin_inlier_distance_cm.is_finite()
        && options.pin_inlier_distance_cm > 0.0
        && options.pin_inlier_span_fraction.is_finite()
        && options.pin_inlier_span_fraction >= 0.0
        && options.minimum_inlier_fraction.is_finite()
        && (0.0..=1.0).contains(&options.minimum_inlier_fraction)
        && options.minimum_layout_spread.is_finite()
        && options.minimum_layout_spread > 0.0
        && options.minimum_layout_spread <= 1.0
        && options.irls_iterations > 0
        && options.irls_iterations <= MAXIMUM_ASSISTED_IRLS_ITERATIONS
        && options.mad_multiplier.is_finite()
        && options.mad_multiplier > 0.0
        && options.outlier_weight_floor.is_finite()
        && options.outlier_weight_floor > 0.0
        && options.outlier_weight_floor <= 1.0
        && options.icp_iterations > 0
        && options.icp_iterations <= MAXIMUM_ASSISTED_ICP_ITERATIONS
        && options.icp_sample_count >= 3
        && options.icp_max_distance_cm.is_finite()
        && options.icp_max_distance_cm > 0.0
        && options.icp_min_normal_dot.is_finite()
        && (-1.0..=1.0).contains(&options.icp_min_normal_dot)
        && options.icp_trim_fraction.is_finite()
        && (0.0..1.0).contains(&options.icp_trim_fraction)
        && options.icp_huber_delta_cm.is_finite()
        && options.icp_huber_delta_cm > 0.0
        && options.icp_minimum_correspondences >= 3
        && options.icp_minimum_coverage.is_finite()
        && (0.0..=1.0).contains(&options.icp_minimum_coverage)
        && options.icp_max_translation_step_cm.is_finite()
        && options.icp_max_translation_step_cm > 0.0
        && options.icp_max_rotation_step_radians.is_finite()
        && options.icp_max_rotation_step_radians > 0.0
        && options.icp_max_rotation_step_radians <= 15.0_f64.to_radians()
        && options.icp_max_scale_step.is_finite()
        && (0.0..=0.25).contains(&options.icp_max_scale_step)
        && options.icp_minimum_rms_improvement_cm.is_finite()
        && options.icp_minimum_rms_improvement_cm >= 0.0
}

pub(super) fn valid_similarity(transform: SimilarityTransform) -> bool {
    transform.scale.is_finite()
        && transform.scale > f64::EPSILON
        && transform.rotation.is_finite()
        && transform.translation.is_finite()
        && (transform.rotation.determinant() - 1.0).abs() <= 1.0e-6
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RankedTriple {
    score: f64,
    indices: [usize; 3],
    hash: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TripleSelectionStats {
    valid_count: usize,
    #[cfg(test)]
    candidate_count: usize,
    #[cfg(test)]
    peak_retained: usize,
}

#[derive(Debug)]
struct TripleSelection {
    triples: Vec<(f64, [usize; 3])>,
    #[cfg(test)]
    stats: TripleSelectionStats,
}

fn bounded_ranked_triples(
    source: &[Vec3],
    target: &[Vec3],
    source_span: f64,
    target_span: f64,
    maximum: usize,
) -> TripleSelection {
    let spread_count = maximum.div_ceil(2);
    let source_span_squared = source_span * source_span;
    let target_span_squared = target_span * target_span;
    let mut spread_winners = Vec::with_capacity(spread_count);
    let mut hash_prefix = Vec::with_capacity(maximum);
    let mut stats = TripleSelectionStats::default();

    for first in 0..source.len() - 2 {
        for second in first + 1..source.len() - 1 {
            for third in second + 1..source.len() {
                #[cfg(test)]
                {
                    stats.candidate_count += 1;
                }
                let source_area =
                    triangle_double_area(source[first], source[second], source[third])
                        / source_span_squared;
                let target_area =
                    triangle_double_area(target[first], target[second], target[third])
                        / target_span_squared;
                let score = source_area.min(target_area);
                if score > 1.0e-8 {
                    stats.valid_count += 1;
                    let indices = [first, second, third];
                    let candidate = RankedTriple {
                        score,
                        indices,
                        hash: triple_hash(indices),
                    };
                    insert_sorted_bounded(
                        &mut spread_winners,
                        candidate,
                        spread_count,
                        spread_order,
                    );
                    insert_sorted_bounded(&mut hash_prefix, candidate, maximum, hash_order);
                    #[cfg(test)]
                    {
                        stats.peak_retained = stats
                            .peak_retained
                            .max(spread_winners.len() + hash_prefix.len());
                    }
                }
            }
        }
    }

    let selected = if stats.valid_count <= maximum {
        hash_prefix.sort_by(spread_order);
        hash_prefix
    } else {
        let mut spread_indices = spread_winners
            .iter()
            .map(|candidate| candidate.indices)
            .collect::<Vec<_>>();
        spread_indices.sort_unstable();
        let diversity_count = maximum - spread_winners.len();
        spread_winners.extend(
            hash_prefix
                .into_iter()
                .filter(|candidate| spread_indices.binary_search(&candidate.indices).is_err())
                .take(diversity_count),
        );
        debug_assert_eq!(spread_winners.len(), maximum);
        spread_winners
    };

    TripleSelection {
        triples: selected
            .into_iter()
            .map(|candidate| (candidate.score, candidate.indices))
            .collect(),
        #[cfg(test)]
        stats,
    }
}

fn insert_sorted_bounded(
    selected: &mut Vec<RankedTriple>,
    candidate: RankedTriple,
    maximum: usize,
    order: fn(&RankedTriple, &RankedTriple) -> std::cmp::Ordering,
) {
    if maximum == 0 {
        return;
    }
    if selected.len() == maximum
        && order(
            &candidate,
            selected.last().expect("bounded set is non-empty"),
        ) != std::cmp::Ordering::Less
    {
        return;
    }
    let position = selected
        .binary_search_by(|current| order(current, &candidate))
        .unwrap_or_else(|position| position);
    selected.insert(position, candidate);
    if selected.len() > maximum {
        selected.pop();
    }
}

fn spread_order(left: &RankedTriple, right: &RankedTriple) -> std::cmp::Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then_with(|| left.indices.cmp(&right.indices))
}

fn hash_order(left: &RankedTriple, right: &RankedTriple) -> std::cmp::Ordering {
    left.hash
        .cmp(&right.hash)
        .then_with(|| left.indices.cmp(&right.indices))
}

fn triple_hash(indices: [usize; 3]) -> u64 {
    let mut value = 0x9e37_79b9_7f4a_7c15_u64;
    for index in indices {
        value ^= (index as u64)
            .wrapping_add(0x9e37_79b9_7f4a_7c15)
            .wrapping_add(value << 6)
            .wrapping_add(value >> 2);
        value ^= value >> 30;
        value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value ^= value >> 27;
        value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
    }
    value
}

fn hypothesis_is_better(candidate: HypothesisScore, current: HypothesisScore) -> bool {
    candidate.inlier_count > current.inlier_count
        || (candidate.inlier_count == current.inlier_count
            && (candidate.inlier_weight > current.inlier_weight
                || (candidate.inlier_weight == current.inlier_weight
                    && (candidate.median_residual < current.median_residual
                        || (candidate.median_residual == current.median_residual
                            && candidate.rms_residual < current.rms_residual)))))
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

fn layout_spread(points: &[Vec3], span: f64) -> f64 {
    if span <= f64::EPSILON {
        return 0.0;
    }
    let mut maximum = 0.0_f64;
    for first in 0..points.len() - 2 {
        for second in first + 1..points.len() - 1 {
            for third in second + 1..points.len() {
                maximum = maximum.max(
                    triangle_double_area(points[first], points[second], points[third])
                        / span.powi(2),
                );
            }
        }
    }
    maximum
}

fn triangle_double_area(first: Vec3, second: Vec3, third: Vec3) -> f64 {
    cross(second - first, third - first).norm()
}

fn cross(first: Vec3, second: Vec3) -> Vec3 {
    Vec3::new(
        first.y * second.z - first.z * second.y,
        first.z * second.x - first.x * second.z,
        first.x * second.y - first.y * second.x,
    )
}

fn similarity_residuals(
    transform: SimilarityTransform,
    source: &[Vec3],
    target: &[Vec3],
) -> Vec<f64> {
    source
        .iter()
        .copied()
        .zip(target.iter().copied())
        .map(|(source, target)| (transform.apply(source) - target).norm())
        .collect()
}

fn huber_weight(residual: f64, delta: f64) -> f64 {
    if residual <= delta {
        1.0
    } else {
        delta / residual.max(1.0e-12)
    }
}

fn deterministic_samples(vertex_count: usize, requested: usize) -> Vec<usize> {
    let count = vertex_count.min(requested);
    if count == vertex_count {
        return (0..vertex_count).collect();
    }
    if count <= 1 {
        return vec![0];
    }
    (0..count)
        .map(|sample| sample * (vertex_count - 1) / (count - 1))
        .collect()
}

fn inverse_apply(transform: SimilarityTransform, point: Vec3) -> Vec3 {
    transform
        .rotation
        .transpose()
        .transform_vector((point - transform.translation) / transform.scale)
}

pub(super) fn clamp_similarity_delta(
    delta: SimilarityTransform,
    options: &AssistedFitOptions,
) -> (SimilarityTransform, bool) {
    let scale = delta.scale.clamp(
        1.0 - options.icp_max_scale_step,
        1.0 + options.icp_max_scale_step,
    );
    let matrix = matrix_to_na(delta.rotation);
    let quaternion =
        UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(matrix));
    let scaled_axis = quaternion.scaled_axis();
    let angle = scaled_axis.norm();
    let clamped_axis = if angle > options.icp_max_rotation_step_radians {
        scaled_axis * (options.icp_max_rotation_step_radians / angle)
    } else {
        scaled_axis
    };
    let rotation = UnitQuaternion::from_scaled_axis(clamped_axis);
    let translation_norm = delta.translation.norm();
    let translation = if translation_norm > options.icp_max_translation_step_cm {
        delta.translation * (options.icp_max_translation_step_cm / translation_norm)
    } else {
        delta.translation
    };
    let clamped = scale != delta.scale
        || angle > options.icp_max_rotation_step_radians
        || translation_norm > options.icp_max_translation_step_cm;
    (
        SimilarityTransform {
            scale,
            rotation: Mat3::from_na(*rotation.to_rotation_matrix().matrix()),
            translation,
        },
        clamped,
    )
}

fn matrix_to_na(matrix: Mat3) -> Matrix3<f64> {
    let rows = matrix.rows();
    Matrix3::new(
        rows[0][0], rows[0][1], rows[0][2], rows[1][0], rows[1][1], rows[1][2], rows[2][0],
        rows[2][1], rows[2][2],
    )
}

fn normalize_or_zero(value: Vec3) -> Vec3 {
    let length = value.norm();
    if length > 1.0e-12 {
        value / length
    } else {
        Vec3::ZERO
    }
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        Some((sorted[middle - 1] + sorted[middle]) * 0.5)
    } else {
        Some(sorted[middle])
    }
}

fn rms(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| {
        (values.iter().map(|value| value * value).sum::<f64>() / values.len() as f64).sqrt()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection_points(count: usize) -> Vec<Vec3> {
        (0..count)
            .map(|index| {
                let parameter = index as f64 * 0.371;
                Vec3::new(
                    parameter.sin() * 4.0 + (parameter * 0.19).cos(),
                    (parameter * 0.73).cos() * 5.0 + index as f64 * 0.013,
                    (parameter * 1.17).sin() * 2.0 + (parameter * 0.11).cos(),
                )
            })
            .collect()
    }

    fn legacy_bounded_ranked_triples(
        source: &[Vec3],
        target: &[Vec3],
        source_span: f64,
        target_span: f64,
        maximum: usize,
    ) -> Vec<(f64, [usize; 3])> {
        let mut ranked = Vec::new();
        for first in 0..source.len() - 2 {
            for second in first + 1..source.len() - 1 {
                for third in second + 1..source.len() {
                    let source_area =
                        triangle_double_area(source[first], source[second], source[third])
                            / source_span.powi(2);
                    let target_area =
                        triangle_double_area(target[first], target[second], target[third])
                            / target_span.powi(2);
                    let score = source_area.min(target_area);
                    if score > 1.0e-8 {
                        ranked.push((score, [first, second, third]));
                    }
                }
            }
        }
        ranked.sort_by(|left, right| {
            right
                .0
                .total_cmp(&left.0)
                .then_with(|| left.1.cmp(&right.1))
        });
        if ranked.len() <= maximum {
            return ranked;
        }
        let spread_count = maximum.div_ceil(2);
        let mut selected = ranked[..spread_count].to_vec();
        let mut remainder = ranked[spread_count..].to_vec();
        remainder.sort_by(|left, right| {
            triple_hash(left.1)
                .cmp(&triple_hash(right.1))
                .then_with(|| left.1.cmp(&right.1))
        });
        selected.extend(remainder.into_iter().take(maximum - selected.len()));
        selected
    }

    #[test]
    fn streaming_selection_exactly_matches_legacy_small_input_order() {
        let source = selection_points(12);
        let target = SimilarityTransform {
            scale: 1.17,
            translation: Vec3::new(0.4, -0.7, 1.1),
            ..SimilarityTransform::IDENTITY
        }
        .apply_slice(&source);
        let source_span = maximum_pair_distance(&source);
        let target_span = maximum_pair_distance(&target);

        for maximum in [7, 19, 512] {
            let expected =
                legacy_bounded_ranked_triples(&source, &target, source_span, target_span, maximum);
            let actual =
                bounded_ranked_triples(&source, &target, source_span, target_span, maximum);
            assert_eq!(actual.triples, expected, "maximum={maximum}");
        }
    }

    #[test]
    fn streaming_selection_has_stable_fixed_hash_diversity_order() {
        let source = selection_points(24);
        let mut target = source.clone();
        for (index, point) in target.iter_mut().enumerate() {
            point.z += (index as f64 * 0.41).sin() * 0.07;
        }
        let source_span = maximum_pair_distance(&source);
        let target_span = maximum_pair_distance(&target);
        let first = bounded_ranked_triples(&source, &target, source_span, target_span, 32);
        let second = bounded_ranked_triples(&source, &target, source_span, target_span, 32);

        assert_eq!(first.triples, second.triples);
        let diversity = &first.triples[16..];
        assert!(diversity.windows(2).all(|pair| {
            triple_hash(pair[0].1) < triple_hash(pair[1].1)
                || (triple_hash(pair[0].1) == triple_hash(pair[1].1) && pair[0].1 < pair[1].1)
        }));
    }

    #[test]
    fn streaming_selection_retention_is_bounded_for_434_pairs() {
        let source = selection_points(434);
        let target = source.clone();
        let source_span = maximum_pair_distance(&source);
        let maximum = 512;
        let selected = bounded_ranked_triples(&source, &target, source_span, source_span, maximum);

        assert_eq!(selected.stats.candidate_count, 434_usize * 433 * 432 / 6);
        assert!(selected.stats.valid_count > maximum);
        assert_eq!(selected.triples.len(), maximum);
        assert!(
            selected.stats.peak_retained <= maximum + maximum.div_ceil(2),
            "retained {} candidates for a bound of {}",
            selected.stats.peak_retained,
            maximum + maximum.div_ceil(2)
        );
        assert!(selected.stats.peak_retained * std::mem::size_of::<RankedTriple>() < 64 * 1024);
    }

    #[test]
    fn bounded_streaming_hypotheses_preserve_robust_fit_accuracy() {
        let source = selection_points(48);
        let expected = SimilarityTransform {
            scale: 1.23,
            rotation: Mat3::from_rows([
                [0.980_066_577_841_241_6, 0.0, 0.198_669_330_795_061_22],
                [0.0, 1.0, 0.0],
                [-0.198_669_330_795_061_22, 0.0, 0.980_066_577_841_241_6],
            ]),
            translation: Vec3::new(1.4, -0.8, 0.35),
        };
        let mut target = expected.apply_slice(&source);
        for (index, point) in target.iter_mut().take(40).enumerate() {
            *point += Vec3::new(
                (index as f64 * 0.7).sin() * 0.004,
                (index as f64 * 0.3).cos() * 0.004,
                (index as f64 * 0.9).sin() * 0.004,
            );
        }
        for (index, point) in target.iter_mut().skip(40).enumerate() {
            *point = Vec3::new(35.0 + index as f64, -28.0, 20.0 - index as f64);
        }
        let options = AssistedFitOptions {
            maximum_hypotheses: 64,
            ..AssistedFitOptions::default()
        };
        let result = estimate_assisted_similarity(&source, &target, &[1.0; 48], &options).unwrap();

        assert_eq!(result.receipt.hypothesis_count, 64);
        assert_eq!(result.receipt.inlier_count, 40);
        let maximum_inlier_error = source[..40]
            .iter()
            .copied()
            .zip(target[..40].iter().copied())
            .map(|(source, target)| (result.transform.apply(source) - target).norm())
            .fold(0.0, f64::max);
        assert!(maximum_inlier_error < 0.02, "error={maximum_inlier_error}");
    }
}
