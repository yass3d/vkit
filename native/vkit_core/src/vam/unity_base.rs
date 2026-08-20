use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::formats::{G2F_TOPOLOGY_SHA256, ObjFace, OrderedObjMesh, topology_digest};
use crate::{G2F_POLYGON_COUNT, G2F_VERTEX_COUNT};

use super::catalog::VaMRoot;
use super::geometry::{GeometrySex, vertex_fingerprint as vam_vertex_fingerprint};
use super::unity_morph_bank::{
    ByteReader, Endian, bounded_i32, checked_slice, decode_unity_bundle, usize_from_u32,
};
use super::{Result, VaMError, io_error};

const MAX_TYPES: usize = 100_000;
const MAX_OBJECTS: usize = 1_000_000;
const MAX_TYPE_TREE_NODES: usize = 1_000_000;
const MAX_STRING_BUFFER_BYTES: usize = 128 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_ARRAY_ELEMENTS: usize = 8_000_000;
const MAX_WALK_DEPTH: usize = 64;
const MIN_BASE_HEIGHT_M: f64 = 1.4;
const MAX_BASE_HEIGHT_M: f64 = 2.2;

const CACHE_SCHEMA_VERSION: u32 = 1;
const CACHE_BLOB_MAGIC: [u8; 8] = *b"FMNBASE1";
const CACHE_DIRECTORY: &str = "neutral-base";

const FEMALE_GEOMETRY_ID_PREFIXES: &[&str] = &["GenesisFemale", "Genesis2Female"];
const MALE_GEOMETRY_ID_PREFIXES: &[&str] = &["Genesis2Male", "GenesisMale"];

fn bundle_error(message: impl Into<String>) -> VaMError {
    VaMError::InvalidBaseBundle(message.into())
}

