use std::{path::PathBuf, sync::Arc};

#[cfg(test)]
use std::env;
use std::{fs, io::Write, path::Path, time::UNIX_EPOCH};

use crc32fast::hash as crc32;
use lz4_flex::compress_prepend_size;
use lz4_flex::decompress_size_prepended;
use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use vkit_core::vam::{BuiltinMorphSession, ResolvedMorph, VaMRoot};
use vkit_core::{
    formats::{DazGeometry, topology_digest},
    vam::{MorphCatalog, MorphCategory, SkinSex, canonical_head_vertex_mask},
};

const MAGIC: [u8; 8] = *b"FMMBC001";
const LEGACY_VERSION: u16 = 2;
const CHEEKBONES_CATEGORY_VERSION: u16 = 3;

const BODY_CATEGORY_VERSION: u16 = 4;

const EYELID_LOOK_CATALOG_VERSION: u16 = 6;
const VERSION: u16 = EYELID_LOOK_CATALOG_VERSION;
const HEADER_BYTES: usize = 86;
const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PAYLOAD_BYTES: usize = 768 * 1024 * 1024;
const MAX_MORPHS: usize = 4_096;
const MAX_TEXT_BYTES: usize = 4_096;
const ZERO_TOLERANCE_CM: f64 = 1.0e-12;
const MAX_REPORTED_WARNINGS: usize = 16;

#[cfg(test)]
pub(crate) fn builtin_morph_names(sex: SkinSex) -> impl Iterator<Item = &'static str> {
    match sex {
        SkinSex::Female => include_str!("../resources/g2f-builtin-morph-names.txt"),
        SkinSex::Male => include_str!("../resources/g2m-builtin-morph-names.txt"),
        SkinSex::Unknown => "",
    }
    .lines()
    .filter(|name| !name.is_empty())
}

#[derive(Clone, Debug)]
pub struct CachedBuiltinMorph {
    pub internal_name: String,
    pub label: String,
    pub category: MorphCategory,
    pub minimum: f64,
    pub maximum: f64,
    pub default: f64,

