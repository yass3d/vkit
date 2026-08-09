//! The parts of a head a scan should never be allowed to move.
//!
//! Ears, nape and the back of the skull are almost never usable in a scan, so
//! a fit that moves them only makes work: the same restore brushwork over the
//! same fixed regions, every head. This computes, once per template, a
//! per-vertex weight — 1 keeps the G2 base exactly, 0 keeps the fit — with a
//! geodesic falloff that runs *outward only*: a seeded vertex sits at distance
//! zero and holds weight 1.0 exactly, so nothing ever bleeds inward past the
//! ears or the nape. That one-sidedness is the half of the bargain that must
//! never soften; the other half is that the falloff toward crown, forehead,
//! cheeks and chin is wide enough to read as skin, not a seam.

use std::collections::HashMap;

use crate::formats::OrderedObjMesh;

/// Geodesic distance inside which the base is kept in full, beyond the seed.
pub const RESTORE_FULL_CM: f64 = 1.0;

/// Width of the smoothstep from full base to full fit.
///
/// Chosen to carry the ear-to-cheekbone strip and the chin-to-throat band in
/// one soft gradient; narrower reads as a crease along the jaw.
pub const RESTORE_SPAN_CM: f64 = 8.0;

/// Per-vertex keep-the-base weights for the neck-and-ears restore.
///
/// The template must be the canonical G2 head export, which names its
/// materials; the regions are found by name where a name exists and cut
/// geometrically where one does not:
/// - the whole `Ears` material;
/// - `Neck`, except its upper front quarter — under the chin the face's own
///   likeness reaches down, so that band is left for the falloff to blend;
/// - `Head` behind the vertical line of the ears — the back of the skull —
///   leaving crown and forehead to take the fit softly.
///
/// Returns one weight per vertex of the template. An empty result means the
/// template does not carry the expected materials and the restore should be
/// skipped rather than guessed.
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

/// The vertices held at the base outright, before any falloff.
///
/// Canonical space: centimetres, Y up, +Z out of the face. Cuts are taken
/// from the template's own extents rather than absolute numbers, so a
/// template variant with a different stature still cuts in proportion.
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
    // The nape at any height, and the whole lower neck; the upper front —
    // throat to chin — is where the face's likeness still matters, so it is
    // left out of the seed and receives the falloff instead.
    seed.extend(
        neck.iter().copied().filter(|&vertex| {
            vertices[vertex][2] <= neck_z_mid || vertices[vertex][1] <= neck_y_mid
        }),
    );
    // The skull behind the vertical line of the ears.
    seed.extend(
        head.iter()
            .copied()
            .filter(|&vertex| vertices[vertex][2] <= ear_z),
    );
    seed.sort_unstable();
    seed.dedup();
    seed
}

/// Smoothstepped keep-the-base weights from geodesic distance to a seed set.
///
/// Every seed vertex is at distance zero and therefore weighs exactly 1.0 —
/// the falloff exists only outside the seed, never inside it.
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

/// One position per vertex: the base where the weight says base, the fit
/// where it says fit, and the straight line between elsewhere.
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

    /// A toy head: a strip of vertices along +Z (chin to nose) in "Face", a
    /// neck column spanning front and back, ears at the sides, and a skull
    /// strip along -Z in "Head". Canonical axes: Y up, +Z forward, cm.
    fn toy_template() -> OrderedObjMesh {
        // indices 0..4  : Face strip, z = 2..10 (front)
        // indices 4..8  : Head strip, z = 1..-11 (crown to back)
        // indices 8..12 : Neck column, (y,z) quadrants
        // indices 12..14: Ears at z = 0
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
                // stitch the islands so distances propagate
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

        // Ears: always seed, weight exactly 1 — the inward side never softens.
        assert_eq!(weights[12], 1.0);
        assert_eq!(weights[13], 1.0);
        // Skull behind the ear line: seed.
        assert_eq!(weights[6], 1.0, "back of the skull is held");
        assert_eq!(weights[7], 1.0);
        // Neck: nape and lower front are seed; the upper front — throat to
        // chin — is not, so the chin band can blend.
        assert_eq!(weights[10], 1.0, "nape");
        assert_eq!(weights[8], 1.0, "lower front neck");
        assert!(
            weights[9] < 1.0,
            "upper front neck is the flexible band, got {}",
            weights[9]
        );
        // The far face follows the fit outright.
        assert!(
            weights[3] < 1.0e-9,
            "the nose tip keeps the fit, got {}",
            weights[3]
        );
        // And the falloff decays monotonically along the face strip.
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
