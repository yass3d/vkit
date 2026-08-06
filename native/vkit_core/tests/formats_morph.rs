use approx::assert_abs_diff_eq;
use serde_json::json;
use vkit_core::formats::{
    DazGeometry, GroupTable, ObjFace, ObjMorphSource, OrderedObjMesh, infer_target_unit_scale,
    load_obj_morph_target, validate_morph_topology,
};

fn obj(vertices: Vec<[f64; 3]>, faces: Vec<Vec<u32>>) -> OrderedObjMesh {
    OrderedObjMesh {
        vertices,
        faces: faces
            .into_iter()
            .map(|vertex_indices| ObjFace {
                vertex_indices,
                group: None,
                material: None,
            })
            .collect(),
    }
}

fn base_vertices() -> Vec<[f64; 3]> {
    vec![
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ]
}

fn base_faces() -> Vec<Vec<u32>> {
    vec![vec![0, 1, 2], vec![0, 2, 3]]
}

fn labeled_obj(vertices: Vec<[f64; 3]>, faces: Vec<Vec<u32>>) -> OrderedObjMesh {
    OrderedObjMesh {
        vertices,
        faces: faces
            .into_iter()
            .enumerate()
            .map(|(index, vertex_indices)| ObjFace {
                vertex_indices,
                group: Some(format!("group_{index}")),
                material: Some(format!("material_{index}")),
            })
            .collect(),
    }
}

fn daz_geometry() -> DazGeometry {
    DazGeometry::new(
        "Genesis2Female".to_owned(),
        base_vertices(),
        base_faces(),
        GroupTable {
            indices: vec![0, 1],
            names: vec!["Head".to_owned(), "Face".to_owned()],
        },
        GroupTable {
            indices: vec![1, 0],
            names: vec!["SkinFace".to_owned(), "Lips".to_owned()],
        },
        json!({
            "id": "root",
            "children": [{"id": "face", "label": "Face"}],
        }),
    )
    .expect("valid test DSF geometry")
}

fn eye_closed_target() -> vkit_core::formats::MorphTarget {
    let mut closed_vertices = base_vertices();
    closed_vertices[1][1] -= 0.2;
    closed_vertices[2][1] -= 0.4;
    load_obj_morph_target(
        "eye_closed",
        ObjMorphSource {
            target: &obj(closed_vertices, base_faces()),
            base_vertices: &base_vertices(),
            base_faces: &base_faces(),
            unit_scale: Some(1.0),
        },
        1.0e-12,
        0.0,
        1.0,
        0.0,
    )
    .expect("valid eye target")
}

#[test]
fn morph_topology_accepts_face_reorder_and_cyclic_starts_but_not_winding_flip() {
    let reordered = obj(base_vertices(), vec![vec![2, 3, 0], vec![1, 2, 0]]);
    assert!(!validate_morph_topology(&reordered, 4, &base_faces(), true).expect("valid"));

    let reversed = obj(base_vertices(), vec![vec![0, 2, 1], vec![0, 2, 3]]);
    assert!(validate_morph_topology(&reversed, 4, &base_faces(), true).is_err());
}

#[test]
fn morph_loader_infers_decimal_units_and_composes_non_destructively() {
    let target = obj(
        vec![
            [0.0, 0.0, 0.0],
            [0.011, 0.0, 0.0],
            [0.010, 0.010, 0.0],
            [0.0, 0.010, 0.0],
        ],
        vec![vec![0, 1, 2], vec![0, 2, 3]],
    );
    assert_eq!(
        infer_target_unit_scale(&base_vertices(), &target.vertices).expect("scale"),
        100.0
    );
    let morph = load_obj_morph_target(
        "eye_closed",
        ObjMorphSource {
            target: &target,
            base_vertices: &base_vertices(),
            base_faces: &base_faces(),
            unit_scale: None,
        },
        1.0e-12,
        0.0,
        1.0,
        0.0,
    )
    .expect("load morph");
    assert_eq!(morph.compatibility.source_unit_scale, 100.0);
    assert_eq!(morph.compatibility.active_vertex_count, 1);
    assert_abs_diff_eq!(morph.deltas[1][0], 0.1, epsilon = 1.0e-12);

    let base = base_vertices();
    let composed = morph.compose(&base, 0.5, 0.0).expect("compose");
    assert_abs_diff_eq!(composed[1][0], 1.05, epsilon = 1.0e-12);
    assert_eq!(base[1][0], 1.0, "compose must not mutate fitted base");
}

