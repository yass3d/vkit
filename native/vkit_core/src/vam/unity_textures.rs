use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::pixels;

use super::catalog::VaMRoot;
use super::skin::{AssetLocator, SkinDiffuseSource, SkinPreset, SkinSex};
use super::unity_base::{TreeValue, TypeTreeNode, build_type_tree, decode_value};
use super::unity_morph_bank::{ByteReader, Endian, bounded_i32, decompress_chunk, usize_from_u32};
use super::{Result, VaMError, io_error};

const MAX_BUNDLE_BLOCKS: usize = 100_000;
const MAX_BUNDLE_NODES: usize = 64;
const MAX_METADATA_BYTES: usize = 8 * 1024 * 1024;
const MAX_TYPES: usize = 4_096;
const MAX_OBJECTS: usize = 100_000;
const MAX_TEXTURE_BYTES: usize = 128 * 1024 * 1024;
const OBJECT_HEADER_WINDOW: usize = 64 * 1024;
const INDEX_SCHEMA_VERSION: u32 = 3;

const CACHE_MAGIC: [u8; 8] = *b"FMSKTX06";
const CACHE_DIRECTORY: &str = "builtin-skins";

const FORMAT_ALPHA8: i64 = 1;
const FORMAT_RGB24: i64 = 3;
const FORMAT_RGBA32: i64 = 4;
const FORMAT_ARGB32: i64 = 5;
const FORMAT_DXT1: i64 = 10;
const FORMAT_DXT5: i64 = 12;

