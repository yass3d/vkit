use thiserror::Error;

const VERTEX_HEADER: u8 = 0xa0;
const INDEX_HEADER: u8 = 0xe0;
const SEQUENCE_HEADER: u8 = 0xd0;

const SEQUENCE_TAIL_BYTES: usize = 4;

const BYTE_GROUP_SIZE: usize = 16;

const BYTE_GROUP_DECODE_LIMIT: usize = 24;

const VERTEX_BLOCK_SIZE_BYTES: usize = 8192;
const VERTEX_BLOCK_MAX_SIZE: usize = 256;
const TAIL_MAX_SIZE: usize = 32;
const MAX_VERTEX_STRIDE: usize = 256;

const FIFO_SIZE: usize = 16;
const CODEAUX_TABLE_BYTES: usize = 16;

pub const MAX_DECODED_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum MeshoptError {
    #[error("unknown EXT_meshopt_compression mode \"{0}\"")]
    UnknownMode(String),

    #[error("unknown EXT_meshopt_compression filter \"{0}\"")]
    UnknownFilter(String),

    #[error(
        "buffer view declares EXT_meshopt_compression mode {declared} but carries a {found} \
         stream"
    )]
    ModeHeaderMismatch {
        declared: &'static str,
        found: &'static str,
    },

    #[error(
        "vertex stream uses codec version {version}; EXT_meshopt_compression permits only \
         version 0, so re-pack the file with a meshoptimizer set to the version 0 vertex codec"
    )]
    VertexCodecVersionUnsupported { version: u8 },

    #[error("{codec} stream header is 0x{found:02x}, expected 0x{expected:02x}")]
    InvalidHeader {
        codec: &'static str,
        found: u8,
        expected: u8,
    },

    #[error("{codec} stream is version {version}, which this decoder does not implement")]
    UnsupportedVersion { codec: &'static str, version: u8 },

    #[error("byteStride {stride} is not valid for {codec} ({reason})")]
    InvalidStride {
        codec: &'static str,
        stride: usize,
        reason: &'static str,
    },

    #[error("element count {count} is not valid for {codec} ({reason})")]
    InvalidCount {
        codec: &'static str,
        count: usize,
        reason: &'static str,
    },

    #[error(
        "{codec} stream is truncated: needed {needed} bytes at offset {offset}, {available} available"
    )]
    Truncated {
        codec: &'static str,
        offset: usize,
        needed: usize,
        available: usize,
    },

    #[error("{codec} stream has {extra} unconsumed trailing bytes")]
    TrailingData { codec: &'static str, extra: usize },

    #[error("decoded size {requested} bytes exceeds the {limit} byte limit")]
    TooLarge { requested: usize, limit: usize },

    #[error("filter {filter} cannot be applied to mode {mode}")]
    FilterModeMismatch {
        filter: &'static str,
        mode: &'static str,
    },
}

type Result<T> = std::result::Result<T, MeshoptError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Attributes,
    Triangles,
    Indices,
}

impl Mode {
    pub fn from_extension_name(name: &str) -> Result<Self> {
        match name {
            "ATTRIBUTES" => Ok(Mode::Attributes),
            "TRIANGLES" => Ok(Mode::Triangles),
            "INDICES" => Ok(Mode::Indices),
            other => Err(MeshoptError::UnknownMode(other.to_string())),
        }
    }

    pub fn extension_name(self) -> &'static str {
        match self {
            Mode::Attributes => "ATTRIBUTES",
            Mode::Triangles => "TRIANGLES",
            Mode::Indices => "INDICES",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    None,
    Octahedral,
    Quaternion,
    Exponential,
}

impl Filter {
    pub fn from_extension_name(name: &str) -> Result<Self> {
        match name {
            "NONE" => Ok(Filter::None),
            "OCTAHEDRAL" => Ok(Filter::Octahedral),
            "QUATERNION" => Ok(Filter::Quaternion),
            "EXPONENTIAL" => Ok(Filter::Exponential),
            other => Err(MeshoptError::UnknownFilter(other.to_string())),
        }
    }

    pub fn extension_name(self) -> &'static str {
        match self {
            Filter::None => "NONE",
            Filter::Octahedral => "OCTAHEDRAL",
            Filter::Quaternion => "QUATERNION",
            Filter::Exponential => "EXPONENTIAL",
        }
    }
}

pub fn decode_buffer_view(
    encoded: &[u8],
    mode: Mode,
    filter: Filter,
    count: usize,
    byte_stride: usize,
) -> Result<Vec<u8>> {
    if filter != Filter::None && mode != Mode::Attributes {
        return Err(MeshoptError::FilterModeMismatch {
            filter: filter.extension_name(),
            mode: mode.extension_name(),
        });
    }

    let mut decoded = match mode {
        Mode::Attributes => decode_vertex_buffer(encoded, count, byte_stride)?,
        Mode::Triangles => decode_index_buffer(encoded, count, byte_stride)?,
        Mode::Indices => decode_index_sequence(encoded, count, byte_stride)?,
    };

    match filter {
        Filter::None => {}
        Filter::Octahedral => filter_octahedral(&mut decoded, count, byte_stride)?,
        Filter::Quaternion => filter_quaternion(&mut decoded, count, byte_stride)?,
        Filter::Exponential => filter_exponential(&mut decoded, count, byte_stride)?,
    }

    Ok(decoded)
}

fn checked_output_len(count: usize, stride: usize) -> Result<usize> {
    let requested = count.checked_mul(stride).ok_or(MeshoptError::TooLarge {
        requested: usize::MAX,
        limit: MAX_DECODED_BYTES,
    })?;
    if requested > MAX_DECODED_BYTES {
        return Err(MeshoptError::TooLarge {
            requested,
            limit: MAX_DECODED_BYTES,
        });
    }
    Ok(requested)
}

fn unzigzag8(value: u8) -> u8 {
    (value >> 1) ^ 0u8.wrapping_sub(value & 1)
}

#[cfg(test)]
fn zigzag8(value: u8) -> u8 {
    (value << 1) ^ ((value as i8) >> 7) as u8
}

fn unzigzag32(value: u32) -> u32 {
    (value >> 1) ^ 0u32.wrapping_sub(value & 1)
}

fn vertex_block_size(stride: usize) -> usize {
    let per_block = (VERTEX_BLOCK_SIZE_BYTES / stride) & !(BYTE_GROUP_SIZE - 1);
    per_block.clamp(BYTE_GROUP_SIZE, VERTEX_BLOCK_MAX_SIZE)
}

