use std::collections::{HashMap, HashSet};

pub const DEFAULT_JOINT_SEARCH_CM: f32 = 1.0;

const MAX_PARTNERS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StyleJoint {
    pub a: u32,
    pub b: u32,
    pub distance_m: f32,
    pub closeness: f32,
}

pub type StyleJointGroups = Vec<Vec<StyleJoint>>;

#[must_use]
pub fn build_style_joints(strands: &[&[[f32; 3]]], search_cm: f32) -> StyleJointGroups {
    let search = search_cm.max(0.0);
    if search <= 0.0 {
        return Vec::new();
    }
    let mut points: Vec<([f32; 3], usize)> = Vec::new();
    for (strand, curve) in strands.iter().enumerate() {
        for point in curve.iter() {
            points.push((*point, strand));
        }
    }

    let cell_of = |point: [f32; 3]| -> [i32; 3] {
        [
            (point[0] / search).floor() as i32,
            (point[1] / search).floor() as i32,
            (point[2] / search).floor() as i32,
        ]
    };
    let mut grid: HashMap<[i32; 3], Vec<u32>> = HashMap::new();
    for (index, (point, _)) in points.iter().enumerate() {
        grid.entry(cell_of(*point)).or_default().push(index as u32);
    }

    let mut pairs: HashSet<(u32, u32)> = HashSet::new();
    let mut ordered: Vec<StyleJoint> = Vec::new();
    let mut neighbours: Vec<(f32, u32)> = Vec::new();
    for (index, (point, strand)) in points.iter().enumerate() {
        if is_root(strands, index) {
            continue;
        }
        neighbours.clear();
        let cell = cell_of(*point);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let Some(bucket) = grid.get(&[cell[0] + dx, cell[1] + dy, cell[2] + dz]) else {
                        continue;
                    };
                    for other in bucket {
                        let other_index = *other as usize;
                        if other_index == index
                            || points[other_index].1 == *strand
                            || is_root(strands, other_index)
                        {
                            continue;
                        }
                        let distance = distance(*point, points[other_index].0);
                        if distance < search && distance > 0.0 {
                            neighbours.push((distance, *other));
                        }
                    }
                }
            }
        }
        neighbours.sort_by(|left, right| left.0.total_cmp(&right.0));
        for (distance, other) in neighbours.iter().take(MAX_PARTNERS) {
            let key = if index as u32 <= *other {
                (*other, index as u32)
            } else {
                (index as u32, *other)
            };
            if pairs.insert(key) {
                ordered.push(StyleJoint {
                    a: key.0,
                    b: key.1,
                    distance_m: distance / 100.0,
                    closeness: (search - distance) / search,
                });
            }
        }
    }

    group_for_parallel_solve(ordered)
}

fn is_root(strands: &[&[[f32; 3]]], flat: usize) -> bool {
    let mut start = 0;
    for curve in strands {
        if flat == start {
            return true;
        }
        start += curve.len();
        if flat < start {
            return false;
        }
    }
    false
}

fn distance(left: [f32; 3], right: [f32; 3]) -> f32 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    let dz = left[2] - right[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn group_for_parallel_solve(joints: Vec<StyleJoint>) -> StyleJointGroups {
    let mut groups: Vec<Vec<StyleJoint>> = Vec::new();
    let mut claimed: Vec<HashSet<u32>> = Vec::new();
    for joint in joints {
        let slot = claimed
            .iter()
            .position(|taken| !taken.contains(&joint.a) && !taken.contains(&joint.b));
        let slot = match slot {
            Some(slot) => slot,
            None => {
                groups.push(Vec::new());
                claimed.push(HashSet::new());
                groups.len() - 1
            }
        };
        claimed[slot].insert(joint.a);
        claimed[slot].insert(joint.b);
        groups[slot].push(joint);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearby_points_on_different_strands_are_tied() {
        let left: Vec<[f32; 3]> = (0..4).map(|i| [0.0, i as f32, 0.0]).collect();
        let right: Vec<[f32; 3]> = (0..4).map(|i| [0.5, i as f32, 0.0]).collect();
        let groups = build_style_joints(&[&left, &right], 1.0);
        let joints: Vec<StyleJoint> = groups.iter().flatten().copied().collect();
        assert_eq!(joints.len(), 3, "three non-root pairs: {joints:?}");
        for joint in &joints {
            assert!(joint.a != 0 && joint.a != 4 && joint.b != 0 && joint.b != 4);
            assert!((joint.distance_m - 0.005).abs() < 1.0e-6, "{joint:?}");
            assert!((joint.closeness - 0.5).abs() < 1.0e-5, "{joint:?}");
        }
    }

    #[test]
    fn no_group_names_a_point_twice() {
        let strands: Vec<Vec<[f32; 3]>> = (0..6)
            .map(|s| {
                (0..5)
                    .map(|p| [s as f32 * 0.3, p as f32 * 0.3, 0.0])
                    .collect()
            })
            .collect();
        let borrowed: Vec<&[[f32; 3]]> = strands.iter().map(Vec::as_slice).collect();
        let groups = build_style_joints(&borrowed, 1.0);
        assert!(!groups.is_empty());
        for group in &groups {
            let mut seen = HashSet::new();
            for joint in group {
                assert!(seen.insert(joint.a), "point {} twice in a group", joint.a);
                assert!(seen.insert(joint.b), "point {} twice in a group", joint.b);
            }
        }
    }

    #[test]
    fn strands_beyond_the_search_stay_free() {
        let left: Vec<[f32; 3]> = (0..4).map(|i| [0.0, i as f32, 0.0]).collect();
        let right: Vec<[f32; 3]> = (0..4).map(|i| [50.0, i as f32, 0.0]).collect();
        assert!(build_style_joints(&[&left, &right], 1.0).is_empty());
    }

    #[test]
    fn a_point_takes_at_most_four_partners() {
        let strands: Vec<Vec<[f32; 3]>> = (0..12)
            .map(|s| {
                let angle = s as f32 * 0.5;
                (0..3)
                    .map(|p| [angle.cos() * 0.2, p as f32 * 0.4, angle.sin() * 0.2])
                    .collect()
            })
            .collect();
        let borrowed: Vec<&[[f32; 3]]> = strands.iter().map(Vec::as_slice).collect();
        let joints: Vec<StyleJoint> = build_style_joints(&borrowed, 1.0)
            .into_iter()
            .flatten()
            .collect();
        let mut per_point: HashMap<u32, usize> = HashMap::new();
        for joint in &joints {
            *per_point.entry(joint.a).or_default() += 1;
            *per_point.entry(joint.b).or_default() += 1;
        }
        for (point, count) in per_point {
            assert!(count <= MAX_PARTNERS * 4, "point {point} took {count}");
        }
    }
}
