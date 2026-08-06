use super::{Result, VaMError};

const ENTRY_BYTES: usize = 16;
const MAX_DELTA_COUNT: usize = 1_000_000;

pub const VAM_SHARED_BODY_VERTEX_COUNT: u32 = 21_556;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SparseDelta {
    pub vertex_index: u32,
    pub delta_cm: [f64; 3],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum VmbVertexRouting {
    #[default]
    Full,
    Genitalia,
}

pub fn vmb_raw_entry_count(encoded: &[u8]) -> Result<usize> {
    if encoded.len() < 4 {
        return Err(VaMError::InvalidMorphBank(
            "VMB stream is shorter than its count field".to_owned(),
        ));
    }
    let count = i32::from_le_bytes(encoded[0..4].try_into().expect("four bytes"));
    if count < 0 || count as usize > MAX_DELTA_COUNT {
        return Err(VaMError::InvalidMorphBank(format!(
            "VMB declares invalid delta count {count}"
        )));
    }
    let count = count as usize;
    let expected = encoded_len(count)?;
    if encoded.len() != expected {
        return Err(VaMError::InvalidMorphBank(format!(
            "VMB contains {} bytes; count {count} requires {expected}",
            encoded.len()
        )));
    }
    Ok(count)
}

pub fn decode_vmb_daz_cm(encoded: &[u8]) -> Result<Vec<SparseDelta>> {
    let count = vmb_raw_entry_count(encoded)?;

    let mut result = Vec::with_capacity(count);
    for chunk in encoded[4..].chunks_exact(ENTRY_BYTES) {
        let vertex_index = u32::from_le_bytes(chunk[0..4].try_into().expect("four bytes"));
        let x = f32::from_le_bytes(chunk[4..8].try_into().expect("four bytes")) as f64;
        let y = f32::from_le_bytes(chunk[8..12].try_into().expect("four bytes")) as f64;
        let z = f32::from_le_bytes(chunk[12..16].try_into().expect("four bytes")) as f64;
        let delta_cm = [-x * 100.0, y * 100.0, z * 100.0];
        if !delta_cm.iter().all(|value| value.is_finite()) {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB vertex {vertex_index} contains a non-finite displacement"
            )));
        }
        result.push(SparseDelta {
            vertex_index,
            delta_cm,
        });
    }
    Ok(result)
}

pub fn decode_vmb_daz_cm_for_topology(
    encoded: &[u8],
    target_vertex_count: usize,
    routing: Option<VmbVertexRouting>,
) -> Result<Vec<SparseDelta>> {
    validate_vertex_count(target_vertex_count)?;
    let routing = routing.unwrap_or_default();
    let mut normalized = decode_vmb_daz_cm(encoded)?;

    for (entry_id, delta) in normalized.iter_mut().enumerate() {
        if !delta.delta_cm.iter().all(|value| value.is_finite()) {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB vertex {} contains a non-finite displacement",
                delta.vertex_index
            )));
        }
        if routing == VmbVertexRouting::Genitalia {
            delta.vertex_index = delta
                .vertex_index
                .checked_add(VAM_SHARED_BODY_VERTEX_COUNT)
                .ok_or_else(|| {
                    VaMError::InvalidMorphBank(format!(
                        "VMB genital entry {entry_id} overflows the u32 vertex index space"
                    ))
                })?;
        }
        if delta.vertex_index as usize >= target_vertex_count {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB entry {entry_id} references vertex {}, but only {target_vertex_count} vertices exist",
                delta.vertex_index
            )));
        }
    }

    normalized.sort_unstable_by_key(|delta| delta.vertex_index);
    for pair in normalized.windows(2) {
        if pair[0].vertex_index == pair[1].vertex_index {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB contains duplicate vertex index {}",
                pair[0].vertex_index
            )));
        }
    }
    normalized.retain(|delta| delta.delta_cm.iter().any(|value| *value != 0.0));
    Ok(normalized)
}

