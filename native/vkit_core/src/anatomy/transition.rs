use std::collections::{BTreeMap, BTreeSet};

use crate::formats::{DazGeometry, HEAD_SKIN_MATERIALS};
use crate::math::{SimilarityTransform, Vec3};

use super::AnatomyError;

pub const SKIN_TRANSITION_MIN_ORIENTATION_COSINE: f64 = 0.02;

#[derive(Clone, Debug, PartialEq)]
pub struct SkinTransitionComponentReceipt {
    pub hard_skin_vertex_count: usize,

    pub transition_vertex_count: usize,
    pub rings: usize,

    pub ring_weights: Vec<f64>,

    pub maximum_transition_from_scan_fit: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransitionBlend {
    pub indices: Vec<usize>,
    pub ring_numbers: Vec<usize>,
    pub receipt: SkinTransitionComponentReceipt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransitionRepairReceipt {
    pub attempted: bool,
    pub initial_flipped_skin_triangles: usize,
    pub final_flipped_skin_triangles: usize,
    pub repaired_flipped_skin_triangles: usize,
    pub minimum_orientation_cosine_required: f64,
    pub initial_minimum_orientation_cosine: f64,
    pub final_minimum_orientation_cosine: f64,
    pub initial_triangles_below_orientation_margin: usize,
    pub final_triangles_below_orientation_margin: usize,
    pub adjusted_transition_vertex_count: usize,
    pub iterations: usize,
    pub minimum_retained_transition_fraction: f64,
    pub maximum_retraction_toward_scan_fit: f64,
    pub topology_preserved: bool,
    pub orientation_margin_preserved: bool,
    pub protected_vertices_changed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkinTransitionReceipt {
    pub mouth: SkinTransitionComponentReceipt,
    pub left_nostril: SkinTransitionComponentReceipt,
    pub right_nostril: SkinTransitionComponentReceipt,
    pub original_flipped_skin_triangles: usize,
    pub pre_repair_flipped_skin_triangles: usize,
    pub final_flipped_skin_triangles: usize,
    pub new_flipped_skin_triangles: usize,
    pub repair: TransitionRepairReceipt,
    pub topology_preserved: bool,
    pub orientation_margin_preserved: bool,
    pub minimum_area_ratio: f64,
    pub maximum_area_ratio: f64,
    pub triangles_outside_area_ratio: usize,
    pub area_ratio_preserved: bool,
    pub triangles_outside_topology_guard: usize,
    pub topology_guard_preserved: bool,
}

fn cross(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(
        left.y * right.z - left.z * right.y,
        left.z * right.x - left.x * right.z,
        left.x * right.y - left.y * right.x,
    )
}

fn validate_triangle_indices(
    triangles: &[[usize; 3]],
    vertex_count: usize,
) -> Result<(), AnatomyError> {
    for (triangle_id, triangle) in triangles.iter().enumerate() {
        if let Some(&index) = triangle.iter().find(|&&index| index >= vertex_count) {
            return Err(AnatomyError::TriangleIndexOutOfRange {
                triangle: triangle_id,
                index,
                vertex_count,
            });
        }
    }
    Ok(())
}

pub fn triangulated_skin_faces(geometry: &DazGeometry) -> Result<Vec<[usize; 3]>, AnatomyError> {
    let mask = geometry.face_mask_for_materials(HEAD_SKIN_MATERIALS.iter().copied());
    let mut triangles = Vec::new();
    for (face_id, face) in geometry.faces.iter().enumerate() {
        if !mask[face_id] {
            continue;
        }
        match *face.as_slice() {
            [a, b, c] => triangles.push([a as usize, b as usize, c as usize]),
            [a, b, c, d] => {
                triangles.push([a as usize, b as usize, c as usize]);
                triangles.push([a as usize, c as usize, d as usize]);
            }
            _ => {
                return Err(AnatomyError::UnsupportedSkinPolygon {
                    face: face_id,
                    corners: face.len(),
                });
            }
        }
    }
    validate_triangle_indices(&triangles, geometry.vertices.len())?;
    Ok(triangles)
}

pub fn skin_adjacency(
    vertex_count: usize,
    triangles: &[[usize; 3]],
) -> Result<Vec<Vec<usize>>, AnatomyError> {
    validate_triangle_indices(triangles, vertex_count)?;
    let mut adjacency = vec![BTreeSet::new(); vertex_count];
    for &[a, b, c] in triangles {
        adjacency[a].extend([b, c]);
        adjacency[b].extend([a, c]);
        adjacency[c].extend([a, b]);
    }
    Ok(adjacency
        .into_iter()
        .map(|neighbors| neighbors.into_iter().collect())
        .collect())
}

fn flipped_triangle_mask(
    base: &[Vec3],
    candidate: &[Vec3],
    triangles: &[[usize; 3]],
) -> Result<Vec<bool>, AnatomyError> {
    if base.len() != candidate.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: base.len(),
            fitted: candidate.len(),
        });
    }
    validate_triangle_indices(triangles, base.len())?;
    Ok(triangles
        .iter()
        .map(|&[a, b, c]| {
            let base_normal = cross(base[b] - base[a], base[c] - base[a]);
            let candidate_normal = cross(candidate[b] - candidate[a], candidate[c] - candidate[a]);
            base_normal.dot(candidate_normal) <= 0.0 || candidate_normal.norm() <= 1.0e-10
        })
        .collect())
}

