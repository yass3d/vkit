use sha2::{Digest, Sha256};

use super::{FormatError, Result, finite_point_with};

#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<[f64; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    pub fn new(vertices: Vec<[f64; 3]>, triangles: Vec<[u32; 3]>) -> Result<Self> {
        if vertices.is_empty() {
            return Err(FormatError::InvalidMesh(
                "at least one vertex is required".to_owned(),
            ));
        }
        if vertices.len() > u32::MAX as usize {
            return Err(FormatError::InvalidMesh(
                "vertex count exceeds the supported u32 index range".to_owned(),
            ));
        }
        for (index, vertex) in vertices.iter().copied().enumerate() {
            finite_point_with(vertex, || format!("vertex {index}"))?;
        }
        validate_triangle_indices(vertices.len(), &triangles)?;
        Ok(Self {
            vertices,
            triangles,
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.vertices.is_empty() {
            return Err(FormatError::InvalidMesh(
                "at least one vertex is required".to_owned(),
            ));
        }
        for (index, vertex) in self.vertices.iter().copied().enumerate() {
            finite_point_with(vertex, || format!("vertex {index}"))?;
        }
        validate_triangle_indices(self.vertices.len(), &self.triangles)
    }

    pub fn require_surface(&self) -> Result<()> {
        self.validate()?;
        if self.triangles.is_empty() {
            return Err(FormatError::InvalidMesh(
                "at least one triangle is required for a surface".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn canonical_hash(&self) -> Result<[u8; 32]> {
        canonical_mesh_hash(&self.vertices, &self.triangles)
    }

    pub fn canonical_hash_hex(&self) -> Result<String> {
        canonical_mesh_hash_hex(&self.vertices, &self.triangles)
    }
}

fn validate_triangle_indices(vertex_count: usize, triangles: &[[u32; 3]]) -> Result<()> {
    for (triangle_id, triangle) in triangles.iter().enumerate() {
        for &vertex_id in triangle {
            if vertex_id as usize >= vertex_count {
                return Err(FormatError::InvalidMesh(format!(
                    "triangle {triangle_id} references vertex {vertex_id}, but the mesh has {vertex_count} vertices"
                )));
            }
        }
    }
    Ok(())
}

pub fn canonical_mesh_hash(vertices: &[[f64; 3]], triangles: &[[u32; 3]]) -> Result<[u8; 32]> {
    if vertices.is_empty() {
        return Err(FormatError::InvalidMesh(
            "at least one vertex is required".to_owned(),
        ));
    }
    for (index, vertex) in vertices.iter().copied().enumerate() {
        finite_point_with(vertex, || format!("vertex {index}"))?;
    }
    validate_triangle_indices(vertices.len(), triangles)?;

    let mut digest = Sha256::new();

    digest.update(b"vkit.canonical_mesh.v1\0");
    digest.update((vertices.len() as u64).to_le_bytes());
    digest.update((triangles.len() as u64).to_le_bytes());
    for vertex in vertices {
        for &coordinate in vertex {
            let normalized = if coordinate == 0.0 { 0.0 } else { coordinate };
            digest.update(normalized.to_bits().to_le_bytes());
        }
    }
    for triangle in triangles {
        for &index in triangle {
            digest.update((index as i64).to_le_bytes());
        }
    }
    Ok(digest.finalize().into())
}

pub fn canonical_mesh_hash_hex(vertices: &[[f64; 3]], triangles: &[[u32; 3]]) -> Result<String> {
    let hash = canonical_mesh_hash(vertices, triangles)?;
    let mut encoded = String::with_capacity(64);
    for byte in hash {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(encoded)
}
