use std::collections::HashMap;

use crate::math::Vec3;

#[derive(Clone, Debug)]
pub struct MirrorMap {
    partner: Vec<Option<usize>>,
    center_x: f64,
    paired: usize,
}

const MATCH_TOLERANCE: f64 = 1.0e-4;

impl MirrorMap {
    pub fn build(vertices: &[[f64; 3]], center_x: f64) -> Self {
        let (mut low, mut high) = ([f64::MAX; 3], [f64::MIN; 3]);
        for vertex in vertices {
            for axis in 0..3 {
                low[axis] = low[axis].min(vertex[axis]);
                high[axis] = high[axis].max(vertex[axis]);
            }
        }
        let diagonal =
            ((high[0] - low[0]).powi(2) + (high[1] - low[1]).powi(2) + (high[2] - low[2]).powi(2))
                .sqrt();
        let tolerance = (diagonal * MATCH_TOLERANCE).max(f64::MIN_POSITIVE);

        let cell = |point: [f64; 3]| {
            [
                (point[0] / tolerance).floor() as i64,
                (point[1] / tolerance).floor() as i64,
                (point[2] / tolerance).floor() as i64,
            ]
        };
        let mut buckets: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
        for (index, vertex) in vertices.iter().enumerate() {
            buckets.entry(cell(*vertex)).or_default().push(index);
        }

        let mut partner = vec![None; vertices.len()];
        let mut paired = 0;
        for (index, vertex) in vertices.iter().enumerate() {
            let reflected = [2.0 * center_x - vertex[0], vertex[1], vertex[2]];
            let home = cell(reflected);
            let mut best: Option<(f64, usize)> = None;
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        let key = [home[0] + dx, home[1] + dy, home[2] + dz];
                        for candidate in buckets.get(&key).into_iter().flatten() {
                            let other = vertices[*candidate];
                            let distance = ((other[0] - reflected[0]).powi(2)
                                + (other[1] - reflected[1]).powi(2)
                                + (other[2] - reflected[2]).powi(2))
                            .sqrt();
                            if distance <= tolerance
                                && best.is_none_or(|(closest, _)| distance < closest)
                            {
                                best = Some((distance, *candidate));
                            }
                        }
                    }
                }
            }
            if let Some((_, found)) = best {
                partner[index] = Some(found);
                paired += 1;
            }
        }

        Self {
            partner,
            center_x,
            paired,
        }
    }

    pub const fn center_x(&self) -> f64 {
        self.center_x
    }

    pub const fn paired(&self) -> usize {
        self.paired
    }

    pub fn partner_of(&self, vertex: usize) -> Option<usize> {
        self.partner.get(vertex).copied().flatten()
    }

    pub fn partners(&self, triangle: &[u32; 3]) -> Option<[u32; 3]> {
        let mut mirrored = [0_u32; 3];
        for (slot, vertex) in triangle.iter().enumerate() {
            let partner = self.partner_of(*vertex as usize)?;
            mirrored[slot] = u32::try_from(partner).ok()?;
        }
        Some(mirrored)
    }

    pub fn is_usable(&self) -> bool {
        self.paired * 10 >= self.partner.len() * 9
    }

    pub fn symmetrize(&self, displacements: &mut [Vec3], ratio: f64) {
        let ratio = ratio.clamp(0.0, 1.0);
        if ratio <= 0.0 || displacements.len() != self.partner.len() {
            return;
        }
        let original = displacements.to_vec();
        for (index, displacement) in displacements.iter_mut().enumerate() {
            let Some(partner) = self.partner[index] else {
                continue;
            };
            let across = original[partner];

            let mirrored = Vec3::new(-across.x, across.y, across.z);
            let symmetric = (original[index] + mirrored) * 0.5;
            *displacement = original[index] * (1.0 - ratio) + symmetric * ratio;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip() -> Vec<[f64; 3]> {
        vec![
            [-2.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [-1.0, 1.0, 0.5],
            [1.0, 1.0, 0.5],
        ]
    }

    #[test]
    fn every_vertex_finds_the_one_across_from_it() {
        let map = MirrorMap::build(&strip(), 0.0);
        assert_eq!(map.partner[0], Some(4));
        assert_eq!(map.partner[1], Some(3));

        assert_eq!(map.partner[2], Some(2));
        assert_eq!(map.partner[5], Some(6));
        assert!(map.is_usable());
        assert_eq!(map.paired(), 7);
    }

    #[test]
    fn the_ratio_runs_from_untouched_to_fully_mirrored() {
        let map = MirrorMap::build(&strip(), 0.0);

        let field = || {
            let mut field = vec![Vec3::new(0.0, 0.0, 0.0); 7];
            field[1] = Vec3::new(-0.4, 0.2, 0.0);
            field
        };

        let mut untouched = field();
        map.symmetrize(&mut untouched, 0.0);
        assert_eq!(untouched[1], Vec3::new(-0.4, 0.2, 0.0));
        assert_eq!(untouched[3], Vec3::new(0.0, 0.0, 0.0));

        let mut half = field();
        map.symmetrize(&mut half, 0.5);

        assert!(
            (half[1] - Vec3::new(-0.3, 0.15, 0.0)).norm() < 1.0e-12,
            "{:?}",
            half[1]
        );
        assert!(
            (half[3] - Vec3::new(0.1, 0.05, 0.0)).norm() < 1.0e-12,
            "{:?}",
            half[3]
        );

        let mut full = field();
        map.symmetrize(&mut full, 1.0);

        assert!(
            (full[1] - Vec3::new(-0.2, 0.1, 0.0)).norm() < 1.0e-12,
            "{:?}",
            full[1]
        );
        assert!(
            (full[3] - Vec3::new(0.2, 0.1, 0.0)).norm() < 1.0e-12,
            "{:?}",
            full[3]
        );
    }

    #[test]
    fn a_centreline_point_loses_only_its_sideways_motion() {
        let map = MirrorMap::build(&strip(), 0.0);
        let mut field = vec![Vec3::new(0.0, 0.0, 0.0); 7];
        field[2] = Vec3::new(0.3, 0.4, 0.5);
        map.symmetrize(&mut field, 1.0);
        assert!(
            (field[2] - Vec3::new(0.0, 0.4, 0.5)).norm() < 1.0e-12,
            "{:?}",
            field[2]
        );
    }

    #[test]
    fn an_asymmetric_starting_head_is_not_straightened() {
        let map = MirrorMap::build(&strip(), 0.0);
        let mut nothing = vec![Vec3::new(0.0, 0.0, 0.0); 7];
        map.symmetrize(&mut nothing, 1.0);
        assert!(nothing.iter().all(|d| d.norm() < 1.0e-15));
    }

    #[test]
    fn an_unsymmetric_head_reports_itself_unusable() {
        let lopsided = vec![
            [-2.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.37, 0.6, 0.0],
            [1.4, 0.9, 0.0],
        ];
        let map = MirrorMap::build(&lopsided, 0.0);
        assert!(!map.is_usable(), "paired {} of 4", map.paired());
    }
}