pub fn flipped_triangle_count(
    base: &[Vec3],
    candidate: &[Vec3],
    triangles: &[[usize; 3]],
) -> Result<usize, AnatomyError> {
    Ok(flipped_triangle_mask(base, candidate, triangles)?
        .into_iter()
        .filter(|flipped| *flipped)
        .count())
}

pub fn triangle_orientation_cosines(
    base: &[Vec3],
    candidate: &[Vec3],
    triangles: &[[usize; 3]],
) -> Result<Vec<f64>, AnatomyError> {
    if base.len() != candidate.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: base.len(),
            fitted: candidate.len(),
        });
    }
    validate_triangle_indices(triangles, base.len())?;
    Ok(triangles
        .iter()
        .map(|&[a, b, c]| {
            let base_normal = cross(base[b] - base[a], base[c] - base[a]);
            let candidate_normal = cross(candidate[b] - candidate[a], candidate[c] - candidate[a]);
            base_normal.dot(candidate_normal)
                / (base_normal.norm() * candidate_normal.norm()).max(1.0e-20)
        })
        .collect())
}

pub fn triangle_area_ratios(
    base: &[Vec3],
    candidate: &[Vec3],
    triangles: &[[usize; 3]],
) -> Result<Vec<f64>, AnatomyError> {
    if base.len() != candidate.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: base.len(),
            fitted: candidate.len(),
        });
    }
    validate_triangle_indices(triangles, base.len())?;
    Ok(triangles
        .iter()
        .map(|&[a, b, c]| {
            let base_area = cross(base[b] - base[a], base[c] - base[a]).norm();
            let candidate_area =
                cross(candidate[b] - candidate[a], candidate[c] - candidate[a]).norm();
            candidate_area / base_area.max(1.0e-20)
        })
        .collect())
}

fn unsafe_triangle_mask(
    base: &[Vec3],
    candidate: &[Vec3],
    triangles: &[[usize; 3]],
    minimum_orientation_cosine: f64,
) -> Result<Vec<bool>, AnatomyError> {
    let flipped = flipped_triangle_mask(base, candidate, triangles)?;
    let orientation = triangle_orientation_cosines(base, candidate, triangles)?;
    Ok(flipped
        .into_iter()
        .zip(orientation)
        .map(|(flipped, cosine)| flipped || cosine < minimum_orientation_cosine)
        .collect())
}

fn guarded_unsafe_triangle_mask(
    base: &[Vec3],
    candidate: &[Vec3],
    triangles: &[[usize; 3]],
    minimum_orientation_cosine: f64,
    area_ratio_bounds: Option<(f64, f64)>,
) -> Result<Vec<bool>, AnatomyError> {
    let orientation_unsafe =
        unsafe_triangle_mask(base, candidate, triangles, minimum_orientation_cosine)?;
    let Some((minimum_area_ratio, maximum_area_ratio)) = area_ratio_bounds else {
        return Ok(orientation_unsafe);
    };
    let area_ratios = triangle_area_ratios(base, candidate, triangles)?;
    Ok(orientation_unsafe
        .into_iter()
        .zip(area_ratios)
        .map(|(unsafe_orientation, ratio)| {
            unsafe_orientation || ratio < minimum_area_ratio || ratio > maximum_area_ratio
        })
        .collect())
}

