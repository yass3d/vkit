use std::path::PathBuf;

use vkit_core::formats::{
    G2F_TOPOLOGY_SHA256, ObjFace, OrderedObjMesh, canonical_g2f_topology_digest, convert_g2f_obj,
    load_dsf_path,
};

fn configured_g2_dsf() -> Option<PathBuf> {
    std::env::var_os("VKIT_G2_DSF").map(PathBuf::from)
}

#[test]
#[ignore = "requires the user's licensed local Genesis2Female.dsf"]
fn canonical_dsf_round_trips_through_the_strict_obj_contract() {
    let dsf_path = configured_g2_dsf().expect("set VKIT_G2_DSF");
    let geometry = load_dsf_path(dsf_path, 0).unwrap();
    assert_eq!(
        canonical_g2f_topology_digest(&geometry.faces).unwrap(),
        G2F_TOPOLOGY_SHA256
    );
    let obj = OrderedObjMesh {
        vertices: geometry
            .vertices
            .iter()
            .map(|point| [point[0] / 100.0, point[1] / 100.0, point[2] / 100.0])
            .collect(),
        faces: geometry
            .faces
            .iter()
            .enumerate()
            .map(|(face_id, indices)| ObjFace {
                vertex_indices: indices.clone(),
                group: Some(
                    geometry.polygon_groups[geometry.polygon_group_indices[face_id] as usize]
                        .clone(),
                ),
                material: Some(
                    geometry.material_groups[geometry.material_group_indices[face_id] as usize]
                        .clone(),
                ),
            })
            .collect(),
    };
    let imported = convert_g2f_obj(&obj).unwrap();
    assert_eq!(imported.receipt.normalization.source_unit_scale, 100.0);
    assert_eq!(imported.geometry.faces, geometry.faces);
    let maximum_error = imported
        .geometry
        .vertices
        .iter()
        .zip(&geometry.vertices)
        .flat_map(|(actual, expected)| {
            (0..3).map(move |axis| (actual[axis] - expected[axis]).abs())
        })
        .fold(0.0_f64, f64::max);
    assert!(
        maximum_error < 1.0e-12,
        "unit round trip error {maximum_error}"
    );

    let mut reordered = obj.clone();
    reordered.faces.swap(0, 1);
    assert!(convert_g2f_obj(&reordered).is_err());
    let mut reversed = obj;
    reversed.faces[0].vertex_indices.reverse();
    assert!(convert_g2f_obj(&reversed).is_err());
}