fn decode_vertex_buffer(encoded: &[u8], count: usize, stride: usize) -> Result<Vec<u8>> {
    const CODEC: &str = "vertex";

    if stride == 0 || stride > MAX_VERTEX_STRIDE {
        return Err(MeshoptError::InvalidStride {
            codec: CODEC,
            stride,
            reason: "must be between 4 and 256",
        });
    }
    if !stride.is_multiple_of(4) {
        return Err(MeshoptError::InvalidStride {
            codec: CODEC,
            stride,
            reason: "must be a multiple of 4",
        });
    }

    let output_len = checked_output_len(count, stride)?;
    let tail = stride.max(TAIL_MAX_SIZE);

    if encoded.len() < 1 + tail {
        return Err(MeshoptError::Truncated {
            codec: CODEC,
            offset: 0,
            needed: 1 + tail,
            available: encoded.len(),
        });
    }

    let header = encoded[0];
    if header & 0xf0 != VERTEX_HEADER {
        return Err(MeshoptError::InvalidHeader {
            codec: CODEC,
            found: header,
            expected: VERTEX_HEADER,
        });
    }
    let version = header & 0x0f;
    if version != 0 {
        return Err(MeshoptError::VertexCodecVersionUnsupported { version });
    }

    let mut last_vertex = vec![0u8; stride];
    last_vertex.copy_from_slice(&encoded[encoded.len() - stride..]);

    let mut output = vec![0u8; output_len];
    let block = vertex_block_size(stride);
    let mut group = vec![0u8; VERTEX_BLOCK_MAX_SIZE];
    let mut position = 1usize;
    let mut vertex_offset = 0usize;

    while vertex_offset < count {
        let block_count = block.min(count - vertex_offset);
        let start = vertex_offset * stride;
        let end = start + block_count * stride;
        position = decode_vertex_block(
            encoded,
            position,
            &mut output[start..end],
            block_count,
            stride,
            &mut last_vertex,
            &mut group,
        )?;
        vertex_offset += block_count;
    }

    let remaining = encoded.len() - position;
    if remaining < tail {
        return Err(MeshoptError::Truncated {
            codec: CODEC,
            offset: position,
            needed: tail,
            available: remaining,
        });
    }
    if remaining > tail {
        return Err(MeshoptError::TrailingData {
            codec: CODEC,
            extra: remaining - tail,
        });
    }

    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn decode_vertex_block(
    encoded: &[u8],
    mut position: usize,
    output: &mut [u8],
    count: usize,
    stride: usize,
    last_vertex: &mut [u8],
    group: &mut [u8],
) -> Result<usize> {
    let aligned = (count + BYTE_GROUP_SIZE - 1) & !(BYTE_GROUP_SIZE - 1);

    for (byte_index, seed) in last_vertex.iter().enumerate() {
        position = decode_bytes(encoded, position, &mut group[..aligned])?;

        let mut previous = *seed;
        let mut slot = byte_index;
        for delta in group.iter().take(count) {
            let value = unzigzag8(*delta).wrapping_add(previous);
            output[slot] = value;
            previous = value;
            slot += stride;
        }
    }

    if count > 0 {
        let last = (count - 1) * stride;
        last_vertex.copy_from_slice(&output[last..last + stride]);
    }

    Ok(position)
}

fn decode_bytes(encoded: &[u8], mut position: usize, group: &mut [u8]) -> Result<usize> {
    const CODEC: &str = "vertex";

    let size = group.len();
    let header_bytes = (size / BYTE_GROUP_SIZE).div_ceil(4);
    let header_start = position;

    if encoded.len().saturating_sub(position) < header_bytes {
        return Err(MeshoptError::Truncated {
            codec: CODEC,
            offset: position,
            needed: header_bytes,
            available: encoded.len().saturating_sub(position),
        });
    }
    position += header_bytes;

    let mut offset = 0usize;
    while offset < size {
        let available = encoded.len().saturating_sub(position);
        if available < BYTE_GROUP_DECODE_LIMIT {
            return Err(MeshoptError::Truncated {
                codec: CODEC,
                offset: position,
                needed: BYTE_GROUP_DECODE_LIMIT,
                available,
            });
        }

        let group_index = offset / BYTE_GROUP_SIZE;
        let control = encoded[header_start + group_index / 4];
        let bits_log2 = (control >> ((group_index % 4) * 2)) & 3;

        position = decode_byte_group(
            encoded,
            position,
            &mut group[offset..offset + BYTE_GROUP_SIZE],
            bits_log2,
        )?;
        offset += BYTE_GROUP_SIZE;
    }

    Ok(position)
}

fn decode_byte_group(
    encoded: &[u8],
    position: usize,
    group: &mut [u8],
    bits_log2: u8,
) -> Result<usize> {
    match bits_log2 {
        0 => {
            group.fill(0);
            Ok(position)
        }
        1 => decode_packed_group(encoded, position, group, 2),
        2 => decode_packed_group(encoded, position, group, 4),
        _ => {
            let end = position + BYTE_GROUP_SIZE;
            let source = encoded.get(position..end).ok_or(MeshoptError::Truncated {
                codec: "vertex",
                offset: position,
                needed: BYTE_GROUP_SIZE,
                available: encoded.len().saturating_sub(position),
            })?;
            group.copy_from_slice(source);
            Ok(end)
        }
    }
}

fn decode_packed_group(
    encoded: &[u8],
    position: usize,
    group: &mut [u8],
    bits: u32,
) -> Result<usize> {
    let per_byte = (8 / bits) as usize;
    let packed_bytes = BYTE_GROUP_SIZE / per_byte;
    let sentinel = (1u8 << bits) - 1;
    let mut overflow = position + packed_bytes;

    for (index, slot) in group.iter_mut().enumerate() {
        let byte = *encoded
            .get(position + index / per_byte)
            .ok_or(MeshoptError::Truncated {
                codec: "vertex",
                offset: position,
                needed: packed_bytes,
                available: encoded.len().saturating_sub(position),
            })?;
        let shift = 8 - bits * ((index % per_byte) as u32 + 1);
        let code = (byte >> shift) & sentinel;

        if code == sentinel {
            *slot = *encoded.get(overflow).ok_or(MeshoptError::Truncated {
                codec: "vertex",
                offset: overflow,
                needed: 1,
                available: 0,
            })?;
            overflow += 1;
        } else {
            *slot = code;
        }
    }

    Ok(overflow)
}

struct IndexReader<'a> {
    data: &'a [u8],
    position: usize,
}

impl IndexReader<'_> {
    fn next_byte(&mut self) -> Result<u8> {
        let byte = *self
            .data
            .get(self.position)
            .ok_or(MeshoptError::Truncated {
                codec: "index",
                offset: self.position,
                needed: 1,
                available: 0,
            })?;
        self.position += 1;
        Ok(byte)
    }

    fn next_vbyte(&mut self) -> Result<u32> {
        let lead = self.next_byte()?;
        if lead < 128 {
            return Ok(u32::from(lead));
        }

        let mut result = u32::from(lead & 127);
        let mut shift = 7u32;
        for _ in 0..4 {
            let group = self.next_byte()?;
            result |= u32::from(group & 127) << shift;
            shift += 7;
            if group < 128 {
                break;
            }
        }
        Ok(result)
    }

    fn next_index(&mut self, last: u32) -> Result<u32> {
        let value = self.next_vbyte()?;
        Ok(last.wrapping_add(unzigzag32(value)))
    }
}

