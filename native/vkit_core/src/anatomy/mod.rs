mod eyelid;
mod eyelid_look;
mod local;
mod propagation;
mod quality;
mod selection;
mod surface_bond;
mod transition;

use thiserror::Error;

use crate::math::{SimilarityError, SimilarityTransform, Vec3, estimate_similarity};

pub use eyelid::{
    EyeSide, EyelidCanonicalGuard, EyelidConformationPlan, EyelidConformationReceipt,
    EyelidSideReceipt, conform_eyelids_to_globes,
};
pub use eyelid_look::{
    EyelidLookRole, EyelidLookTarget, EyelidLookWeights, eyelid_look_role, eyelid_look_weights,
    gaze_pitch_for_lid_weight,
};
pub use local::{
    LocalSimilarityConfig, LocalTransformResult, estimate_local_similarity,
    identity_rotation_at_mapped_center,
};
pub use propagation::{
    AnatomyComponentCounts, AnatomyQualityChecks, AnatomyQualityGateReceipt,
    ComponentTransformReceipt, EyePairConstraintReceipt, HeadAnatomyComponents, HeadAnatomyReceipt,
    HeadAnatomyTransforms, MouthAssemblyReceipt, MouthComponentReceipts, MouthSpacingReceipts,
    NostrilPairConstraintReceipt, PropagatedHeadAnatomy, TotalAnatomyMovementReceipt,
    discover_head_anatomy, propagate_head_anatomy, propagate_head_anatomy_auto,
    propagate_head_anatomy_auto_with_orientation_margin,
    propagate_head_anatomy_auto_with_topology_guard,
};
pub use quality::{
    PairwisePreservation, SpacingPreservation, TransformPreservation, minimum_interset_distance,
    sampled_pairwise_preservation, spacing_preservation, transform_preservation,
};
pub use selection::{
    material_and_polygon_group_vertices, material_vertices, polygon_group_vertices,
    vertices_from_face_mask,
};
pub use surface_bond::{
    AttachmentRig, FollowMode, SurfaceBond, SurfaceBondError, connected_groups,
};
pub use transition::{
    SKIN_TRANSITION_MIN_ORIENTATION_COSINE, SkinTransitionComponentReceipt, SkinTransitionPass,
    SkinTransitionReceipt, TransitionBlend, TransitionRepairReceipt, flipped_triangle_count,
    reconcile_skin_neighborhood, repair_skin_transition_topology, skin_adjacency,
    triangle_area_ratios, triangle_orientation_cosines, triangulated_skin_faces,
};