fn texture_error(message: impl Into<String>) -> VaMError {
    VaMError::InvalidBaseBundle(message.into())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BuiltinTextureRef {
    pub bundle_path: PathBuf,

    pub cab_node: String,

    pub path_id: i64,

    pub texture_name: String,

    #[serde(default)]
    pub normal_map: bool,

    pub cache_directory: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct DecodedBuiltinTexture {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

struct BundleBlockSpan {
    uncompressed_offset: u64,
    uncompressed_size: usize,
    compressed_offset: u64,
    compressed_size: usize,
    flags: u32,
}

struct BundleNodeSpan {
    offset: u64,
    size: u64,
    path: String,
}

struct BundleReader {
    file: fs::File,
    blocks: Vec<BundleBlockSpan>,
    nodes: Vec<BundleNodeSpan>,
}

impl BundleReader {
    fn open(path: &Path) -> Result<Self> {
        let mut file = fs::File::open(path).map_err(|error| io_error(path, error))?;
        let mut head = vec![0_u8; 4096];
        let read = file
            .read(&mut head)
            .map_err(|error| io_error(path, error))?;
        head.truncate(read);
        let mut header = ByteReader::new(&head, Endian::Big);
        if header.null_text()? != "UnityFS" {
            return Err(texture_error("not a UnityFS bundle"));
        }
        let format_version = header.u32()?;
        if format_version < 6 {
            return Err(VaMError::UnsupportedEncoding(format!(
                "UnityFS format version {format_version}"
            )));
        }
        header.null_text()?;
        header.null_text()?;
        let declared_size = header.u64()?;
        let total = file
            .metadata()
            .map_err(|error| io_error(path, error))?
            .len();
        if declared_size != total {
            return Err(texture_error(format!(
                "UnityFS declares {declared_size} bytes but the file has {total}"
            )));
        }
        let packed_directory_size = usize_from_u32(header.u32()?)?;
        let directory_size = usize_from_u32(header.u32()?)?;
        let flags = header.u32()?;
        let header_end = header.position() as u64;
        let directory_offset = if flags & 0x80 != 0 {
            total
                .checked_sub(packed_directory_size as u64)
                .ok_or_else(|| texture_error("UnityFS directory offset underflow"))?
        } else {
            header_end
        };
        let mut packed_directory = vec![0_u8; packed_directory_size];
        file.seek(SeekFrom::Start(directory_offset))
            .map_err(|error| io_error(path, error))?;
        file.read_exact(&mut packed_directory)
            .map_err(|error| io_error(path, error))?;
        let directory = decompress_chunk(&packed_directory, flags & 0x3f, directory_size)?;

        let mut reader = ByteReader::new(&directory, Endian::Big);
        reader.take(16)?;
        let block_count = usize_from_u32(reader.u32()?)?;
        if block_count > MAX_BUNDLE_BLOCKS {
            return Err(texture_error(format!(
                "UnityFS declares {block_count} blocks"
            )));
        }
        let mut data_cursor = if flags & 0x80 != 0 {
            header_end
        } else {
            header_end + packed_directory_size as u64
        };
        if flags & 0x200 != 0 {
            data_cursor = data_cursor.div_ceil(16) * 16;
        }
        let mut blocks = Vec::with_capacity(block_count);
        let mut uncompressed_offset = 0_u64;
        let mut compressed_offset = data_cursor;
        for _ in 0..block_count {
            let uncompressed_size = usize_from_u32(reader.u32()?)?;
            let compressed_size = usize_from_u32(reader.u32()?)?;
            let block_flags = reader.u16()? as u32;
            blocks.push(BundleBlockSpan {
                uncompressed_offset,
                uncompressed_size,
                compressed_offset,
                compressed_size,
                flags: block_flags,
            });
            uncompressed_offset += uncompressed_size as u64;
            compressed_offset += compressed_size as u64;
        }
        let node_count = usize_from_u32(reader.u32()?)?;
        if node_count > MAX_BUNDLE_NODES {
            return Err(texture_error(format!(
                "UnityFS declares {node_count} nodes"
            )));
        }
        let mut nodes = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            let offset = reader.u64()?;
            let size = reader.u64()?;
            reader.u32()?;
            nodes.push(BundleNodeSpan {
                offset,
                size,
                path: reader.null_text()?,
            });
        }
        Ok(Self {
            file,
            blocks,
            nodes,
        })
    }

    fn node(&self, name_suffix: &str) -> Option<(u64, u64)> {
        self.nodes
            .iter()
            .find(|node| node.path.eq_ignore_ascii_case(name_suffix))
            .map(|node| (node.offset, node.size))
    }

    fn serialized_nodes(&self) -> Vec<(u64, u64, String)> {
        self.nodes
            .iter()
            .filter(|node| {
                !node.path.to_ascii_lowercase().ends_with(".ress")
                    && !node.path.to_ascii_lowercase().ends_with(".resource")
            })
            .map(|node| (node.offset, node.size, node.path.clone()))
            .collect()
    }

    fn read_range(&mut self, start: u64, size: usize) -> Result<Vec<u8>> {
        let end = start
            .checked_add(size as u64)
            .ok_or_else(|| texture_error("bundle range overflow"))?;
        let mut output = Vec::with_capacity(size);
        for block in &self.blocks {
            let block_end = block.uncompressed_offset + block.uncompressed_size as u64;
            if block_end <= start {
                continue;
            }
            if block.uncompressed_offset >= end {
                break;
            }
            self.file
                .seek(SeekFrom::Start(block.compressed_offset))
                .map_err(|error| texture_error(format!("bundle seek failed: {error}")))?;
            let mut compressed = vec![0_u8; block.compressed_size];
            self.file
                .read_exact(&mut compressed)
                .map_err(|error| texture_error(format!("bundle read failed: {error}")))?;
            let data = decompress_chunk(&compressed, block.flags & 0x3f, block.uncompressed_size)?;
            let from = start.saturating_sub(block.uncompressed_offset) as usize;
            let to = (end.min(block_end) - block.uncompressed_offset) as usize;
            output.extend_from_slice(&data[from..to]);
        }
        if output.len() != size {
            return Err(texture_error(format!(
                "bundle range {start}+{size} exceeds the data area ({} bytes gathered)",
                output.len()
            )));
        }
        Ok(output)
    }
}

struct PrefixType {
    class_id: i32,
    tree: Option<TypeTreeNode>,
}

struct PrefixObject {
    path_id: i64,
    byte_start: u64,
    byte_size: usize,
    type_index: usize,
}

struct SerializedPrefix {
    endian: Endian,
    data_offset: u64,
    types: Vec<PrefixType>,
    objects: Vec<PrefixObject>,
    externals: Vec<String>,
}

fn parse_serialized_prefix(
    reader: &mut BundleReader,
    node_offset: u64,
) -> Result<SerializedPrefix> {
    let head = reader.read_range(node_offset, 20)?;
    let mut header = ByteReader::new(&head, Endian::Big);
    header.u32()?;
    header.u32()?;
    let version = header.u32()?;
    let data_offset = u64::from(header.u32()?);
    let endian = if header.byte()? == 0 {
        Endian::Little
    } else {
        Endian::Big
    };
    if !(14..=21).contains(&version) {
        return Err(VaMError::UnsupportedEncoding(format!(
            "serialized asset version {version}"
        )));
    }
    if data_offset as usize > MAX_METADATA_BYTES {
        return Err(texture_error("serialized metadata exceeds the safety cap"));
    }
    let meta = reader.read_range(node_offset, data_offset as usize)?;
    let mut cursor = ByteReader::at(&meta, endian, 20)?;
    cursor.null_text()?;
    cursor.i32()?;
    let has_type_tree = cursor.byte()? != 0;
    if !has_type_tree {
        return Err(VaMError::UnsupportedEncoding(
            "serialized asset carries no type trees".to_owned(),
        ));
    }
    let type_count = bounded_i32(cursor.i32()?, MAX_TYPES, "serialized type")?;
    let mut types = Vec::with_capacity(type_count);
    for _ in 0..type_count {
        let class_id = cursor.i32()?;
        if version >= 16 {
            cursor.byte()?;
        }
        if version >= 17 {
            cursor.i16()?;
        }
        if (version < 16 && class_id < 0) || (version >= 16 && class_id == 114) {
            cursor.take(16)?;
        }
        cursor.take(16)?;
        let node_count = bounded_i32(cursor.i32()?, 1_000_000, "type-tree node")?;
        let string_bytes = bounded_i32(cursor.i32()?, MAX_METADATA_BYTES, "type-tree text")?;
        let node_bytes = if version >= 19 { 32 } else { 24 };
        let records = cursor.take(
            node_count
                .checked_mul(node_bytes)
                .ok_or_else(|| texture_error("type-tree byte count overflow"))?,
        )?;
        let string_buffer = cursor.take(string_bytes)?;
        let tree = build_type_tree(records, node_bytes, node_count, string_buffer, endian)?;
        types.push(PrefixType { class_id, tree });
    }
    let object_count = bounded_i32(cursor.i32()?, MAX_OBJECTS, "serialized object")?;
    let mut objects = Vec::with_capacity(object_count);
    for _ in 0..object_count {
        cursor.align(4)?;
        let path_id = cursor.i64()?;
        let byte_start = u64::from(cursor.u32()?);
        let byte_size = usize_from_u32(cursor.u32()?)?;
        let type_index = cursor.i32()?;
        let type_index = usize::try_from(type_index)
            .ok()
            .filter(|&index| index < types.len())
            .ok_or_else(|| texture_error("serialized object references a missing type"))?;
        objects.push(PrefixObject {
            path_id,
            byte_start,
            byte_size,
            type_index,
        });
    }

    let script_count = bounded_i32(cursor.i32()?, MAX_OBJECTS, "script type")?;
    for _ in 0..script_count {
        cursor.align(4)?;
        cursor.take(12)?;
    }
    let external_count = bounded_i32(cursor.i32()?, 256, "external reference")?;
    let mut externals = Vec::with_capacity(external_count);
    for _ in 0..external_count {
        cursor.null_text()?;
        cursor.take(16)?;
        cursor.i32()?;
        externals.push(cursor.null_text()?);
    }
    Ok(SerializedPrefix {
        endian,
        data_offset,
        types,
        objects,
        externals,
    })
}

fn decode_object_fields(
    reader: &mut BundleReader,
    node_offset: u64,
    prefix: &SerializedPrefix,
    object: &PrefixObject,
    required: &[&str],
) -> Result<TreeValue> {
    let tree = prefix.types[object.type_index]
        .tree
        .as_ref()
        .ok_or_else(|| texture_error("serialized object has no type tree"))?;
    let start = node_offset + prefix.data_offset + object.byte_start;
    let window = object.byte_size.min(OBJECT_HEADER_WINDOW);
    let attempt = |bytes: &[u8]| -> Result<TreeValue> {
        let mut cursor = ByteReader::new(bytes, prefix.endian);
        let mut fields = Vec::new();
        let mut missing: Vec<&str> = required.to_vec();
        for child in &tree.children {
            let value = decode_value(&mut cursor, child, 0)?;
            missing.retain(|name| *name != child.name.as_str());
            fields.push((child.name.clone(), value));
            if missing.is_empty() {
                break;
            }
        }
        if !missing.is_empty() {
            return Err(texture_error(format!(
                "object {} is missing fields {missing:?}",
                object.path_id
            )));
        }
        Ok(TreeValue::Record(fields))
    };
    let bytes = reader.read_range(start, window)?;
    match attempt(&bytes) {
        Ok(value) => Ok(value),
        Err(_) if window < object.byte_size => {
            let bytes = reader.read_range(start, object.byte_size)?;
            attempt(&bytes)
        }
        Err(error) => Err(error),
    }
}

fn format_level_bytes(format: i64, width: u32, height: u32) -> Result<usize> {
    let pixels = width as usize * height as usize;
    Ok(match format {
        FORMAT_ALPHA8 => pixels,
        FORMAT_RGB24 => pixels * 3,
        FORMAT_RGBA32 | FORMAT_ARGB32 => pixels * 4,
        FORMAT_DXT1 => width.div_ceil(4) as usize * height.div_ceil(4) as usize * 8,
        FORMAT_DXT5 => width.div_ceil(4) as usize * height.div_ceil(4) as usize * 16,
        other => {
            return Err(VaMError::UnsupportedEncoding(format!(
                "Unity texture format {other}"
            )));
        }
    })
}

fn decode_level(format: i64, width: u32, height: u32, data: &[u8]) -> Result<Vec<u8>> {
    let rgba = match format {
        FORMAT_DXT1 => pixels::decode_dxt(data, width, height, false).map_err(texture_error)?,
        FORMAT_DXT5 => pixels::decode_dxt(data, width, height, true).map_err(texture_error)?,
        FORMAT_RGB24 => {
            let mut rgba = Vec::with_capacity(data.len() / 3 * 4);
            for pixel in data.chunks_exact(3) {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
            rgba
        }
        FORMAT_RGBA32 => data.to_vec(),
        FORMAT_ARGB32 => {
            let mut rgba = Vec::with_capacity(data.len());
            for pixel in data.chunks_exact(4) {
                rgba.extend_from_slice(&[pixel[1], pixel[2], pixel[3], pixel[0]]);
            }
            rgba
        }
        FORMAT_ALPHA8 => {
            let mut rgba = Vec::with_capacity(data.len() * 4);
            for &alpha in data {
                rgba.extend_from_slice(&[255, 255, 255, alpha]);
            }
            rgba
        }
        other => {
            return Err(VaMError::UnsupportedEncoding(format!(
                "Unity texture format {other}"
            )));
        }
    };
    Ok(rgba)
}

pub fn load_builtin_texture_rgba(
    reference: &BuiltinTextureRef,
    max_edge: u32,
) -> Result<DecodedBuiltinTexture> {
    if max_edge == 0 {
        return Err(texture_error("texture edge budget must be positive"));
    }
    let metadata = fs::metadata(&reference.bundle_path)
        .map_err(|error| io_error(&reference.bundle_path, error))?;
    let bundle_len = metadata.len();
    let bundle_mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs());
    if let Some(cached) = read_cached_texture(reference, max_edge, bundle_len, bundle_mtime) {
        return Ok(cached);
    }

    let mut reader = BundleReader::open(&reference.bundle_path)?;
    let (node_offset, _node_size) = reader
        .node(&reference.cab_node)
        .ok_or_else(|| texture_error(format!("bundle node {} is absent", reference.cab_node)))?;
    let prefix = parse_serialized_prefix(&mut reader, node_offset)?;
    let object = prefix
        .objects
        .iter()
        .find(|object| object.path_id == reference.path_id)
        .ok_or_else(|| {
            texture_error(format!(
                "texture object {} is absent from {}",
                reference.path_id, reference.cab_node
            ))
        })?;
    if prefix.types[object.type_index].class_id != 28 {
        return Err(texture_error(format!(
            "object {} is not a Texture2D",
            reference.path_id
        )));
    }
    let record = decode_object_fields(
        &mut reader,
        node_offset,
        &prefix,
        object,
        &[
            "m_Name",
            "m_Width",
            "m_Height",
            "m_TextureFormat",
            "m_MipCount",
            "image data",
            "m_StreamData",
        ],
    )?;
    let width = record
        .field("m_Width")
        .and_then(TreeValue::as_signed)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| texture_error("texture width is missing"))?;
    let height = record
        .field("m_Height")
        .and_then(TreeValue::as_signed)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| texture_error("texture height is missing"))?;
    if width == 0 || height == 0 || width > 16_384 || height > 16_384 {
        return Err(texture_error(format!(
            "texture dimensions {width}x{height} are implausible"
        )));
    }
    let format = record
        .field("m_TextureFormat")
        .and_then(TreeValue::as_signed)
        .ok_or_else(|| texture_error("texture format is missing"))?;
    let mip_count = record
        .field("m_MipCount")
        .and_then(TreeValue::as_signed)
        .unwrap_or(1)
        .max(1) as u32;
    let inline = record
        .field("image data")
        .and_then(TreeValue::as_bytes)
        .unwrap_or(&[]);
    let stream = record.field("m_StreamData");
    let stream_offset = stream
        .and_then(|value| value.field("offset"))
        .and_then(TreeValue::as_unsigned)
        .unwrap_or(0);
    let stream_size = stream
        .and_then(|value| value.field("size"))
        .and_then(TreeValue::as_unsigned)
        .unwrap_or(0) as usize;
    let stream_path = stream
        .and_then(|value| value.field("path"))
        .and_then(TreeValue::as_text)
        .unwrap_or("");

    let mut level_width = width;
    let mut level_height = height;
    let mut level_offset = 0_usize;
    let mut level = 0_u32;
    while level + 1 < mip_count && (level_width.max(level_height)) > max_edge {
        level_offset = level_offset
            .checked_add(format_level_bytes(format, level_width, level_height)?)
            .ok_or_else(|| texture_error("texture mip offset overflow"))?;
        level_width = (level_width / 2).max(1);
        level_height = (level_height / 2).max(1);
        level += 1;
    }
    let level_bytes = format_level_bytes(format, level_width, level_height)?;
    if level_bytes == 0 || level_bytes > MAX_TEXTURE_BYTES {
        return Err(texture_error(format!(
            "texture level holds {level_bytes} bytes"
        )));
    }
    let level_data = if stream_size > 0 {
        let node_name = stream_path
            .rsplit('/')
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| texture_error("texture stream path is empty"))?;
        let (stream_node_offset, stream_node_size) = reader
            .node(node_name)
            .ok_or_else(|| texture_error(format!("stream node {node_name} is absent")))?;
        let level_end = level_offset
            .checked_add(level_bytes)
            .ok_or_else(|| texture_error("texture level range overflow"))?;
        if level_end as u64 > stream_size as u64
            || stream_offset + level_end as u64 > stream_node_size
        {
            return Err(texture_error("texture level exceeds its stream"));
        }
        reader.read_range(
            stream_node_offset + stream_offset + level_offset as u64,
            level_bytes,
        )?
    } else {
        inline
            .get(level_offset..level_offset + level_bytes)
            .ok_or_else(|| texture_error("texture level exceeds the inline payload"))?
            .to_vec()
    };
    let mut rgba = decode_level(format, level_width, level_height, &level_data)?;

    if format == FORMAT_DXT5 && (reference.normal_map || pixels::looks_like_dxt5nm(&rgba)) {
        pixels::unswizzle_dxt5nm_in_place(&mut rgba);
    }

    pixels::flip_rows_in_place(&mut rgba, level_width, level_height);
    if level_width > max_edge || level_height > max_edge {
        let scale = f64::from(max_edge) / f64::from(level_width.max(level_height));
        let target_width = ((f64::from(level_width) * scale).round() as u32).max(1);
        let target_height = ((f64::from(level_height) * scale).round() as u32).max(1);
        let view =
            pixels::RgbaView::new(&rgba, level_width, level_height).map_err(texture_error)?;
        rgba = pixels::resize_rgba_box(view, target_width, target_height);
        level_width = target_width;
        level_height = target_height;
    }
    let decoded = DecodedBuiltinTexture {
        width: level_width,
        height: level_height,
        rgba8: rgba,
    };
    let _ = write_cached_texture(reference, max_edge, bundle_len, bundle_mtime, &decoded);
    Ok(decoded)
}

