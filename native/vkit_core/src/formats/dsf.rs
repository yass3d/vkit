use std::fs::File;
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json::Value;

use super::{
    FormatError, GroupTable, ObjFace, OrderedObjMesh, Result, finite_point, finite_point_with,
};

pub(super) const MAX_DSF_BYTES: u64 = 512 * 1024 * 1024;

pub const HEAD_SKIN_MATERIALS: &[&str] = &["Face", "Head", "Neck", "Ears", "Lips"];

pub const HEAD_VISUAL_EXCLUDED_MATERIALS: &[&str] = &["Lacrimals", "Tear", "Eyelashes"];

pub const HEAD_VISUAL_MATERIALS: &[&str] = &[
    "Face",
    "Head",
    "Neck",
    "Ears",
    "Lips",
    "Nostrils",
    "Sclera",
    "Irises",
    "Pupils",
    "Cornea",
    "EyeReflection",
    "InnerMouth",
    "Teeth",
    "Gums",
    "Tongue",
];

#[derive(Clone, Debug, PartialEq)]
pub struct DazBone {
    pub id: String,
    pub center_point: [f64; 3],
    pub end_point: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct DazGeometry {
    pub geometry_id: String,
    pub vertices: Vec<[f64; 3]>,
    pub faces: Vec<Vec<u32>>,
    pub polygon_group_indices: Vec<u32>,
    pub material_group_indices: Vec<u32>,
    pub polygon_groups: Vec<String>,
    pub material_groups: Vec<String>,
    pub bones: Vec<DazBone>,
    pub root_region: Value,
}

impl DazGeometry {
    pub fn new(
        geometry_id: String,
        vertices: Vec<[f64; 3]>,
        faces: Vec<Vec<u32>>,
        polygon: GroupTable,
        material: GroupTable,
        root_region: Value,
    ) -> Result<Self> {
        let crate::formats::GroupTable {
            indices: polygon_group_indices,
            names: polygon_groups,
        } = polygon;
        let crate::formats::GroupTable {
            indices: material_group_indices,
            names: material_groups,
        } = material;
        let result = Self {
            geometry_id,
            vertices,
            faces,
            polygon_group_indices,
            material_group_indices,
            polygon_groups,
            material_groups,
            bones: Vec::new(),
            root_region,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn validate(&self) -> Result<()> {
        if self.vertices.is_empty() {
            return Err(dsf_error("geometry has no vertices"));
        }
        for (index, vertex) in self.vertices.iter().copied().enumerate() {
            finite_point_with(vertex, || format!("DSF vertex {index}"))
                .map_err(|error| dsf_error(error.to_string()))?;
        }
        if self.faces.len() != self.polygon_group_indices.len()
            || self.faces.len() != self.material_group_indices.len()
        {
            return Err(dsf_error(
                "face, polygon-group, and material-group arrays must have equal length",
            ));
        }
        for (face_id, face) in self.faces.iter().enumerate() {
            if !matches!(face.len(), 3 | 4) {
                return Err(dsf_error(format!(
                    "face {face_id} has {} corners; only triangles and quads are supported",
                    face.len()
                )));
            }
            if let Some(&bad) = face
                .iter()
                .find(|&&index| index as usize >= self.vertices.len())
            {
                return Err(dsf_error(format!(
                    "face {face_id} references vertex {bad}, but the geometry has {} vertices",
                    self.vertices.len()
                )));
            }
        }
        for (face_id, &group) in self.polygon_group_indices.iter().enumerate() {
            if group as usize >= self.polygon_groups.len() {
                return Err(dsf_error(format!(
                    "face {face_id} polygon-group index {group} exceeds the group table"
                )));
            }
        }
        for (face_id, &material) in self.material_group_indices.iter().enumerate() {
            if material as usize >= self.material_groups.len() {
                return Err(dsf_error(format!(
                    "face {face_id} material-group index {material} exceeds the material table"
                )));
            }
        }
        for bone in &self.bones {
            if bone.id.trim().is_empty() {
                return Err(dsf_error("rig bone has an empty id"));
            }
            finite_point(bone.center_point, &format!("DSF bone {} center", bone.id))
                .map_err(|error| dsf_error(error.to_string()))?;
            finite_point(bone.end_point, &format!("DSF bone {} end", bone.id))
                .map_err(|error| dsf_error(error.to_string()))?;
        }
        Ok(())
    }

    pub fn bone(&self, id: &str) -> Option<&DazBone> {
        self.bones
            .iter()
            .find(|bone| bone.id.eq_ignore_ascii_case(id))
    }

    pub fn face_mask_for_materials<'a>(
        &self,
        names: impl IntoIterator<Item = &'a str>,
    ) -> Vec<bool> {
        let wanted: Vec<String> = names
            .into_iter()
            .map(|name| name.to_ascii_lowercase())
            .collect();
        self.material_group_indices
            .iter()
            .map(|&index| {
                self.material_groups
                    .get(index as usize)
                    .is_some_and(|name| wanted.contains(&name.to_ascii_lowercase()))
            })
            .collect()
    }

    pub fn face_mask_for_polygon_groups<'a>(
        &self,
        names: impl IntoIterator<Item = &'a str>,
    ) -> Vec<bool> {
        let wanted: Vec<String> = names
            .into_iter()
            .map(|name| name.to_ascii_lowercase())
            .collect();
        self.polygon_group_indices
            .iter()
            .map(|&index| {
                self.polygon_groups
                    .get(index as usize)
                    .is_some_and(|name| wanted.contains(&name.to_ascii_lowercase()))
            })
            .collect()
    }

