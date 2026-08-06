use std::collections::{BTreeMap, BTreeSet, VecDeque};

use nalgebra::Matrix3;

use crate::formats::{DazGeometry, HEAD_SKIN_MATERIALS};
use crate::math::{Mat3, SimilarityTransform, Vec3};
use crate::spatial::deformation_safety_mask;

use super::{
    AnatomyError, LocalSimilarityConfig, LocalTransformResult,
    SKIN_TRANSITION_MIN_ORIENTATION_COSINE, SkinTransitionPass, SkinTransitionReceipt,
    SpacingPreservation, TransformPreservation, estimate_local_similarity,
    material_and_polygon_group_vertices, material_vertices, polygon_group_vertices,
    reconcile_skin_neighborhood, repair_skin_transition_topology, skin_adjacency,
    spacing_preservation, transform_preservation, triangle_area_ratios, triangulated_skin_faces,
};

const EYE_ATTACHED_MATERIALS: &[&str] = &["Lacrimals", "Tear", "Eyelashes"];
const DISTANCE_TOLERANCE: f64 = 1.0e-7;
const ORIENTATION_TOLERANCE_DEGREES: f64 = 1.0e-7;
const EYE_MINIMUM_SCALE: f64 = 0.90;
const EYE_MAXIMUM_SCALE: f64 = 1.12;
const MOUTH_MINIMUM_SCALE: f64 = 0.85;
const MOUTH_MAXIMUM_SCALE: f64 = 1.18;
const SCALE_SNAP_TO_IDENTITY: f64 = 0.005;

