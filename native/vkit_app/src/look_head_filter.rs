use std::collections::VecDeque;

use vkit_core::{
    formats::{DazGeometry, OrderedObjMesh},
    vam::{canonical_head_face_mask, canonical_head_vertex_mask},
};

const NECK_TRANSITION_RINGS: usize = 10;

pub fn filter_appearance_to_head(
    base: &OrderedObjMesh,
    composed: &OrderedObjMesh,
    geometry: &DazGeometry,
) -> Result<OrderedObjMesh, String> {
    base.validate().map_err(|error| error.to_string())?;
    composed.validate().map_err(|error| error.to_string())?;
    if base.vertices.len() != composed.vertices.len()
        || base.vertices.len() != geometry.vertices.len()
        || base.faces != composed.faces
    {
        return Err("appearance head filter requires matching canonical G2 topology".to_owned());
    }

    let head_vertices = canonical_head_vertex_mask(geometry).map_err(|error| error.to_string())?;
    let head_faces = canonical_head_face_mask(geometry).map_err(|error| error.to_string())?;
    let weights = transition_weights(
        geometry.vertices.len(),
        &geometry.faces,
        &head_faces,
        &head_vertices,
    )?;

    let mut filtered = composed.clone();
    for (index, vertex) in filtered.vertices.iter_mut().enumerate() {
        let weight = weights[index];
        let rest = base.vertices[index];
        let source = composed.vertices[index];
        *vertex = [
            rest[0] + (source[0] - rest[0]) * weight,
            rest[1] + (source[1] - rest[1]) * weight,
            rest[2] + (source[2] - rest[2]) * weight,
        ];
    }
    filtered.validate().map_err(|error| error.to_string())?;
    Ok(filtered)
}

fn transition_weights(
    vertex_count: usize,
    faces: &[Vec<u32>],
    head_faces: &[bool],
    head_vertices: &[bool],
) -> Result<Vec<f64>, String> {
    if head_faces.len() != faces.len() || head_vertices.len() != vertex_count {
        return Err("appearance head mask does not match canonical topology".to_owned());
    }
    let mut adjacency = vec![Vec::<usize>::new(); vertex_count];
    let mut boundary = vec![false; vertex_count];
    for (face_index, face) in faces.iter().enumerate() {
        if head_faces[face_index] {
            for edge in 0..face.len() {
                let first = face[edge] as usize;
                let second = face[(edge + 1) % face.len()] as usize;
                if head_vertices[first] && head_vertices[second] {
                    adjacency[first].push(second);
                    adjacency[second].push(first);
                }
            }
        } else {
            for &vertex in face {
                let vertex = vertex as usize;
                if head_vertices[vertex] {
                    boundary[vertex] = true;
                }
            }
        }
    }
    if !boundary.iter().any(|value| *value) {
        return Err("canonical G2 head has no body transition boundary".to_owned());
    }

    let mut distance = vec![usize::MAX; vertex_count];
    let mut queue = VecDeque::new();
    for (index, is_boundary) in boundary.into_iter().enumerate() {
        if is_boundary {
            distance[index] = 0;
            queue.push_back(index);
        }
    }
    while let Some(vertex) = queue.pop_front() {
        let next = distance[vertex].saturating_add(1);
        if next > NECK_TRANSITION_RINGS {
            continue;
        }
        for &neighbor in &adjacency[vertex] {
            if distance[neighbor] > next {
                distance[neighbor] = next;
                queue.push_back(neighbor);
            }
        }
    }

    Ok(head_vertices
        .iter()
        .zip(distance)
        .map(|(is_head, rings)| {
            if !is_head {
                return 0.0;
            }
            if rings == usize::MAX || rings >= NECK_TRANSITION_RINGS {
                return 1.0;
            }
            let t = rings as f64 / NECK_TRANSITION_RINGS as f64;
            t * t * (3.0 - 2.0 * t)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_keeps_body_restores_seam_and_preserves_head_interior() {
        let faces = vec![vec![0, 1, 2], vec![1, 2, 3], vec![3, 4, 5]];
        let head_faces = vec![false, true, true];
        let head_vertices = vec![false, true, true, true, true, true];
        let weights = transition_weights(6, &faces, &head_faces, &head_vertices).unwrap();
        assert_eq!(weights[0], 0.0);
        assert_eq!(weights[1], 0.0);
        assert_eq!(weights[2], 0.0);
        assert!(weights[3] > 0.0);
        assert!(weights[4] > weights[3]);
        assert!(weights[5] > weights[3]);
    }
}
