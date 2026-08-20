use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::str;

use sha2::{Digest, Sha256};

use super::{FormatError, Result};

pub const TEMPLATE_PACK_MAGIC: [u8; 8] = *b"FMG2PACK";

pub const TEMPLATE_PACK_VERSION: u16 = 1;

const HEADER_SIZE: usize = 80;
const CHECKSUM_OFFSET: usize = 48;
const FLAG_CLOSED_EYE: u32 = 1;
const KNOWN_FLAGS: u32 = FLAG_CLOSED_EYE;
const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENCODED_VERTICES: usize = u16::MAX as usize;
const MAX_POLYGONS: usize = 4_000_000;
const MAX_TABLE_ENTRIES: usize = u16::MAX as usize;
const MAX_NAME_BYTES: usize = u16::MAX as usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplatePolygon {
    Triangle([u32; 3]),
    Quad([u32; 4]),
}

impl TemplatePolygon {
    pub fn indices(&self) -> &[u32] {
        match self {
            Self::Triangle(indices) => indices,
            Self::Quad(indices) => indices,
        }
    }

    pub fn arity(&self) -> usize {
        self.indices().len()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SparseMorphDelta {
    pub vertex_id: u32,
    pub delta: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct TemplatePack {
    pub vertices: Vec<[f32; 3]>,
    pub polygons: Vec<TemplatePolygon>,
    pub polygon_group_indices: Vec<u32>,
    pub material_group_indices: Vec<u32>,
    pub polygon_groups: Vec<String>,
    pub material_groups: Vec<String>,
    pub closed_eye_deltas: Vec<SparseMorphDelta>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupTable {
    pub indices: Vec<u32>,
    pub names: Vec<String>,
}

impl TemplatePack {
    pub fn new(
        vertices: Vec<[f32; 3]>,
        polygons: Vec<TemplatePolygon>,
        polygon: GroupTable,
        material: GroupTable,
        mut closed_eye_deltas: Vec<SparseMorphDelta>,
    ) -> Result<Self> {
        let GroupTable {
            indices: polygon_group_indices,
            names: polygon_groups,
        } = polygon;
        let GroupTable {
            indices: material_group_indices,
            names: material_groups,
        } = material;
        closed_eye_deltas.sort_by_key(|entry| entry.vertex_id);
        let pack = Self {
            vertices,
            polygons,
            polygon_group_indices,
            material_group_indices,
            polygon_groups,
            material_groups,
            closed_eye_deltas,
        };
        pack.validate()?;
        Ok(pack)
    }

    pub fn validate(&self) -> Result<()> {
        if self.vertices.is_empty() {
            return Err(pack_error("template pack contains no vertices"));
        }
        if self.vertices.len() > MAX_ENCODED_VERTICES {
            return Err(pack_error(format!(
                "template pack has {} vertices; v1 supports at most {MAX_ENCODED_VERTICES}",
                self.vertices.len()
            )));
        }
        if self.polygons.is_empty() {
            return Err(pack_error("template pack contains no polygons"));
        }
        if self.polygons.len() > MAX_POLYGONS {
            return Err(pack_error(format!(
                "template pack has {} polygons; limit is {MAX_POLYGONS}",
                self.polygons.len()
            )));
        }
        if self.polygon_group_indices.len() != self.polygons.len()
            || self.material_group_indices.len() != self.polygons.len()
        {
            return Err(pack_error(
                "polygon and group/material index arrays must have equal length",
            ));
        }
        validate_table("polygon group", &self.polygon_groups)?;
        validate_table("material group", &self.material_groups)?;

        for (vertex_id, vertex) in self.vertices.iter().enumerate() {
            if !vertex.iter().all(|coordinate| coordinate.is_finite()) {
                return Err(pack_error(format!(
                    "vertex {vertex_id} contains a non-finite coordinate"
                )));
            }
        }
        for (face_id, polygon) in self.polygons.iter().enumerate() {
            let indices = polygon.indices();
            for &vertex_id in indices {
                if vertex_id as usize >= self.vertices.len() {
                    return Err(pack_error(format!(
                        "polygon {face_id} references vertex {vertex_id}, but only {} exist",
                        self.vertices.len()
                    )));
                }
            }
            for left in 0..indices.len() {
                if indices[left + 1..].contains(&indices[left]) {
                    return Err(pack_error(format!(
                        "polygon {face_id} contains duplicate vertex {}",
                        indices[left]
                    )));
                }
            }
        }
        for (face_id, &group_id) in self.polygon_group_indices.iter().enumerate() {
            if group_id as usize >= self.polygon_groups.len() {
                return Err(pack_error(format!(
                    "polygon {face_id} group index {group_id} exceeds its table"
                )));
            }
        }
        for (face_id, &material_id) in self.material_group_indices.iter().enumerate() {
            if material_id as usize >= self.material_groups.len() {
                return Err(pack_error(format!(
                    "polygon {face_id} material index {material_id} exceeds its table"
                )));
            }
        }
        if self.closed_eye_deltas.len() > self.vertices.len() {
            return Err(pack_error(
                "sparse closed-eye morph contains more entries than vertices",
            ));
        }
        let mut previous = None;
        for (entry_id, entry) in self.closed_eye_deltas.iter().enumerate() {
            if entry.vertex_id as usize >= self.vertices.len() {
                return Err(pack_error(format!(
                    "closed-eye entry {entry_id} references vertex {}, but only {} exist",
                    entry.vertex_id,
                    self.vertices.len()
                )));
            }
            if previous.is_some_and(|value| entry.vertex_id <= value) {
                return Err(pack_error(
                    "closed-eye vertex IDs must be unique and strictly increasing",
                ));
            }
            if !entry.delta.iter().all(|value| value.is_finite()) {
                return Err(pack_error(format!(
                    "closed-eye entry {entry_id} contains a non-finite delta"
                )));
            }
            if entry.delta.iter().all(|&value| value == 0.0) {
                return Err(pack_error(format!(
                    "closed-eye entry {entry_id} is zero and must be omitted"
                )));
            }
            previous = Some(entry.vertex_id);
        }
        Ok(())
    }

    pub fn dense_closed_eye_deltas(&self) -> Result<Vec<[f32; 3]>> {
        self.validate()?;
        let mut dense = vec![[0.0; 3]; self.vertices.len()];
        for entry in &self.closed_eye_deltas {
            dense[entry.vertex_id as usize] = entry.delta;
        }
        Ok(dense)
    }
}

pub fn encode_template_pack(pack: &TemplatePack) -> Result<Vec<u8>> {
    pack.validate()?;
    let payload = encode_payload(pack)?;
    if payload.len() > MAX_FILE_BYTES - HEADER_SIZE {
        return Err(pack_error(format!(
            "template-pack payload exceeds {} bytes",
            MAX_FILE_BYTES - HEADER_SIZE
        )));
    }

    let flags = if pack.closed_eye_deltas.is_empty() {
        0
    } else {
        FLAG_CLOSED_EYE
    };
    let mut prefix = Vec::with_capacity(CHECKSUM_OFFSET);
    prefix.extend_from_slice(&TEMPLATE_PACK_MAGIC);
    push_u16(&mut prefix, TEMPLATE_PACK_VERSION);
    push_u16(
        &mut prefix,
        u16::try_from(HEADER_SIZE).expect("fixed header fits u16"),
    );
    push_u32(&mut prefix, flags);
    push_u64(
        &mut prefix,
        u64::try_from(payload.len()).expect("bounded payload fits u64"),
    );
    push_u32(&mut prefix, count_u32(pack.vertices.len(), "vertex")?);
    push_u32(&mut prefix, count_u32(pack.polygons.len(), "polygon")?);
    push_u32(
        &mut prefix,
        count_u32(pack.polygon_groups.len(), "polygon-group")?,
    );
    push_u32(
        &mut prefix,
        count_u32(pack.material_groups.len(), "material-group")?,
    );
    push_u32(
        &mut prefix,
        count_u32(pack.closed_eye_deltas.len(), "closed-eye delta")?,
    );
    push_u32(&mut prefix, 0);
    debug_assert_eq!(prefix.len(), CHECKSUM_OFFSET);

    let checksum = checksum(&prefix, &payload);
    let mut encoded = Vec::with_capacity(HEADER_SIZE + payload.len());
    encoded.extend_from_slice(&prefix);
    encoded.extend_from_slice(&checksum);
    encoded.extend_from_slice(&payload);
    Ok(encoded)
}

pub fn write_template_pack(mut writer: impl Write, pack: &TemplatePack) -> Result<()> {
    writer.write_all(&encode_template_pack(pack)?)?;
    Ok(())
}

pub fn write_template_pack_path(path: impl AsRef<Path>, pack: &TemplatePack) -> Result<()> {
    write_template_pack(File::create(path)?, pack)
}

pub fn read_template_pack(reader: impl Read) -> Result<TemplatePack> {
    let mut encoded = Vec::new();
    reader
        .take((MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut encoded)?;
    if encoded.len() > MAX_FILE_BYTES {
        return Err(pack_error(format!(
            "template pack exceeds the {MAX_FILE_BYTES}-byte limit"
        )));
    }
    decode_template_pack(&encoded)
}

pub fn load_template_pack_path(path: impl AsRef<Path>) -> Result<TemplatePack> {
    read_template_pack(File::open(path)?)
}

pub fn decode_template_pack(encoded: &[u8]) -> Result<TemplatePack> {
    if encoded.len() > MAX_FILE_BYTES {
        return Err(pack_error(format!(
            "template pack exceeds the {MAX_FILE_BYTES}-byte limit"
        )));
    }
    if encoded.len() < HEADER_SIZE {
        return Err(pack_error(format!(
            "template pack is {} bytes; header requires {HEADER_SIZE}",
            encoded.len()
        )));
    }
    if encoded[..8] != TEMPLATE_PACK_MAGIC {
        return Err(pack_error("template-pack magic does not match"));
    }
    let version = fixed_u16(encoded, 8);
    if version != TEMPLATE_PACK_VERSION {
        return Err(pack_error(format!(
            "unsupported template-pack version {version}"
        )));
    }
    let header_size = fixed_u16(encoded, 10) as usize;
    if header_size != HEADER_SIZE {
        return Err(pack_error(format!(
            "template-pack header size is {header_size}; expected {HEADER_SIZE}"
        )));
    }
    let flags = fixed_u32(encoded, 12);
    if flags & !KNOWN_FLAGS != 0 {
        return Err(pack_error(format!(
            "template pack uses unknown flags 0x{:08x}",
            flags & !KNOWN_FLAGS
        )));
    }
    let payload_length = usize::try_from(fixed_u64(encoded, 16))
        .map_err(|_| pack_error("payload length does not fit this platform"))?;
    let expected_length = HEADER_SIZE
        .checked_add(payload_length)
        .ok_or_else(|| pack_error("template-pack length overflows usize"))?;
    if expected_length != encoded.len() {
        return Err(pack_error(format!(
            "template-pack length is {}; header declares {expected_length}",
            encoded.len()
        )));
    }
    if fixed_u32(encoded, 44) != 0 {
        return Err(pack_error("template-pack reserved header field is nonzero"));
    }
    let payload = &encoded[HEADER_SIZE..];
    let expected_checksum = checksum(&encoded[..CHECKSUM_OFFSET], payload);
    if encoded[CHECKSUM_OFFSET..HEADER_SIZE] != expected_checksum {
        return Err(pack_error("template-pack SHA-256 checksum does not match"));
    }

    let vertex_count =
        checked_header_count(fixed_u32(encoded, 24), MAX_ENCODED_VERTICES, "vertex")?;
    let polygon_count = checked_header_count(fixed_u32(encoded, 28), MAX_POLYGONS, "polygon")?;
    let polygon_group_count =
        checked_header_count(fixed_u32(encoded, 32), MAX_TABLE_ENTRIES, "polygon-group")?;
    let material_group_count =
        checked_header_count(fixed_u32(encoded, 36), MAX_TABLE_ENTRIES, "material-group")?;
    let morph_count = checked_header_count(
        fixed_u32(encoded, 40),
        MAX_ENCODED_VERTICES,
        "closed-eye delta",
    )?;
    if morph_count > vertex_count {
        return Err(pack_error(format!(
            "header declares {morph_count} morph entries for {vertex_count} vertices"
        )));
    }
    if (morph_count > 0) != (flags & FLAG_CLOSED_EYE != 0) {
        return Err(pack_error(
            "closed-eye flag and sparse morph count disagree",
        ));
    }

    let mut decoder = PayloadDecoder::new(payload);
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        vertices.push([decoder.f32()?, decoder.f32()?, decoder.f32()?]);
    }
    let mut polygons = Vec::with_capacity(polygon_count);
    for face_id in 0..polygon_count {
        polygons.push(match decoder.u8()? {
            3 => TemplatePolygon::Triangle([
                decoder.u16()? as u32,
                decoder.u16()? as u32,
                decoder.u16()? as u32,
            ]),
            4 => TemplatePolygon::Quad([
                decoder.u16()? as u32,
                decoder.u16()? as u32,
                decoder.u16()? as u32,
                decoder.u16()? as u32,
            ]),
            arity => {
                return Err(pack_error(format!(
                    "polygon {face_id} has unsupported encoded arity {arity}"
                )));
            }
        });
    }
    let polygon_group_indices = (0..polygon_count)
        .map(|_| decoder.u16().map(u32::from))
        .collect::<Result<Vec<_>>>()?;
    let material_group_indices = (0..polygon_count)
        .map(|_| decoder.u16().map(u32::from))
        .collect::<Result<Vec<_>>>()?;
    let polygon_groups = (0..polygon_group_count)
        .map(|_| decoder.string())
        .collect::<Result<Vec<_>>>()?;
    let material_groups = (0..material_group_count)
        .map(|_| decoder.string())
        .collect::<Result<Vec<_>>>()?;
    let mut closed_eye_deltas = Vec::with_capacity(morph_count);
    for _ in 0..morph_count {
        closed_eye_deltas.push(SparseMorphDelta {
            vertex_id: decoder.u16()? as u32,
            delta: [decoder.f32()?, decoder.f32()?, decoder.f32()?],
        });
    }
    decoder.finish()?;

    let pack = TemplatePack {
        vertices,
        polygons,
        polygon_group_indices,
        material_group_indices,
        polygon_groups,
        material_groups,
        closed_eye_deltas,
    };
    pack.validate()?;
    Ok(pack)
}

fn encode_payload(pack: &TemplatePack) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    for vertex in &pack.vertices {
        for &coordinate in vertex {
            payload.extend_from_slice(&coordinate.to_le_bytes());
        }
    }
    for polygon in &pack.polygons {
        payload.push(polygon.arity() as u8);
        for &vertex_id in polygon.indices() {
            push_u16(
                &mut payload,
                u16::try_from(vertex_id)
                    .map_err(|_| pack_error("polygon vertex index exceeds v1 u16 range"))?,
            );
        }
    }
    for &group_id in &pack.polygon_group_indices {
        push_u16(
            &mut payload,
            u16::try_from(group_id)
                .map_err(|_| pack_error("polygon group index exceeds v1 u16 range"))?,
        );
    }
    for &material_id in &pack.material_group_indices {
        push_u16(
            &mut payload,
            u16::try_from(material_id)
                .map_err(|_| pack_error("material group index exceeds v1 u16 range"))?,
        );
    }
    for table in [&pack.polygon_groups, &pack.material_groups] {
        for name in table {
            push_u16(
                &mut payload,
                u16::try_from(name.len())
                    .map_err(|_| pack_error("table name exceeds v1 u16 byte length"))?,
            );
            payload.extend_from_slice(name.as_bytes());
        }
    }
    for entry in &pack.closed_eye_deltas {
        push_u16(
            &mut payload,
            u16::try_from(entry.vertex_id)
                .map_err(|_| pack_error("morph vertex ID exceeds v1 u16 range"))?,
        );
        for &coordinate in &entry.delta {
            payload.extend_from_slice(&coordinate.to_le_bytes());
        }
    }
    Ok(payload)
}

fn validate_table(label: &str, table: &[String]) -> Result<()> {
    if table.is_empty() {
        return Err(pack_error(format!("{label} table is empty")));
    }
    if table.len() > MAX_TABLE_ENTRIES {
        return Err(pack_error(format!(
            "{label} table has {} entries; limit is {MAX_TABLE_ENTRIES}",
            table.len()
        )));
    }
    for (index, name) in table.iter().enumerate() {
        if name.len() > MAX_NAME_BYTES {
            return Err(pack_error(format!(
                "{label} name {index} exceeds {MAX_NAME_BYTES} UTF-8 bytes"
            )));
        }
    }
    Ok(())
}

fn count_u32(count: usize, label: &str) -> Result<u32> {
    u32::try_from(count).map_err(|_| pack_error(format!("{label} count exceeds u32")))
}

fn checked_header_count(value: u32, maximum: usize, label: &str) -> Result<usize> {
    let value = value as usize;
    if value > maximum {
        Err(pack_error(format!(
            "header {label} count {value} exceeds limit {maximum}"
        )))
    } else {
        Ok(value)
    }
}

fn checksum(prefix: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(prefix);
    digest.update(payload);
    digest.finalize().into()
}

fn fixed_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("fixed header"))
}

fn fixed_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("fixed header"))
}

fn fixed_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("fixed header"))
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

struct PayloadDecoder<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> PayloadDecoder<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| pack_error("payload offset overflow"))?;
        let result = self.bytes.get(self.offset..end).ok_or_else(|| {
            pack_error(format!(
                "template-pack payload ended at byte {} while reading {count} bytes",
                self.offset
            ))
        })?;
        self.offset = end;
        Ok(result)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("exact two-byte slice"),
        ))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(
            self.take(4)?.try_into().expect("exact four-byte slice"),
        ))
    }

    fn string(&mut self) -> Result<String> {
        let length = self.u16()? as usize;
        let value = str::from_utf8(self.take(length)?)
            .map_err(|_| pack_error("template-pack table name is not valid UTF-8"))?;
        Ok(value.to_owned())
    }

    fn finish(self) -> Result<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(pack_error(format!(
                "template-pack payload has {} trailing bytes",
                self.bytes.len() - self.offset
            )))
        }
    }
}

fn pack_error(message: impl Into<String>) -> FormatError {
    FormatError::InvalidTemplatePack(message.into())
}
