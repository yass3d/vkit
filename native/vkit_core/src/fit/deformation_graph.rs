use std::collections::BTreeSet;

use nalgebra::{Matrix3, Vector3};

use crate::math::Vec3;

use super::BarycentricConstraint;

const INFLUENCE_COUNT: usize = 4;
const NEAREST_NODE_COUNT: usize = INFLUENCE_COUNT + 1;

const MINIMUM_ROTATION_SUPPORT_WEIGHT: f64 = 1.0e-12;

const MINIMUM_SINGULAR_VALUE_RATIO: f64 = 1.0e-6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeformationGraphSchedule {
    pub divisor: usize,
    pub minimum_nodes: usize,
    pub maximum_nodes: usize,
    pub graph_blend: f64,
}

#[derive(Clone, Copy, Debug)]
struct Influence {
    node: usize,
    weight: f64,
}

#[derive(Clone, Copy, Debug)]
struct LocalRigidTransform {
    rotation: Matrix3<f64>,
    translation: Vector3<f64>,
}

pub(crate) fn regularize_step(
    current: &[Vec3],
    raw_delta: &[Vec3],
    eligible: &[bool],
    seam: &[usize],
    anchor_weights: &[f64],
    strict_constraints: &[BarycentricConstraint],
    schedule: DeformationGraphSchedule,
) -> Vec<Vec3> {
    debug_assert_eq!(current.len(), raw_delta.len());
    debug_assert_eq!(current.len(), eligible.len());
    debug_assert_eq!(current.len(), anchor_weights.len());
    let seam = seam.iter().copied().collect::<BTreeSet<_>>();
    let candidates = eligible
        .iter()
        .enumerate()
        .filter_map(|(index, selected)| (*selected && !seam.contains(&index)).then_some(index))
        .collect::<Vec<_>>();
    if candidates.len() < INFLUENCE_COUNT {
        return raw_delta.to_vec();
    }

    let requested = (candidates.len() / schedule.divisor.max(1))
        .clamp(schedule.minimum_nodes, schedule.maximum_nodes)
        .min(candidates.len());
    let node_vertices = farthest_point_nodes(current, &candidates, requested);
    let influences = current
        .iter()
        .map(|point| nearest_node_influences(*point, current, &node_vertices))
        .collect::<Vec<_>>();
    let transforms =
        fit_local_rigid_transforms(current, raw_delta, eligible, &node_vertices, &influences);

    let mut constrained = vec![false; current.len()];
    for constraint in strict_constraints {
        for &vertex in &constraint.vertex_indices {
            constrained[vertex] = true;
        }
    }

    current
        .iter()
        .enumerate()
        .map(|(index, &point)| {
            if !eligible[index] || seam.contains(&index) || constrained[index] {
                return raw_delta[index];
            }
            let source = to_na(point);
            let projected = influences[index]
                .iter()
                .fold(Vector3::zeros(), |sum, influence| {
                    let transform = transforms[influence.node];
                    sum + (transform.rotation * source + transform.translation) * influence.weight
                });
            let graph_delta = from_na(projected - source);

            let anchor_attenuation = 1.0 / (1.0 + anchor_weights[index].max(0.0));
            let blend = (schedule.graph_blend * anchor_attenuation).clamp(0.0, 1.0);
            raw_delta[index] * (1.0 - blend) + graph_delta * blend
        })
        .collect()
}

fn farthest_point_nodes(vertices: &[Vec3], candidates: &[usize], count: usize) -> Vec<usize> {
    let centroid = candidates
        .iter()
        .fold(Vec3::ZERO, |sum, &index| sum + vertices[index])
        * (1.0 / candidates.len() as f64);
    let first = candidates
        .iter()
        .copied()
        .max_by(|&left, &right| {
            squared_distance(vertices[left], centroid)
                .total_cmp(&squared_distance(vertices[right], centroid))
                .then_with(|| right.cmp(&left))
        })
        .expect("candidate set is non-empty");
    let mut nodes = Vec::with_capacity(count);
    nodes.push(first);
    let mut nearest = candidates
        .iter()
        .map(|&index| squared_distance(vertices[index], vertices[first]))
        .collect::<Vec<_>>();
    while nodes.len() < count {
        let (slot, _) = nearest
            .iter()
            .enumerate()
            .max_by(|(left_index, left), (right_index, right)| {
                left.total_cmp(right)
                    .then_with(|| right_index.cmp(left_index))
            })
            .expect("candidate distances are non-empty");
        let node = candidates[slot];
        nodes.push(node);
        for (distance, &candidate) in nearest.iter_mut().zip(candidates) {
            *distance = distance.min(squared_distance(vertices[candidate], vertices[node]));
        }
    }
    nodes
}

