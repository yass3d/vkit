use std::io::Cursor;

use vkit_core::formats::{ObjFace, OrderedObjMesh, parse_ordered_obj, write_ordered_obj};

#[test]
fn obj_import_preserves_vertex_polygon_and_display_state_order() {
    let source = concat!(
        "\u{feff}# exported mesh\n",
        "g Face\n",
        "usemtl Skin\n",
        "v 0 0 0\n",
        "v 1 0 0 1\n",
        "v 1 1 0\n",
        "v 0 1 0\n",
        "f 1/7/3 2//4 3/9\n",
        "g off\n",
        "usemtl off\n",
        "f -4 -2 -1\n",
    );

    let mesh = parse_ordered_obj(Cursor::new(source)).expect("valid OBJ");
    assert_eq!(
        mesh.vertices,
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ]
    );
    assert_eq!(mesh.faces[0].vertex_indices, vec![0, 1, 2]);
    assert_eq!(mesh.faces[0].group.as_deref(), Some("Face"));
    assert_eq!(mesh.faces[0].material.as_deref(), Some("Skin"));
    assert_eq!(mesh.faces[1].vertex_indices, vec![0, 2, 3]);
    assert_eq!(mesh.faces[1].group, None);
    assert_eq!(mesh.faces[1].material, None);
}

#[test]
fn obj_round_trip_retains_none_state_and_float_bits() {
    let mesh = OrderedObjMesh {
        vertices: vec![
            [0.1, -0.0, f64::MIN_POSITIVE],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ],
        faces: vec![
            ObjFace {
                vertex_indices: vec![0, 1, 2],
                group: Some("Head shell".to_owned()),
                material: Some("Skin".to_owned()),
            },
            ObjFace {
                vertex_indices: vec![0, 2, 3],
                group: None,
                material: None,
            },
        ],
    };
    let mut encoded = Vec::new();
    write_ordered_obj(&mut encoded, &mesh).expect("write OBJ");
    let decoded = parse_ordered_obj(Cursor::new(encoded)).expect("read OBJ");
    assert_eq!(decoded, mesh);
}

#[test]
fn obj_fan_triangulation_does_not_reorder_vertices() {
    let mesh = OrderedObjMesh {
        vertices: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        faces: vec![ObjFace {
            vertex_indices: vec![3, 0, 1, 2],
            group: None,
            material: None,
        }],
    };
    let triangles = mesh.triangulated().expect("triangulation");
    assert_eq!(triangles.vertices, mesh.vertices);
    assert_eq!(triangles.triangles, vec![[3, 0, 1], [3, 1, 2]]);
}

#[test]
fn obj_rejects_zero_and_out_of_range_indices() {
    for source in [
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 0 2 3\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 4\n",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf -4 -2 -1\n",
    ] {
        assert!(parse_ordered_obj(Cursor::new(source)).is_err());
    }
}
