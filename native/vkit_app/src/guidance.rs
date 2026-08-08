use egui::{CornerRadius, Painter, Rect};

use crate::state::{AppState, ResultState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NextStep {
    ChooseVaMFolder,

    LoadScan,

    PlaceHead,

    PlacePins,

    Generate,
}

pub fn next_step(state: &AppState) -> Option<NextStep> {
    if state.busy() {
        return None;
    }

    if state.workspace.template_geometry.is_none() {
        return state
            .vam_root
            .is_none()
            .then_some(NextStep::ChooseVaMFolder);
    }
    if state.scan_path.is_none() {
        return (state.edit_source_mode == crate::state::EditSourceMode::ScanHead)
            .then_some(NextStep::LoadScan);
    }
    if !state.placed_head {
        return Some(NextStep::PlaceHead);
    }
    if state.complete_pairs == 0 {
        return Some(NextStep::PlacePins);
    }
    matches!(state.result, ResultState::Empty | ResultState::Stale { .. })
        .then_some(NextStep::Generate)
}

const PERIOD: f64 = 1.7;

const OVERLAY_PEAK_ALPHA: f32 = 0.3;

/// Washed **over** a control that has already painted, so it must stay light
/// enough to read the label through. Use this whenever the control fills its
/// own rect.
pub fn glow_over(painter: &Painter, rect: Rect, radius: u8, time: f64) {
    let breath = pulse(time);
    if breath <= 0.01 {
        return;
    }
    painter.rect_filled(
        rect,
        CornerRadius::same(radius),
        crate::theme::COLOR_EMPHASIS.gamma_multiply(OVERLAY_PEAK_ALPHA * breath),
    );
}

const PEAK_ALPHA: f32 = 0.5;

pub fn pulse(time: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a phase in 0..1, then a sine of it"
    )]
    let phase = ((time / PERIOD).rem_euclid(1.0)) as f32;
    (1.0 - (phase * std::f32::consts::TAU).cos()) * 0.5
}

/// Laid down behind a control **before** it paints, so it can carry more
/// weight than the wash — nothing is reading through it.
///
/// The name says `under` because getting this backwards is silent: paint it
/// beneath something opaque and it is simply never seen, and the code still
/// reads as if the control glows. That is exactly how the Generate step went
/// its whole life without pulsing once.
///
/// Neither entry point takes a colour. There is exactly one emphasis, and a
/// parameter would only let two sites disagree about what it looks like.
pub fn glow_under(painter: &Painter, rect: Rect, radius: u8, time: f64) {
    let breath = pulse(time);
    if breath <= 0.01 {
        return;
    }

    painter.rect_filled(
        rect,
        CornerRadius::same(radius),
        crate::theme::COLOR_EMPHASIS.gamma_multiply(PEAK_ALPHA * breath),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pulse_breathes_between_nothing_and_full() {
        let samples: Vec<f32> = (0..240)
            .map(|step| pulse(f64::from(step) * PERIOD / 240.0))
            .collect();
        assert!(samples.iter().all(|value| (0.0..=1.0).contains(value)));
        assert!(samples.iter().copied().fold(1.0_f32, f32::min) < 0.01);
        assert!(samples.iter().copied().fold(0.0_f32, f32::max) > 0.99);

        for step in 0..40 {
            let time = f64::from(step) * 0.1;
            assert!((pulse(time) - pulse(time + PERIOD)).abs() < 1.0e-4);
        }
    }

    #[test]
    fn the_pulse_rises_once_and_falls_once() {
        let at = |fraction: f64| pulse(fraction * PERIOD);
        assert!(at(0.0) < at(0.25));
        assert!(at(0.25) < at(0.5));
        assert!(at(0.5) > at(0.75));
        assert!(at(0.75) > at(0.99));
    }
}