fn nearest_nodes(
    point: Vec3,
    vertices: &[Vec3],
    nodes: &[usize],
) -> [(f64, usize); NEAREST_NODE_COUNT] {
    let mut nearest = [(f64::INFINITY, 0_usize); NEAREST_NODE_COUNT];
    for (node, &vertex) in nodes.iter().enumerate() {
        let candidate = (squared_distance(point, vertices[vertex]), node);
        let slot = nearest.iter().position(|&(distance, existing)| {
            candidate.0 < distance || (candidate.0 == distance && candidate.1 < existing)
        });
        if let Some(slot) = slot {
            nearest[slot..].rotate_right(1);
            nearest[slot] = candidate;
        }
    }
    nearest
}

fn nearest_node_influences(
    point: Vec3,
    vertices: &[Vec3],
    nodes: &[usize],
) -> [Influence; INFLUENCE_COUNT] {
    let nearest = nearest_nodes(point, vertices, nodes);
    if nearest[0].0 <= 1.0e-20 {
        return [
            Influence {
                node: nearest[0].1,
                weight: 1.0,
            },
            Influence {
                node: 0,
                weight: 0.0,
            },
            Influence {
                node: 0,
                weight: 0.0,
            },
            Influence {
                node: 0,
                weight: 0.0,
            },
        ];
    }
    let radius = nearest[INFLUENCE_COUNT].0.sqrt().max(1.0e-10);
    let mut raw = [0.0; INFLUENCE_COUNT];
    for (weight, (distance, _)) in raw
        .iter_mut()
        .zip(nearest[..INFLUENCE_COUNT].iter().copied())
    {
        let compact = (1.0 - distance.sqrt() / radius).max(0.0);
        *weight = compact * compact;
    }
    if raw.iter().sum::<f64>() <= 1.0e-20 {
        for (weight, (distance, _)) in raw
            .iter_mut()
            .zip(nearest[..INFLUENCE_COUNT].iter().copied())
        {
            *weight = 1.0 / distance.max(1.0e-20);
        }
    }
    let sum = raw.iter().sum::<f64>();
    std::array::from_fn(|index| Influence {
        node: nearest[index].1,
        weight: raw[index] / sum,
    })
}

