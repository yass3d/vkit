use crate::math::Vec3;

use super::{AnatomyError, validate_indices};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PairwisePreservation {
    pub sample_vertex_count: usize,
    pub sample_pair_count: usize,
    pub rms_error: f64,
    pub maximum_error: f64,
    pub maximum_relative_error: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpacingPreservation {
    pub base_minimum: f64,
    pub fitted_minimum: f64,
    pub expected_minimum: f64,
    pub absolute_error: f64,
    pub preserved_scale_ratio: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransformPreservation {
    pub vertex_count: usize,
    pub residual_rms: f64,
    pub residual_maximum: f64,
    pub pairwise: PairwisePreservation,
}

fn sampled_indices(indices: &[usize], sample_limit: usize) -> Vec<usize> {
    if sample_limit == 0 || indices.len() <= sample_limit {
        return indices.to_vec();
    }
    if sample_limit == 1 {
        return vec![indices[0]];
    }
    (0..sample_limit)
        .map(|sample| {
            let position = sample * (indices.len() - 1) / (sample_limit - 1);
            indices[position]
        })
        .collect()
}

pub fn sampled_pairwise_preservation(
    canonical: &[Vec3],
    fitted: &[Vec3],
    component: &[usize],
    expected_scale: f64,
    sample_limit: usize,
) -> Result<PairwisePreservation, AnatomyError> {
    if canonical.len() != fitted.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: canonical.len(),
            fitted: fitted.len(),
        });
    }
    if !expected_scale.is_finite() || expected_scale <= 0.0 {
        return Err(AnatomyError::InvalidScaleBounds);
    }
    validate_indices(component, canonical.len())?;
    let sample = sampled_indices(component, sample_limit);
    let mut squared_error = 0.0;
    let mut maximum_error: f64 = 0.0;
    let mut maximum_relative_error: f64 = 0.0;
    let mut pair_count = 0usize;
    for (offset, &first) in sample.iter().enumerate() {
        for &second in &sample[offset + 1..] {
            let expected = (canonical[first] - canonical[second]).norm() * expected_scale;
            if expected <= f64::EPSILON {
                continue;
            }
            let actual = (fitted[first] - fitted[second]).norm();
            let error = (actual - expected).abs();
            squared_error += error * error;
            maximum_error = maximum_error.max(error);
            maximum_relative_error = maximum_relative_error.max(error / expected);
            pair_count += 1;
        }
    }
    Ok(PairwisePreservation {
        sample_vertex_count: sample.len(),
        sample_pair_count: pair_count,
        rms_error: if pair_count == 0 {
            0.0
        } else {
            (squared_error / pair_count as f64).sqrt()
        },
        maximum_error,
        maximum_relative_error,
    })
}

pub fn minimum_interset_distance(
    vertices: &[Vec3],
    first: &[usize],
    second: &[usize],
) -> Result<f64, AnatomyError> {
    validate_indices(first, vertices.len())?;
    validate_indices(second, vertices.len())?;
    let mut minimum = f64::INFINITY;
    for &left in first {
        for &right in second {
            minimum = minimum.min((vertices[left] - vertices[right]).norm());
        }
    }
    Ok(minimum)
}

pub fn spacing_preservation(
    canonical: &[Vec3],
    fitted: &[Vec3],
    first: &[usize],
    second: &[usize],
    expected_scale: f64,
) -> Result<SpacingPreservation, AnatomyError> {
    if canonical.len() != fitted.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: canonical.len(),
            fitted: fitted.len(),
        });
    }
    if !expected_scale.is_finite() || expected_scale <= 0.0 {
        return Err(AnatomyError::InvalidScaleBounds);
    }
    let base_minimum = minimum_interset_distance(canonical, first, second)?;
    let fitted_minimum = minimum_interset_distance(fitted, first, second)?;
    let expected_minimum = expected_scale * base_minimum;
    let absolute_error = (fitted_minimum - expected_minimum).abs();
    Ok(SpacingPreservation {
        base_minimum,
        fitted_minimum,
        expected_minimum,
        absolute_error,
        preserved_scale_ratio: fitted_minimum / expected_minimum.max(1.0e-12),
    })
}

pub fn transform_preservation(
    canonical: &[Vec3],
    fitted: &[Vec3],
    component: &[usize],
    transform: crate::math::SimilarityTransform,
    sample_limit: usize,
) -> Result<TransformPreservation, AnatomyError> {
    if canonical.len() != fitted.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: canonical.len(),
            fitted: fitted.len(),
        });
    }
    if component.is_empty() {
        return Ok(TransformPreservation {
            vertex_count: 0,
            residual_rms: 0.0,
            residual_maximum: 0.0,
            pairwise: PairwisePreservation {
                sample_vertex_count: 0,
                sample_pair_count: 0,
                rms_error: 0.0,
                maximum_error: 0.0,
                maximum_relative_error: 0.0,
            },
        });
    }
    validate_indices(component, canonical.len())?;
    let mut squared_residual = 0.0;
    let mut residual_maximum: f64 = 0.0;
    for &index in component {
        let residual = (fitted[index] - transform.apply(canonical[index])).norm();
        squared_residual += residual * residual;
        residual_maximum = residual_maximum.max(residual);
    }
    Ok(TransformPreservation {
        vertex_count: component.len(),
        residual_rms: (squared_residual / component.len() as f64).sqrt(),
        residual_maximum,
        pairwise: sampled_pairwise_preservation(
            canonical,
            fitted,
            component,
            transform.scale,
            sample_limit,
        )?,
    })
}

#[cfg(test)]
mod tests {
    use approx::assert_abs_diff_eq;

    use super::*;

    #[test]
    fn shared_similarity_preserves_component_shape_and_spacing() {
        let canonical = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
            Vec3::new(1.0, 2.0, 0.0),
        ];
        let fitted: Vec<_> = canonical
            .iter()
            .copied()
            .map(|point| point * 1.04 + Vec3::new(3.0, -2.0, 1.0))
            .collect();
        let pairwise =
            sampled_pairwise_preservation(&canonical, &fitted, &[0, 1, 2, 3], 1.04, 160).unwrap();
        assert!(pairwise.maximum_error < 1.0e-12);
        let spacing = spacing_preservation(&canonical, &fitted, &[0, 1], &[2, 3], 1.04).unwrap();
        assert_abs_diff_eq!(spacing.absolute_error, 0.0, epsilon = 1.0e-12);
        assert_abs_diff_eq!(spacing.preserved_scale_ratio, 1.0, epsilon = 1.0e-12);
    }
}
