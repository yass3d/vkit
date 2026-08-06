use std::io::Cursor;
use std::path::PathBuf;

use vkit_core::formats::{parse_obj_document, parse_ordered_obj, write_obj_document};

#[test]
fn companion_parser_preserves_geometry_and_maps_ngons_deterministically() {
    let source = concat!(
        "\u{feff}mtllib materials/skin.mtl\n",
        "v 0 0 0\n",
        "v 1 0 0 1\n",
        "v 1 1 0\n",
        "v 0 1 0\n",
        "vt 0 0\n",
        "vt 1 0 0\n",
        "vt 1 1\n",
        "vt 0 1\n",
        "vn 0 0 1\n",
        "g Face\n",
        "usemtl Skin\n",
        "f 1/1/1 2/2/1 3/3/1 4/4/1\n",
        "usemtl Lips\n",
        "f -4/-4/1 -2/-2/1 -1/-1/1\n",
    );

    let ordered = parse_ordered_obj(Cursor::new(source)).expect("ordered geometry");
    let document = parse_obj_document(Cursor::new(source)).expect("appearance document");
    assert_eq!(document.geometry, ordered);
    assert_eq!(
        document.appearance.material_libraries,
        vec![PathBuf::from("materials").join("skin.mtl")]
    );
    assert_eq!(document.appearance.material_names, ["Skin", "Lips"]);
    assert_eq!(
        document.appearance.face_texcoord_indices,
        vec![
            vec![Some(0), Some(1), Some(2), Some(3)],
            vec![Some(0), Some(2), Some(3)],
        ]
    );

    let triangles = document
        .triangulated_appearance()
        .expect("fan appearance mapping");
    assert_eq!(
        triangles.mesh.triangles,
        vec![[0, 1, 2], [0, 2, 3], [0, 2, 3]]
    );
    assert_eq!(triangles.triangle_to_polygon, [0, 0, 1]);
    assert_eq!(
        triangles.triangle_texcoord_indices,
        [
            [Some(0), Some(1), Some(2)],
            [Some(0), Some(2), Some(3)],
            [Some(0), Some(2), Some(3)],
        ]
    );
    assert_eq!(
        triangles.triangle_materials,
        [
            Some("Skin".to_owned()),
            Some("Skin".to_owned()),
            Some("Lips".to_owned()),
        ]
    );
}

#[test]
fn negative_position_texture_and_normal_indices_use_independent_stream_counts() {
    let source = concat!(
        "v 0 0 0\n",
        "v 1 0 0\n",
        "v 0 1 0\n",
        "v 0 0 1\n",
        "vt 0 0\n",
        "vt .25 0\n",
        "vt .5 0\n",
        "vt .75 0\n",
        "vt 1 0\n",
        "vn 0 0 1\n",
        "vn 0 1 0\n",
        "f -4/-3/-1 -2/-2/-2 -1/-1/-1\n",
    );
    let document = parse_obj_document(Cursor::new(source)).expect("valid independent indices");
    assert_eq!(document.geometry.faces[0].vertex_indices, [0, 2, 3]);
    assert_eq!(
        document.appearance.face_texcoord_indices[0],
        [Some(2), Some(3), Some(4)]
    );
}

#[test]
fn position_only_and_normal_only_corners_remain_explicitly_untextured() {
    let source = concat!(
        "v 0 0 0\n",
        "v 1 0 0\n",
        "v 0 1 0\n",
        "vn 0 0 1\n",
        "f 1 2//1 3\n",
    );
    let document = parse_obj_document(Cursor::new(source)).expect("untextured OBJ");
    assert_eq!(
        document.appearance.face_texcoord_indices,
        [vec![None, None, None]]
    );
}

#[test]
fn companion_parser_rejects_malformed_or_cross_stream_indices() {
    let prefix = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvn 0 0 1\n";
    for face in [
        "f 1/0/1 2/1/1 3/1/1\n",
        "f 1/2/1 2/1/1 3/1/1\n",
        "f 1/1/0 2/1/1 3/1/1\n",
        "f 1/1/2 2/1/1 3/1/1\n",
        "f 1/ 2/1 3/1\n",
        "f 1/1/ 2/1/1 3/1/1\n",
        "f 1/1/1/1 2/1/1 3/1/1\n",
    ] {
        let source = format!("{prefix}{face}");
        assert!(
            parse_obj_document(Cursor::new(source)).is_err(),
            "face should be rejected: {face}"
        );
    }
}