fn fit_local_rigid_transforms(
    current: &[Vec3],
    raw_delta: &[Vec3],
    eligible: &[bool],
    nodes: &[usize],
    influences: &[[Influence; INFLUENCE_COUNT]],
) -> Vec<LocalRigidTransform> {
    let mut weights = vec![0.0; nodes.len()];
    let mut source_sums = vec![Vector3::zeros(); nodes.len()];
    let mut target_sums = vec![Vector3::zeros(); nodes.len()];
    for (index, node_influences) in influences.iter().enumerate() {
        if !eligible[index] {
            continue;
        }
        let source = to_na(current[index]);
        let target = to_na(current[index] + raw_delta[index]);
        for influence in node_influences {
            weights[influence.node] += influence.weight;
            source_sums[influence.node] += source * influence.weight;
            target_sums[influence.node] += target * influence.weight;
        }
    }
    let source_centroids = source_sums
        .iter()
        .zip(&weights)
        .map(|(sum, &weight)| *sum / weight.max(1.0e-20))
        .collect::<Vec<_>>();
    let target_centroids = target_sums
        .iter()
        .zip(&weights)
        .map(|(sum, &weight)| *sum / weight.max(1.0e-20))
        .collect::<Vec<_>>();
    let mut covariance = vec![Matrix3::zeros(); nodes.len()];
    for (index, node_influences) in influences.iter().enumerate() {
        if !eligible[index] {
            continue;
        }
        let source = to_na(current[index]);
        let target = to_na(current[index] + raw_delta[index]);
        for influence in node_influences {
            let source_offset = source - source_centroids[influence.node];
            let target_offset = target - target_centroids[influence.node];
            covariance[influence.node] +=
                source_offset * target_offset.transpose() * influence.weight;
        }
    }
    covariance
        .into_iter()
        .enumerate()
        .map(|(node, matrix)| {
            let translation_fallback = LocalRigidTransform {
                rotation: Matrix3::identity(),
                translation: target_centroids[node] - source_centroids[node],
            };
            if weights[node] <= MINIMUM_ROTATION_SUPPORT_WEIGHT {
                return translation_fallback;
            }
            let svd = matrix.svd(true, true);
            let (Some(u), Some(v_t)) = (svd.u, svd.v_t) else {
                return translation_fallback;
            };

            let mut largest = 0.0_f64;
            let mut second = 0.0_f64;
            for value in svd.singular_values.iter().copied() {
                if value > largest {
                    second = largest;
                    largest = value;
                } else if value > second {
                    second = value;
                }
            }
            if largest <= 0.0 || second < largest * MINIMUM_SINGULAR_VALUE_RATIO {
                return translation_fallback;
            }
            let mut v = v_t.transpose();
            let mut rotation = v * u.transpose();
            if rotation.determinant() < 0.0 {
                v.column_mut(2).scale_mut(-1.0);
                rotation = v * u.transpose();
            }
            LocalRigidTransform {
                rotation,
                translation: target_centroids[node] - rotation * source_centroids[node],
            }
        })
        .collect()
}

fn squared_distance(left: Vec3, right: Vec3) -> f64 {
    let delta = left - right;
    delta.dot(delta)
}

fn to_na(value: Vec3) -> Vector3<f64> {
    Vector3::new(value.x, value.y, value.z)
}