#[derive(Clone, Debug, PartialEq)]
pub struct NeutralBaseMesh {
    pub sex: GeometrySex,
    pub geometry_id: String,
    pub scene_node_id: String,
    pub object_path_id: i64,
    pub bundle_path: PathBuf,
    pub bundle_sha256: [u8; 32],
    pub topology_sha256: [u8; 32],
    pub mesh: OrderedObjMesh,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NeutralBaseHandle {
    pub base: NeutralBaseMesh,
    pub from_cache: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NeutralBaseCacheReceipt {
    pub schema_version: u32,
    pub sex: String,
    pub bundle_path: String,
    pub bundle_bytes: u64,
    #[serde(default)]
    pub bundle_modified_ns: Option<String>,
    pub bundle_sha256: String,
    pub object_path_id: i64,
    pub geometry_id: String,
    pub scene_node_id: String,
    pub vertex_count: usize,
    pub polygon_count: usize,
    pub topology_sha256: String,
    pub vertex_stream_sha256: String,
}

pub fn extract_neutral_base(root: &VaMRoot, sex: GeometrySex) -> Result<NeutralBaseMesh> {
    let bundle_path = root.neutral_base_bundle_path(sex);
    let bundle = fs::read(&bundle_path).map_err(|error| io_error(&bundle_path, error))?;
    extract_neutral_base_from_bundle(&bundle, &bundle_path, sex)
}

pub fn extract_neutral_base_from_bundle(
    bundle: &[u8],
    bundle_path: &Path,
    sex: GeometrySex,
) -> Result<NeutralBaseMesh> {
    let bundle_sha256: [u8; 32] = Sha256::digest(bundle).into();
    let (data_area, nodes) = decode_unity_bundle(bundle)?;
    let mut candidates = Vec::new();
    for node in &nodes {
        if node.path.ends_with(".resS") || node.path.ends_with(".resource") {
            continue;
        }
        let end = node
            .offset
            .checked_add(node.size)
            .ok_or_else(|| bundle_error("UnityFS node byte range overflow"))?;
        let cab = data_area
            .get(node.offset..end)
            .ok_or_else(|| bundle_error("UnityFS node exceeds decompressed data"))?;
        candidates.extend(collect_daz_mesh_candidates(cab)?);
    }
    let prefixes = match sex {
        GeometrySex::Female => FEMALE_GEOMETRY_ID_PREFIXES,
        GeometrySex::Male => MALE_GEOMETRY_ID_PREFIXES,
    };
    let candidate_summary = candidates
        .iter()
        .map(|candidate| {
            format!(
                "{} ({} vertices, {} polygons)",
                candidate.geometry_id,
                candidate.base_vertices.len(),
                candidate.base_polys.len()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let body = candidates
        .into_iter()
        .find(|candidate| {
            candidate.base_vertices.len() == G2F_VERTEX_COUNT
                && candidate.base_polys.len() == G2F_POLYGON_COUNT
                && prefixes
                    .iter()
                    .any(|prefix| candidate.geometry_id.starts_with(prefix))
        })
        .ok_or_else(|| {
            bundle_error(format!(
                "{} holds no {} full-body DAZMesh with {G2F_VERTEX_COUNT} vertices and {G2F_POLYGON_COUNT} polygons; found: [{candidate_summary}]",
                bundle_path.display(),
                match sex {
                    GeometrySex::Female => "female",
                    GeometrySex::Male => "male",
                },
            ))
        })?;
    candidate_to_neutral_base(body, sex, bundle_path, bundle_sha256)
}

pub fn load_or_extract_neutral_base(
    root: &VaMRoot,
    sex: GeometrySex,
    cache_dir: Option<&Path>,
) -> Result<NeutralBaseHandle> {
    let bundle_path = root.neutral_base_bundle_path(sex);

    let stamp = bundle_stamp(&bundle_path);
    if let Some(dir) = cache_dir
        && let Some(stamp) = stamp
        && let Some(base) = try_load_cached_base_by_stamp(dir, sex, &bundle_path, stamp)
    {
        return Ok(NeutralBaseHandle {
            base,
            from_cache: true,
        });
    }

    let bundle = fs::read(&bundle_path).map_err(|error| io_error(&bundle_path, error))?;
    let bundle_sha256: [u8; 32] = Sha256::digest(&bundle).into();

    if let Some(dir) = cache_dir
        && let Some(base) = try_load_cached_base(dir, sex, &bundle_path, bundle_sha256)
    {
        let _ = write_receipt(dir, sex, &build_receipt(&base, bundle.len() as u64, stamp));
        return Ok(NeutralBaseHandle {
            base,
            from_cache: true,
        });
    }

    let base = extract_neutral_base_from_bundle(&bundle, &bundle_path, sex)?;
    if let Some(dir) = cache_dir {
        let _ = write_cached_base(dir, &base, bundle.len() as u64, stamp);
    }
    Ok(NeutralBaseHandle {
        base,
        from_cache: false,
    })
}

pub fn read_neutral_base_receipt(
    cache_dir: &Path,
    sex: GeometrySex,
) -> Option<NeutralBaseCacheReceipt> {
    let receipt_path = cache_receipt_path(cache_dir, sex);
    let text = fs::read_to_string(receipt_path).ok()?;
    serde_json::from_str(&text).ok()
}

fn cache_file_stem(sex: GeometrySex) -> &'static str {
    match sex {
        GeometrySex::Female => "g2f-base-v1",
        GeometrySex::Male => "g2m-base-v1",
    }
}

fn cache_blob_path(cache_dir: &Path, sex: GeometrySex) -> PathBuf {
    cache_dir
        .join(CACHE_DIRECTORY)
        .join(format!("{}.fmnb", cache_file_stem(sex)))
}

fn cache_receipt_path(cache_dir: &Path, sex: GeometrySex) -> PathBuf {
    cache_dir
        .join(CACHE_DIRECTORY)
        .join(format!("{}-receipt.json", cache_file_stem(sex)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BundleStamp {
    bytes: u64,
    modified_ns: u128,
}

fn bundle_stamp(bundle_path: &Path) -> Option<BundleStamp> {
    let metadata = fs::metadata(bundle_path).ok()?;
    let modified_ns = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(BundleStamp {
        bytes: metadata.len(),
        modified_ns,
    })
}

fn receipt_matches_stamp(
    receipt: &NeutralBaseCacheReceipt,
    bundle_path: &Path,
    stamp: BundleStamp,
) -> bool {
    receipt.bundle_bytes == stamp.bytes
        && receipt.bundle_modified_ns.as_deref() == Some(stamp.modified_ns.to_string().as_str())
        && receipt.bundle_path == bundle_path.to_string_lossy()
}

fn try_load_cached_base_by_stamp(
    cache_dir: &Path,
    sex: GeometrySex,
    bundle_path: &Path,
    stamp: BundleStamp,
) -> Option<NeutralBaseMesh> {
    let receipt = read_neutral_base_receipt(cache_dir, sex)?;
    if !receipt_matches_stamp(&receipt, bundle_path, stamp) {
        return None;
    }
    let recorded = decode_hex32(&receipt.bundle_sha256)?;
    cached_base_from_receipt(&receipt, cache_dir, sex, bundle_path, recorded)
}

fn try_load_cached_base(
    cache_dir: &Path,
    sex: GeometrySex,
    bundle_path: &Path,
    bundle_sha256: [u8; 32],
) -> Option<NeutralBaseMesh> {
    let receipt = read_neutral_base_receipt(cache_dir, sex)?;
    cached_base_from_receipt(&receipt, cache_dir, sex, bundle_path, bundle_sha256)
}

fn cached_base_from_receipt(
    receipt: &NeutralBaseCacheReceipt,
    cache_dir: &Path,
    sex: GeometrySex,
    bundle_path: &Path,
    bundle_sha256: [u8; 32],
) -> Option<NeutralBaseMesh> {
    if receipt.schema_version != CACHE_SCHEMA_VERSION
        || receipt.bundle_sha256 != hex(&bundle_sha256)
        || receipt.vertex_count != G2F_VERTEX_COUNT
        || receipt.polygon_count != G2F_POLYGON_COUNT
        || receipt.topology_sha256 != hex(&G2F_TOPOLOGY_SHA256)
    {
        return None;
    }
    let blob = fs::read(cache_blob_path(cache_dir, sex)).ok()?;
    let base = decode_cache_blob(&blob, sex, bundle_path).ok()?;
    if base.bundle_sha256 != bundle_sha256
        || base.object_path_id != receipt.object_path_id
        || hex(&vam_vertex_fingerprint(&base.mesh.vertices)) != receipt.vertex_stream_sha256
    {
        return None;
    }
    Some(base)
}

fn build_receipt(
    base: &NeutralBaseMesh,
    bundle_bytes: u64,
    stamp: Option<BundleStamp>,
) -> NeutralBaseCacheReceipt {
    NeutralBaseCacheReceipt {
        schema_version: CACHE_SCHEMA_VERSION,
        sex: match base.sex {
            GeometrySex::Female => "female".to_owned(),
            GeometrySex::Male => "male".to_owned(),
        },
        bundle_path: base.bundle_path.to_string_lossy().into_owned(),
        bundle_bytes,
        bundle_modified_ns: stamp
            .filter(|stamp| stamp.bytes == bundle_bytes)
            .map(|stamp| stamp.modified_ns.to_string()),
        bundle_sha256: hex(&base.bundle_sha256),
        object_path_id: base.object_path_id,
        geometry_id: base.geometry_id.clone(),
        scene_node_id: base.scene_node_id.clone(),
        vertex_count: base.mesh.vertices.len(),
        polygon_count: base.mesh.faces.len(),
        topology_sha256: hex(&base.topology_sha256),
        vertex_stream_sha256: hex(&vam_vertex_fingerprint(&base.mesh.vertices)),
    }
}

fn write_receipt(
    cache_dir: &Path,
    sex: GeometrySex,
    receipt: &NeutralBaseCacheReceipt,
) -> Result<()> {
    let directory = cache_dir.join(CACHE_DIRECTORY);
    fs::create_dir_all(&directory).map_err(|error| io_error(&directory, error))?;
    let receipt_text = serde_json::to_string_pretty(receipt)
        .map_err(|error| bundle_error(format!("cache receipt serialization failed: {error}")))?;
    write_replacing(&cache_receipt_path(cache_dir, sex), receipt_text.as_bytes())
}

fn write_cached_base(
    cache_dir: &Path,
    base: &NeutralBaseMesh,
    bundle_bytes: u64,
    stamp: Option<BundleStamp>,
) -> Result<()> {
    let blob = encode_cache_blob(base)?;
    let directory = cache_dir.join(CACHE_DIRECTORY);
    fs::create_dir_all(&directory).map_err(|error| io_error(&directory, error))?;
    write_replacing(&cache_blob_path(cache_dir, base.sex), &blob)?;
    write_receipt(
        cache_dir,
        base.sex,
        &build_receipt(base, bundle_bytes, stamp),
    )
}

fn write_replacing(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    fs::write(&temporary, bytes).map_err(|error| io_error(&temporary, error))?;

    fs::rename(&temporary, path).map_err(|error| io_error(path, error))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex32(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut bytes = [0_u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(text.get(index * 2..index * 2 + 2)?, 16).ok()?;
    }
    Some(bytes)
}

pub(crate) fn neutral_cache_directory(cache_dir: &Path) -> PathBuf {
    cache_dir.join(CACHE_DIRECTORY)
}

fn encode_cache_blob(base: &NeutralBaseMesh) -> Result<Vec<u8>> {
    let mut materials = Vec::<String>::new();
    let mut material_indices = Vec::with_capacity(base.mesh.faces.len());
    for (face_index, face) in base.mesh.faces.iter().enumerate() {
        let material = face
            .material
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| bundle_error(format!("cache face {face_index} has no material")))?;
        let index = match materials.iter().position(|known| known == material) {
            Some(index) => index,
            None => {
                materials.push(material.to_owned());
                materials.len() - 1
            }
        };
        material_indices.push(index as u32);
    }

    let mut output = Vec::new();
    output.extend_from_slice(&CACHE_BLOB_MAGIC);
    output.extend_from_slice(&(CACHE_SCHEMA_VERSION as u16).to_le_bytes());
    output.push(match base.sex {
        GeometrySex::Female => 1,
        GeometrySex::Male => 2,
    });
    output.push(0);
    output.extend_from_slice(&base.object_path_id.to_le_bytes());
    output.extend_from_slice(&base.bundle_sha256);
    output.extend_from_slice(&(base.mesh.vertices.len() as u32).to_le_bytes());
    output.extend_from_slice(&(base.mesh.faces.len() as u32).to_le_bytes());
    output.extend_from_slice(&(materials.len() as u32).to_le_bytes());
    encode_text(&mut output, &base.geometry_id);
    encode_text(&mut output, &base.scene_node_id);
    for material in &materials {
        encode_text(&mut output, material);
    }
    for vertex in &base.mesh.vertices {
        for &coordinate in vertex {
            output.extend_from_slice(&coordinate.to_le_bytes());
        }
    }
    for (face, &material_index) in base.mesh.faces.iter().zip(&material_indices) {
        let arity = u8::try_from(face.vertex_indices.len())
            .map_err(|_| bundle_error("cache face arity exceeds u8"))?;
        output.push(arity);
        for &index in &face.vertex_indices {
            output.extend_from_slice(&index.to_le_bytes());
        }
        output.extend_from_slice(&material_index.to_le_bytes());
    }
    let integrity: [u8; 32] = Sha256::digest(&output).into();
    output.extend_from_slice(&integrity);
    Ok(output)
}

fn encode_text(output: &mut Vec<u8>, value: &str) {
    output.extend_from_slice(&(value.len() as u32).to_le_bytes());
    output.extend_from_slice(value.as_bytes());
}

fn decode_cache_blob(blob: &[u8], sex: GeometrySex, bundle_path: &Path) -> Result<NeutralBaseMesh> {
    if blob.len() < CACHE_BLOB_MAGIC.len() + 32 {
        return Err(bundle_error("cache blob is truncated"));
    }
    let (payload, stored_integrity) = blob.split_at(blob.len() - 32);
    let integrity: [u8; 32] = Sha256::digest(payload).into();
    if integrity != stored_integrity {
        return Err(bundle_error("cache blob integrity hash mismatch"));
    }
    let mut reader = ByteReader::new(payload, Endian::Little);
    if reader.take(8)? != CACHE_BLOB_MAGIC {
        return Err(bundle_error("cache blob magic mismatch"));
    }
    if reader.u16()? != CACHE_SCHEMA_VERSION as u16 {
        return Err(bundle_error("cache blob schema version mismatch"));
    }
    let stored_sex = reader.byte()?;
    let expected_sex = match sex {
        GeometrySex::Female => 1,
        GeometrySex::Male => 2,
    };
    if stored_sex != expected_sex {
        return Err(bundle_error("cache blob figure family mismatch"));
    }
    reader.byte()?;
    let object_path_id = reader.i64()?;
    let mut bundle_sha256 = [0_u8; 32];
    bundle_sha256.copy_from_slice(reader.take(32)?);
    let vertex_count = bounded_i32(reader.i32()?, G2F_VERTEX_COUNT, "cache vertex")?;
    let face_count = bounded_i32(reader.i32()?, G2F_POLYGON_COUNT, "cache polygon")?;
    let material_count = bounded_i32(reader.i32()?, 4_096, "cache material")?;
    if vertex_count != G2F_VERTEX_COUNT || face_count != G2F_POLYGON_COUNT {
        return Err(bundle_error("cache blob counts are not the canonical base"));
    }
    let geometry_id = decode_text(&mut reader)?;
    let scene_node_id = decode_text(&mut reader)?;
    let mut materials = Vec::with_capacity(material_count);
    for _ in 0..material_count {
        materials.push(decode_text(&mut reader)?);
    }
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        let mut vertex = [0.0_f64; 3];
        for coordinate in &mut vertex {
            let bits: [u8; 8] = reader.take(8)?.try_into().expect("eight bytes");
            *coordinate = f64::from_le_bytes(bits);
            if !coordinate.is_finite() {
                return Err(bundle_error("cache blob has a non-finite coordinate"));
            }
        }
        vertices.push(vertex);
    }
    let mut faces = Vec::with_capacity(face_count);
    for face_index in 0..face_count {
        let arity = reader.byte()? as usize;
        if !matches!(arity, 3 | 4) {
            return Err(bundle_error(format!(
                "cache face {face_index} has arity {arity}"
            )));
        }
        let mut vertex_indices = Vec::with_capacity(arity);
        for _ in 0..arity {
            let index = reader.u32()?;
            if index as usize >= vertex_count {
                return Err(bundle_error(format!(
                    "cache face {face_index} references vertex {index}"
                )));
            }
            vertex_indices.push(index);
        }
        let material_index = reader.u32()? as usize;
        let material = materials.get(material_index).ok_or_else(|| {
            bundle_error(format!(
                "cache face {face_index} references material {material_index}"
            ))
        })?;
        faces.push(ObjFace {
            vertex_indices,
            group: Some(material.clone()),
            material: Some(material.clone()),
        });
    }
    if reader.position() != payload.len() {
        return Err(bundle_error("cache blob has trailing bytes"));
    }
    let polygon_streams = faces
        .iter()
        .map(|face| face.vertex_indices.clone())
        .collect::<Vec<_>>();
    let topology_sha256 = topology_digest(vertex_count, &polygon_streams)
        .map_err(|error| bundle_error(error.to_string()))?;
    if topology_sha256 != G2F_TOPOLOGY_SHA256 {
        return Err(bundle_error(
            "cache blob polygon stream is not the canonical Genesis 2 topology",
        ));
    }
    Ok(NeutralBaseMesh {
        sex,
        geometry_id,
        scene_node_id,
        object_path_id,
        bundle_path: bundle_path.to_path_buf(),
        bundle_sha256,
        topology_sha256,
        mesh: OrderedObjMesh { vertices, faces },
    })
}

fn decode_text(reader: &mut ByteReader<'_>) -> Result<String> {
    let length = bounded_i32(reader.i32()?, MAX_TEXT_BYTES, "cache text")?;
    String::from_utf8(reader.take(length)?.to_vec())
        .map_err(|_| bundle_error("cache text is not UTF-8"))
}

#[derive(Clone, Debug)]
pub(crate) struct TypeTreeNode {
    pub(crate) type_name: String,
    pub(crate) name: String,
    pub(crate) meta_flags: u32,
    pub(crate) children: Vec<TypeTreeNode>,
}

pub(crate) struct SerializedType {
    pub(crate) class_id: i32,
    pub(crate) tree: Option<TypeTreeNode>,
}

pub(crate) struct SerializedObjectRef {
    pub(crate) path_id: i64,
    pub(crate) start: usize,
    pub(crate) size: usize,
    pub(crate) type_index: usize,
}

pub(crate) struct SerializedAsset {
    pub(crate) endian: Endian,
    pub(crate) types: Vec<SerializedType>,
    pub(crate) objects: Vec<SerializedObjectRef>,
}

pub(crate) fn parse_serialized_asset(cab: &[u8]) -> Result<SerializedAsset> {
    if cab.len() < 20 {
        return Err(bundle_error("serialized asset header is truncated"));
    }
    let mut header = ByteReader::new(cab, Endian::Big);
    let metadata_size = usize_from_u32(header.u32()?)?;
    let file_size = usize_from_u32(header.u32()?)?;
    let version = header.u32()?;
    let data_offset = usize_from_u32(header.u32()?)?;
    let endian = if header.byte()? == 0 {
        Endian::Little
    } else {
        Endian::Big
    };
    header.take(3)?;
    if file_size != cab.len() {
        return Err(bundle_error(format!(
            "serialized asset declares {file_size} bytes but contains {}",
            cab.len()
        )));
    }
    if !(14..=21).contains(&version) {
        return Err(VaMError::UnsupportedEncoding(format!(
            "serialized asset version {version}"
        )));
    }
    if metadata_size > data_offset || data_offset > cab.len() {
        return Err(bundle_error(
            "serialized asset metadata/data offsets are inconsistent",
        ));
    }

    let mut reader = ByteReader::at(cab, endian, header.position())?;
    reader.null_text()?;
    reader.i32()?;
    let has_type_tree = reader.byte()? != 0;
    if !has_type_tree {
        return Err(VaMError::UnsupportedEncoding(
            "serialized asset carries no type trees; DAZMesh decoding requires them".to_owned(),
        ));
    }
    let type_count = bounded_i32(reader.i32()?, MAX_TYPES, "serialized type")?;
    let mut types = Vec::with_capacity(type_count);
    for _ in 0..type_count {
        let class_id = reader.i32()?;
        if version >= 16 {
            reader.byte()?;
        }
        if version >= 17 {
            reader.i16()?;
        }
        if (version < 16 && class_id < 0) || (version >= 16 && class_id == 114) {
            reader.take(16)?;
        }
        reader.take(16)?;
        let node_count = bounded_i32(reader.i32()?, MAX_TYPE_TREE_NODES, "type-tree node")?;
        let string_bytes = bounded_i32(reader.i32()?, MAX_STRING_BUFFER_BYTES, "type-tree text")?;
        let node_bytes = if version >= 19 { 32 } else { 24 };
        let records = reader.take(
            node_count
                .checked_mul(node_bytes)
                .ok_or_else(|| bundle_error("type-tree byte count overflow"))?,
        )?;
        let string_buffer = reader.take(string_bytes)?;
        let tree = build_type_tree(records, node_bytes, node_count, string_buffer, endian)?;
        types.push(SerializedType { class_id, tree });
    }

    let object_count = bounded_i32(reader.i32()?, MAX_OBJECTS, "serialized object")?;
    let mut objects = Vec::with_capacity(object_count);
    for _ in 0..object_count {
        reader.align(4)?;
        let path_id = reader.i64()?;
        let object_offset = usize_from_u32(reader.u32()?)?;
        let byte_size = usize_from_u32(reader.u32()?)?;
        let type_index = reader.i32()?;
        let type_index = usize::try_from(type_index)
            .ok()
            .filter(|&index| index < types.len())
            .ok_or_else(|| bundle_error("serialized object references a missing type"))?;
        let start = data_offset
            .checked_add(object_offset)
            .ok_or_else(|| bundle_error("serialized object offset overflow"))?;
        checked_slice(cab, start, byte_size)?;
        objects.push(SerializedObjectRef {
            path_id,
            start,
            size: byte_size,
            type_index,
        });
    }
    Ok(SerializedAsset {
        endian,
        types,
        objects,
    })
}

pub(crate) fn build_type_tree(
    records: &[u8],
    node_bytes: usize,
    node_count: usize,
    string_buffer: &[u8],
    endian: Endian,
) -> Result<Option<TypeTreeNode>> {
    if node_count == 0 {
        return Ok(None);
    }
    let mut flat = Vec::with_capacity(node_count);
    for index in 0..node_count {
        let mut reader = ByteReader::at(records, endian, index * node_bytes)?;
        reader.u16()?;
        let level = reader.byte()? as usize;
        reader.byte()?;
        let type_offset = reader.u32()?;
        let name_offset = reader.u32()?;
        reader.i32()?;
        reader.i32()?;
        let meta_flags = reader.u32()?;
        flat.push((
            level,
            TypeTreeNode {
                type_name: tree_string(string_buffer, type_offset)?,
                name: tree_string(string_buffer, name_offset)?,
                meta_flags,
                children: Vec::new(),
            },
        ));
    }

    let mut nodes: Vec<Option<TypeTreeNode>> = Vec::with_capacity(node_count);
    let levels: Vec<usize> = flat.iter().map(|(level, _)| *level).collect();
    for (_, node) in flat {
        nodes.push(Some(node));
    }
    for index in (1..node_count).rev() {
        let parent = (0..index)
            .rev()
            .find(|&candidate| levels[candidate] + 1 == levels[index])
            .ok_or_else(|| bundle_error("type-tree level structure is invalid"))?;
        let child = nodes[index]
            .take()
            .ok_or_else(|| bundle_error("type-tree child was consumed twice"))?;
        nodes[parent]
            .as_mut()
            .ok_or_else(|| bundle_error("type-tree parent was consumed"))?
            .children
            .insert(0, child);
    }
    Ok(nodes[0].take())
}

pub(crate) fn tree_string(buffer: &[u8], offset: u32) -> Result<String> {
    if offset & 0x8000_0000 != 0 {
        return common_type_string(offset & 0x7fff_ffff)
            .map(str::to_owned)
            .ok_or_else(|| bundle_error(format!("unknown common type-tree string {offset:#x}")));
    }
    let start = offset as usize;
    let tail = buffer
        .get(start..)
        .ok_or_else(|| bundle_error("type-tree string offset exceeds buffer"))?;
    let length = tail
        .iter()
        .position(|&byte| byte == 0)
        .ok_or_else(|| bundle_error("unterminated type-tree string"))?;
    std::str::from_utf8(&tail[..length])
        .map(str::to_owned)
        .map_err(|_| bundle_error("type-tree string is not UTF-8"))
}

pub(crate) fn common_type_string(offset: u32) -> Option<&'static str> {
    Some(match offset {
        0 => "AABB",
        5 => "AnimationClip",
        19 => "AnimationCurve",
        34 => "AnimationState",
        49 => "Array",
        55 => "Base",
        60 => "BitField",
        69 => "bitset",
        76 => "bool",
        81 => "char",
        86 => "ColorRGBA",
        96 => "Component",
        106 => "data",
        111 => "deque",
        117 => "double",
        124 => "dynamic_array",
        138 => "FastPropertyName",
        155 => "first",
        161 => "float",
        167 => "Font",
        172 => "GameObject",
        183 => "Generic Mono",
        196 => "GradientNEW",
        208 => "GUID",
        213 => "GUIStyle",
        222 => "int",
        226 => "list",
        231 => "long long",
        241 => "map",
        245 => "Matrix4x4f",
        256 => "MdFour",
        263 => "MonoBehaviour",
        277 => "MonoScript",
        288 => "m_ByteSize",
        299 => "m_Curve",
        307 => "m_EditorClassIdentifier",
        331 => "m_EditorHideFlags",
        349 => "m_Enabled",
        359 => "m_ExtensionPtr",
        374 => "m_GameObject",
        387 => "m_Index",
        395 => "m_IsArray",
        405 => "m_IsStatic",
        416 => "m_MetaFlag",
        427 => "m_Name",
        434 => "m_ObjectHideFlags",
        452 => "m_PrefabInternal",
        469 => "m_PrefabParentObject",
        490 => "m_Script",
        499 => "m_StaticEditorFlags",
        519 => "m_Type",
        526 => "m_Version",
        536 => "Object",
        543 => "pair",
        548 => "PPtr<Component>",
        564 => "PPtr<GameObject>",
        581 => "PPtr<Material>",
        596 => "PPtr<MonoBehaviour>",
        616 => "PPtr<MonoScript>",
        633 => "PPtr<Object>",
        646 => "PPtr<Prefab>",
        659 => "PPtr<Sprite>",
        672 => "PPtr<TextAsset>",
        688 => "PPtr<Texture>",
        702 => "PPtr<Texture2D>",
        718 => "PPtr<Transform>",
        734 => "Prefab",
        741 => "Quaternionf",
        753 => "Rectf",
        759 => "RectInt",
        767 => "RectOffset",
        778 => "second",
        785 => "set",
        789 => "short",
        795 => "size",
        800 => "SInt16",
        807 => "SInt32",
        814 => "SInt64",
        821 => "SInt8",
        827 => "staticvector",
        840 => "string",
        847 => "TextAsset",
        857 => "TextMesh",
        866 => "Texture",
        874 => "Texture2D",
        884 => "Transform",
        894 => "TypelessData",
        907 => "UInt16",
        914 => "UInt32",
        921 => "UInt64",
        928 => "UInt8",
        934 => "unsigned int",
        947 => "unsigned long long",
        966 => "unsigned short",
        981 => "vector",
        988 => "Vector2f",
        997 => "Vector3f",
        1006 => "Vector4f",
        1015 => "m_ScriptingClassIdentifier",
        1042 => "Gradient",
        1051 => "Type*",
        1057 => "int2_storage",
        1070 => "int3_storage",
        1083 => "BoundsInt",
        1093 => "m_CorrespondingSourceObject",
        1121 => "m_PrefabInstance",
        1138 => "m_PrefabAsset",
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TreeValue {
    Bool(bool),
    Signed(i64),
    Unsigned(u64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    Vector3Array(Vec<[f32; 3]>),
    List(Vec<TreeValue>),
    Record(Vec<(String, TreeValue)>),
}

impl TreeValue {
    pub(crate) fn field(&self, name: &str) -> Option<&TreeValue> {
        match self {
            Self::Record(fields) => fields
                .iter()
                .find(|(field, _)| field == name)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    pub(crate) fn as_signed(&self) -> Option<i64> {
        match self {
            Self::Signed(value) => Some(*value),
            Self::Unsigned(value) => i64::try_from(*value).ok(),
            _ => None,
        }
    }

    pub(crate) fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn as_unsigned(&self) -> Option<u64> {
        match self {
            Self::Unsigned(value) => Some(*value),
            Self::Signed(value) => u64::try_from(*value).ok(),
            _ => None,
        }
    }

    pub(crate) fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(value) => Some(*value),
            _ => None,
        }
    }
}

fn primitive_width(type_name: &str) -> Option<(usize, bool)> {
    Some(match type_name {
        "bool" | "UInt8" | "char" => (1, false),
        "SInt8" => (1, true),
        "SInt16" | "short" => (2, true),
        "UInt16" | "unsigned short" => (2, false),
        "int" | "SInt32" | "Type*" => (4, true),
        "UInt32" | "unsigned int" => (4, false),
        "SInt64" | "long long" => (8, true),
        "UInt64" | "unsigned long long" | "FileSize" => (8, false),
        _ => return None,
    })
}

fn is_vector3(node: &TypeTreeNode) -> bool {
    node.type_name == "Vector3f"
        && node.children.len() == 3
        && node
            .children
            .iter()
            .all(|child| child.type_name == "float" && child.children.is_empty())
}

pub(crate) fn decode_value(
    reader: &mut ByteReader<'_>,
    node: &TypeTreeNode,
    depth: usize,
) -> Result<TreeValue> {
    if depth > MAX_WALK_DEPTH {
        return Err(bundle_error("type-tree walk exceeded the depth limit"));
    }
    if node.type_name == "TypelessData" {
        let length = bounded_i32(reader.i32()?, MAX_ARRAY_ELEMENTS * 16, "typeless data")?;
        let value = TreeValue::Bytes(reader.take(length)?.to_vec());
        reader.align(4)?;
        return Ok(value);
    }
    if node.type_name == "string" {
        let length = bounded_i32(reader.i32()?, MAX_TEXT_BYTES, "asset text")?;
        let text = String::from_utf8(reader.take(length)?.to_vec())
            .map_err(|_| bundle_error("asset text is not UTF-8"))?;
        reader.align(4)?;
        return Ok(TreeValue::Text(text));
    }
    if let Some(array) = node
        .children
        .first()
        .filter(|child| child.type_name == "Array")
    {
        let element = array
            .children
            .get(1)
            .ok_or_else(|| bundle_error("type-tree array is missing its element node"))?;
        let count = bounded_i32(reader.i32()?, MAX_ARRAY_ELEMENTS, "asset array")?;
        let value = if is_vector3(element) {
            let mut points = Vec::with_capacity(count);
            for _ in 0..count {
                points.push([reader.f32()?, reader.f32()?, reader.f32()?]);
            }
            TreeValue::Vector3Array(points)
        } else {
            let mut values = Vec::with_capacity(count.min(1_048_576));
            for _ in 0..count {
                values.push(decode_value(reader, element, depth + 1)?);
            }
            TreeValue::List(values)
        };
        reader.align(4)?;
        return Ok(value);
    }
    if node.children.is_empty() {
        let value = match node.type_name.as_str() {
            "float" => TreeValue::Float(f64::from(reader.f32()?)),
            "double" => {
                let bits: [u8; 8] = reader.take(8)?.try_into().expect("eight bytes");
                TreeValue::Float(f64::from_le_bytes(bits))
            }
            "bool" => TreeValue::Bool(reader.byte()? != 0),
            other => {
                let (width, signed) = primitive_width(other).ok_or_else(|| {
                    bundle_error(format!("unsupported leaf type-tree node {other:?}"))
                })?;
                let mut raw = [0_u8; 8];
                raw[..width].copy_from_slice(reader.take(width)?);
                let unsigned = u64::from_le_bytes(raw);
                if signed {
                    let shift = 64 - width * 8;
                    TreeValue::Signed(((unsigned << shift) as i64) >> shift)
                } else {
                    TreeValue::Unsigned(unsigned)
                }
            }
        };
        if node.meta_flags & 0x4000 != 0 {
            reader.align(4)?;
        }
        return Ok(value);
    }
    let mut fields = Vec::with_capacity(node.children.len());
    for child in &node.children {
        fields.push((child.name.clone(), decode_value(reader, child, depth + 1)?));
    }
    if node.meta_flags & 0x4000 != 0 {
        reader.align(4)?;
    }
    Ok(TreeValue::Record(fields))
}

pub(crate) fn decode_record_until(
    cab: &[u8],
    object: &SerializedObjectRef,
    tree: &TypeTreeNode,
    endian: Endian,
    required: &[&str],
) -> Result<Vec<(String, TreeValue)>> {
    checked_slice(cab, object.start, object.size)?;
    let mut reader = ByteReader::at(cab, endian, object.start)?;
    let mut fields = Vec::new();
    let mut missing: Vec<&str> = required.to_vec();
    for child in &tree.children {
        let value = decode_value(&mut reader, child, 0)?;
        if reader.position() > object.start + object.size {
            return Err(bundle_error(format!(
                "object {} field {} overruns its byte size",
                object.path_id, child.name
            )));
        }
        missing.retain(|name| *name != child.name.as_str());
        fields.push((child.name.clone(), value));
        if missing.is_empty() {
            break;
        }
    }
    Ok(fields)
}

const DAZ_MESH_REQUIRED_FIELDS: &[&str] = &[
    "m_Script",
    "sceneNodeId",
    "geometryId",
    "_numBaseVertices",
    "_numBasePolygons",
    "_materialNames",
    "_baseVertices",
    "_basePolyList",
];

#[derive(Clone, Debug)]
struct DazMeshCandidate {
    path_id: i64,
    scene_node_id: String,
    geometry_id: String,
    material_names: Vec<String>,
    base_vertices: Vec<[f32; 3]>,
    base_polys: Vec<(u32, Vec<u32>)>,
}

fn collect_daz_mesh_candidates(cab: &[u8]) -> Result<Vec<DazMeshCandidate>> {
    let asset = parse_serialized_asset(cab)?;
    let scripts = find_daz_mesh_scripts(cab, &asset)?;
    if scripts.is_empty() {
        return Ok(Vec::new());
    }
    let mut candidates = Vec::new();
    for object in &asset.objects {
        let serialized_type = &asset.types[object.type_index];
        if serialized_type.class_id != 114 {
            continue;
        }
        let Some(tree) = serialized_type.tree.as_ref() else {
            continue;
        };

        let (file_id, path_id) = mono_behaviour_script_reference(cab, object, asset.endian)?;
        if file_id != 0 || !scripts.contains(&path_id) {
            continue;
        }
        let fields =
            decode_record_until(cab, object, tree, asset.endian, DAZ_MESH_REQUIRED_FIELDS)?;
        let record = TreeValue::Record(fields);
        let script = record
            .field("m_Script")
            .ok_or_else(|| bundle_error("DAZMesh record is missing m_Script"))?;
        let file_id = script
            .field("m_FileID")
            .and_then(TreeValue::as_signed)
            .unwrap_or(-1);
        let path_id = script
            .field("m_PathID")
            .and_then(TreeValue::as_signed)
            .unwrap_or(0);
        if file_id != 0 || !scripts.contains(&path_id) {
            return Err(bundle_error(
                "type-tree m_Script disagrees with the fixed-offset MonoBehaviour header",
            ));
        }
        candidates.push(candidate_from_record(object.path_id, &record)?);
    }
    Ok(candidates)
}

pub(crate) fn mono_behaviour_script_reference(
    cab: &[u8],
    object: &SerializedObjectRef,
    endian: Endian,
) -> Result<(i32, i64)> {
    if object.size < 28 {
        return Ok((-1, 0));
    }
    let mut reader = ByteReader::at(cab, endian, object.start + 16)?;
    Ok((reader.i32()?, reader.i64()?))
}

const MESH_SCRIPTS: [&str; 2] = ["DAZMesh", "DAZMergedMesh"];

fn find_daz_mesh_scripts(cab: &[u8], asset: &SerializedAsset) -> Result<Vec<i64>> {
    let mut found = Vec::new();
    for name in MESH_SCRIPTS {
        if let Some(script) = find_script_by_class_name(cab, asset, name)? {
            found.push(script);
        }
    }
    Ok(found)
}

pub(crate) fn find_script_by_class_name(
    cab: &[u8],
    asset: &SerializedAsset,
    wanted: &str,
) -> Result<Option<i64>> {
    for object in &asset.objects {
        let serialized_type = &asset.types[object.type_index];
        if serialized_type.class_id != 115 {
            continue;
        }
        let Some(tree) = serialized_type.tree.as_ref() else {
            continue;
        };
        let fields =
            decode_record_until(cab, object, tree, asset.endian, &["m_Name", "m_ClassName"])?;
        let record = TreeValue::Record(fields);
        let class_name = record
            .field("m_ClassName")
            .or_else(|| record.field("m_Name"))
            .and_then(TreeValue::as_text);
        if class_name == Some(wanted) {
            return Ok(Some(object.path_id));
        }
    }
    Ok(None)
}

fn candidate_from_record(path_id: i64, record: &TreeValue) -> Result<DazMeshCandidate> {
    let text_field = |name: &str| -> Result<String> {
        record
            .field(name)
            .and_then(TreeValue::as_text)
            .map(str::to_owned)
            .ok_or_else(|| bundle_error(format!("DAZMesh {path_id} is missing text field {name}")))
    };
    let count_field = |name: &str| -> Result<usize> {
        record
            .field(name)
            .and_then(TreeValue::as_signed)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| bundle_error(format!("DAZMesh {path_id} is missing count field {name}")))
    };

    let scene_node_id = text_field("sceneNodeId")?;
    let geometry_id = text_field("geometryId")?;
    let declared_vertices = count_field("_numBaseVertices")?;
    let declared_polygons = count_field("_numBasePolygons")?;

    let material_names = match record.field("_materialNames") {
        Some(TreeValue::List(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_text()
                    .map(str::to_owned)
                    .ok_or_else(|| bundle_error("DAZMesh material name is not a string"))
            })
            .collect::<Result<Vec<_>>>()?,
        _ => return Err(bundle_error("DAZMesh has no _materialNames list")),
    };

    let base_vertices = match record.field("_baseVertices") {
        Some(TreeValue::Vector3Array(points)) => points.clone(),
        _ => return Err(bundle_error("DAZMesh has no _baseVertices stream")),
    };
    if base_vertices.len() != declared_vertices {
        return Err(bundle_error(format!(
            "DAZMesh {path_id} declares {declared_vertices} vertices but streams {}",
            base_vertices.len()
        )));
    }

    let base_polys = match record.field("_basePolyList") {
        Some(TreeValue::List(polys)) => polys
            .iter()
            .map(|poly| {
                let material = poly
                    .field("materialNum")
                    .and_then(TreeValue::as_signed)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| bundle_error("MeshPoly has no materialNum"))?;
                let vertices = match poly.field("vertices") {
                    Some(TreeValue::List(values)) => values
                        .iter()
                        .map(|value| {
                            value
                                .as_signed()
                                .and_then(|index| u32::try_from(index).ok())
                                .ok_or_else(|| bundle_error("MeshPoly vertex index is invalid"))
                        })
                        .collect::<Result<Vec<_>>>()?,
                    _ => return Err(bundle_error("MeshPoly has no vertices list")),
                };
                Ok((material, vertices))
            })
            .collect::<Result<Vec<_>>>()?,
        _ => return Err(bundle_error("DAZMesh has no _basePolyList stream")),
    };
    if base_polys.len() != declared_polygons {
        return Err(bundle_error(format!(
            "DAZMesh {path_id} declares {declared_polygons} polygons but streams {}",
            base_polys.len()
        )));
    }

    Ok(DazMeshCandidate {
        path_id,
        scene_node_id,
        geometry_id,
        material_names,
        base_vertices,
        base_polys,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DazScalpProviderMesh {
    pub(crate) object_name: String,
    pub(crate) material_names: Vec<String>,
    pub(crate) vertices_m: Vec<[f32; 3]>,
    pub(crate) polygons: Vec<Vec<u32>>,
    pub(crate) uvs: Vec<[f32; 2]>,
}

#[derive(Clone, Debug)]
pub struct FigureUvMesh {
    pub material_names: Vec<String>,
    pub material_indices: Vec<u32>,
    pub base_polygons: Vec<Vec<u32>>,
    pub uv_polygons: Vec<Vec<u32>>,
    pub uvs: Vec<[f32; 2]>,
}

const DAZ_FIGURE_UV_REQUIRED_FIELDS: &[&str] = &[
    "m_Script",
    "sceneNodeId",
    "geometryId",
    "_numBaseVertices",
    "_numBasePolygons",
    "_numUVVertices",
    "_materialNames",
    "_baseVertices",
    "_basePolyList",
    "_UVVertices",
    "_UVPolyList",
    "_OrigUV",
];

pub fn extract_figure_uv(bundle: &[u8]) -> Result<FigureUvMesh> {
    let (data_area, nodes) = decode_unity_bundle(bundle)?;
    for node in &nodes {
        if node.path.ends_with(".resS") || node.path.ends_with(".resource") {
            continue;
        }
        let end = node.offset + node.size;
        let Some(cab) = data_area.get(node.offset..end) else {
            continue;
        };
        let Ok(asset) = parse_serialized_asset(cab) else {
            continue;
        };
        for object in &asset.objects {
            let serialized_type = &asset.types[object.type_index];
            if serialized_type.class_id != 114 {
                continue;
            }
            let Some(tree) = serialized_type.tree.as_ref() else {
                continue;
            };
            if !tree
                .children
                .iter()
                .any(|child| child.name == "_basePolyList")
            {
                continue;
            }
            let Ok(fields) = decode_record_until(
                cab,
                object,
                tree,
                asset.endian,
                DAZ_FIGURE_UV_REQUIRED_FIELDS,
            ) else {
                continue;
            };
            let record = TreeValue::Record(fields);
            let Some(base_polygons) = polygon_list(record.field("_basePolyList"), "figure").ok()
            else {
                continue;
            };
            if base_polygons.len() != crate::G2F_POLYGON_COUNT {
                continue;
            }
            let uv_polygons = polygon_list(record.field("_UVPolyList"), "figure")?;
            let uvs = vector2_list(record.field("_OrigUV"))
                .ok_or_else(|| bundle_error("figure mesh has no _OrigUV stream"))?;
            let material_names = match record.field("_materialNames") {
                Some(TreeValue::List(values)) => values
                    .iter()
                    .filter_map(|value| match value {
                        TreeValue::Text(text) => Some(text.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            let material_indices = match record.field("_basePolyList") {
                Some(TreeValue::List(polygons)) => polygons
                    .iter()
                    .map(|polygon| {
                        polygon
                            .field("materialNum")
                            .and_then(TreeValue::as_signed)
                            .and_then(|value| u32::try_from(value).ok())
                            .unwrap_or(0)
                    })
                    .collect(),
                _ => Vec::new(),
            };
            return Ok(FigureUvMesh {
                material_names,
                material_indices,
                base_polygons,
                uv_polygons,
                uvs,
            });
        }
    }
    Err(bundle_error("no canonical figure mesh in this bundle"))
}

const DAZ_SCALP_REQUIRED_FIELDS: &[&str] = &[
    "m_GameObject",
    "_numBasePolygons",
    "_numUVVertices",
    "_materialNames",
    "_UVVertices",
    "_UVPolyList",
    "_OrigUV",
];

pub(crate) fn extract_daz_scalp_provider_meshes(
    bundle: &[u8],
) -> Result<Vec<DazScalpProviderMesh>> {
    let (data_area, nodes) = decode_unity_bundle(bundle)?;
    let mut providers = BTreeMap::<String, DazScalpProviderMesh>::new();
    for node in nodes {
        if node.path.ends_with(".resS") || node.path.ends_with(".resource") {
            continue;
        }
        let end = node
            .offset
            .checked_add(node.size)
            .ok_or_else(|| bundle_error("UnityFS node byte range overflow"))?;
        let cab = data_area
            .get(node.offset..end)
            .ok_or_else(|| bundle_error("UnityFS node exceeds decompressed data"))?;
        for provider in collect_daz_scalp_provider_meshes(cab)? {
            let key = provider.object_name.to_ascii_lowercase();
            if let Some(existing) = providers.get(&key) {
                if existing != &provider {
                    return Err(bundle_error(format!(
                        "person bundle contains conflicting {} scalp providers",
                        provider.object_name
                    )));
                }
                continue;
            }
            providers.insert(key, provider);
        }
    }
    Ok(providers.into_values().collect())
}

fn collect_daz_scalp_provider_meshes(cab: &[u8]) -> Result<Vec<DazScalpProviderMesh>> {
    let asset = parse_serialized_asset(cab)?;

    let Some(script_path_id) = find_script_by_class_name(cab, &asset, "DAZMesh")? else {
        return Ok(Vec::new());
    };
    let game_objects = collect_game_object_names(cab, &asset)?;
    let mut providers = Vec::new();
    for object in &asset.objects {
        let serialized_type = &asset.types[object.type_index];
        if serialized_type.class_id != 114
            || mono_behaviour_script_reference(cab, object, asset.endian)? != (0, script_path_id)
        {
            continue;
        }
        let Some(tree) = serialized_type.tree.as_ref() else {
            continue;
        };
        let owner = TreeValue::Record(decode_record_until(
            cab,
            object,
            tree,
            asset.endian,
            &["m_GameObject"],
        )?);
        let game_object_path = pointer_path_id(owner.field("m_GameObject")).unwrap_or_default();
        let Some(object_name) = game_objects.get(&game_object_path) else {
            continue;
        };
        if !object_name.to_ascii_lowercase().ends_with("scalp") {
            continue;
        }
        let record = TreeValue::Record(decode_record_until(
            cab,
            object,
            tree,
            asset.endian,
            DAZ_SCALP_REQUIRED_FIELDS,
        )?);
        providers.push(scalp_provider_from_record(object_name, &record)?);
    }
    Ok(providers)
}

fn collect_game_object_names(cab: &[u8], asset: &SerializedAsset) -> Result<BTreeMap<i64, String>> {
    let mut names = BTreeMap::new();
    for object in &asset.objects {
        let serialized_type = &asset.types[object.type_index];
        if serialized_type.class_id != 1 {
            continue;
        }
        let Some(tree) = serialized_type.tree.as_ref() else {
            continue;
        };
        let record = TreeValue::Record(decode_record_until(
            cab,
            object,
            tree,
            asset.endian,
            &["m_Name"],
        )?);
        if let Some(name) = record.field("m_Name").and_then(TreeValue::as_text) {
            names.insert(object.path_id, name.to_owned());
        }
    }
    Ok(names)
}

pub(crate) fn pointer_path_id(pointer: Option<&TreeValue>) -> Option<i64> {
    pointer?.field("m_PathID").and_then(TreeValue::as_signed)
}

fn scalp_provider_from_record(
    object_name: &str,
    record: &TreeValue,
) -> Result<DazScalpProviderMesh> {
    let count = |name: &str| {
        record
            .field(name)
            .and_then(TreeValue::as_signed)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| bundle_error(format!("{object_name} is missing count field {name}")))
    };
    let declared_polygons = count("_numBasePolygons")?;
    let declared_vertices = count("_numUVVertices")?;
    let material_names = text_list(record.field("_materialNames"))
        .ok_or_else(|| bundle_error(format!("{object_name} has no material names")))?;
    let vertices_m = match record.field("_UVVertices") {
        Some(TreeValue::Vector3Array(points)) => points.clone(),
        _ => {
            return Err(bundle_error(format!(
                "{object_name} has no UV vertex stream"
            )));
        }
    };
    let polygons = polygon_list(record.field("_UVPolyList"), object_name)?;
    let uvs = vector2_list(record.field("_OrigUV"))
        .ok_or_else(|| bundle_error(format!("{object_name} has no original UV stream")))?;
    if vertices_m.len() != declared_vertices || uvs.len() != declared_vertices {
        return Err(bundle_error(format!(
            "{object_name} declares {declared_vertices} UV vertices but stores {} positions and {} coordinates",
            vertices_m.len(),
            uvs.len()
        )));
    }
    if polygons.len() != declared_polygons {
        return Err(bundle_error(format!(
            "{object_name} declares {declared_polygons} polygons but stores {}",
            polygons.len()
        )));
    }
    if polygons
        .iter()
        .flatten()
        .any(|&index| index as usize >= declared_vertices)
    {
        return Err(bundle_error(format!(
            "{object_name} UV polygon references a missing vertex"
        )));
    }
    Ok(DazScalpProviderMesh {
        object_name: object_name.to_owned(),
        material_names,
        vertices_m,
        polygons,
        uvs,
    })
}

fn text_list(value: Option<&TreeValue>) -> Option<Vec<String>> {
    let TreeValue::List(values) = value? else {
        return None;
    };
    values
        .iter()
        .map(|value| value.as_text().map(str::to_owned))
        .collect()
}

fn polygon_list(value: Option<&TreeValue>, object_name: &str) -> Result<Vec<Vec<u32>>> {
    let Some(TreeValue::List(polygons)) = value else {
        return Err(bundle_error(format!(
            "{object_name} has no UV polygon stream"
        )));
    };
    polygons
        .iter()
        .map(|polygon| {
            let Some(TreeValue::List(indices)) = polygon.field("vertices") else {
                return Err(bundle_error(format!(
                    "{object_name} UV polygon has no vertices"
                )));
            };
            indices
                .iter()
                .map(|index| {
                    index
                        .as_signed()
                        .and_then(|value| u32::try_from(value).ok())
                        .ok_or_else(|| {
                            bundle_error(format!("{object_name} UV polygon index is invalid"))
                        })
                })
                .collect()
        })
        .collect()
}

fn vector2_list(value: Option<&TreeValue>) -> Option<Vec<[f32; 2]>> {
    let TreeValue::List(values) = value? else {
        return None;
    };
    values
        .iter()
        .map(|value| {
            Some([
                value.field("x")?.as_float()? as f32,
                value.field("y")?.as_float()? as f32,
            ])
        })
        .collect()
}

fn candidate_to_neutral_base(
    candidate: DazMeshCandidate,
    sex: GeometrySex,
    bundle_path: &Path,
    bundle_sha256: [u8; 32],
) -> Result<NeutralBaseMesh> {
    let mut minimum_y = f64::INFINITY;
    let mut maximum_y = f64::NEG_INFINITY;
    let vertices = candidate
        .base_vertices
        .iter()
        .map(|point| {
            let converted = [
                -f64::from(point[0]),
                f64::from(point[1]),
                f64::from(point[2]),
            ];
            minimum_y = minimum_y.min(converted[1]);
            maximum_y = maximum_y.max(converted[1]);
            converted
        })
        .collect::<Vec<_>>();
    if vertices.iter().flatten().any(|value| !value.is_finite()) {
        return Err(bundle_error("base mesh contains a non-finite coordinate"));
    }
    let height = maximum_y - minimum_y;
    if !(MIN_BASE_HEIGHT_M..=MAX_BASE_HEIGHT_M).contains(&height) {
        return Err(bundle_error(format!(
            "base mesh height {height:.4} m is outside {MIN_BASE_HEIGHT_M}..={MAX_BASE_HEIGHT_M} m"
        )));
    }

    let mut faces = Vec::with_capacity(candidate.base_polys.len());
    let mut polygon_streams = Vec::with_capacity(candidate.base_polys.len());
    for (face_index, (material_index, vertex_indices)) in
        candidate.base_polys.into_iter().enumerate()
    {
        if !matches!(vertex_indices.len(), 3 | 4) {
            return Err(bundle_error(format!(
                "base polygon {face_index} has {} corners; expected three or four",
                vertex_indices.len()
            )));
        }
        if vertex_indices
            .iter()
            .any(|&index| index as usize >= vertices.len())
        {
            return Err(bundle_error(format!(
                "base polygon {face_index} references a missing vertex"
            )));
        }
        let material = candidate
            .material_names
            .get(material_index as usize)
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                bundle_error(format!(
                    "base polygon {face_index} references missing material {material_index}"
                ))
            })?;
        polygon_streams.push(vertex_indices.clone());
        faces.push(ObjFace {
            vertex_indices,
            group: Some(material.clone()),
            material: Some(material.clone()),
        });
    }

    let topology_sha256 = topology_digest(vertices.len(), &polygon_streams)
        .map_err(|error| bundle_error(error.to_string()))?;
    if topology_sha256 != G2F_TOPOLOGY_SHA256 {
        return Err(bundle_error(format!(
            "DAZMesh {} polygon stream does not match the canonical Genesis 2 topology",
            candidate.geometry_id
        )));
    }

    Ok(NeutralBaseMesh {
        sex,
        geometry_id: candidate.geometry_id,
        scene_node_id: candidate.scene_node_id,
        object_path_id: candidate.path_id,
        bundle_path: bundle_path.to_path_buf(),
        bundle_sha256,
        topology_sha256,
        mesh: OrderedObjMesh { vertices, faces },
    })
}

pub fn extract_merged_mesh_from_bundle(bundle: &[u8], sex: GeometrySex) -> Result<Vec<[f64; 3]>> {
    let (data_area, nodes) = decode_unity_bundle(bundle)?;
    let prefixes = match sex {
        GeometrySex::Female => FEMALE_GEOMETRY_ID_PREFIXES,
        GeometrySex::Male => MALE_GEOMETRY_ID_PREFIXES,
    };
    let mut best: Option<DazMeshCandidate> = None;
    for node in &nodes {
        if node.path.ends_with(".resS") || node.path.ends_with(".resource") {
            continue;
        }
        let end = node
            .offset
            .checked_add(node.size)
            .ok_or_else(|| bundle_error("UnityFS node byte range overflow"))?;
        let cab = data_area
            .get(node.offset..end)
            .ok_or_else(|| bundle_error("UnityFS node exceeds decompressed data"))?;
        for candidate in collect_daz_mesh_candidates(cab)? {
            let merged = candidate.geometry_id.contains(':')
                && prefixes
                    .iter()
                    .any(|prefix| candidate.geometry_id.starts_with(prefix));
            if merged
                && best
                    .as_ref()
                    .is_none_or(|held| candidate.base_vertices.len() > held.base_vertices.len())
            {
                best = Some(candidate);
            }
        }
    }
    let candidate =
        best.ok_or_else(|| bundle_error("the bundle carries no merged figure mesh for this sex"))?;

    Ok(candidate
        .base_vertices
        .into_iter()
        .map(|vertex| {
            [
                f64::from(vertex[0]),
                f64::from(vertex[1]),
                f64::from(vertex[2]),
            ]
        })
        .collect())
}

pub fn merged_figure_parts(
    bundle: &[u8],
    sex: GeometrySex,
) -> Result<Vec<(i64, String, usize, usize)>> {
    let (data_area, nodes) = decode_unity_bundle(bundle)?;
    let prefixes = match sex {
        GeometrySex::Female => FEMALE_GEOMETRY_ID_PREFIXES,
        GeometrySex::Male => MALE_GEOMETRY_ID_PREFIXES,
    };
    let mut meshes: Vec<DazMeshCandidate> = Vec::new();
    for node in &nodes {
        if node.path.ends_with(".resS") || node.path.ends_with(".resource") {
            continue;
        }
        let end = node
            .offset
            .checked_add(node.size)
            .ok_or_else(|| bundle_error("UnityFS node byte range overflow"))?;
        let cab = data_area
            .get(node.offset..end)
            .ok_or_else(|| bundle_error("UnityFS node exceeds decompressed data"))?;
        meshes.extend(collect_daz_mesh_candidates(cab)?);
    }
    let merged = meshes
        .iter()
        .filter(|candidate| {
            candidate.geometry_id.contains(':')
                && prefixes
                    .iter()
                    .any(|prefix| candidate.geometry_id.starts_with(prefix))
        })
        .max_by_key(|candidate| candidate.base_vertices.len())
        .ok_or_else(|| bundle_error("the bundle carries no merged figure mesh for this sex"))?;

    let mut parts = Vec::new();
    let mut offset = 0_usize;
    for name in merged.geometry_id.split(':') {
        let part = meshes
            .iter()
            .find(|candidate| candidate.geometry_id == name)
            .ok_or_else(|| {
                bundle_error("the merged figure names a mesh the bundle does not carry")
            })?;
        let count = part.base_vertices.len();
        parts.push((part.path_id, part.geometry_id.clone(), offset, count));
        offset += count;
    }
    if offset != merged.base_vertices.len() {
        return Err(bundle_error(
            "the figure's parts do not add up to the merged mesh",
        ));
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StringTable {
        buffer: Vec<u8>,
    }

    impl StringTable {
        fn new() -> Self {
            Self { buffer: Vec::new() }
        }

        fn offset(&mut self, text: &str) -> u32 {
            if let Some(common) = common_offset(text) {
                return common | 0x8000_0000;
            }
            let offset = self.buffer.len() as u32;
            self.buffer.extend_from_slice(text.as_bytes());
            self.buffer.push(0);
            offset
        }
    }

    fn common_offset(text: &str) -> Option<u32> {
        (0..1200).find(|&offset| common_type_string(offset) == Some(text))
    }

    struct TreeSpec {
        type_name: &'static str,
        name: &'static str,
        level: u8,
        meta: u32,
    }

    fn node(type_name: &'static str, name: &'static str, level: u8, meta: u32) -> TreeSpec {
        TreeSpec {
            type_name,
            name,
            level,
            meta,
        }
    }

    fn encode_tree(specs: &[TreeSpec]) -> (Vec<u8>, Vec<u8>) {
        let mut table = StringTable::new();
        let mut records = Vec::new();
        for spec in specs {
            let type_offset = table.offset(spec.type_name);
            let name_offset = table.offset(spec.name);
            records.extend_from_slice(&1_u16.to_le_bytes());
            records.push(spec.level);
            records.push(0);
            records.extend_from_slice(&type_offset.to_le_bytes());
            records.extend_from_slice(&name_offset.to_le_bytes());
            records.extend_from_slice(&(-1_i32).to_le_bytes());
            records.extend_from_slice(&0_i32.to_le_bytes());
            records.extend_from_slice(&spec.meta.to_le_bytes());
        }
        (records, table.buffer)
    }

    fn string_nodes(name: &'static str, level: u8) -> Vec<TreeSpec> {
        vec![
            node("string", name, level, 0x8000),
            node("Array", "Array", level + 1, 0x4001),
            node("int", "size", level + 2, 0x0001),
            node("char", "data", level + 2, 0x0001),
        ]
    }

    fn mono_script_tree() -> Vec<TreeSpec> {
        let mut specs = vec![node("MonoScript", "Base", 0, 0x8000)];
        specs.extend(string_nodes("m_Name", 1));
        specs.extend(string_nodes("m_ClassName", 1));
        specs
    }

    fn daz_mesh_tree() -> Vec<TreeSpec> {
        let mut specs = vec![node("MonoBehaviour", "Base", 0, 0x8000)];
        specs.push(node("PPtr<GameObject>", "m_GameObject", 1, 0));
        specs.push(node("int", "m_FileID", 2, 0));
        specs.push(node("SInt64", "m_PathID", 2, 0));
        specs.push(node("UInt8", "m_Enabled", 1, 0x4101));
        specs.push(node("PPtr<MonoScript>", "m_Script", 1, 0));
        specs.push(node("int", "m_FileID", 2, 0));
        specs.push(node("SInt64", "m_PathID", 2, 0));
        specs.extend(string_nodes("m_Name", 1));
        specs.push(node("UInt8", "drawBaseMesh", 1, 0x4100));
        specs.extend(string_nodes("sceneNodeId", 1));
        specs.extend(string_nodes("geometryId", 1));
        specs.push(node("int", "_numBaseVertices", 1, 0));
        specs.push(node("int", "_numBasePolygons", 1, 0));

        specs.push(node("vector", "_materialNames", 1, 0x8000));
        specs.push(node("Array", "Array", 2, 0xc000));
        specs.push(node("int", "size", 3, 0));
        specs.extend(string_nodes("data", 3));

        specs.push(node("vector", "_baseVertices", 1, 0x8000));
        specs.push(node("Array", "Array", 2, 0x4000));
        specs.push(node("int", "size", 3, 0));
        specs.push(node("Vector3f", "data", 3, 0x200000));
        specs.push(node("float", "x", 4, 0x200000));
        specs.push(node("float", "y", 4, 0x200000));
        specs.push(node("float", "z", 4, 0x200000));

        specs.push(node("MeshPoly", "_basePolyList", 1, 0x8000));
        specs.push(node("Array", "Array", 2, 0x8000));
        specs.push(node("int", "size", 3, 0));
        specs.push(node("MeshPoly", "data", 3, 0));
        specs.push(node("int", "materialNum", 4, 0));
        specs.push(node("vector", "vertices", 4, 0));
        specs.push(node("Array", "Array", 5, 0x4000));
        specs.push(node("int", "size", 6, 0));
        specs.push(node("int", "data", 6, 0));

        specs.push(node("TrailingGarbage", "neverDecoded", 1, 0));
        specs.push(node("UnknownLeaf", "unknown", 2, 0));
        specs
    }

    fn push_aligned_string(bytes: &mut Vec<u8>, text: &str) {
        bytes.extend_from_slice(&(text.len() as u32).to_le_bytes());
        bytes.extend_from_slice(text.as_bytes());
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
    }

    fn mono_script_payload(name: &str, class_name: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_aligned_string(&mut bytes, name);
        push_aligned_string(&mut bytes, class_name);
        bytes
    }

    struct SyntheticMesh {
        script_path_id: i64,
        geometry_id: &'static str,
        scene_node_id: &'static str,
        materials: &'static [&'static str],
        vertices: Vec<[f32; 3]>,
        polys: Vec<(u32, Vec<u32>)>,
    }

    fn daz_mesh_payload(mesh: &SyntheticMesh) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(&0_i64.to_le_bytes());
        bytes.push(1);
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(&mesh.script_path_id.to_le_bytes());
        push_aligned_string(&mut bytes, "body");
        bytes.push(1);
        bytes.extend_from_slice(&[0, 0, 0]);
        push_aligned_string(&mut bytes, mesh.scene_node_id);
        push_aligned_string(&mut bytes, mesh.geometry_id);
        bytes.extend_from_slice(&(mesh.vertices.len() as i32).to_le_bytes());
        bytes.extend_from_slice(&(mesh.polys.len() as i32).to_le_bytes());
        bytes.extend_from_slice(&(mesh.materials.len() as i32).to_le_bytes());
        for material in mesh.materials {
            push_aligned_string(&mut bytes, material);
        }
        bytes.extend_from_slice(&(mesh.vertices.len() as i32).to_le_bytes());
        for vertex in &mesh.vertices {
            for &coordinate in vertex {
                bytes.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&(mesh.polys.len() as i32).to_le_bytes());
        for (material, indices) in &mesh.polys {
            bytes.extend_from_slice(&(*material as i32).to_le_bytes());
            bytes.extend_from_slice(&(indices.len() as i32).to_le_bytes());
            for &index in indices {
                bytes.extend_from_slice(&(index as i32).to_le_bytes());
            }
        }

        bytes.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        bytes
    }

    fn synthetic_serialized_file(objects: &[(i64, usize, Vec<u8>)]) -> Vec<u8> {
        let trees = [mono_script_tree(), daz_mesh_tree()];
        let class_ids = [115_i32, 114_i32];

        let mut metadata = Vec::new();
        metadata.extend_from_slice(b"2018.1.9f1\0");
        metadata.extend_from_slice(&19_i32.to_le_bytes());
        metadata.push(1);
        metadata.extend_from_slice(&(trees.len() as u32).to_le_bytes());
        for (class_id, specs) in class_ids.iter().zip(&trees) {
            metadata.extend_from_slice(&class_id.to_le_bytes());
            metadata.push(0);
            metadata.extend_from_slice(&(-1_i16).to_le_bytes());
            if *class_id == 114 {
                metadata.extend_from_slice(&[0_u8; 16]);
            }
            metadata.extend_from_slice(&[0_u8; 16]);
            let (records, strings) = encode_tree(specs);
            metadata.extend_from_slice(&(specs.len() as u32).to_le_bytes());
            metadata.extend_from_slice(&(strings.len() as u32).to_le_bytes());
            metadata.extend_from_slice(&records);
            metadata.extend_from_slice(&strings);
        }
        metadata.extend_from_slice(&(objects.len() as u32).to_le_bytes());
        let mut data = Vec::new();
        for (path_id, type_index, payload) in objects {
            while metadata.len() % 4 != 0 {
                metadata.push(0);
            }
            metadata.extend_from_slice(&path_id.to_le_bytes());
            metadata.extend_from_slice(&(data.len() as u32).to_le_bytes());
            metadata.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            metadata.extend_from_slice(&(*type_index as i32).to_le_bytes());
            data.extend_from_slice(payload);
            while data.len() % 8 != 0 {
                data.push(0);
            }
        }
        metadata.extend_from_slice(&0_u32.to_le_bytes());
        metadata.extend_from_slice(&0_u32.to_le_bytes());

        let header_len = 20;
        let data_offset = (header_len + metadata.len()).div_ceil(16) * 16;
        let file_size = data_offset + data.len();
        let mut file = Vec::with_capacity(file_size);
        file.extend_from_slice(&(metadata.len() as u32).to_be_bytes());
        file.extend_from_slice(&(file_size as u32).to_be_bytes());
        file.extend_from_slice(&17_u32.to_be_bytes());
        file.extend_from_slice(&(data_offset as u32).to_be_bytes());
        file.extend_from_slice(&[0, 0, 0, 0]);
        file.extend_from_slice(&metadata);
        file.resize(data_offset, 0);
        file.extend_from_slice(&data);
        file
    }

    fn synthetic_bundle(cab: &[u8]) -> Vec<u8> {
        let mut blocks_info = Vec::new();
        blocks_info.extend_from_slice(&[0_u8; 16]);
        blocks_info.extend_from_slice(&1_u32.to_be_bytes());
        blocks_info.extend_from_slice(&(cab.len() as u32).to_be_bytes());
        blocks_info.extend_from_slice(&(cab.len() as u32).to_be_bytes());
        blocks_info.extend_from_slice(&0_u16.to_be_bytes());
        blocks_info.extend_from_slice(&1_u32.to_be_bytes());
        blocks_info.extend_from_slice(&0_u64.to_be_bytes());
        blocks_info.extend_from_slice(&(cab.len() as u64).to_be_bytes());
        blocks_info.extend_from_slice(&0_u32.to_be_bytes());
        blocks_info.extend_from_slice(b"CAB-synthetic\0");
        let packed_info = lz4_flex::block::compress(&blocks_info);

        let mut bundle = Vec::new();
        bundle.extend_from_slice(b"UnityFS\0");
        bundle.extend_from_slice(&6_u32.to_be_bytes());
        bundle.extend_from_slice(b"5.x.x\0");
        bundle.extend_from_slice(b"2018.1.9f1\0");
        let header_tail = 8 + 4 + 4 + 4;
        let total = bundle.len() + header_tail + packed_info.len() + cab.len();
        bundle.extend_from_slice(&(total as u64).to_be_bytes());
        bundle.extend_from_slice(&(packed_info.len() as u32).to_be_bytes());
        bundle.extend_from_slice(&(blocks_info.len() as u32).to_be_bytes());
        bundle.extend_from_slice(&0x43_u32.to_be_bytes());
        bundle.extend_from_slice(&packed_info);
        bundle.extend_from_slice(cab);
        bundle
    }

    fn synthetic_mesh() -> SyntheticMesh {
        SyntheticMesh {
            script_path_id: 77,
            geometry_id: "GenesisFemale-1",
            scene_node_id: "Genesis2Female",
            materials: &["Face", "Head"],
            vertices: vec![
                [0.25, 0.0, 0.0],
                [-0.5, 1.0, 0.125],
                [0.0, 2.0, -0.25],
                [0.75, 1.5, 0.5],
            ],
            polys: vec![(0, vec![0, 1, 2]), (1, vec![0, 2, 3, 1])],
        }
    }

    fn synthetic_cab() -> Vec<u8> {
        let mesh = synthetic_mesh();
        synthetic_serialized_file(&[
            (77, 0, mono_script_payload("DAZMesh", "DAZMesh")),
            (78, 0, mono_script_payload("DAZMergedMesh", "DAZMergedMesh")),
            (900, 1, daz_mesh_payload(&mesh)),
        ])
    }

    #[test]
    fn synthetic_serialized_file_yields_the_daz_mesh_candidate() {
        let cab = synthetic_cab();
        let candidates = collect_daz_mesh_candidates(&cab).unwrap();
        assert_eq!(candidates.len(), 1);
        let candidate = &candidates[0];
        assert_eq!(candidate.path_id, 900);
        assert_eq!(candidate.geometry_id, "GenesisFemale-1");
        assert_eq!(candidate.scene_node_id, "Genesis2Female");
        assert_eq!(candidate.material_names, ["Face", "Head"]);
        assert_eq!(candidate.base_vertices.len(), 4);
        assert_eq!(candidate.base_vertices[1], [-0.5, 1.0, 0.125]);
        assert_eq!(
            candidate.base_polys,
            [(0, vec![0, 1, 2]), (1, vec![0, 2, 3, 1])]
        );
    }

    #[test]
    fn synthetic_unity_bundle_round_trips_to_the_same_candidate() {
        let bundle = synthetic_bundle(&synthetic_cab());
        let (data, nodes) = decode_unity_bundle(&bundle).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].path, "CAB-synthetic");
        let cab = &data[nodes[0].offset..nodes[0].offset + nodes[0].size];
        let candidates = collect_daz_mesh_candidates(cab).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].geometry_id, "GenesisFemale-1");
    }

    #[test]
    fn extraction_rejects_a_body_with_non_canonical_counts() {
        let bundle = synthetic_bundle(&synthetic_cab());
        let error = extract_neutral_base_from_bundle(
            &bundle,
            Path::new("synthetic-bundle"),
            GeometrySex::Female,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("21556"), "{error}");
        assert!(error.contains("GenesisFemale-1"), "{error}");
    }

    #[test]
    fn candidate_conversion_negates_x_and_binds_materials() {
        let mesh = synthetic_mesh();
        let candidate = DazMeshCandidate {
            path_id: 900,
            scene_node_id: mesh.scene_node_id.to_owned(),
            geometry_id: mesh.geometry_id.to_owned(),
            material_names: mesh
                .materials
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            base_vertices: mesh.vertices.clone(),
            base_polys: mesh.polys.clone(),
        };

        let error = candidate_to_neutral_base(
            candidate.clone(),
            GeometrySex::Female,
            Path::new("synthetic"),
            [0; 32],
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("canonical Genesis 2 topology"),
            "small synthetic meshes must fail the topology gate: {error}"
        );

        let mut squashed = candidate;
        squashed.base_vertices[2][1] = 0.5;
        squashed.base_vertices[3][1] = 0.5;
        let error = candidate_to_neutral_base(
            squashed,
            GeometrySex::Female,
            Path::new("synthetic"),
            [0; 32],
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("height"), "{error}");
    }

    #[test]
    fn cache_blob_round_trips_and_detects_corruption() {
        let base = NeutralBaseMesh {
            sex: GeometrySex::Male,
            geometry_id: "Genesis2Male".to_owned(),
            scene_node_id: "Genesis2Male".to_owned(),
            object_path_id: -42,
            bundle_path: PathBuf::from("m_1"),
            bundle_sha256: [7; 32],
            topology_sha256: G2F_TOPOLOGY_SHA256,
            mesh: OrderedObjMesh {
                vertices: vec![[0.1, 0.0, 0.0]; G2F_VERTEX_COUNT],
                faces: (0..G2F_POLYGON_COUNT)
                    .map(|index| ObjFace {
                        vertex_indices: vec![
                            (index % G2F_VERTEX_COUNT) as u32,
                            ((index + 1) % G2F_VERTEX_COUNT) as u32,
                            ((index + 2) % G2F_VERTEX_COUNT) as u32,
                        ],
                        group: Some("Face".to_owned()),
                        material: Some("Face".to_owned()),
                    })
                    .collect(),
            },
        };
        let blob = encode_cache_blob(&base).unwrap();

        let error = decode_cache_blob(&blob, GeometrySex::Male, Path::new("m_1"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("canonical Genesis 2 topology"), "{error}");

        let mut corrupted = blob;
        let flip_at = corrupted.len() / 2;
        corrupted[flip_at] ^= 0xff;
        let error = decode_cache_blob(&corrupted, GeometrySex::Male, Path::new("m_1"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("integrity"), "{error}");
    }

    #[test]
    fn the_cheap_cache_key_is_the_path_the_length_and_the_modification_time() {
        let path = Path::new("m_1");
        let stamp = BundleStamp {
            bytes: 4_351_915,
            modified_ns: 1_700_000_000_123_456_789,
        };
        let receipt = NeutralBaseCacheReceipt {
            schema_version: CACHE_SCHEMA_VERSION,
            sex: "male".to_owned(),
            bundle_path: path.to_string_lossy().into_owned(),
            bundle_bytes: stamp.bytes,
            bundle_modified_ns: Some(stamp.modified_ns.to_string()),
            bundle_sha256: hex(&[7; 32]),
            object_path_id: -42,
            geometry_id: "Genesis2Male".to_owned(),
            scene_node_id: "Genesis2Male".to_owned(),
            vertex_count: G2F_VERTEX_COUNT,
            polygon_count: G2F_POLYGON_COUNT,
            topology_sha256: hex(&G2F_TOPOLOGY_SHA256),
            vertex_stream_sha256: hex(&[9; 32]),
        };
        assert!(receipt_matches_stamp(&receipt, path, stamp));
        assert!(!receipt_matches_stamp(
            &receipt,
            path,
            BundleStamp {
                bytes: stamp.bytes + 1,
                ..stamp
            }
        ));
        assert!(!receipt_matches_stamp(
            &receipt,
            path,
            BundleStamp {
                modified_ns: stamp.modified_ns + 1,
                ..stamp
            }
        ));
        assert!(!receipt_matches_stamp(&receipt, Path::new("f_1"), stamp));

        let mut stored = serde_json::to_value(&receipt).unwrap();
        stored
            .as_object_mut()
            .unwrap()
            .remove("bundle_modified_ns")
            .unwrap();
        let legacy: NeutralBaseCacheReceipt = serde_json::from_value(stored).unwrap();
        assert_eq!(legacy.bundle_modified_ns, None);
        assert!(!receipt_matches_stamp(&legacy, path, stamp));

        assert_eq!(
            decode_hex32(&hex(&G2F_TOPOLOGY_SHA256)),
            Some(G2F_TOPOLOGY_SHA256)
        );
        assert_eq!(decode_hex32(&hex(&[7; 32])), Some([7; 32]));
        assert_eq!(decode_hex32("not a digest"), None);
    }

    #[test]
    fn mono_script_lookup_requires_the_daz_mesh_class_name() {
        let cab = synthetic_serialized_file(&[
            (11, 0, mono_script_payload("SomethingElse", "SomethingElse")),
            (900, 1, daz_mesh_payload(&synthetic_mesh())),
        ]);
        assert!(collect_daz_mesh_candidates(&cab).unwrap().is_empty());
    }

    #[test]
    #[ignore = "requires the user's VaM installation"]
    fn real_a_per_exposes_udane_scalp_with_uvs() {
        let root = VaMRoot::open(std::env::var_os("VKIT_VAM_ROOT").expect("set VKIT_VAM_ROOT"))
            .expect("open the VaM root");
        let bundle_path = root
            .path()
            .join("VaM_Data")
            .join("StreamingAssets")
            .join("a_per");
        let bytes = fs::read(bundle_path).expect("read a_per");
        let providers =
            extract_daz_scalp_provider_meshes(&bytes).expect("extract built-in scalp providers");
        let udane = providers
            .iter()
            .find(|provider| provider.object_name == "UdaneScalp")
            .expect("UdaneScalp provider");
        assert_eq!(udane.vertices_m.len(), 868);
        assert_eq!(udane.uvs.len(), 868);
        assert_eq!(udane.polygons.len(), 806);
        assert!(udane.material_names.iter().any(|name| name == "scalp"));
    }

    #[test]
    #[ignore = "requires the user's VaM installation"]
    fn real_vam_bundles_extract_both_neutral_bases() {
        let root = VaMRoot::open(std::env::var_os("VKIT_VAM_ROOT").expect("set VKIT_VAM_ROOT"))
            .expect("open the VaM root");
        for (sex, prefixes) in [
            (GeometrySex::Female, FEMALE_GEOMETRY_ID_PREFIXES),
            (GeometrySex::Male, MALE_GEOMETRY_ID_PREFIXES),
        ] {
            let base = extract_neutral_base(&root, sex).expect("extract the neutral base");
            assert_eq!(base.mesh.vertices.len(), G2F_VERTEX_COUNT);
            assert_eq!(base.mesh.faces.len(), G2F_POLYGON_COUNT);
            assert_eq!(base.topology_sha256, G2F_TOPOLOGY_SHA256);

            assert!(
                base.mesh.vertices[0][0] > 0.0,
                "extraction must negate the stored X axis"
            );
            assert!(
                prefixes
                    .iter()
                    .any(|prefix| base.geometry_id.starts_with(prefix)),
                "unexpected geometryId {}",
                base.geometry_id
            );
            base.mesh.validate().expect("valid ordered mesh");

            let workspace = std::env::temp_dir().join(format!(
                "vkit-neutral-base-cache-{}-{:?}",
                std::process::id(),
                sex
            ));
            let _ = std::fs::remove_dir_all(&workspace);
            std::fs::create_dir_all(&workspace).unwrap();
            let first = load_or_extract_neutral_base(&root, sex, Some(&workspace)).unwrap();
            assert!(!first.from_cache);
            let second = load_or_extract_neutral_base(&root, sex, Some(&workspace)).unwrap();
            assert!(second.from_cache);
            assert_eq!(first.base, second.base);
            let receipt = read_neutral_base_receipt(&workspace, sex).expect("cache receipt");
            assert_eq!(receipt.vertex_count, G2F_VERTEX_COUNT);
            assert_eq!(receipt.polygon_count, G2F_POLYGON_COUNT);
            std::fs::remove_dir_all(&workspace).unwrap();
        }
    }
}
