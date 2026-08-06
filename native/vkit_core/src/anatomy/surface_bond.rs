use std::collections::BTreeMap;

use crate::math::{Mat3, Vec3};

const DEFAULT_NEIGHBOURS: usize = 4;

const FALLOFF: f64 = 2.5;

#[derive(Clone, Copy, Debug)]
struct Seat {
    triangle: [usize; 3],
    barycentric: [f64; 3],

    local_offset: Vec3,

    rest_axes: Mat3,
    rest_scale: f64,
    weight: f64,
}

#[derive(Clone, Debug)]
pub struct SurfaceBond {
    seats: Vec<Vec<Seat>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceBondError {
    DegenerateSurface,

    TriangleOutOfRange,

    SurfaceChanged,
}

#[derive(Clone, Copy, Debug)]
struct Frame {
    origin: Vec3,
    axes: Mat3,
    scale: f64,
}

fn cross(left: Vec3, right: Vec3) -> Vec3 {
    Vec3::new(
        left.y * right.z - left.z * right.y,
        left.z * right.x - left.x * right.z,
        left.x * right.y - left.y * right.x,
    )
}

impl Frame {
    fn of(vertices: &[Vec3], triangle: [usize; 3], barycentric: [f64; 3]) -> Option<Self> {
        let corners = [
            *vertices.get(triangle[0])?,
            *vertices.get(triangle[1])?,
            *vertices.get(triangle[2])?,
        ];
        let first = corners[1] - corners[0];
        let second = corners[2] - corners[0];
        let normal = cross(first, second);
        let (length, area) = (first.norm(), normal.norm());
        if length <= f64::MIN_POSITIVE || area <= f64::MIN_POSITIVE {
            return None;
        }

        let scale = area.sqrt();
        let tangent = first / length;
        let up = normal / area;
        let bitangent = cross(up, tangent);
        Some(Self {
            origin: corners[0] * barycentric[0]
                + corners[1] * barycentric[1]
                + corners[2] * barycentric[2],
            axes: Mat3::from_rows([
                [tangent.x, bitangent.x, up.x],
                [tangent.y, bitangent.y, up.y],
                [tangent.z, bitangent.z, up.z],
            ]),
            scale,
        })
    }

    fn localize(&self, offset: Vec3) -> Vec3 {
        self.axes.transpose().transform_vector(offset) / self.scale
    }

    fn globalize(&self, local: Vec3) -> Vec3 {
        self.axes.transform_vector(local * self.scale)
    }
}

fn closest_barycentric(corners: [Vec3; 3], point: Vec3) -> Option<([f64; 3], f64)> {
    let first = corners[1] - corners[0];
    let second = corners[2] - corners[0];
    let offset = point - corners[0];
    let (a, b, c) = (first.dot(first), first.dot(second), second.dot(second));
    let (d, e) = (first.dot(offset), second.dot(offset));
    let determinant = a * c - b * b;
    if determinant.abs() <= f64::MIN_POSITIVE {
        return None;
    }

    let mut v = (c * d - b * e) / determinant;
    let mut w = (a * e - b * d) / determinant;
    v = v.clamp(0.0, 1.0);
    w = w.clamp(0.0, 1.0);
    if v + w > 1.0 {
        let total = v + w;
        v /= total;
        w /= total;
    }
    let barycentric = [1.0 - v - w, v, w];
    let seat =
        corners[0] * barycentric[0] + corners[1] * barycentric[1] + corners[2] * barycentric[2];
    Some((barycentric, (point - seat).norm()))
}

impl SurfaceBond {
    pub fn bind(
        attached: &[Vec3],
        surface: &[Vec3],
        triangles: &[[usize; 3]],
    ) -> Result<Self, SurfaceBondError> {
        if triangles
            .iter()
            .flatten()
            .any(|index| *index >= surface.len())
        {
            return Err(SurfaceBondError::TriangleOutOfRange);
        }
        let mut seats = Vec::with_capacity(attached.len());
        for vertex in attached {
            let mut candidates = Vec::<(f64, [usize; 3], [f64; 3])>::new();
            for triangle in triangles {
                let corners = [
                    surface[triangle[0]],
                    surface[triangle[1]],
                    surface[triangle[2]],
                ];
                let Some((barycentric, distance)) = closest_barycentric(corners, *vertex) else {
                    continue;
                };
                if candidates.len() < DEFAULT_NEIGHBOURS {
                    candidates.push((distance, *triangle, barycentric));
                    candidates.sort_by(|left, right| left.0.total_cmp(&right.0));
                } else if distance < candidates[DEFAULT_NEIGHBOURS - 1].0 {
                    candidates[DEFAULT_NEIGHBOURS - 1] = (distance, *triangle, barycentric);
                    candidates.sort_by(|left, right| left.0.total_cmp(&right.0));
                }
            }
            if candidates.is_empty() {
                return Err(SurfaceBondError::DegenerateSurface);
            }

            let nearest = candidates[0].0.max(f64::MIN_POSITIVE);
            let mut bonds = Vec::with_capacity(candidates.len());
            for (distance, triangle, barycentric) in candidates {
                let Some(frame) = Frame::of(surface, triangle, barycentric) else {
                    continue;
                };
                let weight = (1.0 - (distance / (nearest * FALLOFF)).min(1.0)).max(0.0);

                let weight = if bonds.is_empty() {
                    weight.max(1.0)
                } else {
                    weight
                };
                if weight <= 0.0 {
                    continue;
                }
                bonds.push(Seat {
                    triangle,
                    barycentric,
                    local_offset: frame.localize(*vertex - frame.origin),
                    rest_axes: frame.axes,
                    rest_scale: frame.scale,
                    weight,
                });
            }
            if bonds.is_empty() {
                return Err(SurfaceBondError::DegenerateSurface);
            }
            seats.push(bonds);
        }
        Ok(Self { seats })
    }