fn from_na(value: Vector3<f64>) -> Vec3 {
    Vec3::new(value.x, value.y, value.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schedule() -> DeformationGraphSchedule {
        DeformationGraphSchedule {
            divisor: 2,
            minimum_nodes: 4,
            maximum_nodes: 4,
            graph_blend: 1.0,
        }
    }

    #[test]
    fn local_rigid_graph_preserves_a_uniform_translation() {
        let current = (0..12)
            .map(|index| Vec3::new(index as f64, (index % 3) as f64, 0.25 * index as f64))
            .collect::<Vec<_>>();
        let translation = Vec3::new(0.4, -0.2, 0.1);
        let raw = vec![translation; current.len()];
        let result = regularize_step(
            &current,
            &raw,
            &vec![true; current.len()],
            &[],
            &vec![0.0; current.len()],
            &[],
            schedule(),
        );
        assert!(
            result
                .iter()
                .all(|value| (*value - translation).norm() < 1.0e-9)
        );
    }

    #[test]
    fn fifth_nearest_slot_is_the_exact_fifth_smallest_distance_under_permutations() {
        let query = Vec3::ZERO;

        let vertices = (0..8)
            .map(|index| Vec3::new((index + 1) as f64, 0.0, 0.0))
            .collect::<Vec<_>>();
        let base_nodes = (0..8).collect::<Vec<usize>>();
        let permutations: [Vec<usize>; 4] = [
            base_nodes.clone(),
            base_nodes.iter().rev().copied().collect(),
            vec![4, 0, 7, 2, 6, 1, 5, 3],
            vec![3, 5, 1, 6, 2, 7, 0, 4],
        ];
        for nodes in &permutations {
            let nearest = nearest_nodes(query, &vertices, nodes);
            let distances = nearest.map(|(distance, _)| distance);
            assert_eq!(
                distances,
                [1.0, 4.0, 9.0, 16.0, 25.0],
                "nodes={nodes:?} produced {distances:?}"
            );

            let vertex_ids = nearest.map(|(_, node)| nodes[node]);
            assert_eq!(vertex_ids, [0, 1, 2, 3, 4], "nodes={nodes:?}");
        }
    }

    #[test]
    fn nearest_influence_weights_are_permutation_invariant() {
        let query = Vec3::new(0.3, -0.2, 0.1);
        let vertices = (0..9)
            .map(|index| {
                Vec3::new(
                    (index % 3) as f64 * 1.7,
                    (index / 3) as f64 * 1.3,
                    0.25 * index as f64,
                )
            })
            .collect::<Vec<_>>();
        let base_nodes = (0..9).collect::<Vec<usize>>();
        let permuted = vec![8, 2, 5, 0, 7, 1, 4, 6, 3];
        let canonical = nearest_node_influences(query, &vertices, &base_nodes)
            .map(|influence| (base_nodes[influence.node], influence.weight));
        let shuffled = nearest_node_influences(query, &vertices, &permuted)
            .map(|influence| (permuted[influence.node], influence.weight));
        assert_eq!(canonical, shuffled);
        let weight_sum = canonical.iter().map(|(_, weight)| weight).sum::<f64>();
        assert!((weight_sum - 1.0).abs() <= 1.0e-12);
        assert!(canonical.iter().all(|(_, weight)| *weight > 0.0));
    }

    #[test]
    fn collinear_support_falls_back_to_pure_translation() {
        let current = (0..6)
            .map(|index| Vec3::new(index as f64, 0.0, 0.0))
            .collect::<Vec<_>>();
        let centroid = Vec3::new(2.5, 0.0, 0.0);
        let raw_delta = current
            .iter()
            .map(|point| {
                let offset = *point - centroid;
                let rotated = Vec3::new(-offset.y, offset.x, offset.z);
                centroid + rotated - *point
            })
            .collect::<Vec<_>>();
        let nodes = [0_usize];
        let uniform = [Influence {
            node: 0,
            weight: 0.25,
        }; INFLUENCE_COUNT];
        let influences = vec![uniform; current.len()];
        let transforms = fit_local_rigid_transforms(
            &current,
            &raw_delta,
            &vec![true; current.len()],
            &nodes,
            &influences,
        );
        assert_eq!(transforms.len(), 1);
        let transform = transforms[0];
        assert!(
            (transform.rotation - Matrix3::identity()).norm() <= 1.0e-12,
            "degenerate support produced rotation {:?}",
            transform.rotation
        );

        assert!(transform.translation.norm() <= 1.0e-9);
    }

    #[test]
    fn zero_weight_node_support_falls_back_to_pure_translation() {
        let current = (0..6)
            .map(|index| Vec3::new(index as f64, (index % 2) as f64, 0.0))
            .collect::<Vec<_>>();
        let raw_delta = vec![Vec3::new(0.2, 0.0, 0.0); current.len()];
        let nodes = [0_usize, 1];

        let concentrated = [
            Influence {
                node: 0,
                weight: 1.0,
            },
            Influence {
                node: 1,
                weight: 0.0,
            },
            Influence {
                node: 1,
                weight: 0.0,
            },
            Influence {
                node: 1,
                weight: 0.0,
            },
        ];
        let influences = vec![concentrated; current.len()];
        let transforms = fit_local_rigid_transforms(
            &current,
            &raw_delta,
            &vec![true; current.len()],
            &nodes,
            &influences,
        );
        assert!((transforms[1].rotation - Matrix3::identity()).norm() <= 1.0e-12);
    }

    #[test]
    fn seam_and_landmark_vertices_retain_the_guarded_raw_step() {
        let current = (0..8)
            .map(|index| Vec3::new(index as f64, (index % 2) as f64, 0.0))
            .collect::<Vec<_>>();
        let raw = (0..8)
            .map(|index| Vec3::new(0.1 * index as f64, 0.0, 0.0))
            .collect::<Vec<_>>();
        let constraint = BarycentricConstraint {
            vertex_indices: [1, 2, 3],
            barycentric: [0.2, 0.3, 0.5],
            target: Vec3::ZERO,
            effective_weight: 1.0,
        };
        let result = regularize_step(
            &current,
            &raw,
            &vec![true; current.len()],
            &[0],
            &vec![0.0; current.len()],
            &[constraint],
            schedule(),
        );
        for index in 0..=3 {
            assert_eq!(result[index], raw[index]);
        }
    }
}
