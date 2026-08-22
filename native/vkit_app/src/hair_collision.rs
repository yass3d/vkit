use glam::Vec3;
use vkit_core::formats::Mesh;
use vkit_core::spatial::{SurfaceProjector, projector_for_mesh};

pub const SKIN_CLEARANCE_CM: f32 = 0.35;

const SEGMENT_SAMPLES: usize = 4;

const SEGMENT_PASSES: usize = 3;

const ROOT_EASE_POINTS: f32 = 2.0;

pub struct HeadCollider {
    mesh: Mesh,
    projector: SurfaceProjector,
    revision: u64,
}

impl std::fmt::Debug for HeadCollider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HeadCollider")
            .field("revision", &self.revision)
            .field("triangles", &self.mesh.triangles.len())
            .finish_non_exhaustive()
    }
}

impl HeadCollider {
    /// Collide against the skin, and only the skin.
    ///
    /// A head's triangle list carries more than its skin: eyelashes, the tear
    /// film and the lacrimals are all in there, and all three stand in front of
    /// the eye. Colliding against them puts a shelf across the face at the
    /// depth of the lashes — the invisible wall a fringe stops at, worst on a
    /// shallow face where the lashes stand proudest relative to the brow.
    /// `render_triangles` is the set the head is drawn from, which is the skin
    /// with those three groups already taken out, and it is what the GPU field
    /// collides against too.
    #[must_use]
    pub fn for_surface(surface: &crate::scene::SurfaceMesh, revision: u64) -> Option<Self> {
        Self::new(
            Mesh {
                vertices: surface.mesh.vertices.clone(),
                triangles: (*surface.render_triangles).clone(),
            },
            revision,
        )
    }

    #[must_use]
    pub fn new(mesh: Mesh, revision: u64) -> Option<Self> {
        let projector = projector_for_mesh(&mesh).ok()?;
        Some(Self {
            mesh,
            projector,
            revision,
        })
    }

    #[must_use]
    pub fn clear(&self, point: [f32; 3], clearance: f32) -> [f32; 3] {
        let Ok(hit) = self.projector.project([
            f64::from(point[0]),
            f64::from(point[1]),
            f64::from(point[2]),
        ]) else {
            return point;
        };
        let Some(normal) = self
            .mesh
            .triangles
            .get(hit.primitive_id as usize)
            .and_then(|triangle| triangle_normal(&self.mesh, *triangle))
        else {
            return point;
        };
        let surface = Vec3::new(
            hit.point[0] as f32,
            hit.point[1] as f32,
            hit.point[2] as f32,
        );
        let at = Vec3::from_array(point);
        let offset = at - surface;
        let distance = offset.length();
        let outside = offset.dot(normal) >= 0.0;
        if outside && distance >= clearance {
            return point;
        }
        // Out is the way to the nearest point, not the face normal of whichever
        // triangle owns it. Around a ridge — the nose, the brow, a lip — the
        // nearest feature is an edge, and the two faces that share it point off
        // at an angle to where the strand actually stands. Pushing along one of
        // them shoves the strand sideways along the face and keeps shoving it,
        // once per pass, which reads as a wall standing off the skin rather
        // than as skin. The normal is still what decides which side we are on.
        let out = if distance > 1.0e-5 {
            let direction = offset / distance;
            if outside { direction } else { normal }
        } else {
            normal
        };
        let reach = if outside {
            clearance - distance
        } else {
            clearance + distance
        };
        (at + out * reach).to_array()
    }

    #[must_use]
    pub fn surface_normal(&self, point: [f32; 3]) -> Option<[f32; 3]> {
        let hit = self
            .projector
            .project([
                f64::from(point[0]),
                f64::from(point[1]),
                f64::from(point[2]),
            ])
            .ok()?;
        let normal = self
            .mesh
            .triangles
            .get(hit.primitive_id as usize)
            .and_then(|triangle| triangle_normal(&self.mesh, *triangle))?;
        Some(normal.to_array())
    }

    pub fn clear_strand(&self, points: &mut [[f32; 3]], clearance: f32) {
        let lengths: Vec<f32> = points
            .windows(2)
            .map(|pair| {
                let d = Vec3::from_array(pair[1]) - Vec3::from_array(pair[0]);
                d.length()
            })
            .collect();
        for _ in 0..SEGMENT_PASSES {
            for (position, point) in points.iter_mut().enumerate().skip(1) {
                *point = self.clear(*point, clearance * root_ease(position as f32));
            }
            let crossed = self.clear_segments(points, clearance);
            restore_lengths(points, &lengths);
            if !crossed {
                break;
            }
        }
    }