    pub fn len(&self) -> usize {
        self.seats.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seats.is_empty()
    }

    pub fn place(&self, surface: &[Vec3]) -> Result<Vec<Vec3>, SurfaceBondError> {
        self.seats
            .iter()
            .map(|bonds| {
                let mut total = 0.0;
                let mut sum = Vec3::new(0.0, 0.0, 0.0);
                for bond in bonds {
                    let Some(frame) = Frame::of(surface, bond.triangle, bond.barycentric) else {
                        continue;
                    };
                    total += bond.weight;
                    sum += (frame.origin + frame.globalize(bond.local_offset)) * bond.weight;
                }
                if total <= 0.0 {
                    return Err(SurfaceBondError::DegenerateSurface);
                }
                Ok(sum / total)
            })
            .collect()
    }

    pub fn place_rigid(
        &self,
        rest: &[Vec3],
        surface: &[Vec3],
        groups: &[Vec<usize>],
    ) -> Result<Vec<Vec3>, SurfaceBondError> {
        if rest.len() != self.seats.len() {
            return Err(SurfaceBondError::SurfaceChanged);
        }
        let target = self.place(surface)?;
        let mut placed = target.clone();
        for group in groups {
            if group.iter().any(|index| *index >= rest.len()) {
                return Err(SurfaceBondError::SurfaceChanged);
            }
            let Some(motion) = self.group_motion(rest, &target, surface, group) else {
                continue;
            };
            for index in group {
                placed[*index] = motion.apply(rest[*index]);
            }
        }
        Ok(placed)
    }

