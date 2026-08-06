use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use super::{FormatError, Mesh, Result, finite_point_with};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjFace {
    pub vertex_indices: Vec<u32>,
    pub group: Option<String>,
    pub material: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrderedObjMesh {
    pub vertices: Vec<[f64; 3]>,
    pub faces: Vec<ObjFace>,
}

impl OrderedObjMesh {
    pub fn validate(&self) -> Result<()> {
        if self.vertices.is_empty() {
            return Err(FormatError::InvalidMesh(
                "OBJ contains no vertices".to_owned(),
            ));
        }
        for (index, vertex) in self.vertices.iter().copied().enumerate() {
            finite_point_with(vertex, || format!("OBJ vertex {index}"))?;
        }
        for (face_id, face) in self.faces.iter().enumerate() {
            if face.vertex_indices.len() < 3 {
                return Err(FormatError::InvalidMesh(format!(
                    "OBJ face {face_id} contains fewer than three vertices"
                )));
            }
            for &index in &face.vertex_indices {
                if index as usize >= self.vertices.len() {
                    return Err(FormatError::InvalidMesh(format!(
                        "OBJ face {face_id} references vertex {index}, but only {} vertices exist",
                        self.vertices.len()
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn triangulated(&self) -> Result<Mesh> {
        self.validate()?;
        let triangle_count = self
            .faces
            .iter()
            .map(|face| face.vertex_indices.len().saturating_sub(2))
            .sum();
        let mut triangles = Vec::with_capacity(triangle_count);
        for face in &self.faces {
            let first = face.vertex_indices[0];
            for corner in 1..face.vertex_indices.len() - 1 {
                triangles.push([
                    first,
                    face.vertex_indices[corner],
                    face.vertex_indices[corner + 1],
                ]);
            }
        }
        Mesh::new(self.vertices.clone(), triangles)
    }

    pub fn polygon_indices(&self) -> impl ExactSizeIterator<Item = &[u32]> {
        self.faces.iter().map(|face| face.vertex_indices.as_slice())
    }
}

pub fn load_ordered_obj(path: impl AsRef<Path>) -> Result<OrderedObjMesh> {
    let stream = BufReader::new(File::open(path)?);
    parse_ordered_obj(stream)
}

pub fn parse_ordered_obj(reader: impl BufRead) -> Result<OrderedObjMesh> {
    let mut vertices = Vec::new();
    let mut faces = Vec::new();
    let mut active_group: Option<String> = None;
    let mut active_material: Option<String> = None;

    for (zero_based_line, line) in reader.lines().enumerate() {
        let line_number = zero_based_line + 1;
        let line = line?;
        let line = if line_number == 1 {
            line.trim_start_matches('\u{feff}')
        } else {
            line.as_str()
        };
        let mut fields = line.split_whitespace();
        let Some(kind) = fields.next() else {
            continue;
        };
        if kind.starts_with('#') {
            continue;
        }
        match kind {
            "v" => {
                let coordinates: Vec<_> = fields.take(3).collect();
                if coordinates.len() != 3 {
                    return Err(obj_error(
                        line_number,
                        "vertex has fewer than three coordinates",
                    ));
                }
                let mut vertex = [0.0; 3];
                for (axis, raw) in coordinates.into_iter().enumerate() {
                    vertex[axis] = raw.parse::<f64>().map_err(|_| {
                        obj_error(line_number, format!("invalid vertex coordinate {raw:?}"))
                    })?;
                }
                if !vertex.iter().all(|value| value.is_finite()) {
                    return Err(obj_error(line_number, "vertex contains a non-finite value"));
                }
                if vertices.len() == u32::MAX as usize {
                    return Err(obj_error(line_number, "OBJ exceeds the u32 vertex limit"));
                }
                vertices.push(vertex);
            }
            "f" => {
                let tokens: Vec<_> = fields.collect();
                if tokens.len() < 3 {
                    return Err(obj_error(line_number, "face has fewer than three corners"));
                }
                let mut indices = Vec::with_capacity(tokens.len());
                for token in tokens {
                    indices.push(resolve_face_index(token, vertices.len(), line_number)?);
                }
                faces.push(ObjFace {
                    vertex_indices: indices,
                    group: active_group.clone(),
                    material: active_material.clone(),
                });
            }
            "g" => {
                let value = fields.collect::<Vec<_>>().join(" ");
                active_group = (!value.is_empty() && value != "off").then_some(value);
            }
            "usemtl" => {
                let value = fields.collect::<Vec<_>>().join(" ");
                active_material = (!value.is_empty() && value != "off").then_some(value);
            }

            _ => {}
        }
    }

    let mesh = OrderedObjMesh { vertices, faces };
    mesh.validate()?;
    Ok(mesh)
}

fn resolve_face_index(token: &str, vertex_count: usize, line: usize) -> Result<u32> {
    let raw = token.split('/').next().unwrap_or_default();
    if raw.is_empty() {
        return Err(obj_error(line, format!("invalid face corner {token:?}")));
    }
    let value = raw
        .parse::<i64>()
        .map_err(|_| obj_error(line, format!("invalid face index {token:?}")))?;
    if value == 0 {
        return Err(obj_error(line, "OBJ face index zero is invalid"));
    }
    let resolved = if value > 0 {
        value - 1
    } else {
        i64::try_from(vertex_count)
            .map_err(|_| obj_error(line, "vertex count exceeds the signed index range"))?
            + value
    };
    if resolved < 0 || resolved as usize >= vertex_count {
        return Err(obj_error(
            line,
            format!("face index {value} is outside {vertex_count} vertices"),
        ));
    }
    u32::try_from(resolved)
        .map_err(|_| obj_error(line, "resolved face index exceeds the u32 range"))
}

fn obj_error(line: usize, message: impl Into<String>) -> FormatError {
    FormatError::InvalidObj {
        line,
        message: message.into(),
    }
}

pub fn write_ordered_obj(mut writer: impl Write, mesh: &OrderedObjMesh) -> Result<()> {
    mesh.validate()?;
    writeln!(writer, "# Vkit ordered mesh")?;
    for [x, y, z] in &mesh.vertices {
        writeln!(writer, "v {x} {y} {z}")?;
    }

    let mut current_group: Option<&str> = None;
    let mut current_material: Option<&str> = None;
    for face in &mesh.faces {
        let next_group = face.group.as_deref();
        if next_group != current_group {
            match next_group {
                Some(group) => writeln!(writer, "g {group}")?,
                None => writeln!(writer, "g off")?,
            }
            current_group = next_group;
        }
        let next_material = face.material.as_deref();
        if next_material != current_material {
            match next_material {
                Some(material) => writeln!(writer, "usemtl {material}")?,
                None => writeln!(writer, "usemtl off")?,
            }
            current_material = next_material;
        }
        write!(writer, "f")?;
        for &index in &face.vertex_indices {
            write!(writer, " {}", u64::from(index) + 1)?;
        }
        writeln!(writer)?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WeldReceipt {
    pub merged_vertices: usize,

    pub dropped_faces: usize,
}

impl WeldReceipt {
    #[must_use]
    pub const fn changed(self) -> bool {
        self.merged_vertices > 0 || self.dropped_faces > 0
    }
}

pub fn weld_ordered_obj_vertices(mesh: &mut OrderedObjMesh) -> WeldReceipt {
    let map = weld_position_map(&mesh.vertices);
    let merged = map
        .iter()
        .enumerate()
        .filter(|(index, representative)| **representative != *index as u32)
        .count();
    if merged == 0 {
        return WeldReceipt::default();
    }

    let mut renumbered = vec![u32::MAX; mesh.vertices.len()];
    let mut vertices = Vec::with_capacity(mesh.vertices.len() - merged);
    for (index, representative) in map.iter().enumerate() {
        if *representative == index as u32 {
            renumbered[index] = vertices.len() as u32;
            vertices.push(mesh.vertices[index]);
        }
    }
    let mut dropped_faces = 0;
    let mut faces = Vec::with_capacity(mesh.faces.len());
    for face in mesh.faces.drain(..) {
        let mut corners: Vec<u32> = Vec::with_capacity(face.vertex_indices.len());
        for index in &face.vertex_indices {
            let Some(representative) = map.get(*index as usize).copied() else {
                continue;
            };
            let welded = renumbered[representative as usize];

            if corners.last() != Some(&welded) {
                corners.push(welded);
            }
        }
        if corners.first() == corners.last() && corners.len() > 1 {
            corners.pop();
        }
        if corners.len() < 3 {
            dropped_faces += 1;
            continue;
        }
        faces.push(ObjFace {
            vertex_indices: corners,
            ..face
        });
    }
    mesh.vertices = vertices;
    mesh.faces = faces;
    WeldReceipt {
        merged_vertices: merged,
        dropped_faces,
    }
}

fn weld_position_map(vertices: &[[f64; 3]]) -> Vec<u32> {
    if vertices.is_empty() {
        return Vec::new();
    }
    let mut minimum = [f64::INFINITY; 3];
    let mut maximum = [f64::NEG_INFINITY; 3];
    for vertex in vertices {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(vertex[axis]);
            maximum[axis] = maximum[axis].max(vertex[axis]);
        }
    }
    let diagonal = (0..3)
        .map(|axis| (maximum[axis] - minimum[axis]).powi(2))
        .sum::<f64>()
        .sqrt();
    let tolerance = (diagonal * 1.0e-6).max(1.0e-9);
    let inverse_cell = 1.0 / tolerance;
    let cell_key = |vertex: &[f64; 3]| {
        (
            (vertex[0] * inverse_cell).round() as i64,
            (vertex[1] * inverse_cell).round() as i64,
            (vertex[2] * inverse_cell).round() as i64,
        )
    };
    let tolerance_squared = tolerance * tolerance;
    let mut cells = std::collections::BTreeMap::<(i64, i64, i64), Vec<u32>>::new();
    let mut map = vec![0_u32; vertices.len()];
    for (index, vertex) in vertices.iter().enumerate() {
        let base = cell_key(vertex);
        let mut best: Option<(f64, u32)> = None;
        for dx in -1..=1_i64 {
            for dy in -1..=1_i64 {
                for dz in -1..=1_i64 {
                    let Some(candidates) = cells.get(&(base.0 + dx, base.1 + dy, base.2 + dz))
                    else {
                        continue;
                    };
                    for &candidate in candidates {
                        let other = vertices[candidate as usize];
                        let distance_squared = (0..3)
                            .map(|axis| (vertex[axis] - other[axis]).powi(2))
                            .sum::<f64>();
                        if distance_squared <= tolerance_squared
                            && best
                                .is_none_or(|(best_distance, _)| distance_squared < best_distance)
                        {
                            best = Some((distance_squared, candidate));
                        }
                    }
                }
            }
        }
        if let Some((_, representative)) = best {
            map[index] = representative;
        } else {
            map[index] = index as u32;
            cells.entry(base).or_default().push(index as u32);
        }
    }
    map
}

#[cfg(test)]
mod weld_tests {
    use super::*;

    #[test]
    fn coincident_positions_become_one_vertex() {
        let mut mesh = OrderedObjMesh {
            vertices: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            faces: vec![
                ObjFace {
                    vertex_indices: vec![0, 1, 2],
                    group: Some("Scan".into()),
                    material: Some("Scan".into()),
                },
                ObjFace {
                    vertex_indices: vec![3, 4, 5],
                    group: Some("Scan".into()),
                    material: Some("Scan".into()),
                },
            ],
        };
        let receipt = weld_ordered_obj_vertices(&mut mesh);
        assert_eq!(receipt.merged_vertices, 2);
        assert_eq!(receipt.dropped_faces, 0);
        assert_eq!(mesh.vertices.len(), 4, "the duplicated corners merged");
        assert_eq!(mesh.faces.len(), 2, "both triangles survived");

        let first: std::collections::BTreeSet<u32> =
            mesh.faces[0].vertex_indices.iter().copied().collect();
        let second: std::collections::BTreeSet<u32> =
            mesh.faces[1].vertex_indices.iter().copied().collect();
        assert_eq!(first.intersection(&second).count(), 2);
        mesh.validate().expect("the welded mesh is still valid");
    }

    #[test]
    fn a_face_that_collapses_is_dropped() {
        let mut mesh = OrderedObjMesh {
            vertices: vec![
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            faces: vec![
                ObjFace {
                    vertex_indices: vec![0, 1, 2],
                    group: None,
                    material: None,
                },
                ObjFace {
                    vertex_indices: vec![0, 3, 4],
                    group: None,
                    material: None,
                },
            ],
        };
        let receipt = weld_ordered_obj_vertices(&mut mesh);
        assert_eq!(receipt.dropped_faces, 1);
        assert_eq!(mesh.faces.len(), 1);
    }

    #[test]
    fn a_clean_mesh_is_untouched() {
        let mut mesh = OrderedObjMesh {
            vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            faces: vec![ObjFace {
                vertex_indices: vec![0, 1, 2],
                group: None,
                material: None,
            }],
        };
        let before = mesh.clone();
        let receipt = weld_ordered_obj_vertices(&mut mesh);
        assert!(!receipt.changed());
        assert_eq!(mesh.vertices, before.vertices);
        assert_eq!(mesh.faces.len(), before.faces.len());
    }
}
