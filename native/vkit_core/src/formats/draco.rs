//! Decoder for `KHR_draco_mesh_compression` payloads.
//!
//! Draco is not hand-written here the way meshopt is. Its EdgeBreaker traversal, rANS entropy
//! layer and constrained-multi-parallelogram predictors are several thousand lines in which a
//! subtle error produces plausible topology with displaced vertices — silently wrong geometry,
//! the worst failure this importer can have. `draco-core` is a pure-Rust decoder for the same
//! bitstream, so the executable stays free of C, and this module is the guard rail around it:
//! the bitstream version is checked against what the glTF extension pins before a decode is
//! attempted, the decode itself runs inside `catch_unwind`, and the decoded size is bounded
//! before any of it is copied out.
//!
//! `catch_unwind` earns its place in debug and test builds, where a panic in third-party code
//! becomes an `Err` a caller can report. Release builds use `panic = "abort"` and nothing can
//! catch anything there, which is exactly why the checks below run *before* the decode rather
//! than relying on the net underneath it.

use std::panic::{AssertUnwindSafe, catch_unwind};

use draco_core::{
    DataType, DecoderBuffer, FaceIndex, GeometryAttributeType, Mesh, MeshDecoder, PointAttribute,
    PointIndex,
};
use thiserror::Error;

/// The bitstream version `KHR_draco_mesh_compression` pins. The extension says nothing about
/// what a decoder should do with a newer stream, so anything past this is refused by name
/// rather than fed to a decoder that would interpret it under the older rules.
pub const MAX_BITSTREAM_VERSION: (u8, u8) = (2, 2);

/// Ceiling on the geometry a single Draco payload may expand to, counting the float attribute
/// values and the index list. Draco's own header numbers are attacker-controlled, so this is
/// applied to what the decoder reports before any of it is copied into our own buffers.
pub const MAX_DECODED_BYTES: usize = 256 * 1024 * 1024;

const DRACO_MAGIC: &[u8; 5] = b"DRACO";

/// Byte layout of the Draco header this module parses itself: magic, major, minor, geometry
/// type, encoding method, flags.
const HEADER_BYTES: usize = 11;

#[derive(Debug, Error)]
pub enum DracoError {
    #[error("Draco payload is {available} bytes, too short to hold even a header")]
    TooShort { available: usize },

    #[error("Draco payload does not start with the DRACO magic")]
    BadMagic,

    #[error(
        "Draco bitstream version {major}.{minor} is newer than the {max_major}.{max_minor} \
         that KHR_draco_mesh_compression defines; re-export with a current glTF exporter"
    )]
    BitstreamVersionUnsupported {
        major: u8,
        minor: u8,
        max_major: u8,
        max_minor: u8,
    },

    #[error("Draco payload declares geometry type {found}, which is not a triangle mesh")]
    NotATriangleMesh { found: u8 },

    #[error("Draco decode failed: {0}")]
    Decode(String),

    #[error("Draco decoder panicked")]
    DecoderPanicked,

    #[error("Draco payload decodes to {requested} bytes of geometry, over the {limit} byte limit")]
    TooLarge { requested: usize, limit: usize },

    #[error("Draco mesh has no POSITION attribute")]
    NoPositions,

    #[error("Draco attribute {unique_id} declares {components} components, which is not usable")]
    UnusableComponents { unique_id: u32, components: u8 },

    #[error("Draco attribute {unique_id} has a value buffer shorter than its own point count")]
    AttributeBufferTruncated { unique_id: u32 },

    #[error("Draco face {face} references point {point}, past the {num_points} points decoded")]
    FaceIndexOutOfRange {
        face: usize,
        point: u32,
        num_points: usize,
    },
}

type Result<T> = std::result::Result<T, DracoError>;

/// What a decoded attribute is for, in glTF's vocabulary rather than Draco's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttributeKind {
    Position,
    Normal,
    Color,
    TexCoord,
    Generic,
}