#[derive(Debug, Error, PartialEq)]
pub enum AnatomyError {
    #[error("base and fitted vertex counts differ ({base} != {fitted})")]
    VertexCountMismatch { base: usize, fitted: usize },
    #[error("component or control set is empty")]
    EmptySelection,
    #[error("vertex index {index} is outside a mesh of {vertex_count} vertices")]
    IndexOutOfRange { index: usize, vertex_count: usize },
    #[error("scale bounds must be finite, positive, and ordered")]
    InvalidScaleBounds,
    #[error("face mask has {actual} entries; expected {expected}")]
    FaceMaskLengthMismatch { expected: usize, actual: usize },
    #[error("base and fitted skin must contain at least eight matching vertices")]
    TooFewSkinAnchors,
    #[error("local transform center must contain finite coordinates")]
    NonFiniteCenter,
    #[error("local transform configuration is invalid")]
    InvalidLocalConfiguration,
    #[error("G2 geometry is missing required anatomy component {component}")]
    MissingComponent { component: &'static str },
    #[error(
        "skin selection contains only {actual} unique vertices; at least {required} are required"
    )]
    TooFewSkinVertices { required: usize, actual: usize },
    #[error("vertex array {name} contains non-finite coordinates at index {index}")]
    NonFiniteVertex { name: &'static str, index: usize },
    #[error("skin adjacency has {actual} vertices; expected {expected}")]
    AdjacencyLengthMismatch { expected: usize, actual: usize },
    #[error("boolean mask {name} has {actual} entries; expected {expected}")]
    MaskLengthMismatch {
        name: &'static str,
        expected: usize,
        actual: usize,
    },
    #[error(
        "triangle {triangle} references vertex {index}, but only {vertex_count} vertices exist"
    )]
    TriangleIndexOutOfRange {
        triangle: usize,
        index: usize,
        vertex_count: usize,
    },
    #[error("skin face {face} has {corners} corners; only triangles and quads are supported")]
    UnsupportedSkinPolygon { face: usize, corners: usize },
    #[error("transition repair configuration is invalid")]
    InvalidTransitionConfiguration,
    #[error(
        "internal anatomy preservation quality gate failed: {failed:?}; transition rings {transition_rings}; minimum skin orientation cosine {minimum_skin_orientation_cosine}, required {required_minimum_skin_orientation_cosine}, remaining triangles below margin {remaining_skin_triangles_below_margin}; skin area ratio {minimum_skin_area_ratio}..{maximum_skin_area_ratio}, required {required_minimum_skin_area_ratio}..{required_maximum_skin_area_ratio}, remaining triangles outside area range {remaining_skin_triangles_outside_area_ratio}, remaining triangles outside the complete topology guard {remaining_skin_triangles_outside_topology_guard}"
    )]
    QualityGateFailed {
        failed: Vec<&'static str>,
        transition_rings: usize,
        minimum_skin_orientation_cosine: f64,
        required_minimum_skin_orientation_cosine: f64,
        remaining_skin_triangles_below_margin: usize,
        minimum_skin_area_ratio: f64,
        maximum_skin_area_ratio: f64,
        required_minimum_skin_area_ratio: f64,
        required_maximum_skin_area_ratio: f64,
        remaining_skin_triangles_outside_area_ratio: usize,
        remaining_skin_triangles_outside_topology_guard: usize,
    },
    #[error(transparent)]
    Geometry(#[from] crate::spatial::GeometryKernelError),
    #[error("weighted rotation averaging failed")]
    RotationAverageFailed,
    #[error(transparent)]
    Similarity(#[from] SimilarityError),
}

pub(super) fn validate_indices(indices: &[usize], vertex_count: usize) -> Result<(), AnatomyError> {
    if indices.is_empty() {
        return Err(AnatomyError::EmptySelection);
    }
    if let Some(&index) = indices.iter().find(|&&index| index >= vertex_count) {
        return Err(AnatomyError::IndexOutOfRange {
            index,
            vertex_count,
        });
    }
    Ok(())
}

pub fn mean_control_displacement(
    canonical: &[Vec3],
    fitted_skin: &[Vec3],
    controls: &[usize],
) -> Result<Vec3, AnatomyError> {
    if canonical.len() != fitted_skin.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: canonical.len(),
            fitted: fitted_skin.len(),
        });
    }
    validate_indices(controls, canonical.len())?;
    let mut displacement = Vec3::ZERO;
    for &index in controls {
        displacement += fitted_skin[index] - canonical[index];
    }
    Ok(displacement / controls.len() as f64)
}

pub fn apply_rigid_translation(
    canonical: &[Vec3],
    output: &mut [Vec3],
    component: &[usize],
    translation: Vec3,
) -> Result<(), AnatomyError> {
    if canonical.len() != output.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: canonical.len(),
            fitted: output.len(),
        });
    }
    validate_indices(component, canonical.len())?;
    for &index in component {
        output[index] = canonical[index] + translation;
    }
    Ok(())
}

