use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::G2F_VERTEX_COUNT;
use crate::formats::{MorphAuthoring, MorphTarget};

use super::catalog::{BuiltinMorphSource, VaMRoot};
use super::skin::SkinSex;
use super::vmb::SparseDelta;
use super::{Result, VaMError, io_error};

const MAX_BLOCKS: usize = 100_000;
const MAX_NODES: usize = 100_000;
const MAX_OBJECTS: usize = 1_000_000;
const MAX_TYPES: usize = 100_000;
const MAX_MORPHS_PER_OBJECT: usize = 20_000;
const MAX_DELTAS_PER_MORPH: usize = 1_000_000;
const MAX_FORMULAS_PER_MORPH: usize = 100_000;
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_RESOLUTION_DEPTH: usize = 32;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormulaTarget {
    MorphValue,
    BoneCenterX,
    BoneCenterY,
    BoneCenterZ,
    OrientationX,
    OrientationY,
    OrientationZ,
    GeneralScale,
    ScaleX,
    ScaleY,
    ScaleZ,
    Mcm,
    McmMultiplier,
    RotationX,
    RotationY,
    RotationZ,
    Unknown(i32),
}

impl From<i32> for FormulaTarget {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::MorphValue,
            1 => Self::BoneCenterX,
            2 => Self::BoneCenterY,
            3 => Self::BoneCenterZ,
            4 => Self::OrientationX,
            5 => Self::OrientationY,
            6 => Self::OrientationZ,
            7 => Self::GeneralScale,
            8 => Self::ScaleX,
            9 => Self::ScaleY,
            10 => Self::ScaleZ,
            11 => Self::Mcm,
            12 => Self::McmMultiplier,
            13 => Self::RotationX,
            14 => Self::RotationY,
            15 => Self::RotationZ,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Formula {
    pub target_type: FormulaTarget,
    pub target: String,
    pub multiplier: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MorphResolutionReceipt {
    pub source_delta_count: usize,
    pub resolved_vertex_count: usize,
    pub filtered_outside_head: usize,
    pub filtered_out_of_range: usize,
    pub unsupported_formulas: Vec<Formula>,
    pub missing_morph_targets: Vec<String>,
    pub cyclic_dependencies: Vec<String>,
    pub depth_limited_targets: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedMorph {
    pub stable_id: String,
    pub internal_name: String,
    pub label: String,
    pub sparse_deltas: Vec<SparseDelta>,
    pub vertex_count: usize,
    pub receipt: MorphResolutionReceipt,
}

impl ResolvedMorph {
    pub fn dense_deltas(&self) -> Vec<[f64; 3]> {
        let mut dense = vec![[0.0; 3]; self.vertex_count];
        for delta in &self.sparse_deltas {
            if let Some(slot) = dense.get_mut(delta.vertex_index as usize) {
                *slot = delta.delta_cm;
            }
        }
        dense
    }

    pub fn to_morph_target(
        &self,
        canonical_faces: &[Vec<u32>],
        zero_tolerance: f64,
        minimum: f64,
        maximum: f64,
        default: f64,
    ) -> crate::formats::Result<MorphTarget> {
        MorphTarget::from_dense_deltas(
            self.stable_id.clone(),
            self.dense_deltas(),
            canonical_faces.to_vec(),
            MorphAuthoring {
                source_unit_scale: 100.0,
                zero_tolerance,
                minimum,
                maximum,
                default,
            },
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RawMorphMetadata {
    pub object_path_id: i64,
    pub morph_index: u32,
    pub internal_name: String,
    pub label: String,
    pub region: String,
    pub group: String,
    pub visible: bool,
    pub disabled: bool,
    pub is_pose_control: bool,
    pub minimum: f64,
    pub maximum: f64,
    pub default: f64,
    pub delta_count: usize,
    pub formulas: Vec<Formula>,
}

#[derive(Clone, Debug)]
struct BankMorph {
    metadata: RawMorphMetadata,
    direct_deltas: Vec<SparseDelta>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Endian {
    Little,
    Big,
}

#[derive(Clone, Debug)]
pub(crate) struct BundleNode {
    pub(crate) offset: usize,
    pub(crate) size: usize,
    pub(crate) path: String,
}

#[derive(Clone, Debug)]
struct SerializedPayload {
    path_id: i64,
    endian: Endian,
    bytes: Vec<u8>,
}

#[derive(Clone)]
pub struct BuiltinMorphSession {
    inner: Arc<BuiltinMorphSessionInner>,
}

struct BuiltinMorphSessionInner {
    bundle_path: PathBuf,
    objects: BTreeMap<i64, SerializedPayload>,
    metadata: Vec<RawMorphMetadata>,
    recent_object: Mutex<Option<(i64, Arc<Vec<BankMorph>>)>>,
}

impl std::fmt::Debug for BuiltinMorphSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BuiltinMorphSession")
            .field("bundle_path", &self.inner.bundle_path)
            .field("object_count", &self.inner.objects.len())
            .field("morph_count", &self.inner.metadata.len())
            .finish_non_exhaustive()
    }
}

impl BuiltinMorphSession {
    pub fn open(root: &VaMRoot) -> Result<Self> {
        Self::open_for_sex(root, SkinSex::Female)
    }

    pub fn open_for_sex(root: &VaMRoot, sex: SkinSex) -> Result<Self> {
        Self::open_bank(root.morph_bank_path(sex)?)
    }

    fn open_bank(path: &Path) -> Result<Self> {
        let bundle_path = fs::canonicalize(path).map_err(|error| io_error(path, error))?;
        let payload = fs::read(&bundle_path).map_err(|error| io_error(&bundle_path, error))?;
        let (data_area, nodes) = decode_unity_bundle(&payload)?;
        let mut objects = BTreeMap::new();
        let mut metadata = Vec::new();
        for node in nodes {
            let end = node.offset.checked_add(node.size).ok_or_else(|| {
                VaMError::InvalidMorphBank("UnityFS node byte range overflow".to_owned())
            })?;
            let cab = data_area.get(node.offset..end).ok_or_else(|| {
                VaMError::InvalidMorphBank("UnityFS node exceeds decompressed data".to_owned())
            })?;
            for object in serialized_behaviours(cab)? {
                let Ok(records) =
                    parse_bank_object(&object.bytes, object.endian, object.path_id, false)
                else {
                    continue;
                };
                if records.is_empty() {
                    continue;
                }
                metadata.extend(records.into_iter().map(|record| record.metadata));
                objects.entry(object.path_id).or_insert(object);
            }
        }
        if metadata.is_empty() {
            return Err(VaMError::InvalidMorphBank(format!(
                "no morph records were found in {}",
                bundle_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("selected VaM morph bank")
            )));
        }
        Ok(Self {
            inner: Arc::new(BuiltinMorphSessionInner {
                bundle_path,
                objects,
                metadata,
                recent_object: Mutex::new(None),
            }),
        })
    }

    pub fn bundle_path(&self) -> &Path {
        &self.inner.bundle_path
    }

    pub fn morph_count(&self) -> usize {
        self.inner.metadata.len()
    }

    pub fn load_resolved(
        &self,
        source: &BuiltinMorphSource,
        head_vertex_mask: &[bool],
    ) -> Result<ResolvedMorph> {
        if head_vertex_mask.len() != G2F_VERTEX_COUNT {
            return Err(VaMError::InvalidMorphBank(format!(
                "canonical head mask has {} entries; expected {G2F_VERTEX_COUNT}",
                head_vertex_mask.len()
            )));
        }
        let records = self.records_for_source(source)?;
        let target = records
            .iter()
            .position(|record| record.metadata.morph_index == source.morph_index)
            .ok_or_else(|| {
                VaMError::InvalidMorphBank(format!(
                    "morph index {} is absent from object {}",
                    source.morph_index, source.object_path_id
                ))
            })?;
        resolve_records(&records, target, head_vertex_mask, source)
    }

    pub fn load_direct_deltas(&self, source: &BuiltinMorphSource) -> Result<Vec<SparseDelta>> {
        let records = self.records_for_source(source)?;
        records
            .iter()
            .find(|record| record.metadata.morph_index == source.morph_index)
            .map(|record| record.direct_deltas.clone())
            .ok_or_else(|| {
                VaMError::InvalidMorphBank(format!(
                    "morph index {} is absent from object {}",
                    source.morph_index, source.object_path_id
                ))
            })
    }

    fn records_for_source(&self, source: &BuiltinMorphSource) -> Result<Arc<Vec<BankMorph>>> {
        let requested_path = fs::canonicalize(&source.bundle_path)
            .map_err(|error| io_error(&source.bundle_path, error))?;
        if requested_path != self.inner.bundle_path {
            return Err(VaMError::InvalidMorphBank(format!(
                "morph source {} does not belong to session {}",
                source.bundle_path.display(),
                self.inner.bundle_path.display()
            )));
        }
        let records = {
            let mut cache = self.inner.recent_object.lock().map_err(|_| {
                VaMError::InvalidMorphBank("built-in morph session cache was poisoned".to_owned())
            })?;
            if let Some((path_id, records)) = cache.as_ref() {
                if *path_id == source.object_path_id {
                    records.clone()
                } else {
                    let parsed = self.parse_object(source.object_path_id)?;
                    *cache = Some((source.object_path_id, parsed.clone()));
                    parsed
                }
            } else {
                let parsed = self.parse_object(source.object_path_id)?;
                *cache = Some((source.object_path_id, parsed.clone()));
                parsed
            }
        };
        Ok(records)
    }

    fn parse_object(&self, object_path_id: i64) -> Result<Arc<Vec<BankMorph>>> {
        let object = self.inner.objects.get(&object_path_id).ok_or_else(|| {
            VaMError::InvalidMorphBank(format!(
                "MonoBehaviour object {object_path_id} is absent from {}",
                self.inner.bundle_path.display()
            ))
        })?;
        Ok(Arc::new(parse_bank_object(
            &object.bytes,
            object.endian,
            object.path_id,
            true,
        )?))
    }

    pub(crate) fn metadata(&self) -> &[RawMorphMetadata] {
        &self.inner.metadata
    }
}

pub fn load_resolved_builtin(
    source: &BuiltinMorphSource,
    head_vertex_mask: &[bool],
) -> Result<ResolvedMorph> {
    BuiltinMorphSession::open_bank(&source.bundle_path)?.load_resolved(source, head_vertex_mask)
}

fn resolve_records(
    records: &[BankMorph],
    target_index: usize,
    head_vertex_mask: &[bool],
    source: &BuiltinMorphSource,
) -> Result<ResolvedMorph> {
    let target = records.get(target_index).ok_or_else(|| {
        VaMError::InvalidMorphBank("resolved morph target index is out of range".to_owned())
    })?;
    let mut names = HashMap::new();
    for (index, record) in records.iter().enumerate() {
        names
            .entry(record.metadata.internal_name.as_str())
            .or_insert(index);
    }

    let mut accumulator: BTreeMap<u32, [f64; 3]> = BTreeMap::new();
    let mut receipt = MorphResolutionReceipt {
        source_delta_count: target.direct_deltas.len(),
        ..MorphResolutionReceipt::default()
    };
    let mut stack = Vec::new();
    resolve_one(
        records,
        &names,
        target_index,
        1.0,
        0,
        head_vertex_mask,
        &mut Resolution {
            stack: &mut stack,
            accumulator: &mut accumulator,
            receipt: &mut receipt,
        },
    )?;

    receipt.missing_morph_targets.sort();
    receipt.missing_morph_targets.dedup();
    receipt.cyclic_dependencies.sort();
    receipt.cyclic_dependencies.dedup();
    receipt.depth_limited_targets.sort();
    receipt.depth_limited_targets.dedup();
    receipt.resolved_vertex_count = accumulator.len();

    let sparse_deltas = accumulator
        .into_iter()
        .filter(|(_, delta)| delta.iter().any(|value| *value != 0.0))
        .map(|(vertex_index, delta_cm)| SparseDelta {
            vertex_index,
            delta_cm,
        })
        .collect();
    Ok(ResolvedMorph {
        stable_id: format!(
            "vam:builtin:f:{:016x}:{}",
            source.object_path_id as u64, source.morph_index
        ),
        internal_name: target.metadata.internal_name.clone(),
        label: target.metadata.label.clone(),
        sparse_deltas,
        vertex_count: head_vertex_mask.len(),
        receipt,
    })
}

struct Resolution<'a> {
    stack: &'a mut Vec<usize>,
    accumulator: &'a mut BTreeMap<u32, [f64; 3]>,
    receipt: &'a mut MorphResolutionReceipt,
}

fn resolve_one<'a>(
    records: &'a [BankMorph],
    names: &HashMap<&'a str, usize>,
    record_index: usize,
    weight: f64,
    depth: usize,
    head_vertex_mask: &[bool],
    resolution: &mut Resolution<'_>,
) -> Result<()> {
    let Resolution {
        stack,
        accumulator,
        receipt,
    } = resolution;
    let record = &records[record_index];
    if depth > MAX_RESOLUTION_DEPTH {
        receipt
            .depth_limited_targets
            .push(record.metadata.internal_name.clone());
        return Ok(());
    }
    if stack.contains(&record_index) {
        receipt
            .cyclic_dependencies
            .push(record.metadata.internal_name.clone());
        return Ok(());
    }
    if !weight.is_finite() {
        return Err(VaMError::InvalidMorphBank(format!(
            "morph {} resolved to a non-finite weight",
            record.metadata.internal_name
        )));
    }
    stack.push(record_index);

    for delta in &record.direct_deltas {
        let index = delta.vertex_index as usize;
        if index >= head_vertex_mask.len() {
            receipt.filtered_out_of_range += 1;
            continue;
        }
        if !head_vertex_mask[index] {
            receipt.filtered_outside_head += 1;
            continue;
        }
        let slot = accumulator.entry(delta.vertex_index).or_insert([0.0; 3]);
        for (axis, component) in slot.iter_mut().enumerate() {
            *component += delta.delta_cm[axis] * weight;
            if !component.is_finite() {
                return Err(VaMError::InvalidMorphBank(format!(
                    "morph {} produced a non-finite displacement",
                    record.metadata.internal_name
                )));
            }
        }
    }

    for formula in &record.metadata.formulas {
        let scaled = Formula {
            target_type: formula.target_type.clone(),
            target: formula.target.clone(),
            multiplier: formula.multiplier * weight,
        };
        if scaled.target_type == FormulaTarget::MorphValue {
            if scaled.multiplier == 0.0 {
                continue;
            }
            if let Some(&child) = names.get(scaled.target.as_str()) {
                resolve_one(
                    records,
                    names,
                    child,
                    scaled.multiplier,
                    depth + 1,
                    head_vertex_mask,
                    &mut Resolution {
                        stack,
                        accumulator,
                        receipt,
                    },
                )?;
            } else {
                receipt.missing_morph_targets.push(scaled.target);
            }
        } else {
            receipt.unsupported_formulas.push(scaled);
        }
    }
    stack.pop();
    Ok(())
}

pub(crate) fn decode_unity_bundle(encoded: &[u8]) -> Result<(Vec<u8>, Vec<BundleNode>)> {
    let mut header = ByteReader::new(encoded, Endian::Big);
    if header.null_text()? != "UnityFS" {
        return Err(VaMError::InvalidMorphBank(
            "built-in bank is not a UnityFS bundle".to_owned(),
        ));
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
    if declared_size != encoded.len() as u64 {
        return Err(VaMError::InvalidMorphBank(format!(
            "UnityFS declares {declared_size} bytes but contains {}",
            encoded.len()
        )));
    }
    let packed_directory_size = usize_from_u32(header.u32()?)?;
    let directory_size = usize_from_u32(header.u32()?)?;
    let flags = header.u32()?;
    let header_end = header.position();
    let directory_offset = if flags & 0x80 != 0 {
        encoded
            .len()
            .checked_sub(packed_directory_size)
            .ok_or_else(|| {
                VaMError::InvalidMorphBank("UnityFS directory offset underflow".to_owned())
            })?
    } else {
        header_end
    };
    let packed_directory = checked_slice(encoded, directory_offset, packed_directory_size)?;
    let directory = decompress_chunk(packed_directory, flags & 0x3f, directory_size)?;
    let (blocks, nodes) = parse_bundle_directory(&directory)?;

    let mut cursor = if flags & 0x80 != 0 {
        header_end
    } else {
        header_end
            .checked_add(packed_directory_size)
            .ok_or_else(|| VaMError::InvalidMorphBank("UnityFS data offset overflow".to_owned()))?
    };
    if flags & 0x200 != 0 {
        cursor = align_up(cursor, 16)?;
    }
    let total_size = blocks.iter().try_fold(0_usize, |total, block| {
        total.checked_add(block.uncompressed_size).ok_or_else(|| {
            VaMError::InvalidMorphBank("UnityFS decompressed byte count overflow".to_owned())
        })
    })?;
    let mut data_area = Vec::with_capacity(total_size);
    for block in blocks {
        let compressed = checked_slice(encoded, cursor, block.compressed_size)?;
        cursor = cursor.checked_add(block.compressed_size).ok_or_else(|| {
            VaMError::InvalidMorphBank("UnityFS block cursor overflow".to_owned())
        })?;
        data_area.extend_from_slice(&decompress_chunk(
            compressed,
            block.flags & 0x3f,
            block.uncompressed_size,
        )?);
    }
    Ok((data_area, nodes))
}

#[derive(Clone, Copy, Debug)]
struct BundleBlock {
    uncompressed_size: usize,
    compressed_size: usize,
    flags: u32,
}

fn parse_bundle_directory(bytes: &[u8]) -> Result<(Vec<BundleBlock>, Vec<BundleNode>)> {
    let mut reader = ByteReader::new(bytes, Endian::Big);
    reader.take(16)?;
    let block_count = usize_from_u32(reader.u32()?)?;
    if block_count > MAX_BLOCKS {
        return Err(VaMError::InvalidMorphBank(format!(
            "UnityFS declares {block_count} blocks"
        )));
    }
    let mut blocks = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        blocks.push(BundleBlock {
            uncompressed_size: usize_from_u32(reader.u32()?)?,
            compressed_size: usize_from_u32(reader.u32()?)?,
            flags: reader.u16()? as u32,
        });
    }
    let node_count = usize_from_u32(reader.u32()?)?;
    if node_count > MAX_NODES {
        return Err(VaMError::InvalidMorphBank(format!(
            "UnityFS declares {node_count} nodes"
        )));
    }
    let mut nodes = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        let offset = usize_from_u64(reader.u64()?)?;
        let size = usize_from_u64(reader.u64()?)?;
        reader.u32()?;
        nodes.push(BundleNode {
            offset,
            size,
            path: reader.null_text()?,
        });
    }
    Ok((blocks, nodes))
}

pub(crate) fn decompress_chunk(
    encoded: &[u8],
    compression: u32,
    expected: usize,
) -> Result<Vec<u8>> {
    match compression {
        0 => {
            if encoded.len() != expected {
                return Err(VaMError::InvalidMorphBank(format!(
                    "uncompressed UnityFS chunk has {} bytes; expected {expected}",
                    encoded.len()
                )));
            }
            Ok(encoded.to_vec())
        }
        2 | 3 => lz4_flex::block::decompress(encoded, expected).map_err(|error| {
            VaMError::InvalidMorphBank(format!("UnityFS LZ4 decompression failed: {error}"))
        }),
        1 => Err(VaMError::UnsupportedEncoding(
            "UnityFS LZMA chunks are not used by the supported VaM f_mb build".to_owned(),
        )),
        other => Err(VaMError::UnsupportedEncoding(format!(
            "UnityFS compression type {other}"
        ))),
    }
}

fn serialized_behaviours(cab: &[u8]) -> Result<Vec<SerializedPayload>> {
    if cab.len() < 20 {
        return Err(VaMError::InvalidMorphBank(
            "serialized asset header is truncated".to_owned(),
        ));
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
        return Err(VaMError::InvalidMorphBank(format!(
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
        return Err(VaMError::InvalidMorphBank(
            "serialized asset metadata/data offsets are inconsistent".to_owned(),
        ));
    }

    let mut reader = ByteReader::at(cab, endian, header.position())?;
    reader.null_text()?;
    reader.i32()?;
    let has_type_tree = reader.byte()? != 0;
    let type_count = bounded_i32(reader.i32()?, MAX_TYPES, "serialized type")?;
    let mut class_ids = Vec::with_capacity(type_count);
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
        if has_type_tree {
            let node_count = bounded_i32(reader.i32()?, 1_000_000, "type-tree node")?;
            let string_bytes = bounded_i32(reader.i32()?, 128 * 1024 * 1024, "type-tree text")?;
            let node_bytes = if version >= 19 { 32 } else { 24 };
            reader.take(node_count.checked_mul(node_bytes).ok_or_else(|| {
                VaMError::InvalidMorphBank("type-tree byte count overflow".to_owned())
            })?)?;
            reader.take(string_bytes)?;
        }
        class_ids.push(class_id);
    }

    let object_count = bounded_i32(reader.i32()?, MAX_OBJECTS, "serialized object")?;
    let mut result = Vec::new();
    for _ in 0..object_count {
        reader.align(4)?;
        let path_id = reader.i64()?;
        let object_offset = usize_from_u32(reader.u32()?)?;
        let byte_size = usize_from_u32(reader.u32()?)?;
        let type_index = reader.i32()?;
        let class_id = usize::try_from(type_index)
            .ok()
            .and_then(|index| class_ids.get(index))
            .copied();
        let start = data_offset.checked_add(object_offset).ok_or_else(|| {
            VaMError::InvalidMorphBank("serialized object offset overflow".to_owned())
        })?;
        let object = checked_slice(cab, start, byte_size)?;
        if class_id == Some(114) {
            result.push(SerializedPayload {
                path_id,
                endian,
                bytes: object.to_vec(),
            });
        }
    }
    Ok(result)
}

fn parse_bank_object(
    payload: &[u8],
    endian: Endian,
    object_path_id: i64,
    decode_deltas: bool,
) -> Result<Vec<BankMorph>> {
    let mut reader = ByteReader::new(payload, endian);
    reader.take(12)?;
    reader.aligned_flag()?;
    reader.take(12)?;
    reader.aligned_text()?;
    reader.aligned_flag()?;
    let morph_count = bounded_i32(reader.i32()?, MAX_MORPHS_PER_OBJECT, "bank morph")?;
    let mut records = Vec::with_capacity(morph_count);
    for morph_index in 0..morph_count {
        let visible = reader.aligned_flag()?;
        reader.aligned_flag()?;
        let disabled = reader.aligned_flag()?;
        let is_pose_control = reader.aligned_flag()?;
        let internal_name = reader.aligned_text()?;
        let display_name = reader.aligned_text()?;
        let override_name = reader.aligned_text()?;
        let region = reader.aligned_text()?;
        let override_region = reader.aligned_text()?;
        let group = reader.aligned_text()?;
        reader.f32()?;
        let start_value = reader.f32()? as f64;
        reader.f32()?;
        reader.aligned_flag()?;
        reader.f32()?;
        let minimum = reader.f32()? as f64;
        let maximum = reader.f32()? as f64;
        if !minimum.is_finite() || !maximum.is_finite() || minimum > maximum {
            return Err(VaMError::InvalidMorphBank(format!(
                "morph {internal_name:?} has invalid slider range [{minimum}, {maximum}]"
            )));
        }
        let default = if start_value.is_finite() && (minimum..=maximum).contains(&start_value) {
            start_value
        } else {
            0.0_f64.clamp(minimum, maximum)
        };
        let declared_delta_count = reader.i32()?;
        reader.aligned_flag()?;
        reader.aligned_flag()?;
        let delta_count = bounded_i32(reader.i32()?, MAX_DELTAS_PER_MORPH, "morph delta")?;
        let mut direct_deltas = if decode_deltas {
            Vec::with_capacity(delta_count)
        } else {
            Vec::new()
        };
        for _ in 0..delta_count {
            let vertex_index = reader.u32()?;
            let source_x = reader.f32()? as f64;
            let source_y = reader.f32()? as f64;
            let source_z = reader.f32()? as f64;
            if decode_deltas {
                let delta_cm = [-source_x * 100.0, source_y * 100.0, source_z * 100.0];
                if !delta_cm.iter().all(|value| value.is_finite()) {
                    return Err(VaMError::InvalidMorphBank(format!(
                        "morph {internal_name:?} contains a non-finite displacement"
                    )));
                }
                direct_deltas.push(SparseDelta {
                    vertex_index,
                    delta_cm,
                });
            }
        }

        let formula_count = bounded_i32(reader.i32()?, MAX_FORMULAS_PER_MORPH, "morph formula")?;
        let mut formulas = Vec::with_capacity(formula_count);
        for _ in 0..formula_count {
            let target_type = FormulaTarget::from(reader.i32()?);
            let target = reader.aligned_text()?;
            let multiplier = reader.f32()? as f64;
            if !multiplier.is_finite() {
                return Err(VaMError::InvalidMorphBank(format!(
                    "morph {internal_name:?} contains a non-finite formula"
                )));
            }
            formulas.push(Formula {
                target_type,
                target,
                multiplier,
            });
        }

        for _ in 0..4 {
            reader.aligned_flag()?;
        }
        reader.aligned_text()?;
        reader.aligned_text()?;
        reader.aligned_flag()?;
        reader.aligned_flag()?;
        reader.aligned_text()?;
        reader.aligned_text()?;
        reader.aligned_text()?;
        reader.aligned_flag()?;
        reader.aligned_text()?;

        if internal_name.is_empty() || declared_delta_count != delta_count as i32 {
            continue;
        }
        records.push(BankMorph {
            metadata: RawMorphMetadata {
                object_path_id,
                morph_index: morph_index as u32,
                label: if !override_name.is_empty() {
                    override_name
                } else if !display_name.is_empty() {
                    display_name
                } else {
                    internal_name.clone()
                },
                internal_name,
                region: if override_region.is_empty() {
                    region
                } else {
                    override_region
                },
                group,
                visible,
                disabled,
                is_pose_control,
                minimum,
                maximum,
                default,
                delta_count,
                formulas,
            },
            direct_deltas,
        });
    }
    Ok(records)
}

pub(crate) struct ByteReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
    endian: Endian,
}

impl<'a> ByteReader<'a> {
    pub(crate) fn new(bytes: &'a [u8], endian: Endian) -> Self {
        Self {
            bytes,
            cursor: 0,
            endian,
        }
    }

    pub(crate) fn at(bytes: &'a [u8], endian: Endian, cursor: usize) -> Result<Self> {
        if cursor > bytes.len() {
            return Err(VaMError::InvalidMorphBank(
                "binary reader starts beyond its input".to_owned(),
            ));
        }
        Ok(Self {
            bytes,
            cursor,
            endian,
        })
    }

    pub(crate) fn position(&self) -> usize {
        self.cursor
    }

    pub(crate) fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .cursor
            .checked_add(count)
            .ok_or_else(|| VaMError::InvalidMorphBank("binary cursor overflow".to_owned()))?;
        let value = self.bytes.get(self.cursor..end).ok_or_else(|| {
            VaMError::InvalidMorphBank("unexpected end of binary asset".to_owned())
        })?;
        self.cursor = end;
        Ok(value)
    }

    pub(crate) fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16> {
        let bytes: [u8; 2] = self.take(2)?.try_into().expect("two bytes");
        Ok(match self.endian {
            Endian::Little => u16::from_le_bytes(bytes),
            Endian::Big => u16::from_be_bytes(bytes),
        })
    }

    pub(crate) fn i16(&mut self) -> Result<i16> {
        let bytes: [u8; 2] = self.take(2)?.try_into().expect("two bytes");
        Ok(match self.endian {
            Endian::Little => i16::from_le_bytes(bytes),
            Endian::Big => i16::from_be_bytes(bytes),
        })
    }

    pub(crate) fn u32(&mut self) -> Result<u32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("four bytes");
        Ok(match self.endian {
            Endian::Little => u32::from_le_bytes(bytes),
            Endian::Big => u32::from_be_bytes(bytes),
        })
    }

    pub(crate) fn i32(&mut self) -> Result<i32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("four bytes");
        Ok(match self.endian {
            Endian::Little => i32::from_le_bytes(bytes),
            Endian::Big => i32::from_be_bytes(bytes),
        })
    }

