use std::collections::BTreeMap;
use std::path::Path;

use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{G2F_POLYGON_COUNT, G2F_VERTEX_COUNT};

use super::{DazGeometry, FormatError, OrderedObjMesh, Result, load_ordered_obj};

const TOPOLOGY_PREFIX: &[u8] = b"vkit.g2f.topology.v1\0";
const MINIMUM_G2_HEIGHT_CM: f64 = 150.0;
const MAXIMUM_G2_HEIGHT_CM: f64 = 220.0;
const UNIT_SCALE_CANDIDATES: [f64; 5] = [0.01, 0.1, 1.0, 10.0, 100.0];

pub const CANONICAL_G2_HEIGHT_CM: f64 = 179.497_38;
pub const CANONICAL_G2F_HEIGHT_CM: f64 = CANONICAL_G2_HEIGHT_CM;

pub const G2F_TOPOLOGY_SHA256: [u8; 32] = [
    0xe0, 0x24, 0x04, 0x74, 0x44, 0x68, 0xf4, 0x0f, 0x69, 0x1b, 0x3e, 0xb7, 0xb1, 0x88, 0xab, 0xdd,
    0x6d, 0x96, 0xa7, 0x48, 0x39, 0xea, 0x53, 0x49, 0x2e, 0xa8, 0x83, 0x45, 0xdf, 0x09, 0x7f, 0x5d,
];

const REQUIRED_G2F_MATERIALS: &[&str] = &[
    "Face",
    "Head",
    "Neck",
    "Ears",
    "Lips",
    "Nostrils",
    "Lacrimals",
    "Pupils",
    "Irises",
    "Cornea",
    "Sclera",
    "EyeReflection",
    "Tear",
    "Eyelashes",
    "Gums",
    "Teeth",
    "Tongue",
    "InnerMouth",
];

const REQUIRED_G2F_POLYGON_GROUPS: &[&str] = &["lEye", "rEye", "upperJaw", "lowerJaw", "tongue"];

#[derive(Clone, Debug, PartialEq)]
pub struct G2TemplateNormalizationReceipt {
    pub source_height: f64,
    pub source_unit_scale: f64,
    pub unit_normalized_height_cm: f64,
    pub height_scale_to_canonical: f64,
    pub applied_uniform_scale: f64,
    pub normalized_height_cm: f64,
    pub dominant_axis: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct G2TemplateImportReceipt {
    pub vertex_count: usize,
    pub polygon_count: usize,
    pub normalization: G2TemplateNormalizationReceipt,
    pub topology_sha256: [u8; 32],
    pub polygon_group_count: usize,
    pub material_group_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct G2TemplateImport {
    pub geometry: DazGeometry,
    pub receipt: G2TemplateImportReceipt,
}

pub type G2ObjImportReceipt = G2TemplateImportReceipt;
pub type G2ObjImport = G2TemplateImport;

#[derive(Clone, Copy)]
struct G2TemplateContract<'a> {
    vertex_count: usize,
    polygon_count: usize,
    topology_sha256: Option<[u8; 32]>,
    minimum_height_cm: f64,
    maximum_height_cm: f64,
    required_materials: &'a [&'a str],
    required_polygon_groups: &'a [&'a str],
}

const CANONICAL_CONTRACT: G2TemplateContract<'static> = G2TemplateContract {
    vertex_count: G2F_VERTEX_COUNT,
    polygon_count: G2F_POLYGON_COUNT,
    topology_sha256: Some(G2F_TOPOLOGY_SHA256),
    minimum_height_cm: MINIMUM_G2_HEIGHT_CM,
    maximum_height_cm: MAXIMUM_G2_HEIGHT_CM,
    required_materials: REQUIRED_G2F_MATERIALS,
    required_polygon_groups: REQUIRED_G2F_POLYGON_GROUPS,
};