    pub fn to_ordered_obj(&self, face_mask: Option<&[bool]>) -> Result<OrderedObjMesh> {
        self.validate()?;
        if let Some(mask) = face_mask
            && mask.len() != self.faces.len()
        {
            return Err(dsf_error(format!(
                "face mask has {} entries; expected {}",
                mask.len(),
                self.faces.len()
            )));
        }
        let mut faces = Vec::new();
        for face_id in 0..self.faces.len() {
            if face_mask.is_some_and(|mask| !mask[face_id]) {
                continue;
            }
            let material =
                self.material_groups[self.material_group_indices[face_id] as usize].clone();
            faces.push(ObjFace {
                vertex_indices: self.faces[face_id].clone(),

                group: Some(material.clone()),
                material: Some(material),
            });
        }
        Ok(OrderedObjMesh {
            vertices: self.vertices.clone(),
            faces,
        })
    }
}

#[derive(Deserialize)]
struct DsfDocument {
    #[serde(default)]
    geometry_library: Vec<DsfGeometry>,
    #[serde(default)]
    node_library: Vec<DsfNode>,
}

#[derive(Deserialize)]
struct DsfNode {
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "type")]
    node_type: Option<String>,
    #[serde(default)]
    center_point: Vec<DsfChannel>,
    #[serde(default)]
    end_point: Vec<DsfChannel>,
}

#[derive(Deserialize)]
struct DsfChannel {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    value: Option<f64>,
}

#[derive(Deserialize)]
struct DsfGeometry {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    vertices: CountedValues<Vec<f64>>,
    #[serde(default)]
    polylist: CountedValues<Vec<i64>>,
    #[serde(default)]
    polygon_groups: ValueTable,
    #[serde(default)]
    polygon_material_groups: ValueTable,
    #[serde(default = "empty_object")]
    root_region: Value,
}

#[derive(Deserialize)]
struct CountedValues<T> {
    count: Option<usize>,
    #[serde(default)]
    values: Vec<T>,
}

impl<T> Default for CountedValues<T> {
    fn default() -> Self {
        Self {
            count: None,
            values: Vec::new(),
        }
    }
}