    fn group_motion(
        &self,
        rest: &[Vec3],
        target: &[Vec3],
        surface: &[Vec3],
        group: &[usize],
    ) -> Option<RigidMotion> {
        let mut change = [[0.0_f64; 3]; 3];
        let (mut total, mut scale) = (0.0_f64, 0.0_f64);
        for index in group {
            for seat in self.seats.get(*index)? {
                let Some(frame) = Frame::of(surface, seat.triangle, seat.barycentric) else {
                    continue;
                };

                let turned = multiply(frame.axes, seat.rest_axes.transpose());
                let rows = turned.rows();
                for row in 0..3 {
                    for column in 0..3 {
                        change[row][column] += rows[row][column] * seat.weight;
                    }
                }
                scale += (frame.scale / seat.rest_scale.max(f64::MIN_POSITIVE)) * seat.weight;
                total += seat.weight;
            }
        }
        if total <= 0.0 {
            return None;
        }
        let rotation = orthonormalize(Mat3::from_rows(change))?;
        let count = group.len() as f64;
        let centroid = |points: &[Vec3]| {
            group
                .iter()
                .fold(Vec3::new(0.0, 0.0, 0.0), |sum, index| sum + points[*index])
                / count
        };
        Some(RigidMotion {
            rotation,
            scale: (scale / total).clamp(0.5, 2.0),
            from: centroid(rest),
            to: centroid(target),
        })
    }
}

fn multiply(left: Mat3, right: Mat3) -> Mat3 {
    let (a, b) = (left.rows(), right.rows());
    Mat3::from_rows(std::array::from_fn(|row| {
        std::array::from_fn(|column| (0..3).map(|inner| a[row][inner] * b[inner][column]).sum())
    }))
}

#[derive(Clone, Copy, Debug)]
struct RigidMotion {
    rotation: Mat3,
    scale: f64,
    from: Vec3,
    to: Vec3,
}

impl RigidMotion {
    fn apply(&self, point: Vec3) -> Vec3 {
        self.to + self.rotation.transform_vector(point - self.from) * self.scale
    }
}

fn orthonormalize(matrix: Mat3) -> Option<Mat3> {
    let mut current = matrix;
    if current.determinant().abs() <= 1.0e-18 {
        return None;
    }
    for _ in 0..24 {
        let inverse_transpose = invert(current)?;
        let (here, inverse) = (current.rows(), inverse_transpose.rows());
        let next = Mat3::from_rows(std::array::from_fn(|row| {
            std::array::from_fn(|column| (here[row][column] + inverse[column][row]) * 0.5)
        }));
        let there = next.rows();
        let change: f64 = (0..3)
            .flat_map(|row| (0..3).map(move |column| (row, column)))
            .map(|(row, column)| (there[row][column] - here[row][column]).abs())
            .fold(0.0, f64::max);
        current = next;
        if change <= 1.0e-15 {
            break;
        }
    }

    (current.is_finite() && current.determinant() > 0.0).then_some(current)
}

fn invert(matrix: Mat3) -> Option<Mat3> {
    let determinant = matrix.determinant();
    if determinant.abs() <= 1.0e-18 {
        return None;
    }
    let m = matrix.rows();
    let cofactor = |row: usize, column: usize| {
        let rows: Vec<usize> = (0..3).filter(|index| *index != row).collect();
        let columns: Vec<usize> = (0..3).filter(|index| *index != column).collect();
        let minor = m[rows[0]][columns[0]] * m[rows[1]][columns[1]]
            - m[rows[0]][columns[1]] * m[rows[1]][columns[0]];
        if (row + column).is_multiple_of(2) {
            minor
        } else {
            -minor
        }
    };

    Some(Mat3::from_rows(std::array::from_fn(|row| {
        std::array::from_fn(|column| cofactor(column, row) / determinant)
    })))
}

pub fn connected_groups(vertex_count: usize, triangles: &[[usize; 3]]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..vertex_count).collect();
    fn find(parent: &mut [usize], mut index: usize) -> usize {
        while parent[index] != index {
            parent[index] = parent[parent[index]];
            index = parent[index];
        }
        index
    }
    for triangle in triangles {
        if triangle.iter().any(|index| *index >= vertex_count) {
            continue;
        }
        let root = find(&mut parent, triangle[0]);
        for corner in &triangle[1..] {
            let other = find(&mut parent, *corner);
            parent[other] = root;
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for index in 0..vertex_count {
        let root = find(&mut parent, index);
        groups.entry(root).or_default().push(index);
    }
    groups.into_values().collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FollowMode {
    Rigid,

    Conforming,
}

#[derive(Clone, Debug)]
pub struct AttachmentRig {
    parts: Vec<Part>,
}

#[derive(Clone, Debug)]
struct Part {
    vertices: Vec<usize>,
    groups: Vec<Vec<usize>>,
    bond: SurfaceBond,
    mode: FollowMode,
}

impl AttachmentRig {
    pub fn build(
        rest: &[Vec3],
        triangles: &[[usize; 3]],
        is_skin: &[bool],
        mode: &[Option<FollowMode>],
    ) -> Result<Self, SurfaceBondError> {
        if is_skin.len() != rest.len() || mode.len() != rest.len() {
            return Err(SurfaceBondError::SurfaceChanged);
        }
        let skin_triangles = triangles
            .iter()
            .copied()
            .filter(|triangle| {
                triangle
                    .iter()
                    .all(|index| is_skin.get(*index) == Some(&true))
            })
            .collect::<Vec<_>>();
        if skin_triangles.is_empty() {
            return Err(SurfaceBondError::DegenerateSurface);
        }

        let components = connected_groups(rest.len(), triangles);
        let mut parts = Vec::new();
        for following in [FollowMode::Rigid, FollowMode::Conforming] {
            let vertices = (0..rest.len())
                .filter(|index| mode[*index] == Some(following))
                .collect::<Vec<_>>();
            if vertices.is_empty() {
                continue;
            }
            let seat = vertices
                .iter()
                .map(|index| rest[*index])
                .collect::<Vec<_>>();
            let bond = SurfaceBond::bind(&seat, rest, &skin_triangles)?;

            let mut local = vec![usize::MAX; rest.len()];
            for (slot, index) in vertices.iter().enumerate() {
                local[*index] = slot;
            }
            let groups = components
                .iter()
                .map(|component| {
                    component
                        .iter()
                        .filter_map(|index| (local[*index] != usize::MAX).then_some(local[*index]))
                        .collect::<Vec<_>>()
                })
                .filter(|group: &Vec<usize>| !group.is_empty())
                .collect();
            parts.push(Part {
                vertices,
                groups,
                bond,
                mode: following,
            });
        }
        Ok(Self { parts })
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    pub fn reseat(&self, rest: &[Vec3], moved: &mut [Vec3]) -> Result<(), SurfaceBondError> {
        if rest.len() != moved.len() {
            return Err(SurfaceBondError::SurfaceChanged);
        }
        for part in &self.parts {
            let seat = part
                .vertices
                .iter()
                .map(|index| rest[*index])
                .collect::<Vec<_>>();
            let placed = match part.mode {
                FollowMode::Conforming => part.bond.place(moved)?,
                FollowMode::Rigid => part.bond.place_rigid(&seat, moved, &part.groups)?,
            };
            for (index, point) in part.vertices.iter().zip(placed) {
                moved[*index] = point;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lid(rows: usize, columns: usize) -> (Vec<Vec3>, Vec<[usize; 3]>) {
        let mut vertices = Vec::new();
        for row in 0..=rows {
            for column in 0..=columns {
                vertices.push(Vec3::new(column as f64, row as f64, 0.0));
            }
        }
        let mut triangles = Vec::new();
        for row in 0..rows {
            for column in 0..columns {
                let corner = row * (columns + 1) + column;
                triangles.push([corner, corner + 1, corner + columns + 2]);
                triangles.push([corner, corner + columns + 2, corner + columns + 1]);
            }
        }
        (vertices, triangles)
    }

    fn lash(at: Vec3) -> Vec<Vec3> {
        vec![
            at,
            at + Vec3::new(0.0, 0.0, 0.4),
            at + Vec3::new(0.1, 0.0, 0.8),
            at + Vec3::new(0.2, 0.0, 1.2),
        ]
    }

    fn apply(points: &[Vec3], transform: impl Fn(Vec3) -> Vec3) -> Vec<Vec3> {
        points.iter().copied().map(transform).collect()
    }

    fn furthest(left: &[Vec3], right: &[Vec3]) -> f64 {
        left.iter()
            .zip(right)
            .map(|(a, b)| (*a - *b).norm())
            .fold(0.0_f64, f64::max)
    }

    #[test]
    fn a_rigid_surface_carries_its_attachment_exactly() {
        let (surface, triangles) = lid(4, 4);
        let attached = lash(Vec3::new(2.0, 2.0, 0.0));
        let bond = SurfaceBond::bind(&attached, &surface, &triangles).unwrap();

        let motion =
            |point: Vec3| Vec3::new(point.x, -point.z, point.y) + Vec3::new(10.0, -3.0, 7.0);
        let placed = bond.place(&apply(&surface, motion)).unwrap();
        assert!(
            furthest(&placed, &apply(&attached, motion)) < 1.0e-9,
            "{placed:?}"
        );
    }

    #[test]
    fn a_lash_turns_with_the_lid_it_grows_from() {
        let (surface, triangles) = lid(4, 4);
        let attached = lash(Vec3::new(2.0, 2.0, 0.0));
        let bond = SurfaceBond::bind(&attached, &surface, &triangles).unwrap();

        let fold = |point: Vec3| Vec3::new(point.x, 2.0 - point.z, point.y - 2.0);
        let placed = bond.place(&apply(&surface, fold)).unwrap();

        let stood_off = attached[3] - attached[0];
        let stands_off = placed[3] - placed[0];
        assert!(stood_off.z > 1.0, "fixture: {stood_off:?}");
        assert!(
            stands_off.y < -1.0 && stands_off.z.abs() < 0.3,
            "the lash did not turn with its lid: {stands_off:?}"
        );
    }

    #[test]
    fn lashes_follow_the_part_of_the_lid_under_each_of_them() {
        let (surface, triangles) = lid(4, 8);
        let inner = lash(Vec3::new(1.0, 2.0, 0.0));
        let outer = lash(Vec3::new(7.0, 2.0, 0.0));
        let attached = [inner.clone(), outer].concat();
        let bond = SurfaceBond::bind(&attached, &surface, &triangles).unwrap();

        let droop = |point: Vec3| {
            let fall = ((point.x - 4.0) / 4.0).max(0.0);
            Vec3::new(point.x, point.y, point.z - fall)
        };
        let placed = bond.place(&apply(&surface, droop)).unwrap();

        assert!(
            (placed[0] - inner[0]).norm() < 1.0e-9,
            "the inner lash should not have moved: {:?}",
            placed[0]
        );
        assert!(
            (placed[4].z + 0.75).abs() < 0.05,
            "the outer lash should have followed the droop: {:?}",
            placed[4]
        );
    }

    #[test]
    fn a_strand_keeps_its_shape_however_the_lid_bends_under_it() {
        let (surface, triangles) = lid(4, 8);

        let attached = (0..5)
            .map(|step| Vec3::new(2.0 + f64::from(step), 2.0, 0.3))
            .collect::<Vec<_>>();
        let bond = SurfaceBond::bind(&attached, &surface, &triangles).unwrap();

        let bend = |point: Vec3| {
            let lift = ((point.x - 2.0) / 4.0).clamp(0.0, 1.0).powi(2);
            Vec3::new(point.x, point.y, point.z + lift * 2.0)
        };
        let bent = apply(&surface, bend);

        let smooth = bond.place(&bent).unwrap();
        let rigid = bond
            .place_rigid(&attached, &bent, &[(0..attached.len()).collect()])
            .unwrap();

        let span = |points: &[Vec3]| {
            points
                .windows(2)
                .map(|pair| (pair[1] - pair[0]).norm())
                .collect::<Vec<_>>()
        };
        let (rest, smoothed, kept) = (span(&attached), span(&smooth), span(&rigid));
        let stretch = |lengths: &[f64]| {
            lengths
                .iter()
                .zip(&rest)
                .map(|(now, was)| (now / was - 1.0).abs())
                .fold(0.0_f64, f64::max)
        };
        assert!(
            stretch(&smoothed) > 0.05,
            "fixture must actually stretch the strand: {smoothed:?}"
        );

        let kept_stretch = kept
            .windows(2)
            .map(|pair| (pair[1] / pair[0] - 1.0).abs())
            .fold(0.0_f64, f64::max);
        assert!(
            kept_stretch < 1.0e-9,
            "a rigid strand must keep its proportions: {kept:?}"
        );
    }

    #[test]
    fn separate_clumps_are_found_as_separate_groups() {
        let groups = connected_groups(6, &[[0, 1, 2], [3, 4, 5]]);
        assert_eq!(groups, vec![vec![0, 1, 2], vec![3, 4, 5]]);
        let joined = connected_groups(5, &[[0, 1, 2], [2, 3, 4]]);
        assert_eq!(joined, vec![vec![0, 1, 2, 3, 4]]);
    }

    #[test]
    fn a_triangle_outside_the_surface_is_refused() {
        let (surface, _) = lid(2, 2);
        let attached = lash(Vec3::new(1.0, 1.0, 0.0));
        assert_eq!(
            SurfaceBond::bind(&attached, &surface, &[[0, 1, 999]]).unwrap_err(),
            SurfaceBondError::TriangleOutOfRange
        );
    }
}