fn cache_texture_path(reference: &BuiltinTextureRef, max_edge: u32) -> Option<PathBuf> {
    let directory = reference.cache_directory.as_ref()?;
    let stem: String = reference
        .texture_name
        .chars()
        .map(|value| {
            if value.is_ascii_alphanumeric() || value == '-' || value == '_' {
                value
            } else {
                '_'
            }
        })
        .collect();
    Some(directory.join(format!(
        "{stem}-{:016x}-{max_edge}.fmskintex",
        reference.path_id as u64
    )))
}

fn read_cached_texture(
    reference: &BuiltinTextureRef,
    max_edge: u32,
    bundle_len: u64,
    bundle_mtime: u64,
) -> Option<DecodedBuiltinTexture> {
    let path = cache_texture_path(reference, max_edge)?;
    let blob = fs::read(path).ok()?;
    if blob.len() < CACHE_MAGIC.len() + 34 + 32 {
        return None;
    }
    let (payload, stored_digest) = blob.split_at(blob.len() - 32);
    let digest: [u8; 32] = Sha256::digest(payload).into();
    if digest != stored_digest {
        return None;
    }
    let mut reader = ByteReader::new(payload, Endian::Little);
    if reader.take(8).ok()? != CACHE_MAGIC {
        return None;
    }
    if reader.u16().ok()? != 1 {
        return None;
    }
    let width = reader.u32().ok()?;
    let height = reader.u32().ok()?;
    if reader.u64().ok()? != bundle_len || reader.u64().ok()? != bundle_mtime {
        return None;
    }
    let compressed_len = reader.u32().ok()? as usize;
    let raw_len = width as usize * height as usize * 4;
    if raw_len == 0 || raw_len > MAX_TEXTURE_BYTES {
        return None;
    }
    let compressed = reader.take(compressed_len).ok()?;
    let rgba8 = lz4_flex::block::decompress(compressed, raw_len).ok()?;
    Some(DecodedBuiltinTexture {
        width,
        height,
        rgba8,
    })
}