pub struct SkinTransitionPass<'a> {
    pub base: &'a [Vec3],
    pub original_fitted: &'a [Vec3],
    pub result: &'a mut [Vec3],
}

pub fn reconcile_skin_neighborhood(
    pass: SkinTransitionPass<'_>,
    adjacency: &[Vec<usize>],
    hard_indices: &[usize],
    protected_mask: &[bool],
    transform: SimilarityTransform,
    rings: usize,
) -> Result<TransitionBlend, AnatomyError> {
    let SkinTransitionPass {
        base,
        original_fitted,
        result,
    } = pass;
    if base.len() != original_fitted.len() || base.len() != result.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: base.len(),
            fitted: original_fitted.len().min(result.len()),
        });
    }
    if adjacency.len() != base.len() {
        return Err(AnatomyError::AdjacencyLengthMismatch {
            expected: base.len(),
            actual: adjacency.len(),
        });
    }
    if protected_mask.len() != base.len() {
        return Err(AnatomyError::MaskLengthMismatch {
            name: "protected",
            expected: base.len(),
            actual: protected_mask.len(),
        });
    }
    if let Some(&index) = hard_indices.iter().find(|&&index| index >= base.len()) {
        return Err(AnatomyError::IndexOutOfRange {
            index,
            vertex_count: base.len(),
        });
    }

    let hard: BTreeSet<_> = hard_indices.iter().copied().collect();
    let ring_weights: Vec<_> = (1..=rings)
        .map(|ring| (rings - ring + 1) as f64 / rings.max(1) as f64)
        .collect();
    if hard.is_empty() || rings == 0 {
        return Ok(TransitionBlend {
            indices: Vec::new(),
            ring_numbers: Vec::new(),
            receipt: SkinTransitionComponentReceipt {
                hard_skin_vertex_count: hard.len(),
                transition_vertex_count: 0,
                rings,
                ring_weights,
                maximum_transition_from_scan_fit: 0.0,
            },
        });
    }

    let mut visited = hard.clone();
    let mut frontier = hard;
    let mut transition = BTreeMap::new();
    for ring in 1..=rings {
        let mut next_frontier = BTreeSet::new();
        for &index in &frontier {
            next_frontier.extend(adjacency[index].iter().copied());
        }
        next_frontier.retain(|index| !visited.contains(index));
        visited.extend(next_frontier.iter().copied());
        for &index in &next_frontier {
            if !protected_mask[index] {
                transition.insert(index, ring);
            }
        }
        frontier = next_frontier;
        if frontier.is_empty() {
            break;
        }
    }

    let indices: Vec<_> = transition.keys().copied().collect();
    let ring_numbers: Vec<_> = indices.iter().map(|index| transition[index]).collect();
    let mut maximum_transition_from_scan_fit: f64 = 0.0;
    for (&index, &ring) in indices.iter().zip(&ring_numbers) {
        let weight = (rings - ring + 1) as f64 / rings as f64;
        let desired = transform.apply(base[index]);
        let scan_fit = original_fitted[index];
        let blended = scan_fit + (desired - scan_fit) * weight;
        maximum_transition_from_scan_fit =
            maximum_transition_from_scan_fit.max((blended - scan_fit).norm());
        result[index] = blended;
    }

    Ok(TransitionBlend {
        receipt: SkinTransitionComponentReceipt {
            hard_skin_vertex_count: hard_indices.iter().copied().collect::<BTreeSet<_>>().len(),
            transition_vertex_count: indices.len(),
            rings,
            ring_weights,
            maximum_transition_from_scan_fit,
        },
        indices,
        ring_numbers,
    })
}