const STRUCTURAL_G2_CONTRACT: G2TemplateContract<'static> = G2TemplateContract {
    vertex_count: G2F_VERTEX_COUNT,
    polygon_count: G2F_POLYGON_COUNT,
    topology_sha256: None,
    minimum_height_cm: MINIMUM_G2_HEIGHT_CM,
    maximum_height_cm: MAXIMUM_G2_HEIGHT_CM,
    required_materials: REQUIRED_G2F_MATERIALS,
    required_polygon_groups: REQUIRED_G2F_POLYGON_GROUPS,
};

pub fn load_g2_template_obj_path(path: impl AsRef<Path>) -> Result<G2TemplateImport> {
    convert_g2_template(&load_ordered_obj(path)?)
}

pub fn convert_g2_template(source: &OrderedObjMesh) -> Result<G2TemplateImport> {
    convert_with_contract(source, CANONICAL_CONTRACT)
}

pub fn convert_structural_g2_template(source: &OrderedObjMesh) -> Result<G2TemplateImport> {
    convert_with_contract(source, STRUCTURAL_G2_CONTRACT)
}

pub fn load_g2f_obj_path(path: impl AsRef<Path>) -> Result<G2ObjImport> {
    load_g2_template_obj_path(path)
}

pub fn convert_g2f_obj(source: &OrderedObjMesh) -> Result<G2ObjImport> {
    convert_g2_template(source)
}

pub fn canonical_g2f_topology_digest(faces: &[Vec<u32>]) -> Result<[u8; 32]> {
    topology_digest(G2F_VERTEX_COUNT, faces)
}

pub fn matches_canonical_g2f_topology(vertex_count: usize, faces: &[Vec<u32>]) -> Result<bool> {
    if vertex_count != G2F_VERTEX_COUNT || faces.len() != G2F_POLYGON_COUNT {
        return Ok(false);
    }
    Ok(canonical_g2f_topology_digest(faces)? == G2F_TOPOLOGY_SHA256)
}

fn convert_with_contract(
    source: &OrderedObjMesh,
    contract: G2TemplateContract<'_>,
) -> Result<G2TemplateImport> {
    source.validate()?;
    if source.vertices.len() != contract.vertex_count {
        return Err(g2_error(format!(
            "expected {} vertices, got {}",
            contract.vertex_count,
            source.vertices.len()
        )));
    }
    if source.faces.len() != contract.polygon_count {
        return Err(g2_error(format!(
            "expected {} polygons, got {}",
            contract.polygon_count,
            source.faces.len()
        )));
    }
    for (face_id, face) in source.faces.iter().enumerate() {
        if !matches!(face.vertex_indices.len(), 3 | 4) {
            return Err(g2_error(format!(
                "polygon {face_id} has {} corners; canonical G2 supports only triangles and quads",
                face.vertex_indices.len()
            )));
        }
    }

    let faces = source
        .faces
        .iter()
        .map(|face| face.vertex_indices.clone())
        .collect::<Vec<_>>();
    let topology_sha256 = topology_digest(contract.vertex_count, &faces)?;
    if contract
        .topology_sha256
        .is_some_and(|expected| topology_sha256 != expected)
    {
        return Err(g2_error(
            "polygon order, membership, or winding differs from canonical G2F",
        ));
    }

    let normalization = normalization_receipt(
        &source.vertices,
        contract.minimum_height_cm,
        contract.maximum_height_cm,
        CANONICAL_G2_HEIGHT_CM,
    )?;

    let mut polygon_groups = Vec::<String>::new();
    let mut polygon_group_lookup = BTreeMap::<String, u32>::new();
    let mut material_groups = Vec::<String>::new();
    let mut material_group_lookup = BTreeMap::<String, u32>::new();
    let mut polygon_group_indices = Vec::with_capacity(source.faces.len());
    let mut material_group_indices = Vec::with_capacity(source.faces.len());
    for (face_id, face) in source.faces.iter().enumerate() {
        let group = face
            .group
            .as_deref()
            .ok_or_else(|| g2_error(format!("polygon {face_id} has no active OBJ group (`g`)")))?;
        let material = face.material.as_deref().ok_or_else(|| {
            g2_error(format!(
                "polygon {face_id} has no active OBJ material (`usemtl`)"
            ))
        })?;
        polygon_group_indices.push(intern_first_seen(
            group,
            &mut polygon_groups,
            &mut polygon_group_lookup,
        )?);
        material_group_indices.push(intern_first_seen(
            material,
            &mut material_groups,
            &mut material_group_lookup,
        )?);
    }
    require_labels(&material_groups, contract.required_materials, "material")?;
    require_labels(
        &polygon_groups,
        contract.required_polygon_groups,
        "polygon group",
    )?;

    let vertices = source
        .vertices
        .iter()
        .map(|point| {
            [
                point[0] * normalization.applied_uniform_scale,
                point[1] * normalization.applied_uniform_scale,
                point[2] * normalization.applied_uniform_scale,
            ]
        })
        .collect();
    let geometry = DazGeometry::new(
        "Genesis2-OBJ".to_owned(),
        vertices,
        faces,
        crate::formats::GroupTable {
            indices: polygon_group_indices,
            names: polygon_groups,
        },
        crate::formats::GroupTable {
            indices: material_group_indices,
            names: material_groups,
        },
        json!({
            "vkit_import": {
                "source": "OBJ",
                "unit_scale_to_cm": normalization.source_unit_scale,
                "height_scale_to_canonical": normalization.height_scale_to_canonical,
                "uniform_scale": normalization.applied_uniform_scale
            }
        }),
    )?;
    let receipt = G2TemplateImportReceipt {
        vertex_count: geometry.vertices.len(),
        polygon_count: geometry.faces.len(),
        normalization,
        topology_sha256,
        polygon_group_count: geometry.polygon_groups.len(),
        material_group_count: geometry.material_groups.len(),
    };
    Ok(G2TemplateImport { geometry, receipt })
}

