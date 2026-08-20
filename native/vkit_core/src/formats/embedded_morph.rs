use super::g2_obj::{G2F_TOPOLOGY_SHA256, topology_digest};
use super::{DazGeometry, FormatError, MorphAuthoring, MorphTarget, Result};

pub const EMBEDDED_MORPH_MAGIC: [u8; 8] = *b"FMEYE001";
pub const EMBEDDED_MORPH_VERSION: u16 = 1;
const HEADER_BYTES: usize = 60;
const DELTA_BYTES: usize = 14;

pub const EMBEDDED_G2F_EYE_CLOSED_BYTES: &[u8] =
    include_bytes!("../../resources/g2f_eye_closed_sparse.fmeye");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmbeddedMorphDelta {
    pub vertex_id: u16,
    pub delta_cm: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct EmbeddedSparseMorph {
    pub vertex_count: u32,
    pub face_count: u32,
    pub topology_sha256: [u8; 32],
    pub zero_tolerance_cm: f32,
    pub deltas: Vec<EmbeddedMorphDelta>,
}

impl EmbeddedSparseMorph {
    pub fn validate(&self) -> Result<()> {
        if self.vertex_count == 0 || self.vertex_count > u16::MAX.into() {
            return Err(resource_error("invalid v1 vertex count"));
        }
        if self.face_count == 0 {
            return Err(resource_error("invalid v1 face count"));
        }
        if !self.zero_tolerance_cm.is_finite() || self.zero_tolerance_cm < 0.0 {
            return Err(resource_error(
                "zero tolerance must be finite and non-negative",
            ));
        }
        let mut previous = None;
        for (entry_id, entry) in self.deltas.iter().enumerate() {
            if u32::from(entry.vertex_id) >= self.vertex_count {
                return Err(resource_error(format!(
                    "entry {entry_id} references vertex {}, but only {} vertices exist",
                    entry.vertex_id, self.vertex_count
                )));
            }
            if previous.is_some_and(|value| entry.vertex_id <= value) {
                return Err(resource_error(
                    "sparse vertex IDs must be unique and strictly increasing",
                ));
            }
            if !entry.delta_cm.iter().all(|value| value.is_finite())
                || entry.delta_cm.iter().all(|value| *value == 0.0)
            {
                return Err(resource_error(format!(
                    "entry {entry_id} must contain a finite nonzero delta"
                )));
            }
            previous = Some(entry.vertex_id);
        }
        Ok(())
    }

    pub fn to_morph_target(
        &self,
        key: impl Into<String>,
        geometry: &DazGeometry,
    ) -> Result<MorphTarget> {
        self.validate()?;
        geometry.validate()?;
        if geometry.vertices.len() != self.vertex_count as usize
            || geometry.faces.len() != self.face_count as usize
        {
            return Err(resource_error(format!(
                "resource expects {}/{} vertices/faces, geometry has {}/{}",
                self.vertex_count,
                self.face_count,
                geometry.vertices.len(),
                geometry.faces.len()
            )));
        }
        let actual_digest = topology_digest(geometry.vertices.len(), &geometry.faces)?;
        if actual_digest != self.topology_sha256 {
            return Err(resource_error(
                "geometry topology does not match the embedded morph binding",
            ));
        }
        let mut dense = vec![[0.0; 3]; geometry.vertices.len()];
        for entry in &self.deltas {
            dense[entry.vertex_id as usize] = entry.delta_cm.map(f64::from);
        }
        MorphTarget::from_dense_deltas(
            key,
            dense,
            geometry.faces.clone(),
            MorphAuthoring {
                zero_tolerance: f64::from(self.zero_tolerance_cm),
                ..MorphAuthoring::UNIT
            },
        )
    }

    fn to_morph_target_for_verified_vertex_order(
        &self,
        key: impl Into<String>,
        geometry: &DazGeometry,
    ) -> Result<MorphTarget> {
        self.validate()?;
        geometry.validate()?;
        if geometry.vertices.len() != self.vertex_count as usize {
            return Err(resource_error(format!(
                "resource expects {} vertices, geometry has {}",
                self.vertex_count,
                geometry.vertices.len()
            )));
        }
        let mut dense = vec![[0.0; 3]; geometry.vertices.len()];
        for entry in &self.deltas {
            dense[entry.vertex_id as usize] = entry.delta_cm.map(f64::from);
        }
        MorphTarget::from_dense_deltas(
            key,
            dense,
            geometry.faces.clone(),
            MorphAuthoring {
                zero_tolerance: f64::from(self.zero_tolerance_cm),
                ..MorphAuthoring::UNIT
            },
        )
    }
}

pub fn embedded_g2f_eye_closed_morph(geometry: &DazGeometry) -> Result<MorphTarget> {
    let resource = decode_embedded_sparse_morph(EMBEDDED_G2F_EYE_CLOSED_BYTES)?;
    if resource.topology_sha256 != G2F_TOPOLOGY_SHA256 {
        return Err(resource_error(
            "embedded eye morph is not bound to canonical G2F topology",
        ));
    }
    resource.to_morph_target("eye_closed_natural", geometry)
}

pub fn embedded_g2f_eye_closed_morph_for_vam_anchor(geometry: &DazGeometry) -> Result<MorphTarget> {
    let resource = decode_embedded_sparse_morph(EMBEDDED_G2F_EYE_CLOSED_BYTES)?;
    if resource.topology_sha256 != G2F_TOPOLOGY_SHA256 {
        return Err(resource_error(
            "embedded eye morph is not bound to canonical G2F topology",
        ));
    }
    resource.to_morph_target_for_verified_vertex_order("eye_closed_natural", geometry)
}

pub fn encode_embedded_sparse_morph(resource: &EmbeddedSparseMorph) -> Result<Vec<u8>> {
    resource.validate()?;
    let entry_count = u32::try_from(resource.deltas.len())
        .map_err(|_| resource_error("sparse entry count exceeds u32"))?;
    let mut output = Vec::with_capacity(HEADER_BYTES + resource.deltas.len() * DELTA_BYTES);
    output.extend_from_slice(&EMBEDDED_MORPH_MAGIC);
    output.extend_from_slice(&EMBEDDED_MORPH_VERSION.to_le_bytes());
    output.extend_from_slice(&0_u16.to_le_bytes());
    output.extend_from_slice(&resource.vertex_count.to_le_bytes());
    output.extend_from_slice(&resource.face_count.to_le_bytes());
    output.extend_from_slice(&resource.topology_sha256);
    output.extend_from_slice(&resource.zero_tolerance_cm.to_bits().to_le_bytes());
    output.extend_from_slice(&entry_count.to_le_bytes());
    for entry in &resource.deltas {
        output.extend_from_slice(&entry.vertex_id.to_le_bytes());
        for coordinate in entry.delta_cm {
            output.extend_from_slice(&coordinate.to_bits().to_le_bytes());
        }
    }
    Ok(output)
}

pub fn decode_embedded_sparse_morph(encoded: &[u8]) -> Result<EmbeddedSparseMorph> {
    if encoded.len() < HEADER_BYTES {
        return Err(resource_error("resource is shorter than the v1 header"));
    }
    if encoded[..8] != EMBEDDED_MORPH_MAGIC {
        return Err(resource_error("resource magic does not match"));
    }
    let version = read_u16(encoded, 8);
    if version != EMBEDDED_MORPH_VERSION {
        return Err(resource_error(format!(
            "unsupported resource version {version}"
        )));
    }
    if read_u16(encoded, 10) != 0 {
        return Err(resource_error("reserved header bits must be zero"));
    }
    let vertex_count = read_u32(encoded, 12);
    let face_count = read_u32(encoded, 16);
    let mut topology_sha256 = [0_u8; 32];
    topology_sha256.copy_from_slice(&encoded[20..52]);
    let zero_tolerance_cm = f32::from_bits(read_u32(encoded, 52));
    let entry_count = read_u32(encoded, 56) as usize;
    let expected = HEADER_BYTES
        .checked_add(
            entry_count
                .checked_mul(DELTA_BYTES)
                .ok_or_else(|| resource_error("entry byte length overflows usize"))?,
        )
        .ok_or_else(|| resource_error("resource byte length overflows usize"))?;
    if encoded.len() != expected {
        return Err(resource_error(format!(
            "resource has {} bytes; header declares {expected}",
            encoded.len()
        )));
    }
    let mut deltas = Vec::with_capacity(entry_count);
    let mut offset = HEADER_BYTES;
    for _ in 0..entry_count {
        let vertex_id = read_u16(encoded, offset);
        offset += 2;
        let mut delta_cm = [0.0; 3];
        for coordinate in &mut delta_cm {
            *coordinate = f32::from_bits(read_u32(encoded, offset));
            offset += 4;
        }
        deltas.push(EmbeddedMorphDelta {
            vertex_id,
            delta_cm,
        });
    }
    let result = EmbeddedSparseMorph {
        vertex_count,
        face_count,
        topology_sha256,
        zero_tolerance_cm,
        deltas,
    };
    result.validate()?;
    Ok(result)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("validated header"),
    )
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated length"),
    )
}

