use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub type Shifts = Vec<Vec<[f32; 3]>>;

#[derive(Clone, Default)]
pub struct SettleSink(Arc<Mutex<HashMap<u64, Shifts>>>);

impl SettleSink {
    pub fn deliver(&self, part_id: u64, shifts: Shifts) {
        if let Ok(mut held) = self.0.lock() {
            held.insert(part_id, shifts);
        }
    }

    #[must_use]
    pub fn take(&self) -> Vec<(u64, Shifts)> {
        self.0
            .lock()
            .map(|mut held| held.drain().collect())
            .unwrap_or_default()
    }
}

impl std::fmt::Debug for SettleSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SettleSink")
    }
}

impl PartialEq for SettleSink {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Clone, Debug)]
pub struct Capture {
    pub part_id: u64,
    pub sink: SettleSink,
}

#[must_use]
pub fn fall(points: &[[f32; 3]], weights: &[f32], pull: f32) -> Option<Vec<[f32; 3]>> {
    if points.len() < 2 || pull <= 0.0 {
        return None;
    }
    let rest: Vec<f32> = points
        .windows(2)
        .map(|pair| span(pair[0], pair[1]).1)
        .collect();
    let topple = topple_from(points[0]);

    let mut moved = points.to_vec();
    let step = (pull / PASSES as f32).clamp(0.0, 1.0);
    for _ in 0..PASSES {
        for index in 1..moved.len() {
            let weight = weights.get(index).copied().unwrap_or(0.0);
            let length = rest[index - 1];
            let anchor = moved[index - 1];
            let (direction, held) = span(anchor, moved[index]);
            if held <= 1.0e-6 {
                moved[index] = [anchor[0], anchor[1] - length, anchor[2]];
                continue;
            }
            if weight <= 0.0 {
                moved[index] = [
                    anchor[0] + direction[0] * length,
                    anchor[1] + direction[1] * length,
                    anchor[2] + direction[2] * length,
                ];
                continue;
            }
            let target = if direction[1] >= 1.0 - 1.0e-3 {
                topple
            } else {
                DOWN
            };
            let turned = [
                direction[0] + (target[0] - direction[0]) * step * weight,
                direction[1] + (target[1] - direction[1]) * step * weight,
                direction[2] + (target[2] - direction[2]) * step * weight,
            ];
            let size =
                (turned[0] * turned[0] + turned[1] * turned[1] + turned[2] * turned[2]).sqrt();
            let turned = if size <= 1.0e-6 {
                topple
            } else {
                [turned[0] / size, turned[1] / size, turned[2] / size]
            };
            moved[index] = [
                anchor[0] + turned[0] * length,
                anchor[1] + turned[1] * length,
                anchor[2] + turned[2] * length,
            ];
        }
    }

    let stirred = moved.iter().zip(points).any(|(now, before)| {
        (now[0] - before[0]).abs() > 1.0e-5
            || (now[1] - before[1]).abs() > 1.0e-5
            || (now[2] - before[2]).abs() > 1.0e-5
    });
    stirred.then_some(moved)
}

const DOWN: [f32; 3] = [0.0, -1.0, 0.0];

fn topple_from(root: [f32; 3]) -> [f32; 3] {
    let flat = (root[0] * root[0] + root[2] * root[2]).sqrt();
    if flat <= 1.0e-4 {
        return [1.0, 0.0, 0.0];
    }
    [root[0] / flat, 0.0, root[2] / flat]
}

fn span(from: [f32; 3], to: [f32; 3]) -> ([f32; 3], f32) {
    let delta = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let length = (delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2]).sqrt();
    if length <= 1.0e-6 {
        return (DOWN, 0.0);
    }
    (
        [delta[0] / length, delta[1] / length, delta[2] / length],
        length,
    )
}

const PASSES: usize = 8;

pub const PULL_CM: f32 = 0.9;