pub fn repair_skin_transition_topology(
    pass: SkinTransitionPass<'_>,
    triangles: &[[usize; 3]],
    transition_mask: &[bool],
    protected_mask: &[bool],
    max_iterations: usize,
    minimum_orientation_cosine: f64,
    area_ratio_bounds: Option<(f64, f64)>,
) -> Result<TransitionRepairReceipt, AnatomyError> {
    let SkinTransitionPass {
        base,
        original_fitted,
        result,
    } = pass;
    if base.len() != original_fitted.len() || base.len() != result.len() {
        return Err(AnatomyError::VertexCountMismatch {
            base: base.len(),
            fitted: original_fitted.len().min(result.len()),
        });
    }
    for (name, mask) in [
        ("transition", transition_mask),
        ("protected", protected_mask),
    ] {
        if mask.len() != base.len() {
            return Err(AnatomyError::MaskLengthMismatch {
                name,
                expected: base.len(),
                actual: mask.len(),
            });
        }
    }
    if !minimum_orientation_cosine.is_finite()
        || !(-1.0..=1.0).contains(&minimum_orientation_cosine)
        || area_ratio_bounds.is_some_and(|(minimum, maximum)| {
            !minimum.is_finite() || !maximum.is_finite() || minimum <= 0.0 || minimum > maximum
        })
    {
        return Err(AnatomyError::InvalidTransitionConfiguration);
    }
    validate_triangle_indices(triangles, base.len())?;

    let desired = result.to_vec();
    let displacement: Vec<_> = desired
        .iter()
        .copied()
        .zip(original_fitted.iter().copied())
        .map(|(desired, original)| desired - original)
        .collect();
    let movable: Vec<_> = (0..base.len())
        .map(|index| {
            transition_mask[index] && !protected_mask[index] && displacement[index].norm() > 1.0e-12
        })
        .collect();
    let protected_before: Vec<_> = protected_mask
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, protected)| protected.then_some((index, result[index])))
        .collect();
    let initial_flipped_skin_triangles = flipped_triangle_count(base, result, triangles)?;
    let initial_orientation = triangle_orientation_cosines(base, result, triangles)?;
    let initial_orientation_unsafe =
        unsafe_triangle_mask(base, result, triangles, minimum_orientation_cosine)?;
    let initial_triangles_below_orientation_margin = initial_orientation_unsafe
        .iter()
        .filter(|unsafe_triangle| **unsafe_triangle)
        .count();
    let initial_guard_unsafe = guarded_unsafe_triangle_mask(
        base,
        result,
        triangles,
        minimum_orientation_cosine,
        area_ratio_bounds,
    )?;
    let initial_guard_unsafe_count = initial_guard_unsafe
        .iter()
        .filter(|unsafe_triangle| **unsafe_triangle)
        .count();

    let mut retained = vec![1.0; base.len()];
    let mut adjusted = vec![false; base.len()];
    let mut iterations = 0usize;
    for pass in 1..=max_iterations {
        let unsafe_triangles = guarded_unsafe_triangle_mask(
            base,
            result,
            triangles,
            minimum_orientation_cosine,
            area_ratio_bounds,
        )?;
        if unsafe_triangles
            .iter()
            .all(|unsafe_triangle| !unsafe_triangle)
        {
            break;
        }
        iterations = pass;
        let mut candidates = BTreeSet::new();
        for (triangle, unsafe_triangle) in triangles.iter().zip(unsafe_triangles) {
            if unsafe_triangle {
                candidates.extend(triangle.iter().copied().filter(|&index| movable[index]));
            }
        }
        if candidates.is_empty() {
            break;
        }
        for index in candidates {
            retained[index] *= 0.5;
            result[index] = original_fitted[index] + displacement[index] * retained[index];
            adjusted[index] = true;
        }
    }

    let final_flipped_skin_triangles = flipped_triangle_count(base, result, triangles)?;
    let final_orientation = triangle_orientation_cosines(base, result, triangles)?;
    let final_orientation_unsafe =
        unsafe_triangle_mask(base, result, triangles, minimum_orientation_cosine)?;
    let final_triangles_below_orientation_margin = final_orientation_unsafe
        .iter()
        .filter(|unsafe_triangle| **unsafe_triangle)
        .count();
    let adjusted_indices: Vec<_> = adjusted
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, adjusted)| adjusted.then_some(index))
        .collect();
    let minimum_retained_transition_fraction = adjusted_indices
        .iter()
        .map(|&index| retained[index])
        .reduce(f64::min)
        .unwrap_or(1.0);
    let maximum_retraction_toward_scan_fit = desired
        .iter()
        .copied()
        .zip(result.iter().copied())
        .map(|(desired, result)| (desired - result).norm())
        .reduce(f64::max)
        .unwrap_or(0.0);
    let protected_vertices_changed = protected_before
        .iter()
        .any(|&(index, before)| result[index] != before);

    Ok(TransitionRepairReceipt {
        attempted: initial_guard_unsafe_count != 0,
        initial_flipped_skin_triangles,
        final_flipped_skin_triangles,
        repaired_flipped_skin_triangles: initial_flipped_skin_triangles
            .saturating_sub(final_flipped_skin_triangles),
        minimum_orientation_cosine_required: minimum_orientation_cosine,
        initial_minimum_orientation_cosine: initial_orientation
            .into_iter()
            .reduce(f64::min)
            .unwrap_or(1.0),
        final_minimum_orientation_cosine: final_orientation
            .into_iter()
            .reduce(f64::min)
            .unwrap_or(1.0),
        initial_triangles_below_orientation_margin,
        final_triangles_below_orientation_margin,
        adjusted_transition_vertex_count: adjusted_indices.len(),
        iterations,
        minimum_retained_transition_fraction,
        maximum_retraction_toward_scan_fit,
        topology_preserved: final_flipped_skin_triangles == 0,
        orientation_margin_preserved: final_triangles_below_orientation_margin == 0,
        protected_vertices_changed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backtracking_repairs_only_the_transition_vertex() {
        let base = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
        ];
        let triangles = vec![[0, 1, 2], [0, 2, 3]];
        let original = base.clone();
        let mut candidate = base.clone();
        candidate[2].y = -1.0;
        let receipt = repair_skin_transition_topology(
            SkinTransitionPass {
                base: &base,
                original_fitted: &original,
                result: &mut candidate,
            },
            &triangles,
            &[false, false, true, false],
            &[false; 4],
            32,
            SKIN_TRANSITION_MIN_ORIENTATION_COSINE,
            None,
        )
        .unwrap();
        assert_eq!(receipt.initial_flipped_skin_triangles, 2);
        assert_eq!(receipt.final_flipped_skin_triangles, 0);
        assert_eq!(receipt.adjusted_transition_vertex_count, 1);
        assert!(receipt.minimum_retained_transition_fraction < 0.5);
        assert!(receipt.orientation_margin_preserved);
        assert_eq!(candidate[0], base[0]);
        assert_eq!(candidate[1], base[1]);
        assert_eq!(candidate[3], base[3]);
    }

    #[test]
    fn protected_hard_anatomy_is_never_retracted() {
        let base = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let original = base.clone();
        let mut candidate = base.clone();
        candidate[2].y = -1.0;
        let before = candidate.clone();
        let receipt = repair_skin_transition_topology(
            SkinTransitionPass {
                base: &base,
                original_fitted: &original,
                result: &mut candidate,
            },
            &[[0, 1, 2]],
            &[false, false, true],
            &[false, false, true],
            32,
            SKIN_TRANSITION_MIN_ORIENTATION_COSINE,
            None,
        )
        .unwrap();
        assert_eq!(candidate, before);
        assert_eq!(receipt.adjusted_transition_vertex_count, 0);
        assert_eq!(receipt.final_flipped_skin_triangles, 1);
        assert!(!receipt.topology_preserved);
        assert!(!receipt.protected_vertices_changed);
    }

    #[test]
    fn backtracking_repairs_an_isolated_area_ratio_outlier() {
        let base = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ];
        let original = base.clone();
        let mut candidate = base.clone();
        candidate[2].y = 5.0;

        let receipt = repair_skin_transition_topology(
            SkinTransitionPass {
                base: &base,
                original_fitted: &original,
                result: &mut candidate,
            },
            &[[0, 1, 2]],
            &[false, false, true],
            &[false; 3],
            32,
            SKIN_TRANSITION_MIN_ORIENTATION_COSINE,
            Some((0.08, 4.0)),
        )
        .unwrap();

        assert!(receipt.attempted);
        assert_eq!(receipt.initial_triangles_below_orientation_margin, 0);
        assert_eq!(receipt.final_triangles_below_orientation_margin, 0);
        assert_eq!(receipt.adjusted_transition_vertex_count, 1);
        let ratio = triangle_area_ratios(&base, &candidate, &[[0, 1, 2]]).unwrap()[0];
        assert!((0.08..=4.0).contains(&ratio), "unrepaired ratio: {ratio}");
    }
}
