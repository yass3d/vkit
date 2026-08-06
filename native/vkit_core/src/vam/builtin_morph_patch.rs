use std::collections::{BTreeMap, BTreeSet};

use super::unity_morph_bank::{
    ByteReader, Endian, Formula, FormulaTarget, align_up, bounded_i32, checked_slice,
    decompress_chunk, usize_from_u32, usize_from_u64,
};
use super::vmb::{
    VAM_SHARED_BODY_VERTEX_COUNT, decode_vmb_daz_cm_for_topology, vmb_raw_entry_count,
};
use super::{Result, VaMError};

const MAX_BLOCKS: usize = 100_000;
const MAX_NODES: usize = 100_000;
const MAX_OBJECTS: usize = 1_000_000;
const MAX_TYPES: usize = 100_000;
const MAX_MORPHS_PER_OBJECT: usize = 20_000;
const MAX_DELTAS_PER_MORPH: usize = 1_000_000;
const MAX_FORMULAS_PER_MORPH: usize = 100_000;
const UNITY_BLOCK_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct BuiltinMorphReplacement {
    pub internal_name: String,
    pub vmb_bytes: Vec<u8>,

    pub formula_replacement: Option<Vec<Formula>>,

    pub slider_range_replacement: Option<(f32, f32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuiltinMorphPatchEntry {
    pub internal_name: String,
    pub object_path_id: i64,
    pub morph_index: u32,
    pub original_delta_count: usize,
    pub replacement_delta_count: usize,
    pub original_formula_count: usize,
    pub replacement_formula_count: usize,
    pub original_slider_range: (f32, f32),
    pub replacement_slider_range: (f32, f32),
}

#[derive(Clone, Debug)]
pub struct BuiltinMorphPatchOutput {
    pub encoded_bank: Vec<u8>,
    pub entries: Vec<BuiltinMorphPatchEntry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuiltinMorphDigest {
    pub internal_name: String,
    pub delta_count: usize,
    pub delta_sha256: String,
    pub formula_count: usize,
    pub formula_sha256: String,
    pub slider_range: (f32, f32),
}

#[derive(Clone, Debug)]
struct UnityArchive {
    format_version: u32,
    unity_version: String,
    unity_revision: String,
    flags: u32,
    data_area: Vec<u8>,
    nodes: Vec<ArchiveNode>,
}

#[derive(Clone, Debug)]
struct ArchiveNode {
    offset: usize,
    size: usize,
    flags: u32,
    path: String,
}

#[derive(Clone, Copy, Debug)]
struct ArchiveBlock {
    uncompressed_size: usize,
    compressed_size: usize,
    flags: u16,
}

#[derive(Clone, Debug)]
struct SerializedObject {
    path_id: i64,
    offset_field: usize,
    size_field: usize,
    start: usize,
    size: usize,
    class_id: Option<i32>,
}

#[derive(Clone, Debug)]
struct SerializedAssetLayout {
    endian: Endian,
    data_offset: usize,
    objects: Vec<SerializedObject>,
}

#[derive(Clone, Debug)]
struct MorphLayout {
    morph_index: u32,
    internal_name: String,
    minimum_field: usize,
    maximum_field: usize,
    minimum: f32,
    maximum: f32,
    declared_count_field: usize,
    delta_count_field: usize,
    deltas_start: usize,
    deltas_end: usize,
    delta_count: usize,
    formula_count_field: usize,
    formulas_start: usize,
    formulas_end: usize,
    formula_count: usize,
}

#[derive(Clone, Debug)]
struct ByteEdit {
    start: usize,
    end: usize,
    bytes: Vec<u8>,
}

pub fn patch_builtin_morph_bank(
    encoded_bank: &[u8],
    replacements: &[BuiltinMorphReplacement],
) -> Result<BuiltinMorphPatchOutput> {
    if replacements.is_empty() {
        return Err(patch_error("no built-in morph replacements were supplied"));
    }
    let mut replacement_map = BTreeMap::new();
    for replacement in replacements {
        if replacement.internal_name.trim().is_empty() {
            return Err(patch_error("replacement internal name is empty"));
        }
        if replacement_map
            .insert(replacement.internal_name.as_str(), replacement)
            .is_some()
        {
            return Err(patch_error(format!(
                "replacement {:?} is declared more than once",
                replacement.internal_name
            )));
        }
        let raw_count = vmb_raw_entry_count(&replacement.vmb_bytes)?;
        let normalized = decode_vmb_daz_cm_for_topology(
            &replacement.vmb_bytes,
            VAM_SHARED_BODY_VERTEX_COUNT as usize,
            None,
        )?;
        if normalized.len() != raw_count {
            return Err(patch_error(format!(
                "replacement {:?} contains exact-zero or duplicate rows",
                replacement.internal_name
            )));
        }
        if let Some((minimum, maximum)) = replacement.slider_range_replacement
            && (!minimum.is_finite() || !maximum.is_finite() || minimum > maximum)
        {
            return Err(patch_error(format!(
                "replacement {:?} has invalid slider range [{minimum}, {maximum}]",
                replacement.internal_name
            )));
        }
    }

    let mut archive = decode_archive(encoded_bank)?;
    let original_hash = md5_digest(&archive.data_area);
    let mut patched_names = BTreeSet::new();
    let mut receipts = Vec::new();
    let mut rebuilt_data = Vec::with_capacity(archive.data_area.len());
    let mut old_cursor = 0_usize;
    let mut node_order = (0..archive.nodes.len()).collect::<Vec<_>>();
    node_order.sort_unstable_by_key(|&index| archive.nodes[index].offset);
    for node_index in node_order {
        let original_offset = archive.nodes[node_index].offset;
        let original_size = archive.nodes[node_index].size;
        if original_offset < old_cursor {
            return Err(patch_error("UnityFS nodes overlap"));
        }
        rebuilt_data.extend_from_slice(checked_slice(
            &archive.data_area,
            old_cursor,
            original_offset - old_cursor,
        )?);
        let original_node = checked_slice(&archive.data_area, original_offset, original_size)?;
        let (patched_node, mut node_receipts) =
            patch_serialized_asset(original_node, &replacement_map, &mut patched_names)?;
        archive.nodes[node_index].offset = rebuilt_data.len();
        archive.nodes[node_index].size = patched_node.len();
        rebuilt_data.extend_from_slice(&patched_node);
        receipts.append(&mut node_receipts);
        old_cursor = original_offset
            .checked_add(original_size)
            .ok_or_else(|| patch_error("UnityFS node end overflows"))?;
    }
    rebuilt_data.extend_from_slice(
        archive
            .data_area
            .get(old_cursor..)
            .ok_or_else(|| patch_error("UnityFS trailing-data offset exceeds input"))?,
    );

    let missing = replacement_map
        .keys()
        .filter(|name| !patched_names.contains(**name))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(patch_error(format!(
            "replacement targets were not found exactly once: {}",
            missing
                .iter()
                .map(|name| format!("{name:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    receipts.sort_by(|left, right| left.internal_name.cmp(&right.internal_name));
    archive.data_area = rebuilt_data;
    let encoded_bank = encode_archive(&archive)?;

    let verification = decode_archive(&encoded_bank)?;
    if verification.data_area == archive.data_area
        && md5_digest(&verification.data_area) != original_hash
    {
        verify_patch_receipts(&verification, &receipts)?;
        Ok(BuiltinMorphPatchOutput {
            encoded_bank,
            entries: receipts,
        })
    } else if verification.data_area != archive.data_area {
        Err(patch_error(
            "rebuilt UnityFS data differs after a decode round trip",
        ))
    } else {
        Err(patch_error(
            "replacement produced the original UnityFS data unchanged",
        ))
    }
}

pub fn read_builtin_morph_digests(encoded_bank: &[u8]) -> Result<Vec<BuiltinMorphDigest>> {
    let archive = decode_archive(encoded_bank)?;
    let mut digests = Vec::new();
    visit_bank_records(&archive, |_object, payload, morph| {
        let deltas = checked_slice(
            payload,
            morph.deltas_start,
            morph
                .deltas_end
                .checked_sub(morph.deltas_start)
                .ok_or_else(|| patch_error("morph delta range runs backwards"))?,
        )?;
        let formulas = checked_slice(
            payload,
            morph.formulas_start,
            morph
                .formulas_end
                .checked_sub(morph.formulas_start)
                .ok_or_else(|| patch_error("morph formula range runs backwards"))?,
        )?;
        digests.push(BuiltinMorphDigest {
            internal_name: morph.internal_name,
            delta_count: morph.delta_count,
            delta_sha256: super::sha256_hex(deltas),
            formula_count: morph.formula_count,
            formula_sha256: super::sha256_hex(formulas),
            slider_range: (morph.minimum, morph.maximum),
        });
        Ok(())
    })?;
    Ok(digests)
}

fn visit_bank_records(
    archive: &UnityArchive,
    mut visit: impl FnMut(&SerializedObject, &[u8], MorphLayout) -> Result<()>,
) -> Result<()> {
    for node in &archive.nodes {
        let cab = checked_slice(&archive.data_area, node.offset, node.size)?;
        let Ok(layout) = parse_serialized_asset_layout(cab) else {
            continue;
        };
        for object in &layout.objects {
            if object.class_id != Some(114) {
                continue;
            }
            let payload = checked_slice(cab, object.start, object.size)?;
            let Ok(morphs) = scan_bank_object(payload, layout.endian) else {
                continue;
            };
            for morph in morphs {
                visit(object, payload, morph)?;
            }
        }
    }
    Ok(())
}

fn verify_patch_receipts(
    archive: &UnityArchive,
    receipts: &[BuiltinMorphPatchEntry],
) -> Result<()> {
    let expected = receipts
        .iter()
        .map(|entry| {
            (
                (entry.object_path_id, entry.morph_index),
                (
                    entry.internal_name.as_str(),
                    entry.replacement_delta_count,
                    entry.replacement_formula_count,
                    entry.replacement_slider_range,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut observed = BTreeMap::new();
    visit_bank_records(archive, |object, _payload, morph| {
        let key = (object.path_id, morph.morph_index);
        if expected.contains_key(&key)
            && observed
                .insert(
                    key,
                    (
                        morph.internal_name,
                        morph.delta_count,
                        morph.formula_count,
                        (morph.minimum, morph.maximum),
                    ),
                )
                .is_some()
        {
            return Err(patch_error("patched morph record occurs more than once"));
        }
        Ok(())
    })?;
    for (key, (expected_name, expected_count, expected_formula_count, expected_slider_range)) in
        expected
    {
        let Some((observed_name, observed_count, observed_formula_count, observed_slider_range)) =
            observed.get(&key)
        else {
            return Err(patch_error(format!(
                "patched morph {expected_name:?} is absent after rebuilding"
            )));
        };
        if observed_name != expected_name
            || *observed_count != expected_count
            || *observed_formula_count != expected_formula_count
            || *observed_slider_range != expected_slider_range
        {
            return Err(patch_error(format!(
                "patched morph {expected_name:?} failed verification: got name {observed_name:?}, deltas {observed_count}, formulas {observed_formula_count}, range {observed_slider_range:?}; expected deltas {expected_count}, formulas {expected_formula_count}, range {expected_slider_range:?}"
            )));
        }
    }
    Ok(())
}

fn patch_serialized_asset(
    cab: &[u8],
    replacements: &BTreeMap<&str, &BuiltinMorphReplacement>,
    patched_names: &mut BTreeSet<String>,
) -> Result<(Vec<u8>, Vec<BuiltinMorphPatchEntry>)> {
    let Ok(layout) = parse_serialized_asset_layout(cab) else {
        return Ok((cab.to_vec(), Vec::new()));
    };
    let mut object_payloads = BTreeMap::<usize, Vec<u8>>::new();
    let mut receipts = Vec::new();
    for (object_index, object) in layout.objects.iter().enumerate() {
        if object.class_id != Some(114) {
            continue;
        }
        let payload = checked_slice(cab, object.start, object.size)?;
        let Ok(morphs) = scan_bank_object(payload, layout.endian) else {
            continue;
        };
        let mut edits = Vec::new();
        for morph in morphs {
            let Some(replacement) = replacements.get(morph.internal_name.as_str()).copied() else {
                continue;
            };
            let vmb = replacement.vmb_bytes.as_slice();
            if !patched_names.insert(morph.internal_name.clone()) {
                return Err(patch_error(format!(
                    "built-in morph {:?} occurs more than once",
                    morph.internal_name
                )));
            }
            if layout.endian != Endian::Little {
                return Err(patch_error(format!(
                    "built-in morph {:?} uses unsupported big-endian delta rows",
                    morph.internal_name
                )));
            }
            let replacement_count = vmb_raw_entry_count(vmb)?;
            let count_bytes = i32::try_from(replacement_count)
                .map_err(|_| patch_error("replacement delta count exceeds i32"))?
                .to_le_bytes()
                .to_vec();
            edits.push(ByteEdit {
                start: morph.declared_count_field,
                end: morph.declared_count_field + 4,
                bytes: count_bytes.clone(),
            });
            edits.push(ByteEdit {
                start: morph.delta_count_field,
                end: morph.delta_count_field + 4,
                bytes: count_bytes,
            });
            edits.push(ByteEdit {
                start: morph.deltas_start,
                end: morph.deltas_end,
                bytes: vmb[4..].to_vec(),
            });
            let replacement_slider_range =
                if let Some((minimum, maximum)) = replacement.slider_range_replacement {
                    edits.push(ByteEdit {
                        start: morph.minimum_field,
                        end: morph.minimum_field + 4,
                        bytes: encode_f32(minimum, layout.endian).to_vec(),
                    });
                    edits.push(ByteEdit {
                        start: morph.maximum_field,
                        end: morph.maximum_field + 4,
                        bytes: encode_f32(maximum, layout.endian).to_vec(),
                    });
                    (minimum, maximum)
                } else {
                    (morph.minimum, morph.maximum)
                };
            let replacement_formula_count = if let Some(formulas) = &replacement.formula_replacement
            {
                let count = i32::try_from(formulas.len())
                    .map_err(|_| patch_error("replacement formula count exceeds i32"))?;
                edits.push(ByteEdit {
                    start: morph.formula_count_field,
                    end: morph.formula_count_field + 4,
                    bytes: encode_i32(count, layout.endian).to_vec(),
                });
                edits.push(ByteEdit {
                    start: morph.formulas_start,
                    end: morph.formulas_end,
                    bytes: encode_formulas(formulas, layout.endian)?,
                });
                formulas.len()
            } else {
                morph.formula_count
            };
            receipts.push(BuiltinMorphPatchEntry {
                internal_name: morph.internal_name,
                object_path_id: object.path_id,
                morph_index: morph.morph_index,
                original_delta_count: morph.delta_count,
                replacement_delta_count: replacement_count,
                original_formula_count: morph.formula_count,
                replacement_formula_count,
                original_slider_range: (morph.minimum, morph.maximum),
                replacement_slider_range,
            });
        }
        if !edits.is_empty() {
            object_payloads.insert(object_index, apply_byte_edits(payload, edits)?);
        }
    }
    if object_payloads.is_empty() {
        return Ok((cab.to_vec(), receipts));
    }

    let mut order = (0..layout.objects.len()).collect::<Vec<_>>();
    order.sort_unstable_by_key(|&index| layout.objects[index].start);
    let mut output = cab[..layout.data_offset].to_vec();
    let mut old_cursor = layout.data_offset;
    for object_index in order {
        let object = &layout.objects[object_index];
        if object.start < old_cursor {
            return Err(patch_error("serialized objects overlap"));
        }
        output.extend_from_slice(checked_slice(cab, old_cursor, object.start - old_cursor)?);
        let new_start = output.len();
        let payload = object_payloads
            .get(&object_index)
            .map(Vec::as_slice)
            .unwrap_or(checked_slice(cab, object.start, object.size)?);
        output.extend_from_slice(payload);
        write_u32_at(
            &mut output,
            object.offset_field,
            u32::try_from(new_start - layout.data_offset)
                .map_err(|_| patch_error("serialized object offset exceeds u32"))?,
            layout.endian,
        )?;
        write_u32_at(
            &mut output,
            object.size_field,
            u32::try_from(payload.len())
                .map_err(|_| patch_error("serialized object size exceeds u32"))?,
            layout.endian,
        )?;
        old_cursor = object
            .start
            .checked_add(object.size)
            .ok_or_else(|| patch_error("serialized object end overflows"))?;
    }
    output.extend_from_slice(
        cab.get(old_cursor..)
            .ok_or_else(|| patch_error("serialized trailing-data offset exceeds input"))?,
    );
    let output_size =
        u32::try_from(output.len()).map_err(|_| patch_error("serialized asset exceeds u32"))?;
    write_u32_at(&mut output, 4, output_size, Endian::Big)?;
    Ok((output, receipts))
}

fn parse_serialized_asset_layout(cab: &[u8]) -> Result<SerializedAssetLayout> {
    if cab.len() < 20 {
        return Err(patch_error("serialized asset header is truncated"));
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
    if file_size != cab.len()
        || !(14..=21).contains(&version)
        || metadata_size > data_offset
        || data_offset > cab.len()
    {
        return Err(patch_error("serialized asset header is inconsistent"));
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
            reader.take(
                node_count
                    .checked_mul(node_bytes)
                    .ok_or_else(|| patch_error("type-tree byte count overflows"))?,
            )?;
            reader.take(string_bytes)?;
        }
        class_ids.push(class_id);
    }

    let object_count = bounded_i32(reader.i32()?, MAX_OBJECTS, "serialized object")?;
    let mut objects = Vec::with_capacity(object_count);
    for _ in 0..object_count {
        reader.align(4)?;
        let path_id = reader.i64()?;
        let offset_field = reader.position();
        let object_offset = usize_from_u32(reader.u32()?)?;
        let size_field = reader.position();
        let size = usize_from_u32(reader.u32()?)?;
        let type_index = reader.i32()?;
        let class_id = usize::try_from(type_index)
            .ok()
            .and_then(|index| class_ids.get(index))
            .copied();
        let start = data_offset
            .checked_add(object_offset)
            .ok_or_else(|| patch_error("serialized object offset overflows"))?;
        checked_slice(cab, start, size)?;
        objects.push(SerializedObject {
            path_id,
            offset_field,
            size_field,
            start,
            size,
            class_id,
        });
    }
    Ok(SerializedAssetLayout {
        endian,
        data_offset,
        objects,
    })
}

fn scan_bank_object(payload: &[u8], endian: Endian) -> Result<Vec<MorphLayout>> {
    let mut reader = ByteReader::new(payload, endian);
    reader.take(12)?;
    reader.aligned_flag()?;
    reader.take(12)?;
    reader.aligned_text()?;
    reader.aligned_flag()?;
    let morph_count = bounded_i32(reader.i32()?, MAX_MORPHS_PER_OBJECT, "bank morph")?;
    let mut layouts = Vec::with_capacity(morph_count);
    for morph_index in 0..morph_count {
        reader.aligned_flag()?;
        reader.aligned_flag()?;
        reader.aligned_flag()?;
        reader.aligned_flag()?;
        let internal_name = reader.aligned_text()?;
        for _ in 0..5 {
            reader.aligned_text()?;
        }
        reader.f32()?;
        reader.f32()?;
        reader.f32()?;
        reader.aligned_flag()?;
        reader.f32()?;
        let minimum_field = reader.position();
        let minimum = reader.f32()?;
        let maximum_field = reader.position();
        let maximum = reader.f32()?;
        if !minimum.is_finite() || !maximum.is_finite() || minimum > maximum {
            return Err(patch_error(format!(
                "morph {internal_name:?} has invalid slider range [{minimum}, {maximum}]"
            )));
        }
        let declared_count_field = reader.position();
        let declared_count = reader.i32()?;
        reader.aligned_flag()?;
        reader.aligned_flag()?;
        let delta_count_field = reader.position();
        let delta_count = bounded_i32(reader.i32()?, MAX_DELTAS_PER_MORPH, "morph delta")?;
        let deltas_start = reader.position();
        reader.take(
            delta_count
                .checked_mul(16)
                .ok_or_else(|| patch_error("morph delta byte count overflows"))?,
        )?;
        let deltas_end = reader.position();
        let formula_count_field = reader.position();
        let formula_count = bounded_i32(reader.i32()?, MAX_FORMULAS_PER_MORPH, "morph formula")?;
        let formulas_start = reader.position();
        for _ in 0..formula_count {
            reader.i32()?;
            reader.aligned_text()?;
            reader.f32()?;
        }
        let formulas_end = reader.position();
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
        if !internal_name.is_empty() && declared_count == delta_count as i32 {
            layouts.push(MorphLayout {
                morph_index: morph_index as u32,
                internal_name,
                minimum_field,
                maximum_field,
                minimum,
                maximum,
                declared_count_field,
                delta_count_field,
                deltas_start,
                deltas_end,
                delta_count,
                formula_count_field,
                formulas_start,
                formulas_end,
                formula_count,
            });
        }
    }
    Ok(layouts)
}

fn apply_byte_edits(bytes: &[u8], mut edits: Vec<ByteEdit>) -> Result<Vec<u8>> {
    edits.sort_unstable_by_key(|edit| edit.start);
    let mut output = Vec::new();
    let mut cursor = 0_usize;
    for edit in edits {
        if edit.start < cursor || edit.end < edit.start || edit.end > bytes.len() {
            return Err(patch_error(
                "morph byte edits overlap or exceed the payload",
            ));
        }
        output.extend_from_slice(&bytes[cursor..edit.start]);
        output.extend_from_slice(&edit.bytes);
        cursor = edit.end;
    }
    output.extend_from_slice(&bytes[cursor..]);
    Ok(output)
}

fn encode_formulas(formulas: &[Formula], endian: Endian) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    for formula in formulas {
        output.extend_from_slice(&encode_i32(
            formula_target_code(&formula.target_type),
            endian,
        ));
        let target = formula.target.as_bytes();
        let target_len =
            i32::try_from(target.len()).map_err(|_| patch_error("formula target is too long"))?;
        output.extend_from_slice(&encode_i32(target_len, endian));
        output.extend_from_slice(target);
        output.resize(align_up(output.len(), 4)?, 0);
        let multiplier = formula.multiplier as f32;
        if !multiplier.is_finite() {
            return Err(patch_error("formula multiplier is not a finite f32"));
        }
        output.extend_from_slice(&match endian {
            Endian::Little => multiplier.to_le_bytes(),
            Endian::Big => multiplier.to_be_bytes(),
        });
    }
    Ok(output)
}

fn formula_target_code(target: &FormulaTarget) -> i32 {
    match target {
        FormulaTarget::MorphValue => 0,
        FormulaTarget::BoneCenterX => 1,
        FormulaTarget::BoneCenterY => 2,
        FormulaTarget::BoneCenterZ => 3,
        FormulaTarget::OrientationX => 4,
        FormulaTarget::OrientationY => 5,
        FormulaTarget::OrientationZ => 6,
        FormulaTarget::GeneralScale => 7,
        FormulaTarget::ScaleX => 8,
        FormulaTarget::ScaleY => 9,
        FormulaTarget::ScaleZ => 10,
        FormulaTarget::Mcm => 11,
        FormulaTarget::McmMultiplier => 12,
        FormulaTarget::RotationX => 13,
        FormulaTarget::RotationY => 14,
        FormulaTarget::RotationZ => 15,
        FormulaTarget::Unknown(value) => *value,
    }
}

fn encode_i32(value: i32, endian: Endian) -> [u8; 4] {
    match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    }
}

fn encode_f32(value: f32, endian: Endian) -> [u8; 4] {
    match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    }
}

fn decode_archive(encoded: &[u8]) -> Result<UnityArchive> {
    let mut header = ByteReader::new(encoded, Endian::Big);
    if header.null_text()? != "UnityFS" {
        return Err(patch_error("built-in bank is not a UnityFS bundle"));
    }
    let format_version = header.u32()?;
    if format_version < 6 {
        return Err(patch_error(format!(
            "UnityFS version {format_version} is unsupported"
        )));
    }
    let unity_version = header.null_text()?;
    let unity_revision = header.null_text()?;
    let declared_size = usize_from_u64(header.u64()?)?;
    if declared_size != encoded.len() {
        return Err(patch_error(
            "UnityFS declared size does not match its bytes",
        ));
    }
    let packed_directory_size = usize_from_u32(header.u32()?)?;
    let directory_size = usize_from_u32(header.u32()?)?;
    let flags = header.u32()?;
    let header_end = header.position();
    let directory_offset = if flags & 0x80 != 0 {
        encoded
            .len()
            .checked_sub(packed_directory_size)
            .ok_or_else(|| patch_error("UnityFS directory offset underflows"))?
    } else {
        header_end
    };
    let packed_directory = checked_slice(encoded, directory_offset, packed_directory_size)?;
    let directory = decompress_chunk(packed_directory, flags & 0x3f, directory_size)?;
    let (expected_hash, blocks, nodes) = parse_archive_directory(&directory)?;

    let mut cursor = if flags & 0x80 != 0 {
        header_end
    } else {
        header_end
            .checked_add(packed_directory_size)
            .ok_or_else(|| patch_error("UnityFS data offset overflows"))?
    };
    if flags & 0x200 != 0 {
        cursor = align_up(cursor, 16)?;
    }
    let total_size = blocks.iter().try_fold(0_usize, |total, block| {
        total
            .checked_add(block.uncompressed_size)
            .ok_or_else(|| patch_error("UnityFS decompressed size overflows"))
    })?;
    let mut data_area = Vec::with_capacity(total_size);
    for block in blocks {
        let compressed = checked_slice(encoded, cursor, block.compressed_size)?;
        cursor = cursor
            .checked_add(block.compressed_size)
            .ok_or_else(|| patch_error("UnityFS block cursor overflows"))?;
        data_area.extend_from_slice(&decompress_chunk(
            compressed,
            u32::from(block.flags & 0x3f),
            block.uncompressed_size,
        )?);
    }
    if expected_hash != [0; 16] && md5_digest(&data_area) != expected_hash {
        return Err(patch_error(
            "UnityFS uncompressed-data MD5 does not match the directory",
        ));
    }
    Ok(UnityArchive {
        format_version,
        unity_version,
        unity_revision,
        flags,
        data_area,
        nodes,
    })
}

fn parse_archive_directory(
    directory: &[u8],
) -> Result<([u8; 16], Vec<ArchiveBlock>, Vec<ArchiveNode>)> {
    let mut reader = ByteReader::new(directory, Endian::Big);
    let mut hash = [0_u8; 16];
    hash.copy_from_slice(reader.take(16)?);
    let block_count = usize_from_u32(reader.u32()?)?;
    if block_count > MAX_BLOCKS {
        return Err(patch_error("UnityFS declares too many blocks"));
    }
    let mut blocks = Vec::with_capacity(block_count);
    for _ in 0..block_count {
        blocks.push(ArchiveBlock {
            uncompressed_size: usize_from_u32(reader.u32()?)?,
            compressed_size: usize_from_u32(reader.u32()?)?,
            flags: reader.u16()?,
        });
    }
    let node_count = usize_from_u32(reader.u32()?)?;
    if node_count > MAX_NODES {
        return Err(patch_error("UnityFS declares too many nodes"));
    }
    let mut nodes = Vec::with_capacity(node_count);
    for _ in 0..node_count {
        nodes.push(ArchiveNode {
            offset: usize_from_u64(reader.u64()?)?,
            size: usize_from_u64(reader.u64()?)?,
            flags: reader.u32()?,
            path: reader.null_text()?,
        });
    }
    Ok((hash, blocks, nodes))
}

fn encode_archive(archive: &UnityArchive) -> Result<Vec<u8>> {
    let mut compressed_blocks = Vec::new();
    let mut blocks = Vec::new();
    for chunk in archive.data_area.chunks(UNITY_BLOCK_BYTES) {
        let compressed = lz4_flex::block::compress(chunk);
        if compressed.len() < chunk.len() {
            blocks.push(ArchiveBlock {
                uncompressed_size: chunk.len(),
                compressed_size: compressed.len(),
                flags: 3,
            });
            compressed_blocks.push(compressed);
        } else {
            blocks.push(ArchiveBlock {
                uncompressed_size: chunk.len(),
                compressed_size: chunk.len(),
                flags: 0,
            });
            compressed_blocks.push(chunk.to_vec());
        }
    }
    let directory = encode_archive_directory(&archive.data_area, &blocks, &archive.nodes)?;
    let packed_directory = lz4_flex::block::compress(&directory);
    let directory_compression = if packed_directory.len() < directory.len() {
        3
    } else {
        0
    };
    let packed_directory = if directory_compression == 0 {
        directory.clone()
    } else {
        packed_directory
    };
    let flags = (archive.flags & !0x3f) | directory_compression;

    let mut output = Vec::new();
    output.extend_from_slice(b"UnityFS\0");
    push_u32(&mut output, archive.format_version, Endian::Big);
    output.extend_from_slice(archive.unity_version.as_bytes());
    output.push(0);
    output.extend_from_slice(archive.unity_revision.as_bytes());
    output.push(0);
    let size_field = output.len();
    push_u64(&mut output, 0, Endian::Big);
    push_u32(
        &mut output,
        u32::try_from(packed_directory.len())
            .map_err(|_| patch_error("UnityFS directory exceeds u32"))?,
        Endian::Big,
    );
    push_u32(
        &mut output,
        u32::try_from(directory.len()).map_err(|_| patch_error("UnityFS directory exceeds u32"))?,
        Endian::Big,
    );
    push_u32(&mut output, flags, Endian::Big);

    if flags & 0x80 == 0 {
        output.extend_from_slice(&packed_directory);
    }
    if flags & 0x200 != 0 {
        let aligned = align_up(output.len(), 16)?;
        output.resize(aligned, 0);
    }
    for block in compressed_blocks {
        output.extend_from_slice(&block);
    }
    if flags & 0x80 != 0 {
        output.extend_from_slice(&packed_directory);
    }
    let output_size =
        u64::try_from(output.len()).map_err(|_| patch_error("UnityFS size exceeds u64"))?;
    write_u64_at(&mut output, size_field, output_size, Endian::Big)?;
    Ok(output)
}

fn encode_archive_directory(
    data_area: &[u8],
    blocks: &[ArchiveBlock],
    nodes: &[ArchiveNode],
) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    output.extend_from_slice(&md5_digest(data_area));
    push_u32(
        &mut output,
        u32::try_from(blocks.len()).map_err(|_| patch_error("too many UnityFS blocks"))?,
        Endian::Big,
    );
    for block in blocks {
        push_u32(
            &mut output,
            u32::try_from(block.uncompressed_size)
                .map_err(|_| patch_error("UnityFS block size exceeds u32"))?,
            Endian::Big,
        );
        push_u32(
            &mut output,
            u32::try_from(block.compressed_size)
                .map_err(|_| patch_error("UnityFS block size exceeds u32"))?,
            Endian::Big,
        );
        push_u16(&mut output, block.flags, Endian::Big);
    }
    push_u32(
        &mut output,
        u32::try_from(nodes.len()).map_err(|_| patch_error("too many UnityFS nodes"))?,
        Endian::Big,
    );
    for node in nodes {
        push_u64(
            &mut output,
            u64::try_from(node.offset).map_err(|_| patch_error("node offset exceeds u64"))?,
            Endian::Big,
        );
        push_u64(
            &mut output,
            u64::try_from(node.size).map_err(|_| patch_error("node size exceeds u64"))?,
            Endian::Big,
        );
        push_u32(&mut output, node.flags, Endian::Big);
        output.extend_from_slice(node.path.as_bytes());
        output.push(0);
    }
    Ok(output)
}

fn write_u32_at(bytes: &mut [u8], offset: usize, value: u32, endian: Endian) -> Result<()> {
    let encoded = match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    };
    let destination = bytes
        .get_mut(offset..offset + 4)
        .ok_or_else(|| patch_error("u32 patch offset exceeds output"))?;
    destination.copy_from_slice(&encoded);
    Ok(())
}

fn write_u64_at(bytes: &mut [u8], offset: usize, value: u64, endian: Endian) -> Result<()> {
    let encoded = match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    };
    let destination = bytes
        .get_mut(offset..offset + 8)
        .ok_or_else(|| patch_error("u64 patch offset exceeds output"))?;
    destination.copy_from_slice(&encoded);
    Ok(())
}

fn push_u16(output: &mut Vec<u8>, value: u16, endian: Endian) {
    let encoded = match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    };
    output.extend_from_slice(&encoded);
}

fn push_u32(output: &mut Vec<u8>, value: u32, endian: Endian) {
    let encoded = match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    };
    output.extend_from_slice(&encoded);
}

fn push_u64(output: &mut Vec<u8>, value: u64, endian: Endian) {
    let encoded = match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    };
    output.extend_from_slice(&encoded);
}

fn md5_digest(bytes: &[u8]) -> [u8; 16] {
    const SHIFTS: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const CONSTANTS: [u32; 64] = [
        0xd76a_a478,
        0xe8c7_b756,
        0x2420_70db,
        0xc1bd_ceee,
        0xf57c_0faf,
        0x4787_c62a,
        0xa830_4613,
        0xfd46_9501,
        0x6980_98d8,
        0x8b44_f7af,
        0xffff_5bb1,
        0x895c_d7be,
        0x6b90_1122,
        0xfd98_7193,
        0xa679_438e,
        0x49b4_0821,
        0xf61e_2562,
        0xc040_b340,
        0x265e_5a51,
        0xe9b6_c7aa,
        0xd62f_105d,
        0x0244_1453,
        0xd8a1_e681,
        0xe7d3_fbc8,
        0x21e1_cde6,
        0xc337_07d6,
        0xf4d5_0d87,
        0x455a_14ed,
        0xa9e3_e905,
        0xfcef_a3f8,
        0x676f_02d9,
        0x8d2a_4c8a,
        0xfffa_3942,
        0x8771_f681,
        0x6d9d_6122,
        0xfde5_380c,
        0xa4be_ea44,
        0x4bde_cfa9,
        0xf6bb_4b60,
        0xbebf_bc70,
        0x289b_7ec6,
        0xeaa1_27fa,
        0xd4ef_3085,
        0x0488_1d05,
        0xd9d4_d039,
        0xe6db_99e5,
        0x1fa2_7cf8,
        0xc4ac_5665,
        0xf429_2244,
        0x432a_ff97,
        0xab94_23a7,
        0xfc93_a039,
        0x655b_59c3,
        0x8f0c_cc92,
        0xffef_f47d,
        0x8584_5dd1,
        0x6fa8_7e4f,
        0xfe2c_e6e0,
        0xa301_4314,
        0x4e08_11a1,
        0xf753_7e82,
        0xbd3a_f235,
        0x2ad7_d2bb,
        0xeb86_d391,
    ];

    let bit_length = (bytes.len() as u64).wrapping_mul(8);
    let mut padded = bytes.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_length.to_le_bytes());

    let mut state = [0x6745_2301_u32, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    for block in padded.chunks_exact(64) {
        let mut words = [0_u32; 16];
        for (index, word) in words.iter_mut().enumerate() {
            *word = u32::from_le_bytes(
                block[index * 4..index * 4 + 4]
                    .try_into()
                    .expect("four bytes"),
            );
        }
        let [mut a, mut b, mut c, mut d] = state;
        for index in 0..64 {
            let (function, word_index) = match index {
                0..=15 => ((b & c) | (!b & d), index),
                16..=31 => ((d & b) | (!d & c), (5 * index + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * index + 5) % 16),
                _ => (c ^ (b | !d), (7 * index) % 16),
            };
            let next = a
                .wrapping_add(function)
                .wrapping_add(CONSTANTS[index])
                .wrapping_add(words[word_index])
                .rotate_left(SHIFTS[index])
                .wrapping_add(b);
            a = d;
            d = c;
            c = b;
            b = next;
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }
    let mut output = [0_u8; 16];
    for (index, value) in state.iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    output
}

fn patch_error(message: impl Into<String>) -> VaMError {
    VaMError::InvalidMorphBank(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_standard_vectors() {
        assert_eq!(
            md5_digest(b""),
            [
                0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04, 0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8,
                0x42, 0x7e,
            ]
        );
        assert_eq!(
            md5_digest(b"abc"),
            [
                0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0, 0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1,
                0x7f, 0x72,
            ]
        );
    }

    #[test]
    fn unity_archive_round_trip_preserves_data_and_nodes() {
        let data_area = (0..(UNITY_BLOCK_BYTES + 257))
            .map(|index| (index % 251) as u8)
            .collect::<Vec<_>>();
        let archive = UnityArchive {
            format_version: 6,
            unity_version: "5.x.x".to_owned(),
            unity_revision: "2018.1.9f1".to_owned(),
            flags: 0x43,
            data_area: data_area.clone(),
            nodes: vec![
                ArchiveNode {
                    offset: 0,
                    size: 1024,
                    flags: 4,
                    path: "CAB-test".to_owned(),
                },
                ArchiveNode {
                    offset: 1024,
                    size: data_area.len() - 1024,
                    flags: 0,
                    path: "resource".to_owned(),
                },
            ],
        };
        let encoded = encode_archive(&archive).unwrap();
        let decoded = decode_archive(&encoded).unwrap();
        assert_eq!(decoded.format_version, archive.format_version);
        assert_eq!(decoded.unity_version, archive.unity_version);
        assert_eq!(decoded.unity_revision, archive.unity_revision);
        assert_eq!(decoded.data_area, data_area);
        assert_eq!(decoded.nodes.len(), 2);
        assert_eq!(decoded.nodes[0].path, "CAB-test");
        assert_eq!(decoded.nodes[1].offset, 1024);
        assert_eq!(decoded.nodes[1].size, archive.nodes[1].size);
    }

    #[test]
    fn formula_encoding_matches_bank_wire_contract() {
        let encoded = encode_formulas(
            &[Formula {
                target_type: FormulaTarget::RotationX,
                target: "lowerJaw".to_owned(),
                multiplier: 30.0,
            }],
            Endian::Little,
        )
        .unwrap();
        let mut reader = ByteReader::new(&encoded, Endian::Little);
        assert_eq!(reader.i32().unwrap(), 13);
        assert_eq!(reader.aligned_text().unwrap(), "lowerJaw");
        assert_eq!(reader.f32().unwrap(), 30.0);
        assert_eq!(reader.position(), encoded.len());
        assert!(encode_formulas(&[], Endian::Little).unwrap().is_empty());
    }
}