    pub deltas: Arc<Vec<(u32, [f64; 3])>>,
    pub unsupported_formula_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MorphCacheDisposition {
    Hit,

    Rebuilt,
}

#[derive(Clone, Debug)]
pub struct MorphCacheReceipt {
    pub path: PathBuf,
    pub disposition: MorphCacheDisposition,
    pub source_bytes: u64,
    pub morph_count: usize,
    pub skipped_count: usize,
}

#[derive(Clone, Debug)]
pub struct MorphCacheResult {
    pub morphs: Vec<CachedBuiltinMorph>,
    pub receipt: MorphCacheReceipt,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SourceStamp {
    bytes: u64,
    modified_ns: u64,
}

#[derive(Clone, Debug)]
struct CachedEntry {
    internal_name: String,
    label: String,
    category: MorphCategory,
    minimum: f64,
    maximum: f64,
    default: f64,
    unsupported_formula_count: u32,
    deltas: Vec<(u32, [f64; 3])>,
}

#[derive(Debug)]
struct DecodedCache {
    morphs: Vec<CachedBuiltinMorph>,
}

#[derive(Clone, Copy)]
struct ExpectedSource<'a> {
    canonical_bank: &'a Path,
    stamp: SourceStamp,
}

pub fn default_cache_root() -> Option<PathBuf> {
    vkit_core::cache_root()
}

pub fn load_or_build(
    root: &VaMRoot,
    sex: SkinSex,
    geometry: &DazGeometry,
    cache_root: &Path,
) -> Result<MorphCacheResult, String> {
    geometry.validate().map_err(|error| error.to_string())?;
    let bank = root
        .morph_bank_path(sex)
        .map_err(|error| error.to_string())?;
    let canonical_bank = fs::canonicalize(bank).map_err(|error| {
        format!(
            "failed to canonicalize morph bank {}: {error}",
            bank.display()
        )
    })?;
    let stamp = source_stamp(&canonical_bank)?;
    let topology = topology_digest(geometry.vertices.len(), &geometry.faces)
        .map_err(|error| error.to_string())?;
    let cache_path = cache_path(cache_root, &canonical_bank, sex);

    let mut warnings = Vec::new();
    if cache_path.is_file() {
        match read_cache(
            &cache_path,
            Some(ExpectedSource {
                canonical_bank: &canonical_bank,
                stamp,
            }),
            sex,
            topology,
            geometry,
        ) {
            Ok(decoded) => {
                return Ok(MorphCacheResult {
                    receipt: MorphCacheReceipt {
                        path: cache_path,
                        disposition: MorphCacheDisposition::Hit,
                        source_bytes: stamp.bytes,
                        morph_count: decoded.morphs.len(),
                        skipped_count: 0,
                    },
                    morphs: decoded.morphs,
                    warnings,
                });
            }
            Err(error) => warnings.push(format!(
                "existing local morph cache was stale or invalid and will be rebuilt: {error}"
            )),
        }
    }

    let (entries, skipped_count, build_warnings) = build_entries(root, sex, geometry)?;
    warnings.extend(build_warnings);
    if entries.is_empty() {
        return Err("VaM morph bank produced no usable head morphs".to_owned());
    }

    match encode_cache(&canonical_bank, stamp, sex, topology, &entries)
        .and_then(|encoded| write_atomic(&cache_path, &encoded))
    {
        Ok(()) => {}
        Err(error) => push_bounded_warning(
            &mut warnings,
            format!(
                "the resolved catalog could not be cached at {}, so it will be rebuilt next launch: {error}",
                cache_path.display()
            ),
        ),
    }
    let morphs = entries_to_descriptors(entries);
    Ok(MorphCacheResult {
        receipt: MorphCacheReceipt {
            path: cache_path,
            disposition: MorphCacheDisposition::Rebuilt,
            source_bytes: stamp.bytes,
            morph_count: morphs.len(),
            skipped_count,
        },
        morphs,
        warnings,
    })
}

fn load_rig_sources(
    root: &VaMRoot,
    sex: SkinSex,
) -> Result<(vkit_core::vam::RestSkeleton, vkit_core::vam::SkinBinding), String> {
    let geometry_sex = match sex {
        SkinSex::Male => vkit_core::vam::GeometrySex::Male,
        _ => vkit_core::vam::GeometrySex::Female,
    };
    let skeleton =
        vkit_core::vam::extract_rest_skeleton(&root.person_atom_bundle_path(), geometry_sex)
            .map_err(|error| format!("rest skeleton: {error}"))?
            .to_canonical_space(100.0);
    let binding =
        vkit_core::vam::extract_skin_binding(&root.neutral_base_bundle_path(geometry_sex))
            .map_err(|error| format!("skin binding: {error}"))?;
    Ok((skeleton, binding))
}

fn merge_rig_into(
    mut resolved: vkit_core::vam::ResolvedMorph,
    rig: &[(u32, [f64; 3])],
) -> vkit_core::vam::ResolvedMorph {
    let merged = vkit_core::vam::merge_rig_delta(
        &resolved
            .sparse_deltas
            .iter()
            .map(|delta| (delta.vertex_index, delta.delta_cm))
            .collect::<Vec<_>>(),
        rig,
    );
    resolved.sparse_deltas = merged
        .into_iter()
        .map(|(vertex_index, delta_cm)| vkit_core::vam::SparseDelta {
            vertex_index,
            delta_cm,
        })
        .collect();
    resolved
}

fn build_entries(
    root: &VaMRoot,
    sex: SkinSex,
    geometry: &DazGeometry,
) -> Result<(Vec<CachedEntry>, usize, Vec<String>), String> {
    let session =
        BuiltinMorphSession::open_for_sex(root, sex).map_err(|error| error.to_string())?;
    let catalog = MorphCatalog::from_session(&session);
    let mut descriptors = catalog
        .entries()
        .iter()
        .filter(|entry| entry.is_offered_control() && entry.category != MorphCategory::Body)
        .cloned()
        .collect::<Vec<_>>();

    descriptors.sort_by_key(|entry| (entry.source.object_path_id, entry.source.morph_index));
    let head_vertex_mask =
        canonical_head_vertex_mask(geometry).map_err(|error| error.to_string())?;

    let mut entries = Vec::new();
    let mut skipped_count = 0;
    let mut warnings = Vec::new();
    let rig = match load_rig_sources(root, sex) {
        Ok(sources) => Some(sources),
        Err(error) => {
            push_bounded_warning(&mut warnings, error);
            None
        }
    };
    for descriptor in &descriptors {
        let resolved = match session.load_resolved(&descriptor.source, &head_vertex_mask) {
            Ok(resolved) => resolved,
            Err(error) => {
                skipped_count += 1;
                push_bounded_warning(
                    &mut warnings,
                    format!("{}: {error}", descriptor.internal_name),
                );
                continue;
            }
        };
        let unsupported = resolved.receipt.unsupported_formulas.len()
            + resolved.receipt.missing_morph_targets.len()
            + resolved.receipt.cyclic_dependencies.len()
            + resolved.receipt.depth_limited_targets.len();

        let rig_delta = rig.as_ref().map_or_else(Vec::new, |(skeleton, binding)| {
            vkit_core::vam::rig_delta(skeleton, binding, &descriptor.formulas, &geometry.vertices)
        });
        let resolved = if rig_delta.is_empty() {
            resolved
        } else {
            merge_rig_into(resolved, &rig_delta)
        };
        let deltas = match compact_resolved_deltas(
            &resolved,
            geometry.vertices.len(),
            descriptor.minimum,
            descriptor.maximum,
            descriptor.default,
        ) {
            Ok(deltas) => deltas,
            Err(error) => {
                skipped_count += 1;
                push_bounded_warning(
                    &mut warnings,
                    format!("{}: {error}", descriptor.internal_name),
                );
                continue;
            }
        };
        if deltas.is_empty() {
            skipped_count += 1;
            push_bounded_warning(
                &mut warnings,
                format!("{}: resolved target is empty", descriptor.internal_name),
            );
            continue;
        }
        entries.push(CachedEntry {
            internal_name: descriptor.internal_name.clone(),
            label: if descriptor.label.trim().is_empty() {
                descriptor.internal_name.clone()
            } else {
                descriptor.label.clone()
            },
            category: descriptor.category,
            minimum: descriptor.minimum,
            maximum: descriptor.maximum,
            default: descriptor.default,
            unsupported_formula_count: u32::try_from(unsupported).unwrap_or(u32::MAX),
            deltas,
        });
    }
    entries.sort_by(|left, right| {
        left.internal_name
            .to_ascii_lowercase()
            .cmp(&right.internal_name.to_ascii_lowercase())
    });
    entries.dedup_by(|left, right| {
        left.internal_name
            .eq_ignore_ascii_case(&right.internal_name)
    });
    Ok((entries, skipped_count, warnings))
}

fn compact_resolved_deltas(
    resolved: &ResolvedMorph,
    expected_vertex_count: usize,
    minimum: f64,
    maximum: f64,
    default: f64,
) -> Result<Vec<(u32, [f64; 3])>, String> {
    if resolved.vertex_count != expected_vertex_count {
        return Err(format!(
            "resolved target has {} vertices; template has {expected_vertex_count}",
            resolved.vertex_count
        ));
    }
    if !minimum.is_finite()
        || !maximum.is_finite()
        || !default.is_finite()
        || minimum >= maximum
        || !(minimum..=maximum).contains(&default)
    {
        return Err("resolved target has an invalid slider range".to_owned());
    }

    let mut compacted = Vec::with_capacity(resolved.sparse_deltas.len());
    let mut previous = None;
    for sparse in &resolved.sparse_deltas {
        let vertex_id = sparse.vertex_index;
        if vertex_id as usize >= expected_vertex_count {
            return Err(format!(
                "resolved target references vertex {vertex_id}, but template has {expected_vertex_count} vertices"
            ));
        }
        if previous.is_some_and(|prior| prior >= vertex_id) {
            return Err("resolved target vertex IDs are not strictly increasing".to_owned());
        }
        if !sparse.delta_cm.iter().all(|value| value.is_finite()) {
            return Err(format!(
                "resolved target vertex {vertex_id} has a non-finite delta"
            ));
        }
        let compact = sparse.delta_cm.map(|value| f64::from(value as f32));
        let norm = compact
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
        if norm > ZERO_TOLERANCE_CM {
            compacted.push((vertex_id, compact));
        }
        previous = Some(vertex_id);
    }
    Ok(compacted)
}

fn push_bounded_warning(warnings: &mut Vec<String>, warning: String) {
    if warnings.len() < MAX_REPORTED_WARNINGS {
        warnings.push(warning);
    }
}

fn source_stamp(path: &Path) -> Result<SourceStamp, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to inspect morph bank {}: {error}", path.display()))?;
    let modified = metadata.modified().map_err(|error| {
        format!(
            "failed to read morph bank modification time {}: {error}",
            path.display()
        )
    })?;
    let modified_ns = modified
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "morph bank modification time predates UNIX epoch".to_owned())?
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64;
    Ok(SourceStamp {
        bytes: metadata.len(),
        modified_ns,
    })
}