const TRANSITION_RING_ATTEMPTS: [usize; 3] = [4, 8, 12];
const TRANSITION_MAX_ITERATIONS: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadAnatomyComponents {
    pub skin: Vec<usize>,
    pub left_eye: Vec<usize>,
    pub right_eye: Vec<usize>,
    pub left_eye_attached: Vec<usize>,
    pub right_eye_attached: Vec<usize>,
    pub upper_jaw: Vec<usize>,
    pub lower_jaw: Vec<usize>,
    pub tongue: Vec<usize>,
    pub inner_mouth: Vec<usize>,
    pub mouth_assembly: Vec<usize>,
    pub upper_teeth: Vec<usize>,
    pub lower_teeth: Vec<usize>,
    pub upper_gums: Vec<usize>,
    pub lower_gums: Vec<usize>,
    pub left_nostril: Vec<usize>,
    pub right_nostril: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnatomyComponentCounts {
    pub skin: usize,
    pub left_eye: usize,
    pub right_eye: usize,
    pub left_eye_attached: usize,
    pub right_eye_attached: usize,
    pub upper_jaw: usize,
    pub lower_jaw: usize,
    pub tongue: usize,
    pub inner_mouth: usize,
    pub mouth_assembly: usize,
    pub left_nostril: usize,
    pub right_nostril: usize,
}

impl HeadAnatomyComponents {
    #[must_use]
    pub fn counts(&self) -> AnatomyComponentCounts {
        AnatomyComponentCounts {
            skin: self.skin.len(),
            left_eye: self.left_eye.len(),
            right_eye: self.right_eye.len(),
            left_eye_attached: self.left_eye_attached.len(),
            right_eye_attached: self.right_eye_attached.len(),
            upper_jaw: self.upper_jaw.len(),
            lower_jaw: self.lower_jaw.len(),
            tongue: self.tongue.len(),
            inner_mouth: self.inner_mouth.len(),
            mouth_assembly: self.mouth_assembly.len(),
            left_nostril: self.left_nostril.len(),
            right_nostril: self.right_nostril.len(),
        }
    }

    #[must_use]
    pub fn protected_vertices(&self) -> Vec<usize> {
        union_sorted(&[
            &self.left_eye,
            &self.right_eye,
            &self.left_eye_attached,
            &self.right_eye_attached,
            &self.mouth_assembly,
            &self.left_nostril,
            &self.right_nostril,
        ])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentTransformReceipt {
    pub requested_vertex_count: usize,
    pub applied_vertex_count: usize,
    pub overwritten_skin_vertex_count: usize,
    pub anchor_count: usize,
    pub anchor_rms: f64,
    pub transform: SimilarityTransform,
    pub preservation: TransformPreservation,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EyePairConstraintReceipt {
    pub candidate_relative_rotation_degrees: f64,
    pub final_relative_rotation_degrees: f64,
    pub left_scale: f64,
    pub right_scale: f64,
    pub shared_rotation_degrees: f64,
    pub exact_rigid_globes: bool,
    pub uniform_similarity_globes: bool,
    pub left_anchor_cage_vertex_count: usize,
    pub right_anchor_cage_vertex_count: usize,
    pub left_fell_back_to_translation: bool,
    pub right_fell_back_to_translation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NostrilPairConstraintReceipt {
    pub candidate_relative_rotation_degrees: f64,
    pub final_relative_rotation_degrees: f64,
    pub left_scale: f64,
    pub right_scale: f64,
    pub exact_rigid_loops: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MouthSpacingReceipts {
    pub upper_to_lower_teeth: SpacingPreservation,
    pub upper_to_lower_gums: SpacingPreservation,
    pub upper_teeth_to_tongue: SpacingPreservation,
    pub lower_teeth_to_tongue: SpacingPreservation,
}

impl MouthSpacingReceipts {
    #[must_use]
    pub fn all_preserved(self) -> bool {
        [
            self.upper_to_lower_teeth,
            self.upper_to_lower_gums,
            self.upper_teeth_to_tongue,
            self.lower_teeth_to_tongue,
        ]
        .into_iter()
        .all(|spacing| spacing.absolute_error <= (spacing.expected_minimum * 1.0e-6).max(1.0e-8))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MouthAssemblyReceipt {
    pub component: ComponentTransformReceipt,
    pub upper_jaw_vertex_count: usize,
    pub lower_jaw_vertex_count: usize,
    pub tongue_vertex_count: usize,
    pub inner_mouth_vertex_count: usize,
    pub spacing: MouthSpacingReceipts,
    pub uniform_scale_only: bool,
    pub connected_inner_mouth_split: bool,
    pub anchor_cage_vertex_count: usize,
    pub fell_back_to_tighter_transform: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MouthComponentReceipts {
    pub upper_jaw: ComponentTransformReceipt,
    pub lower_jaw: ComponentTransformReceipt,
    pub tongue: ComponentTransformReceipt,
    pub inner_mouth: ComponentTransformReceipt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnatomyQualityChecks {
    pub eye_pair_safe_uniform_scale_identity_rotation: bool,
    pub eye_pair_shared_orientation: bool,
    pub eye_globe_pair_distances: bool,
    pub eye_attachments_follow_globes: bool,
    pub mouth_single_bounded_similarity: bool,
    pub mouth_component_shapes: bool,
    pub mouth_internal_spacing: bool,
    pub nostril_pair_unit_scale: bool,
    pub nostril_pair_shared_orientation: bool,
    pub nostril_loop_pair_distances: bool,
    pub skin_transition_topology: bool,
    pub skin_transition_quality_margin: bool,
    pub skin_transition_area_ratio: bool,
    pub skin_transition_full_topology_guard: bool,
    pub protected_anatomy_unchanged_by_repair: bool,
}

impl AnatomyQualityChecks {
    fn failed(self) -> Vec<&'static str> {
        [
            (
                "eye_pair_safe_uniform_scale_identity_rotation",
                self.eye_pair_safe_uniform_scale_identity_rotation,
            ),
            (
                "eye_pair_shared_orientation",
                self.eye_pair_shared_orientation,
            ),
            ("eye_globe_pair_distances", self.eye_globe_pair_distances),
            (
                "eye_attachments_follow_globes",
                self.eye_attachments_follow_globes,
            ),
            (
                "mouth_single_bounded_similarity",
                self.mouth_single_bounded_similarity,
            ),
            ("mouth_component_shapes", self.mouth_component_shapes),
            ("mouth_internal_spacing", self.mouth_internal_spacing),
            ("nostril_pair_unit_scale", self.nostril_pair_unit_scale),
            (
                "nostril_pair_shared_orientation",
                self.nostril_pair_shared_orientation,
            ),
            (
                "nostril_loop_pair_distances",
                self.nostril_loop_pair_distances,
            ),
            ("skin_transition_topology", self.skin_transition_topology),
            (
                "skin_transition_quality_margin",
                self.skin_transition_quality_margin,
            ),
            (
                "skin_transition_area_ratio",
                self.skin_transition_area_ratio,
            ),
            (
                "skin_transition_full_topology_guard",
                self.skin_transition_full_topology_guard,
            ),
            (
                "protected_anatomy_unchanged_by_repair",
                self.protected_anatomy_unchanged_by_repair,
            ),
        ]
        .into_iter()
        .filter_map(|(name, passed)| (!passed).then_some(name))
        .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnatomyQualityGateReceipt {
    pub passed: bool,
    pub checks: AnatomyQualityChecks,
    pub distance_tolerance: f64,
    pub orientation_tolerance_degrees: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TotalAnatomyMovementReceipt {
    pub moved_vertex_count: usize,
    pub newly_owned_internal_vertex_count: usize,
    pub overwritten_skin_boundary_vertex_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeadAnatomyTransforms {
    pub left_eye: SimilarityTransform,
    pub right_eye: SimilarityTransform,
    pub left_eye_attached: SimilarityTransform,
    pub right_eye_attached: SimilarityTransform,
    pub mouth_assembly: SimilarityTransform,
    pub upper_jaw: SimilarityTransform,
    pub lower_jaw: SimilarityTransform,
    pub tongue: SimilarityTransform,
    pub inner_mouth: SimilarityTransform,
    pub left_nostril: SimilarityTransform,
    pub right_nostril: SimilarityTransform,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HeadAnatomyReceipt {
    pub component_counts: AnatomyComponentCounts,
    pub left_eye: ComponentTransformReceipt,
    pub right_eye: ComponentTransformReceipt,
    pub left_eye_attached: ComponentTransformReceipt,
    pub right_eye_attached: ComponentTransformReceipt,
    pub eye_pair: EyePairConstraintReceipt,
    pub mouth_assembly: MouthAssemblyReceipt,
    pub mouth_components: MouthComponentReceipts,
    pub left_nostril: ComponentTransformReceipt,
    pub right_nostril: ComponentTransformReceipt,
    pub nostril_pair: NostrilPairConstraintReceipt,
    pub skin_transition: SkinTransitionReceipt,
    pub quality_gate: AnatomyQualityGateReceipt,
    pub total_movement: TotalAnatomyMovementReceipt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropagatedHeadAnatomy {
    pub vertices: Vec<Vec3>,
    pub components: HeadAnatomyComponents,

    pub transition_collar: Vec<usize>,
    pub transforms: HeadAnatomyTransforms,
    pub receipt: HeadAnatomyReceipt,
}

fn union_sorted(sets: &[&[usize]]) -> Vec<usize> {
    sets.iter()
        .flat_map(|set| set.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

#[derive(Clone, Debug)]
struct LocalAnchorCage {
    indices: Vec<usize>,
}

#[derive(Clone, Debug)]
struct SkinAnchorTopology {
    adjacency: Vec<Vec<usize>>,
    interface_components: Vec<Vec<usize>>,
}

fn ordered_edge(first: usize, second: usize) -> (usize, usize) {
    if first < second {
        (first, second)
    } else {
        (second, first)
    }
}

fn boundary_components(
    edge_counts: &BTreeMap<(usize, usize), usize>,
    vertex_count: usize,
) -> Vec<Vec<usize>> {
    let mut adjacency = vec![BTreeSet::new(); vertex_count];
    for (&(first, second), &count) in edge_counts {
        if count == 1 {
            adjacency[first].insert(second);
            adjacency[second].insert(first);
        }
    }
    let mut unvisited = adjacency
        .iter()
        .enumerate()
        .filter_map(|(index, neighbors)| (!neighbors.is_empty()).then_some(index))
        .collect::<BTreeSet<_>>();
    let mut components = Vec::new();
    while let Some(&start) = unvisited.iter().next() {
        let mut component = BTreeSet::new();
        let mut pending = vec![start];
        unvisited.remove(&start);
        while let Some(index) = pending.pop() {
            component.insert(index);
            for &neighbor in &adjacency[index] {
                if unvisited.remove(&neighbor) {
                    pending.push(neighbor);
                }
            }
        }
        if component.len() >= 3 {
            components.push(component.into_iter().collect());
        }
    }
    components
}

fn build_skin_anchor_topology(geometry: &DazGeometry, skin_mask: &[bool]) -> SkinAnchorTopology {
    let skin_faces = geometry.face_mask_for_materials(HEAD_SKIN_MATERIALS.iter().copied());
    let mut adjacency = vec![BTreeSet::new(); geometry.vertices.len()];
    let mut boundary_edges = BTreeMap::new();
    for (face, selected) in geometry.faces.iter().zip(skin_faces) {
        if !selected {
            continue;
        }
        for offset in 0..face.len() {
            let first = face[offset] as usize;
            let second = face[(offset + 1) % face.len()] as usize;
            if !skin_mask[first] || !skin_mask[second] {
                continue;
            }
            adjacency[first].insert(second);
            adjacency[second].insert(first);
            *boundary_edges
                .entry(ordered_edge(first, second))
                .or_default() += 1;
        }
    }
    SkinAnchorTopology {
        adjacency: adjacency
            .into_iter()
            .map(|neighbors| neighbors.into_iter().collect())
            .collect(),
        interface_components: boundary_components(&boundary_edges, geometry.vertices.len()),
    }
}

fn face_selection_boundary(
    geometry: &DazGeometry,
    face_mask: &[bool],
    skin_mask: &[bool],
) -> Vec<usize> {
    let mut edges = BTreeMap::<(usize, usize), usize>::new();
    for (face, selected) in geometry.faces.iter().zip(face_mask.iter().copied()) {
        if !selected {
            continue;
        }
        for offset in 0..face.len() {
            let first = face[offset] as usize;
            let second = face[(offset + 1) % face.len()] as usize;
            if skin_mask[first] && skin_mask[second] {
                *edges.entry(ordered_edge(first, second)).or_default() += 1;
            }
        }
    }
    edges
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .flat_map(|((first, second), _)| [first, second])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn expand_skin_cage(
    seeds: &[usize],
    topology: &SkinAnchorTopology,
    ring_count: usize,
) -> LocalAnchorCage {
    let mut selected = seeds.iter().copied().collect::<BTreeSet<_>>();
    let mut frontier = seeds.iter().copied().collect::<VecDeque<_>>();
    for _ in 0..ring_count {
        let count = frontier.len();
        for _ in 0..count {
            let index = frontier.pop_front().expect("frontier length was sampled");
            for &neighbor in &topology.adjacency[index] {
                if selected.insert(neighbor) {
                    frontier.push_back(neighbor);
                }
            }
        }
    }
    LocalAnchorCage {
        indices: selected.into_iter().collect(),
    }
}

fn nearest_indices(base: &[Vec3], candidates: &[usize], center: Vec3, count: usize) -> Vec<usize> {
    let mut ranked = candidates
        .iter()
        .copied()
        .map(|index| (index, (base[index] - center).norm_squared()))
        .collect::<Vec<_>>();
    ranked.sort_by(|(left_index, left), (right_index, right)| {
        left.total_cmp(right).then(left_index.cmp(right_index))
    });
    ranked.truncate(count.min(ranked.len()));
    ranked.into_iter().map(|(index, _)| index).collect()
}

fn nearest_interface_component<'a>(
    topology: &'a SkinAnchorTopology,
    base: &[Vec3],
    center: Vec3,
    maximum_distance: f64,
) -> Option<&'a [usize]> {
    topology
        .interface_components
        .iter()
        .filter_map(|component| {
            let distance = component
                .iter()
                .map(|&index| (base[index] - center).norm())
                .reduce(f64::min)?;
            (distance <= maximum_distance).then_some((component.as_slice(), distance))
        })
        .min_by(|(left_indices, left), (right_indices, right)| {
            left.total_cmp(right)
                .then_with(|| left_indices.first().cmp(&right_indices.first()))
        })
        .map(|(component, _)| component)
}

fn eye_anchor_cage(
    topology: &SkinAnchorTopology,
    base: &[Vec3],
    skin_indices: &[usize],
    center: Vec3,
) -> LocalAnchorCage {
    let seeds = nearest_interface_component(topology, base, center, 5.0)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| nearest_indices(base, skin_indices, center, 24));
    let cage = expand_skin_cage(&seeds, topology, 3);
    if cage.indices.len() >= 8 {
        cage
    } else {
        LocalAnchorCage {
            indices: nearest_indices(base, skin_indices, center, 64),
        }
    }
}

fn mouth_anchor_cage(
    geometry: &DazGeometry,
    topology: &SkinAnchorTopology,
    skin_mask: &[bool],
    base: &[Vec3],
    skin_indices: &[usize],
    center: Vec3,
) -> LocalAnchorCage {
    let lip_faces = geometry.face_mask_for_materials(["Lips"]);
    let mut seeds = face_selection_boundary(geometry, &lip_faces, skin_mask);
    if seeds.len() < 8 {
        seeds = material_vertices(geometry, &["Lips"]).unwrap_or_default();
    }
    if seeds.len() < 8 {
        seeds = nearest_interface_component(topology, base, center, 6.0)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| nearest_indices(base, skin_indices, center, 32));
    }
    let cage = expand_skin_cage(&seeds, topology, 2);
    if cage.indices.len() >= 8 {
        cage
    } else {
        LocalAnchorCage {
            indices: nearest_indices(base, skin_indices, center, 96),
        }
    }
}

fn residual_rms(
    transform: SimilarityTransform,
    canonical: &[Vec3],
    fitted: &[Vec3],
    anchors: &[usize],
) -> f64 {
    let squared_error = anchors
        .iter()
        .map(|&index| (transform.apply(canonical[index]) - fitted[index]).norm_squared())
        .sum::<f64>();
    (squared_error / anchors.len().max(1) as f64).sqrt()
}

fn estimate_cage_similarity(
    canonical: &[Vec3],
    fitted: &[Vec3],
    cage: &LocalAnchorCage,
    center: Vec3,
    mut config: LocalSimilarityConfig,
) -> Result<LocalTransformResult, AnatomyError> {
    let source = cage
        .indices
        .iter()
        .map(|&index| canonical[index])
        .collect::<Vec<_>>();
    let target = cage
        .indices
        .iter()
        .map(|&index| fitted[index])
        .collect::<Vec<_>>();
    config.anchor_count = config.anchor_count.min(source.len());
    let local = estimate_local_similarity(&source, &target, center, config)?;
    let anchor_indices = local
        .anchor_indices
        .iter()
        .map(|&index| cage.indices[index])
        .collect::<Vec<_>>();
    Ok(LocalTransformResult {
        transform: local.transform,
        anchor_rms: residual_rms(local.transform, canonical, fitted, &anchor_indices),
        anchor_indices,
    })
}

fn transform_at_mapped_center(
    candidate: SimilarityTransform,
    center: Vec3,
    scale: f64,
    rotation: Mat3,
) -> SimilarityTransform {
    let mapped_center = candidate.apply(center);
    SimilarityTransform {
        scale,
        rotation,
        translation: mapped_center - rotation.transform_vector(center) * scale,
    }
}

fn local_result_for_global_transform(
    transform: SimilarityTransform,
    candidate: &LocalTransformResult,
    canonical: &[Vec3],
    fitted: &[Vec3],
) -> LocalTransformResult {
    LocalTransformResult {
        transform,
        anchor_indices: candidate.anchor_indices.clone(),
        anchor_rms: residual_rms(transform, canonical, fitted, &candidate.anchor_indices),
    }
}

fn choose_eye_similarity(
    candidate: &LocalTransformResult,
    center: Vec3,
    canonical: &[Vec3],
    fitted: &[Vec3],
) -> (LocalTransformResult, bool) {
    let mut scale = candidate
        .transform
        .scale
        .clamp(EYE_MINIMUM_SCALE, EYE_MAXIMUM_SCALE);
    if (scale - 1.0).abs() <= SCALE_SNAP_TO_IDENTITY {
        scale = 1.0;
    }
    let scaled = transform_at_mapped_center(candidate.transform, center, scale, Mat3::IDENTITY);
    let translation = transform_at_mapped_center(candidate.transform, center, 1.0, Mat3::IDENTITY);
    let scaled_rms = residual_rms(scaled, canonical, fitted, &candidate.anchor_indices);
    let translation_rms = residual_rms(translation, canonical, fitted, &candidate.anchor_indices);
    let use_scaled =
        scale != 1.0 && scaled_rms.is_finite() && scaled_rms + 1.0e-6 < translation_rms * 0.98;
    let selected = if use_scaled { scaled } else { translation };
    (
        local_result_for_global_transform(selected, candidate, canonical, fitted),
        !use_scaled && scale != 1.0,
    )
}

fn choose_mouth_similarity(
    candidate: &LocalTransformResult,
    center: Vec3,
    canonical: &[Vec3],
    fitted: &[Vec3],
) -> (LocalTransformResult, bool) {
    let mut full = candidate.transform;
    full.scale = full.scale.clamp(MOUTH_MINIMUM_SCALE, MOUTH_MAXIMUM_SCALE);
    if (full.scale - 1.0).abs() <= SCALE_SNAP_TO_IDENTITY {
        full = transform_at_mapped_center(full, center, 1.0, full.rotation);
    }
    let translation = transform_at_mapped_center(full, center, 1.0, Mat3::IDENTITY);
    let full_rms = residual_rms(full, canonical, fitted, &candidate.anchor_indices);
    let translation_rms = residual_rms(translation, canonical, fitted, &candidate.anchor_indices);
    let unstable = !full_rms.is_finite()
        || rotation_distance_degrees(full.rotation, Mat3::IDENTITY) > 25.0
        || (full.scale == MOUTH_MINIMUM_SCALE || full.scale == MOUTH_MAXIMUM_SCALE)
            && full_rms > 0.20;
    let meaningful_improvement = full_rms + 1.0e-5 < translation_rms * 0.97;
    if !unstable && meaningful_improvement {
        return (
            local_result_for_global_transform(full, candidate, canonical, fitted),
            false,
        );
    }

    let tight_scale = 1.0 + (full.scale - 1.0) * 0.5;
    let tight_rotation = if rotation_distance_degrees(full.rotation, Mat3::IDENTITY) <= 12.0 {
        full.rotation
    } else {
        Mat3::IDENTITY
    };
    let tight = transform_at_mapped_center(full, center, tight_scale, tight_rotation);
    let tight_rms = residual_rms(tight, canonical, fitted, &candidate.anchor_indices);
    let selected = if tight_rms < translation_rms {
        tight
    } else {
        translation
    };
    (
        local_result_for_global_transform(selected, candidate, canonical, fitted),
        true,
    )
}

#[derive(Debug, PartialEq)]
struct TransitionComposition {
    vertices: Vec<Vec3>,
    repair_mask: Vec<bool>,
    collar: Vec<usize>,
}

fn compose_transition_proposals(
    baseline: &[Vec3],
    proposals: &[(&[usize], &[usize], &[Vec3])],
) -> TransitionComposition {
    let mut vertices = baseline.to_vec();
    let mut owner_ring = vec![usize::MAX; baseline.len()];
    let mut owner_count = vec![0_u32; baseline.len()];
    for &(indices, rings, proposal) in proposals {
        debug_assert_eq!(indices.len(), rings.len());
        debug_assert_eq!(proposal.len(), baseline.len());
        for (&index, &ring) in indices.iter().zip(rings) {
            debug_assert!(index < baseline.len());
            if ring < owner_ring[index] {
                owner_ring[index] = ring;
                owner_count[index] = 1;
                vertices[index] = proposal[index];
            } else if ring == owner_ring[index] {
                owner_count[index] += 1;
                let current = vertices[index];
                vertices[index] =
                    current + (proposal[index] - current) / f64::from(owner_count[index]);
            }
        }
    }
    let collar = owner_ring
        .iter()
        .enumerate()
        .filter_map(|(index, &ring)| (ring == 1).then_some(index))
        .collect();
    let repair_mask = owner_ring
        .iter()
        .map(|&ring| ring > 1 && ring != usize::MAX)
        .collect();
    TransitionComposition {
        vertices,
        repair_mask,
        collar,
    }
}

fn first_passing_or_last<T, E>(
    attempts: &[usize],
    mut run: impl FnMut(usize) -> Result<(T, bool), E>,
) -> Result<Option<T>, E> {
    let mut last = None;
    for &attempt in attempts {
        let (value, passed) = run(attempt)?;
        if passed {
            return Ok(Some(value));
        }
        last = Some(value);
    }
    Ok(last)
}

fn require_component(indices: &[usize], component: &'static str) -> Result<(), AnatomyError> {
    if indices.is_empty() {
        Err(AnatomyError::MissingComponent { component })
    } else {
        Ok(())
    }
}

pub fn discover_head_anatomy(
    geometry: &DazGeometry,
) -> Result<HeadAnatomyComponents, AnatomyError> {
    let skin = material_vertices(geometry, HEAD_SKIN_MATERIALS)?;
    let left_eye = polygon_group_vertices(geometry, &["lEye"])?;
    let right_eye = polygon_group_vertices(geometry, &["rEye"])?;
    let attached = material_vertices(geometry, EYE_ATTACHED_MATERIALS)?;
    let mut left_eye_attached = Vec::new();
    let mut right_eye_attached = Vec::new();
    for index in attached {
        if geometry.vertices[index][0] >= 0.0 {
            left_eye_attached.push(index);
        } else {
            right_eye_attached.push(index);
        }
    }

    let upper_jaw = polygon_group_vertices(geometry, &["upperJaw"])?;
    let lower_jaw = polygon_group_vertices(geometry, &["lowerJaw"])?;
    let tongue_group = polygon_group_vertices(geometry, &["tongue"])?;
    let tongue_material = material_vertices(geometry, &["Tongue"])?;
    let tongue = union_sorted(&[&tongue_group, &tongue_material]);
    let inner_mouth = material_vertices(geometry, &["InnerMouth"])?;
    let mouth_assembly = union_sorted(&[&upper_jaw, &lower_jaw, &tongue, &inner_mouth]);
    let upper_teeth = material_and_polygon_group_vertices(geometry, &["Teeth"], &["upperJaw"])?;
    let lower_teeth = material_and_polygon_group_vertices(geometry, &["Teeth"], &["lowerJaw"])?;
    let upper_gums = material_and_polygon_group_vertices(geometry, &["Gums"], &["upperJaw"])?;
    let lower_gums = material_and_polygon_group_vertices(geometry, &["Gums"], &["lowerJaw"])?;

    let nostrils = material_vertices(geometry, &["Nostrils"])?;
    let mut left_nostril = Vec::new();
    let mut right_nostril = Vec::new();
    for index in nostrils {
        if geometry.vertices[index][0] >= 0.0 {
            left_nostril.push(index);
        } else {
            right_nostril.push(index);
        }
    }

    for (indices, name) in [
        (&skin, "head skin"),
        (&left_eye, "lEye"),
        (&right_eye, "rEye"),
        (&upper_jaw, "upperJaw"),
        (&lower_jaw, "lowerJaw"),
        (&tongue, "tongue"),
        (&inner_mouth, "InnerMouth"),
        (&upper_teeth, "upperJaw Teeth"),
        (&lower_teeth, "lowerJaw Teeth"),
        (&upper_gums, "upperJaw Gums"),
        (&lower_gums, "lowerJaw Gums"),
        (&left_nostril, "left Nostrils"),
        (&right_nostril, "right Nostrils"),
    ] {
        require_component(indices, name)?;
    }

    Ok(HeadAnatomyComponents {
        skin,
        left_eye,
        right_eye,
        left_eye_attached,
        right_eye_attached,
        upper_jaw,
        lower_jaw,
        tongue,
        inner_mouth,
        mouth_assembly,
        upper_teeth,
        lower_teeth,
        upper_gums,
        lower_gums,
        left_nostril,
        right_nostril,
    })
}

fn centroid(vertices: &[Vec3], indices: &[usize]) -> Result<Vec3, AnatomyError> {
    require_component(indices, "centroid selection")?;
    Ok(indices
        .iter()
        .copied()
        .map(|index| vertices[index])
        .fold(Vec3::ZERO, |sum, point| sum + point)
        / indices.len() as f64)
}

fn local_result_for_transform(
    transform: SimilarityTransform,
    anchor_indices: &[usize],
    base_skin: &[Vec3],
    fitted_skin: &[Vec3],
) -> LocalTransformResult {
    let squared_error: f64 = anchor_indices
        .iter()
        .copied()
        .map(|index| {
            let residual = transform.apply(base_skin[index]) - fitted_skin[index];
            residual.norm_squared()
        })
        .sum();
    LocalTransformResult {
        transform,
        anchor_indices: anchor_indices.to_vec(),
        anchor_rms: (squared_error / anchor_indices.len().max(1) as f64).sqrt(),
    }
}

fn mat3_to_na(matrix: Mat3) -> Matrix3<f64> {
    let rows = matrix.rows();
    Matrix3::from_row_slice(&[
        rows[0][0], rows[0][1], rows[0][2], rows[1][0], rows[1][1], rows[1][2], rows[2][0],
        rows[2][1], rows[2][2],
    ])
}

fn rotation_distance_degrees(first: Mat3, second: Mat3) -> f64 {
    if first == second {
        return 0.0;
    }
    let relative = mat3_to_na(first) * mat3_to_na(second).transpose();
    (((relative.trace() - 1.0) * 0.5).clamp(-1.0, 1.0)).acos() * 180.0 / std::f64::consts::PI
}

fn average_rotations(rotations: [Mat3; 2], weights: [f64; 2]) -> Result<Mat3, AnatomyError> {
    if weights
        .iter()
        .any(|weight| !weight.is_finite() || *weight < 0.0)
        || weights.iter().sum::<f64>() <= 0.0
    {
        return Err(AnatomyError::RotationAverageFailed);
    }
    let matrix = mat3_to_na(rotations[0]) * weights[0] + mat3_to_na(rotations[1]) * weights[1];
    let decomposition = matrix.svd(true, true);
    let u = decomposition.u.ok_or(AnatomyError::RotationAverageFailed)?;
    let v_t = decomposition
        .v_t
        .ok_or(AnatomyError::RotationAverageFailed)?;
    let mut correction = Matrix3::identity();
    if (u * v_t).determinant() < 0.0 {
        correction[(2, 2)] = -1.0;
    }
    Ok(Mat3::from_na(u * correction * v_t))
}

struct VertexOwnership<'a> {
    skin_mask: &'a [bool],

    owned: &'a mut [bool],

    anatomy_owned: &'a mut [bool],

    overwritten_skin: &'a mut [bool],
}

fn apply_group(
    base: &[Vec3],
    result: &mut [Vec3],
    indices: &[usize],
    local: &LocalTransformResult,
    ownership: &mut VertexOwnership<'_>,
    overwrite_skin_vertices: bool,
) -> Result<ComponentTransformReceipt, AnatomyError> {
    let VertexOwnership {
        skin_mask,
        owned,
        anatomy_owned,
        overwritten_skin,
    } = ownership;
    let requested: Vec<_> = indices
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let movable: Vec<_> = requested
        .iter()
        .copied()
        .filter(|&index| {
            if overwrite_skin_vertices {
                !anatomy_owned[index]
            } else {
                !owned[index]
            }
        })
        .collect();
    for &index in &movable {
        result[index] = local.transform.apply(base[index]);
        owned[index] = true;
        anatomy_owned[index] = true;
        overwritten_skin[index] = skin_mask[index];
    }
    Ok(ComponentTransformReceipt {
        requested_vertex_count: requested.len(),
        applied_vertex_count: movable.len(),
        overwritten_skin_vertex_count: movable.iter().filter(|&&index| skin_mask[index]).count(),
        anchor_count: local.anchor_indices.len(),
        anchor_rms: local.anchor_rms,
        transform: local.transform,
        preservation: transform_preservation(base, result, &movable, local.transform, 160)?,
    })
}

fn existing_component_receipt(
    base: &[Vec3],
    result: &[Vec3],
    indices: &[usize],
    local: &LocalTransformResult,
    skin_mask: &[bool],
) -> Result<ComponentTransformReceipt, AnatomyError> {
    Ok(ComponentTransformReceipt {
        requested_vertex_count: indices.len(),
        applied_vertex_count: indices.len(),
        overwritten_skin_vertex_count: indices.iter().filter(|&&index| skin_mask[index]).count(),
        anchor_count: local.anchor_indices.len(),
        anchor_rms: local.anchor_rms,
        transform: local.transform,
        preservation: transform_preservation(base, result, indices, local.transform, 160)?,
    })
}

fn pairwise_exact(receipt: &ComponentTransformReceipt) -> bool {
    receipt.preservation.pairwise.maximum_error <= DISTANCE_TOLERANCE
        && receipt.preservation.residual_maximum <= DISTANCE_TOLERANCE
}

fn identity_rotation(rotation: Mat3) -> bool {
    rotation_distance_degrees(rotation, Mat3::IDENTITY) <= ORIENTATION_TOLERANCE_DEGREES
}

pub fn propagate_head_anatomy_auto(
    geometry: &DazGeometry,
    fitted_vertices: &[Vec3],
) -> Result<PropagatedHeadAnatomy, AnatomyError> {
    propagate_head_anatomy_auto_with_orientation_margin(
        geometry,
        fitted_vertices,
        SKIN_TRANSITION_MIN_ORIENTATION_COSINE,
    )
}

pub fn propagate_head_anatomy_auto_with_orientation_margin(
    geometry: &DazGeometry,
    fitted_vertices: &[Vec3],
    minimum_orientation_cosine: f64,
) -> Result<PropagatedHeadAnatomy, AnatomyError> {
    if !minimum_orientation_cosine.is_finite()
        || !(-1.0..=1.0).contains(&minimum_orientation_cosine)
    {
        return Err(AnatomyError::InvalidTransitionConfiguration);
    }
    let components = discover_head_anatomy(geometry)?;
    let skin = components.skin.clone();
    propagate_with_components(
        geometry,
        fitted_vertices,
        &skin,
        components,
        minimum_orientation_cosine.max(SKIN_TRANSITION_MIN_ORIENTATION_COSINE),
        None,
    )
}

pub fn propagate_head_anatomy_auto_with_topology_guard(
    geometry: &DazGeometry,
    fitted_vertices: &[Vec3],
    minimum_orientation_cosine: f64,
    minimum_area_ratio: f64,
    maximum_area_ratio: f64,
) -> Result<PropagatedHeadAnatomy, AnatomyError> {
    if !minimum_orientation_cosine.is_finite()
        || !(-1.0..=1.0).contains(&minimum_orientation_cosine)
        || !minimum_area_ratio.is_finite()
        || !maximum_area_ratio.is_finite()
        || minimum_area_ratio <= 0.0
        || minimum_area_ratio > maximum_area_ratio
    {
        return Err(AnatomyError::InvalidTransitionConfiguration);
    }
    let components = discover_head_anatomy(geometry)?;
    let skin = components.skin.clone();
    propagate_with_components(
        geometry,
        fitted_vertices,
        &skin,
        components,
        minimum_orientation_cosine.max(SKIN_TRANSITION_MIN_ORIENTATION_COSINE),
        Some((minimum_area_ratio, maximum_area_ratio)),
    )
}

pub fn propagate_head_anatomy(
    geometry: &DazGeometry,
    fitted_vertices: &[Vec3],
    skin_global_indices: &[usize],
) -> Result<PropagatedHeadAnatomy, AnatomyError> {
    let components = discover_head_anatomy(geometry)?;
    propagate_with_components(
        geometry,
        fitted_vertices,
        skin_global_indices,
        components,
        SKIN_TRANSITION_MIN_ORIENTATION_COSINE,
        None,
    )
}

fn propagate_with_components(
    geometry: &DazGeometry,
    fitted_vertices: &[Vec3],
    skin_global_indices: &[usize],
    components: HeadAnatomyComponents,
    minimum_orientation_cosine: f64,
    area_ratio_bounds: Option<(f64, f64)>,
) -> Result<PropagatedHeadAnatomy, AnatomyError> {
    let base: Vec<_> = geometry.vertices.iter().copied().map(Vec3::from).collect();
    if base.len() != fitted_vertices.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: base.len(),
            fitted: fitted_vertices.len(),
        });
    }
    for (name, vertices) in [("canonical", base.as_slice()), ("fitted", fitted_vertices)] {
        if let Some((index, _)) = vertices
            .iter()
            .enumerate()
            .find(|(_, vertex)| !vertex.is_finite())
        {
            return Err(AnatomyError::NonFiniteVertex { name, index });
        }
    }
    if let Some(&index) = skin_global_indices
        .iter()
        .find(|&&index| index >= base.len())
    {
        return Err(AnatomyError::IndexOutOfRange {
            index,
            vertex_count: base.len(),
        });
    }
    let skin_indices: Vec<_> = skin_global_indices
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if skin_indices.len() < 8 {
        return Err(AnatomyError::TooFewSkinVertices {
            required: 8,
            actual: skin_indices.len(),
        });
    }
    let base_skin: Vec<_> = skin_indices.iter().map(|&index| base[index]).collect();
    let fitted_skin: Vec<_> = skin_indices
        .iter()
        .map(|&index| fitted_vertices[index])
        .collect();
    let mut skin_mask = vec![false; base.len()];
    for &index in &skin_indices {
        skin_mask[index] = true;
    }
    let anchor_topology = build_skin_anchor_topology(geometry, &skin_mask);

    let mut result = fitted_vertices.to_vec();
    let mut owned = skin_mask.clone();
    let mut anatomy_owned = vec![false; base.len()];
    let mut overwritten_skin = vec![false; base.len()];

    let mut ownership = VertexOwnership {
        skin_mask: &skin_mask,
        owned: &mut owned,
        anatomy_owned: &mut anatomy_owned,
        overwritten_skin: &mut overwritten_skin,
    };

    let left_eye_center = centroid(&base, &components.left_eye)?;
    let right_eye_center = centroid(&base, &components.right_eye)?;
    let left_eye_cage = eye_anchor_cage(&anchor_topology, &base, &skin_indices, left_eye_center);
    let right_eye_cage = eye_anchor_cage(&anchor_topology, &base, &skin_indices, right_eye_center);
    let eye_config = LocalSimilarityConfig {
        anchor_count: 160,
        gaussian_sigma: 1.45,
        minimum_scale: EYE_MINIMUM_SCALE,
        maximum_scale: EYE_MAXIMUM_SCALE,
        robust_iterations: 5,
    };
    let left_eye_candidate = estimate_cage_similarity(
        &base,
        fitted_vertices,
        &left_eye_cage,
        left_eye_center,
        eye_config,
    )?;
    let right_eye_candidate = estimate_cage_similarity(
        &base,
        fitted_vertices,
        &right_eye_cage,
        right_eye_center,
        eye_config,
    )?;
    let (left_eye_local, left_eye_fallback) =
        choose_eye_similarity(&left_eye_candidate, left_eye_center, &base, fitted_vertices);
    let (right_eye_local, right_eye_fallback) = choose_eye_similarity(
        &right_eye_candidate,
        right_eye_center,
        &base,
        fitted_vertices,
    );
    let left_eye_transform = left_eye_local.transform;
    let right_eye_transform = right_eye_local.transform;
    let left_eye = apply_group(
        &base,
        &mut result,
        &components.left_eye,
        &left_eye_local,
        &mut ownership,
        true,
    )?;
    let right_eye = apply_group(
        &base,
        &mut result,
        &components.right_eye,
        &right_eye_local,
        &mut ownership,
        true,
    )?;
    let left_eye_attached = apply_group(
        &base,
        &mut result,
        &components.left_eye_attached,
        &left_eye_local,
        &mut ownership,
        false,
    )?;
    let right_eye_attached = apply_group(
        &base,
        &mut result,
        &components.right_eye_attached,
        &right_eye_local,
        &mut ownership,
        false,
    )?;
    let eye_pair = EyePairConstraintReceipt {
        candidate_relative_rotation_degrees: rotation_distance_degrees(
            left_eye_candidate.transform.rotation,
            right_eye_candidate.transform.rotation,
        ),
        final_relative_rotation_degrees: rotation_distance_degrees(
            left_eye_transform.rotation,
            right_eye_transform.rotation,
        ),
        left_scale: left_eye_transform.scale,
        right_scale: right_eye_transform.scale,
        shared_rotation_degrees: rotation_distance_degrees(
            left_eye_transform.rotation,
            Mat3::IDENTITY,
        ),
        exact_rigid_globes: (left_eye_transform.scale - 1.0).abs() <= 1.0e-12
            && (right_eye_transform.scale - 1.0).abs() <= 1.0e-12,
        uniform_similarity_globes: true,
        left_anchor_cage_vertex_count: left_eye_cage.indices.len(),
        right_anchor_cage_vertex_count: right_eye_cage.indices.len(),
        left_fell_back_to_translation: left_eye_fallback,
        right_fell_back_to_translation: right_eye_fallback,
    };

    let mouth_coarse_center =
        centroid(&base, &components.mouth_assembly)? + Vec3::new(0.0, 0.25, 1.5);
    let mouth_cage = mouth_anchor_cage(
        geometry,
        &anchor_topology,
        &skin_mask,
        &base,
        &skin_indices,
        mouth_coarse_center,
    );
    let mouth_center = centroid(&base, &mouth_cage.indices)?;
    let mouth_candidate = estimate_cage_similarity(
        &base,
        fitted_vertices,
        &mouth_cage,
        mouth_center,
        LocalSimilarityConfig {
            anchor_count: 280,
            gaussian_sigma: 2.1,
            minimum_scale: MOUTH_MINIMUM_SCALE,
            maximum_scale: MOUTH_MAXIMUM_SCALE,
            robust_iterations: 5,
        },
    )?;
    let (mouth_local, mouth_fallback) =
        choose_mouth_similarity(&mouth_candidate, mouth_center, &base, fitted_vertices);
    let mouth_component = apply_group(
        &base,
        &mut result,
        &components.mouth_assembly,
        &mouth_local,
        &mut ownership,
        true,
    )?;
    let mouth_components = MouthComponentReceipts {
        upper_jaw: existing_component_receipt(
            &base,
            &result,
            &components.upper_jaw,
            &mouth_local,
            &skin_mask,
        )?,
        lower_jaw: existing_component_receipt(
            &base,
            &result,
            &components.lower_jaw,
            &mouth_local,
            &skin_mask,
        )?,
        tongue: existing_component_receipt(
            &base,
            &result,
            &components.tongue,
            &mouth_local,
            &skin_mask,
        )?,
        inner_mouth: existing_component_receipt(
            &base,
            &result,
            &components.inner_mouth,
            &mouth_local,
            &skin_mask,
        )?,
    };
    let mouth_spacing = MouthSpacingReceipts {
        upper_to_lower_teeth: spacing_preservation(
            &base,
            &result,
            &components.upper_teeth,
            &components.lower_teeth,
            mouth_local.transform.scale,
        )?,
        upper_to_lower_gums: spacing_preservation(
            &base,
            &result,
            &components.upper_gums,
            &components.lower_gums,
            mouth_local.transform.scale,
        )?,
        upper_teeth_to_tongue: spacing_preservation(
            &base,
            &result,
            &components.upper_teeth,
            &components.tongue,
            mouth_local.transform.scale,
        )?,
        lower_teeth_to_tongue: spacing_preservation(
            &base,
            &result,
            &components.lower_teeth,
            &components.tongue,
            mouth_local.transform.scale,
        )?,
    };
    let mouth_assembly = MouthAssemblyReceipt {
        component: mouth_component,
        upper_jaw_vertex_count: components.upper_jaw.len(),
        lower_jaw_vertex_count: components.lower_jaw.len(),
        tongue_vertex_count: components.tongue.len(),
        inner_mouth_vertex_count: components.inner_mouth.len(),
        spacing: mouth_spacing,
        uniform_scale_only: true,
        connected_inner_mouth_split: false,
        anchor_cage_vertex_count: mouth_cage.indices.len(),
        fell_back_to_tighter_transform: mouth_fallback,
    };

    let left_nostril_center = centroid(&base, &components.left_nostril)?;
    let right_nostril_center = centroid(&base, &components.right_nostril)?;
    let nostril_config = LocalSimilarityConfig {
        anchor_count: 120,
        gaussian_sigma: 1.3,
        minimum_scale: 1.0,
        maximum_scale: 1.0,
        robust_iterations: 5,
    };
    let left_nostril_candidate = estimate_local_similarity(
        &base_skin,
        &fitted_skin,
        left_nostril_center,
        nostril_config,
    )?;
    let right_nostril_candidate = estimate_local_similarity(
        &base_skin,
        &fitted_skin,
        right_nostril_center,
        nostril_config,
    )?;
    let shared_nostril_rotation = average_rotations(
        [
            left_nostril_candidate.transform.rotation,
            right_nostril_candidate.transform.rotation,
        ],
        [
            1.0 / left_nostril_candidate.anchor_rms.max(0.02).powi(2),
            1.0 / right_nostril_candidate.anchor_rms.max(0.02).powi(2),
        ],
    )?;
    let left_nostril_mapped = left_nostril_candidate.transform.apply(left_nostril_center);
    let right_nostril_mapped = right_nostril_candidate
        .transform
        .apply(right_nostril_center);
    let left_nostril_transform = SimilarityTransform {
        scale: 1.0,
        rotation: shared_nostril_rotation,
        translation: left_nostril_mapped
            - shared_nostril_rotation.transform_vector(left_nostril_center),
    };
    let right_nostril_transform = SimilarityTransform {
        scale: 1.0,
        rotation: shared_nostril_rotation,
        translation: right_nostril_mapped
            - shared_nostril_rotation.transform_vector(right_nostril_center),
    };
    let left_nostril_local = local_result_for_transform(
        left_nostril_transform,
        &left_nostril_candidate.anchor_indices,
        &base_skin,
        &fitted_skin,
    );
    let right_nostril_local = local_result_for_transform(
        right_nostril_transform,
        &right_nostril_candidate.anchor_indices,
        &base_skin,
        &fitted_skin,
    );
    let left_nostril = apply_group(
        &base,
        &mut result,
        &components.left_nostril,
        &left_nostril_local,
        &mut ownership,
        true,
    )?;
    let right_nostril = apply_group(
        &base,
        &mut result,
        &components.right_nostril,
        &right_nostril_local,
        &mut ownership,
        true,
    )?;
    let nostril_pair = NostrilPairConstraintReceipt {
        candidate_relative_rotation_degrees: rotation_distance_degrees(
            left_nostril_candidate.transform.rotation,
            right_nostril_candidate.transform.rotation,
        ),
        final_relative_rotation_degrees: rotation_distance_degrees(
            left_nostril_transform.rotation,
            right_nostril_transform.rotation,
        ),
        left_scale: left_nostril_transform.scale,
        right_scale: right_nostril_transform.scale,
        exact_rigid_loops: true,
    };

    let skin_triangles = triangulated_skin_faces(geometry)?;
    let adjacency = skin_adjacency(base.len(), &skin_triangles)?;
    let mouth_hard: Vec<_> = components
        .mouth_assembly
        .iter()
        .copied()
        .filter(|&index| skin_mask[index])
        .collect();
    let left_nostril_hard: Vec<_> = components
        .left_nostril
        .iter()
        .copied()
        .filter(|&index| skin_mask[index])
        .collect();
    let right_nostril_hard: Vec<_> = components
        .right_nostril
        .iter()
        .copied()
        .filter(|&index| skin_mask[index])
        .collect();
    let original_flipped_skin_triangles =
        super::flipped_triangle_count(&base, fitted_vertices, &skin_triangles)?;
    let post_anatomy = result.clone();
    let selected_transition = first_passing_or_last(&TRANSITION_RING_ATTEMPTS, |rings| {
        let mut mouth_candidate = post_anatomy.clone();
        let mouth_transition = reconcile_skin_neighborhood(
            SkinTransitionPass {
                base: &base,
                original_fitted: fitted_vertices,
                result: &mut mouth_candidate,
            },
            &adjacency,
            &mouth_hard,
            &anatomy_owned,
            mouth_local.transform,
            rings,
        )?;
        let mut left_nostril_candidate = post_anatomy.clone();
        let left_nostril_transition = reconcile_skin_neighborhood(
            SkinTransitionPass {
                base: &base,
                original_fitted: fitted_vertices,
                result: &mut left_nostril_candidate,
            },
            &adjacency,
            &left_nostril_hard,
            &anatomy_owned,
            left_nostril_transform,
            rings,
        )?;
        let mut right_nostril_candidate = post_anatomy.clone();
        let right_nostril_transition = reconcile_skin_neighborhood(
            SkinTransitionPass {
                base: &base,
                original_fitted: fitted_vertices,
                result: &mut right_nostril_candidate,
            },
            &adjacency,
            &right_nostril_hard,
            &anatomy_owned,
            right_nostril_transform,
            rings,
        )?;
        let composition = compose_transition_proposals(
            &post_anatomy,
            &[
                (
                    &mouth_transition.indices,
                    &mouth_transition.ring_numbers,
                    &mouth_candidate,
                ),
                (
                    &left_nostril_transition.indices,
                    &left_nostril_transition.ring_numbers,
                    &left_nostril_candidate,
                ),
                (
                    &right_nostril_transition.indices,
                    &right_nostril_transition.ring_numbers,
                    &right_nostril_candidate,
                ),
            ],
        );
        let mut candidate = composition.vertices;
        let pre_repair_flipped_skin_triangles =
            super::flipped_triangle_count(&base, &candidate, &skin_triangles)?;
        let repair = repair_skin_transition_topology(
            SkinTransitionPass {
                base: &base,
                original_fitted: fitted_vertices,
                result: &mut candidate,
            },
            &skin_triangles,
            &composition.repair_mask,
            &anatomy_owned,
            TRANSITION_MAX_ITERATIONS,
            minimum_orientation_cosine,
            area_ratio_bounds,
        )?;
        let final_flipped_skin_triangles =
            super::flipped_triangle_count(&base, &candidate, &skin_triangles)?;
        let area_ratios = triangle_area_ratios(&base, &candidate, &skin_triangles)?;
        let minimum_area_ratio = area_ratios.iter().copied().reduce(f64::min).unwrap_or(1.0);
        let maximum_area_ratio = area_ratios.iter().copied().reduce(f64::max).unwrap_or(1.0);
        let triangles_outside_area_ratio = area_ratio_bounds.map_or(0, |(minimum, maximum)| {
            area_ratios
                .iter()
                .filter(|&&ratio| ratio < minimum || ratio > maximum)
                .count()
        });
        let area_ratio_preserved = triangles_outside_area_ratio == 0;
        let triangles_outside_topology_guard =
            if let Some((minimum_area, maximum_area)) = area_ratio_bounds {
                let canonical_arrays = base.iter().copied().map(Vec3::to_array).collect::<Vec<_>>();
                let candidate_arrays = candidate
                    .iter()
                    .copied()
                    .map(Vec3::to_array)
                    .collect::<Vec<_>>();
                let triangle_arrays = skin_triangles
                    .iter()
                    .map(|triangle| triangle.map(|index| index as u32))
                    .collect::<Vec<_>>();
                deformation_safety_mask(
                    &canonical_arrays,
                    &candidate_arrays,
                    &triangle_arrays,
                    minimum_orientation_cosine,
                    minimum_area,
                    maximum_area,
                )?
                .into_iter()
                .filter(|unsafe_triangle| *unsafe_triangle)
                .count()
            } else {
                0
            };
        let topology_guard_preserved = triangles_outside_topology_guard == 0;
        let passed = final_flipped_skin_triangles == 0
            && repair.orientation_margin_preserved
            && area_ratio_preserved
            && topology_guard_preserved
            && !repair.protected_vertices_changed;
        let attempt = (
            candidate,
            mouth_transition,
            left_nostril_transition,
            right_nostril_transition,
            pre_repair_flipped_skin_triangles,
            final_flipped_skin_triangles,
            composition.collar,
            minimum_area_ratio,
            maximum_area_ratio,
            triangles_outside_area_ratio,
            area_ratio_preserved,
            triangles_outside_topology_guard,
            topology_guard_preserved,
            repair,
        );
        Ok::<_, AnatomyError>((attempt, passed))
    })?
    .expect("the transition attempt table is never empty");
    let (
        selected_result,
        mouth_transition,
        left_nostril_transition,
        right_nostril_transition,
        pre_repair_flipped_skin_triangles,
        final_flipped_skin_triangles,
        transition_collar,
        minimum_area_ratio,
        maximum_area_ratio,
        triangles_outside_area_ratio,
        area_ratio_preserved,
        triangles_outside_topology_guard,
        topology_guard_preserved,
        repair,
    ) = selected_transition;
    result = selected_result;
    let skin_transition = SkinTransitionReceipt {
        mouth: mouth_transition.receipt,
        left_nostril: left_nostril_transition.receipt,
        right_nostril: right_nostril_transition.receipt,
        original_flipped_skin_triangles,
        pre_repair_flipped_skin_triangles,
        final_flipped_skin_triangles,
        new_flipped_skin_triangles: final_flipped_skin_triangles
            .saturating_sub(original_flipped_skin_triangles),
        topology_preserved: final_flipped_skin_triangles == 0,
        orientation_margin_preserved: repair.orientation_margin_preserved,
        minimum_area_ratio,
        maximum_area_ratio,
        triangles_outside_area_ratio,
        area_ratio_preserved,
        triangles_outside_topology_guard,
        topology_guard_preserved,
        repair,
    };

    let checks = AnatomyQualityChecks {
        eye_pair_safe_uniform_scale_identity_rotation: (EYE_MINIMUM_SCALE..=EYE_MAXIMUM_SCALE)
            .contains(&left_eye_transform.scale)
            && (EYE_MINIMUM_SCALE..=EYE_MAXIMUM_SCALE).contains(&right_eye_transform.scale)
            && identity_rotation(left_eye_transform.rotation)
            && identity_rotation(right_eye_transform.rotation),
        eye_pair_shared_orientation: eye_pair.final_relative_rotation_degrees
            <= ORIENTATION_TOLERANCE_DEGREES,
        eye_globe_pair_distances: pairwise_exact(&left_eye) && pairwise_exact(&right_eye),
        eye_attachments_follow_globes: pairwise_exact(&left_eye_attached)
            && pairwise_exact(&right_eye_attached),
        mouth_single_bounded_similarity: mouth_assembly.uniform_scale_only
            && !mouth_assembly.connected_inner_mouth_split
            && (MOUTH_MINIMUM_SCALE..=MOUTH_MAXIMUM_SCALE).contains(&mouth_local.transform.scale),
        mouth_component_shapes: [
            &mouth_components.upper_jaw,
            &mouth_components.lower_jaw,
            &mouth_components.tongue,
            &mouth_components.inner_mouth,
        ]
        .into_iter()
        .all(pairwise_exact),
        mouth_internal_spacing: mouth_spacing.all_preserved(),
        nostril_pair_unit_scale: (left_nostril_transform.scale - 1.0).abs() <= 1.0e-12
            && (right_nostril_transform.scale - 1.0).abs() <= 1.0e-12,
        nostril_pair_shared_orientation: nostril_pair.final_relative_rotation_degrees
            <= ORIENTATION_TOLERANCE_DEGREES,
        nostril_loop_pair_distances: pairwise_exact(&left_nostril)
            && pairwise_exact(&right_nostril),
        skin_transition_topology: skin_transition.topology_preserved,
        skin_transition_quality_margin: skin_transition.orientation_margin_preserved,
        skin_transition_area_ratio: skin_transition.area_ratio_preserved,
        skin_transition_full_topology_guard: skin_transition.topology_guard_preserved,
        protected_anatomy_unchanged_by_repair: !skin_transition.repair.protected_vertices_changed,
    };
    let failed = checks.failed();
    if !failed.is_empty() {
        let (required_minimum_skin_area_ratio, required_maximum_skin_area_ratio) =
            area_ratio_bounds.unwrap_or((0.0, f64::INFINITY));
        return Err(AnatomyError::QualityGateFailed {
            failed,
            transition_rings: skin_transition.mouth.rings,
            minimum_skin_orientation_cosine: skin_transition
                .repair
                .final_minimum_orientation_cosine,
            required_minimum_skin_orientation_cosine: skin_transition
                .repair
                .minimum_orientation_cosine_required,
            remaining_skin_triangles_below_margin: skin_transition
                .repair
                .final_triangles_below_orientation_margin,
            minimum_skin_area_ratio: skin_transition.minimum_area_ratio,
            maximum_skin_area_ratio: skin_transition.maximum_area_ratio,
            required_minimum_skin_area_ratio,
            required_maximum_skin_area_ratio,
            remaining_skin_triangles_outside_area_ratio: skin_transition
                .triangles_outside_area_ratio,
            remaining_skin_triangles_outside_topology_guard: skin_transition
                .triangles_outside_topology_guard,
        });
    }
    let quality_gate = AnatomyQualityGateReceipt {
        passed: true,
        checks,
        distance_tolerance: DISTANCE_TOLERANCE,
        orientation_tolerance_degrees: ORIENTATION_TOLERANCE_DEGREES,
    };
    let total_movement = TotalAnatomyMovementReceipt {
        moved_vertex_count: result
            .iter()
            .copied()
            .zip(base.iter().copied())
            .filter(|(result, base)| (*result - *base).norm() > DISTANCE_TOLERANCE)
            .count(),
        newly_owned_internal_vertex_count: anatomy_owned
            .iter()
            .copied()
            .zip(skin_mask.iter().copied())
            .filter(|(owned, skin)| *owned && !*skin)
            .count(),
        overwritten_skin_boundary_vertex_count: overwritten_skin
            .iter()
            .filter(|overwritten| **overwritten)
            .count(),
    };
    let transforms = HeadAnatomyTransforms {
        left_eye: left_eye_transform,
        right_eye: right_eye_transform,
        left_eye_attached: left_eye_transform,
        right_eye_attached: right_eye_transform,
        mouth_assembly: mouth_local.transform,
        upper_jaw: mouth_local.transform,
        lower_jaw: mouth_local.transform,
        tongue: mouth_local.transform,
        inner_mouth: mouth_local.transform,
        left_nostril: left_nostril_transform,
        right_nostril: right_nostril_transform,
    };
    let receipt = HeadAnatomyReceipt {
        component_counts: components.counts(),
        left_eye,
        right_eye,
        left_eye_attached,
        right_eye_attached,
        eye_pair,
        mouth_assembly,
        mouth_components,
        left_nostril,
        right_nostril,
        nostril_pair,
        skin_transition,
        quality_gate,
        total_movement,
    };
    Ok(PropagatedHeadAnatomy {
        vertices: result,
        components,
        transition_collar,
        transforms,
        receipt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn transition_composition_is_order_independent_and_protects_the_closest_collar() {
        let baseline = vec![
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(20.0, 0.0, 0.0),
            Vec3::new(30.0, 0.0, 0.0),
            Vec3::new(40.0, 0.0, 0.0),
        ];
        let first = vec![
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            baseline[3],
        ];
        let second = vec![
            Vec3::new(9.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
            baseline[2],
            baseline[3],
        ];
        let third = vec![
            baseline[0],
            Vec3::new(6.0, 0.0, 0.0),
            baseline[2],
            baseline[3],
        ];
        let proposals: [(&[usize], &[usize], &[Vec3]); 3] = [
            (&[0, 1, 2], &[1, 2, 3], &first),
            (&[0, 1], &[3, 2], &second),
            (&[1], &[2], &third),
        ];
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let ordered = order.map(|index| proposals[index]);
            let composed = compose_transition_proposals(&baseline, &ordered);
            assert_eq!(composed.collar, vec![0]);
            assert_eq!(composed.repair_mask, vec![false, true, true, false]);
            assert!((composed.vertices[0].x - 1.0).abs() <= 1.0e-12);
            assert!((composed.vertices[1].x - 3.0).abs() <= 1.0e-12);
            assert!((composed.vertices[2].x - 2.0).abs() <= 1.0e-12);
            assert_eq!(composed.vertices[3], baseline[3]);
        }
    }

    #[test]
    fn transition_retry_stops_at_the_first_pass_and_returns_the_last_failure() {
        let mut visited = Vec::new();
        let selected = first_passing_or_last(&[4, 8, 12], |rings| {
            visited.push(rings);
            Ok::<_, ()>((rings, rings == 8))
        })
        .unwrap();
        assert_eq!(selected, Some(8));
        assert_eq!(visited, vec![4, 8]);

        visited.clear();
        let selected = first_passing_or_last(&[4, 8, 12], |rings| {
            visited.push(rings);
            Ok::<_, ()>((rings, false))
        })
        .unwrap();
        assert_eq!(selected, Some(12));
        assert_eq!(visited, vec![4, 8, 12]);
    }

    #[test]
    fn topology_guard_rejects_nonfinite_and_out_of_range_configuration() {
        let geometry = DazGeometry::new(
            "guard".into(),
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![vec![0, 1, 2]],
            crate::formats::GroupTable {
                indices: vec![0],
                names: vec!["head".into()],
            },
            crate::formats::GroupTable {
                indices: vec![0],
                names: vec!["Face".into()],
            },
            json!({}),
        )
        .unwrap();
        for (orientation, minimum_area, maximum_area) in [
            (f64::NAN, 0.08, 4.0),
            (-1.01, 0.08, 4.0),
            (1.01, 0.08, 4.0),
            (0.03, f64::NAN, 4.0),
            (0.03, 0.0, 4.0),
            (0.03, 4.0, 0.08),
        ] {
            assert_eq!(
                propagate_head_anatomy_auto_with_topology_guard(
                    &geometry,
                    &[],
                    orientation,
                    minimum_area,
                    maximum_area,
                ),
                Err(AnatomyError::InvalidTransitionConfiguration)
            );
        }
    }
}