    pub(crate) fn u64(&mut self) -> Result<u64> {
        let bytes: [u8; 8] = self.take(8)?.try_into().expect("eight bytes");
        Ok(match self.endian {
            Endian::Little => u64::from_le_bytes(bytes),
            Endian::Big => u64::from_be_bytes(bytes),
        })
    }

    pub(crate) fn i64(&mut self) -> Result<i64> {
        let bytes: [u8; 8] = self.take(8)?.try_into().expect("eight bytes");
        Ok(match self.endian {
            Endian::Little => i64::from_le_bytes(bytes),
            Endian::Big => i64::from_be_bytes(bytes),
        })
    }

    pub(crate) fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    pub(crate) fn align(&mut self, alignment: usize) -> Result<()> {
        self.cursor = align_up(self.cursor, alignment)?;
        if self.cursor > self.bytes.len() {
            return Err(VaMError::InvalidMorphBank(
                "binary alignment exceeds input".to_owned(),
            ));
        }
        Ok(())
    }

    pub(crate) fn null_text(&mut self) -> Result<String> {
        let tail = self
            .bytes
            .get(self.cursor..)
            .ok_or_else(|| VaMError::InvalidMorphBank("text cursor exceeds input".to_owned()))?;
        let length = tail
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| VaMError::InvalidMorphBank("unterminated binary text".to_owned()))?;
        if length > MAX_TEXT_BYTES {
            return Err(VaMError::InvalidMorphBank(
                "binary text exceeds safety limit".to_owned(),
            ));
        }
        let result = std::str::from_utf8(&tail[..length])
            .map_err(|_| VaMError::InvalidMorphBank("binary text is not UTF-8".to_owned()))?
            .to_owned();
        self.cursor += length + 1;
        Ok(result)
    }

    pub(crate) fn aligned_text(&mut self) -> Result<String> {
        let length = bounded_i32(self.i32()?, MAX_TEXT_BYTES, "text")?;
        let value = std::str::from_utf8(self.take(length)?)
            .map_err(|_| VaMError::InvalidMorphBank("morph text is not UTF-8".to_owned()))?
            .to_owned();
        self.align(4)?;
        Ok(value)
    }

    pub(crate) fn aligned_flag(&mut self) -> Result<bool> {
        let value = self.byte()? != 0;
        self.align(4)?;
        Ok(value)
    }
}