pub fn build_sparse_deltas_daz_cm(
    base_vertices_cm: &[[f64; 3]],
    target_vertices_cm: &[[f64; 3]],
    epsilon_cm: f64,
) -> Result<Vec<SparseDelta>> {
    if base_vertices_cm.len() != target_vertices_cm.len() {
        return Err(VaMError::InvalidMorphBank(format!(
            "VMB base mesh has {} vertices, but target mesh has {}",
            base_vertices_cm.len(),
            target_vertices_cm.len()
        )));
    }
    validate_vertex_count(base_vertices_cm.len())?;
    if !epsilon_cm.is_finite() || epsilon_cm < 0.0 {
        return Err(VaMError::InvalidMorphBank(format!(
            "VMB delta epsilon must be a finite non-negative value, got {epsilon_cm}"
        )));
    }

    let mut deltas = Vec::new();
    for (vertex_index, (base, target)) in
        base_vertices_cm.iter().zip(target_vertices_cm).enumerate()
    {
        if !base.iter().all(|value| value.is_finite()) {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB base vertex {vertex_index} contains a non-finite coordinate"
            )));
        }
        if !target.iter().all(|value| value.is_finite()) {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB target vertex {vertex_index} contains a non-finite coordinate"
            )));
        }

        let delta_cm = [
            target[0] - base[0],
            target[1] - base[1],
            target[2] - base[2],
        ];
        if !delta_cm.iter().all(|value| value.is_finite()) {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB vertex {vertex_index} displacement is not finite"
            )));
        }
        if delta_cm.iter().all(|value| value.abs() <= epsilon_cm) {
            continue;
        }
        deltas.push(SparseDelta {
            vertex_index: vertex_index as u32,
            delta_cm,
        });
    }
    Ok(deltas)
}

pub fn encode_vmb_daz_cm(deltas: &[SparseDelta], vertex_count: usize) -> Result<Vec<u8>> {
    validate_vertex_count(vertex_count)?;
    let encoded_len = encoded_len(deltas.len())?;

    let mut ordered = deltas.to_vec();
    ordered.sort_unstable_by_key(|delta| delta.vertex_index);

    let mut output = Vec::with_capacity(encoded_len);
    output.extend_from_slice(
        &i32::try_from(ordered.len())
            .map_err(|_| VaMError::InvalidMorphBank("VMB delta count exceeds i32".to_owned()))?
            .to_le_bytes(),
    );

    let mut previous_index = None;
    for (entry_id, delta) in ordered.iter().enumerate() {
        let vertex_index = delta.vertex_index as usize;
        if vertex_index >= vertex_count {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB entry {entry_id} references vertex {}, but only {vertex_count} vertices exist",
                delta.vertex_index
            )));
        }
        if previous_index == Some(delta.vertex_index) {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB contains duplicate vertex index {}",
                delta.vertex_index
            )));
        }
        if !delta.delta_cm.iter().all(|value| value.is_finite()) {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB vertex {} contains a non-finite displacement",
                delta.vertex_index
            )));
        }
        if delta.delta_cm.iter().all(|value| *value == 0.0) {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB vertex {} contains a zero displacement",
                delta.vertex_index
            )));
        }

        let metres = [
            -delta.delta_cm[0] / 100.0,
            delta.delta_cm[1] / 100.0,
            delta.delta_cm[2] / 100.0,
        ];
        let encoded_metres = metres.map(|value| value as f32);
        if !encoded_metres.iter().all(|value| value.is_finite())
            || encoded_metres.iter().all(|value| *value == 0.0)
            || metres
                .iter()
                .zip(encoded_metres)
                .any(|(source, encoded)| *source != 0.0 && encoded == 0.0)
        {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB vertex {} displacement cannot be represented as a finite nonzero f32 metre vector",
                delta.vertex_index
            )));
        }

        output.extend_from_slice(&delta.vertex_index.to_le_bytes());
        for coordinate in encoded_metres {
            output.extend_from_slice(&coordinate.to_le_bytes());
        }
        previous_index = Some(delta.vertex_index);
    }
    debug_assert_eq!(output.len(), encoded_len);
    Ok(output)
}

