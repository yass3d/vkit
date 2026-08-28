use egui::Rect;
use glam::Vec3;

use crate::camera::TurntableCamera;

#[derive(Clone, Debug)]
pub(super) struct MarkerSize {
    share: f32,

    range: std::ops::RangeInclusive<f32>,
}

impl MarkerSize {
    const CROWDED: f32 = 0.25;

    pub(super) fn new(share: f32, range: std::ops::RangeInclusive<f32>) -> Self {
        debug_assert!(
            share <= Self::CROWDED,
            "a share of {share} fills {}% of the gap to the next marker, so the              field is solid however far away it is",
            share * 200.0,
        );
        Self { share, range }
    }

    pub(super) fn points(
        &self,
        camera: TurntableCamera,
        viewport: Rect,
        world: Vec3,
        spacing: f32,
    ) -> f32 {
        let world_per_point = camera
            .world_units_per_point_at(world, viewport.height())
            .max(1.0e-6);
        self.clamp(spacing * self.share / world_per_point)
    }

    pub(super) fn of_screen_spacing(&self, spacing_points: f32) -> f32 {
        self.clamp(spacing_points * self.share)
    }

    pub(super) fn smallest(&self) -> f32 {
        *self.range.start()
    }

    fn clamp(&self, value: f32) -> f32 {
        value.clamp(*self.range.start(), *self.range.end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn viewport() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0))
    }

    #[test]
    fn the_share_decides_the_size_and_not_the_clamp() {
        let head_cm = 25.0_f32;
        for (label, spacing_cm, size, largest) in [
            (
                "scalp socket",
                0.45_f32,
                MarkerSize::new(0.20, 0.3..=3.6),
                [3.4_f32, 1.7, 0.9, 0.5],
            ),
            (
                "stream node",
                0.45,
                MarkerSize::new(0.13, 0.25..=3.0),
                [2.2, 1.1, 0.6, 0.35],
            ),
        ] {
            for (framed_points, largest) in
                [900.0_f32, 450.0, 220.0, 110.0].into_iter().zip(largest)
            {
                let world_per_point = head_cm / framed_points;
                let spacing_points = spacing_cm / world_per_point;
                let wanted = spacing_points * size.share;
                let got = size.of_screen_spacing(spacing_points);
                assert!(
                    (got - wanted).abs() <= wanted * 0.35 || framed_points <= 110.0,
                    "{label} at {framed_points} points of head: the clamp is                      deciding ({got} against a share of {wanted}), so shrinking                      the share will not move it",
                );
                assert!(
                    got <= largest,
                    "{label} at {framed_points} points of head draws {got} points                      across the radius, over the {largest} it is allowed",
                );
            }
        }
    }

    #[test]
    fn a_marker_shrinks_as_its_subject_pulls_away() {
        let size = MarkerSize::new(0.2, 0.2..=40.0);
        let near = TurntableCamera {
            distance: 30.0,
            ..TurntableCamera::default()
        };
        let far = TurntableCamera {
            distance: 300.0,
            ..near
        };
        let at = Vec3::ZERO;
        let close = size.points(near, viewport(), at, 1.0);
        let away = size.points(far, viewport(), at, 1.0);
        assert!(
            away < close,
            "the same point read {close} near and {away} far away",
        );
    }

    #[test]
    fn the_range_holds_at_both_ends() {
        let size = MarkerSize::new(0.25, 1.0..=3.0);
        assert_eq!(size.of_screen_spacing(0.0), 1.0, "under the floor");
        assert_eq!(size.of_screen_spacing(100.0), 3.0, "over the ceiling");
        assert_eq!(size.of_screen_spacing(8.0), 2.0, "inside, it is the share");
        assert_eq!(size.smallest(), 1.0);
    }

    #[test]
    fn markers_leave_room_between_themselves_however_far_away_they_are() {
        for spacing in [2.0_f32, 8.0, 40.0, 400.0] {
            let size = MarkerSize::new(MarkerSize::CROWDED, 0.0..=f32::MAX);
            let diameter = size.of_screen_spacing(spacing) * 2.0;
            assert!(
                diameter <= spacing * 0.5,
                "at {spacing} points apart a marker is {diameter} across",
            );
        }
    }
}