fn write_cached_texture(
    reference: &BuiltinTextureRef,
    max_edge: u32,
    bundle_len: u64,
    bundle_mtime: u64,
    decoded: &DecodedBuiltinTexture,
) -> Result<()> {
    let Some(path) = cache_texture_path(reference, max_edge) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    let compressed = lz4_flex::block::compress(&decoded.rgba8);
    let mut payload = Vec::with_capacity(compressed.len() + 64);
    payload.extend_from_slice(&CACHE_MAGIC);
    payload.extend_from_slice(&1_u16.to_le_bytes());
    payload.extend_from_slice(&decoded.width.to_le_bytes());
    payload.extend_from_slice(&decoded.height.to_le_bytes());
    payload.extend_from_slice(&bundle_len.to_le_bytes());
    payload.extend_from_slice(&bundle_mtime.to_le_bytes());
    payload.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    payload.extend_from_slice(&compressed);
    let digest: [u8; 32] = Sha256::digest(&payload).into();
    payload.extend_from_slice(&digest);
    let temporary = path.with_extension("fmskintex.tmp");
    fs::write(&temporary, &payload).map_err(|error| io_error(&temporary, error))?;
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    fs::rename(&temporary, &path).map_err(|error| io_error(&path, error))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuiltinScalpTextureSet {
    pub provider_name: String,
    pub diffuse: Option<BuiltinTextureRef>,
    pub alpha: Option<BuiltinTextureRef>,
}

const SCALP_TEXTURE_BUNDLES: [(&str, &str); 6] = [
    ("UdaneScalp", "h_zzz_mat"),
    ("KrayonScalp", "h_kra_mat"),
    ("SoleilScalp", "h_sol_mat"),
    ("LeytonScalp", "h_ley_mat"),
    ("PantyRegionScalp", "p_gen_mat"),
    ("OmriScalp", "h_omr_mat"),
];

const SCALP_SIM_MATERIAL: &str = "scalp-sim3";

pub fn scan_builtin_scalp_textures(
    root: &VaMRoot,
    cache_dir: Option<&Path>,
) -> Vec<BuiltinScalpTextureSet> {
    let streaming = root.path().join("VaM_Data").join("StreamingAssets");
    let mut sets = Vec::new();
    for (provider, bundle_name) in SCALP_TEXTURE_BUNDLES {
        let bundle_path = streaming.join(bundle_name);
        let Some(set) = scan_scalp_bundle(provider, &bundle_path, cache_dir) else {
            continue;
        };
        sets.push(set);
    }
    sets
}

fn scan_scalp_bundle(
    provider: &str,
    bundle_path: &Path,
    cache_dir: Option<&Path>,
) -> Option<BuiltinScalpTextureSet> {
    let mut reader = BundleReader::open(bundle_path).ok()?;
    let (node_offset, _size, cab_name) = reader.serialized_nodes().first().cloned()?;
    let prefix = parse_serialized_prefix(&mut reader, node_offset).ok()?;

    let mut texture_names: BTreeMap<i64, String> = BTreeMap::new();
    for object in &prefix.objects {
        if prefix.types[object.type_index].class_id != 28 {
            continue;
        }
        if let Ok(record) =
            decode_object_fields(&mut reader, node_offset, &prefix, object, &["m_Name"])
            && let Some(name) = record.field("m_Name").and_then(TreeValue::as_text)
        {
            texture_names.insert(object.path_id, name.to_owned());
        }
    }

    for object in &prefix.objects {
        if prefix.types[object.type_index].class_id != 21 {
            continue;
        }
        let record = decode_object_fields(
            &mut reader,
            node_offset,
            &prefix,
            object,
            &["m_Name", "m_SavedProperties"],
        )
        .ok()?;
        if !record
            .field("m_Name")
            .and_then(TreeValue::as_text)
            .is_some_and(|name| name.eq_ignore_ascii_case(SCALP_SIM_MATERIAL))
        {
            continue;
        }
        let Some(TreeValue::List(tex_envs)) = record
            .field("m_SavedProperties")
            .and_then(|value| value.field("m_TexEnvs"))
        else {
            continue;
        };
        let lookup = |wanted: &str| -> Option<BuiltinTextureRef> {
            for entry in tex_envs {
                let slot = entry.field("first").and_then(|value| {
                    value.as_text().map(str::to_owned).or_else(|| {
                        value
                            .field("name")
                            .and_then(TreeValue::as_text)
                            .map(str::to_owned)
                    })
                })?;
                if slot != wanted {
                    continue;
                }
                let pointer = entry
                    .field("second")
                    .and_then(|value| value.field("m_Texture"))?;
                let file_id = pointer
                    .field("m_FileID")
                    .and_then(TreeValue::as_signed)
                    .unwrap_or(0);
                let path_id = pointer
                    .field("m_PathID")
                    .and_then(TreeValue::as_signed)
                    .unwrap_or(0);
                if file_id != 0 || path_id == 0 {
                    return None;
                }
                return Some(BuiltinTextureRef {
                    bundle_path: bundle_path.to_path_buf(),
                    cab_node: cab_name.clone(),
                    path_id,
                    texture_name: texture_names
                        .get(&path_id)
                        .cloned()
                        .unwrap_or_else(|| format!("texture-{path_id:016x}")),
                    normal_map: false,
                    cache_directory: cache_dir.map(Path::to_path_buf),
                });
            }
            None
        };
        return Some(BuiltinScalpTextureSet {
            provider_name: provider.to_owned(),
            diffuse: lookup("_MainTex"),
            alpha: lookup("_AlphaTex"),
        });
    }
    None
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct BuiltinIndex {
    schema_version: u32,

    cab_index: BTreeMap<String, String>,
    figures: BTreeMap<String, FigureEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FigureEntry {
    display_name: String,
    figure_len: u64,
    figure_mtime: u64,
    material_len: u64,
    material_mtime: u64,

    channels: BTreeMap<String, IndexedTexture>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct IndexedTexture {
    bundle_file: String,
    cab_node: String,
    path_id: i64,
    texture_name: String,
}

fn file_identity(path: &Path) -> Result<(u64, u64)> {
    let metadata = fs::metadata(path).map_err(|error| io_error(path, error))?;
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs());
    Ok((metadata.len(), mtime))
}

const FOREIGN_UV_FIGURES: [(&str, &str); 2] = [
    (
        "f_10",
        "Danika is painted on a UV layout this build does not render with",
    ),
    (
        "f_13",
        "Monique is painted on a UV layout this build does not render with",
    ),
];

pub fn scan_builtin_skins(
    root: &VaMRoot,
    cache_dir: Option<&Path>,
) -> Result<(Vec<SkinPreset>, Vec<String>)> {
    let streaming = root.path().join("VaM_Data").join("StreamingAssets");
    let index_path = cache_dir.map(|directory| directory.join(CACHE_DIRECTORY).join("index.json"));
    let mut index: BuiltinIndex = index_path
        .as_ref()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .filter(|index: &BuiltinIndex| index.schema_version == INDEX_SCHEMA_VERSION)
        .unwrap_or(BuiltinIndex {
            schema_version: INDEX_SCHEMA_VERSION,
            ..BuiltinIndex::default()
        });
    let mut warnings = Vec::new();
    let mut presets = Vec::new();
    let mut index_changed = false;

    let mut figures: Vec<(String, SkinSex)> = Vec::new();
    for number in 1..=17 {
        figures.push((format!("f_{number}"), SkinSex::Female));
    }
    for number in 1..=9 {
        figures.push((format!("m_{number}"), SkinSex::Male));
    }

    for (figure, sex) in figures {
        if let Some(reason) = FOREIGN_UV_FIGURES
            .iter()
            .find(|(id, _)| *id == figure)
            .map(|(_, reason)| *reason)
        {
            warnings.push(format!("built-in skin {figure}: {reason}"));
            continue;
        }
        let figure_path = streaming.join(&figure);
        let material_path = streaming.join(format!("{figure}_mat"));
        if !figure_path.is_file() || !material_path.is_file() {
            continue;
        }
        let (figure_len, figure_mtime) = match file_identity(&figure_path) {
            Ok(identity) => identity,
            Err(error) => {
                warnings.push(format!("built-in skin {figure}: {error}"));
                continue;
            }
        };
        let (material_len, material_mtime) = match file_identity(&material_path) {
            Ok(identity) => identity,
            Err(error) => {
                warnings.push(format!("built-in skin {figure}: {error}"));
                continue;
            }
        };
        let cached = index.figures.get(&figure).filter(|entry| {
            entry.figure_len == figure_len
                && entry.figure_mtime == figure_mtime
                && entry.material_len == material_len
                && entry.material_mtime == material_mtime
        });
        let entry = match cached {
            Some(entry) => entry.clone(),
            None => {
                match scan_figure(
                    &streaming,
                    &figure,
                    &figure_path,
                    &material_path,
                    &mut index.cab_index,
                ) {
                    Ok(mut entry) => {
                        entry.figure_len = figure_len;
                        entry.figure_mtime = figure_mtime;
                        entry.material_len = material_len;
                        entry.material_mtime = material_mtime;
                        index.figures.insert(figure.clone(), entry.clone());
                        index_changed = true;
                        entry
                    }
                    Err(error) => {
                        warnings.push(format!("built-in skin {figure}: {error}"));
                        continue;
                    }
                }
            }
        };
        presets.push(preset_from_entry(
            &streaming, &figure, sex, &entry, cache_dir,
        ));
    }

    if index_changed && let Some(path) = &index_path {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(text) = serde_json::to_string_pretty(&index) {
            let _ = fs::write(path, text);
        }
    }
    Ok((presets, warnings))
}

const CHANNEL_SLOTS: &[(&str, &str, &str)] = &[
    ("face_diffuse", "Face", "_MainTex"),
    ("face_normal", "Face", "_BumpMap"),
    ("face_specular", "Face", "_SpecTex"),
    ("face_gloss", "Face", "_GlossTex"),
    ("torso_diffuse", "Torso", "_MainTex"),
    ("torso_normal", "Torso", "_BumpMap"),
    ("torso_specular", "Torso", "_SpecTex"),
    ("torso_gloss", "Torso", "_GlossTex"),
    ("sclera_diffuse", "Sclera", "_MainTex"),
    ("sclera_normal", "Sclera", "_BumpMap"),
    ("sclera_specular", "Sclera", "_SpecTex"),
    ("sclera_gloss", "Sclera", "_GlossTex"),
    ("iris_diffuse", "Irises", "_MainTex"),
    ("iris_normal", "Irises", "_BumpMap"),
    ("iris_specular", "Irises", "_SpecTex"),
    ("iris_gloss", "Irises", "_GlossTex"),
    ("lacrimal_diffuse", "Lacrimals", "_MainTex"),
    ("lacrimal_specular", "Lacrimals", "_SpecTex"),
    ("inner_mouth_diffuse", "InnerMouth", "_MainTex"),
    ("inner_mouth_specular", "InnerMouth", "_SpecTex"),
    ("teeth_diffuse", "Teeth", "_MainTex"),
    ("teeth_normal", "Teeth", "_BumpMap"),
    ("teeth_specular", "Teeth", "_SpecTex"),
    ("gums_diffuse", "Gums", "_MainTex"),
    ("gums_specular", "Gums", "_SpecTex"),
    ("tongue_diffuse", "Tongue", "_MainTex"),
    ("tongue_normal", "Tongue", "_BumpMap"),
    ("tongue_specular", "Tongue", "_SpecTex"),
    ("eyelash_alpha", "Eyelashes", "_AlphaTex"),
];

fn scan_figure(
    streaming: &Path,
    figure: &str,
    figure_path: &Path,
    material_path: &Path,
    cab_index: &mut BTreeMap<String, String>,
) -> Result<FigureEntry> {
    let mut figure_reader = BundleReader::open(figure_path)?;
    let mut display_name = figure.to_owned();
    'outer: for (offset, _size, _name) in figure_reader.serialized_nodes() {
        let prefix = parse_serialized_prefix(&mut figure_reader, offset)?;
        for object in &prefix.objects {
            if prefix.types[object.type_index].class_id != 1 {
                continue;
            }
            let record =
                decode_object_fields(&mut figure_reader, offset, &prefix, object, &["m_Name"])?;
            if let Some(name) = record.field("m_Name").and_then(TreeValue::as_text) {
                display_name = name
                    .rsplit('-')
                    .next()
                    .filter(|value| !value.is_empty())
                    .unwrap_or(name)
                    .to_owned();
                break 'outer;
            }
        }
    }

    let mut reader = BundleReader::open(material_path)?;
    let nodes = reader.serialized_nodes();
    let (node_offset, _node_size, cab_name) = nodes
        .first()
        .cloned()
        .ok_or_else(|| texture_error("material bundle has no serialized node"))?;
    let prefix = parse_serialized_prefix(&mut reader, node_offset)?;

    let mut texture_names: BTreeMap<i64, String> = BTreeMap::new();
    for object in &prefix.objects {
        if prefix.types[object.type_index].class_id != 28 {
            continue;
        }
        if let Ok(record) =
            decode_object_fields(&mut reader, node_offset, &prefix, object, &["m_Name"])
            && let Some(name) = record.field("m_Name").and_then(TreeValue::as_text)
        {
            texture_names.insert(object.path_id, name.to_owned());
        }
    }

    let mut slots: BTreeMap<(String, String), (i32, i64)> = BTreeMap::new();
    for object in &prefix.objects {
        if prefix.types[object.type_index].class_id != 21 {
            continue;
        }
        let record = decode_object_fields(
            &mut reader,
            node_offset,
            &prefix,
            object,
            &["m_Name", "m_SavedProperties"],
        )?;
        let Some(material_name) = record.field("m_Name").and_then(TreeValue::as_text) else {
            continue;
        };
        let base_material = material_name
            .rsplit_once('-')
            .map_or(material_name, |(base, _)| base)
            .to_owned();
        let Some(TreeValue::List(tex_envs)) = record
            .field("m_SavedProperties")
            .and_then(|value| value.field("m_TexEnvs"))
        else {
            continue;
        };
        for entry in tex_envs {
            let Some(slot) = entry.field("first").and_then(|value| {
                value.as_text().map(str::to_owned).or_else(|| {
                    value
                        .field("name")
                        .and_then(TreeValue::as_text)
                        .map(str::to_owned)
                })
            }) else {
                continue;
            };
            let Some(texture) = entry
                .field("second")
                .and_then(|value| value.field("m_Texture"))
            else {
                continue;
            };
            let file_id = texture
                .field("m_FileID")
                .and_then(TreeValue::as_signed)
                .unwrap_or(0) as i32;
            let path_id = texture
                .field("m_PathID")
                .and_then(TreeValue::as_signed)
                .unwrap_or(0);
            if path_id != 0 {
                slots.insert((base_material.clone(), slot), (file_id, path_id));
            }
        }
    }

    let mut channels = BTreeMap::new();
    for (role, material, slot) in CHANNEL_SLOTS {
        let Some(&(file_id, path_id)) = slots.get(&((*material).to_owned(), (*slot).to_owned()))
        else {
            continue;
        };
        let indexed = if file_id == 0 {
            IndexedTexture {
                bundle_file: material_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                cab_node: cab_name.clone(),
                path_id,
                texture_name: texture_names
                    .get(&path_id)
                    .cloned()
                    .unwrap_or_else(|| format!("texture-{path_id:016x}")),
            }
        } else {
            let external = prefix
                .externals
                .get((file_id - 1) as usize)
                .ok_or_else(|| texture_error("material references a missing external"))?;
            let cab = external
                .rsplit('/')
                .next()
                .unwrap_or(external)
                .to_ascii_lowercase();
            let Some(bundle_file) = resolve_cab_bundle(streaming, &cab, cab_index)? else {
                continue;
            };
            let mut external_reader = BundleReader::open(&streaming.join(&bundle_file))?;
            let Some((external_offset, _)) = external_reader
                .nodes
                .iter()
                .find(|node| node.path.eq_ignore_ascii_case(&cab))
                .map(|node| (node.offset, node.size))
            else {
                continue;
            };
            let external_prefix = parse_serialized_prefix(&mut external_reader, external_offset)?;
            let Some(external_object) = external_prefix
                .objects
                .iter()
                .find(|object| object.path_id == path_id)
            else {
                continue;
            };
            let texture_name = decode_object_fields(
                &mut external_reader,
                external_offset,
                &external_prefix,
                external_object,
                &["m_Name"],
            )
            .ok()
            .and_then(|record| {
                record
                    .field("m_Name")
                    .and_then(TreeValue::as_text)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| format!("texture-{path_id:016x}"));
            IndexedTexture {
                bundle_file,
                cab_node: external_prefix_node_name(&external_reader, &cab),
                path_id,
                texture_name,
            }
        };
        channels.insert((*role).to_owned(), indexed);
    }

    Ok(FigureEntry {
        display_name,
        figure_len: 0,
        figure_mtime: 0,
        material_len: 0,
        material_mtime: 0,
        channels,
    })
}

fn external_prefix_node_name(reader: &BundleReader, cab: &str) -> String {
    reader
        .nodes
        .iter()
        .find(|node| node.path.eq_ignore_ascii_case(cab))
        .map(|node| node.path.clone())
        .unwrap_or_else(|| cab.to_owned())
}

fn resolve_cab_bundle(
    streaming: &Path,
    cab: &str,
    cab_index: &mut BTreeMap<String, String>,
) -> Result<Option<String>> {
    if let Some(bundle) = cab_index.get(cab) {
        return Ok(Some(bundle.clone()));
    }

    let mut candidates: Vec<String> = ["p_eye_mat", "p_sha_mat", "z_sha", "f_1_mat", "m_1_mat"]
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    if let Ok(entries) = fs::read_dir(streaming) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".manifest") && !candidates.contains(&name) {
                candidates.push(name);
            }
        }
    }
    for name in candidates {
        let path = streaming.join(&name);
        if !path.is_file() {
            continue;
        }
        let Ok(reader) = BundleReader::open(&path) else {
            continue;
        };
        for node in &reader.nodes {
            let lowered = node.path.to_ascii_lowercase();
            if lowered.starts_with("cab-") && !lowered.ends_with(".ress") {
                cab_index.entry(lowered).or_insert_with(|| name.clone());
            }
        }
        if cab_index.contains_key(cab) {
            return Ok(Some(name));
        }
    }
    Ok(cab_index.get(cab).cloned())
}