fn cache_path(cache_root: &Path, canonical_bank: &Path, sex: SkinSex) -> PathBuf {
    let mut digest = Sha256::new();

    digest.update(b"vkit.vam-morph-cache.path.v1\0");
    digest.update(canonical_bank.to_string_lossy().as_bytes());
    digest.update([sex_byte(sex)]);
    let hash = digest.finalize();
    let hex = hash
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let family = match sex {
        SkinSex::Female => "g2f",
        SkinSex::Male => "g2m",
        SkinSex::Unknown => "unknown",
    };
    cache_root
        .join("morphs")
        .join(format!("{family}-{hex}.fmmcache"))
}

fn encode_cache(
    canonical_bank: &Path,
    stamp: SourceStamp,
    sex: SkinSex,
    topology: [u8; 32],
    entries: &[CachedEntry],
) -> Result<Vec<u8>, String> {
    if entries.is_empty() || entries.len() > MAX_MORPHS {
        return Err(format!(
            "local morph cache entry count {} is outside 1..={MAX_MORPHS}",
            entries.len()
        ));
    }
    let bank_text = canonical_bank.to_string_lossy();
    let bank_len = u16::try_from(bank_text.len())
        .map_err(|_| "morph bank path is too long for cache provenance".to_owned())?;
    let mut payload = Vec::new();
    for entry in entries {
        encode_entry(&mut payload, entry)?;
    }
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(format!(
            "local morph cache payload is {} bytes; limit is {MAX_PAYLOAD_BYTES}",
            payload.len()
        ));
    }
    let compressed = compress_prepend_size(&payload);
    let mut output = Vec::with_capacity(HEADER_BYTES + bank_text.len() + compressed.len());
    output.extend_from_slice(&MAGIC);
    output.extend_from_slice(&VERSION.to_le_bytes());
    output.push(sex_byte(sex));
    output.push(0);
    output.extend_from_slice(&stamp.bytes.to_le_bytes());
    output.extend_from_slice(&stamp.modified_ns.to_le_bytes());
    output.extend_from_slice(&topology);
    output.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    output.extend_from_slice(&(compressed.len() as u64).to_le_bytes());
    output.extend_from_slice(&bank_len.to_le_bytes());
    output.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    output.extend_from_slice(&crc32(&payload).to_le_bytes());
    debug_assert_eq!(output.len(), HEADER_BYTES);
    output.extend_from_slice(bank_text.as_bytes());
    output.extend_from_slice(&compressed);
    if output.len() as u64 > MAX_CACHE_BYTES {
        return Err(format!(
            "local morph cache is {} bytes; limit is {MAX_CACHE_BYTES}",
            output.len()
        ));
    }
    Ok(output)
}

