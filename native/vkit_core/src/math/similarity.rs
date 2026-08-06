use nalgebra::Matrix3;
use thiserror::Error;

use super::{Mat3, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimilarityTransform {
    pub scale: f64,
    pub rotation: Mat3,
    pub translation: Vec3,
}

impl SimilarityTransform {
    pub const IDENTITY: Self = Self {
        scale: 1.0,
        rotation: Mat3::IDENTITY,
        translation: Vec3::ZERO,
    };

    #[must_use]
    pub fn apply(self, point: Vec3) -> Vec3 {
        self.rotation.transform_vector(point) * self.scale + self.translation
    }

    pub fn apply_slice(self, points: &[Vec3]) -> Vec<Vec3> {
        points
            .iter()
            .copied()
            .map(|point| self.apply(point))
            .collect()
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SimilarityError {
    #[error("source and target point counts differ ({source_count} != {target_count})")]
    PointCountMismatch {
        source_count: usize,
        target_count: usize,
    },
    #[error("at least three point pairs are required, got {0}")]
    TooFewPoints(usize),
    #[error("weight count {actual} does not match point count {expected}")]
    WeightCountMismatch { expected: usize, actual: usize },
    #[error("source point {0} is not finite")]
    NonFiniteSource(usize),
    #[error("target point {0} is not finite")]
    NonFiniteTarget(usize),
    #[error("weight {0} is negative or non-finite")]
    InvalidWeight(usize),
    #[error("at least one positive weight is required")]
    NoPositiveWeight,
    #[error("source points have negligible weighted variance")]
    DegenerateSource,
    #[error("3x3 singular-value decomposition did not return both factors")]
    DecompositionFailed,
}

pub fn estimate_similarity(
    source: &[Vec3],
    target: &[Vec3],
    weights: Option<&[f64]>,
) -> Result<SimilarityTransform, SimilarityError> {
    if source.len() != target.len() {
        return Err(SimilarityError::PointCountMismatch {
            source_count: source.len(),
            target_count: target.len(),
        });
    }
    if source.len() < 3 {
        return Err(SimilarityError::TooFewPoints(source.len()));
    }
    if let Some(weights) = weights
        && weights.len() != source.len()
    {
        return Err(SimilarityError::WeightCountMismatch {
            expected: source.len(),
            actual: weights.len(),
        });
    }
    for (index, point) in source.iter().copied().enumerate() {
        if !point.is_finite() {
            return Err(SimilarityError::NonFiniteSource(index));
        }
    }
    for (index, point) in target.iter().copied().enumerate() {
        if !point.is_finite() {
            return Err(SimilarityError::NonFiniteTarget(index));
        }
    }

    let mut normalized_weights = Vec::with_capacity(source.len());
    let mut total_weight = 0.0;
    for index in 0..source.len() {
        let weight = weights.map_or(1.0, |values| values[index]);
        if !weight.is_finite() || weight < 0.0 {
            return Err(SimilarityError::InvalidWeight(index));
        }
        normalized_weights.push(weight);
        total_weight += weight;
    }
    if total_weight <= 0.0 || !total_weight.is_finite() {
        return Err(SimilarityError::NoPositiveWeight);
    }
    for weight in &mut normalized_weights {
        *weight /= total_weight;
    }

    let mut source_mean = Vec3::ZERO;
    let mut target_mean = Vec3::ZERO;
    for ((source_point, target_point), weight) in source
        .iter()
        .copied()
        .zip(target.iter().copied())
        .zip(normalized_weights.iter().copied())
    {
        source_mean += source_point * weight;
        target_mean += target_point * weight;
    }

    let mut covariance = Matrix3::<f64>::zeros();
    let mut source_variance = 0.0;
    for ((source_point, target_point), weight) in source
        .iter()
        .copied()
        .zip(target.iter().copied())
        .zip(normalized_weights.iter().copied())
    {
        let source_centered = (source_point - source_mean).to_na();
        let target_centered = (target_point - target_mean).to_na();
        covariance += weight * target_centered * source_centered.transpose();
        source_variance += weight * source_centered.norm_squared();
    }
    if source_variance <= f64::EPSILON || !source_variance.is_finite() {
        return Err(SimilarityError::DegenerateSource);
    }

    let decomposition = covariance.svd(true, true);
    let u = decomposition
        .u
        .ok_or(SimilarityError::DecompositionFailed)?;
    let v_t = decomposition
        .v_t
        .ok_or(SimilarityError::DecompositionFailed)?;
    let mut correction = Matrix3::<f64>::identity();
    if (u * v_t).determinant() < 0.0 {
        correction[(2, 2)] = -1.0;
    }
    let rotation_na = u * correction * v_t;
    let signed_singular_sum = decomposition.singular_values[0]
        + decomposition.singular_values[1]
        + correction[(2, 2)] * decomposition.singular_values[2];
    let scale = signed_singular_sum / source_variance;
    let translation_na = target_mean.to_na() - scale * rotation_na * source_mean.to_na();

    Ok(SimilarityTransform {
        scale,
        rotation: Mat3::from_na(rotation_na),
        translation: Vec3::from_na(translation_na),
    })
}