#[derive(Clone, Debug, PartialEq)]
pub enum SettleScope {
    Everything,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sink_hands_over_what_arrived_and_empties_itself() {
        let sink = SettleSink::default();
        assert!(sink.take().is_empty());

        sink.deliver(7, vec![vec![[1.0, 0.0, 0.0]]]);
        let taken = sink.take();
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].0, 7);
        assert!(
            sink.take().is_empty(),
            "the sink kept a copy of what it handed over",
        );
    }

    #[test]
    fn a_second_answer_for_one_part_replaces_the_first() {
        let sink = SettleSink::default();
        sink.deliver(3, vec![vec![[1.0, 0.0, 0.0]]]);
        sink.deliver(3, vec![vec![[2.0, 0.0, 0.0]]]);
        let taken = sink.take();
        assert_eq!(taken.len(), 1, "one part, one answer");
        assert_eq!(taken[0].1[0][0], [2.0, 0.0, 0.0]);
    }

    fn straight_up(points: usize) -> Vec<[f32; 3]> {
        (0..points).map(|step| [0.0, step as f32, 0.0]).collect()
    }

    fn lengths(points: &[[f32; 3]]) -> Vec<f32> {
        points
            .windows(2)
            .map(|pair| {
                let span = [
                    pair[1][0] - pair[0][0],
                    pair[1][1] - pair[0][1],
                    pair[1][2] - pair[0][2],
                ];
                (span[0] * span[0] + span[1] * span[1] + span[2] * span[2]).sqrt()
            })
            .collect()
    }

    #[test]
    fn a_strand_hangs_from_its_root_and_never_grows() {
        let points = straight_up(6);
        let fallen = fall(&points, &[1.0; 6], PULL_CM).expect("it should move");
        assert_eq!(fallen[0], points[0], "the root left the scalp");
        assert!(
            fallen.last().unwrap()[1] < points.last().unwrap()[1],
            "the tip did not come down: {fallen:?}",
        );
        for (before, after) in lengths(&points).into_iter().zip(lengths(&fallen)) {
            assert!(
                (after - before).abs() < 1.0e-3,
                "a segment changed length: {before} to {after}",
            );
        }
    }

    #[test]
    fn only_the_painted_part_of_a_strand_comes_down() {
        let points = straight_up(6);
        let mut weights = vec![0.0; 6];
        weights[4] = 1.0;
        weights[5] = 1.0;
        let fallen = fall(&points, &weights, PULL_CM).expect("it should move");
        for step in 0..3 {
            assert_eq!(fallen[step], points[step], "point {step} moved untouched");
        }
        assert!(
            fallen[5][1] < points[5][1] - 0.1,
            "the painted tip stayed up"
        );
    }

    #[test]
    fn a_dab_that_covers_nothing_changes_nothing() {
        let points = straight_up(6);
        assert!(fall(&points, &[0.0; 6], PULL_CM).is_none());
        assert!(fall(&points, &[1.0; 6], 0.0).is_none());
        assert!(fall(&points[..1], &[1.0], PULL_CM).is_none());
    }

    #[test]
    fn dabs_accumulate_instead_of_snapping_all_the_way_down() {
        let mut points = straight_up(6);
        let first = fall(&points, &[1.0; 6], PULL_CM).expect("one dab");
        let one = points.last().unwrap()[1] - first.last().unwrap()[1];
        points = first;
        for _ in 0..4 {
            points = fall(&points, &[1.0; 6], PULL_CM).expect("another dab");
        }
        let total = straight_up(6).last().unwrap()[1] - points.last().unwrap()[1];
        assert!(
            total > one * 1.5,
            "holding the brush went nowhere: {one:.3} then {total:.3}",
        );
        assert!(one < 5.0, "one dab dropped the whole strand: {one:.3}");
    }

    #[test]
    fn two_sinks_are_only_the_same_sink_when_they_share_one() {
        let sink = SettleSink::default();
        assert_eq!(sink, sink.clone());
        assert_ne!(sink, SettleSink::default());
    }
}