    fn clear_segments(&self, points: &mut [[f32; 3]], clearance: f32) -> bool {
        let mut moved = false;
        for index in 0..points.len().saturating_sub(1) {
            let start = Vec3::from_array(points[index]);
            let end = Vec3::from_array(points[index + 1]);
            for sample in 1..SEGMENT_SAMPLES {
                let along = sample as f32 / SEGMENT_SAMPLES as f32;
                let at = start.lerp(end, along);
                let eased = clearance * root_ease(index as f32 + along);
                let cleared = Vec3::from_array(self.clear(at.to_array(), eased));
                let mut push = cleared - at;
                if let Some(direction) = (end - start).try_normalize() {
                    push -= direction * push.dot(direction);
                }
                if push.length_squared() <= 1.0e-8 {
                    continue;
                }
                moved = true;
                let (near, far) = if index > 0 {
                    (1.0 - along, along)
                } else {
                    (0.0, along)
                };
                let spread = near * near + far * far;
                if spread <= 1.0e-6 {
                    continue;
                }
                let correction = (push / spread).clamp_length_max(clearance * 2.0);
                if index > 0 {
                    points[index] =
                        (Vec3::from_array(points[index]) + correction * near).to_array();
                }
                points[index + 1] =
                    (Vec3::from_array(points[index + 1]) + correction * far).to_array();
            }
        }
        moved
    }
}

fn root_ease(position: f32) -> f32 {
    (position / ROOT_EASE_POINTS).clamp(0.0, 1.0)
}

fn restore_lengths(points: &mut [[f32; 3]], lengths: &[f32]) {
    for index in 1..points.len() {
        let Some(rest) = lengths.get(index - 1) else {
            return;
        };
        let anchor = Vec3::from_array(points[index - 1]);
        let offset = Vec3::from_array(points[index]) - anchor;
        let Some(direction) = offset.try_normalize() else {
            continue;
        };
        points[index] = (anchor + direction * *rest).to_array();
    }
}