pub fn encode_vmb_daz_cm_for_topology(
    deltas: &[SparseDelta],
    target_vertex_count: usize,
    routing: Option<VmbVertexRouting>,
) -> Result<Vec<u8>> {
    let routing = routing.unwrap_or_default();
    if routing == VmbVertexRouting::Full {
        return encode_vmb_daz_cm(deltas, target_vertex_count);
    }

    validate_vertex_count(target_vertex_count)?;
    let shared_count = VAM_SHARED_BODY_VERTEX_COUNT as usize;
    let genital_vertex_count = target_vertex_count.checked_sub(shared_count).ok_or_else(|| {
        VaMError::InvalidMorphBank(format!(
            "VMB genital target has {target_vertex_count} vertices, fewer than the {shared_count} shared-body vertices"
        ))
    })?;
    if genital_vertex_count == 0 {
        return Err(VaMError::InvalidMorphBank(format!(
            "VMB genital target has no vertices after the {shared_count} shared-body vertices"
        )));
    }

    let mut local_deltas = Vec::with_capacity(deltas.len());
    for (entry_id, delta) in deltas.iter().enumerate() {
        if delta.vertex_index < VAM_SHARED_BODY_VERTEX_COUNT {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB genital entry {entry_id} references shared-body vertex {}; genital vertices begin at {VAM_SHARED_BODY_VERTEX_COUNT}",
                delta.vertex_index
            )));
        }
        if delta.vertex_index as usize >= target_vertex_count {
            return Err(VaMError::InvalidMorphBank(format!(
                "VMB genital entry {entry_id} references vertex {}, but only {target_vertex_count} vertices exist",
                delta.vertex_index
            )));
        }
        local_deltas.push(SparseDelta {
            vertex_index: delta.vertex_index - VAM_SHARED_BODY_VERTEX_COUNT,
            delta_cm: delta.delta_cm,
        });
    }

    encode_vmb_daz_cm(&local_deltas, genital_vertex_count)
}

fn validate_vertex_count(vertex_count: usize) -> Result<()> {
    if vertex_count == 0 {
        return Err(VaMError::InvalidMorphBank(
            "VMB target vertex count must be positive".to_owned(),
        ));
    }
    if u32::try_from(vertex_count - 1).is_err() {
        return Err(VaMError::InvalidMorphBank(format!(
            "VMB target vertex count {vertex_count} exceeds the u32 index space"
        )));
    }
    Ok(())
}

