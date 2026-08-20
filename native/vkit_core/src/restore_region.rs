use std::collections::HashMap;

use crate::formats::OrderedObjMesh;

pub const RESTORE_FULL_CM: f64 = 1.0;

pub const RESTORE_SPAN_CM: f64 = 8.0;

#[must_use]
pub fn neck_ear_restore_weights(template: &OrderedObjMesh) -> Vec<f64> {
    let mut by_material: HashMap<&str, Vec<usize>> = HashMap::new();
    for face in &template.faces {
        if let Some(material) = face.material.as_deref() {
            let bucket = by_material.entry(material).or_default();
            bucket.extend(face.vertex_indices.iter().map(|&index| index as usize));
        }
    }
    for bucket in by_material.values_mut() {
        bucket.sort_unstable();
        bucket.dedup();
    }
    let (Some(ears), Some(neck), Some(head)) = (
        by_material.get("Ears"),
        by_material.get("Neck"),
        by_material.get("Head"),
    ) else {
        return Vec::new();
    };
    let seed = seed_from_regions(ears, neck, head, &template.vertices);
    if seed.is_empty() {
        return Vec::new();
    }
    let edges = template.faces.iter().flat_map(|face| {
        let count = face.vertex_indices.len();
        (0..count).map(move |corner| {
            (
                face.vertex_indices[corner] as usize,
                face.vertex_indices[(corner + 1) % count] as usize,
            )
        })
    });
    geodesic_keep_weights(&template.vertices, edges, &seed)
}

fn seed_from_regions(
    ears: &[usize],
    neck: &[usize],
    head: &[usize],
    vertices: &[[f64; 3]],
) -> Vec<usize> {
    if ears.is_empty() || neck.is_empty() || head.is_empty() {
        return Vec::new();
    }
    #[expect(clippy::cast_precision_loss, reason = "vertex counts")]
    let ear_z = ears.iter().map(|&vertex| vertices[vertex][2]).sum::<f64>() / ears.len() as f64;
    let (mut neck_y_low, mut neck_y_high) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut neck_z_sum = 0.0;
    for &vertex in neck {
        neck_y_low = neck_y_low.min(vertices[vertex][1]);
        neck_y_high = neck_y_high.max(vertices[vertex][1]);
        neck_z_sum += vertices[vertex][2];
    }
    #[expect(clippy::cast_precision_loss, reason = "vertex counts")]
    let neck_z_mid = neck_z_sum / neck.len() as f64;
    let neck_y_mid = f64::midpoint(neck_y_low, neck_y_high);

    let mut seed = ears.to_vec();
    seed.extend(
        neck.iter().copied().filter(|&vertex| {
            vertices[vertex][2] <= neck_z_mid || vertices[vertex][1] <= neck_y_mid
        }),
    );
    seed.extend(
        head.iter()
            .copied()
            .filter(|&vertex| vertices[vertex][2] <= ear_z),
    );
    seed.sort_unstable();
    seed.dedup();
    seed
}

pub fn geodesic_keep_weights(
    vertices: &[[f64; 3]],
    edges: impl Iterator<Item = (usize, usize)>,
    seed: &[usize],
) -> Vec<f64> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let mut adjacency: Vec<Vec<(u32, f64)>> = vec![Vec::new(); vertices.len()];
    for (first, second) in edges {
        if first >= vertices.len() || second >= vertices.len() {
            continue;
        }
        let length = {
            let (a, b) = (vertices[first], vertices[second]);
            let delta = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            delta[2]
                .mul_add(delta[2], delta[1].mul_add(delta[1], delta[0] * delta[0]))
                .sqrt()
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "vertex ids fit u32 by format"
        )]
        {
            adjacency[first].push((second as u32, length));
            adjacency[second].push((first as u32, length));
        }
    }
    let mut distances = vec![f64::INFINITY; vertices.len()];
    let mut heap = BinaryHeap::new();
    for &vertex in seed {
        if vertex < distances.len() {
            distances[vertex] = 0.0;
            heap.push(Reverse((0_u64, vertex)));
        }
    }
    while let Some(Reverse((bits, vertex))) = heap.pop() {
        let distance = f64::from_bits(bits);
        if distance > distances[vertex] {
            continue;
        }
        for &(neighbor, length) in &adjacency[vertex] {
            let candidate = distance + length;
            if candidate < distances[neighbor as usize] {
                distances[neighbor as usize] = candidate;
                heap.push(Reverse((candidate.to_bits(), neighbor as usize)));
            }
        }
    }
    distances
        .into_iter()
        .map(|distance| {
            let t = ((distance - RESTORE_FULL_CM) / RESTORE_SPAN_CM).clamp(0.0, 1.0);
            (2.0 * t).mul_add(t * t, -(3.0 * t * t)) + 1.0
        })
        .collect()
}