pub fn bounded_control_similarity(
    canonical: &[Vec3],
    fitted_skin: &[Vec3],
    controls: &[usize],
    minimum_scale: f64,
    maximum_scale: f64,
) -> Result<SimilarityTransform, AnatomyError> {
    if canonical.len() != fitted_skin.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: canonical.len(),
            fitted: fitted_skin.len(),
        });
    }
    if !minimum_scale.is_finite()
        || !maximum_scale.is_finite()
        || minimum_scale <= 0.0
        || minimum_scale > maximum_scale
    {
        return Err(AnatomyError::InvalidScaleBounds);
    }
    validate_indices(controls, canonical.len())?;
    let source: Vec<_> = controls.iter().map(|&index| canonical[index]).collect();
    let target: Vec<_> = controls.iter().map(|&index| fitted_skin[index]).collect();
    let mut transform = estimate_similarity(&source, &target, None)?;
    transform.scale = transform.scale.clamp(minimum_scale, maximum_scale);

    let source_center = source
        .iter()
        .copied()
        .fold(Vec3::ZERO, |sum, point| sum + point)
        / source.len() as f64;
    let target_center = target
        .iter()
        .copied()
        .fold(Vec3::ZERO, |sum, point| sum + point)
        / target.len() as f64;
    transform.translation =
        target_center - transform.rotation.transform_vector(source_center) * transform.scale;
    Ok(transform)
}

pub fn apply_component_similarity(
    canonical: &[Vec3],
    output: &mut [Vec3],
    component: &[usize],
    transform: SimilarityTransform,
) -> Result<(), AnatomyError> {
    if canonical.len() != output.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: canonical.len(),
            fitted: output.len(),
        });
    }
    validate_indices(component, canonical.len())?;
    for &index in component {
        output[index] = transform.apply(canonical[index]);
    }
    Ok(())
}

pub fn maximum_pairwise_scale_error(
    canonical: &[Vec3],
    fitted: &[Vec3],
    component: &[usize],
    expected_scale: f64,
) -> Result<f64, AnatomyError> {
    if canonical.len() != fitted.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: canonical.len(),
            fitted: fitted.len(),
        });
    }
    validate_indices(component, canonical.len())?;
    let mut maximum: f64 = 0.0;
    for (offset, &first) in component.iter().enumerate() {
        for &second in &component[offset + 1..] {
            let base_distance = (canonical[first] - canonical[second]).norm();
            if base_distance <= f64::EPSILON {
                continue;
            }
            let fitted_distance = (fitted[first] - fitted[second]).norm();
            maximum = maximum.max((fitted_distance / base_distance - expected_scale).abs());
        }
    }
    Ok(maximum)
}

#[cfg(test)]
mod tests {
    use approx::assert_abs_diff_eq;

    use super::*;
    use crate::math::Mat3;

    fn fixture() -> Vec<Vec3> {
        vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
        ]
    }

    #[test]
    fn eye_component_is_translation_only() {
        let canonical = fixture();
        let delta = Vec3::new(2.5, -1.0, 0.75);
        let fitted_skin: Vec<_> = canonical
            .iter()
            .copied()
            .map(|point| point + delta)
            .collect();
        let measured = mean_control_displacement(&canonical, &fitted_skin, &[0, 1, 2]).unwrap();
        let mut output = fitted_skin.clone();
        apply_rigid_translation(&canonical, &mut output, &[3, 4], measured).unwrap();
        assert_eq!(output[3], canonical[3] + delta);
        assert_eq!(output[4], canonical[4] + delta);
        assert_abs_diff_eq!(
            (output[4] - output[3]).norm(),
            (canonical[4] - canonical[3]).norm(),
            epsilon = 1e-14
        );
    }

    #[test]
    fn mouth_uses_one_bounded_similarity() {
        let canonical = fixture();
        let requested = SimilarityTransform {
            scale: 1.2,
            rotation: Mat3::from_rows([[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]),
            translation: Vec3::new(3.0, 4.0, 5.0),
        };
        let fitted_skin = requested.apply_slice(&canonical);
        let bounded =
            bounded_control_similarity(&canonical, &fitted_skin, &[0, 1, 2, 3], 0.96, 1.04)
                .unwrap();
        assert_abs_diff_eq!(bounded.scale, 1.04, epsilon = 1e-12);

        let mut output = canonical.clone();
        apply_component_similarity(&canonical, &mut output, &[0, 1, 2, 3, 4], bounded).unwrap();
        let error =
            maximum_pairwise_scale_error(&canonical, &output, &[0, 1, 2, 3, 4], 1.04).unwrap();
        assert!(error < 1e-12, "pairwise scale error {error}");
    }
}
