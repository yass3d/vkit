use std::io::{Cursor, Write};

use flate2::{Compression, write::GzEncoder};
use serde_json::json;
use vkit_core::formats::{
    DazGeometry, GroupTable, HEAD_SKIN_MATERIALS, HEAD_VISUAL_EXCLUDED_MATERIALS,
    HEAD_VISUAL_MATERIALS, load_dsf,
};

fn fixture() -> Vec<u8> {
    br#"{
      "geometry_library": [{
        "id": "GenesisFemale-1",
        "vertices": {"count": 4, "values": [
          [0.1, 0.0, 0.0], [1.0, 0.0, 0.0],
          [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]
        ]},
        "polylist": {"count": 2, "values": [
          [0, 0, 0, 1, 2], [1, 1, 0, 2, 3, 1]
        ]},
        "polygon_groups": {"values": ["Head", "Mouth"]},
        "polygon_material_groups": {"values": ["Face", "Tongue"]},
        "root_region": {"id": "root"}
      }]
    }"#
    .to_vec()
}

#[test]
fn dsf_loads_raw_or_gzip_and_preserves_tables_and_order() {
    let raw = fixture();
    let plain = load_dsf(Cursor::new(&raw), 0).expect("raw DSF");

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(&raw).expect("compress fixture");
    let compressed = encoder.finish().expect("finish gzip");
    let gzip = load_dsf(Cursor::new(compressed), 0).expect("gzip DSF");

    assert_eq!(plain, gzip);
    assert_eq!(plain.geometry_id, "GenesisFemale-1");
    assert_eq!(plain.vertices[0][0], 0.1_f32 as f64);
    assert_eq!(plain.faces, vec![vec![0, 1, 2], vec![0, 2, 3, 1]]);
    assert_eq!(plain.polygon_group_indices, vec![0, 1]);
    assert_eq!(plain.material_group_indices, vec![0, 1]);
    assert_eq!(plain.root_region["id"], "root");

    let mask = plain.face_mask_for_materials(HEAD_SKIN_MATERIALS.iter().copied());
    assert_eq!(mask, vec![true, false]);
    let obj = plain.to_ordered_obj(Some(&mask)).expect("masked OBJ");
    assert_eq!(obj.vertices.len(), 4, "mask must not renumber vertices");
    assert_eq!(obj.faces.len(), 1);
    assert_eq!(obj.faces[0].group.as_deref(), Some("Face"));
    assert_eq!(obj.faces[0].material.as_deref(), Some("Face"));
}

#[test]
fn dsf_rejects_bad_counts_and_indices() {
    let count_mismatch = String::from_utf8(fixture())
        .expect("UTF-8 fixture")
        .replacen("\"count\": 4", "\"count\": 5", 1);
    assert!(load_dsf(Cursor::new(count_mismatch), 0).is_err());

    let bad_vertex = String::from_utf8(fixture())
        .expect("UTF-8 fixture")
        .replacen("[0, 0, 0, 1, 2]", "[0, 0, 0, 1, 9]", 1);
    assert!(load_dsf(Cursor::new(bad_vertex), 0).is_err());

    let bad_group = String::from_utf8(fixture())
        .expect("UTF-8 fixture")
        .replacen("[1, 1, 0, 2, 3, 1]", "[9, 1, 0, 2, 3, 1]", 1);
    assert!(load_dsf(Cursor::new(bad_group), 0).is_err());
}

#[test]
fn head_visual_mask_excludes_thin_eye_shells_without_removing_stable_anatomy() {
    assert_eq!(
        HEAD_VISUAL_EXCLUDED_MATERIALS,
        ["Lacrimals", "Tear", "Eyelashes"]
    );
    let materials = [
        "Face",
        "Lacrimals",
        "Tear",
        "Eyelashes",
        "Sclera",
        "Irises",
        "Pupils",
        "InnerMouth",
        "Teeth",
        "Tongue",
    ];
    let vertices = (0..materials.len() * 3)
        .map(|index| [index as f64, (index % 3) as f64, 0.0])
        .collect::<Vec<_>>();
    let faces = (0..materials.len())
        .map(|index| {
            let start = (index * 3) as u32;
            vec![start, start + 1, start + 2]
        })
        .collect::<Vec<_>>();
    let geometry = DazGeometry::new(
        "visual-mask".into(),
        vertices,
        faces,
        GroupTable {
            indices: vec![0; materials.len()],
            names: vec!["head".into()],
        },
        GroupTable {
            indices: (0..materials.len() as u32).collect(),
            names: materials.iter().map(|name| (*name).into()).collect(),
        },
        json!({}),
    )
    .unwrap();
    let mask = geometry.face_mask_for_materials(HEAD_VISUAL_MATERIALS.iter().copied());
    assert_eq!(
        mask,
        vec![
            true, false, false, false, true, true, true, true, true, true
        ]
    );
    let visual = geometry.to_ordered_obj(Some(&mask)).unwrap();
    assert_eq!(
        visual.faces.len(),
        materials.len() - HEAD_VISUAL_EXCLUDED_MATERIALS.len()
    );
    let complete = geometry.to_ordered_obj(None).unwrap();
    assert_eq!(complete.faces.len(), materials.len());
    assert_eq!(complete.vertices.len(), geometry.vertices.len());
}
