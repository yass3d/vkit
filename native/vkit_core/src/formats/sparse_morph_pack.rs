use std::{collections::BTreeSet, str, sync::Arc};

use super::g2_obj::topology_digest;
use super::{DazGeometry, FormatError, MorphAuthoring, MorphTarget, Result};
use crate::{G2F_POLYGON_COUNT, G2F_VERTEX_COUNT};

pub const SPARSE_MORPH_PACK_MAGIC: [u8; 8] = *b"FMMRPH01";

pub const EMBEDDED_G2F_CURATED_MORPH_PACK_BYTES: &[u8] =
    include_bytes!("../../resources/g2f_curated_morphs.fmmorph");

pub const SPARSE_MORPH_PACK_VERSION: u16 = 1;

pub const G2F_CURATED_HEAD_MATERIALS: &[&str] = &[
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

const HEADER_BYTES: usize = 64;
const MORPH_HEADER_BYTES: usize = 24;
const DELTA_BYTES: usize = 14;
const MAX_PACK_BYTES: usize = 16 * 1024 * 1024;
const MAX_PROVENANCE_BYTES: usize = 4096;
const MAX_TEXT_BYTES: usize = 1024;
const MAX_MORPHS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MorphPackDelta {
    pub vertex_id: u16,
    pub delta_cm: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct MorphPackEntry {
    pub id: String,
    pub display_label: String,
    pub category: String,
    pub minimum: f32,
    pub maximum: f32,
    pub default: f32,
    pub deltas: Vec<MorphPackDelta>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SparseMorphPack {
    pub vertex_count: u32,
    pub face_count: u32,
    pub topology_sha256: [u8; 32],
    pub zero_tolerance_cm: f32,
    pub provenance: String,

    pub head_vertex_mask: Vec<u8>,
    pub morphs: Vec<MorphPackEntry>,
}

impl SparseMorphPack {
    pub fn new(
        vertex_count: u32,
        face_count: u32,
        topology_sha256: [u8; 32],
        zero_tolerance_cm: f32,
        provenance: String,
        head_vertex_mask: Vec<u8>,
        morphs: Vec<MorphPackEntry>,
    ) -> Result<Self> {
        let pack = Self {
            vertex_count,
            face_count,
            topology_sha256,
            zero_tolerance_cm,
            provenance,
            head_vertex_mask,
            morphs,
        };
        pack.validate()?;
        Ok(pack)
    }

    pub fn validate(&self) -> Result<()> {
        if self.vertex_count == 0 || self.vertex_count > u16::MAX.into() {
            return Err(pack_error(
                "v1 vertex count must be in the inclusive range 1..=65535",
            ));
        }
        if self.face_count == 0 {
            return Err(pack_error("face count must be nonzero"));
        }
        if self.topology_sha256.iter().all(|byte| *byte == 0) {
            return Err(pack_error("topology digest must be nonzero"));
        }
        if !self.zero_tolerance_cm.is_finite() || self.zero_tolerance_cm < 0.0 {
            return Err(pack_error("zero tolerance must be finite and non-negative"));
        }
        if self.provenance.len() > MAX_PROVENANCE_BYTES {
            return Err(pack_error(format!(
                "provenance is {} bytes; limit is {MAX_PROVENANCE_BYTES}",
                self.provenance.len()
            )));
        }
        let expected_mask_bytes = mask_byte_count(self.vertex_count as usize);
        if self.head_vertex_mask.len() != expected_mask_bytes {
            return Err(pack_error(format!(
                "head vertex mask is {} bytes; expected {expected_mask_bytes}",
                self.head_vertex_mask.len()
            )));
        }
        if self.head_vertex_count() == 0 {
            return Err(pack_error("head vertex mask is empty"));
        }
        validate_mask_padding(&self.head_vertex_mask, self.vertex_count as usize)?;
        if self.morphs.is_empty() || self.morphs.len() > MAX_MORPHS {
            return Err(pack_error(format!(
                "morph count {} is outside 1..={MAX_MORPHS}",
                self.morphs.len()
            )));
        }

        let mut previous_id: Option<&str> = None;
        let mut ids = BTreeSet::new();
        for (morph_index, morph) in self.morphs.iter().enumerate() {
            validate_stable_id(&morph.id, morph_index)?;
            validate_text(&morph.display_label, "display label", morph_index)?;
            validate_text(&morph.category, "category", morph_index)?;
            if previous_id.is_some_and(|value| value >= morph.id.as_str()) {
                return Err(pack_error("morph IDs must be unique and strictly sorted"));
            }
            if !ids.insert(morph.id.as_str()) {
                return Err(pack_error(format!("duplicate morph ID {:?}", morph.id)));
            }
            previous_id = Some(&morph.id);
            validate_slider(morph)?;
            if morph.deltas.is_empty() || morph.deltas.len() > self.vertex_count as usize {
                return Err(pack_error(format!(
                    "morph {:?} has invalid sparse delta count {}",
                    morph.id,
                    morph.deltas.len()
                )));
            }
            let mut previous_vertex = None;
            for (delta_index, delta) in morph.deltas.iter().enumerate() {
                if u32::from(delta.vertex_id) >= self.vertex_count {
                    return Err(pack_error(format!(
                        "morph {:?} delta {delta_index} references vertex {}, but only {} exist",
                        morph.id, delta.vertex_id, self.vertex_count
                    )));
                }
                if !self.contains_head_vertex(delta.vertex_id as usize) {
                    return Err(pack_error(format!(
                        "morph {:?} delta {delta_index} references non-head vertex {}",
                        morph.id, delta.vertex_id
                    )));
                }
                if previous_vertex.is_some_and(|value| delta.vertex_id <= value) {
                    return Err(pack_error(format!(
                        "morph {:?} vertex IDs must be unique and strictly increasing",
                        morph.id
                    )));
                }
                if !delta.delta_cm.iter().all(|value| value.is_finite()) {
                    return Err(pack_error(format!(
                        "morph {:?} delta {delta_index} is non-finite",
                        morph.id
                    )));
                }
                let norm = delta
                    .delta_cm
                    .iter()
                    .map(|value| value * value)
                    .sum::<f32>()
                    .sqrt();
                if norm <= self.zero_tolerance_cm {
                    return Err(pack_error(format!(
                        "morph {:?} delta {delta_index} norm {norm} does not exceed tolerance {}",
                        morph.id, self.zero_tolerance_cm
                    )));
                }
                previous_vertex = Some(delta.vertex_id);
            }
        }
        Ok(())
    }

    pub fn head_vertex_count(&self) -> usize {
        self.head_vertex_mask
            .iter()
            .map(|byte| byte.count_ones() as usize)
            .sum()
    }

    pub fn contains_head_vertex(&self, vertex_id: usize) -> bool {
        vertex_id < self.vertex_count as usize
            && self
                .head_vertex_mask
                .get(vertex_id / 8)
                .is_some_and(|byte| byte & (1 << (vertex_id % 8)) != 0)
    }

    pub fn find(&self, id: &str) -> Option<&MorphPackEntry> {
        self.morphs
            .binary_search_by(|entry| entry.id.as_str().cmp(id))
            .ok()
            .map(|index| &self.morphs[index])
    }

    pub fn validate_geometry(&self, geometry: &DazGeometry) -> Result<()> {
        self.validate()?;
        geometry.validate()?;
        if geometry.vertices.len() != self.vertex_count as usize
            || geometry.faces.len() != self.face_count as usize
        {
            return Err(pack_error(format!(
                "pack expects {}/{} vertices/faces, geometry has {}/{}",
                self.vertex_count,
                self.face_count,
                geometry.vertices.len(),
                geometry.faces.len()
            )));
        }
        let actual = topology_digest(geometry.vertices.len(), &geometry.faces)?;
        if actual != self.topology_sha256 {
            return Err(pack_error(
                "geometry topology does not match sparse morph pack binding",
            ));
        }
        Ok(())
    }

    pub fn validate_head_material_mask(
        &self,
        geometry: &DazGeometry,
        material_names: &[&str],
    ) -> Result<()> {
        self.validate_geometry(geometry)?;
        let actual = vertex_mask_for_materials(geometry, material_names)?;
        if actual != self.head_vertex_mask {
            return Err(pack_error(
                "geometry material-derived head vertex mask does not match the pack",
            ));
        }
        Ok(())
    }

    pub fn to_morph_targets(&self, geometry: &DazGeometry) -> Result<Vec<MorphTarget>> {
        self.validate_geometry(geometry)?;
        self.to_morph_targets_after_binding_validation(geometry)
    }

    fn validate_verified_vertex_order(&self, geometry: &DazGeometry) -> Result<()> {
        self.validate()?;
        geometry.validate()?;
        if geometry.vertices.len() != self.vertex_count as usize {
            return Err(pack_error(format!(
                "pack expects {} vertices, geometry has {}",
                self.vertex_count,
                geometry.vertices.len()
            )));
        }
        Ok(())
    }

    fn to_morph_targets_after_binding_validation(
        &self,
        geometry: &DazGeometry,
    ) -> Result<Vec<MorphTarget>> {
        let reference_faces = Arc::new(geometry.faces.clone());
        self.morphs
            .iter()
            .map(|entry| {
                let mut dense = vec![[0.0; 3]; self.vertex_count as usize];
                for delta in &entry.deltas {
                    dense[delta.vertex_id as usize] = delta.delta_cm.map(f64::from);
                }
                MorphTarget::from_dense_deltas_shared(
                    entry.id.clone(),
                    dense,
                    Arc::clone(&reference_faces),
                    MorphAuthoring {
                        zero_tolerance: f64::from(self.zero_tolerance_cm),
                        minimum: f64::from(entry.minimum),
                        maximum: f64::from(entry.maximum),
                        default: f64::from(entry.default),
                        ..MorphAuthoring::UNIT
                    },
                )
            })
            .collect()
    }
}

pub fn embedded_g2f_curated_morph_pack() -> Result<SparseMorphPack> {
    let mut pack = decode_sparse_morph_pack(EMBEDDED_G2F_CURATED_MORPH_PACK_BYTES)?;
    for morph in &mut pack.morphs {
        let semantic = format!("{} {}", morph.id, morph.display_label).to_ascii_lowercase();
        if semantic
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|token| matches!(token, "smile" | "smiles" | "smiling" | "grin" | "grinning"))
        {
            morph.category = "expression".to_owned();
        }
    }
    if pack.vertex_count as usize != G2F_VERTEX_COUNT
        || pack.face_count as usize != G2F_POLYGON_COUNT
        || pack.topology_sha256 != super::G2F_TOPOLOGY_SHA256
    {
        return Err(pack_error(
            "embedded curated morph pack is not bound to canonical G2F topology",
        ));
    }
    Ok(pack)
}

pub fn embedded_g2f_curated_morph_targets(geometry: &DazGeometry) -> Result<Vec<MorphTarget>> {
    let pack = embedded_g2f_curated_morph_pack()?;
    pack.validate_head_material_mask(geometry, G2F_CURATED_HEAD_MATERIALS)?;
    pack.to_morph_targets(geometry)
}

pub fn embedded_g2f_curated_morph_targets_for_vam_anchor(
    geometry: &DazGeometry,
) -> Result<Vec<MorphTarget>> {
    let pack = embedded_g2f_curated_morph_pack()?;
    pack.validate_verified_vertex_order(geometry)?;
    pack.to_morph_targets_after_binding_validation(geometry)
}

pub fn vertex_mask_for_materials(
    geometry: &DazGeometry,
    material_names: &[&str],
) -> Result<Vec<u8>> {
    geometry.validate()?;
    let wanted = material_names
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut mask = vec![0_u8; mask_byte_count(geometry.vertices.len())];
    for (face_index, face) in geometry.faces.iter().enumerate() {
        let material_index = geometry.material_group_indices[face_index] as usize;
        let Some(material) = geometry.material_groups.get(material_index) else {
            return Err(pack_error(format!(
                "face {face_index} references missing material {material_index}"
            )));
        };
        if wanted.contains(&material.to_ascii_lowercase()) {
            for &vertex_id in face {
                set_mask_bit(&mut mask, vertex_id as usize);
            }
        }
    }
    Ok(mask)
}

pub fn encode_sparse_morph_pack(pack: &SparseMorphPack) -> Result<Vec<u8>> {
    pack.validate()?;
    let provenance_len = u16_len(&pack.provenance, "provenance")?;
    let morph_count =
        u16::try_from(pack.morphs.len()).map_err(|_| pack_error("morph count exceeds u16"))?;
    let mask_len = u32::try_from(pack.head_vertex_mask.len())
        .map_err(|_| pack_error("head mask byte length exceeds u32"))?;
    let mut output = Vec::with_capacity(HEADER_BYTES + pack.head_vertex_mask.len());
    output.extend_from_slice(&SPARSE_MORPH_PACK_MAGIC);
    output.extend_from_slice(&SPARSE_MORPH_PACK_VERSION.to_le_bytes());
    output.extend_from_slice(&0_u16.to_le_bytes());
    output.extend_from_slice(&pack.vertex_count.to_le_bytes());
    output.extend_from_slice(&pack.face_count.to_le_bytes());
    output.extend_from_slice(&pack.topology_sha256);
    output.extend_from_slice(&pack.zero_tolerance_cm.to_bits().to_le_bytes());
    output.extend_from_slice(&morph_count.to_le_bytes());
    output.extend_from_slice(&provenance_len.to_le_bytes());
    output.extend_from_slice(&mask_len.to_le_bytes());
    output.extend_from_slice(pack.provenance.as_bytes());
    output.extend_from_slice(&pack.head_vertex_mask);
    for morph in &pack.morphs {
        let id_len = u16_len(&morph.id, "morph ID")?;
        let label_len = u16_len(&morph.display_label, "display label")?;
        let category_len = u16_len(&morph.category, "category")?;
        let delta_count =
            u32::try_from(morph.deltas.len()).map_err(|_| pack_error("delta count exceeds u32"))?;
        output.extend_from_slice(&id_len.to_le_bytes());
        output.extend_from_slice(&label_len.to_le_bytes());
        output.extend_from_slice(&category_len.to_le_bytes());
        output.extend_from_slice(&0_u16.to_le_bytes());
        output.extend_from_slice(&morph.minimum.to_bits().to_le_bytes());
        output.extend_from_slice(&morph.maximum.to_bits().to_le_bytes());
        output.extend_from_slice(&morph.default.to_bits().to_le_bytes());
        output.extend_from_slice(&delta_count.to_le_bytes());
        output.extend_from_slice(morph.id.as_bytes());
        output.extend_from_slice(morph.display_label.as_bytes());
        output.extend_from_slice(morph.category.as_bytes());
        for delta in &morph.deltas {
            output.extend_from_slice(&delta.vertex_id.to_le_bytes());
            for coordinate in delta.delta_cm {
                output.extend_from_slice(&coordinate.to_bits().to_le_bytes());
            }
        }
    }
    if output.len() > MAX_PACK_BYTES {
        return Err(pack_error(format!(
            "encoded pack is {} bytes; limit is {MAX_PACK_BYTES}",
            output.len()
        )));
    }
    Ok(output)
}

pub fn decode_sparse_morph_pack(encoded: &[u8]) -> Result<SparseMorphPack> {
    if encoded.len() < HEADER_BYTES {
        return Err(pack_error("resource is shorter than the v1 header"));
    }
    if encoded.len() > MAX_PACK_BYTES {
        return Err(pack_error(format!(
            "resource is {} bytes; limit is {MAX_PACK_BYTES}",
            encoded.len()
        )));
    }
    let mut cursor = Cursor::new(encoded);
    if cursor.take(8)? != SPARSE_MORPH_PACK_MAGIC {
        return Err(pack_error("resource magic does not match"));
    }
    let version = cursor.u16()?;
    if version != SPARSE_MORPH_PACK_VERSION {
        return Err(pack_error(format!(
            "unsupported resource version {version}"
        )));
    }
    if cursor.u16()? != 0 {
        return Err(pack_error("reserved header bits must be zero"));
    }
    let vertex_count = cursor.u32()?;
    let face_count = cursor.u32()?;
    let mut topology_sha256 = [0_u8; 32];
    topology_sha256.copy_from_slice(cursor.take(32)?);
    let zero_tolerance_cm = f32::from_bits(cursor.u32()?);
    let morph_count = cursor.u16()? as usize;
    let provenance_len = cursor.u16()? as usize;
    let mask_len = cursor.u32()? as usize;
    if provenance_len > MAX_PROVENANCE_BYTES {
        return Err(pack_error("provenance length exceeds limit"));
    }
    if morph_count == 0 || morph_count > MAX_MORPHS {
        return Err(pack_error("morph count exceeds v1 bounds"));
    }
    let provenance = cursor.text(provenance_len, "provenance")?;
    let head_vertex_mask = cursor.take(mask_len)?.to_vec();
    let mut morphs = Vec::with_capacity(morph_count);
    for morph_index in 0..morph_count {
        if cursor.remaining() < MORPH_HEADER_BYTES {
            return Err(pack_error(format!(
                "morph {morph_index} header is truncated"
            )));
        }
        let id_len = cursor.u16()? as usize;
        let label_len = cursor.u16()? as usize;
        let category_len = cursor.u16()? as usize;
        if cursor.u16()? != 0 {
            return Err(pack_error(format!(
                "morph {morph_index} reserved bits must be zero"
            )));
        }
        let minimum = f32::from_bits(cursor.u32()?);
        let maximum = f32::from_bits(cursor.u32()?);
        let default = f32::from_bits(cursor.u32()?);
        let delta_count = cursor.u32()? as usize;
        if id_len > MAX_TEXT_BYTES || label_len > MAX_TEXT_BYTES || category_len > MAX_TEXT_BYTES {
            return Err(pack_error(format!(
                "morph {morph_index} text field exceeds {MAX_TEXT_BYTES} bytes"
            )));
        }
        if delta_count == 0 || delta_count > vertex_count as usize {
            return Err(pack_error(format!(
                "morph {morph_index} delta count {delta_count} is invalid"
            )));
        }
        let id = cursor.text(id_len, "morph ID")?;
        let display_label = cursor.text(label_len, "display label")?;
        let category = cursor.text(category_len, "category")?;
        let delta_bytes = delta_count
            .checked_mul(DELTA_BYTES)
            .ok_or_else(|| pack_error("delta byte length overflows usize"))?;
        if cursor.remaining() < delta_bytes {
            return Err(pack_error(format!(
                "morph {morph_index} delta stream is truncated"
            )));
        }
        let mut deltas = Vec::with_capacity(delta_count);
        for _ in 0..delta_count {
            let vertex_id = cursor.u16()?;
            let mut delta_cm = [0.0; 3];
            for coordinate in &mut delta_cm {
                *coordinate = f32::from_bits(cursor.u32()?);
            }
            deltas.push(MorphPackDelta {
                vertex_id,
                delta_cm,
            });
        }
        morphs.push(MorphPackEntry {
            id,
            display_label,
            category,
            minimum,
            maximum,
            default,
            deltas,
        });
    }
    if cursor.remaining() != 0 {
        return Err(pack_error(format!(
            "resource has {} trailing bytes",
            cursor.remaining()
        )));
    }
    SparseMorphPack::new(
        vertex_count,
        face_count,
        topology_sha256,
        zero_tolerance_cm,
        provenance,
        head_vertex_mask,
        morphs,
    )
}

fn validate_stable_id(id: &str, morph_index: usize) -> Result<()> {
    if id.is_empty()
        || id.len() > MAX_TEXT_BYTES
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(pack_error(format!(
            "morph {morph_index} ID must use lowercase ASCII letters, digits, or underscores"
        )));
    }
    Ok(())
}

fn validate_text(value: &str, kind: &str, morph_index: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > MAX_TEXT_BYTES {
        return Err(pack_error(format!(
            "morph {morph_index} {kind} must be 1..={MAX_TEXT_BYTES} UTF-8 bytes"
        )));
    }
    Ok(())
}

fn validate_slider(morph: &MorphPackEntry) -> Result<()> {
    if !morph.minimum.is_finite()
        || !morph.maximum.is_finite()
        || !morph.default.is_finite()
        || morph.minimum >= morph.maximum
        || !(morph.minimum..=morph.maximum).contains(&morph.default)
    {
        return Err(pack_error(format!(
            "morph {:?} has invalid slider range/default",
            morph.id
        )));
    }
    Ok(())
}

fn mask_byte_count(vertex_count: usize) -> usize {
    vertex_count.div_ceil(8)
}

fn set_mask_bit(mask: &mut [u8], vertex_id: usize) {
    mask[vertex_id / 8] |= 1 << (vertex_id % 8);
}

fn validate_mask_padding(mask: &[u8], vertex_count: usize) -> Result<()> {
    let used = vertex_count % 8;
    if used != 0 {
        let allowed = ((1_u16 << used) - 1) as u8;
        if mask.last().is_some_and(|byte| byte & !allowed != 0) {
            return Err(pack_error("head vertex mask has nonzero padding bits"));
        }
    }
    Ok(())
}

fn u16_len(value: &str, label: &str) -> Result<u16> {
    u16::try_from(value.len()).map_err(|_| pack_error(format!("{label} byte length exceeds u16")))
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| pack_error("resource offset overflows usize"))?;
        let result = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| pack_error("resource is truncated"))?;
        self.offset = end;
        Ok(result)
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("two-byte slice"),
        ))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four-byte slice"),
        ))
    }

    fn text(&mut self, count: usize, label: &str) -> Result<String> {
        str::from_utf8(self.take(count)?)
            .map(str::to_owned)
            .map_err(|_| pack_error(format!("{label} is not valid UTF-8")))
    }
}

