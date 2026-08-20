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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SweepInput {
    pub active: bool,
    pub key_pressed: bool,
    pub primary_pressed: bool,
    pub can_start: bool,
}

pub const fn sweep_phase(input: SweepInput) -> SweepPhase {
    if input.active {
        if input.key_pressed || input.primary_pressed {
            SweepPhase::Finish
        } else {
            SweepPhase::Update
        }
    } else if input.key_pressed && input.can_start {
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

const PRESS_SPENT_ID: &str = "vkit.sweep.press-spent";

pub const fn sweep_spends_press(finished: bool, primary_pressed: bool) -> bool {
    finished && primary_pressed
}

pub fn press_spent(ui: &Ui) -> bool {
    ui.data(|data| data.get_temp::<bool>(Id::new(PRESS_SPENT_ID)))
        .unwrap_or(false)
}

pub fn spend_press(ui: &Ui) {
    ui.data_mut(|data| data.insert_temp(Id::new(PRESS_SPENT_ID), true));
}

pub const fn spent_press_settled(primary_down: bool, clicked: bool) -> bool {
    !primary_down && !clicked
}

pub fn settle_press(ui: &Ui) {
    if !press_spent(ui) {
        return;
    }
    let (primary_down, clicked) = ui.input(|input| {
        (
            input.pointer.button_down(egui::PointerButton::Primary),
            input.pointer.any_click(),
        )
    });
    if spent_press_settled(primary_down, clicked) {
        ui.data_mut(|data| data.remove::<bool>(Id::new(PRESS_SPENT_ID)));
    }
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
    let active = ui.data(|data| data.get_temp::<Sweep>(id)).is_some();
    let armed_pass_id = id.with("armed-pass");
    let this_pass = ui.ctx().cumulative_pass_nr();
    let armed_this_pass = ui.data(|data| data.get_temp::<u64>(armed_pass_id)) == Some(this_pass);
    let key_pressed = key_pressed && !(active && armed_this_pass);
    let value_now = || {
        pointer.and_then(|pointer| {
            ui.data(|data| data.get_temp::<Sweep>(id))
                .map(|sweep| swept_value(sweep, pointer, sensitivity, range.clone()))
        })
    };
    match sweep_phase(SweepInput {
        active,
        key_pressed,
        primary_pressed,
        can_start,
    }) {
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
                data.insert_temp(armed_pass_id, this_pass);
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
            if sweep_spends_press(true, primary_pressed) {
                spend_press(ui);
            }
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

    const RUNNING: SweepInput = SweepInput {
        active: true,
        key_pressed: false,
        primary_pressed: false,
        can_start: true,
    };

    #[test]
    fn the_key_and_a_click_both_end_it() {
        assert_eq!(
            sweep_phase(SweepInput {
                key_pressed: true,
                ..RUNNING
            }),
            SweepPhase::Finish,
            "the arming key must also disarm"
        );
        assert_eq!(
            sweep_phase(SweepInput {
                primary_pressed: true,
                ..RUNNING
            }),
            SweepPhase::Finish,
            "a click must disarm"
        );
        assert_eq!(sweep_phase(RUNNING), SweepPhase::Update);
    }

    #[test]
    fn letting_the_key_go_leaves_the_sweep_where_it_is() {
        assert_eq!(sweep_phase(RUNNING), SweepPhase::Update);
    }

    #[test]
    fn it_cannot_be_armed_while_something_is_being_typed_into() {
        let idle = SweepInput {
            active: false,
            ..RUNNING
        };
        assert_eq!(
            sweep_phase(SweepInput {
                key_pressed: true,
                can_start: false,
                ..idle
            }),
            SweepPhase::Idle
        );
        assert_eq!(
            sweep_phase(SweepInput {
                key_pressed: true,
                ..idle
            }),
            SweepPhase::Start
        );
        assert_eq!(
            sweep_phase(SweepInput {
                primary_pressed: true,
                ..idle
            }),
            SweepPhase::Idle
        );
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
            let phase = sweep_phase(SweepInput {
                active,
                key_pressed: key,
                primary_pressed: click,
                ..RUNNING
            });
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
