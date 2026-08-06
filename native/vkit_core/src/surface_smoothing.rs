use rayon::prelude::*;
use thiserror::Error;

pub const VAM_SMOOTH_PASSES_MAX: u8 = 4;

pub const VAM_SMOOTH_PASSES_DEFAULT: u8 = 3;

const VAM_HC_BETA: f64 = 0.5;

const PARALLEL_VERTEX_THRESHOLD: usize = 4_096;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum SurfaceSmoothingError {
    #[error("surface smoothing requires at least one vertex")]
    EmptyVertexSet,
    #[error(
        "surface smoothing polygon {polygon} references vertex {vertex}, but only {vertex_count} vertices exist"
    )]
    InvalidVertexIndex {
        polygon: usize,
        vertex: u32,
        vertex_count: usize,
    },
    #[error("surface smoothing neighbor table exceeds the supported u32 range")]
    NeighborTableTooLarge,
    #[error("surface smoothing received {actual} positions for a {expected}-vertex topology")]
    VertexCountMismatch { expected: usize, actual: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SurfaceSmoothingTopology {
    neighbor_offsets: Vec<u32>,
    neighbor_indices: Vec<u32>,
}

impl SurfaceSmoothingTopology {
    pub fn from_polygons<'a>(
        vertex_count: usize,
        polygons: impl IntoIterator<Item = &'a [u32]>,
    ) -> Result<Self, SurfaceSmoothingError> {
        if vertex_count == 0 {
            return Err(SurfaceSmoothingError::EmptyVertexSet);
        }
        let mut rows = vec![Vec::<u32>::new(); vertex_count];
        for (polygon_index, polygon) in polygons.into_iter().enumerate() {
            if polygon.len() < 2 {
                continue;
            }
            for edge_index in 0..polygon.len() {
                let from = polygon[edge_index];
                let to = polygon[(edge_index + 1) % polygon.len()];
                if from as usize >= vertex_count {
                    return Err(SurfaceSmoothingError::InvalidVertexIndex {
                        polygon: polygon_index,
                        vertex: from,
                        vertex_count,
                    });
                }
                if to as usize >= vertex_count {
                    return Err(SurfaceSmoothingError::InvalidVertexIndex {
                        polygon: polygon_index,
                        vertex: to,
                        vertex_count,
                    });
                }
                if from != to {
                    rows[from as usize].push(to);
                    rows[to as usize].push(from);
                }
            }
        }

        let mut neighbor_offsets = Vec::with_capacity(vertex_count + 1);
        let mut neighbor_indices = Vec::new();
        neighbor_offsets.push(0);
        for row in &mut rows {
            row.sort_unstable();
            row.dedup();
            neighbor_indices.extend_from_slice(row);
            neighbor_offsets.push(
                u32::try_from(neighbor_indices.len())
                    .map_err(|_| SurfaceSmoothingError::NeighborTableTooLarge)?,
            );
        }
        Ok(Self {
            neighbor_offsets,
            neighbor_indices,
        })
    }

    pub fn from_triangles(
        vertex_count: usize,
        triangles: &[[u32; 3]],
    ) -> Result<Self, SurfaceSmoothingError> {
        Self::from_polygons(vertex_count, triangles.iter().map(<[u32; 3]>::as_slice))
    }

    pub fn vertex_count(&self) -> usize {
        self.neighbor_offsets.len().saturating_sub(1)
    }

    pub fn revision(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for value in self
            .neighbor_offsets
            .iter()
            .chain(self.neighbor_indices.iter())
        {
            hash ^= u64::from(*value);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    pub fn neighbors(&self, vertex: usize) -> &[u32] {
        let Some((&start, &end)) = self
            .neighbor_offsets
            .get(vertex)
            .zip(self.neighbor_offsets.get(vertex + 1))
        else {
            return &[];
        };
        &self.neighbor_indices[start as usize..end as usize]
    }

    pub fn smooth_into<'a>(
        &self,
        input: &[[f64; 3]],
        passes: u8,
        scratch: &'a mut SurfaceSmoothingScratch,
    ) -> Result<&'a [[f64; 3]], SurfaceSmoothingError> {
        let vertex_count = self.vertex_count();
        if input.len() != vertex_count {
            return Err(SurfaceSmoothingError::VertexCountMismatch {
                expected: vertex_count,
                actual: input.len(),
            });
        }

        scratch.positions.clear();
        scratch.positions.extend_from_slice(input);
        scratch.laplacian.resize(vertex_count, [0.0; 3]);
        scratch.differences.resize(vertex_count, [0.0; 3]);

        for _ in 0..passes.min(VAM_SMOOTH_PASSES_MAX) {
            if vertex_count >= PARALLEL_VERTEX_THRESHOLD {
                let positions = scratch.positions.as_slice();
                scratch
                    .laplacian
                    .par_iter_mut()
                    .enumerate()
                    .for_each(|(vertex, output)| {
                        *output = neighbor_average(self.neighbors(vertex), positions)
                            .unwrap_or(positions[vertex]);
                    });

                let laplacian = scratch.laplacian.as_slice();
                scratch
                    .differences
                    .par_iter_mut()
                    .enumerate()
                    .for_each(|(vertex, output)| {
                        *output = subtract(laplacian[vertex], positions[vertex]);
                    });

                let differences = scratch.differences.as_slice();
                scratch
                    .positions
                    .par_iter_mut()
                    .enumerate()
                    .for_each(|(vertex, output)| {
                        *output = corrected_position(
                            laplacian[vertex],
                            differences[vertex],
                            neighbor_average(self.neighbors(vertex), differences)
                                .unwrap_or(differences[vertex]),
                        );
                    });
            } else {
                for vertex in 0..vertex_count {
                    scratch.laplacian[vertex] =
                        neighbor_average(self.neighbors(vertex), &scratch.positions)
                            .unwrap_or(scratch.positions[vertex]);
                }

                for vertex in 0..vertex_count {
                    scratch.differences[vertex] =
                        subtract(scratch.laplacian[vertex], scratch.positions[vertex]);
                }

                for vertex in 0..vertex_count {
                    let difference = scratch.differences[vertex];
                    scratch.positions[vertex] = corrected_position(
                        scratch.laplacian[vertex],
                        difference,
                        neighbor_average(self.neighbors(vertex), &scratch.differences)
                            .unwrap_or(difference),
                    );
                }
            }
        }

        Ok(scratch.positions.as_slice())
    }
}

