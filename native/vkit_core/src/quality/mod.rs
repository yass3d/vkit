use crate::formats::{DazGeometry, OrderedObjMesh};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyQuality {
    pub valid: bool,
    pub vertex_count_match: bool,
    pub polygon_count_match: bool,
    pub face_order_and_winding_match: bool,
    pub material_transitions_match: bool,
    pub first_face_mismatch: Option<usize>,
    pub first_material_mismatch: Option<usize>,
}

pub fn output_topology_quality(
    canonical: &DazGeometry,
    output: &OrderedObjMesh,
) -> TopologyQuality {
    let vertex_count_match = canonical.vertices.len() == output.vertices.len();
    let polygon_count_match = canonical.faces.len() == output.faces.len();

    let first_face_mismatch = canonical
        .faces
        .iter()
        .zip(&output.faces)
        .position(|(expected, actual)| expected != &actual.vertex_indices)
        .or_else(|| {
            (!polygon_count_match).then_some(canonical.faces.len().min(output.faces.len()))
        });

    let first_material_mismatch = canonical
        .material_group_indices
        .iter()
        .zip(&output.faces)
        .position(|(&material_id, actual)| {
            let expected = canonical
                .material_groups
                .get(material_id as usize)
                .map(String::as_str);
            actual.material.as_deref() != expected || actual.group.as_deref() != expected
        })
        .or_else(|| {
            (!polygon_count_match).then_some(canonical.faces.len().min(output.faces.len()))
        });

    let face_order_and_winding_match = first_face_mismatch.is_none() && polygon_count_match;
    let material_transitions_match = first_material_mismatch.is_none() && polygon_count_match;
    TopologyQuality {
        valid: vertex_count_match
            && polygon_count_match
            && face_order_and_winding_match
            && material_transitions_match,
        vertex_count_match,
        polygon_count_match,
        face_order_and_winding_match,
        material_transitions_match,
        first_face_mismatch,
        first_material_mismatch,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::formats::{DazGeometry, ObjFace};

    fn canonical() -> DazGeometry {
        DazGeometry::new(
            "fixture".into(),
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![vec![0, 1, 2], vec![0, 2, 3]],
            crate::formats::GroupTable {
                indices: vec![0, 0],
                names: vec!["Head".into()],
            },
            crate::formats::GroupTable {
                indices: vec![0, 1],
                names: vec!["Face".into(), "Lips".into()],
            },
            json!({}),
        )
        .unwrap()
    }

    fn output() -> OrderedObjMesh {
        OrderedObjMesh {
            vertices: canonical().vertices,
            faces: vec![
                ObjFace {
                    vertex_indices: vec![0, 1, 2],
                    group: Some("Face".into()),
                    material: Some("Face".into()),
                },
                ObjFace {
                    vertex_indices: vec![0, 2, 3],
                    group: Some("Lips".into()),
                    material: Some("Lips".into()),
                },
            ],
        }
    }

    #[test]
    fn exact_topology_and_material_order_pass() {
        assert!(output_topology_quality(&canonical(), &output()).valid);
    }

    #[test]
    fn reversed_face_and_material_substitution_fail_at_their_indices() {
        let mut changed = output();
        changed.faces[0].vertex_indices.reverse();
        changed.faces[1].material = Some("Face".into());
        changed.faces[1].group = Some("Face".into());
        let report = output_topology_quality(&canonical(), &changed);
        assert!(!report.valid);
        assert_eq!(report.first_face_mismatch, Some(0));
        assert_eq!(report.first_material_mismatch, Some(1));
    }
}
