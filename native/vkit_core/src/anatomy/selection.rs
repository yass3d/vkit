use std::collections::BTreeSet;

use crate::formats::DazGeometry;

use super::AnatomyError;

pub fn vertices_from_face_mask(
    geometry: &DazGeometry,
    face_mask: &[bool],
) -> Result<Vec<usize>, AnatomyError> {
    if face_mask.len() != geometry.faces.len() {
        return Err(AnatomyError::FaceMaskLengthMismatch {
            expected: geometry.faces.len(),
            actual: face_mask.len(),
        });
    }
    let mut indices = BTreeSet::new();
    for (face, selected) in geometry.faces.iter().zip(face_mask.iter().copied()) {
        if selected {
            indices.extend(face.iter().map(|&index| index as usize));
        }
    }
    Ok(indices.into_iter().collect())
}

pub fn material_vertices(
    geometry: &DazGeometry,
    names: &[&str],
) -> Result<Vec<usize>, AnatomyError> {
    let mask = geometry.face_mask_for_materials(names.iter().copied());
    vertices_from_face_mask(geometry, &mask)
}

pub fn polygon_group_vertices(
    geometry: &DazGeometry,
    names: &[&str],
) -> Result<Vec<usize>, AnatomyError> {
    let mask = geometry.face_mask_for_polygon_groups(names.iter().copied());
    vertices_from_face_mask(geometry, &mask)
}

pub fn material_and_polygon_group_vertices(
    geometry: &DazGeometry,
    materials: &[&str],
    polygon_groups: &[&str],
) -> Result<Vec<usize>, AnatomyError> {
    let material_mask = geometry.face_mask_for_materials(materials.iter().copied());
    let polygon_mask = geometry.face_mask_for_polygon_groups(polygon_groups.iter().copied());
    let combined: Vec<_> = material_mask
        .into_iter()
        .zip(polygon_mask)
        .map(|(material, polygon)| material && polygon)
        .collect();
    vertices_from_face_mask(geometry, &combined)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn geometry() -> DazGeometry {
        DazGeometry::new(
            "fixture".into(),
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![vec![2, 0, 1], vec![3, 2, 1]],
            crate::formats::GroupTable {
                indices: vec![0, 1],
                names: vec!["head".into(), "mouth".into()],
            },
            crate::formats::GroupTable {
                indices: vec![0, 1],
                names: vec!["Face".into(), "Teeth".into()],
            },
            json!({}),
        )
        .unwrap()
    }

    #[test]
    fn selections_are_unique_sorted_and_case_insensitive() {
        let geometry = geometry();
        assert_eq!(
            material_vertices(&geometry, &["face"]).unwrap(),
            vec![0, 1, 2]
        );
        assert_eq!(
            polygon_group_vertices(&geometry, &["MOUTH"]).unwrap(),
            vec![1, 2, 3]
        );
        assert_eq!(
            material_and_polygon_group_vertices(&geometry, &["teeth"], &["mouth"]).unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn mask_length_is_rejected() {
        assert!(matches!(
            vertices_from_face_mask(&geometry(), &[true]),
            Err(AnatomyError::FaceMaskLengthMismatch { .. })
        ));
    }
}