pub fn topology_digest(vertex_count: usize, faces: &[Vec<u32>]) -> Result<[u8; 32]> {
    let mut digest = Sha256::new();
    digest.update(TOPOLOGY_PREFIX);
    digest.update((vertex_count as u64).to_le_bytes());
    digest.update((faces.len() as u64).to_le_bytes());
    for (face_id, face) in faces.iter().enumerate() {
        if !matches!(face.len(), 3 | 4) {
            return Err(g2_error(format!(
                "polygon {face_id} has {} corners; expected three or four",
                face.len()
            )));
        }
        digest.update([face.len() as u8]);
        for &index in face {
            if index as usize >= vertex_count {
                return Err(g2_error(format!(
                    "polygon {face_id} references vertex {index}, but only {vertex_count} vertices exist"
                )));
            }
            digest.update(index.to_le_bytes());
        }
    }
    Ok(digest.finalize().into())
}

pub fn normalize_g2_template_geometry(
    geometry: &mut DazGeometry,
) -> Result<G2TemplateNormalizationReceipt> {
    geometry.validate()?;
    if !matches_canonical_g2f_topology(geometry.vertices.len(), &geometry.faces)? {
        return Err(g2_error(format!(
            "expected canonical Genesis 2 ordered topology ({} vertices, {} polygons)",
            G2F_VERTEX_COUNT, G2F_POLYGON_COUNT
        )));
    }
    require_used_labels(
        &geometry.material_groups,
        &geometry.material_group_indices,
        REQUIRED_G2F_MATERIALS,
        "material",
    )?;
    require_used_labels(
        &geometry.polygon_groups,
        &geometry.polygon_group_indices,
        REQUIRED_G2F_POLYGON_GROUPS,
        "polygon group",
    )?;
    let receipt = scale_geometry_to_height(
        geometry,
        MINIMUM_G2_HEIGHT_CM,
        MAXIMUM_G2_HEIGHT_CM,
        CANONICAL_G2_HEIGHT_CM,
    )?;
    geometry.validate()?;
    Ok(receipt)
}

pub fn matches_canonical_g2_topology(vertex_count: usize, faces: &[Vec<u32>]) -> Result<bool> {
    matches_canonical_g2f_topology(vertex_count, faces)
}

