use crate::math::{Mat3, SimilarityTransform, Vec3, estimate_similarity};

use super::AnatomyError;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalSimilarityConfig {
    pub anchor_count: usize,
    pub gaussian_sigma: f64,
    pub minimum_scale: f64,
    pub maximum_scale: f64,
    pub robust_iterations: usize,
}

impl Default for LocalSimilarityConfig {
    fn default() -> Self {
        Self {
            anchor_count: 256,
            gaussian_sigma: 2.5,
            minimum_scale: 0.90,
            maximum_scale: 1.10,
            robust_iterations: 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocalTransformResult {
    pub transform: SimilarityTransform,
    pub anchor_indices: Vec<usize>,
    pub anchor_rms: f64,
}

fn validate_config(config: LocalSimilarityConfig) -> Result<(), AnatomyError> {
    if config.anchor_count < 3
        || !config.gaussian_sigma.is_finite()
        || config.gaussian_sigma <= 0.0
        || !config.minimum_scale.is_finite()
        || !config.maximum_scale.is_finite()
        || config.minimum_scale <= 0.0
        || config.minimum_scale > config.maximum_scale
        || config.robust_iterations > 64
    {
        return Err(AnatomyError::InvalidLocalConfiguration);
    }
    Ok(())
}

fn weighted_mean(points: &[Vec3], weights: &[f64]) -> Vec3 {
    let total: f64 = weights.iter().sum();
    points
        .iter()
        .copied()
        .zip(weights.iter().copied())
        .fold(Vec3::ZERO, |sum, (point, weight)| sum + point * weight)
        / total
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) * 0.5
    } else {
        values[middle]
    }
}

pub fn estimate_local_similarity(
    base_skin: &[Vec3],
    fitted_skin: &[Vec3],
    center: Vec3,
    config: LocalSimilarityConfig,
) -> Result<LocalTransformResult, AnatomyError> {
    if base_skin.len() != fitted_skin.len() || base_skin.len() < 8 {
        return Err(AnatomyError::TooFewSkinAnchors);
    }
    if !center.is_finite() {
        return Err(AnatomyError::NonFiniteCenter);
    }
    validate_config(config)?;

    let count = config.anchor_count.min(base_skin.len());
    let mut ranked: Vec<_> = base_skin
        .iter()
        .copied()
        .enumerate()
        .map(|(index, point)| (index, (point - center).norm()))
        .collect();
    if ranked.iter().any(|(_, distance)| !distance.is_finite())
        || fitted_skin.iter().any(|point| !point.is_finite())
    {
        return Err(AnatomyError::NonFiniteCenter);
    }
    ranked.sort_by(|(left_index, left), (right_index, right)| {
        left.total_cmp(right).then(left_index.cmp(right_index))
    });
    ranked.truncate(count);

    let anchor_indices: Vec<_> = ranked.iter().map(|(index, _)| *index).collect();
    let source: Vec<_> = anchor_indices
        .iter()
        .map(|&index| base_skin[index])
        .collect();
    let target: Vec<_> = anchor_indices
        .iter()
        .map(|&index| fitted_skin[index])
        .collect();
    let base_weights: Vec<_> = ranked
        .iter()
        .map(|(_, distance)| {
            let normalized = distance / config.gaussian_sigma;
            (-0.5 * normalized * normalized).exp()
        })
        .collect();
    let mut weights = base_weights.clone();
    let mut transform = estimate_similarity(&source, &target, Some(&weights))?;

    for _ in 0..config.robust_iterations {
        let residuals: Vec<_> = source
            .iter()
            .copied()
            .zip(target.iter().copied())
            .map(|(source, target)| (transform.apply(source) - target).norm())
            .collect();
        let mut sorted_residuals = residuals.clone();
        let residual_median = median(&mut sorted_residuals);
        let huber_delta = (residual_median * 2.5).max(0.02);
        for ((weight, &base_weight), residual) in
            weights.iter_mut().zip(&base_weights).zip(residuals)
        {
            let robust = if residual > huber_delta {
                huber_delta / residual.max(1.0e-12)
            } else {
                1.0
            };
            *weight = base_weight * robust;
        }
        transform = estimate_similarity(&source, &target, Some(&weights))?;
    }

    let clamped_scale = transform
        .scale
        .clamp(config.minimum_scale, config.maximum_scale);
    if clamped_scale != transform.scale {
        let source_center = weighted_mean(&source, &weights);
        let target_center = weighted_mean(&target, &weights);
        transform.scale = clamped_scale;
        transform.translation =
            target_center - transform.rotation.transform_vector(source_center) * clamped_scale;
    }

    let weighted_squared_error: f64 = source
        .iter()
        .copied()
        .zip(target.iter().copied())
        .zip(weights.iter().copied())
        .map(|((source, target), weight)| {
            let residual = transform.apply(source) - target;
            weight * residual.norm_squared()
        })
        .sum();
    let anchor_rms = (weighted_squared_error / weights.iter().sum::<f64>()).sqrt();

    Ok(LocalTransformResult {
        transform,
        anchor_indices,
        anchor_rms,
    })
}

pub fn identity_rotation_at_mapped_center(
    candidate: SimilarityTransform,
    component_center: Vec3,
) -> SimilarityTransform {
    let mapped_center = candidate.apply(component_center);
    SimilarityTransform {
        scale: 1.0,
        rotation: Mat3::IDENTITY,
        translation: mapped_center - component_center,
    }
}

#[cfg(test)]
mod tests {
    use approx::assert_abs_diff_eq;