#[must_use]
pub fn blend_toward_base(base: &[[f64; 3]], fitted: &[[f64; 3]], weights: &[f64]) -> Vec<[f64; 3]> {
    fitted
        .iter()
        .zip(base)
        .zip(weights)
        .map(|((&fit, &rest), &weight)| {
            if weight <= 0.0 {
                fit
            } else if weight >= 1.0 {
                rest
            } else {
                [0, 1, 2].map(|axis| (rest[axis] - fit[axis]).mul_add(weight, fit[axis]))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::ObjFace;

    fn toy_template() -> OrderedObjMesh {
        let vertices = vec![
            [0.0, 0.0, 2.0],
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 8.0],
            [0.0, 0.0, 10.0],
            [0.0, 8.0, 1.0],
            [0.0, 8.0, -3.0],
            [0.0, 8.0, -7.0],
            [0.0, 8.0, -11.0],
            [0.0, -6.0, 2.0],
            [0.0, -2.0, 2.0],
            [0.0, -6.0, -2.0],
            [0.0, -2.0, -2.0],
            [6.0, 4.0, 0.0],
            [-6.0, 4.0, 0.0],
        ];
        let face = |indices: &[u32], material: &str| ObjFace {
            vertex_indices: indices.to_vec(),
            group: Some(material.to_owned()),
            material: Some(material.to_owned()),
        };
        OrderedObjMesh {
            vertices,
            faces: vec![
                face(&[0, 1, 2], "Face"),
                face(&[2, 3, 0], "Face"),
                face(&[4, 5, 6], "Head"),
                face(&[6, 7, 4], "Head"),
                face(&[8, 9, 10], "Neck"),
                face(&[10, 11, 8], "Neck"),
                face(&[12, 13, 4], "Ears"),
                face(&[0, 4, 12], "Face"),
                face(&[0, 8, 9], "Face"),
            ],
        }
    }

    #[test]
    fn the_seed_holds_the_base_exactly_and_only_spreads_outward() {
        let template = toy_template();
        let weights = neck_ear_restore_weights(&template);
        assert_eq!(weights.len(), template.vertices.len());

        assert_eq!(weights[12], 1.0);
        assert_eq!(weights[13], 1.0);
        assert_eq!(weights[6], 1.0, "back of the skull is held");
        assert_eq!(weights[7], 1.0);
        assert_eq!(weights[10], 1.0, "nape");
        assert_eq!(weights[8], 1.0, "lower front neck");
        assert!(
            weights[9] < 1.0,
            "upper front neck is the flexible band, got {}",
            weights[9]
        );
        assert!(
            weights[3] < 1.0e-9,
            "the nose tip keeps the fit, got {}",
            weights[3]
        );
        assert!(weights[0] >= weights[1] && weights[1] >= weights[2]);
    }

    #[test]
    fn a_template_without_the_named_materials_declines_rather_than_guessing() {
        let mut template = toy_template();
        for face in &mut template.faces {
            face.material = Some("Torso".to_owned());
        }
        assert!(neck_ear_restore_weights(&template).is_empty());
    }

    #[test]
    fn blending_returns_the_base_at_full_weight_and_the_fit_at_none() {
        let base = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [2.0, 2.0, 2.0]];
        let fitted = vec![[9.0, 9.0, 9.0], [9.0, 9.0, 9.0], [9.0, 9.0, 9.0]];
        let blended = blend_toward_base(&base, &fitted, &[1.0, 0.0, 0.5]);
        assert_eq!(blended[0], base[0], "weight 1 is the base bit-exactly");
        assert_eq!(blended[1], fitted[1], "weight 0 is the fit bit-exactly");
        assert_eq!(blended[2], [5.5, 5.5, 5.5]);
    }
}