fn encode_entry(output: &mut Vec<u8>, entry: &CachedEntry) -> Result<(), String> {
    let name_len = text_len(&entry.internal_name, "morph name")?;
    let label_len = text_len(&entry.label, "morph label")?;
    let delta_count = u32::try_from(entry.deltas.len())
        .map_err(|_| "morph delta count exceeds u32".to_owned())?;
    output.extend_from_slice(&name_len.to_le_bytes());
    output.extend_from_slice(&label_len.to_le_bytes());
    output.push(category_byte(entry.category));
    output.push(0);
    output.extend_from_slice(&entry.unsupported_formula_count.to_le_bytes());
    output.extend_from_slice(&entry.minimum.to_bits().to_le_bytes());
    output.extend_from_slice(&entry.maximum.to_bits().to_le_bytes());
    output.extend_from_slice(&entry.default.to_bits().to_le_bytes());
    output.extend_from_slice(&delta_count.to_le_bytes());
    output.extend_from_slice(entry.internal_name.as_bytes());
    output.extend_from_slice(entry.label.as_bytes());
    let mut previous = None;
    for &(vertex_id, delta) in &entry.deltas {
        if previous.is_some_and(|prior| prior >= vertex_id) {
            return Err(format!(
                "morph {} cache deltas are not strictly ordered",
                entry.internal_name
            ));
        }
        output.extend_from_slice(&vertex_id.to_le_bytes());
        for value in delta {
            let compact = value as f32;
            if !compact.is_finite() {
                return Err(format!(
                    "morph {} contains a non-finite cache delta",
                    entry.internal_name
                ));
            }
            output.extend_from_slice(&compact.to_bits().to_le_bytes());
        }
        previous = Some(vertex_id);
    }
    Ok(())
}

fn read_cache(
    path: &Path,
    expected_source: Option<ExpectedSource<'_>>,
    expected_sex: SkinSex,
    expected_topology: [u8; 32],
    geometry: &DazGeometry,
) -> Result<DecodedCache, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("failed to inspect cache {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("cache candidate is not a regular local file".to_owned());
    }
    let file_bytes = metadata.len();
    if file_bytes > MAX_CACHE_BYTES {
        return Err(format!(
            "cache is {file_bytes} bytes; maximum is {MAX_CACHE_BYTES}"
        ));
    }
    let encoded = fs::read(path)
        .map_err(|error| format!("failed to read cache {}: {error}", path.display()))?;
    let mut cursor = Cursor::new(&encoded);
    if cursor.take(8)? != MAGIC {
        return Err("cache magic does not match".to_owned());
    }
    let schema_version = cursor.u16()?;
    if schema_version == LEGACY_VERSION {
        return Err(format!(
            "legacy cache schema {LEGACY_VERSION} lacks region/group identity metadata and must be rebuilt"
        ));
    }
    if schema_version != VERSION {
        return Err(format!(
            "cache schema version {schema_version} is unsupported; expected {VERSION}"
        ));
    }
    if cursor.u8()? != sex_byte(expected_sex) || cursor.u8()? != 0 {
        return Err("cache figure family or reserved header does not match".to_owned());
    }
    let actual_stamp = SourceStamp {
        bytes: cursor.u64()?,
        modified_ns: cursor.u64()?,
    };
    if expected_source.is_some_and(|source| actual_stamp != source.stamp) {
        return Err("source morph bank size or modification time changed".to_owned());
    }
    if cursor.take(32)? != expected_topology {
        return Err("G2 topology binding changed".to_owned());
    }
    let raw_len = usize::try_from(cursor.u64()?)
        .map_err(|_| "cache payload length does not fit this platform".to_owned())?;
    let compressed_len = usize::try_from(cursor.u64()?)
        .map_err(|_| "cache compressed length does not fit this platform".to_owned())?;
    let bank_len = cursor.u16()? as usize;
    let entry_count = cursor.u32()? as usize;
    let expected_crc32 = cursor.u32()?;
    if raw_len > MAX_PAYLOAD_BYTES || entry_count == 0 || entry_count > MAX_MORPHS {
        return Err("cache payload bounds are invalid".to_owned());
    }
    let cached_bank = cursor.text(bank_len, "morph bank path")?;
    if cached_bank.is_empty() {
        return Err("cache provenance path is empty".to_owned());
    }
    if expected_source.is_some_and(|source| Path::new(&cached_bank) != source.canonical_bank) {
        return Err("cache provenance path does not match selected VaM bank".to_owned());
    }
    let compressed = cursor.take(compressed_len)?;
    if cursor.remaining() != 0 {
        return Err("cache has trailing bytes".to_owned());
    }
    if compressed.len() < 4 {
        return Err("cache compressed payload is truncated".to_owned());
    }
    let declared =
        u32::from_le_bytes(compressed[..4].try_into().expect("four-byte prefix")) as usize;
    if declared != raw_len || declared > MAX_PAYLOAD_BYTES {
        return Err("cache compressed size prefix is invalid".to_owned());
    }
    let payload = decompress_size_prepended(compressed)
        .map_err(|error| format!("cache LZ4 payload is invalid: {error}"))?;
    if payload.len() != raw_len {
        return Err("cache decompressed length does not match header".to_owned());
    }
    if crc32(&payload) != expected_crc32 {
        return Err("cache payload CRC-32 does not match".to_owned());
    }
    let entries = decode_entries(
        &payload,
        entry_count,
        geometry.vertices.len(),
        schema_version,
    )?;
    Ok(DecodedCache {
        morphs: entries_to_descriptors(entries),
    })
}