    use super::*;

    fn grid() -> Vec<Vec3> {
        (-2..=2)
            .flat_map(|x| {
                (-2..=2).map(move |y| Vec3::new(f64::from(x), f64::from(y), f64::from(x * y) * 0.1))
            })
            .collect()
    }

    #[test]
    fn robust_local_fit_rejects_one_far_residual_and_clamps_scale() {
        let source = grid();
        let requested = SimilarityTransform {
            scale: 1.2,
            rotation: Mat3::IDENTITY,
            translation: Vec3::new(2.0, -1.0, 0.5),
        };
        let mut target = requested.apply_slice(&source);
        target[0] += Vec3::new(50.0, -40.0, 30.0);
        let result = estimate_local_similarity(
            &source,
            &target,
            Vec3::ZERO,
            LocalSimilarityConfig {
                anchor_count: source.len(),
                minimum_scale: 0.95,
                maximum_scale: 1.04,
                ..Default::default()
            },
        )
        .unwrap();
        assert_abs_diff_eq!(result.transform.scale, 1.04, epsilon = 1.0e-12);
        assert!((result.transform.translation - requested.translation).norm() < 0.2);
        assert_eq!(result.anchor_indices.len(), source.len());
    }

    #[test]
    fn eye_rule_preserves_gaze_and_exact_globe_distances() {
        let candidate = SimilarityTransform {
            scale: 1.3,
            rotation: Mat3::from_rows([[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]),
            translation: Vec3::new(3.0, 4.0, 5.0),
        };
        let center = Vec3::new(1.0, 2.0, 3.0);
        let rigid = identity_rotation_at_mapped_center(candidate, center);
        assert_eq!(rigid.scale, 1.0);
        assert_eq!(rigid.rotation, Mat3::IDENTITY);
        assert_abs_diff_eq!(
            (rigid.apply(center) - candidate.apply(center)).norm(),
            0.0,
            epsilon = 1.0e-14
        );
        let first = center + Vec3::new(1.0, 0.0, 0.0);
        let second = center + Vec3::new(0.0, 1.0, 0.0);
        assert_abs_diff_eq!(
            (rigid.apply(first) - rigid.apply(second)).norm(),
            (first - second).norm(),
            epsilon = 1.0e-14
        );
    }

    #[test]
    fn custom_lower_shell_moves_internal_mouth_through_a_bounded_similarity() {
        let canonical_shell = grid();
        let fitted_shell = canonical_shell
            .iter()
            .map(|point| {
                let lower_face = (1.0 - (point.y + 1.0).abs() / 2.0).max(0.0);
                Vec3::new(
                    point.x * (1.0 + 0.35 * lower_face),
                    point.y - 0.12 * lower_face,
                    point.z + 0.90 * lower_face,
                )
            })
            .collect::<Vec<_>>();
        let mouth_center = Vec3::new(0.0, -1.0, 0.0);
        let local = estimate_local_similarity(
            &canonical_shell,
            &fitted_shell,
            mouth_center,
            LocalSimilarityConfig {
                anchor_count: canonical_shell.len(),
                gaussian_sigma: 1.5,
                minimum_scale: 0.85,
                maximum_scale: 1.18,
                robust_iterations: 5,
            },
        )
        .unwrap();
        let canonical_internal = [
            mouth_center + Vec3::new(-0.4, 0.0, 0.0),
            mouth_center + Vec3::new(0.4, 0.0, 0.0),
            mouth_center + Vec3::new(0.0, 0.2, 0.25),
            mouth_center + Vec3::new(0.0, -0.2, -0.25),
        ];
        let fitted_internal = canonical_internal.map(|point| local.transform.apply(point));

        assert!((0.85..=1.18).contains(&local.transform.scale));
        assert!(local.transform.apply(mouth_center).z > mouth_center.z + 0.20);
        assert!(local.anchor_rms.is_finite());
        assert!(fitted_internal.iter().all(|point| point.is_finite()));
        for first in 0..canonical_internal.len() {
            for second in first + 1..canonical_internal.len() {
                let canonical_distance =
                    (canonical_internal[first] - canonical_internal[second]).norm();
                let fitted_distance = (fitted_internal[first] - fitted_internal[second]).norm();
                let ratio = fitted_distance / canonical_distance;
                assert_abs_diff_eq!(ratio, local.transform.scale, epsilon = 1.0e-12);
                assert!(ratio <= 1.18 + 1.0e-12);
            }
        }
    }
}