pub fn normalize_g2m_template_geometry(
    geometry: &mut DazGeometry,
) -> Result<G2TemplateNormalizationReceipt> {
    geometry.validate()?;
    validate_male_structural_geometry(geometry)?;
    let receipt = scale_geometry_to_height(
        geometry,
        MINIMUM_G2_HEIGHT_CM,
        MAXIMUM_G2_HEIGHT_CM,
        CANONICAL_G2_HEIGHT_CM,
    )?;
    geometry.validate()?;
    Ok(receipt)
}

fn validate_male_structural_geometry(geometry: &DazGeometry) -> Result<()> {
    if geometry.vertices.len() != G2F_VERTEX_COUNT || geometry.faces.len() != G2F_POLYGON_COUNT {
        return Err(g2_error(format!(
            "expected G2 base counts ({} vertices, {} polygons)",
            G2F_VERTEX_COUNT, G2F_POLYGON_COUNT
        )));
    }
    topology_digest(geometry.vertices.len(), &geometry.faces)?;
    require_used_labels(
        &geometry.material_groups,
        &geometry.material_group_indices,
        REQUIRED_G2F_MATERIALS,
        "material",
    )?;
    require_used_labels(
        &geometry.polygon_groups,
        &geometry.polygon_group_indices,
        REQUIRED_G2F_POLYGON_GROUPS,
        "polygon group",
    )
}

fn scale_geometry_to_height(
    geometry: &mut DazGeometry,
    minimum_height_cm: f64,
    maximum_height_cm: f64,
    canonical_height_cm: f64,
) -> Result<G2TemplateNormalizationReceipt> {
    let receipt = normalization_receipt(
        &geometry.vertices,
        minimum_height_cm,
        maximum_height_cm,
        canonical_height_cm,
    )?;
    for point in &mut geometry.vertices {
        for coordinate in point {
            *coordinate *= receipt.applied_uniform_scale;
        }
    }
    for bone in &mut geometry.bones {
        for point in [&mut bone.center_point, &mut bone.end_point] {
            for coordinate in point {
                *coordinate *= receipt.applied_uniform_scale;
            }
        }
    }
    Ok(receipt)
}

fn normalization_receipt(
    vertices: &[[f64; 3]],
    minimum_height_cm: f64,
    maximum_height_cm: f64,
    canonical_height_cm: f64,
) -> Result<G2TemplateNormalizationReceipt> {
    if !canonical_height_cm.is_finite() || canonical_height_cm <= 0.0 {
        return Err(g2_error("canonical height must be positive and finite"));
    }
    let extents = extents(vertices)?;
    let dominant_axis = (0..3)
        .max_by(|&left, &right| extents[left].total_cmp(&extents[right]))
        .expect("three axes are always present");
    if dominant_axis != 1
        || extents[1] <= extents[0] * (1.0 + 1.0e-9)
        || extents[1] <= extents[2] * (1.0 + 1.0e-9)
    {
        return Err(g2_error(format!(
            "expected a uniquely Y-up full-body mesh, got bounds extents {extents:?}"
        )));
    }
    let source_height = extents[1];
    let source_unit_scale = infer_unit_scale(source_height, minimum_height_cm, maximum_height_cm)?;
    let unit_normalized_height_cm = source_height * source_unit_scale;
    let height_scale_to_canonical = canonical_height_cm / unit_normalized_height_cm;
    let applied_uniform_scale = source_unit_scale * height_scale_to_canonical;
    if !height_scale_to_canonical.is_finite() || !applied_uniform_scale.is_finite() {
        return Err(g2_error("normalization scale is not finite"));
    }
    Ok(G2TemplateNormalizationReceipt {
        source_height,
        source_unit_scale,
        unit_normalized_height_cm,
        height_scale_to_canonical,
        applied_uniform_scale,
        normalized_height_cm: canonical_height_cm,
        dominant_axis,
    })
}