fn encoded_len(count: usize) -> Result<usize> {
    if count > MAX_DELTA_COUNT || i32::try_from(count).is_err() {
        return Err(VaMError::InvalidMorphBank(format!(
            "VMB delta count {count} exceeds the supported maximum {MAX_DELTA_COUNT}"
        )));
    }
    4_usize
        .checked_add(count.checked_mul(ENTRY_BYTES).ok_or_else(|| {
            VaMError::InvalidMorphBank("VMB delta byte count overflow".to_owned())
        })?)
        .ok_or_else(|| VaMError::InvalidMorphBank("VMB byte count overflow".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    fn raw_vmb(entries: &[(u32, [f32; 3])]) -> Vec<u8> {
        let mut bytes = (entries.len() as i32).to_le_bytes().to_vec();
        for (vertex_index, delta_metres) in entries {
            bytes.extend_from_slice(&vertex_index.to_le_bytes());
            for coordinate in delta_metres {
                bytes.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        bytes
    }

    #[test]
    fn decodes_axis_and_unit_contract() {
        let mut bytes = 1_i32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&7_u32.to_le_bytes());
        bytes.extend_from_slice(&0.25_f32.to_le_bytes());
        bytes.extend_from_slice(&(-0.5_f32).to_le_bytes());
        bytes.extend_from_slice(&1.5_f32.to_le_bytes());
        assert_eq!(
            decode_vmb_daz_cm(&bytes).unwrap(),
            vec![SparseDelta {
                vertex_index: 7,
                delta_cm: [-25.0, -50.0, 150.0]
            }]
        );
    }

    #[test]
    fn rejects_trailing_or_non_finite_payloads() {
        assert!(decode_vmb_daz_cm(&[0, 0, 0, 0, 1]).is_err());

        let mut bytes = 1_i32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&f32::NAN.to_le_bytes());
        bytes.extend_from_slice(&0_f32.to_le_bytes());
        bytes.extend_from_slice(&0_f32.to_le_bytes());
        assert!(decode_vmb_daz_cm(&bytes).is_err());
    }

    #[test]
    fn topology_decoder_sorts_and_filters_zero_rows() {
        let encoded = raw_vmb(&[
            (8, [0.0, 0.0, 0.0]),
            (5, [-0.03, 0.04, 0.0]),
            (2, [-0.01, 0.0, 0.0]),
        ]);
        let decoded = decode_vmb_daz_cm_for_topology(&encoded, 10, None).unwrap();

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].vertex_index, 2);
        assert_eq!(decoded[1].vertex_index, 5);
        for (actual, expected) in decoded[0].delta_cm.iter().zip([1.0, 0.0, 0.0]) {
            assert_abs_diff_eq!(*actual, expected, epsilon = 1e-6);
        }
        for (actual, expected) in decoded[1].delta_cm.iter().zip([3.0, 4.0, 0.0]) {
            assert_abs_diff_eq!(*actual, expected, epsilon = 1e-6);
        }
    }

    #[test]
    fn raw_entry_count_includes_rows_removed_by_normalization() {
        let encoded = raw_vmb(&[(3, [0.0, 0.0, 0.0]), (1, [-0.01, 0.0, 0.0])]);

        assert_eq!(vmb_raw_entry_count(&encoded).unwrap(), 2);
        assert_eq!(
            decode_vmb_daz_cm_for_topology(&encoded, 4, None)
                .unwrap()
                .len(),
            1
        );
        assert!(vmb_raw_entry_count(&encoded[..encoded.len() - 1]).is_err());
    }

    #[test]
    fn topology_decoder_rejects_duplicates_bounds_and_non_finite_rows() {
        let duplicate = raw_vmb(&[(3, [-0.01, 0.0, 0.0]), (3, [0.0, 0.0, 0.0])]);
        assert!(decode_vmb_daz_cm_for_topology(&duplicate, 4, None).is_err());

        let out_of_bounds = raw_vmb(&[(4, [-0.01, 0.0, 0.0])]);
        assert!(decode_vmb_daz_cm_for_topology(&out_of_bounds, 4, None).is_err());

        let non_finite = raw_vmb(&[(0, [f32::INFINITY, 0.0, 0.0])]);
        assert!(decode_vmb_daz_cm_for_topology(&non_finite, 4, None).is_err());
    }

    #[test]
    fn topology_decoder_routes_genital_local_indices_after_shared_body() {
        let encoded = raw_vmb(&[(2, [-0.01, 0.0, 0.0]), (0, [-0.02, 0.0, 0.0])]);
        let decoded = decode_vmb_daz_cm_for_topology(
            &encoded,
            VAM_SHARED_BODY_VERTEX_COUNT as usize + 3,
            Some(VmbVertexRouting::Genitalia),
        )
        .unwrap();

        assert_eq!(decoded[0].vertex_index, VAM_SHARED_BODY_VERTEX_COUNT);
        assert_eq!(decoded[1].vertex_index, VAM_SHARED_BODY_VERTEX_COUNT + 2);
        assert!(
            decode_vmb_daz_cm_for_topology(
                &encoded,
                VAM_SHARED_BODY_VERTEX_COUNT as usize + 2,
                Some(VmbVertexRouting::Genitalia),
            )
            .is_err()
        );
    }

    #[test]
    fn encoder_has_deterministic_golden_byte_shape() {
        let encoded = encode_vmb_daz_cm(
            &[
                SparseDelta {
                    vertex_index: 9,
                    delta_cm: [-100.0, 200.0, 400.0],
                },
                SparseDelta {
                    vertex_index: 7,
                    delta_cm: [-25.0, -50.0, 150.0],
                },
            ],
            10,
        )
        .unwrap();

        assert_eq!(
            encoded,
            vec![
                2, 0, 0, 0, 7, 0, 0, 0, 0, 0, 128, 62, 0, 0, 0, 191, 0, 0, 192, 63, 9, 0, 0, 0, 0,
                0, 128, 63, 0, 0, 0, 64, 0, 0, 128, 64,
            ]
        );
    }

    #[test]
    fn encoder_round_trips_axis_units_and_sorted_indices() {
        let encoded = encode_vmb_daz_cm(
            &[
                SparseDelta {
                    vertex_index: 42,
                    delta_cm: [12.5, -25.0, 75.0],
                },
                SparseDelta {
                    vertex_index: 3,
                    delta_cm: [-50.0, 100.0, -200.0],
                },
            ],
            43,
        )
        .unwrap();
        let decoded = decode_vmb_daz_cm(&encoded).unwrap();

        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].vertex_index, 3);
        assert_eq!(decoded[0].delta_cm, [-50.0, 100.0, -200.0]);
        assert_eq!(decoded[1].vertex_index, 42);
        assert_eq!(decoded[1].delta_cm, [12.5, -25.0, 75.0]);
    }

    #[test]
    fn topology_encoder_rebases_genital_global_indices_and_round_trips() {
        let target_vertex_count = VAM_SHARED_BODY_VERTEX_COUNT as usize + 3;
        let global = [
            SparseDelta {
                vertex_index: VAM_SHARED_BODY_VERTEX_COUNT + 2,
                delta_cm: [100.0, 0.0, 0.0],
            },
            SparseDelta {
                vertex_index: VAM_SHARED_BODY_VERTEX_COUNT,
                delta_cm: [0.0, -50.0, 25.0],
            },
        ];
        let encoded = encode_vmb_daz_cm_for_topology(
            &global,
            target_vertex_count,
            Some(VmbVertexRouting::Genitalia),
        )
        .unwrap();

        assert_eq!(vmb_raw_entry_count(&encoded).unwrap(), 2);
        let local = decode_vmb_daz_cm(&encoded).unwrap();
        assert_eq!(local[0].vertex_index, 0);
        assert_eq!(local[1].vertex_index, 2);
        let decoded = decode_vmb_daz_cm_for_topology(
            &encoded,
            target_vertex_count,
            Some(VmbVertexRouting::Genitalia),
        )
        .unwrap();
        assert_eq!(decoded, [global[1], global[0]]);
    }

    #[test]
    fn topology_encoder_rejects_invalid_genital_bounds_duplicates_and_values() {
        let target_vertex_count = VAM_SHARED_BODY_VERTEX_COUNT as usize + 2;
        let valid = SparseDelta {
            vertex_index: VAM_SHARED_BODY_VERTEX_COUNT,
            delta_cm: [1.0, 0.0, 0.0],
        };
        let route = Some(VmbVertexRouting::Genitalia);

        assert!(
            encode_vmb_daz_cm_for_topology(
                &[SparseDelta {
                    vertex_index: VAM_SHARED_BODY_VERTEX_COUNT - 1,
                    ..valid
                }],
                target_vertex_count,
                route,
            )
            .is_err()
        );
        assert!(
            encode_vmb_daz_cm_for_topology(
                &[SparseDelta {
                    vertex_index: target_vertex_count as u32,
                    ..valid
                }],
                target_vertex_count,
                route,
            )
            .is_err()
        );
        assert!(
            encode_vmb_daz_cm_for_topology(&[valid], VAM_SHARED_BODY_VERTEX_COUNT as usize, route,)
                .is_err()
        );
        assert!(
            encode_vmb_daz_cm_for_topology(&[valid, valid], target_vertex_count, route).is_err()
        );
        assert!(
            encode_vmb_daz_cm_for_topology(
                &[SparseDelta {
                    delta_cm: [f64::NAN, 0.0, 0.0],
                    ..valid
                }],
                target_vertex_count,
                route,
            )
            .is_err()
        );
        assert!(
            encode_vmb_daz_cm_for_topology(
                &[SparseDelta {
                    delta_cm: [0.0; 3],
                    ..valid
                }],
                target_vertex_count,
                route,
            )
            .is_err()
        );
    }

    #[test]
    fn topology_encoder_full_route_matches_legacy_encoder() {
        let deltas = [SparseDelta {
            vertex_index: 1,
            delta_cm: [25.0, 0.0, 0.0],
        }];
        assert_eq!(
            encode_vmb_daz_cm_for_topology(&deltas, 2, Some(VmbVertexRouting::Full),).unwrap(),
            encode_vmb_daz_cm(&deltas, 2).unwrap()
        );
    }

    #[test]
    fn encoder_rejects_invalid_sparse_entries_and_bounds() {
        let finite = SparseDelta {
            vertex_index: 2,
            delta_cm: [1.0, 0.0, 0.0],
        };
        assert!(encode_vmb_daz_cm(&[], 0).is_err());
        assert!(encode_vmb_daz_cm(&[finite], 2).is_err());
        assert!(encode_vmb_daz_cm(&[finite, finite], 3).is_err());
        assert!(
            encode_vmb_daz_cm(
                &[SparseDelta {
                    vertex_index: 0,
                    delta_cm: [0.0; 3],
                }],
                1,
            )
            .is_err()
        );
        assert!(
            encode_vmb_daz_cm(
                &[SparseDelta {
                    vertex_index: 0,
                    delta_cm: [f64::NAN, 0.0, 0.0],
                }],
                1,
            )
            .is_err()
        );
        assert!(
            encode_vmb_daz_cm(
                &[SparseDelta {
                    vertex_index: 0,
                    delta_cm: [f64::MAX, 0.0, 0.0],
                }],
                1,
            )
            .is_err()
        );
        assert!(
            encode_vmb_daz_cm(
                &[SparseDelta {
                    vertex_index: 0,
                    delta_cm: [f64::MIN_POSITIVE, 1.0, 0.0],
                }],
                1,
            )
            .is_err()
        );
        assert!(encoded_len(MAX_DELTA_COUNT + 1).is_err());
    }

    #[test]
    fn mesh_pair_builder_filters_epsilon_and_round_trips_through_vmb() {
        let base = [[0.0, 0.0, 0.0], [10.0, 20.0, 30.0], [-5.0, 2.0, 9.0]];
        let target = [
            [0.0005, -0.0005, 0.0],
            [11.25, 17.5, 37.5],
            [-5.0, 2.0, 9.0],
        ];
        let sparse = build_sparse_deltas_daz_cm(&base, &target, 0.001).unwrap();
        assert_eq!(
            sparse,
            vec![SparseDelta {
                vertex_index: 1,
                delta_cm: [1.25, -2.5, 7.5],
            }]
        );

        let encoded = encode_vmb_daz_cm(&sparse, base.len()).unwrap();
        let decoded = decode_vmb_daz_cm_for_topology(&encoded, base.len(), None).unwrap();
        assert_eq!(decoded.len(), sparse.len());
        assert_eq!(decoded[0].vertex_index, sparse[0].vertex_index);
        for (actual, expected) in decoded[0].delta_cm.iter().zip(sparse[0].delta_cm) {
            assert_abs_diff_eq!(*actual, expected, epsilon = 1e-6);
        }
    }

    #[test]
    fn mesh_pair_builder_rejects_invalid_meshes_and_epsilon() {
        assert!(build_sparse_deltas_daz_cm(&[[0.0; 3]], &[], 0.0).is_err());
        assert!(build_sparse_deltas_daz_cm(&[], &[], 0.0).is_err());
        assert!(build_sparse_deltas_daz_cm(&[[f64::NAN, 0.0, 0.0]], &[[0.0; 3]], 0.0).is_err());
        assert!(
            build_sparse_deltas_daz_cm(&[[0.0; 3]], &[[f64::INFINITY, 0.0, 0.0]], 0.0).is_err()
        );
        assert!(
            build_sparse_deltas_daz_cm(&[[f64::MAX, 0.0, 0.0]], &[[-f64::MAX, 0.0, 0.0]], 0.0)
                .is_err()
        );
        assert!(build_sparse_deltas_daz_cm(&[[0.0; 3]], &[[0.0; 3]], -1.0).is_err());
        assert!(build_sparse_deltas_daz_cm(&[[0.0; 3]], &[[0.0; 3]], f64::NAN).is_err());
    }
}