pub(crate) fn checked_slice(bytes: &[u8], offset: usize, size: usize) -> Result<&[u8]> {
    let end = offset
        .checked_add(size)
        .ok_or_else(|| VaMError::InvalidMorphBank("binary byte range overflow".to_owned()))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| VaMError::InvalidMorphBank("binary byte range exceeds input".to_owned()))
}

pub(crate) fn align_up(value: usize, alignment: usize) -> Result<usize> {
    if alignment == 0 || !alignment.is_power_of_two() {
        return Err(VaMError::InvalidMorphBank(
            "invalid binary alignment".to_owned(),
        ));
    }
    value
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .ok_or_else(|| VaMError::InvalidMorphBank("binary alignment overflow".to_owned()))
}

pub(crate) fn bounded_i32(value: i32, maximum: usize, label: &str) -> Result<usize> {
    let converted = usize::try_from(value)
        .map_err(|_| VaMError::InvalidMorphBank(format!("{label} count {value} is negative")))?;
    if converted > maximum {
        return Err(VaMError::InvalidMorphBank(format!(
            "{label} count {converted} exceeds {maximum}"
        )));
    }
    Ok(converted)
}

pub(crate) fn usize_from_u32(value: u32) -> Result<usize> {
    usize::try_from(value)
        .map_err(|_| VaMError::InvalidMorphBank("32-bit size exceeds this platform".to_owned()))
}