#[test]
fn companion_parser_rejects_nonfinite_uvs_and_unsafe_material_libraries() {
    let geometry = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
    assert!(parse_obj_document(Cursor::new(format!("vt NaN 0\n{geometry}"))).is_err());
    for library in ["../skin.mtl", "C:\\skin.mtl", "//server/skin.mtl"] {
        let source = format!("mtllib {library}\n{geometry}");
        assert!(
            parse_obj_document(Cursor::new(source)).is_err(),
            "library should be rejected: {library}"
        );
    }
}

#[test]
fn document_roundtrip_preserves_no_group_material_transitions_and_uv_corners() {
    let source = concat!(
        "mtllib materials/skin.mtl\n",
        "mtllib materials/eyes.mtl\n",
        "v -0.125 0 0\n",
        "v 1 0 0\n",
        "v 1 1 0\n",
        "v 0 1 0\n",
        "vt 0.125 0.25\n",
        "vt 0.75 0.25\n",
        "vt 0.75 1\n",
        "usemtl Skin\n",
        "f 1/1 2/2 3/3\n",
        "usemtl off\n",
        "f 1 3/3 4/1\n",
        "usemtl Skin\n",
        "f 1/1 3 4/3\n",
    );
    let original = parse_obj_document(Cursor::new(source)).expect("parse source document");
    assert!(
        original
            .geometry
            .faces
            .iter()
            .all(|face| face.group.is_none())
    );

    let mut encoded = Vec::new();
    write_obj_document(&mut encoded, &original).expect("write appearance OBJ");
    let text = String::from_utf8(encoded.clone()).expect("writer emits UTF-8 metadata");
    assert!(!text.lines().any(|line| line.starts_with("g ")));
    assert_eq!(
        text.lines()
            .filter(|line| line.starts_with("mtllib "))
            .collect::<Vec<_>>(),
        ["mtllib materials/skin.mtl", "mtllib materials/eyes.mtl"]
    );
    assert!(text.contains("f 1/1 3 4/3\n"));

    let reparsed = parse_obj_document(Cursor::new(encoded)).expect("reparse written document");
    assert_eq!(reparsed, original);
}

#[test]
fn document_roundtrip_preserves_present_and_cleared_groups() {
    let source = concat!(
        "v 0 0 0\n",
        "v 1 0 0\n",
        "v 1 1 0\n",
        "v 0 1 0\n",
        "vt 0 0\n",
        "vt 1 0\n",
        "vt 1 1\n",
        "g Face Skin\n",
        "usemtl Skin\n",
        "f 1/1 2/2 3/3\n",
        "g off\n",
        "usemtl Eye Wet\n",
        "f 1/1 3/3 4\n",
    );
    let original = parse_obj_document(Cursor::new(source)).expect("parse grouped source");
    let mut encoded = Vec::new();
    write_obj_document(&mut encoded, &original).expect("write grouped source");
    let text = String::from_utf8(encoded.clone()).expect("UTF-8 output");
    assert!(text.contains("g Face Skin\n"));
    assert!(text.contains("g off\n"));
    assert!(text.contains("usemtl Skin\n"));
    assert!(text.contains("usemtl Eye Wet\n"));

    let reparsed = parse_obj_document(Cursor::new(encoded)).expect("reparse grouped output");
    assert_eq!(reparsed, original);
}

#[test]
fn writer_recreates_material_first_use_names_that_precede_faces() {
    let source = concat!(
        "v 0 0 0\n",
        "v 1 0 0\n",
        "v 0 1 0\n",
        "usemtl Unused Prelude\n",
        "usemtl Skin\n",
        "f 1 2 3\n",
    );
    let original = parse_obj_document(Cursor::new(source)).expect("parse material prelude");
    assert_eq!(
        original.appearance.material_names,
        ["Unused Prelude", "Skin"]
    );

    let mut encoded = Vec::new();
    write_obj_document(&mut encoded, &original).expect("write material prelude");
    let reparsed = parse_obj_document(Cursor::new(encoded)).expect("reparse material prelude");
    assert_eq!(reparsed, original);
}

#[test]
fn writer_validates_the_complete_document_before_emitting_bytes() {
    let source = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nf 1/1 2/1 3/1\n";
    let mut document = parse_obj_document(Cursor::new(source)).expect("parse source");
    document.appearance.face_texcoord_indices[0][2] = Some(9);

    let mut output = Vec::new();
    assert!(write_obj_document(&mut output, &document).is_err());
    assert!(output.is_empty(), "validation must precede all writes");
}