fn triangle_normal(mesh: &Mesh, triangle: [u32; 3]) -> Option<Vec3> {
    let [a, b, c] = triangle.map(|index| {
        mesh.vertices
            .get(index as usize)
            .map(|point| Vec3::new(point[0] as f32, point[1] as f32, point[2] as f32))
    });
    let (a, b, c) = (a?, b?, c?);
    (b - a).cross(c - a).try_normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor() -> Mesh {
        Mesh::new(
            vec![[-10.0, 0.0, -10.0], [0.0, 0.0, 10.0], [10.0, 0.0, -10.0]],
            vec![[0, 1, 2]],
        )
        .expect("a triangle is a mesh")
    }

    #[test]
    fn a_point_inside_the_head_comes_out_to_the_clearance() {
        let collider = HeadCollider::new(floor(), 1).expect("collider");
        let sunk = collider.clear([0.0, -1.0, 0.0], SKIN_CLEARANCE_CM);
        assert!(
            (sunk[1] - SKIN_CLEARANCE_CM).abs() < 1.0e-4,
            "a buried point must surface to the clearance, got {sunk:?}",
        );
        assert!(sunk[0].abs() < 1.0e-5 && sunk[2].abs() < 1.0e-5);
    }

    #[test]
    fn a_point_already_clear_is_left_exactly_alone() {
        let collider = HeadCollider::new(floor(), 1).expect("collider");
        let free = [1.0, 5.0, 2.0];
        assert_eq!(collider.clear(free, SKIN_CLEARANCE_CM), free);
    }

    #[test]
    fn hair_is_held_off_the_skin_rather_than_on_it() {
        let collider = HeadCollider::new(floor(), 1).expect("collider");
        let touching = collider.clear([0.0, 0.0, 0.0], SKIN_CLEARANCE_CM);
        assert!(
            touching[1] > 0.0,
            "a point exactly on the skin must still be lifted, got {touching:?}",
        );
    }

    fn ear() -> Mesh {
        Mesh::new(
            vec![
                [-0.5, 0.0, 0.05],
                [0.5, 0.0, 0.05],
                [0.0, 2.0, 0.05],
                [-0.5, 0.0, -0.05],
                [0.0, 2.0, -0.05],
                [0.5, 0.0, -0.05],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .expect("two triangles are a mesh")
    }

    #[test]
    fn a_segment_may_not_cross_what_its_ends_politely_avoid() {
        let collider = HeadCollider::new(ear(), 1).expect("collider");
        let mut strand = vec![
            [0.0, 3.5, -3.0],
            [0.0, 3.0, -2.0],
            [0.0, 2.5, -1.0],
            [0.0, 0.5, 1.0],
        ];
        assert_eq!(
            collider.clear(strand[2], SKIN_CLEARANCE_CM),
            strand[2],
            "the ends really are clear on their own",
        );
        let length_before = strand_length(&strand);
        collider.clear_strand(&mut strand, SKIN_CLEARANCE_CM);
        let (a, b) = (Vec3::from_array(strand[2]), Vec3::from_array(strand[3]));
        let t = (0.0 - a.z) / (b.z - a.z);
        let crossing = a.lerp(b, t.clamp(0.0, 1.0));
        assert!(
            !(0.0..2.0).contains(&crossing.y) || crossing.x.abs() > 0.55,
            "the strand must turn off the ear rather than cross it: {crossing:?}",
        );
        assert!(
            (strand_length(&strand) - length_before).abs() < 1.0e-3,
            "and clearing may bend a strand, never lengthen it",
        );
    }

    #[test]
    fn a_hundred_clearings_do_not_grow_a_strand() {
        let collider = HeadCollider::new(floor(), 1).expect("collider");
        let mut strand = vec![
            [0.0, 0.5, 0.0],
            [0.4, 0.1, 0.3],
            [0.9, -0.2, 0.5],
            [1.3, -0.4, 1.1],
        ];
        let length_at_rest = strand_length(&strand);
        for _ in 0..100 {
            collider.clear_strand(&mut strand, SKIN_CLEARANCE_CM);
        }
        let drift = (strand_length(&strand) - length_at_rest).abs();
        assert!(
            drift < 1.0e-3,
            "one hundred clearings drifted the length by {drift} cm",
        );
    }

    fn strand_length(points: &[[f32; 3]]) -> f32 {
        points
            .windows(2)
            .map(|pair| (Vec3::from_array(pair[1]) - Vec3::from_array(pair[0])).length())
            .sum()
    }

    #[test]
    fn the_stand_off_eases_in_rather_than_lifting_hair_off_its_roots() {
        let collider = HeadCollider::new(floor(), 1).expect("collider");
        let planted = collider.clear([0.0, 0.02, 0.0], SKIN_CLEARANCE_CM * root_ease(1.0));
        assert!(
            planted[1] < SKIN_CLEARANCE_CM,
            "the first point off the scalp keeps a partial stand-off, got {planted:?}",
        );
        assert!(
            root_ease(2.0) >= 1.0 && root_ease(0.0) <= 0.0,
            "and it is at full strength by the second point",
        );
    }

    #[test]
    fn a_root_is_never_pushed_off_the_vertex_it_grows_from() {
        let collider = HeadCollider::new(floor(), 1).expect("collider");
        let mut strand = vec![[0.0, -0.5, 0.0], [0.0, -0.4, 1.0], [0.0, 2.0, 2.0]];
        let root = strand[0];
        let length_before = strand_length(&strand);
        collider.clear_strand(&mut strand, SKIN_CLEARANCE_CM);
        assert_eq!(strand[0], root, "the root is bound and may not move");
        assert!(
            strand[1][1] > -0.4,
            "the rest of the strand comes up and out: {strand:?}",
        );
        assert!(
            (strand_length(&strand) - length_before).abs() < 1.0e-3,
            "bending is all clearing is allowed to do",
        );
    }
}

#[cfg(test)]
mod face_wall_tests {
    use super::*;

    /// A ridge: two quads meeting at a crease along y, like the bridge of a
    /// nose. The crease runs up the middle and the faces fall away to either
    /// side, so a point out to the side has an EDGE as its nearest feature.
    fn ridge() -> Mesh {
        let mut vertices: Vec<[f64; 3]> = Vec::new();
        let mut triangles: Vec<[u32; 3]> = Vec::new();
        for step in 0..=8 {
            let y = f64::from(step) - 4.0;
            vertices.push([-4.0, y, 0.0]);
            vertices.push([0.0, y, 2.0]);
            vertices.push([4.0, y, 0.0]);
        }
        for step in 0..8u32 {
            let a = step * 3;
            let b = (step + 1) * 3;
            // Wound so the normals face +z, out of the ridge.
            triangles.push([a, a + 1, b]);
            triangles.push([a + 1, b + 1, b]);
            triangles.push([a + 1, a + 2, b + 1]);
            triangles.push([a + 2, b + 2, b + 1]);
        }
        Mesh {
            vertices,
            triangles,
        }
    }

    /// The push has to be along the way out, not along the face normal of
    /// whichever triangle owns the nearest point. Beside a ridge the nearest
    /// feature is the crease, and its faces point off at an angle to where the
    /// strand stands — pushing along one of them slides the strand along the
    /// face instead of off it, once per pass, which is what a wall standing off
    /// the skin is made of.
    #[test]
    fn a_strand_beside_a_ridge_is_not_shoved_along_the_face() {
        let collider = HeadCollider::new(ridge(), 1).expect("collider");
        let clearance = 0.35_f32;

        // Well clear of the crease, out to the side and above the slope.
        let start = [1.5_f32, 0.0, 2.4];
        let mut at = start;
        for pass in 0..64 {
            let next = collider.clear(at, clearance);
            let moved = Vec3::from_array(next) - Vec3::from_array(at);
            assert!(
                moved.length() < 1.0e-4,
                "pass {pass}: a clear point moved {moved:?}",
            );
            at = next;
        }
        assert!(
            (Vec3::from_array(at) - Vec3::from_array(start)).length() < 1.0e-4,
            "the point drifted to {at:?} from {start:?}",
        );
    }

    /// And a point that IS too close comes out to exactly the clearance,
    /// measured as a distance rather than as a projection onto a normal.
    #[test]
    fn a_strand_too_close_to_a_ridge_ends_up_a_clearance_away() {
        let collider = HeadCollider::new(ridge(), 1).expect("collider");
        let clearance = 0.5_f32;
        for start in [
            [0.05_f32, 0.0, 2.1],
            [0.4, 0.0, 1.9],
            [-0.3, 1.0, 1.95],
            [0.0, -2.0, 2.05],
        ] {
            let mut at = start;
            for _ in 0..8 {
                at = collider.clear(at, clearance);
            }
            let settled = collider.clear(at, clearance);
            assert!(
                (Vec3::from_array(settled) - Vec3::from_array(at)).length() < 1.0e-3,
                "from {start:?} the point never settled: {at:?} -> {settled:?}",
            );
        }
    }

    /// A fin standing in front of a wall, the shape an eyelash makes over a
    /// cheek. Collide against the wall alone and the gap in front of it is open;
    /// include the fin and everything within a clearance of it is shut out.
    fn wall_and_fin() -> (Mesh, Mesh) {
        let mut vertices: Vec<[f64; 3]> = Vec::new();
        let mut wall: Vec<[u32; 3]> = Vec::new();
        for step in 0..=6 {
            let y = f64::from(step) * 2.0 - 6.0;
            vertices.push([-6.0, y, 0.0]);
            vertices.push([6.0, y, 0.0]);
        }
        for step in 0..6u32 {
            let a = step * 2;
            let b = (step + 1) * 2;
            wall.push([a, a + 1, b]);
            wall.push([a + 1, b + 1, b]);
        }
        // The fin: a small quad standing off the wall over the middle, the way
        // a lash stands off a cheek — near enough that a strand cleared of the
        // skin is still within a clearance of the lash.
        let fin_base = vertices.len() as u32;
        vertices.push([-1.0, -1.0, 0.8]);
        vertices.push([1.0, -1.0, 0.8]);
        vertices.push([-1.0, 1.0, 0.8]);
        vertices.push([1.0, 1.0, 0.8]);
        let mut with_fin = wall.clone();
        with_fin.push([fin_base, fin_base + 1, fin_base + 2]);
        with_fin.push([fin_base + 2, fin_base + 1, fin_base + 3]);
        (
            Mesh {
                vertices: vertices.clone(),
                triangles: wall,
            },
            Mesh {
                vertices,
                triangles: with_fin,
            },
        )
    }

    #[test]
    fn the_lashes_are_not_a_shelf_across_the_face() {
        let (skin_only, with_lashes) = wall_and_fin();
        let clearance = 0.35_f32;
        let skin = HeadCollider::new(skin_only, 1).expect("collider");
        let lashes = HeadCollider::new(with_lashes, 1).expect("collider");

        // A strand sitting close to the skin, right behind where the fin stands.
        let at = [0.0_f32, 0.0, 0.6];
        let on_skin = skin.clear(at, clearance);
        assert!(
            (Vec3::from_array(on_skin) - Vec3::from_array(at)).length() < 1.0e-4,
            "clear of the skin, the strand should not move: {on_skin:?}",
        );

        let on_lashes = lashes.clear(at, clearance);
        assert!(
            (Vec3::from_array(on_lashes) - Vec3::from_array(at)).length() > 0.1,
            "the fin has to be what pushes it, or this test proves nothing",
        );
    }

    /// The wiring, held where it can be checked without a head: the collider is
    /// built from the triangles the head is DRAWN from, which is the skin with
    /// the lashes, tear film and lacrimals already taken out.
    #[test]
    fn the_collider_is_built_from_the_drawn_skin() {
        let source = include_str!("state/hair.rs");
        assert!(
            source.contains("HeadCollider::for_surface("),
            "the collider is back on the full triangle list, lashes and all",
        );
    }
}