pub(crate) fn usize_from_u64(value: u64) -> Result<usize> {
    usize::try_from(value)
        .map_err(|_| VaMError::InvalidMorphBank("64-bit size exceeds this platform".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(name: &str, formulas: Vec<Formula>, delta_count: usize) -> RawMorphMetadata {
        RawMorphMetadata {
            object_path_id: 3,
            morph_index: 0,
            internal_name: name.to_owned(),
            label: name.to_owned(),
            region: "Head/Eyes".to_owned(),
            group: String::new(),
            visible: true,
            disabled: false,
            is_pose_control: false,
            minimum: -1.0,
            maximum: 1.0,
            default: 0.0,
            delta_count,
            formulas,
        }
    }

    #[test]
    fn recursively_composes_morph_value_and_preserves_bone_formula() {
        let parent = BankMorph {
            metadata: metadata(
                "parent",
                vec![Formula {
                    target_type: FormulaTarget::MorphValue,
                    target: "child".to_owned(),
                    multiplier: 0.5,
                }],
                1,
            ),
            direct_deltas: vec![SparseDelta {
                vertex_index: 0,
                delta_cm: [1.0, 0.0, 0.0],
            }],
        };
        let child = BankMorph {
            metadata: metadata(
                "child",
                vec![Formula {
                    target_type: FormulaTarget::BoneCenterX,
                    target: "lEye".to_owned(),
                    multiplier: 2.0,
                }],
                1,
            ),
            direct_deltas: vec![SparseDelta {
                vertex_index: 0,
                delta_cm: [0.0, 4.0, 0.0],
            }],
        };
        let source = BuiltinMorphSource {
            bundle_path: PathBuf::from("f_mb"),
            object_path_id: 3,
            morph_index: 0,
        };
        let resolved = resolve_records(&[parent, child], 0, &[true], &source).unwrap();
        assert_eq!(resolved.sparse_deltas[0].delta_cm, [1.0, 2.0, 0.0]);
        assert_eq!(resolved.receipt.unsupported_formulas.len(), 1);
        assert_eq!(resolved.receipt.unsupported_formulas[0].multiplier, 1.0);
    }

    #[test]
    fn recursion_cycle_is_reported_without_looping() {
        let formula = |target: &str| Formula {
            target_type: FormulaTarget::MorphValue,
            target: target.to_owned(),
            multiplier: 1.0,
        };
        let a = BankMorph {
            metadata: metadata("a", vec![formula("b")], 0),
            direct_deltas: Vec::new(),
        };
        let b = BankMorph {
            metadata: metadata("b", vec![formula("a")], 0),
            direct_deltas: Vec::new(),
        };
        let source = BuiltinMorphSource {
            bundle_path: PathBuf::from("f_mb"),
            object_path_id: 3,
            morph_index: 0,
        };
        let resolved = resolve_records(&[a, b], 0, &[true], &source).unwrap();
        assert_eq!(resolved.receipt.cyclic_dependencies, ["a"]);
    }

    #[test]
    fn lz4hc_bundle_chunks_decode_with_the_raw_lz4_contract() {
        let expected = b"Vkit VaM LZ4 block".repeat(32);
        let encoded = lz4_flex::block::compress(&expected);
        assert_eq!(
            decompress_chunk(&encoded, 3, expected.len()).unwrap(),
            expected
        );
    }

    #[test]
    fn public_payload_types_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ResolvedMorph>();
        assert_send_sync::<MorphResolutionReceipt>();
        assert_send_sync::<BuiltinMorphSession>();
    }
}