fn pack_error(message: impl Into<String>) -> FormatError {
    FormatError::InvalidMorphPack(message.into())
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
            vec![vec![0, 1, 2], vec![0, 2, 3]],
            crate::formats::GroupTable {
                indices: vec![0, 0],
                names: vec!["fixture".into()],
            },
            crate::formats::GroupTable {
                indices: vec![0, 1],
                names: vec!["Face".into(), "Torso".into()],
            },
            json!({}),
        )
        .unwrap()
    }

    fn fixture_pack() -> SparseMorphPack {
        let geometry = geometry();
        let topology = topology_digest(geometry.vertices.len(), &geometry.faces).unwrap();
        SparseMorphPack::new(
            4,
            2,
            topology,
            0.001,
            "fixture".into(),
            vertex_mask_for_materials(&geometry, &["Face"]).unwrap(),
            vec![MorphPackEntry {
                id: "mouth_test".into(),
                display_label: "Mouth Test".into(),
                category: "mouth".into(),
                minimum: 0.0,
                maximum: 1.0,
                default: 0.0,
                deltas: vec![MorphPackDelta {
                    vertex_id: 1,
                    delta_cm: [0.0, 0.2, 0.0],
                }],
            }],
        )
        .unwrap()
    }

    #[test]
    fn generic_pack_round_trips_and_applies_to_bound_topology() {
        let pack = fixture_pack();
        let encoded = encode_sparse_morph_pack(&pack).unwrap();
        let decoded = decode_sparse_morph_pack(&encoded).unwrap();
        assert_eq!(decoded, pack);
        decoded
            .validate_head_material_mask(&geometry(), &["Face"])
            .unwrap();
        let targets = decoded.to_morph_targets(&geometry()).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].key, "mouth_test");
        assert!((targets[0].deltas[1][1] - 0.2).abs() < 1.0e-7);
    }

    #[test]
    fn topology_mismatch_and_non_head_delta_are_rejected() {
        let pack = fixture_pack();
        let mut changed = geometry();
        changed.faces[0].swap(1, 2);
        assert!(pack.to_morph_targets(&changed).is_err());

        let mut leaked = pack.clone();
        leaked.morphs[0].deltas[0].vertex_id = 3;
        assert!(leaked.validate().is_err());
    }

    #[test]
    fn a_mask_that_disagrees_with_the_callers_geometry_is_refused() {
        let pack = fixture_pack();
        pack.validate_head_material_mask(&geometry(), &["Face"])
            .unwrap();
        assert!(
            pack.validate_head_material_mask(&geometry(), &["Face", "Torso"])
                .is_err()
        );
    }

    #[test]
    fn the_seven_bundled_shapes_are_present_and_filed_where_they_belong() {
        let pack = embedded_g2f_curated_morph_pack().unwrap();
        assert_eq!(
            pack.morphs
                .iter()
                .map(|morph| (morph.id.as_str(), morph.category.as_str()))
                .collect::<Vec<_>>(),
            [
                ("mouth_open_wide", "mouth"),
                ("mouth_open_wide_2", "mouth"),
                ("mouth_open_wide_3", "mouth"),
                ("mouth_smile", "expression"),
                ("mouth_smile_open", "expression"),
                ("smile_full_face", "expression"),
                ("smile_open_full_face", "expression"),
            ]
        );

        let reach = pack
            .find("mouth_open_wide")
            .unwrap()
            .deltas
            .iter()
            .map(|delta| delta.delta_cm.iter().fold(0.0_f32, |a, b| a.max(b.abs())))
            .fold(0.0_f32, f32::max);
        assert!(
            reach > 2.0,
            "the chin swing belongs in the shape, not in a bone formula only VaM runs: {reach}"
        );
    }

    #[test]
    fn decoder_rejects_trailing_bytes_and_reserved_bits() {
        let encoded = encode_sparse_morph_pack(&fixture_pack()).unwrap();
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(decode_sparse_morph_pack(&trailing).is_err());
        let mut reserved = encoded;
        reserved[10] = 1;
        assert!(decode_sparse_morph_pack(&reserved).is_err());
    }
}