#[derive(Debug, Default)]
pub struct SurfaceSmoothingScratch {
    positions: Vec<[f64; 3]>,
    laplacian: Vec<[f64; 3]>,
    differences: Vec<[f64; 3]>,
}

impl SurfaceSmoothingScratch {
    pub fn positions(&self) -> &[[f64; 3]] {
        &self.positions
    }
}

fn neighbor_average(neighbors: &[u32], values: &[[f64; 3]]) -> Option<[f64; 3]> {
    if neighbors.is_empty() {
        return None;
    }
    let mut sum = [0.0; 3];
    for &neighbor in neighbors {
        add_assign(&mut sum, values[neighbor as usize]);
    }
    Some(scale(sum, 1.0 / neighbors.len() as f64))
}

fn corrected_position(
    laplacian: [f64; 3],
    difference: [f64; 3],
    neighbor_difference: [f64; 3],
) -> [f64; 3] {
    let correction = add(
        scale(difference, VAM_HC_BETA),
        scale(neighbor_difference, 1.0 - VAM_HC_BETA),
    );
    subtract(laplacian, correction)
}

fn add(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn add_assign(target: &mut [f64; 3], value: [f64; 3]) {
    target[0] += value[0];
    target[1] += value[1];
    target[2] += value[2];
}

fn subtract(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale(value: [f64; 3], factor: f64) -> [f64; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_edges_do_not_include_a_fan_diagonal() {
        let topology =
            SurfaceSmoothingTopology::from_polygons(4, [[0, 1, 2, 3].as_slice()]).unwrap();
        assert_eq!(topology.neighbors(0), &[1, 3]);
        assert_eq!(topology.neighbors(1), &[0, 2]);
        assert_eq!(topology.neighbors(2), &[1, 3]);
        assert_eq!(topology.neighbors(3), &[0, 2]);
    }

    #[test]
    fn one_pass_matches_vam_hc_beta_half() {
        let topology =
            SurfaceSmoothingTopology::from_polygons(4, [[0, 1, 2, 3].as_slice()]).unwrap();
        let input = [
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 0.0],
            [0.0, 2.0, 0.0],
        ];
        let mut scratch = SurfaceSmoothingScratch::default();
        let output = topology.smooth_into(&input, 1, &mut scratch).unwrap();
        assert_eq!(
            output,
            &[
                [0.5, 0.5, 0.0],
                [1.5, 0.5, 0.0],
                [1.5, 1.5, 0.0],
                [0.5, 1.5, 0.0],
            ]
        );
    }

    #[test]
    fn zero_passes_preserve_the_authoritative_low_poly_positions() {
        let topology = SurfaceSmoothingTopology::from_triangles(3, &[[0, 1, 2]]).unwrap();
        let input = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.25, 0.8, 0.1]];
        let mut scratch = SurfaceSmoothingScratch::default();
        assert_eq!(
            topology.smooth_into(&input, 0, &mut scratch).unwrap(),
            input.as_slice()
        );
    }

    #[test]
    fn pass_count_clamps_to_vam_maximum() {
        let topology = SurfaceSmoothingTopology::from_triangles(3, &[[0, 1, 2]]).unwrap();
        let input = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.25, 0.8, 0.1]];
        let mut maximum = SurfaceSmoothingScratch::default();
        let maximum = topology
            .smooth_into(&input, VAM_SMOOTH_PASSES_MAX, &mut maximum)
            .unwrap()
            .to_vec();
        let mut excessive = SurfaceSmoothingScratch::default();
        assert_eq!(
            topology
                .smooth_into(&input, u8::MAX, &mut excessive)
                .unwrap(),
            maximum.as_slice()
        );
    }

    #[test]
    fn invalid_polygon_index_is_reported() {
        assert_eq!(
            SurfaceSmoothingTopology::from_polygons(3, [[0, 1, 3].as_slice()]),
            Err(SurfaceSmoothingError::InvalidVertexIndex {
                polygon: 0,
                vertex: 3,
                vertex_count: 3,
            })
        );
    }

    #[test]
    fn large_surface_parallel_path_matches_one_pass_reference() {
        let vertex_count = PARALLEL_VERTEX_THRESHOLD;
        let polygon = (0..vertex_count as u32).collect::<Vec<_>>();
        let topology =
            SurfaceSmoothingTopology::from_polygons(vertex_count, [polygon.as_slice()]).unwrap();
        let input = (0..vertex_count)
            .map(|vertex| {
                let angle = vertex as f64 * 0.013;
                [angle.cos() * 2.0, angle.sin(), (angle * 0.37).sin() * 0.2]
            })
            .collect::<Vec<_>>();
        let laplacian = (0..vertex_count)
            .map(|vertex| {
                let previous = input[(vertex + vertex_count - 1) % vertex_count];
                let next = input[(vertex + 1) % vertex_count];
                scale(add(previous, next), 0.5)
            })
            .collect::<Vec<_>>();
        let differences = laplacian
            .iter()
            .zip(input.iter())
            .map(|(&smooth, &original)| subtract(smooth, original))
            .collect::<Vec<_>>();
        let expected = (0..vertex_count)
            .map(|vertex| {
                let neighbor_difference = scale(
                    add(
                        differences[(vertex + vertex_count - 1) % vertex_count],
                        differences[(vertex + 1) % vertex_count],
                    ),
                    0.5,
                );
                corrected_position(laplacian[vertex], differences[vertex], neighbor_difference)
            })
            .collect::<Vec<_>>();
        let mut scratch = SurfaceSmoothingScratch::default();
        let actual = topology.smooth_into(&input, 1, &mut scratch).unwrap();

        for (actual, expected) in actual.iter().zip(expected) {
            for axis in 0..3 {
                assert!((actual[axis] - expected[axis]).abs() <= 1.0e-12);
            }
        }
    }
}