fn infer_unit_scale(source_height: f64, minimum: f64, maximum: f64) -> Result<f64> {
    if !source_height.is_finite() || source_height <= 0.0 {
        return Err(g2_error("cannot infer units from a non-positive height"));
    }
    let candidates = UNIT_SCALE_CANDIDATES
        .into_iter()
        .filter(|scale| (minimum..=maximum).contains(&(source_height * scale)))
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [scale] => Ok(*scale),
        [] => Err(g2_error(format!(
            "height {source_height} does not normalize to {minimum}..={maximum} cm using a supported decimal unit scale"
        ))),
        _ => Err(g2_error(format!(
            "height {source_height} has ambiguous decimal unit scales {candidates:?}"
        ))),
    }
}

fn extents(vertices: &[[f64; 3]]) -> Result<[f64; 3]> {
    let Some(first) = vertices.first().copied() else {
        return Err(g2_error("OBJ contains no vertices"));
    };
    let mut minimum = first;
    let mut maximum = first;
    for point in &vertices[1..] {
        if !point.iter().all(|value| value.is_finite()) {
            return Err(g2_error("OBJ contains a non-finite vertex"));
        }
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(point[axis]);
            maximum[axis] = maximum[axis].max(point[axis]);
        }
    }
    Ok([
        maximum[0] - minimum[0],
        maximum[1] - minimum[1],
        maximum[2] - minimum[2],
    ])
}

fn intern_first_seen(
    value: &str,
    table: &mut Vec<String>,
    lookup: &mut BTreeMap<String, u32>,
) -> Result<u32> {
    if let Some(index) = lookup.get(value) {
        return Ok(*index);
    }
    let index = u32::try_from(table.len())
        .map_err(|_| g2_error("OBJ label table exceeds the u32 range"))?;
    let owned = value.to_owned();
    table.push(owned.clone());
    lookup.insert(owned, index);
    Ok(index)
}

fn require_labels(actual: &[String], required: &[&str], kind: &str) -> Result<()> {
    for &required in required {
        if !actual
            .iter()
            .any(|value| value.eq_ignore_ascii_case(required))
        {
            return Err(g2_error(format!(
                "required anatomy {kind} {required:?} is missing"
            )));
        }
    }
    Ok(())
}

fn require_used_labels(
    labels: &[String],
    indices: &[u32],
    required: &[&str],
    kind: &str,
) -> Result<()> {
    for &required in required {
        let used = indices.iter().any(|&index| {
            labels
                .get(index as usize)
                .is_some_and(|value| value.eq_ignore_ascii_case(required))
        });
        if !used {
            return Err(g2_error(format!(
                "required anatomy {kind} {required:?} is missing or unused"
            )));
        }
    }
    Ok(())
}