#[test]
fn morph_scale_inference_rejects_non_decimal_ambiguity() {
    let target: Vec<[f64; 3]> = base_vertices()
        .into_iter()
        .map(|point| [point[0] / 3.0, point[1] / 3.0, point[2] / 3.0])
        .collect();
    assert!(infer_target_unit_scale(&base_vertices(), &target).is_err());
}

#[test]
fn applies_open_to_closed_on_a_cloned_daz_geometry() {
    let source = daz_geometry();
    let source_snapshot = source.clone();
    let target = eye_closed_target();

    let closed = target
        .apply_to_daz_geometry(&source, 1.0)
        .expect("apply closed-eye template");

    assert_eq!(source, source_snapshot, "the imported DSF must not mutate");
    assert_abs_diff_eq!(closed.vertices[1][1], -0.2, epsilon = 1.0e-12);
    assert_abs_diff_eq!(closed.vertices[2][1], 0.6, epsilon = 1.0e-12);
    let mut expected = source_snapshot;
    expected.vertices = closed.vertices.clone();
    assert_eq!(closed, expected, "only DSF vertex positions may change");
}

#[test]
fn reopens_a_result_fitted_from_the_closed_reference() {
    let target = eye_closed_target();
    let mut closed_fitted_vertices = base_vertices();
    closed_fitted_vertices[1][1] = -0.15;
    closed_fitted_vertices[2][1] = 0.65;
    let closed_fitted = labeled_obj(closed_fitted_vertices, base_faces());
    let input_snapshot = closed_fitted.clone();

    let reopened = target
        .bake_onto_ordered_obj(&closed_fitted, 0.0, 1.0)
        .expect("reopen fitted result");

    assert_eq!(
        closed_fitted, input_snapshot,
        "the fitted result must not mutate"
    );
    assert_abs_diff_eq!(reopened.vertices[1][1], 0.05, epsilon = 1.0e-12);
    assert_abs_diff_eq!(reopened.vertices[2][1], 1.05, epsilon = 1.0e-12);
    assert_eq!(reopened.faces, input_snapshot.faces);
}

#[test]
fn default_value_is_the_pre_fit_reference_shape() {
    let target = eye_closed_target();
    let source = daz_geometry();
    let unchanged = target
        .apply_to_daz_geometry(&source, target.default)
        .expect("apply default state");
    assert_eq!(unchanged, source);
}

#[test]
fn application_rejects_count_topology_nonfinite_range_and_tampered_receipts() {
    let target = eye_closed_target();

    let wrong_count = labeled_obj(base_vertices()[..3].to_vec(), vec![vec![0, 1, 2]]);
    assert!(
        target
            .bake_onto_ordered_obj(&wrong_count, 0.0, 0.0)
            .is_err()
    );

    let wrong_topology = labeled_obj(base_vertices(), vec![vec![0, 1, 3], vec![1, 2, 3]]);
    assert!(
        target
            .bake_onto_ordered_obj(&wrong_topology, 0.0, 0.0)
            .is_err()
    );

    assert!(
        target
            .bake_onto_ordered_obj(&labeled_obj(base_vertices(), base_faces()), 1.01, 0.0,)
            .is_err()
    );

    let mut nonfinite = target.clone();
    nonfinite.deltas[0][0] = f64::NAN;
    assert!(
        nonfinite
            .apply_to_daz_geometry(&daz_geometry(), 1.0)
            .is_err()
    );

    let mut bad_range = target.clone();
    bad_range.minimum = 2.0;
    assert!(
        bad_range
            .apply_to_daz_geometry(&daz_geometry(), 1.0)
            .is_err()
    );

    let mut bad_receipt = target;
    bad_receipt.compatibility.vertex_count -= 1;
    assert!(
        bad_receipt
            .apply_to_daz_geometry(&daz_geometry(), 1.0)
            .is_err()
    );
}
