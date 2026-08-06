use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{DazGeometry, FormatError, Result};

pub const EYE_GLOBE_MATERIALS: &[&str] = &["Sclera", "Cornea", "Irises", "Pupils", "EyeReflection"];

const TEETH_MATERIAL: &str = "Teeth";
const GUMS_MATERIAL: &str = "Gums";
const TONGUE_MATERIAL: &str = "Tongue";

pub const CANONICAL_HEAD_GROUP_NAMES: [&str; 5] =
    ["lEye", "rEye", "upperJaw", "lowerJaw", "tongue"];

pub const CANONICAL_HEAD_GROUPS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CanonicalHeadGroupFaces {
    pub left_eye: Vec<u32>,
    pub right_eye: Vec<u32>,
    pub upper_jaw: Vec<u32>,
    pub lower_jaw: Vec<u32>,
    pub tongue: Vec<u32>,
}

impl CanonicalHeadGroupFaces {
    fn named_sets(&self) -> [(&'static str, &Vec<u32>); 5] {
        [
            ("lEye", &self.left_eye),
            ("rEye", &self.right_eye),
            ("upperJaw", &self.upper_jaw),
            ("lowerJaw", &self.lower_jaw),
            ("tongue", &self.tongue),
        ]
    }

    fn validate_for(&self, face_count: usize) -> Result<()> {
        let mut seen = HashSet::new();
        for (name, faces) in self.named_sets() {
            if faces.is_empty() {
                return Err(group_error(format!(
                    "canonical head group {name} derived no faces"
                )));
            }
            for &face in faces {
                if face as usize >= face_count {
                    return Err(group_error(format!(
                        "canonical head group {name} references face {face}, but only {face_count} faces exist"
                    )));
                }
                if !seen.insert(face) {
                    return Err(group_error(format!(
                        "canonical head group {name} overlaps another canonical group at face {face}"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalHeadGroupsDocument {
    pub schema_version: u32,

    pub topology_sha256: String,

    pub source: String,
    pub left_eye_faces: Vec<u32>,
    pub right_eye_faces: Vec<u32>,
    pub upper_jaw_faces: Vec<u32>,
    pub lower_jaw_faces: Vec<u32>,
    pub tongue_faces: Vec<u32>,
}

impl CanonicalHeadGroupsDocument {
    pub fn new(
        topology_sha256_hex: String,
        source: String,
        faces: &CanonicalHeadGroupFaces,
    ) -> Self {
        Self {
            schema_version: CANONICAL_HEAD_GROUPS_SCHEMA_VERSION,
            topology_sha256: topology_sha256_hex,
            source,
            left_eye_faces: faces.left_eye.clone(),
            right_eye_faces: faces.right_eye.clone(),
            upper_jaw_faces: faces.upper_jaw.clone(),
            lower_jaw_faces: faces.lower_jaw.clone(),
            tongue_faces: faces.tongue.clone(),
        }
    }

    pub fn face_sets(&self) -> CanonicalHeadGroupFaces {
        CanonicalHeadGroupFaces {
            left_eye: self.left_eye_faces.clone(),
            right_eye: self.right_eye_faces.clone(),
            upper_jaw: self.upper_jaw_faces.clone(),
            lower_jaw: self.lower_jaw_faces.clone(),
            tongue: self.tongue_faces.clone(),
        }
    }
}

pub fn canonical_head_groups_cache_path(cache_directory: &Path) -> PathBuf {
    cache_directory.join("g2-head-anatomy-v1.json")
}

pub fn write_canonical_head_groups(
    path: &Path,
    document: &CanonicalHeadGroupsDocument,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(document)?;
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, text.as_bytes())?;
    if path.exists() {
        let _ = fs::remove_file(path);
    }
    fs::rename(&temporary, path)?;
    Ok(())
}

pub fn read_canonical_head_groups(
    path: &Path,
    expected_topology_sha256_hex: &str,
    face_count: usize,
) -> Result<CanonicalHeadGroupsDocument> {
    let text = fs::read_to_string(path)?;
    let document: CanonicalHeadGroupsDocument = serde_json::from_str(&text)?;
    if document.schema_version != CANONICAL_HEAD_GROUPS_SCHEMA_VERSION {
        return Err(group_error(format!(
            "canonical head-group cache schema {} is unsupported",
            document.schema_version
        )));
    }
    if document.topology_sha256 != expected_topology_sha256_hex {
        return Err(group_error(
            "canonical head-group cache is bound to a different topology",
        ));
    }
    document.face_sets().validate_for(face_count)?;
    Ok(document)
}

pub fn collect_canonical_head_groups(geometry: &DazGeometry) -> Option<CanonicalHeadGroupFaces> {
    let collect = |required: &str| -> Vec<u32> {
        geometry
            .polygon_group_indices
            .iter()
            .enumerate()
            .filter_map(|(face_index, &group)| {
                geometry
                    .polygon_groups
                    .get(group as usize)
                    .is_some_and(|name| name.eq_ignore_ascii_case(required))
                    .then_some(face_index as u32)
            })
            .collect()
    };
    let faces = CanonicalHeadGroupFaces {
        left_eye: collect("lEye"),
        right_eye: collect("rEye"),
        upper_jaw: collect("upperJaw"),
        lower_jaw: collect("lowerJaw"),
        tongue: collect("tongue"),
    };
    faces
        .named_sets()
        .iter()
        .all(|(_, set)| !set.is_empty())
        .then_some(faces)
}

pub fn has_canonical_head_groups(geometry: &DazGeometry) -> bool {
    CANONICAL_HEAD_GROUP_NAMES.iter().all(|required| {
        geometry.polygon_group_indices.iter().any(|&index| {
            geometry
                .polygon_groups
                .get(index as usize)
                .is_some_and(|name| name.eq_ignore_ascii_case(required))
        })
    })
}

pub fn derive_canonical_head_group_faces(
    geometry: &DazGeometry,
) -> Result<CanonicalHeadGroupFaces> {
    geometry.validate()?;
    let material_of = |face_index: usize| -> Option<&str> {
        geometry
            .material_groups
            .get(geometry.material_group_indices[face_index] as usize)
            .map(String::as_str)
    };

    let mut left_eye = Vec::new();
    let mut right_eye = Vec::new();
    for face_index in 0..geometry.faces.len() {
        let Some(material) = material_of(face_index) else {
            continue;
        };
        if !matches_any(material, EYE_GLOBE_MATERIALS) {
            continue;
        }
        if face_centroid(geometry, face_index)[0] >= 0.0 {
            left_eye.push(face_index as u32);
        } else {
            right_eye.push(face_index as u32);
        }
    }

    let mouth_faces: Vec<usize> = (0..geometry.faces.len())
        .filter(|&face_index| {
            material_of(face_index).is_some_and(|material| {
                matches_any(material, &[TEETH_MATERIAL, GUMS_MATERIAL, TONGUE_MATERIAL])
            })
        })
        .collect();
    if mouth_faces.is_empty() {
        return Err(group_error(
            "geometry has no Teeth/Gums/Tongue faces to derive jaw components from",
        ));
    }
    let component_of = connected_components(geometry, &mouth_faces);

    let mut core_ids = Vec::new();
    for (&face_index, &component) in mouth_faces.iter().zip(&component_of) {
        if matches_any(material_of(face_index).unwrap_or(""), &[GUMS_MATERIAL])
            && !core_ids.contains(&component)
        {
            core_ids.push(component);
        }
    }
    if core_ids.len() != 2 {
        return Err(group_error(format!(
            "expected exactly two gums-cored jaw components, found {}",
            core_ids.len()
        )));
    }
    let gums_mean_y = |core: usize| -> f64 {
        let mut sum = 0.0;
        let mut count = 0usize;
        for (&face_index, &component) in mouth_faces.iter().zip(&component_of) {
            if component == core
                && matches_any(material_of(face_index).unwrap_or(""), &[GUMS_MATERIAL])
            {
                for &vertex in &geometry.faces[face_index] {
                    sum += geometry.vertices[vertex as usize][1];
                    count += 1;
                }
            }
        }
        sum / count.max(1) as f64
    };
    let (upper_core, lower_core) = if gums_mean_y(core_ids[0]) >= gums_mean_y(core_ids[1]) {
        (core_ids[0], core_ids[1])
    } else {
        (core_ids[1], core_ids[0])
    };

    let gums_cloud = |core: usize| -> Vec<[f64; 3]> {
        let mut cloud = Vec::new();
        for (&face_index, &component) in mouth_faces.iter().zip(&component_of) {
            if component == core
                && matches_any(material_of(face_index).unwrap_or(""), &[GUMS_MATERIAL])
            {
                for &vertex in &geometry.faces[face_index] {
                    cloud.push(geometry.vertices[vertex as usize]);
                }
            }
        }
        cloud
    };
    let upper_cloud = gums_cloud(upper_core);
    let lower_cloud = gums_cloud(lower_core);

    let gums_vertices: HashSet<u32> = mouth_faces
        .iter()
        .filter(|&&face_index| matches_any(material_of(face_index).unwrap_or(""), &[GUMS_MATERIAL]))
        .flat_map(|&face_index| geometry.faces[face_index].iter().copied())
        .collect();
    let tongue_faces: Vec<usize> = mouth_faces
        .iter()
        .copied()
        .filter(|&face_index| {
            matches_any(material_of(face_index).unwrap_or(""), &[TONGUE_MATERIAL])
        })
        .collect();
    let mut root_band: HashSet<usize> = tongue_faces
        .iter()
        .copied()
        .filter(|&face_index| {
            geometry.faces[face_index]
                .iter()
                .any(|vertex| gums_vertices.contains(vertex))
        })
        .collect();
    let tongue_edge_neighbors = edge_neighbors(geometry, &tongue_faces);
    loop {
        let additions: Vec<usize> = tongue_faces
            .iter()
            .copied()
            .filter(|face_index| !root_band.contains(face_index))
            .filter(|face_index| {
                tongue_edge_neighbors
                    .get(face_index)
                    .is_some_and(|neighbors| {
                        neighbors
                            .iter()
                            .filter(|neighbor| root_band.contains(neighbor))
                            .count()
                            >= 2
                    })
            })
            .collect();
        if additions.is_empty() {
            break;
        }
        root_band.extend(additions);
    }

    let mut upper_jaw = Vec::new();
    let mut lower_jaw = Vec::new();
    let mut tongue = Vec::new();
    let mut island_assignments = HashMap::<usize, bool>::new();
    for (&face_index, &component) in mouth_faces.iter().zip(&component_of) {
        let material = material_of(face_index).unwrap_or("");
        if matches_any(material, &[TONGUE_MATERIAL]) {
            if root_band.contains(&face_index) {
                lower_jaw.push(face_index as u32);
            } else {
                tongue.push(face_index as u32);
            }
            continue;
        }
        let upper = if component == upper_core {
            true
        } else if component == lower_core {
            false
        } else {
            *island_assignments.entry(component).or_insert_with(|| {
                let island_vertices: Vec<[f64; 3]> = mouth_faces
                    .iter()
                    .zip(&component_of)
                    .filter(|&(_, &island)| island == component)
                    .flat_map(|(&island_face, _)| {
                        geometry.faces[island_face]
                            .iter()
                            .map(|&vertex| geometry.vertices[vertex as usize])
                    })
                    .collect();
                minimum_squared_distance(&island_vertices, &upper_cloud)
                    <= minimum_squared_distance(&island_vertices, &lower_cloud)
            })
        };
        if upper {
            upper_jaw.push(face_index as u32);
        } else {
            lower_jaw.push(face_index as u32);
        }
    }

    for set in [
        &mut left_eye,
        &mut right_eye,
        &mut upper_jaw,
        &mut lower_jaw,
        &mut tongue,
    ] {
        set.sort_unstable();
    }
    let faces = CanonicalHeadGroupFaces {
        left_eye,
        right_eye,
        upper_jaw,
        lower_jaw,
        tongue,
    };
    faces.validate_for(geometry.faces.len())?;
    Ok(faces)
}

pub fn apply_canonical_head_groups(
    geometry: &mut DazGeometry,
    faces: &CanonicalHeadGroupFaces,
) -> Result<()> {
    faces.validate_for(geometry.faces.len())?;
    let mut override_of = HashMap::<u32, &'static str>::new();
    for (name, set) in faces.named_sets() {
        for &face in set {
            override_of.insert(face, name);
        }
    }
    let mut polygon_groups = Vec::<String>::new();
    let mut lookup = HashMap::<String, u32>::new();
    let mut polygon_group_indices = Vec::with_capacity(geometry.faces.len());
    for face_index in 0..geometry.faces.len() {
        let label = match override_of.get(&(face_index as u32)) {
            Some(&canonical) => canonical,
            None => geometry
                .polygon_groups
                .get(geometry.polygon_group_indices[face_index] as usize)
                .map(String::as_str)
                .ok_or_else(|| group_error("polygon-group table is inconsistent"))?,
        };
        let index = match lookup.get(label) {
            Some(&index) => index,
            None => {
                let index = u32::try_from(polygon_groups.len())
                    .map_err(|_| group_error("polygon-group table exceeds the u32 range"))?;
                polygon_groups.push(label.to_owned());
                lookup.insert(label.to_owned(), index);
                index
            }
        };
        polygon_group_indices.push(index);
    }
    geometry.polygon_groups = polygon_groups;
    geometry.polygon_group_indices = polygon_group_indices;
    geometry.validate()
}

pub fn ensure_canonical_head_groups(
    geometry: &mut DazGeometry,
) -> Result<Option<CanonicalHeadGroupFaces>> {
    if has_canonical_head_groups(geometry) {
        return Ok(None);
    }
    let faces = derive_canonical_head_group_faces(geometry)?;
    apply_canonical_head_groups(geometry, &faces)?;
    Ok(Some(faces))
}

fn matches_any(material: &str, names: &[&str]) -> bool {
    names.iter().any(|name| material.eq_ignore_ascii_case(name))
}

fn face_centroid(geometry: &DazGeometry, face_index: usize) -> [f64; 3] {
    let face = &geometry.faces[face_index];
    let mut centroid = [0.0; 3];
    for &vertex in face {
        for (axis, component) in centroid.iter_mut().enumerate() {
            *component += geometry.vertices[vertex as usize][axis];
        }
    }
    for component in &mut centroid {
        *component /= face.len() as f64;
    }
    centroid
}

fn connected_components(geometry: &DazGeometry, selected: &[usize]) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..selected.len()).collect();
    fn find(parent: &mut [usize], mut node: usize) -> usize {
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }
    let mut owner_of_vertex = HashMap::<u32, usize>::new();
    for (position, &face_index) in selected.iter().enumerate() {
        for &vertex in &geometry.faces[face_index] {
            match owner_of_vertex.get(&vertex) {
                Some(&other) => {
                    let left = find(&mut parent, position);
                    let right = find(&mut parent, other);
                    if left != right {
                        parent[left] = right;
                    }
                }
                None => {
                    owner_of_vertex.insert(vertex, position);
                }
            }
        }
    }
    (0..selected.len())
        .map(|position| find(&mut parent, position))
        .collect()
}

fn edge_neighbors(geometry: &DazGeometry, selected: &[usize]) -> HashMap<usize, Vec<usize>> {
    let mut owner_of_edge = HashMap::<(u32, u32), Vec<usize>>::new();
    for &face_index in selected {
        let face = &geometry.faces[face_index];
        for corner in 0..face.len() {
            let a = face[corner];
            let b = face[(corner + 1) % face.len()];
            let edge = (a.min(b), a.max(b));
            owner_of_edge.entry(edge).or_default().push(face_index);
        }
    }
    let mut neighbors = HashMap::<usize, Vec<usize>>::new();
    for owners in owner_of_edge.values() {
        for &face in owners {
            for &other in owners {
                if face != other {
                    neighbors.entry(face).or_default().push(other);
                }
            }
        }
    }
    neighbors
}

fn minimum_squared_distance(from: &[[f64; 3]], to: &[[f64; 3]]) -> f64 {
    let mut best = f64::INFINITY;
    for a in from {
        for b in to {
            let distance = (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
            if distance < best {
                best = distance;
            }
        }
    }
    best
}

fn group_error(message: impl Into<String>) -> FormatError {
    FormatError::InvalidG2Template(message.into())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[derive(Default)]
    struct FixtureBuilder {
        vertices: Vec<[f64; 3]>,
        faces: Vec<Vec<u32>>,
        materials: Vec<String>,
        material_indices: Vec<u32>,
    }

    impl FixtureBuilder {
        fn material_index(&mut self, material: &str) -> u32 {
            match self.materials.iter().position(|known| known == material) {
                Some(index) => index as u32,
                None => {
                    self.materials.push(material.to_owned());
                    (self.materials.len() - 1) as u32
                }
            }
        }

        fn quad(&mut self, x: f64, y: f64, material: &str) -> usize {
            let base = self.vertices.len() as u32;
            self.vertices.extend_from_slice(&[
                [x, y, 0.0],
                [x + 0.5, y, 0.0],
                [x + 0.5, y + 0.5, 0.0],
                [x, y + 0.5, 0.0],
            ]);
            self.faces.push(vec![base, base + 1, base + 2, base + 3]);
            let material = self.material_index(material);
            self.material_indices.push(material);
            self.faces.len() - 1
        }

        fn vertex(&mut self, x: f64, y: f64) -> u32 {
            self.vertices.push([x, y, 0.0]);
            (self.vertices.len() - 1) as u32
        }

        fn triangle(&mut self, corners: [u32; 3], material: &str) -> usize {
            self.faces.push(corners.to_vec());
            let material = self.material_index(material);
            self.material_indices.push(material);
            self.faces.len() - 1
        }

        fn build(self) -> DazGeometry {
            let face_count = self.faces.len();
            DazGeometry::new(
                "fixture".into(),
                self.vertices,
                self.faces,
                crate::formats::GroupTable {
                    indices: vec![0; face_count],
                    names: vec!["body".into()],
                },
                crate::formats::GroupTable {
                    indices: self.material_indices,
                    names: self.materials,
                },
                json!({}),
            )
            .unwrap()
        }
    }

    fn fixture() -> (DazGeometry, Expected) {
        let mut builder = FixtureBuilder::default();

        builder.quad(0.0, 12.0, "Face");
        let left_eye = vec![
            builder.quad(3.0, 10.0, "Sclera"),
            builder.quad(3.6, 10.0, "Cornea"),
        ];
        let right_eye = vec![
            builder.quad(-4.0, 10.0, "Sclera"),
            builder.quad(-3.4, 10.0, "Cornea"),
        ];
        let upper_gums = builder.quad(0.0, 6.0, "Gums");
        let lower_gums = builder.quad(0.0, 2.0, "Gums");
        let upper_teeth = [
            builder.quad(-1.5, 5.2, "Teeth"),
            builder.quad(1.5, 5.2, "Teeth"),
        ];
        let lower_teeth = [
            builder.quad(-1.5, 2.9, "Teeth"),
            builder.quad(1.5, 2.9, "Teeth"),
        ];

        let (gums_a, gums_b) = {
            let face = &builder.faces[lower_gums];
            (face[0], face[1])
        };
        let p = builder.vertex(0.25, 1.5);
        let r = builder.vertex(0.6, 1.5);
        let s = builder.vertex(-0.1, 1.5);
        let t = builder.vertex(0.25, 0.9);
        let ring = vec![
            builder.triangle([gums_a, gums_b, p], "Tongue"),
            builder.triangle([gums_b, r, p], "Tongue"),
            builder.triangle([gums_a, p, s], "Tongue"),
        ];
        let interior = builder.triangle([p, r, s], "Tongue");
        let blade = builder.triangle([r, t, s], "Tongue");
        let mut lower_jaw = vec![lower_gums, lower_teeth[0], lower_teeth[1], interior];
        lower_jaw.extend(ring);
        let expected = Expected {
            left_eye,
            right_eye,
            upper_jaw: vec![upper_gums, upper_teeth[0], upper_teeth[1]],
            lower_jaw,
            blade,
        };
        (builder.build(), expected)
    }

    struct Expected {
        left_eye: Vec<usize>,
        right_eye: Vec<usize>,
        upper_jaw: Vec<usize>,
        lower_jaw: Vec<usize>,
        blade: usize,
    }

    fn sorted(mut faces: Vec<usize>) -> Vec<u32> {
        faces.sort_unstable();
        faces.into_iter().map(|face| face as u32).collect()
    }

    #[test]
    fn derivation_assigns_eyes_jaws_teeth_islands_and_tongue_root() {
        let (geometry, expected) = fixture();
        let derived = derive_canonical_head_group_faces(&geometry).unwrap();
        assert_eq!(derived.left_eye, sorted(expected.left_eye));
        assert_eq!(derived.right_eye, sorted(expected.right_eye));
        assert_eq!(derived.upper_jaw, sorted(expected.upper_jaw));

        assert_eq!(derived.lower_jaw, sorted(expected.lower_jaw));
        assert_eq!(derived.tongue, vec![expected.blade as u32]);
    }

    #[test]
    fn apply_and_ensure_install_canonical_groups_once() {
        let (mut geometry, _) = fixture();
        assert!(!has_canonical_head_groups(&geometry));
        let applied = ensure_canonical_head_groups(&mut geometry)
            .unwrap()
            .expect("first call derives and applies");
        assert!(has_canonical_head_groups(&geometry));

        assert!(geometry.polygon_groups.iter().any(|name| name == "body"));

        assert_eq!(ensure_canonical_head_groups(&mut geometry).unwrap(), None);
        let rederived = derive_canonical_head_group_faces(&geometry).unwrap();
        assert_eq!(applied, rederived);
        geometry.validate().unwrap();
    }

    #[test]
    fn cache_document_round_trips_and_verifies_binding() {
        let (geometry, _) = fixture();
        let derived = derive_canonical_head_group_faces(&geometry).unwrap();
        let document =
            CanonicalHeadGroupsDocument::new("aa".repeat(32), "test-fixture".to_owned(), &derived);
        let directory =
            std::env::temp_dir().join(format!("vkit-canonical-anatomy-{}", std::process::id()));
        let path = canonical_head_groups_cache_path(&directory);
        write_canonical_head_groups(&path, &document).unwrap();
        let loaded =
            read_canonical_head_groups(&path, &"aa".repeat(32), geometry.faces.len()).unwrap();
        assert_eq!(loaded, document);
        assert_eq!(loaded.face_sets(), derived);
        assert!(read_canonical_head_groups(&path, &"bb".repeat(32), geometry.faces.len()).is_err());
        assert!(read_canonical_head_groups(&path, &"aa".repeat(32), 2).is_err());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn missing_mouth_materials_are_a_typed_error() {
        let mut builder = FixtureBuilder::default();
        builder.quad(1.0, 1.0, "Face");
        let geometry = builder.build();
        let error = derive_canonical_head_group_faces(&geometry).unwrap_err();
        assert!(error.to_string().contains("Teeth/Gums/Tongue"), "{error}");
    }
}