fn decode_entries(
    payload: &[u8],
    entry_count: usize,
    vertex_count: usize,
    schema_version: u16,
) -> Result<Vec<CachedEntry>, String> {
    let mut cursor = Cursor::new(payload);
    let mut entries = Vec::with_capacity(entry_count);
    let mut previous_name: Option<String> = None;
    for index in 0..entry_count {
        let name_len = cursor.u16()? as usize;
        let label_len = cursor.u16()? as usize;
        if name_len == 0
            || name_len > MAX_TEXT_BYTES
            || label_len == 0
            || label_len > MAX_TEXT_BYTES
        {
            return Err(format!("cache morph {index} text bounds are invalid"));
        }
        let category = category_from_byte(cursor.u8()?, schema_version)?;
        if cursor.u8()? != 0 {
            return Err(format!("cache morph {index} reserved byte is nonzero"));
        }
        let unsupported_formula_count = cursor.u32()?;
        let minimum = f64::from_bits(cursor.u64()?);
        let maximum = f64::from_bits(cursor.u64()?);
        let default = f64::from_bits(cursor.u64()?);
        if !minimum.is_finite()
            || !maximum.is_finite()
            || !default.is_finite()
            || minimum >= maximum
            || !(minimum..=maximum).contains(&default)
        {
            return Err(format!("cache morph {index} slider range is invalid"));
        }
        let delta_count = cursor.u32()? as usize;
        if delta_count == 0 || delta_count > vertex_count {
            return Err(format!("cache morph {index} delta count is invalid"));
        }
        let internal_name = cursor.text(name_len, "morph name")?;
        let label = cursor.text(label_len, "morph label")?;
        let normalized_name = internal_name.to_ascii_lowercase();
        if previous_name
            .as_ref()
            .is_some_and(|previous| previous.as_str() >= normalized_name.as_str())
        {
            return Err("cache morph names are not strictly ordered".to_owned());
        }
        previous_name = Some(normalized_name);
        let mut deltas = Vec::with_capacity(delta_count);
        let mut previous_vertex = None;
        for delta_index in 0..delta_count {
            let vertex_id = cursor.u32()?;
            if vertex_id as usize >= vertex_count
                || previous_vertex.is_some_and(|previous| previous >= vertex_id)
            {
                return Err(format!(
                    "cache morph {index} delta {delta_index} vertex ID is invalid"
                ));
            }
            let mut delta = [0.0; 3];
            for value in &mut delta {
                *value = f64::from(f32::from_bits(cursor.u32()?));
            }
            if !delta.iter().all(|value| value.is_finite())
                || delta.iter().map(|value| value * value).sum::<f64>().sqrt() <= ZERO_TOLERANCE_CM
            {
                return Err(format!(
                    "cache morph {index} delta {delta_index} is invalid"
                ));
            }
            deltas.push((vertex_id, delta));
            previous_vertex = Some(vertex_id);
        }
        entries.push(CachedEntry {
            internal_name,
            label,
            category,
            minimum,
            maximum,
            default,
            unsupported_formula_count,
            deltas,
        });
    }
    if cursor.remaining() != 0 {
        return Err("cache morph payload has trailing bytes".to_owned());
    }
    Ok(entries)
}

fn entries_to_descriptors(entries: Vec<CachedEntry>) -> Vec<CachedBuiltinMorph> {
    entries
        .into_iter()
        .filter(|entry| {
            !MorphCatalog::is_person_specific_morph(&entry.internal_name, &entry.label, "", "")
        })
        .map(|entry| {
            let category =
                MorphCatalog::refine_category(entry.category, &entry.internal_name, &entry.label);
            CachedBuiltinMorph {
                internal_name: entry.internal_name,
                label: entry.label,
                category,
                minimum: entry.minimum,
                maximum: entry.maximum,
                default: entry.default,
                deltas: Arc::new(entry.deltas),
                unsupported_formula_count: entry.unsupported_formula_count as usize,
            }
        })
        .collect()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "morph cache path has no parent".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create morph cache directory: {error}"))?;
    let mut file = NamedTempFile::new_in(parent)
        .map_err(|error| format!("failed to create temporary morph cache: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("failed to write temporary morph cache: {error}"))?;
    file.as_file()
        .sync_all()
        .map_err(|error| format!("failed to flush temporary morph cache: {error}"))?;
    file.persist(path)
        .map_err(|error| format!("failed to publish morph cache: {}", error.error))?;
    Ok(())
}

fn text_len(value: &str, label: &str) -> Result<u16, String> {
    if value.is_empty() || value.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "{label} must contain 1..={MAX_TEXT_BYTES} UTF-8 bytes"
        ));
    }
    u16::try_from(value.len()).map_err(|_| format!("{label} length exceeds u16"))
}

const fn sex_byte(sex: SkinSex) -> u8 {
    match sex {
        SkinSex::Female => 1,
        SkinSex::Male => 2,
        SkinSex::Unknown => 0,
    }
}

const fn category_byte(category: MorphCategory) -> u8 {
    match category {
        MorphCategory::Eyes => 1,
        MorphCategory::Brows => 2,
        MorphCategory::Nose => 3,
        MorphCategory::Mouth => 4,
        MorphCategory::Jaw => 5,
        MorphCategory::Cheeks => 6,
        MorphCategory::Ears => 7,
        MorphCategory::Expression => 8,
        MorphCategory::Head => 9,
        MorphCategory::Cheekbones => 10,
        MorphCategory::Body => 11,
    }
}

