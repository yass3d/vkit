use serde_json::json;
use vkit_core::formats::{
    LegacyLandmarkSessionV1, Mesh, SurfaceAttachment, canonical_mesh_hash_hex,
};

#[test]
fn canonical_hash_is_pinned_and_normalizes_negative_zero() {
    let vertices = vec![[0.0, -0.0, 1.25], [2.0, 3.5, -4.0], [5.0, 6.0, 7.0]];
    let triangles = vec![[0, 1, 2]];
    let digest = canonical_mesh_hash_hex(&vertices, &triangles).expect("hash");
    assert_eq!(
        digest,
        "88bfdd5a2036ab32e3cc16482c48330695ec7debe5086784d99526cdda0c19c5"
    );

    let positive_zero = vec![[0.0, 0.0, 1.25], [2.0, 3.5, -4.0], [5.0, 6.0, 7.0]];
    assert_eq!(
        digest,
        canonical_mesh_hash_hex(&positive_zero, &triangles).expect("hash")
    );
}

#[test]
fn surface_attachment_normalizes_tolerance_noise_and_resolves() {
    let mut attachment = SurfaceAttachment {
        triangle_vertex_ids: [0, 1, 2],
        barycentric: [-1.0e-8, 0.500000005, 0.500000005],
        primitive_id: Some(7),
    };
    attachment
        .validate_and_normalize()
        .expect("accepted tolerance noise");
    assert_eq!(attachment.barycentric[0], 0.0);
    let point = attachment
        .resolve(&[[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]])
        .expect("resolve attachment");
    assert!((point[0] - 1.0).abs() < 1.0e-12);
    assert!((point[1] - 1.0).abs() < 1.0e-12);
    assert_eq!(point[2], 0.0);
}

fn endpoint(point: [f64; 3]) -> serde_json::Value {
    json!({
        "coordinate_space": "mesh_local",
        "point": point,
        "surface": {
            "triangle_vertex_ids": [0, 1, 2],
            "barycentric": [0.5, 0.25, 0.25],
            "primitive_id": 0
        },
        "pick": {}
    })
}

fn session_value() -> serde_json::Value {
    let mesh = Mesh::new(
        vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
        vec![[0, 1, 2]],
    )
    .expect("mesh");
    let digest = mesh.canonical_hash_hex().expect("digest");
    json!({
        "format": "vkit.landmark_pairs",
        "schema_version": 1,
        "meshes": {
            "scan": {"canonical_sha256": digest, "vertex_count": 3, "triangle_count": 1},
            "template": {"canonical_sha256": digest, "vertex_count": 3, "triangle_count": 1}
        },
        "views": {},
        "pairs": [{
            "id": "pair_0001",
            "region": "core",
            "status": "complete",
            "alignment": {"enabled": true, "weight": 3.0},
            "scan": endpoint([0.5, 0.5, 0.0]),
            "template": endpoint([0.5, 0.5, 0.0])
        }],
        "metadata": {}
    })
}

#[test]
fn legacy_v1_session_imports_defaults_validates_and_round_trips() {
    let source = serde_json::to_vec(&session_value()).expect("encode fixture");
    let session = LegacyLandmarkSessionV1::from_slice(&source).expect("legacy import");
    assert_eq!(session.alignment_pair_count(), 1);
    assert_eq!(session.fit_pair_count(), 1);
    assert_eq!(session.pairs[0].source, "manual");
    assert_eq!(session.pairs[0].confidence, 1.0);
    assert!(session.pairs[0].enabled);
    assert!(session.pairs[0].fit.enabled);
    assert_eq!(session.pairs[0].fit.weight, 1.0);

    let mesh = Mesh::new(
        vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
        vec![[0, 1, 2]],
    )
    .expect("mesh");
    session.validate_meshes(&mesh, &mesh).expect("bindings");

    let mut encoded = Vec::new();
    session.write_pretty(&mut encoded).expect("write session");
    let decoded = LegacyLandmarkSessionV1::from_slice(&encoded).expect("read session");
    assert_eq!(decoded, session);
}

#[test]
fn legacy_session_rejects_duplicates_and_incomplete_complete_pairs() {
    let mut duplicate = session_value();
    let pair = duplicate["pairs"][0].clone();
    duplicate["pairs"].as_array_mut().expect("pairs").push(pair);
    assert!(
        LegacyLandmarkSessionV1::from_slice(
            &serde_json::to_vec(&duplicate).expect("encode duplicate")
        )
        .is_err()
    );

    let mut incomplete = session_value();
    incomplete["pairs"][0]
        .as_object_mut()
        .expect("pair")
        .remove("scan");
    assert!(
        LegacyLandmarkSessionV1::from_slice(
            &serde_json::to_vec(&incomplete).expect("encode incomplete")
        )
        .is_err()
    );
}