fn resource_error(message: impl Into<String>) -> FormatError {
    FormatError::InvalidEmbeddedMorph(message.into())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::formats::DazGeometry;

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
                indices: vec![0, 0],
                names: vec!["Face".into()],
            },
            json!({}),
        )
        .unwrap()
    }

    #[test]
    fn sparse_codec_round_trip_builds_a_topology_bound_morph() {
        let geometry = geometry();
        let resource = EmbeddedSparseMorph {
            vertex_count: 4,
            face_count: 2,
            topology_sha256: topology_digest(4, &geometry.faces).unwrap(),
            zero_tolerance_cm: 0.001,
            deltas: vec![
                EmbeddedMorphDelta {
                    vertex_id: 1,
                    delta_cm: [0.0, -0.2, 0.0],
                },
                EmbeddedMorphDelta {
                    vertex_id: 3,
                    delta_cm: [0.0, -0.1, 0.0],
                },
            ],
        };
        let encoded = encode_embedded_sparse_morph(&resource).unwrap();
        assert_eq!(encoded.len(), HEADER_BYTES + 2 * DELTA_BYTES);
        let decoded = decode_embedded_sparse_morph(&encoded).unwrap();
        assert_eq!(decoded, resource);
        let morph = decoded.to_morph_target("closed", &geometry).unwrap();
        assert_eq!(morph.compatibility.active_vertex_count, 2);
        assert!((morph.deltas[1][1] + 0.2).abs() < 1.0e-7);
    }

    #[test]
    fn sparse_codec_rejects_wrong_topology_and_noncanonical_order() {
        let geometry = geometry();
        let mut resource = EmbeddedSparseMorph {
            vertex_count: 4,
            face_count: 2,
            topology_sha256: topology_digest(4, &geometry.faces).unwrap(),
            zero_tolerance_cm: 0.001,
            deltas: vec![EmbeddedMorphDelta {
                vertex_id: 1,
                delta_cm: [0.0, -0.2, 0.0],
            }],
        };
        let mut changed = geometry.clone();
        changed.faces[0].swap(1, 2);
        assert!(resource.to_morph_target("closed", &changed).is_err());
        resource.deltas.push(EmbeddedMorphDelta {
            vertex_id: 0,
            delta_cm: [0.0, -0.1, 0.0],
        });
        assert!(resource.validate().is_err());
    }
}