fn write_index(output: &mut [u8], slot: usize, index_size: usize, value: u32) {
    let offset = slot * index_size;
    if index_size == 2 {
        output[offset..offset + 2].copy_from_slice(&(value as u16).to_le_bytes());
    } else {
        output[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}

fn decode_index_buffer(encoded: &[u8], count: usize, index_size: usize) -> Result<Vec<u8>> {
    const CODEC: &str = "index";

    if index_size != 2 && index_size != 4 {
        return Err(MeshoptError::InvalidStride {
            codec: CODEC,
            stride: index_size,
            reason: "must be 2 or 4",
        });
    }
    if !count.is_multiple_of(3) {
        return Err(MeshoptError::InvalidCount {
            codec: CODEC,
            count,
            reason: "must be a multiple of 3",
        });
    }

    let output_len = checked_output_len(count, index_size)?;
    let minimum = 1 + count / 3 + CODEAUX_TABLE_BYTES;
    if encoded.len() < minimum {
        return Err(MeshoptError::Truncated {
            codec: CODEC,
            offset: 0,
            needed: minimum,
            available: encoded.len(),
        });
    }

    let header = encoded[0];
    if header & 0xf0 != INDEX_HEADER {
        if header & 0xf0 == SEQUENCE_HEADER {
            return Err(MeshoptError::ModeHeaderMismatch {
                declared: Mode::Triangles.extension_name(),
                found: Mode::Indices.extension_name(),
            });
        }
        return Err(MeshoptError::InvalidHeader {
            codec: CODEC,
            found: header,
            expected: INDEX_HEADER,
        });
    }
    let version = header & 0x0f;
    if version > 1 {
        return Err(MeshoptError::UnsupportedVersion {
            codec: CODEC,
            version,
        });
    }

    let fec_max: u32 = if version >= 1 { 13 } else { 15 };

    let codeaux_table_start = encoded.len() - CODEAUX_TABLE_BYTES;
    let mut output = vec![0u8; output_len];

    let mut edge_fifo = [[u32::MAX; 2]; FIFO_SIZE];
    let mut vertex_fifo = [u32::MAX; FIFO_SIZE];
    let mut edge_offset = 0usize;
    let mut vertex_offset = 0usize;
    let mut next = 0u32;
    let mut last = 0u32;

    let mut code_position = 1usize;
    let mut reader = IndexReader {
        data: encoded,
        position: 1 + count / 3,
    };

    let mut triangle = 0usize;
    while triangle < count {
        if reader.position > codeaux_table_start {
            return Err(MeshoptError::Truncated {
                codec: CODEC,
                offset: reader.position,
                needed: CODEAUX_TABLE_BYTES,
                available: encoded.len().saturating_sub(reader.position),
            });
        }

        let code_triangle = encoded[code_position];
        code_position += 1;

        let (a, b, c) = if code_triangle < 0xf0 {
            let edge = (code_triangle >> 4) as usize;
            let slot = edge_offset.wrapping_sub(1 + edge) & (FIFO_SIZE - 1);
            let a = edge_fifo[slot][0];
            let b = edge_fifo[slot][1];
            let fec = u32::from(code_triangle & 15);

            if fec < fec_max {
                let cached =
                    vertex_fifo[vertex_offset.wrapping_sub(1 + fec as usize) & (FIFO_SIZE - 1)];
                let fresh = fec == 0;
                let c = if fresh { next } else { cached };
                if fresh {
                    next = next.wrapping_add(1);
                }

                push_vertex(&mut vertex_fifo, &mut vertex_offset, c, fresh);
                push_edge(&mut edge_fifo, &mut edge_offset, c, b);
                push_edge(&mut edge_fifo, &mut edge_offset, a, c);
                (a, b, c)
            } else {
                let c = if fec != 15 {
                    let step = fec as i64 - (fec ^ 3) as i64;
                    last = last.wrapping_add(step as u32);
                    last
                } else {
                    last = reader.next_index(last)?;
                    last
                };

                push_vertex(&mut vertex_fifo, &mut vertex_offset, c, true);
                push_edge(&mut edge_fifo, &mut edge_offset, c, b);
                push_edge(&mut edge_fifo, &mut edge_offset, a, c);
                (a, b, c)
            }
        } else {
            let (fea, codeaux, literal_codeaux) = if code_triangle < 0xfe {
                (
                    0u32,
                    encoded[codeaux_table_start + (code_triangle & 15) as usize],
                    false,
                )
            } else if code_triangle == 0xfe {
                (0u32, reader.next_byte()?, true)
            } else {
                (15u32, reader.next_byte()?, true)
            };

            if literal_codeaux && codeaux == 0 {
                next = 0;
            }

            let feb = u32::from(codeaux >> 4);
            let fec = u32::from(codeaux & 15);

            let mut a = if fea == 0 {
                let value = next;
                next = next.wrapping_add(1);
                value
            } else {
                0
            };
            let mut b = if feb == 0 {
                let value = next;
                next = next.wrapping_add(1);
                value
            } else {
                vertex_fifo[vertex_offset.wrapping_sub(feb as usize) & (FIFO_SIZE - 1)]
            };
            let mut c = if fec == 0 {
                let value = next;
                next = next.wrapping_add(1);
                value
            } else {
                vertex_fifo[vertex_offset.wrapping_sub(fec as usize) & (FIFO_SIZE - 1)]
            };

            if fea == 15 {
                last = reader.next_index(last)?;
                a = last;
            }
            if feb == 15 {
                last = reader.next_index(last)?;
                b = last;
            }
            if fec == 15 {
                last = reader.next_index(last)?;
                c = last;
            }

            push_vertex(&mut vertex_fifo, &mut vertex_offset, a, true);
            push_vertex(
                &mut vertex_fifo,
                &mut vertex_offset,
                b,
                feb == 0 || feb == 15,
            );
            push_vertex(
                &mut vertex_fifo,
                &mut vertex_offset,
                c,
                fec == 0 || fec == 15,
            );

            push_edge(&mut edge_fifo, &mut edge_offset, b, a);
            push_edge(&mut edge_fifo, &mut edge_offset, c, b);
            push_edge(&mut edge_fifo, &mut edge_offset, a, c);
            (a, b, c)
        };

        write_index(&mut output, triangle, index_size, a);
        write_index(&mut output, triangle + 1, index_size, b);
        write_index(&mut output, triangle + 2, index_size, c);

        triangle += 3;
    }

    if reader.position != codeaux_table_start {
        if reader.position < codeaux_table_start {
            return Err(MeshoptError::TrailingData {
                codec: CODEC,
                extra: codeaux_table_start - reader.position,
            });
        }
        return Err(MeshoptError::Truncated {
            codec: CODEC,
            offset: reader.position,
            needed: 0,
            available: 0,
        });
    }

    Ok(output)
}

fn decode_index_sequence(encoded: &[u8], count: usize, index_size: usize) -> Result<Vec<u8>> {
    const CODEC: &str = "index sequence";

    if index_size != 2 && index_size != 4 {
        return Err(MeshoptError::InvalidStride {
            codec: CODEC,
            stride: index_size,
            reason: "must be 2 or 4",
        });
    }

    let output_len = checked_output_len(count, index_size)?;

    let header = *encoded.first().ok_or(MeshoptError::Truncated {
        codec: CODEC,
        offset: 0,
        needed: 1,
        available: 0,
    })?;
    if header & 0xf0 != SEQUENCE_HEADER {
        if header & 0xf0 == INDEX_HEADER {
            return Err(MeshoptError::ModeHeaderMismatch {
                declared: Mode::Indices.extension_name(),
                found: Mode::Triangles.extension_name(),
            });
        }
        return Err(MeshoptError::InvalidHeader {
            codec: CODEC,
            found: header,
            expected: SEQUENCE_HEADER,
        });
    }
    let version = header & 0x0f;
    if version > 1 {
        return Err(MeshoptError::UnsupportedVersion {
            codec: CODEC,
            version,
        });
    }

    let minimum = 1 + count + SEQUENCE_TAIL_BYTES;
    if encoded.len() < minimum {
        return Err(MeshoptError::Truncated {
            codec: CODEC,
            offset: 0,
            needed: minimum,
            available: encoded.len(),
        });
    }

    let tail_start = encoded.len() - SEQUENCE_TAIL_BYTES;
    let mut reader = IndexReader {
        data: &encoded[..tail_start],
        position: 1,
    };

    let mut output = vec![0u8; output_len];
    let mut last = [0u32; 2];
    for slot in 0..count {
        let value = reader.next_vbyte()?;
        let baseline = (value & 1) as usize;
        let index = last[baseline].wrapping_add(unzigzag32(value >> 1));
        last[baseline] = index;
        write_index(&mut output, slot, index_size, index);
    }

    if reader.position != tail_start {
        return Err(MeshoptError::TrailingData {
            codec: CODEC,
            extra: tail_start - reader.position,
        });
    }

    Ok(output)
}

fn push_edge(fifo: &mut [[u32; 2]; FIFO_SIZE], offset: &mut usize, a: u32, b: u32) {
    fifo[*offset] = [a, b];
    *offset = (*offset + 1) & (FIFO_SIZE - 1);
}

fn push_vertex(fifo: &mut [u32; FIFO_SIZE], offset: &mut usize, value: u32, advance: bool) {
    fifo[*offset] = value;
    *offset = (*offset + usize::from(advance)) & (FIFO_SIZE - 1);
}

fn round_to_int(value: f32) -> i32 {
    let biased = if value >= 0.0 {
        value + 0.5
    } else {
        value - 0.5
    };
    if biased.is_nan() { 0 } else { biased as i32 }
}

fn octahedral_element(x_in: f32, y_in: f32, w_in: f32, max: f32) -> [f32; 3] {
    let mut x = x_in;
    let mut y = y_in;
    let z = w_in - x.abs() - y.abs();

    let fold = if z >= 0.0 { 0.0 } else { z };
    x += if x >= 0.0 { fold } else { -fold };
    y += if y >= 0.0 { fold } else { -fold };

    let length = (x * x + y * y + z * z).sqrt();
    if !length.is_finite() || length <= 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let scale = max / length;
    [
        (x * scale).clamp(-max, max),
        (y * scale).clamp(-max, max),
        (z * scale).clamp(-max, max),
    ]
}

fn filter_octahedral(data: &mut [u8], count: usize, stride: usize) -> Result<()> {
    match stride {
        4 => {
            for element in 0..count {
                let base = element * 4;
                let x = data[base] as i8 as f32;
                let y = data[base + 1] as i8 as f32;
                let w = data[base + 2] as i8 as f32;
                let decoded = octahedral_element(x, y, w, 127.0);
                for (lane, value) in decoded.iter().enumerate() {
                    data[base + lane] = round_to_int(*value).clamp(-127, 127) as i8 as u8;
                }
            }
            Ok(())
        }
        8 => {
            for element in 0..count {
                let base = element * 8;
                let read = |offset: usize| -> f32 {
                    i16::from_le_bytes([data[base + offset], data[base + offset + 1]]) as f32
                };
                let decoded = octahedral_element(read(0), read(2), read(4), 32767.0);
                for (lane, value) in decoded.iter().enumerate() {
                    let encoded = round_to_int(*value).clamp(-32767, 32767) as i16;
                    data[base + lane * 2..base + lane * 2 + 2]
                        .copy_from_slice(&encoded.to_le_bytes());
                }
            }
            Ok(())
        }
        _ => Err(MeshoptError::InvalidStride {
            codec: "octahedral filter",
            stride,
            reason: "must be 4 or 8",
        }),
    }
}

fn filter_quaternion(data: &mut [u8], count: usize, stride: usize) -> Result<()> {
    if stride != 8 {
        return Err(MeshoptError::InvalidStride {
            codec: "quaternion filter",
            stride,
            reason: "must be 8",
        });
    }

    let component_scale = (1.0f32 / 2.0f32.sqrt()) / 32767.0;

    for element in 0..count {
        let base = element * 8;
        let read = |offset: usize| -> i16 {
            i16::from_le_bytes([data[base + offset], data[base + offset + 1]])
        };

        let dropped = (read(6) & 3) as usize;
        let x = f32::from(read(0)) * component_scale;
        let y = f32::from(read(2)) * component_scale;
        let z = f32::from(read(4)) * component_scale;

        let squared = 1.0 - x * x - y * y - z * z;
        let w = if squared > 0.0 { squared.sqrt() } else { 0.0 };

        let quantized = [
            round_to_int(x * 32767.0).clamp(-32767, 32767) as i16,
            round_to_int(y * 32767.0).clamp(-32767, 32767) as i16,
            round_to_int(z * 32767.0).clamp(-32767, 32767) as i16,
            round_to_int(w * 32767.0).clamp(-32767, 32767) as i16,
        ];

        let lanes = [
            (dropped + 1) & 3,
            (dropped + 2) & 3,
            (dropped + 3) & 3,
            dropped,
        ];
        for (value, lane) in quantized.iter().zip(lanes.iter()) {
            let offset = base + lane * 2;
            data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
    }

    Ok(())
}

fn filter_exponential(data: &mut [u8], count: usize, stride: usize) -> Result<()> {
    if stride == 0 || !stride.is_multiple_of(4) {
        return Err(MeshoptError::InvalidStride {
            codec: "exponential filter",
            stride,
            reason: "must be a non-zero multiple of 4",
        });
    }

    let lanes = count * (stride / 4);
    for lane in 0..lanes {
        let offset = lane * 4;
        let packed = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);

        let mantissa = ((packed << 8) as i32) >> 8;
        let exponent = (packed as i32) >> 24;

        let biased = exponent + 127;
        let value = if (0..=255).contains(&biased) {
            mantissa as f32 * f32::from_bits((biased as u32) << 23)
        } else {
            mantissa as f32 * 2.0f32.powi(exponent)
        };

        data[offset..offset + 4].copy_from_slice(&value.to_bits().to_le_bytes());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_byte_group(output: &mut Vec<u8>, group: &[u8]) -> u8 {
        let all_zero = group.iter().all(|value| *value == 0);
        if all_zero {
            return 0;
        }

        let two_bit_overflow = group.iter().filter(|value| **value >= 3).count();
        let four_bit_overflow = group.iter().filter(|value| **value >= 15).count();
        let cost_two = 4 + two_bit_overflow;
        let cost_four = 8 + four_bit_overflow;

        if cost_two <= cost_four && cost_two <= BYTE_GROUP_SIZE {
            encode_packed_group(output, group, 2);
            1
        } else if cost_four <= BYTE_GROUP_SIZE {
            encode_packed_group(output, group, 4);
            2
        } else {
            output.extend_from_slice(group);
            3
        }
    }

    fn encode_packed_group(output: &mut Vec<u8>, group: &[u8], bits: u32) {
        let per_byte = (8 / bits) as usize;
        let packed_bytes = BYTE_GROUP_SIZE / per_byte;
        let sentinel = (1u8 << bits) - 1;

        let mut packed = vec![0u8; packed_bytes];
        let mut overflow = Vec::new();
        for (index, value) in group.iter().enumerate() {
            let code = if *value >= sentinel {
                overflow.push(*value);
                sentinel
            } else {
                *value
            };
            let shift = 8 - bits * ((index % per_byte) as u32 + 1);
            packed[index / per_byte] |= code << shift;
        }
        output.extend_from_slice(&packed);
        output.extend_from_slice(&overflow);
    }

    fn encode_bytes(output: &mut Vec<u8>, values: &[u8]) {
        let groups = values.len() / BYTE_GROUP_SIZE;
        let header_bytes = groups.div_ceil(4);
        let header_start = output.len();
        output.extend(std::iter::repeat_n(0u8, header_bytes));

        for group_index in 0..groups {
            let start = group_index * BYTE_GROUP_SIZE;
            let bits_log2 = encode_byte_group(output, &values[start..start + BYTE_GROUP_SIZE]);
            output[header_start + group_index / 4] |= bits_log2 << ((group_index % 4) * 2);
        }
    }

    fn encode_vertex_buffer(vertices: &[u8], count: usize, stride: usize) -> Vec<u8> {
        let mut output = vec![VERTEX_HEADER];
        let first: Vec<u8> = if count > 0 {
            vertices[..stride].to_vec()
        } else {
            vec![0u8; stride]
        };
        let mut last = first.clone();

        let block = vertex_block_size(stride);
        let mut offset = 0usize;
        while offset < count {
            let block_count = block.min(count - offset);
            let aligned = (block_count + BYTE_GROUP_SIZE - 1) & !(BYTE_GROUP_SIZE - 1);

            for byte_index in 0..stride {
                let mut deltas = vec![0u8; aligned];
                let mut previous = last[byte_index];
                for i in 0..block_count {
                    let value = vertices[(offset + i) * stride + byte_index];
                    deltas[i] = zigzag8(value.wrapping_sub(previous));
                    previous = value;
                }
                encode_bytes(&mut output, &deltas);
            }

            let last_row = (offset + block_count - 1) * stride;
            last.copy_from_slice(&vertices[last_row..last_row + stride]);
            offset += block_count;
        }

        let tail = stride.max(TAIL_MAX_SIZE);
        output.extend(std::iter::repeat_n(0u8, tail - stride));
        output.extend_from_slice(&first);
        output
    }

    fn zigzag32(value: i32) -> u32 {
        ((value << 1) ^ (value >> 31)) as u32
    }

    fn encode_vbyte(output: &mut Vec<u8>, mut value: u32) {
        loop {
            let byte = (value & 127) as u8;
            value >>= 7;
            if value == 0 {
                output.push(byte);
                break;
            }
            output.push(byte | 128);
        }
    }

    struct IndexEncoder {
        codes: Vec<u8>,
        data: Vec<u8>,
        edge_fifo: [[u32; 2]; FIFO_SIZE],
        vertex_fifo: [u32; FIFO_SIZE],
        edge_offset: usize,
        vertex_offset: usize,
        next: u32,
        last: u32,
    }

    impl IndexEncoder {
        fn new() -> Self {
            IndexEncoder {
                codes: Vec::new(),
                data: Vec::new(),
                edge_fifo: [[u32::MAX; 2]; FIFO_SIZE],
                vertex_fifo: [u32::MAX; FIFO_SIZE],
                edge_offset: 0,
                vertex_offset: 0,
                next: 0,
                last: 0,
            }
        }

        fn find_edge(&self, a: u32, b: u32) -> Option<usize> {
            (0..FIFO_SIZE).find(|index| {
                let slot = self.edge_offset.wrapping_sub(1 + index) & (FIFO_SIZE - 1);
                self.edge_fifo[slot] == [a, b]
            })
        }

        fn find_vertex(&self, value: u32) -> Option<usize> {
            (0..FIFO_SIZE).find(|index| {
                let slot = self.vertex_offset.wrapping_sub(1 + index) & (FIFO_SIZE - 1);
                self.vertex_fifo[slot] == value
            })
        }

        fn push(&mut self, triangle: [u32; 3]) {
            let [a, b, c] = triangle;

            if let Some(edge) = self.find_edge(a, b) {
                let found = self.find_vertex(c).filter(|index| (1..15).contains(index));
                let fec = if c == self.next {
                    0usize
                } else {
                    found.unwrap_or(15)
                };

                self.codes.push(((edge as u8) << 4) | fec as u8);
                if fec == 0 {
                    self.next = self.next.wrapping_add(1);
                } else if fec == 15 {
                    encode_vbyte(&mut self.data, zigzag32(c.wrapping_sub(self.last) as i32));
                    self.last = c;
                }

                push_vertex(
                    &mut self.vertex_fifo,
                    &mut self.vertex_offset,
                    c,
                    fec == 0 || fec == 15,
                );
                push_edge(&mut self.edge_fifo, &mut self.edge_offset, c, b);
                push_edge(&mut self.edge_fifo, &mut self.edge_offset, a, c);
                return;
            }

            let mut cursor = self.next;
            let fea = if a == cursor {
                cursor = cursor.wrapping_add(1);
                0usize
            } else {
                15
            };
            let feb = if b == cursor {
                cursor = cursor.wrapping_add(1);
                0usize
            } else {
                match self.find_vertex(b).filter(|index| *index < 14) {
                    Some(index) => index + 1,
                    None => 15,
                }
            };
            let fec = if c == cursor {
                cursor = cursor.wrapping_add(1);
                0usize
            } else {
                match self.find_vertex(c).filter(|index| *index < 14) {
                    Some(index) => index + 1,
                    None => 15,
                }
            };
            self.next = cursor;

            let codeaux = ((feb as u8) << 4) | fec as u8;
            if fea == 0 && codeaux == 0 {
                self.codes.push(0xf0);
            } else {
                self.codes.push(if fea == 0 { 0xfe } else { 0xff });
                self.data.push(codeaux);
            }

            for (code, value) in [(fea, a), (feb, b), (fec, c)] {
                if code == 15 {
                    encode_vbyte(
                        &mut self.data,
                        zigzag32(value.wrapping_sub(self.last) as i32),
                    );
                    self.last = value;
                }
            }

            push_vertex(&mut self.vertex_fifo, &mut self.vertex_offset, a, true);
            push_vertex(
                &mut self.vertex_fifo,
                &mut self.vertex_offset,
                b,
                feb == 0 || feb == 15,
            );
            push_vertex(
                &mut self.vertex_fifo,
                &mut self.vertex_offset,
                c,
                fec == 0 || fec == 15,
            );

            push_edge(&mut self.edge_fifo, &mut self.edge_offset, b, a);
            push_edge(&mut self.edge_fifo, &mut self.edge_offset, c, b);
            push_edge(&mut self.edge_fifo, &mut self.edge_offset, a, c);
        }

        fn finish(self) -> Vec<u8> {
            let mut buffer = vec![INDEX_HEADER];
            buffer.extend_from_slice(&self.codes);
            buffer.extend_from_slice(&self.data);
            buffer.extend(std::iter::repeat_n(0u8, CODEAUX_TABLE_BYTES));
            buffer
        }
    }

    fn encode_index_buffer(indices: &[u32]) -> Vec<u8> {
        let mut encoder = IndexEncoder::new();
        for triangle in indices.chunks_exact(3) {
            encoder.push([triangle[0], triangle[1], triangle[2]]);
        }
        encoder.finish()
    }

    fn decoded_indices(bytes: &[u8], index_size: usize) -> Vec<u32> {
        bytes
            .chunks_exact(index_size)
            .map(|chunk| {
                if index_size == 2 {
                    u32::from(u16::from_le_bytes([chunk[0], chunk[1]]))
                } else {
                    u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                }
            })
            .collect()
    }

    fn pseudo_random(seed: &mut u32) -> u32 {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *seed >> 8
    }

    #[test]
    fn vertex_round_trip_over_multiple_blocks() {
        let stride = 12usize;
        let count = 700usize;
        let mut seed = 7u32;
        let mut vertices = vec![0u8; count * stride];
        for element in 0..count {
            for lane in 0..stride {
                let jitter = (pseudo_random(&mut seed) % 5) as i32 - 2;
                let base = (element as i32 * 3 + lane as i32 * 17 + jitter) as u8;
                vertices[element * stride + lane] = base;
            }
        }

        let encoded = encode_vertex_buffer(&vertices, count, stride);
        let decoded =
            decode_buffer_view(&encoded, Mode::Attributes, Filter::None, count, stride).unwrap();
        assert_eq!(decoded, vertices);
    }

    #[test]
    fn vertex_round_trip_exercises_every_byte_group_encoding() {
        let stride = 4usize;
        let count = 64usize;
        let mut vertices = vec![0u8; count * stride];
        for element in 0..count {
            vertices[element * stride] = 0;
            vertices[element * stride + 1] = (element % 2) as u8;
            vertices[element * stride + 2] = (element as u8).wrapping_mul(7);
            vertices[element * stride + 3] = if element % 16 == 0 { 200 } else { 3 };
        }

        let encoded = encode_vertex_buffer(&vertices, count, stride);
        let decoded =
            decode_buffer_view(&encoded, Mode::Attributes, Filter::None, count, stride).unwrap();
        assert_eq!(decoded, vertices);
    }

    #[test]
    fn vertex_round_trip_at_maximum_stride() {
        let stride = 256usize;
        let count = 40usize;
        let mut seed = 11u32;
        let mut vertices = vec![0u8; count * stride];
        for byte in vertices.iter_mut() {
            *byte = (pseudo_random(&mut seed) % 251) as u8;
        }

        let encoded = encode_vertex_buffer(&vertices, count, stride);
        let decoded =
            decode_buffer_view(&encoded, Mode::Attributes, Filter::None, count, stride).unwrap();
        assert_eq!(decoded, vertices);
    }

    #[test]
    fn vertex_rejects_foreign_header_and_future_version() {
        let vertices = vec![1u8; 4 * 8];
        let mut encoded = encode_vertex_buffer(&vertices, 8, 4);

        let mut wrong_header = encoded.clone();
        wrong_header[0] = 0xe0;
        assert!(matches!(
            decode_buffer_view(&wrong_header, Mode::Attributes, Filter::None, 8, 4),
            Err(MeshoptError::InvalidHeader { .. })
        ));

        encoded[0] = VERTEX_HEADER | 1;
        assert!(matches!(
            decode_buffer_view(&encoded, Mode::Attributes, Filter::None, 8, 4),
            Err(MeshoptError::VertexCodecVersionUnsupported { version: 1 })
        ));
    }

    #[test]
    fn vertex_rejects_truncation_at_every_length() {
        let vertices = vec![9u8; 12 * 40];
        let encoded = encode_vertex_buffer(&vertices, 40, 12);
        assert!(decode_buffer_view(&encoded, Mode::Attributes, Filter::None, 40, 12).is_ok());

        for length in 0..encoded.len() {
            let result =
                decode_buffer_view(&encoded[..length], Mode::Attributes, Filter::None, 40, 12);
            assert!(result.is_err(), "truncation to {length} bytes must fail");
        }
    }

    #[test]
    fn vertex_rejects_invalid_strides() {
        let payload = vec![0u8; 128];
        for stride in [0usize, 1, 2, 3, 5, 260, 1024] {
            assert!(matches!(
                decode_buffer_view(&payload, Mode::Attributes, Filter::None, 1, stride),
                Err(MeshoptError::InvalidStride { .. })
            ));
        }
    }

    #[test]
    fn vertex_rejects_trailing_bytes() {
        let vertices = vec![4u8; 8 * 16];
        let mut encoded = encode_vertex_buffer(&vertices, 16, 8);
        encoded.push(0);
        assert!(matches!(
            decode_buffer_view(&encoded, Mode::Attributes, Filter::None, 16, 8),
            Err(MeshoptError::TrailingData { .. })
        ));
    }

    #[test]
    fn oversized_request_is_refused_before_allocating() {
        let payload = vec![0u8; 64];
        assert!(matches!(
            decode_buffer_view(&payload, Mode::Attributes, Filter::None, usize::MAX / 2, 12),
            Err(MeshoptError::TooLarge { .. })
        ));
    }

    #[test]
    fn index_round_trip_for_a_fan_and_a_strip() {
        let mut indices = Vec::new();
        for i in 0..40u32 {
            indices.extend_from_slice(&[0, i + 1, i + 2]);
        }
        for i in 0..40u32 {
            indices.extend_from_slice(&[i, i + 1, i + 2]);
        }

        let encoded = encode_index_buffer(&indices);
        let decoded =
            decode_buffer_view(&encoded, Mode::Triangles, Filter::None, indices.len(), 4).unwrap();
        assert_eq!(decoded_indices(&decoded, 4), indices);
    }

    #[test]
    fn index_round_trip_for_a_grid_with_shared_edges() {
        let width = 12u32;
        let mut indices = Vec::new();
        for row in 0..width - 1 {
            for column in 0..width - 1 {
                let a = row * width + column;
                indices.extend_from_slice(&[a, a + 1, a + width]);
                indices.extend_from_slice(&[a + 1, a + width + 1, a + width]);
            }
        }

        let encoded = encode_index_buffer(&indices);
        let decoded =
            decode_buffer_view(&encoded, Mode::Triangles, Filter::None, indices.len(), 4).unwrap();
        assert_eq!(decoded_indices(&decoded, 4), indices);
    }

    #[test]
    fn index_round_trip_with_sixteen_bit_output() {
        let indices: Vec<u32> = (0..90u32).collect();
        let encoded = encode_index_buffer(&indices);
        let decoded =
            decode_buffer_view(&encoded, Mode::Triangles, Filter::None, indices.len(), 2).unwrap();
        assert_eq!(decoded_indices(&decoded, 2), indices);
    }

    #[test]
    fn index_codeaux_table_entry_decodes_a_fresh_triangle() {
        let mut encoded = vec![INDEX_HEADER, 0xf0];
        encoded.extend(std::iter::repeat_n(0u8, CODEAUX_TABLE_BYTES));
        let decoded = decode_buffer_view(&encoded, Mode::Triangles, Filter::None, 3, 4).unwrap();
        assert_eq!(decoded_indices(&decoded, 4), vec![0, 1, 2]);
    }

    #[test]
    fn index_rejects_foreign_header_bad_count_and_truncation() {
        let indices: Vec<u32> = (0..30u32).collect();
        let encoded = encode_index_buffer(&indices);

        let mut wrong_header = encoded.clone();
        wrong_header[0] = 0xa0;
        assert!(matches!(
            decode_buffer_view(&wrong_header, Mode::Triangles, Filter::None, 30, 4),
            Err(MeshoptError::InvalidHeader { .. })
        ));

        let mut future = encoded.clone();
        future[0] = INDEX_HEADER | 2;
        assert!(matches!(
            decode_buffer_view(&future, Mode::Triangles, Filter::None, 30, 4),
            Err(MeshoptError::UnsupportedVersion { version: 2, .. })
        ));

        assert!(matches!(
            decode_buffer_view(&encoded, Mode::Triangles, Filter::None, 31, 4),
            Err(MeshoptError::InvalidCount { .. })
        ));

        assert!(matches!(
            decode_buffer_view(&encoded, Mode::Triangles, Filter::None, 30, 3),
            Err(MeshoptError::InvalidStride { .. })
        ));

        for length in 0..encoded.len() {
            assert!(
                decode_buffer_view(&encoded[..length], Mode::Triangles, Filter::None, 30, 4)
                    .is_err(),
                "truncation to {length} bytes must fail"
            );
        }
    }

    #[test]
    fn index_never_panics_on_arbitrary_bytes() {
        let mut seed = 3u32;
        for _ in 0..500 {
            let length = 17 + (pseudo_random(&mut seed) % 200) as usize;
            let mut payload = vec![0u8; length];
            for byte in payload.iter_mut() {
                *byte = pseudo_random(&mut seed) as u8;
            }
            payload[0] = INDEX_HEADER;
            let count = ((length / 3) * 3).min(60);
            let _ = decode_buffer_view(&payload, Mode::Triangles, Filter::None, count, 4);
        }
    }

    #[test]
    fn vertex_never_panics_on_arbitrary_bytes() {
        let mut seed = 19u32;
        for _ in 0..500 {
            let length = 33 + (pseudo_random(&mut seed) % 400) as usize;
            let mut payload = vec![0u8; length];
            for byte in payload.iter_mut() {
                *byte = pseudo_random(&mut seed) as u8;
            }
            payload[0] = VERTEX_HEADER;
            let _ = decode_buffer_view(&payload, Mode::Attributes, Filter::None, 30, 8);
        }
    }

    fn encode_index_sequence(indices: &[u32]) -> Vec<u8> {
        let mut output = vec![SEQUENCE_HEADER | 1];
        let mut last = [0u32; 2];
        let mut current = 0usize;
        for index in indices {
            let candidate = index.wrapping_sub(last[current]) as i32;
            if candidate.saturating_abs() >= 30 {
                current ^= 1;
            }
            let delta = index.wrapping_sub(last[current]);
            let zigzag = (delta << 1) ^ (((delta as i32) >> 31) as u32);
            encode_vbyte(&mut output, (zigzag << 1) | current as u32);
            last[current] = *index;
        }
        output.extend(std::iter::repeat_n(0u8, SEQUENCE_TAIL_BYTES));
        output
    }

    #[test]
    fn index_sequence_round_trips_a_run_with_large_jumps() {
        let indices: Vec<u32> = (0..64u32)
            .flat_map(|value| [value, value + 5_000, value.wrapping_sub(3)])
            .collect();

        let encoded = encode_index_sequence(&indices);
        let decoded =
            decode_buffer_view(&encoded, Mode::Indices, Filter::None, indices.len(), 4).unwrap();
        assert_eq!(decoded_indices(&decoded, 4), indices);
    }

    #[test]
    fn index_sequence_writes_sixteen_bit_output() {
        let indices: Vec<u32> = (0..40u32).map(|value| value * 3).collect();
        let encoded = encode_index_sequence(&indices);
        let decoded =
            decode_buffer_view(&encoded, Mode::Indices, Filter::None, indices.len(), 2).unwrap();
        assert_eq!(decoded_indices(&decoded, 2), indices);
    }

    #[test]
    fn index_sequence_rejects_the_wrong_stream_bad_version_and_truncation() {
        let indices: Vec<u32> = (0..30u32).collect();
        let encoded = encode_index_sequence(&indices);

        let triangles = encode_index_buffer(&indices);
        assert!(matches!(
            decode_buffer_view(&triangles, Mode::Indices, Filter::None, 30, 4),
            Err(MeshoptError::ModeHeaderMismatch { .. })
        ));
        assert!(matches!(
            decode_buffer_view(&encoded, Mode::Triangles, Filter::None, 30, 4),
            Err(MeshoptError::ModeHeaderMismatch { .. })
        ));

        let mut future = encoded.clone();
        future[0] = SEQUENCE_HEADER | 2;
        assert!(matches!(
            decode_buffer_view(&future, Mode::Indices, Filter::None, 30, 4),
            Err(MeshoptError::UnsupportedVersion { version: 2, .. })
        ));

        let mut foreign = encoded.clone();
        foreign[0] = 0x10;
        assert!(matches!(
            decode_buffer_view(&foreign, Mode::Indices, Filter::None, 30, 4),
            Err(MeshoptError::InvalidHeader { .. })
        ));

        assert!(matches!(
            decode_buffer_view(&encoded, Mode::Indices, Filter::None, 30, 3),
            Err(MeshoptError::InvalidStride { .. })
        ));

        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(
            decode_buffer_view(&trailing, Mode::Indices, Filter::None, 30, 4).is_err(),
            "an unconsumed byte before the tail must fail"
        );

        for length in 0..encoded.len() {
            assert!(
                decode_buffer_view(&encoded[..length], Mode::Indices, Filter::None, 30, 4).is_err(),
                "truncation to {length} bytes must fail"
            );
        }
    }

    #[test]
    fn index_sequence_never_panics_on_arbitrary_bytes() {
        let mut seed = 23u32;
        for _ in 0..500 {
            let length = 5 + (pseudo_random(&mut seed) % 200) as usize;
            let mut payload = vec![0u8; length];
            for byte in payload.iter_mut() {
                *byte = pseudo_random(&mut seed) as u8;
            }
            payload[0] = SEQUENCE_HEADER;
            let count = (length - 5).min(40);
            let _ = decode_buffer_view(&payload, Mode::Indices, Filter::None, count, 4);
            let _ = decode_buffer_view(&payload, Mode::Indices, Filter::None, count, 2);
        }
    }

    #[test]
    fn octahedral_output_is_unit_length_for_byte_components() {
        let mut data = Vec::new();
        let mut expected = Vec::new();
        for x in (-127i32..=127).step_by(9) {
            for y in (-127i32..=127).step_by(9) {
                if x.abs() + y.abs() > 127 {
                    continue;
                }
                data.push(x as i8 as u8);
                data.push(y as i8 as u8);
                data.push(127u8);
                data.push(0u8);
                expected.push((x, y));
            }
        }
        let count = expected.len();

        filter_octahedral(&mut data, count, 4).unwrap();

        for element in 0..count {
            let base = element * 4;
            let x = data[base] as i8 as f32 / 127.0;
            let y = data[base + 1] as i8 as f32 / 127.0;
            let z = data[base + 2] as i8 as f32 / 127.0;
            let length = (x * x + y * y + z * z).sqrt();
            assert!(
                (length - 1.0).abs() < 0.02,
                "element {element} length {length}"
            );
            assert_eq!(data[base + 3], 0, "fourth component must be untouched");
        }
    }

    #[test]
    fn octahedral_output_is_unit_length_for_short_components() {
        let mut data = Vec::new();
        let mut count = 0usize;
        for x in (-32767i32..=32767).step_by(2731) {
            for y in (-32767i32..=32767).step_by(2731) {
                if x.abs() + y.abs() > 32767 {
                    continue;
                }
                data.extend_from_slice(&(x as i16).to_le_bytes());
                data.extend_from_slice(&(y as i16).to_le_bytes());
                data.extend_from_slice(&32767i16.to_le_bytes());
                data.extend_from_slice(&0i16.to_le_bytes());
                count += 1;
            }
        }

        filter_octahedral(&mut data, count, 8).unwrap();

        for element in 0..count {
            let base = element * 8;
            let read = |offset: usize| -> f32 {
                i16::from_le_bytes([data[base + offset], data[base + offset + 1]]) as f32 / 32767.0
            };
            let length = (read(0) * read(0) + read(2) * read(2) + read(4) * read(4)).sqrt();
            assert!(
                (length - 1.0).abs() < 0.001,
                "element {element} length {length}"
            );
        }
    }

    #[test]
    fn octahedral_rejects_strides_it_cannot_address() {
        let mut data = vec![0u8; 48];
        for stride in [2usize, 3, 6, 12, 16] {
            assert!(filter_octahedral(&mut data, 1, stride).is_err());
        }
    }

    #[test]
    fn quaternion_output_is_unit_length_and_restores_the_dropped_lane() {
        let sources: [[f32; 4]; 5] = [
            [0.0, 0.0, 0.0, 1.0],
            [0.5, 0.5, 0.5, 0.5],
            [-0.2, 0.3, -0.1, 0.927],
            [
                std::f32::consts::FRAC_1_SQRT_2,
                0.0,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
            ],
            [0.1, -0.9539, 0.2, 0.2],
        ];

        let mut data = Vec::new();
        for source in sources.iter() {
            let mut largest = 0usize;
            for lane in 1..4 {
                if source[lane].abs() > source[largest].abs() {
                    largest = lane;
                }
            }
            let sign = if source[largest] < 0.0 { -1.0 } else { 1.0 };
            let scale = 32767.0 * 2.0f32.sqrt();
            let mut lanes = [0i16; 4];
            for offset in 1..4 {
                let component = source[(largest + offset) & 3] * sign;
                lanes[offset - 1] = (component * scale).round() as i16;
            }
            lanes[3] = largest as i16;
            for lane in lanes.iter() {
                data.extend_from_slice(&lane.to_le_bytes());
            }
        }

        filter_quaternion(&mut data, sources.len(), 8).unwrap();

        for (element, source) in sources.iter().enumerate() {
            let base = element * 8;
            let read = |lane: usize| -> f32 {
                i16::from_le_bytes([data[base + lane * 2], data[base + lane * 2 + 1]]) as f32
                    / 32767.0
            };
            let decoded = [read(0), read(1), read(2), read(3)];
            let length = decoded.iter().map(|v| v * v).sum::<f32>().sqrt();
            assert!(
                (length - 1.0).abs() < 0.002,
                "element {element} length {length}"
            );

            let dot: f32 = decoded
                .iter()
                .zip(source.iter())
                .map(|(a, b)| a * b)
                .sum::<f32>()
                .abs();
            assert!((dot - 1.0).abs() < 0.01, "element {element} dot {dot}");
        }
    }

    #[test]
    fn quaternion_rejects_any_stride_but_eight() {
        let mut data = vec![0u8; 64];
        for stride in [4usize, 12, 16] {
            assert!(filter_quaternion(&mut data, 1, stride).is_err());
        }
    }

    #[test]
    fn exponential_reconstructs_the_mantissa_and_exponent() {
        let cases: [(i32, i32); 8] = [
            (0, 0),
            (1, 0),
            (-1, 0),
            (1, 10),
            (-8_388_608, 0),
            (8_388_607, -12),
            (3, -24),
            (-5, 30),
        ];

        let mut data = Vec::new();
        for (mantissa, exponent) in cases.iter() {
            let packed = ((*exponent as u32) << 24) | (*mantissa as u32 & 0x00ff_ffff);
            data.extend_from_slice(&packed.to_le_bytes());
        }

        filter_exponential(&mut data, cases.len(), 4).unwrap();

        for (element, (mantissa, exponent)) in cases.iter().enumerate() {
            let offset = element * 4;
            let value = f32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            let expected = *mantissa as f32 * 2.0f32.powi(*exponent);
            assert_eq!(value, expected, "element {element}");
        }
    }

    #[test]
    fn exponential_handles_wide_strides_and_rejects_misaligned_ones() {
        let mut data = vec![0u8; 3 * 12];
        for lane in 0..9 {
            let packed = (2u32 << 24) | 3u32;
            data[lane * 4..lane * 4 + 4].copy_from_slice(&packed.to_le_bytes());
        }
        filter_exponential(&mut data, 3, 12).unwrap();
        for lane in 0..9 {
            let value = f32::from_le_bytes([
                data[lane * 4],
                data[lane * 4 + 1],
                data[lane * 4 + 2],
                data[lane * 4 + 3],
            ]);
            assert_eq!(value, 12.0);
        }

        let mut misaligned = vec![0u8; 16];
        for stride in [0usize, 2, 6, 7] {
            assert!(filter_exponential(&mut misaligned, 1, stride).is_err());
        }
    }

    #[test]
    fn filters_are_refused_on_index_modes() {
        let indices: Vec<u32> = (0..30u32).collect();
        let encoded = encode_index_buffer(&indices);
        assert!(matches!(
            decode_buffer_view(&encoded, Mode::Triangles, Filter::Octahedral, 30, 4),
            Err(MeshoptError::FilterModeMismatch { .. })
        ));
    }

    #[test]
    fn extension_names_round_trip() {
        for name in ["ATTRIBUTES", "TRIANGLES", "INDICES"] {
            assert_eq!(
                Mode::from_extension_name(name).unwrap().extension_name(),
                name
            );
        }
        for name in ["NONE", "OCTAHEDRAL", "QUATERNION", "EXPONENTIAL"] {
            assert_eq!(
                Filter::from_extension_name(name).unwrap().extension_name(),
                name
            );
        }
        assert!(Mode::from_extension_name("SPHERES").is_err());
        assert!(Filter::from_extension_name("LOG").is_err());
    }

    #[test]
    fn zigzag_is_its_own_inverse_over_the_whole_byte_range() {
        for value in 0..=255u8 {
            assert_eq!(unzigzag8(zigzag8(value)), value);
        }
    }
}