pub const fn base_figure(sex: SkinSex) -> &'static str {
    match sex {
        SkinSex::Male => "m_1",
        SkinSex::Female | SkinSex::Unknown => "f_1",
    }
}

#[must_use]
pub fn builtin_stable_id(figure: &str) -> String {
    format!("vam:skin:builtin:{}", figure.replace('_', "-"))
}

fn preset_from_entry(
    streaming: &Path,
    figure: &str,
    sex: SkinSex,
    entry: &FigureEntry,
    cache_dir: Option<&Path>,
) -> SkinPreset {
    let cache_directory = cache_dir.map(|directory| directory.join(CACHE_DIRECTORY).join(figure));
    let locator = |role: &str| -> Option<AssetLocator> {
        let indexed = entry.channels.get(role)?;
        Some(AssetLocator::BuiltinTexture(Arc::new(BuiltinTextureRef {
            bundle_path: streaming.join(&indexed.bundle_file),
            cab_node: indexed.cab_node.clone(),
            path_id: indexed.path_id,
            texture_name: indexed.texture_name.clone(),
            normal_map: role.ends_with("_normal"),
            cache_directory: cache_directory.clone(),
        })))
    };
    let mut preset = SkinPreset {
        stable_id: builtin_stable_id(figure),
        label: format!("{} (VaM)", entry.display_name),
        source: AssetLocator::File(streaming.join(format!("{figure}_mat"))),
        sex,
        textures: {
            let mut set = super::skin::SkinTextureSet::default();
            for region in crate::vam::SkinRegion::ALL {
                for channel in super::skin::SkinTextureChannel::ALL {
                    let role = format!(
                        "{}_{}",
                        region.vam_prefix(),
                        channel.vam_suffix().to_ascii_lowercase()
                    );
                    if let Some(found) = locator(&role) {
                        set.insert(region, channel, found);
                    }
                }
            }
            set
        },
        auxiliary: super::skin::SkinAuxiliary::default(),
    };
    let auxiliary = |region: &str, material: &mut super::skin::SkinAuxMaterial| {
        if let Some(diffuse) = locator(&format!("{region}_diffuse")) {
            material.diffuse = Some(diffuse);
            material.diffuse_source = SkinDiffuseSource::CustomTexture;
        }
        material.surface.normal = locator(&format!("{region}_normal"));
        material.surface.specular = locator(&format!("{region}_specular"));
        material.surface.gloss = locator(&format!("{region}_gloss"));
    };
    auxiliary("sclera", &mut preset.auxiliary.sclera);
    auxiliary("iris", &mut preset.auxiliary.iris);
    auxiliary("lacrimal", &mut preset.auxiliary.lacrimal);
    auxiliary("inner_mouth", &mut preset.auxiliary.inner_mouth);
    auxiliary("teeth", &mut preset.auxiliary.teeth);
    auxiliary("gums", &mut preset.auxiliary.gums);
    auxiliary("tongue", &mut preset.auxiliary.tongue);
    if let Some(alpha) = locator("eyelash_alpha") {
        preset.auxiliary.eyelashes.diffuse = Some(alpha);
        preset.auxiliary.eyelashes.diffuse_source = SkinDiffuseSource::CustomTexture;
    }
    preset
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a real VaM install; see VKIT_VAM_ROOT"]
    fn builtin_scalp_textures_scan_and_decode_from_a_real_install() {
        let root =
            VaMRoot::open(std::env::var_os("VKIT_VAM_ROOT").expect("set VKIT_VAM_ROOT")).unwrap();
        let sets = scan_builtin_scalp_textures(&root, None);
        let found: Vec<&str> = sets.iter().map(|set| set.provider_name.as_str()).collect();
        for wanted in [
            "UdaneScalp",
            "KrayonScalp",
            "SoleilScalp",
            "LeytonScalp",
            "OmriScalp",
        ] {
            assert!(
                found.contains(&wanted),
                "{wanted} ships a scalp-sim3 material and both its sheets, so a blank \
                 cap means the bundle table or the name match is wrong; found {found:?}",
            );
        }
        for set in &sets {
            let diffuse = set.diffuse.as_ref().expect("diffuse sheet");
            let alpha = set.alpha.as_ref().expect("alpha mask");
            for reference in [diffuse, alpha] {
                let decoded = load_builtin_texture_rgba(reference, 2048).expect("decode");
                assert!(decoded.width >= 64 && decoded.height >= 64);
                assert_eq!(
                    decoded.rgba8.len(),
                    decoded.width as usize * decoded.height as usize * 4
                );
            }
        }
    }

    fn push_aligned_string(bytes: &mut Vec<u8>, text: &str) {
        bytes.extend_from_slice(&(text.len() as u32).to_le_bytes());
        bytes.extend_from_slice(text.as_bytes());
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
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
        let mut table: Vec<u8> = Vec::new();
        let offset_of = |text: &str, table: &mut Vec<u8>| -> u32 {
            for offset in 0..1200 {
                if super::super::unity_base::common_type_string(offset) == Some(text) {
                    return offset | 0x8000_0000;
                }
            }
            let offset = table.len() as u32;
            table.extend_from_slice(text.as_bytes());
            table.push(0);
            offset
        };
        let mut records = Vec::new();
        for spec in specs {
            let type_offset = offset_of(spec.type_name, &mut table);
            let name_offset = offset_of(spec.name, &mut table);
            records.extend_from_slice(&1_u16.to_le_bytes());
            records.push(spec.level);
            records.push(0);
            records.extend_from_slice(&type_offset.to_le_bytes());
            records.extend_from_slice(&name_offset.to_le_bytes());
            records.extend_from_slice(&(-1_i32).to_le_bytes());
            records.extend_from_slice(&0_i32.to_le_bytes());
            records.extend_from_slice(&spec.meta.to_le_bytes());
        }
        (records, table)
    }

    fn texture2d_tree() -> Vec<TreeSpec> {
        let mut specs = vec![node("Texture2D", "Base", 0, 0x8000)];
        specs.push(node("string", "m_Name", 1, 0x8000));
        specs.push(node("Array", "Array", 2, 0x4001));
        specs.push(node("int", "size", 3, 0x0001));
        specs.push(node("char", "data", 3, 0x0001));
        specs.push(node("int", "m_Width", 1, 0));
        specs.push(node("int", "m_Height", 1, 0));
        specs.push(node("int", "m_TextureFormat", 1, 0));
        specs.push(node("int", "m_MipCount", 1, 0));
        specs.push(node("TypelessData", "image data", 1, 0));
        specs.push(node("int", "size", 2, 0));
        specs.push(node("UInt8", "data", 2, 0));
        specs.push(node("StreamingInfo", "m_StreamData", 1, 0));
        specs.push(node("unsigned int", "offset", 2, 0));
        specs.push(node("unsigned int", "size", 2, 0));
        specs.push(node("string", "path", 2, 0x8000));
        specs.push(node("Array", "Array", 3, 0x4001));
        specs.push(node("int", "size", 4, 0x0001));
        specs.push(node("char", "data", 4, 0x0001));
        specs
    }

    fn texture2d_payload(
        name: &str,
        width: u32,
        height: u32,
        format: i64,
        inline: &[u8],
        stream: Option<(u64, u64, &str)>,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_aligned_string(&mut bytes, name);
        bytes.extend_from_slice(&(width as i32).to_le_bytes());
        bytes.extend_from_slice(&(height as i32).to_le_bytes());
        bytes.extend_from_slice(&(format as i32).to_le_bytes());
        bytes.extend_from_slice(&1_i32.to_le_bytes());
        bytes.extend_from_slice(&(inline.len() as i32).to_le_bytes());
        bytes.extend_from_slice(inline);
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
        let (offset, size, path) = stream.unwrap_or((0, 0, ""));
        bytes.extend_from_slice(&(offset as u32).to_le_bytes());
        bytes.extend_from_slice(&(size as u32).to_le_bytes());
        push_aligned_string(&mut bytes, path);
        bytes
    }

    fn synthetic_serialized(objects: &[(i64, Vec<u8>)]) -> Vec<u8> {
        let specs = texture2d_tree();
        let (records, strings) = encode_tree(&specs);
        let mut metadata = Vec::new();
        metadata.extend_from_slice(b"2018.1.9f1\0");
        metadata.extend_from_slice(&19_i32.to_le_bytes());
        metadata.push(1);
        metadata.extend_from_slice(&1_u32.to_le_bytes());
        metadata.extend_from_slice(&28_i32.to_le_bytes());
        metadata.push(0);
        metadata.extend_from_slice(&(-1_i16).to_le_bytes());
        metadata.extend_from_slice(&[0_u8; 16]);
        metadata.extend_from_slice(&(specs.len() as u32).to_le_bytes());
        metadata.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        metadata.extend_from_slice(&records);
        metadata.extend_from_slice(&strings);
        metadata.extend_from_slice(&(objects.len() as u32).to_le_bytes());
        let mut data = Vec::new();
        for (path_id, payload) in objects {
            while !metadata.len().is_multiple_of(4) {
                metadata.push(0);
            }
            metadata.extend_from_slice(&path_id.to_le_bytes());
            metadata.extend_from_slice(&(data.len() as u32).to_le_bytes());
            metadata.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            metadata.extend_from_slice(&0_i32.to_le_bytes());
            data.extend_from_slice(payload);
            while !data.len().is_multiple_of(8) {
                data.push(0);
            }
        }
        metadata.extend_from_slice(&0_u32.to_le_bytes());
        metadata.extend_from_slice(&0_u32.to_le_bytes());
        let data_offset = (20 + metadata.len()).div_ceil(16) * 16;
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

    fn synthetic_bundle(nodes: &[(&str, &[u8])]) -> Vec<u8> {
        let mut data = Vec::new();
        let mut directory = Vec::new();
        directory.extend_from_slice(&[0_u8; 16]);
        let mut node_records = Vec::new();
        let mut offset = 0_u64;
        for (name, payload) in nodes {
            node_records.extend_from_slice(&offset.to_be_bytes());
            node_records.extend_from_slice(&(payload.len() as u64).to_be_bytes());
            node_records.extend_from_slice(&0_u32.to_be_bytes());
            node_records.extend_from_slice(name.as_bytes());
            node_records.push(0);
            data.extend_from_slice(payload);
            offset += payload.len() as u64;
        }
        directory.extend_from_slice(&1_u32.to_be_bytes());
        directory.extend_from_slice(&(data.len() as u32).to_be_bytes());
        directory.extend_from_slice(&(data.len() as u32).to_be_bytes());
        directory.extend_from_slice(&0_u16.to_be_bytes());
        directory.extend_from_slice(&(nodes.len() as u32).to_be_bytes());
        directory.extend_from_slice(&node_records);
        let packed = lz4_flex::block::compress(&directory);

        let mut bundle = Vec::new();
        bundle.extend_from_slice(b"UnityFS\0");
        bundle.extend_from_slice(&6_u32.to_be_bytes());
        bundle.extend_from_slice(b"5.x.x\0");
        bundle.extend_from_slice(b"2018.1.9f1\0");
        let total = bundle.len() + 20 + packed.len() + data.len();
        bundle.extend_from_slice(&(total as u64).to_be_bytes());
        bundle.extend_from_slice(&(packed.len() as u32).to_be_bytes());
        bundle.extend_from_slice(&(directory.len() as u32).to_be_bytes());
        bundle.extend_from_slice(&0x43_u32.to_be_bytes());
        bundle.extend_from_slice(&packed);
        bundle.extend_from_slice(&data);
        bundle
    }

    fn scratch(label: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "vkit-unity-textures-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn windowed_reader_extracts_an_inline_rgba_texture_with_row_flip() {
        let directory = scratch("inline");

        let inline = [
            255, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255,
        ];
        let cab = synthetic_serialized(&[(
            42,
            texture2d_payload("SynthTex", 2, 2, FORMAT_RGBA32, &inline, None),
        )]);
        let bundle_path = directory.join("synth_bundle");
        fs::write(&bundle_path, synthetic_bundle(&[("CAB-synth", &cab)])).unwrap();

        let reference = BuiltinTextureRef {
            bundle_path: bundle_path.clone(),
            cab_node: "CAB-synth".to_owned(),
            path_id: 42,
            texture_name: "SynthTex".to_owned(),
            normal_map: false,
            cache_directory: Some(directory.join("cache")),
        };
        let decoded = load_builtin_texture_rgba(&reference, 2048).unwrap();
        assert_eq!((decoded.width, decoded.height), (2, 2));
        assert_eq!(&decoded.rgba8[..4], &[0, 255, 0, 255]);
        assert_eq!(&decoded.rgba8[8..12], &[255, 0, 0, 255]);

        let cached = load_builtin_texture_rgba(&reference, 2048).unwrap();
        assert_eq!(cached.rgba8, decoded.rgba8);
        assert!(
            fs::read_dir(directory.join("cache"))
                .unwrap()
                .flatten()
                .any(|entry| {
                    entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "fmskintex")
                })
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn streamed_dxt1_texture_reads_only_its_ress_range() {
        let directory = scratch("stream");

        let mut block = Vec::new();
        block.extend_from_slice(&0xf800_u16.to_le_bytes());
        block.extend_from_slice(&0x07e0_u16.to_le_bytes());
        block.extend_from_slice(&0_u32.to_le_bytes());
        let mut ress = vec![0xaa_u8; 8];
        ress.extend_from_slice(&block);
        let cab = synthetic_serialized(&[(
            7,
            texture2d_payload(
                "StreamTex",
                4,
                4,
                FORMAT_DXT1,
                &[],
                Some((8, 8, "archive:/CAB-synth/CAB-synth.resS")),
            ),
        )]);
        let bundle_path = directory.join("synth_stream_bundle");
        fs::write(
            &bundle_path,
            synthetic_bundle(&[("CAB-synth", &cab), ("CAB-synth.resS", &ress)]),
        )
        .unwrap();

        let reference = BuiltinTextureRef {
            bundle_path,
            cab_node: "CAB-synth".to_owned(),
            path_id: 7,
            texture_name: "StreamTex".to_owned(),
            normal_map: false,
            cache_directory: None,
        };
        let decoded = load_builtin_texture_rgba(&reference, 2048).unwrap();
        assert_eq!((decoded.width, decoded.height), (4, 4));
        assert!(
            decoded
                .rgba8
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
        fs::remove_dir_all(
            std::env::temp_dir().join(format!("vkit-unity-textures-stream-{}", std::process::id())),
        )
        .unwrap();
    }

    #[test]
    fn stale_cache_is_refreshed_when_the_bundle_identity_changes() {
        let directory = scratch("stale");
        let inline = [
            10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255,
        ];
        let cab = synthetic_serialized(&[(
            9,
            texture2d_payload("StaleTex", 2, 2, FORMAT_RGBA32, &inline, None),
        )]);
        let bundle_path = directory.join("synth_bundle");
        fs::write(&bundle_path, synthetic_bundle(&[("CAB-synth", &cab)])).unwrap();
        let reference = BuiltinTextureRef {
            bundle_path: bundle_path.clone(),
            cab_node: "CAB-synth".to_owned(),
            path_id: 9,
            texture_name: "StaleTex".to_owned(),
            normal_map: false,
            cache_directory: Some(directory.join("cache")),
        };
        load_builtin_texture_rgba(&reference, 2048).unwrap();

        let inline2 = [77_u8, 88, 99, 255].repeat(4);
        let cab2 = synthetic_serialized(&[
            (
                9,
                texture2d_payload("StaleTex", 2, 2, FORMAT_RGBA32, &inline2, None),
            ),
            (
                10,
                texture2d_payload("Padding", 1, 1, FORMAT_RGBA32, &[1, 2, 3, 255], None),
            ),
        ]);
        fs::write(&bundle_path, synthetic_bundle(&[("CAB-synth", &cab2)])).unwrap();
        let refreshed = load_builtin_texture_rgba(&reference, 2048).unwrap();
        assert_eq!(&refreshed.rgba8[..4], &[77, 88, 99, 255]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn the_foreign_uv_figures_are_female_figures_that_exist() {
        for (figure, reason) in FOREIGN_UV_FIGURES {
            let number: usize = figure
                .strip_prefix("f_")
                .expect("only female figures are excluded")
                .parse()
                .expect("a figure id is f_<number>");
            assert!(
                (1..=17).contains(&number),
                "{figure} is not a scanned figure"
            );
            assert!(!reason.is_empty(), "{figure} is excluded with no reason");
        }
    }

    #[test]
    #[ignore = "requires the user's local VaM installation"]
    fn real_install_lists_builtin_skins_with_names_and_decodes_victoria_face() {
        let root_path = std::env::var_os("VKIT_VAM_ROOT")
            .map(std::path::PathBuf::from)
            .expect("set VKIT_VAM_ROOT to a VaM install");
        let root = VaMRoot::open(root_path).unwrap();
        let workspace = scratch("real");
        let (presets, warnings) = scan_builtin_skins(&root, Some(&workspace)).unwrap();
        eprintln!("built-in skins: {}; warnings: {warnings:?}", presets.len());
        for preset in &presets {
            eprintln!("  {} [{:?}] {}", preset.stable_id, preset.sex, preset.label);
            for locator in [
                preset.surface(crate::vam::SkinRegion::Face).normal.as_ref(),
                preset
                    .surface(crate::vam::SkinRegion::Torso)
                    .normal
                    .as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                let AssetLocator::BuiltinTexture(reference) = locator else {
                    continue;
                };
                assert!(
                    reference.normal_map,
                    "{} normal slot lost its semantic marker",
                    preset.label
                );
            }
        }
        assert!(presets.len() >= 12, "expected a dozen-plus built-in skins");
        let female = presets
            .iter()
            .filter(|preset| preset.sex == SkinSex::Female)
            .count();
        let male = presets
            .iter()
            .filter(|preset| preset.sex == SkinSex::Male)
            .count();
        assert!(female >= 9 && male >= 5, "female={female} male={male}");
        let victoria = presets
            .iter()
            .find(|preset| preset.label.contains("Victoria"))
            .expect("Victoria ships with VaM");
        let AssetLocator::BuiltinTexture(reference) = victoria
            .diffuse(crate::vam::SkinRegion::Face)
            .as_ref()
            .expect("face diffuse")
        else {
            panic!("built-in face diffuse must be a bundle texture");
        };
        let started = std::time::Instant::now();
        let decoded = load_builtin_texture_rgba(reference, 2048).unwrap();
        eprintln!(
            "victoria face: {}x{} in {:.0}ms (cold)",
            decoded.width,
            decoded.height,
            started.elapsed().as_secs_f64() * 1000.0
        );
        assert!(decoded.width >= 1024 && decoded.width <= 2048);
        let started = std::time::Instant::now();
        let cached = load_builtin_texture_rgba(reference, 2048).unwrap();
        eprintln!(
            "victoria face: cached reload {:.0}ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
        assert_eq!(cached.rgba8.len(), decoded.rgba8.len());

        let face_surface = victoria.surface(crate::vam::SkinRegion::Face);
        let AssetLocator::BuiltinTexture(normal_reference) =
            face_surface.normal.as_ref().expect("face normal")
        else {
            panic!("built-in face normal must be a bundle texture");
        };
        let normal = load_builtin_texture_rgba(normal_reference, 1024).unwrap();
        assert!(
            !pixels::looks_like_dxt5nm(&normal.rgba8),
            "the decoded normal map must leave Unity's DXT5nm packing behind"
        );
        let pixels_count = (normal.rgba8.len() / 4) as u64;
        let mut sums = [0_u64; 4];
        let mut pinned_red = 0_u64;
        for pixel in normal.rgba8.chunks_exact(4) {
            for (channel, sum) in sums.iter_mut().enumerate() {
                *sum += u64::from(pixel[channel]);
            }
            if pixel[0] >= 250 {
                pinned_red += 1;
            }
        }
        let means = sums.map(|sum| sum / pixels_count);
        eprintln!(
            "victoria face normal {}x{} channel means r={} g={} b={} a={}",
            normal.width, normal.height, means[0], means[1], means[2], means[3]
        );
        assert!(
            pinned_red * 4 < pixels_count,
            "tangent X must vary across the restored normal map"
        );

        assert!((80..=176).contains(&means[0]) && (80..=176).contains(&means[1]));
        assert!(means[2] > 200, "reconstructed Z must point outward");
        assert_eq!(means[3], 255, "restored normals are opaque");

        let started = std::time::Instant::now();
        let (again, _) = scan_builtin_skins(&root, Some(&workspace)).unwrap();
        eprintln!(
            "indexed rescan: {:.0}ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
        assert_eq!(again.len(), presets.len());
        fs::remove_dir_all(workspace).unwrap();
    }
}
