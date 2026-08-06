use sha2::{Digest, Sha256};
use vkit_core::formats::{
    GroupTable, SparseMorphDelta, TEMPLATE_PACK_MAGIC, TEMPLATE_PACK_VERSION, TemplatePack,
    TemplatePolygon, decode_template_pack, encode_template_pack,
};

const HEADER_SIZE: usize = 80;
const CHECKSUM_OFFSET: usize = 48;

fn synthetic_pack() -> TemplatePack {
    TemplatePack::new(
        vec![
            [0.25, 0.0, -1.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ],
        vec![
            TemplatePolygon::Triangle([2, 0, 1]),
            TemplatePolygon::Quad([4, 3, 2, 1]),
        ],
        GroupTable {
            indices: vec![1, 0],
            names: vec!["Head".to_owned(), "Mouth".to_owned()],
        },
        GroupTable {
            indices: vec![0, 1],
            names: vec!["얼굴".to_owned(), "Tongue".to_owned()],
        },
        vec![
            SparseMorphDelta {
                vertex_id: 4,
                delta: [0.0, -0.125, 0.25],
            },
            SparseMorphDelta {
                vertex_id: 1,
                delta: [0.5, 0.0, 0.0],
            },
        ],
    )
    .expect("synthetic template pack")
}

fn resign(encoded: &mut [u8]) {
    let mut digest = Sha256::new();
    digest.update(&encoded[..CHECKSUM_OFFSET]);
    digest.update(&encoded[HEADER_SIZE..]);
    let checksum: [u8; 32] = digest.finalize().into();
    encoded[CHECKSUM_OFFSET..HEADER_SIZE].copy_from_slice(&checksum);
}

#[test]
fn template_pack_round_trip_is_ordered_sparse_and_byte_deterministic() {
    let pack = synthetic_pack();
    assert_eq!(pack.closed_eye_deltas[0].vertex_id, 1);
    assert_eq!(pack.closed_eye_deltas[1].vertex_id, 4);

    let first = encode_template_pack(&pack).expect("encode pack");
    let second = encode_template_pack(&pack).expect("encode pack again");
    assert_eq!(first, second);
    assert_eq!(&first[..8], &TEMPLATE_PACK_MAGIC);
    assert_eq!(
        u16::from_le_bytes(first[8..10].try_into().expect("version bytes")),
        TEMPLATE_PACK_VERSION
    );

    let decoded = decode_template_pack(&first).expect("decode pack");
    assert_eq!(decoded, pack);
    assert_eq!(decoded.polygons[0], TemplatePolygon::Triangle([2, 0, 1]));
    assert_eq!(decoded.polygons[1], TemplatePolygon::Quad([4, 3, 2, 1]));
    assert_eq!(decoded.polygon_group_indices, vec![1, 0]);
    assert_eq!(decoded.material_group_indices, vec![0, 1]);
    assert_eq!(decoded.material_groups[0], "얼굴");
    assert_eq!(encode_template_pack(&decoded).expect("re-encode"), first);

    let dense = decoded.dense_closed_eye_deltas().expect("dense morph");
    assert_eq!(dense[0], [0.0; 3]);
    assert_eq!(dense[1], [0.5, 0.0, 0.0]);
    assert_eq!(dense[4], [0.0, -0.125, 0.25]);
}

#[test]
fn template_pack_checksum_covers_header_and_payload() {
    let encoded = encode_template_pack(&synthetic_pack()).expect("encode pack");

    let mut payload_corruption = encoded.clone();
    payload_corruption[HEADER_SIZE] ^= 0x40;
    assert!(
        decode_template_pack(&payload_corruption)
            .expect_err("payload corruption")
            .to_string()
            .contains("checksum")
    );

    let mut header_corruption = encoded.clone();
    header_corruption[32] ^= 1;
    assert!(
        decode_template_pack(&header_corruption)
            .expect_err("header corruption")
            .to_string()
            .contains("checksum")
    );

    let mut truncated = encoded.clone();
    truncated.pop();
    assert!(decode_template_pack(&truncated).is_err());

    let mut extended = encoded;
    extended.push(0);
    assert!(decode_template_pack(&extended).is_err());
}

#[test]
fn template_pack_rejects_resigned_malformed_payload_and_oversized_counts() {
    let mut bad_arity = encode_template_pack(&synthetic_pack()).expect("encode pack");
    let polygon_offset = HEADER_SIZE + synthetic_pack().vertices.len() * 3 * 4;
    bad_arity[polygon_offset] = 5;
    resign(&mut bad_arity);
    assert!(
        decode_template_pack(&bad_arity)
            .expect_err("bad polygon arity")
            .to_string()
            .contains("arity")
    );

    let mut oversized = encode_template_pack(&synthetic_pack()).expect("encode pack");
    oversized[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
    resign(&mut oversized);
    assert!(
        decode_template_pack(&oversized)
            .expect_err("oversized vertex count")
            .to_string()
            .contains("exceeds limit")
    );
}

#[test]
fn template_pack_validation_rejects_unsafe_indices_and_sparse_entries() {
    let mut duplicate_corner = synthetic_pack();
    duplicate_corner.polygons[0] = TemplatePolygon::Triangle([0, 0, 1]);
    assert!(encode_template_pack(&duplicate_corner).is_err());

    let mut bad_material = synthetic_pack();
    bad_material.material_group_indices[0] = 99;
    assert!(encode_template_pack(&bad_material).is_err());

    let mut unsorted = synthetic_pack();
    unsorted.closed_eye_deltas.swap(0, 1);
    assert!(encode_template_pack(&unsorted).is_err());

    let mut zero_sparse_entry = synthetic_pack();
    zero_sparse_entry.closed_eye_deltas[0].delta = [0.0; 3];
    assert!(encode_template_pack(&zero_sparse_entry).is_err());

    let mut non_finite = synthetic_pack();
    non_finite.vertices[0][0] = f32::NAN;
    assert!(encode_template_pack(&non_finite).is_err());
}