fn g2_error(message: impl Into<String>) -> FormatError {
    FormatError::InvalidG2Template(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_domains_keep_their_original_spelling() {
        assert_eq!(TOPOLOGY_PREFIX, b"vkit.g2f.topology.v1\x00");
    }
    use crate::formats::ObjFace;

    fn fixture() -> (OrderedObjMesh, G2TemplateContract<'static>) {
        let vertices = vec![
            [-0.5, 0.0, -0.25],
            [0.5, 0.0, -0.25],
            [0.0, 2.0, -0.25],
            [-0.4, 0.2, 0.25],
            [0.4, 0.2, 0.25],
            [0.0, 1.8, 0.25],
        ];
        let faces = vec![
            ObjFace {
                vertex_indices: vec![0, 1, 2],
                group: Some("lEye".into()),
                material: Some("Face".into()),
            },
            ObjFace {
                vertex_indices: vec![3, 4, 5],
                group: Some("rEye".into()),
                material: Some("Nostrils".into()),
            },
        ];
        let topology = topology_digest(
            vertices.len(),
            &faces
                .iter()
                .map(|f| f.vertex_indices.clone())
                .collect::<Vec<_>>(),
        )
        .unwrap();
        (
            OrderedObjMesh { vertices, faces },
            G2TemplateContract {
                vertex_count: 6,
                polygon_count: 2,
                topology_sha256: Some(topology),
                minimum_height_cm: 150.0,
                maximum_height_cm: 220.0,
                required_materials: &["Face", "Nostrils"],
                required_polygon_groups: &["lEye", "rEye"],
            },
        )
    }

    #[test]
    fn synthetic_contract_normalizes_units_and_preserves_first_seen_labels() {
        let (source, contract) = fixture();
        let imported = convert_with_contract(&source, contract).unwrap();
        assert_eq!(imported.receipt.normalization.source_unit_scale, 100.0);
        assert_eq!(
            imported.receipt.normalization.unit_normalized_height_cm,
            200.0
        );
        assert_eq!(
            imported.receipt.normalization.normalized_height_cm,
            CANONICAL_G2_HEIGHT_CM
        );
        assert_eq!(imported.geometry.polygon_groups, ["lEye", "rEye"]);
        assert_eq!(imported.geometry.material_groups, ["Face", "Nostrils"]);
        assert_eq!(imported.geometry.vertices[2][1], CANONICAL_G2_HEIGHT_CM);
    }

    #[test]
    fn reordered_reversed_and_missing_metadata_are_rejected() {
        let (source, contract) = fixture();

        let mut reordered = source.clone();
        reordered.faces.swap(0, 1);
        assert!(convert_with_contract(&reordered, contract).is_err());

        let mut reversed = source.clone();
        reversed.faces[0].vertex_indices.swap(1, 2);
        assert!(convert_with_contract(&reversed, contract).is_err());

        let mut missing_required_label = source.clone();
        missing_required_label.faces[1].material = Some("Face".into());
        assert!(convert_with_contract(&missing_required_label, contract).is_err());

        let mut missing_group = source.clone();
        missing_group.faces[0].group = None;
        assert!(convert_with_contract(&missing_group, contract).is_err());

        let mut missing_material = source;
        missing_material.faces[0].material = None;
        assert!(convert_with_contract(&missing_material, contract).is_err());
    }

    #[test]
    fn ambiguous_units_and_up_axis_are_rejected() {
        assert!(infer_unit_scale(200.0, 1.0, 30_000.0).is_err());

        let (mut source, contract) = fixture();
        source.vertices[0][0] = -1.0;
        source.vertices[1][0] = 1.0;
        assert!(convert_with_contract(&source, contract).is_err());
    }

    #[test]
    fn generic_topology_does_not_claim_canonical_g2_identity() {
        assert!(!matches_canonical_g2f_topology(6, &[vec![0, 1, 2]]).unwrap());
    }

    fn structural_g2_fixture() -> OrderedObjMesh {
        let mut vertices = vec![[0.0, 90.0, 0.0]; G2F_VERTEX_COUNT];
        vertices[0] = [-30.0, 0.0, -15.0];
        vertices[1] = [30.0, CANONICAL_G2_HEIGHT_CM, 15.0];
        let faces = (0..G2F_POLYGON_COUNT)
            .map(|face_index| ObjFace {
                vertex_indices: vec![
                    (face_index % G2F_VERTEX_COUNT) as u32,
                    ((face_index + 1) % G2F_VERTEX_COUNT) as u32,
                    ((face_index + 2) % G2F_VERTEX_COUNT) as u32,
                ],
                group: Some(
                    REQUIRED_G2F_POLYGON_GROUPS[face_index % REQUIRED_G2F_POLYGON_GROUPS.len()]
                        .to_owned(),
                ),
                material: Some(
                    REQUIRED_G2F_MATERIALS[face_index % REQUIRED_G2F_MATERIALS.len()].to_owned(),
                ),
            })
            .collect();
        OrderedObjMesh { vertices, faces }
    }

    #[test]
    fn non_g2f_structural_conversion_is_sex_neutral() {
        let source = structural_g2_fixture();
        assert!(
            !matches_canonical_g2f_topology(source.vertices.len(), &source_faces(&source)).unwrap()
        );
        let imported = convert_structural_g2_template(&source).unwrap();
        assert_eq!(imported.geometry.geometry_id, "Genesis2-OBJ");
        assert!(
            !imported
                .geometry
                .geometry_id
                .to_ascii_lowercase()
                .contains("male")
        );
        assert!(
            !imported
                .geometry
                .geometry_id
                .to_ascii_lowercase()
                .contains("female")
        );
    }

    #[test]
    fn exact_g2f_converter_remains_digest_bound() {
        let source = structural_g2_fixture();
        let error = convert_g2_template(&source).unwrap_err().to_string();
        assert!(error.contains("canonical G2F"), "{error}");
    }

    fn source_faces(source: &OrderedObjMesh) -> Vec<Vec<u32>> {
        source
            .faces
            .iter()
            .map(|face| face.vertex_indices.clone())
            .collect()
    }

    fn scale_fixture(source_height: f64) -> DazGeometry {
        let mut geometry = DazGeometry::new(
            "synthetic-g2".into(),
            vec![
                [-source_height * 0.2, 0.0, -source_height * 0.1],
                [source_height * 0.2, source_height, source_height * 0.1],
                [
                    source_height * 0.05,
                    source_height * 0.5,
                    -source_height * 0.025,
                ],
            ],
            vec![vec![0, 1, 2]],
            crate::formats::GroupTable {
                indices: vec![0],
                names: vec!["body".into()],
            },
            crate::formats::GroupTable {
                indices: vec![0],
                names: vec!["skin".into()],
            },
            json!({}),
        )
        .unwrap();
        geometry.bones.push(super::super::DazBone {
            id: "head".into(),
            center_point: [source_height * 0.05, source_height * 0.8, 0.0],
            end_point: [source_height * 0.05, source_height * 0.9, 0.0],
        });
        geometry
    }

    #[test]
    fn metre_centimetre_and_millimetre_inputs_use_one_uniform_scale() {
        for (source_height, expected_unit_scale) in [(1.8, 100.0), (180.0, 1.0), (1_800.0, 0.1)] {
            let mut geometry = scale_fixture(source_height);
            let source = geometry.vertices[2];
            let receipt = scale_geometry_to_height(
                &mut geometry,
                MINIMUM_G2_HEIGHT_CM,
                MAXIMUM_G2_HEIGHT_CM,
                CANONICAL_G2_HEIGHT_CM,
            )
            .unwrap();
            assert_eq!(receipt.source_unit_scale, expected_unit_scale);
            assert!(
                (geometry.vertices[2][0] - source[0] * receipt.applied_uniform_scale).abs()
                    < 1.0e-12
            );
            assert!(
                (geometry.vertices[2][1] - source[1] * receipt.applied_uniform_scale).abs()
                    < 1.0e-12
            );
            assert!(
                (geometry.vertices[2][2] - source[2] * receipt.applied_uniform_scale).abs()
                    < 1.0e-12
            );
            let normalized_height = geometry.vertices[1][1] - geometry.vertices[0][1];
            assert!((normalized_height - CANONICAL_G2_HEIGHT_CM).abs() < 1.0e-10);
        }
    }

    #[test]
    fn dsf_bone_points_receive_the_geometry_uniform_scale() {
        let mut geometry = scale_fixture(1.8);
        let center = geometry.bones[0].center_point;
        let end = geometry.bones[0].end_point;
        let receipt = scale_geometry_to_height(
            &mut geometry,
            MINIMUM_G2_HEIGHT_CM,
            MAXIMUM_G2_HEIGHT_CM,
            CANONICAL_G2_HEIGHT_CM,
        )
        .unwrap();
        for axis in 0..3 {
            assert!(
                (geometry.bones[0].center_point[axis]
                    - center[axis] * receipt.applied_uniform_scale)
                    .abs()
                    < 1.0e-12
            );
            assert!(
                (geometry.bones[0].end_point[axis] - end[axis] * receipt.applied_uniform_scale)
                    .abs()
                    < 1.0e-12
            );
        }
    }
}