fn category_from_byte(value: u8, schema_version: u16) -> Result<MorphCategory, String> {
    match value {
        1 => Ok(MorphCategory::Eyes),
        2 => Ok(MorphCategory::Brows),
        3 => Ok(MorphCategory::Nose),
        4 => Ok(MorphCategory::Mouth),
        5 => Ok(MorphCategory::Jaw),
        6 => Ok(MorphCategory::Cheeks),
        7 => Ok(MorphCategory::Ears),
        8 => Ok(MorphCategory::Expression),
        9 => Ok(MorphCategory::Head),
        10 if schema_version >= CHEEKBONES_CATEGORY_VERSION => Ok(MorphCategory::Cheekbones),
        10 => Err(format!(
            "cached morph category 10 requires cache schema {CHEEKBONES_CATEGORY_VERSION}; found {schema_version}"
        )),
        11 if schema_version >= BODY_CATEGORY_VERSION => Ok(MorphCategory::Body),
        11 => Err(format!(
            "cached morph category 11 requires cache schema {BODY_CATEGORY_VERSION}; found {schema_version}"
        )),
        _ => Err(format!("unknown cached morph category {value}")),
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| "cache cursor overflow".to_owned())?;
        let result = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| "cache is truncated".to_owned())?;
        self.offset = end;
        Ok(result)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("two-byte cache slice"),
        ))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four-byte cache slice"),
        ))
    }

    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("eight-byte cache slice"),
        ))
    }

    fn text(&mut self, count: usize, label: &str) -> Result<String, String> {
        std::str::from_utf8(self.take(count)?)
            .map(str::to_owned)
            .map_err(|_| format!("cache {label} is not valid UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vkit_core::vam::{MorphResolutionReceipt, SparseDelta};

    use super::*;

    fn geometry() -> DazGeometry {
        DazGeometry::new(
            "cache-fixture".into(),
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![vec![0, 1, 2]],
            vkit_core::formats::GroupTable {
                indices: vec![0],
                names: vec!["fixture".into()],
            },
            vkit_core::formats::GroupTable {
                indices: vec![0],
                names: vec!["Face".into()],
            },
            json!({}),
        )
        .unwrap()
    }

    fn entries() -> Vec<CachedEntry> {
        vec![CachedEntry {
            internal_name: "PHMCacheTest".into(),
            label: "Cache Test".into(),
            category: MorphCategory::Mouth,
            minimum: -1.0,
            maximum: 1.0,
            default: 0.0,
            unsupported_formula_count: 2,
            deltas: vec![(1, [0.0, 0.25, 0.0])],
        }]
    }

    #[test]
    fn legacy_cache_rows_are_refined_and_named_identities_are_removed() {
        let mut legacy = entries();
        legacy[0].internal_name = "PHMCheekBonesWidthUpper".into();
        legacy[0].label = "Cheek Bones Width Upper".into();
        legacy[0].category = MorphCategory::Cheeks;
        legacy.push(CachedEntry {
            internal_name: "FHMAiko6Head".into(),
            label: "Aiko 6 Head".into(),
            category: MorphCategory::Head,
            minimum: -1.0,
            maximum: 1.0,
            default: 0.0,
            unsupported_formula_count: 0,
            deltas: vec![(2, [0.0, 0.0, 0.25])],
        });

        let descriptors = entries_to_descriptors(legacy);
        assert_eq!(descriptors.len(), 1);
        assert_eq!(descriptors[0].category, MorphCategory::Cheekbones);
        assert_eq!(descriptors[0].internal_name, "PHMCheekBonesWidthUpper");
    }

    #[test]
    fn both_packs_carry_the_gaze_driven_eyelid_controls() {
        for (sex, expected) in [
            (
                SkinSex::Female,
                vec![
                    "PHMEyeLidsBottomDownL",
                    "PHMEyeLidsBottomDownR",
                    "PHMEyeLidsBottomUpL",
                    "PHMEyeLidsBottomUpR",
                    "PHMEyeLidsTopUpL",
                    "PHMEyeLidsTopUpR",
                    "PHMEyelidsTopDownL",
                    "PHMEyelidsTopDownR",
                ],
            ),
            (
                SkinSex::Male,
                vec![
                    "CTRLEyeLidsBottomDown",
                    "CTRLEyeLidsBottomUp",
                    "CTRLEyeLidsTopDown",
                    "CTRLEyeLidsTopUp",
                ],
            ),
        ] {
            let mut found: Vec<_> = builtin_morph_names(sex)
                .filter(|name| vkit_core::vam::is_eyelid_look_control(name))
                .collect();
            found.sort();
            assert_eq!(found, expected, "{sex:?}");
        }
    }

    #[test]
    fn cheekbone_cache_code_is_appended_without_reassigning_legacy_codes() {
        assert_eq!(category_byte(MorphCategory::Cheeks), 6);
        assert_eq!(category_byte(MorphCategory::Head), 9);
        assert_eq!(category_byte(MorphCategory::Cheekbones), 10);
        assert_eq!(
            category_from_byte(6, LEGACY_VERSION).unwrap(),
            MorphCategory::Cheeks
        );
        assert_eq!(
            category_from_byte(10, VERSION).unwrap(),
            MorphCategory::Cheekbones
        );
        assert!(category_from_byte(10, LEGACY_VERSION).is_err());
    }

    #[test]
    fn legacy_v2_cache_is_rejected_before_metadata_free_rows_can_escape_filtering() {
        let geometry = geometry();
        let topology = topology_digest(geometry.vertices.len(), &geometry.faces).unwrap();
        let stamp = SourceStamp {
            bytes: 123,
            modified_ns: 456,
        };
        let bank = Path::new(r"C:\VaM\VaM_Data\StreamingAssets\f_mb");

        let legacy_entries = vec![CachedEntry {
            internal_name: "FHMHeadHeight".into(),
            label: "Head Height".into(),
            category: MorphCategory::Head,
            minimum: -1.0,
            maximum: 1.0,
            default: 0.0,
            unsupported_formula_count: 0,
            deltas: vec![(1, [0.0, 0.25, 0.0])],
        }];
        let mut encoded =
            encode_cache(bank, stamp, SkinSex::Female, topology, &legacy_entries).unwrap();
        encoded[MAGIC.len()..MAGIC.len() + 2].copy_from_slice(&LEGACY_VERSION.to_le_bytes());
        let temporary = std::env::temp_dir().join(format!(
            "vkit-morph-cache-legacy-v2-{}.fmmcache",
            std::process::id()
        ));
        fs::write(&temporary, encoded).unwrap();

        let error = read_cache(
            &temporary,
            Some(ExpectedSource {
                canonical_bank: bank,
                stamp,
            }),
            SkinSex::Female,
            topology,
            &geometry,
        )
        .unwrap_err();
        assert!(
            error.contains("lacks region/group identity metadata")
                && error.contains("must be rebuilt"),
            "{error}"
        );
        fs::remove_file(temporary).unwrap();
    }

    #[test]
    fn v3_header_distinguishes_the_appended_cheekbone_category() {
        let geometry = geometry();
        let topology = topology_digest(geometry.vertices.len(), &geometry.faces).unwrap();
        let stamp = SourceStamp {
            bytes: 123,
            modified_ns: 456,
        };
        let bank = Path::new(r"C:\VaM\VaM_Data\StreamingAssets\f_mb");
        let mut current_entries = entries();
        current_entries[0].category = MorphCategory::Cheekbones;
        let encoded =
            encode_cache(bank, stamp, SkinSex::Female, topology, &current_entries).unwrap();
        assert_eq!(
            u16::from_le_bytes(encoded[MAGIC.len()..MAGIC.len() + 2].try_into().unwrap()),
            VERSION
        );
        let temporary = std::env::temp_dir().join(format!(
            "vkit-morph-cache-v3-category10-{}.fmmcache",
            std::process::id()
        ));
        fs::write(&temporary, encoded).unwrap();

        let decoded = read_cache(
            &temporary,
            Some(ExpectedSource {
                canonical_bank: bank,
                stamp,
            }),
            SkinSex::Female,
            topology,
            &geometry,
        )
        .unwrap();
        assert_eq!(decoded.morphs.len(), 1);
        assert_eq!(decoded.morphs[0].category, MorphCategory::Cheekbones);
        fs::remove_file(temporary).unwrap();
    }

    #[test]
    fn sparse_cache_compaction_matches_the_previous_dense_round_trip() {
        let geometry = geometry();
        let resolved = ResolvedMorph {
            stable_id: "fixture".into(),
            internal_name: "PHMFixture".into(),
            label: "Fixture".into(),
            sparse_deltas: vec![
                SparseDelta {
                    vertex_index: 0,
                    delta_cm: [0.123_456_789, 0.0, 0.0],
                },
                SparseDelta {
                    vertex_index: 2,
                    delta_cm: [0.0, -0.25, 0.5],
                },
            ],
            vertex_count: geometry.vertices.len(),
            receipt: MorphResolutionReceipt::default(),
        };
        let compact =
            compact_resolved_deltas(&resolved, geometry.vertices.len(), -1.0, 1.0, 0.0).unwrap();

        let dense = resolved
            .to_morph_target(&geometry.faces, ZERO_TOLERANCE_CM, -1.0, 1.0, 0.0)
            .unwrap();
        let reference = dense
            .deltas
            .iter()
            .enumerate()
            .filter_map(|(vertex_id, delta)| {
                let compact = delta.map(|value| f64::from(value as f32));
                let norm = compact
                    .iter()
                    .map(|value| value * value)
                    .sum::<f64>()
                    .sqrt();
                (norm > ZERO_TOLERANCE_CM).then_some((vertex_id as u32, compact))
            })
            .collect::<Vec<_>>();
        assert_eq!(compact, reference);
    }

    #[test]
    fn cache_round_trip_is_topology_and_source_stamp_bound() {
        let geometry = geometry();
        let topology = topology_digest(geometry.vertices.len(), &geometry.faces).unwrap();
        let stamp = SourceStamp {
            bytes: 123,
            modified_ns: 456,
        };
        let bank = Path::new(r"C:\VaM\VaM_Data\StreamingAssets\f_mb");
        let encoded = encode_cache(bank, stamp, SkinSex::Female, topology, &entries()).unwrap();
        let temporary = std::env::temp_dir().join(format!(
            "vkit-morph-cache-roundtrip-{}.fmmcache",
            std::process::id()
        ));
        fs::write(&temporary, encoded).unwrap();
        let decoded = read_cache(
            &temporary,
            Some(ExpectedSource {
                canonical_bank: bank,
                stamp,
            }),
            SkinSex::Female,
            topology,
            &geometry,
        )
        .unwrap();
        assert_eq!(decoded.morphs.len(), 1);
        assert_eq!(decoded.morphs[0].internal_name, "PHMCacheTest");
        assert_eq!(decoded.morphs[0].unsupported_formula_count, 2);
        assert_eq!(
            decoded.morphs[0].deltas.as_slice(),
            &[(1, [0.0, 0.25, 0.0])]
        );
        assert!(
            read_cache(
                &temporary,
                Some(ExpectedSource {
                    canonical_bank: bank,
                    stamp: SourceStamp {
                        bytes: 124,
                        modified_ns: 456,
                    },
                }),
                SkinSex::Female,
                topology,
                &geometry,
            )
            .is_err()
        );
        fs::remove_file(temporary).unwrap();
    }

    #[test]
    fn corrupt_declared_size_is_rejected_before_lz4_allocation() {
        let geometry = geometry();
        let topology = topology_digest(geometry.vertices.len(), &geometry.faces).unwrap();
        let stamp = SourceStamp {
            bytes: 123,
            modified_ns: 456,
        };
        let bank = Path::new(r"C:\VaM\VaM_Data\StreamingAssets\f_mb");
        let mut encoded = encode_cache(bank, stamp, SkinSex::Female, topology, &entries()).unwrap();
        let compressed_offset = HEADER_BYTES + bank.to_string_lossy().len();
        encoded[compressed_offset..compressed_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let temporary = std::env::temp_dir().join(format!(
            "vkit-morph-cache-corrupt-{}.fmmcache",
            std::process::id()
        ));
        fs::write(&temporary, encoded).unwrap();
        assert!(
            read_cache(
                &temporary,
                Some(ExpectedSource {
                    canonical_bank: bank,
                    stamp,
                }),
                SkinSex::Female,
                topology,
                &geometry,
            )
            .is_err()
        );
        fs::remove_file(temporary).unwrap();
    }

    #[test]
    fn atomic_writer_replaces_an_existing_cache_without_sidecars() {
        let directory =
            std::env::temp_dir().join(format!("vkit-morph-cache-atomic-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("female.fmmcache");
        write_atomic(&path, b"old").unwrap();
        write_atomic(&path, b"new-cache").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new-cache");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn valid_cache_hit_never_parses_the_source_unity_bank() {
        let root_path =
            std::env::temp_dir().join(format!("vkit-morph-cache-hit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root_path);
        let bank = root_path
            .join("VaM_Data")
            .join("StreamingAssets")
            .join("f_mb");
        fs::create_dir_all(bank.parent().unwrap()).unwrap();

        fs::write(&bank, b"deliberately invalid Unity bundle").unwrap();
        let root = VaMRoot::open(&root_path).unwrap();
        let canonical_bank = fs::canonicalize(&bank).unwrap();
        let geometry = geometry();
        let topology = topology_digest(geometry.vertices.len(), &geometry.faces).unwrap();
        let stamp = source_stamp(&canonical_bank).unwrap();
        let cache_root = root_path.join("local-cache");
        let path = cache_path(&cache_root, &canonical_bank, SkinSex::Female);
        let encoded = encode_cache(
            &canonical_bank,
            stamp,
            SkinSex::Female,
            topology,
            &entries(),
        )
        .unwrap();
        write_atomic(&path, &encoded).unwrap();

        let result = load_or_build(&root, SkinSex::Female, &geometry, &cache_root).unwrap();
        assert_eq!(result.receipt.disposition, MorphCacheDisposition::Hit);
        assert_eq!(result.morphs.len(), 1);
        fs::remove_dir_all(root_path).unwrap();
    }

    #[test]
    fn installed_cache_rejects_wrong_sex_even_when_the_filename_is_renamed() {
        let base = std::env::temp_dir().join(format!(
            "vkit-installed-cache-wrong-sex-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let geometry = geometry();
        let topology = topology_digest(geometry.vertices.len(), &geometry.faces).unwrap();
        let path = base.join("morphs").join("g2f-renamed-male.fmmcache");
        let encoded = encode_cache(
            Path::new(r"C:\VaM\VaM_Data\StreamingAssets\m_mb"),
            SourceStamp {
                bytes: 321,
                modified_ns: 654,
            },
            SkinSex::Male,
            topology,
            &entries(),
        )
        .unwrap();
        write_atomic(&path, &encoded).unwrap();

        let error = read_cache(&path, None, SkinSex::Female, topology, &geometry).unwrap_err();
        assert!(error.contains("figure family"), "{error}");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn installed_cache_rejects_a_crc_mismatch() {
        let base =
            std::env::temp_dir().join(format!("vkit-installed-cache-crc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let geometry = geometry();
        let topology = topology_digest(geometry.vertices.len(), &geometry.faces).unwrap();
        let path = base.join("morphs").join("g2f-corrupt.fmmcache");
        let mut encoded = encode_cache(
            Path::new(r"C:\VaM\VaM_Data\StreamingAssets\f_mb"),
            SourceStamp {
                bytes: 123,
                modified_ns: 456,
            },
            SkinSex::Female,
            topology,
            &entries(),
        )
        .unwrap();
        encoded[HEADER_BYTES - 4..HEADER_BYTES].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        write_atomic(&path, &encoded).unwrap();

        let error = read_cache(&path, None, SkinSex::Female, topology, &geometry).unwrap_err();
        assert!(error.contains("CRC-32"), "{error}");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    #[ignore = "requires the user's VaM installation"]
    fn builtin_morphs_bind_to_the_discovered_bundle_anchor_for_both_sexes() {
        use vkit_core::vam::{GeometryBaseRequest, GeometrySex, discover_geometry_base};
        let vam_root = env::var_os("VKIT_VAM_ROOT").expect("set VKIT_VAM_ROOT");
        let root = VaMRoot::open(vam_root).unwrap();
        let cache_root = default_cache_root().expect("LOCALAPPDATA cache root");
        for (sex, skin) in [
            (GeometrySex::Female, SkinSex::Female),
            (GeometrySex::Male, SkinSex::Male),
        ] {
            let discovered = discover_geometry_base(GeometryBaseRequest {
                root: Some(&root),
                sex,
                licensed_anchor: None,
                explicit_candidates: &[],
                cache_dir: None,
            })
            .expect("tiered discovery must enroll from the real install");
            let result = load_or_build(&root, skin, discovered.provider.daz_anchor(), &cache_root)
                .unwrap_or_else(|error| {
                    panic!("{skin:?}: built-in morphs must bind to the bundle anchor: {error}")
                });
            assert!(
                result.morphs.len() > 100,
                "{skin:?}: unexpectedly small catalog"
            );
        }
    }
}