impl AttributeKind {
    fn from_draco(kind: GeometryAttributeType) -> Option<Self> {
        match kind {
            GeometryAttributeType::Position => Some(AttributeKind::Position),
            GeometryAttributeType::Normal => Some(AttributeKind::Normal),
            GeometryAttributeType::Color => Some(AttributeKind::Color),
            GeometryAttributeType::TexCoord => Some(AttributeKind::TexCoord),
            GeometryAttributeType::Generic => Some(AttributeKind::Generic),
            GeometryAttributeType::Invalid => None,
        }
    }
}

/// One decoded attribute, expanded so that value `point * components + lane` belongs to the
/// point that the index list refers to. Draco stores unique values plus a point map; glTF
/// wants one value per point, and doing that expansion here is what lets a caller treat the
/// result exactly like an ordinary accessor.
#[derive(Clone, Debug)]
pub struct DracoAttribute {
    pub unique_id: u32,
    pub kind: AttributeKind,
    pub components: usize,
    /// Draco's declared storage type, before this module widened every value to `f32`. A
    /// caller that must honour glTF normalization needs it; one that only wants geometry
    /// does not, because positions and texcoords from every mainstream exporter are floats.
    pub integral: bool,
    pub normalized: bool,
    pub values: Vec<f32>,
}

impl DracoAttribute {
    pub fn value(&self, point: usize) -> Option<&[f32]> {
        let start = point.checked_mul(self.components)?;
        self.values.get(start..start.checked_add(self.components)?)
    }
}

/// A decoded Draco triangle mesh.
#[derive(Clone, Debug)]
pub struct DracoMesh {
    pub num_points: usize,
    pub indices: Vec<u32>,
    pub attributes: Vec<DracoAttribute>,
}

impl DracoMesh {
    /// Looks an attribute up the way `KHR_draco_mesh_compression` addresses it: the extension
    /// JSON maps each glTF semantic to a Draco unique id, not to a semantic name, because one
    /// mesh may carry several generic attributes that only the JSON can tell apart.
    pub fn attribute_by_unique_id(&self, unique_id: u32) -> Option<&DracoAttribute> {
        self.attributes
            .iter()
            .find(|attribute| attribute.unique_id == unique_id)
    }

    pub fn attribute_by_kind(&self, kind: AttributeKind) -> Option<&DracoAttribute> {
        self.attributes.iter().find(|a| a.kind == kind)
    }

    pub fn positions(&self) -> Option<&DracoAttribute> {
        self.attribute_by_kind(AttributeKind::Position)
    }

    pub fn normals(&self) -> Option<&DracoAttribute> {
        self.attribute_by_kind(AttributeKind::Normal)
    }

    pub fn texcoords(&self) -> Option<&DracoAttribute> {
        self.attribute_by_kind(AttributeKind::TexCoord)
    }
}

/// Reads the Draco header without handing the bytes to the decoder first.
///
/// The version gate has to happen here: `KHR_draco_mesh_compression` pins bitstream 2.2 and is
/// silent about newer ones, and a decoder written against 2.2 will happily reinterpret a 2.3
/// stream under the old rules rather than refuse it.
fn check_header(encoded: &[u8]) -> Result<()> {
    let header = encoded.get(..HEADER_BYTES).ok_or(DracoError::TooShort {
        available: encoded.len(),
    })?;

    if &header[..5] != DRACO_MAGIC {
        return Err(DracoError::BadMagic);
    }

    let (major, minor) = (header[5], header[6]);
    let (max_major, max_minor) = MAX_BITSTREAM_VERSION;
    if (major, minor) > (max_major, max_minor) {
        return Err(DracoError::BitstreamVersionUnsupported {
            major,
            minor,
            max_major,
            max_minor,
        });
    }

    let geometry_type = header[7];
    if geometry_type != 1 {
        return Err(DracoError::NotATriangleMesh {
            found: geometry_type,
        });
    }

    Ok(())
}