#[derive(Default, Deserialize)]
struct ValueTable {
    #[serde(default)]
    values: Vec<String>,
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

pub fn load_dsf_path(path: impl AsRef<Path>, geometry_index: usize) -> Result<DazGeometry> {
    load_dsf(File::open(path)?, geometry_index)
}

pub fn load_dsf(reader: impl Read, geometry_index: usize) -> Result<DazGeometry> {
    let encoded = read_limited(reader, MAX_DSF_BYTES, "encoded DSF")?;
    let json = if encoded.starts_with(&[0x1f, 0x8b]) {
        read_limited(
            GzDecoder::new(encoded.as_slice()),
            MAX_DSF_BYTES,
            "expanded DSF",
        )?
    } else {
        encoded
    };
    let document: DsfDocument = serde_json::from_slice(&json)?;
    if document.geometry_library.is_empty() {
        return Err(dsf_error("document contains no geometry_library"));
    }
    let bones = document
        .node_library
        .into_iter()
        .filter(|node| node.node_type.as_deref().is_none_or(|kind| kind == "bone"))
        .filter_map(|node| {
            let id = node.id?.trim().to_owned();
            let center_point = dsf_point(&node.center_point)?;
            let end_point = dsf_point(&node.end_point)?;
            (!id.is_empty()).then_some(DazBone {
                id,
                center_point,
                end_point,
            })
        })
        .collect::<Vec<_>>();
    let library_len = document.geometry_library.len();
    let source = document
        .geometry_library
        .into_iter()
        .nth(geometry_index)
        .ok_or_else(|| {
            dsf_error(format!(
                "geometry index {geometry_index} is outside a library of {library_len}"
            ))
        })?;

    if source
        .vertices
        .count
        .unwrap_or(source.vertices.values.len())
        != source.vertices.values.len()
    {
        return Err(dsf_error("vertex count does not match its values"));
    }
    let mut vertices = Vec::with_capacity(source.vertices.values.len());
    for (vertex_id, raw) in source.vertices.values.into_iter().enumerate() {
        if raw.len() != 3 || !raw.iter().all(|value| value.is_finite()) {
            return Err(dsf_error(format!(
                "vertex {vertex_id} must contain three finite coordinates"
            )));
        }

        vertices.push([
            raw[0] as f32 as f64,
            raw[1] as f32 as f64,
            raw[2] as f32 as f64,
        ]);
    }

    if source
        .polylist
        .count
        .unwrap_or(source.polylist.values.len())
        != source.polylist.values.len()
    {
        return Err(dsf_error("polygon count does not match its values"));
    }
    let mut faces = Vec::with_capacity(source.polylist.values.len());
    let mut polygon_group_indices = Vec::with_capacity(source.polylist.values.len());
    let mut material_group_indices = Vec::with_capacity(source.polylist.values.len());
    for (face_id, row) in source.polylist.values.into_iter().enumerate() {
        if !matches!(row.len(), 5 | 6) {
            return Err(dsf_error(format!(
                "polylist row {face_id} has {} values; expected 5 or 6",
                row.len()
            )));
        }
        polygon_group_indices.push(nonnegative_u32(row[0], "polygon group", face_id)?);
        material_group_indices.push(nonnegative_u32(row[1], "material group", face_id)?);
        let mut face = Vec::with_capacity(row.len() - 2);
        for raw_index in &row[2..] {
            let index = nonnegative_u32(*raw_index, "vertex", face_id)?;
            if index as usize >= vertices.len() {
                return Err(dsf_error(format!(
                    "polylist row {face_id} references vertex {index}, but only {} vertices exist",
                    vertices.len()
                )));
            }
            face.push(index);
        }
        faces.push(face);
    }

    let mut geometry = DazGeometry::new(
        source.id.unwrap_or_else(|| "geometry".to_owned()),
        vertices,
        faces,
        crate::formats::GroupTable {
            indices: polygon_group_indices,
            names: source.polygon_groups.values,
        },
        crate::formats::GroupTable {
            indices: material_group_indices,
            names: source.polygon_material_groups.values,
        },
        source.root_region,
    )?;
    geometry.bones = bones;
    geometry.validate()?;
    Ok(geometry)
}

fn dsf_point(channels: &[DsfChannel]) -> Option<[f64; 3]> {
    let mut point = [None; 3];
    for channel in channels {
        let axis = match channel.id.as_deref()? {
            "x" | "X" => 0,
            "y" | "Y" => 1,
            "z" | "Z" => 2,
            _ => continue,
        };
        let value = channel.value?;
        if !value.is_finite() {
            return None;
        }
        point[axis] = Some(value);
    }
    Some([point[0]?, point[1]?, point[2]?])
}

fn nonnegative_u32(value: i64, label: &str, face_id: usize) -> Result<u32> {
    u32::try_from(value).map_err(|_| {
        dsf_error(format!(
            "polylist row {face_id} contains invalid {label} index {value}"
        ))
    })
}

pub(super) fn read_limited(reader: impl Read, maximum: u64, label: &str) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    reader.take(maximum + 1).read_to_end(&mut result)?;
    if result.len() as u64 > maximum {
        return Err(dsf_error(format!("{label} exceeds {maximum} bytes")));
    }
    Ok(result)
}

pub(super) fn dsf_error(message: impl Into<String>) -> FormatError {
    FormatError::InvalidDsf(message.into())
}
