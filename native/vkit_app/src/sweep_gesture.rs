use std::ops::RangeInclusive;

use egui::{Id, Pos2, Rect, Ui};

use crate::shortcuts::Shortcut;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sweep {
    pub start_pointer: Pos2,
    pub start_value: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SweepPhase {
    Idle,
    Start,
    Update,

    Finish,
}

pub const fn sweep_phase(
    active: bool,
    key_pressed: bool,
    primary_pressed: bool,
    can_start: bool,
) -> SweepPhase {
    if active {
        if key_pressed || primary_pressed {
            SweepPhase::Finish
        } else {
            SweepPhase::Update
        }
    } else if key_pressed && can_start {
        SweepPhase::Start
    } else {
        SweepPhase::Idle
    }
}

pub fn swept_value(
    sweep: Sweep,
    pointer: Pos2,
    sensitivity: f32,
    range: Option<RangeInclusive<f32>>,
) -> f32 {
    let travel = pointer.x - sweep.start_pointer.x;
    let value = sweep.start_value + travel * sensitivity;
    match range {
        Some(range) => value.clamp(*range.start(), *range.end()),
        None => value,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SweepUpdate {
    pub consumed: bool,
    pub value: Option<f32>,

    pub finished: bool,
}

pub fn handle_sweep(
    ui: &Ui,
    id: Id,
    arm: Shortcut,
    viewport: Rect,
    current_value: f32,
    sensitivity: f32,
    range: Option<RangeInclusive<f32>>,
) -> SweepUpdate {
    let key_pressed = arm.pressed(ui);
    let (pointer, primary_pressed) = ui.input(|input| {
        (
            input.pointer.hover_pos(),
            input.pointer.button_pressed(egui::PointerButton::Primary),
        )
    });
    let can_start = pointer.is_some_and(|point| viewport.contains(point));
    let active = ui.data(|data| data.get_temp::<Sweep>(id).is_some());
    let value_now = || {
        pointer.and_then(|pointer| {
            ui.data(|data| data.get_temp::<Sweep>(id))
                .map(|sweep| swept_value(sweep, pointer, sensitivity, range.clone()))
        })
    };
    match sweep_phase(active, key_pressed, primary_pressed, can_start) {
        SweepPhase::Idle => SweepUpdate::default(),
        SweepPhase::Start => {
            ui.data_mut(|data| {
                data.insert_temp(
                    id,
                    Sweep {
                        start_pointer: pointer.unwrap_or_else(|| viewport.center()),
                        start_value: current_value,
                    },
                );
            });
            ui.ctx().request_repaint();
            SweepUpdate {
                consumed: true,
                value: None,
                finished: false,
            }
        }
        SweepPhase::Update => {
            let value = value_now();
            ui.ctx().request_repaint();
            SweepUpdate {
                consumed: true,
                value,
                finished: false,
            }
        }
        SweepPhase::Finish => {
            let value = value_now();
            ui.data_mut(|data| data.remove::<Sweep>(id));
            SweepUpdate {
                consumed: true,
                value,
                finished: true,
            }
        }
    }
}

pub fn sweep_active(ui: &Ui, id: Id) -> bool {
    ui.data(|data| data.get_temp::<Sweep>(id).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f32) -> Pos2 {
        Pos2::new(x, 100.0)
    }

    #[test]
    fn the_key_and_a_click_both_end_it() {
        assert_eq!(
            sweep_phase(true, true, false, true),
            SweepPhase::Finish,
            "the arming key must also disarm"
        );
        assert_eq!(
            sweep_phase(true, false, true, true),
            SweepPhase::Finish,
            "a click must disarm"
        );
        assert_eq!(sweep_phase(true, false, false, true), SweepPhase::Update);
    }

    #[test]
    fn it_cannot_be_armed_while_something_is_being_typed_into() {
        assert_eq!(sweep_phase(false, true, false, false), SweepPhase::Idle);
        assert_eq!(sweep_phase(false, true, false, true), SweepPhase::Start);

        assert_eq!(sweep_phase(false, false, true, true), SweepPhase::Idle);
    }

    #[test]
    fn the_value_follows_the_pointer_rather_than_accumulating() {
        let sweep = Sweep {
            start_pointer: point(100.0),
            start_value: 0.5,
        };
        assert_eq!(swept_value(sweep, point(100.0), 0.01, None), 0.5);
        assert_eq!(swept_value(sweep, point(200.0), 0.01, None), 1.5);

        assert_eq!(swept_value(sweep, point(100.0), 0.01, None), 0.5);

        assert_eq!(swept_value(sweep, point(50.0), 0.01, None), 0.0);
    }

    #[test]
    fn a_bounded_value_clamps_and_an_unbounded_one_keeps_going() {
        let sweep = Sweep {
            start_pointer: point(0.0),
            start_value: 0.0,
        };
        let bounded = swept_value(sweep, point(10_000.0), 0.01, Some(0.0..=1.0));
        assert_eq!(bounded, 1.0);
        let unbounded = swept_value(sweep, point(10_000.0), 0.01, None);
        assert!(unbounded > 99.0, "an angle was clamped: {unbounded}");
    }

    #[test]
    fn a_whole_gesture_runs_start_update_finish_and_then_stops() {
        let mut active = false;
        let mut phases = Vec::new();

        for (key, click) in [
            (true, false),
            (false, false),
            (false, false),
            (false, true),
            (false, false),
        ] {
            let phase = sweep_phase(active, key, click, true);
            active = match phase {
                SweepPhase::Start | SweepPhase::Update => true,
                SweepPhase::Finish | SweepPhase::Idle => false,
            };
            phases.push(phase);
        }
        assert_eq!(
            phases,
            vec![
                SweepPhase::Start,
                SweepPhase::Update,
                SweepPhase::Update,
                SweepPhase::Finish,
                SweepPhase::Idle,
            ]
        );
    }
}
