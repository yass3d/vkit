use std::collections::HashMap;

use thiserror::Error;

use crate::pixels::RgbaView;
use crate::symmetry::MirrorMap;
use crate::texture_bake::TextureBakeImage;
use crate::vam::{G2UvMapping, UvMaterialRegion};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FaceMirror {
    #[default]
    Off,

    ToNegativeX,

    ToPositiveX,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum FaceMirrorError {
    #[error("the head is not symmetric at rest, so it has no mirror to follow")]
    AsymmetricHead,
    #[error("a UV triangle references a vertex the head does not have")]
    TriangleOutOfRange,
}

#[derive(Clone, Debug)]
pub struct FaceMirrorMap {
    partner: HashMap<usize, (usize, [usize; 3])>,

    center_x: Vec<(usize, f64)>,
    plane_x: f64,
}

impl FaceMirrorMap {
    pub fn build(mapping: &G2UvMapping, vertices: &[[f64; 3]]) -> Result<Self, FaceMirrorError> {
        let plane_x = vertices
            .iter()
            .fold((f64::MAX, f64::MIN), |(low, high), vertex| {
                (low.min(vertex[0]), high.max(vertex[0]))
            });
        let plane_x = (plane_x.0 + plane_x.1) * 0.5;
        let mirror = MirrorMap::build(vertices, plane_x);
        if !mirror.is_usable() {
            return Err(FaceMirrorError::AsymmetricHead);
        }

        let face = |index: usize| {
            mapping
                .triangles
                .get(index)
                .filter(|triangle| triangle.material_region == UvMaterialRegion::Face)
        };
        let mut by_vertices: HashMap<[u32; 3], usize> = HashMap::new();
        let mut center_x = Vec::new();
        for index in 0..mapping.triangles.len() {
            let Some(triangle) = face(index) else {
                continue;
            };
            let mut key = triangle.position_indices;
            key.sort_unstable();
            by_vertices.insert(key, index);
            let mut sum = 0.0;
            for vertex in triangle.position_indices {
                sum += vertices
                    .get(vertex as usize)
                    .ok_or(FaceMirrorError::TriangleOutOfRange)?[0];
            }
            center_x.push((index, sum / 3.0));
        }

        let mut partner = HashMap::new();
        for (index, _) in &center_x {
            let Some(triangle) = face(*index) else {
                continue;
            };
            let Some(mirrored) = mirror.partners(&triangle.position_indices) else {
                continue;
            };
            let mut key = mirrored;
            key.sort_unstable();
            let Some(other) = by_vertices.get(&key).copied() else {
                continue;
            };
            let Some(other_triangle) = face(other) else {
                continue;
            };

            let mut corners = [0_usize; 3];
            let mut matched = true;
            for (slot, vertex) in mirrored.iter().enumerate() {
                match other_triangle
                    .position_indices
                    .iter()
                    .position(|candidate| candidate == vertex)
                {
                    Some(corner) => corners[slot] = corner,
                    None => {
                        matched = false;
                        break;
                    }
                }
            }
            if matched {
                partner.insert(*index, (other, corners));
            }
        }

        Ok(Self {
            partner,
            center_x,
            plane_x,
        })
    }

    pub fn paired(&self) -> usize {
        self.partner.len()
    }

    pub fn is_usable(&self) -> bool {
        self.partner.len() * 10 >= self.center_x.len() * 9
    }

    #[must_use]
    pub fn mirror_uv(
        &self,
        mapping: &G2UvMapping,
        triangle: usize,
        barycentric: [f64; 3],
    ) -> Option<[f32; 2]> {
        let (other, corners) = self.partner.get(&triangle).copied()?;
        let partner = mapping.triangles.get(other)?;

        let mut uv = [0.0_f32; 2];
        for (corner, source) in corners.into_iter().enumerate() {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "barycentric weights are authored at f32 precision"
            )]
            let weight = *barycentric.get(source)? as f32;
            for (axis, value) in uv.iter_mut().enumerate() {
                *value = partner.uvs[corner][axis].mul_add(weight, *value);
            }
        }
        uv.iter().all(|value| value.is_finite()).then_some(uv)
    }

    pub fn apply(
        &self,
        image: &mut TextureBakeImage,
        mapping: &G2UvMapping,
        side: FaceMirror,
    ) -> usize {
        if side == FaceMirror::Off {
            return 0;
        }
        let source = image.rgba8.clone();
        let Ok(view) = RgbaView::new(&source, image.width, image.height) else {
            return 0;
        };
        let mut filled = 0;
        for (index, center) in &self.center_x {
            let replace = match side {
                FaceMirror::ToNegativeX => *center < self.plane_x,
                FaceMirror::ToPositiveX => *center > self.plane_x,
                FaceMirror::Off => false,
            };
            if !replace {
                continue;
            }
            let Some((other, corners)) = self.partner.get(index).copied() else {
                continue;
            };
            let (Some(target), Some(source_triangle)) =
                (mapping.triangles.get(*index), mapping.triangles.get(other))
            else {
                continue;
            };
            crate::texture_bake::raster_face_triangle(
                image,
                target.uvs,
                view,
                [
                    source_triangle.uvs[corners[0]],
                    source_triangle.uvs[corners[1]],
                    source_triangle.uvs[corners[2]],
                ],
            );
            filled += 1;
        }
        filled
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod mirror_point_tests {
    use super::*;
    use crate::vam::{G2UvTriangle, UvMaterialRegion};
    use std::path::PathBuf;

    #[test]
    fn a_point_mirrors_to_its_partner_and_back() {
        let (map, mapping) = symmetric_fixture();
        let (triangle, _) = map.partner.iter().next().expect("a paired triangle");
        let barycentric = [0.6, 0.3, 0.1];
        let there = map
            .mirror_uv(&mapping, *triangle, barycentric)
            .expect("a paired triangle mirrors");

        assert!(there.iter().all(|value| value.is_finite()));
        let here = {
            let corner = mapping.triangles.get(*triangle).expect("triangle");
            [0, 1].map(|axis| {
                corner.uvs[0][axis].mul_add(
                    barycentric[0] as f32,
                    corner.uvs[1][axis].mul_add(
                        barycentric[1] as f32,
                        corner.uvs[2][axis] * barycentric[2] as f32,
                    ),
                )
            })
        };
        assert!(
            (there[0] - here[0]).abs() > 1.0e-6 || (there[1] - here[1]).abs() > 1.0e-6,
            "a mirrored point must land somewhere else: {there:?} against {here:?}"
        );
    }

    #[test]
    fn an_unpaired_triangle_has_no_mirror() {
        let (map, mapping) = symmetric_fixture();
        let orphan = (0..mapping.triangles.len()).find(|index| !map.partner.contains_key(index));
        if let Some(orphan) = orphan {
            assert_eq!(map.mirror_uv(&mapping, orphan, [1.0, 0.0, 0.0]), None);
        }

        assert_eq!(
            map.mirror_uv(&mapping, mapping.triangles.len() + 9, [1.0, 0.0, 0.0]),
            None
        );
    }

    fn symmetric_fixture() -> (FaceMirrorMap, G2UvMapping) {
        let vertices = vec![
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [1.5, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-2.0, 0.0, 0.0],
            [-1.5, 1.0, 0.0],
        ];
        let triangle = |face: u32, positions: [u32; 3], uvs: [[f32; 2]; 3]| G2UvTriangle {
            canonical_face_index: face,
            canonical_triangle_index: face,
            material_region: UvMaterialRegion::Face,
            on_head: true,
            position_indices: positions,
            uvs,
        };
        let mapping = G2UvMapping {
            source_path: PathBuf::from("fixture"),
            coordinate_rms_cm: 0.0,
            coordinate_max_cm: 0.0,
            uncovered_triangles: 0,
            faces: Vec::new(),
            triangles: vec![
                triangle(0, [0, 1, 2], [[0.6, 0.1], [0.9, 0.1], [0.75, 0.6]]),
                triangle(1, [3, 4, 5], [[0.4, 0.1], [0.1, 0.1], [0.25, 0.6]]),
            ],
        };
        let map = FaceMirrorMap::build(&mapping, &vertices).expect("the fixture pairs");

        assert!(
            map.is_usable(),
            "the fixture must actually pair, or these tests prove nothing"
        );
        assert_eq!(map.paired(), 2, "both triangles pair");
        (map, mapping)
    }
}