fn read_component(bytes: &[u8], data_type: DataType) -> f32 {
    match data_type {
        DataType::Int8 => bytes.first().map_or(0.0, |v| f32::from(*v as i8)),
        DataType::Uint8 | DataType::Bool => bytes.first().map_or(0.0, |v| f32::from(*v)),
        DataType::Int16 => bytes
            .get(..2)
            .map_or(0.0, |b| f32::from(i16::from_le_bytes([b[0], b[1]]))),
        DataType::Uint16 => bytes
            .get(..2)
            .map_or(0.0, |b| f32::from(u16::from_le_bytes([b[0], b[1]]))),
        DataType::Int32 => bytes
            .get(..4)
            .map_or(0.0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32),
        DataType::Uint32 => bytes
            .get(..4)
            .map_or(0.0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32),
        DataType::Float32 => bytes
            .get(..4)
            .map_or(0.0, |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        DataType::Int64 => bytes.get(..8).map_or(0.0, |b| {
            i64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32
        }),
        DataType::Uint64 => bytes.get(..8).map_or(0.0, |b| {
            u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32
        }),
        DataType::Float64 => bytes.get(..8).map_or(0.0, |b| {
            f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32
        }),
        DataType::Invalid => 0.0,
    }
}

fn expand_attribute(attribute: &PointAttribute, num_points: usize) -> Result<DracoAttribute> {
    let unique_id = attribute.unique_id();
    let Some(kind) = AttributeKind::from_draco(attribute.attribute_type()) else {
        return Err(DracoError::UnusableComponents {
            unique_id,
            components: attribute.num_components(),
        });
    };

    let components = usize::from(attribute.num_components());
    if components == 0 || components > 4 {
        return Err(DracoError::UnusableComponents {
            unique_id,
            components: attribute.num_components(),
        });
    }

    let data_type = attribute.data_type();
    let component_bytes = data_type.byte_length();
    if component_bytes == 0 {
        return Err(DracoError::UnusableComponents {
            unique_id,
            components: attribute.num_components(),
        });
    }

    let stride = usize::try_from(attribute.byte_stride()).unwrap_or(0);
    let stride = if stride == 0 {
        components * component_bytes
    } else {
        stride
    };
    let data = attribute.buffer().data();
    let element_bytes = components * component_bytes;

    let mut values = vec![0.0f32; num_points * components];
    for point in 0..num_points {
        let value_index = attribute.mapped_index(PointIndex(point as u32)).0 as usize;
        let start = value_index
            .checked_mul(stride)
            .ok_or(DracoError::AttributeBufferTruncated { unique_id })?;
        let end = start
            .checked_add(element_bytes)
            .ok_or(DracoError::AttributeBufferTruncated { unique_id })?;
        let element = data
            .get(start..end)
            .ok_or(DracoError::AttributeBufferTruncated { unique_id })?;

        for lane in 0..components {
            let offset = lane * component_bytes;
            values[point * components + lane] =
                read_component(&element[offset..offset + component_bytes], data_type);
        }
    }

    Ok(DracoAttribute {
        unique_id,
        kind,
        components,
        integral: data_type.is_integral(),
        normalized: attribute.normalized(),
        values,
    })
}

/// Decodes one `KHR_draco_mesh_compression` payload into plain glTF-shaped geometry.
///
/// Refuses only when the geometry cannot be recovered. Every refusal here is a case where
/// there is nothing to hand back: a payload that is not Draco, a bitstream version whose
/// meaning is undefined, a decode that failed, or a mesh larger than the process will hold.
pub fn decode_mesh(encoded: &[u8]) -> Result<DracoMesh> {
    check_header(encoded)?;

    let decoded = catch_unwind(AssertUnwindSafe(|| {
        let mut buffer = DecoderBuffer::new(encoded);
        let mut mesh = Mesh::new();
        MeshDecoder::new()
            .decode(&mut buffer, &mut mesh)
            .map(|()| mesh)
    }))
    .map_err(|_| DracoError::DecoderPanicked)?
    .map_err(|error| DracoError::Decode(error.to_string()))?;

    let num_points = decoded.num_points();
    let num_faces = decoded.num_faces();

    let mut float_lanes = 0usize;
    for id in 0..decoded.num_attributes() {
        let attribute = decoded.attribute(id);
        float_lanes = float_lanes
            .checked_add(
                num_points
                    .checked_mul(usize::from(attribute.num_components()))
                    .ok_or(DracoError::TooLarge {
                        requested: usize::MAX,
                        limit: MAX_DECODED_BYTES,
                    })?,
            )
            .ok_or(DracoError::TooLarge {
                requested: usize::MAX,
                limit: MAX_DECODED_BYTES,
            })?;
    }
    let requested = float_lanes
        .checked_mul(4)
        .and_then(|bytes| num_faces.checked_mul(12).and_then(|f| bytes.checked_add(f)))
        .ok_or(DracoError::TooLarge {
            requested: usize::MAX,
            limit: MAX_DECODED_BYTES,
        })?;
    if requested > MAX_DECODED_BYTES {
        return Err(DracoError::TooLarge {
            requested,
            limit: MAX_DECODED_BYTES,
        });
    }

    let mut indices = Vec::with_capacity(num_faces * 3);
    for face in 0..num_faces {
        let corners = decoded.face(FaceIndex(face as u32));
        for corner in corners {
            if corner.0 as usize >= num_points {
                return Err(DracoError::FaceIndexOutOfRange {
                    face,
                    point: corner.0,
                    num_points,
                });
            }
            indices.push(corner.0);
        }
    }

    let mut attributes = Vec::new();
    for id in 0..decoded.num_attributes() {
        attributes.push(expand_attribute(decoded.attribute(id), num_points)?);
    }

    if !attributes
        .iter()
        .any(|attribute| attribute.kind == AttributeKind::Position)
    {
        return Err(DracoError::NoPositions);
    }

    Ok(DracoMesh {
        num_points,
        indices,
        attributes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(major: u8, minor: u8, geometry_type: u8) -> Vec<u8> {
        let mut bytes = DRACO_MAGIC.to_vec();
        bytes.extend_from_slice(&[major, minor, geometry_type, 1, 0, 0]);
        bytes
    }

    #[test]
    fn a_payload_that_is_not_draco_is_named_as_such() {
        assert!(matches!(
            decode_mesh(&[]),
            Err(DracoError::TooShort { available: 0 })
        ));
        assert!(matches!(decode_mesh(&[0u8; 64]), Err(DracoError::BadMagic)));
    }

    #[test]
    fn a_bitstream_newer_than_the_extension_defines_is_refused_by_version() {
        let mut payload = header(2, 3, 1);
        payload.extend(std::iter::repeat_n(0u8, 64));
        assert!(matches!(
            decode_mesh(&payload),
            Err(DracoError::BitstreamVersionUnsupported {
                major: 2,
                minor: 3,
                ..
            })
        ));

        let mut ancient = header(1, 3, 1);
        ancient.extend(std::iter::repeat_n(0u8, 64));
        assert!(!matches!(
            decode_mesh(&ancient),
            Err(DracoError::BitstreamVersionUnsupported { .. })
        ));
    }

    #[test]
    fn a_point_cloud_payload_is_refused_before_the_decoder_sees_it() {
        let mut payload = header(2, 2, 0);
        payload.extend(std::iter::repeat_n(0u8, 64));
        assert!(matches!(
            decode_mesh(&payload),
            Err(DracoError::NotATriangleMesh { found: 0 })
        ));
    }

    #[test]
    fn arbitrary_bytes_behind_a_valid_header_never_panic() {
        let mut seed = 31u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as u8
        };

        for _ in 0..400 {
            let mut payload = header(2, 2, 1);
            let length = 16 + usize::from(next()) * 3;
            for _ in 0..length {
                payload.push(next());
            }
            let _ = decode_mesh(&payload);
        }
    }

    #[test]
    fn every_truncation_of_a_valid_header_is_refused() {
        let payload = header(2, 2, 1);
        for length in 0..payload.len() {
            assert!(
                matches!(
                    decode_mesh(&payload[..length]),
                    Err(DracoError::TooShort { .. })
                ),
                "truncation to {length} bytes must be named as too short"
            );
        }
    }
}
