use super::*;

pub(super) fn handle_sculpt_interaction(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
    input_blocked: bool,
    pane: crate::viewport::ViewPane,
) {
    let pressed = ui.input(|input| input.pointer.button_pressed(PointerButton::Primary));
    if pressed && response.hovered() {
        crate::viewport::claim_stroke_pane(ui, SCULPT_DRAG_ID, pane);
    }
    if !ui.input(|input| input.pointer.button_down(PointerButton::Primary))
        && !ui.input(|input| input.pointer.button_released(PointerButton::Primary))
    {
        crate::viewport::release_stroke_pane(ui, SCULPT_DRAG_ID);
    }
    if !crate::viewport::stroke_pane_gate(ui, SCULPT_DRAG_ID, pane) {
        return;
    }
    let stroke_id = Id::new(SCULPT_DRAG_ID);
    if input_blocked {
        let existed = ui.data_mut(|data| {
            let existed = data.get_temp::<SculptViewportStroke>(stroke_id).is_some();
            data.remove::<SculptViewportStroke>(stroke_id);
            existed
        });
        if existed {
            state.dispatch(Action::EndSculptStroke);
        }
        return;
    }
    if handle_sculpt_brush_size_gesture(ui, state, viewport) {
        return;
    }
    handle_sculpt_brush_hotkeys(ui, state);
    let hit_targets = sculpt_hit_targets(state);
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let primary_pressed = ui.input(|input| input.pointer.button_pressed(PointerButton::Primary));
    let modifiers = ui.input(|input| input.modifiers);
    let press_input_mode = sculpt_input_mode(modifiers, state.sculpt_brush);
    if primary_pressed
        && response.hovered()
        && let Some(pointer) = pointer
        && let Some(ray) = camera.ray_from_screen(pointer, viewport)
        && let Some(hit) = state.sculpt.raycast_visible_with_brush_radius(
            ray.origin.to_array(),
            ray.direction.to_array(),
            hit_targets,
            f64::from(
                camera.world_units_per_point_at(camera.target, viewport.height())
                    * state.sculpt_brush_radius_points.max(1.0),
            ),
        )
    {
        if state.sculpt_brush.edits_geometry() {
            state.dispatch(Action::BeginSculptStroke {
                view_direction_local: Some(ray.direction.to_array().map(f64::from)),
                brush_direction_local: None,
            });
        }
        ui.data_mut(|data| {
            data.insert_temp(
                stroke_id,
                SculptViewportStroke {
                    mask_step_open: false,
                    center_local: hit.point_local,
                    last_pointer: pointer,
                    last_sample_pointer: pointer,
                    distance_since_last_sample: 0.0,
                    smooth_time_accumulator_seconds: if press_input_mode.is_paint_style() {
                        SCULPT_SMOOTH_DAB_INTERVAL_SECONDS
                    } else {
                        0.0
                    },
                    input_mode: press_input_mode,
                },
            );
        });
    }

    let primary_down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));
    let primary_released = ui.input(|input| input.pointer.button_released(PointerButton::Primary));
    if (primary_down || primary_released)
        && let Some(pointer) = pointer
        && let Some(mut stroke) =
            ui.data_mut(|data| data.get_temp::<SculptViewportStroke>(stroke_id))
    {
        let input_mode = stroke.input_mode;
        let mut sample_pointers = sculpt_input_samples(
            input_mode,
            stroke.last_pointer,
            pointer,
            state.sculpt_brush_radius_points,
            &mut stroke.distance_since_last_sample,
        );

        if primary_released
            && pointer.distance(stroke.last_sample_pointer) > f32::EPSILON
            && sample_pointers.last().copied() != Some(pointer)
        {
            sample_pointers.push(pointer);
            stroke.distance_since_last_sample = 0.0;
        }

        let dab_viewport = SculptDabViewport {
            rect: viewport,
            camera,
            visible_targets: hit_targets,
        };
        if input_mode == SculptInputMode::Mask {
            let alt = ui.input(|input| input.modifiers.alt);
            let mut gathered: std::collections::BTreeMap<u32, f32> =
                std::collections::BTreeMap::new();
            for sample_pointer in &sample_pointers {
                for (vertex, weight) in morph_mask_vertices(state, dab_viewport, *sample_pointer) {
                    let entry = gathered.entry(vertex).or_insert(0.0);
                    *entry = entry.max(weight);
                }
            }
            if !gathered.is_empty() {
                let vertices = gathered.into_iter().collect::<Vec<_>>();
                state.dispatch(Action::PaintMorphMask {
                    vertices,
                    target: if alt { 1.0 } else { 0.0 },
                    amount: MASK_DAB_STRENGTH * state.sculpt_strength.clamp(0.01, 1.0),
                    begins_step: !stroke.mask_step_open,
                });
                stroke.mask_step_open = true;
            }
            stroke.last_pointer = pointer;
            ui.data_mut(|data| data.insert_temp(stroke_id, stroke));
            if primary_down {
                ui.ctx().request_repaint();
            }
            return;
        }

        let mut submitted_dab = false;
        for sample_pointer in sample_pointers {
            let pointer_delta = sample_pointer - stroke.last_sample_pointer;
            stroke.last_sample_pointer = sample_pointer;
            if let Some(dab) = make_sculpt_viewport_dab(
                state,
                dab_viewport,
                &mut stroke,
                input_mode,
                sample_pointer,
                pointer_delta,
            ) {
                state.dispatch(Action::SculptDabDeferred(dab));
                submitted_dab = true;
            }
        }

        if input_mode.is_paint_style() {
            let stable_dt = ui.input(|input| input.stable_dt);
            let time_dabs =
                sculpt_smooth_time_dabs(&mut stroke.smooth_time_accumulator_seconds, stable_dt);
            if time_dabs > 0
                && let Some(dab) = make_sculpt_viewport_dab(
                    state,
                    dab_viewport,
                    &mut stroke,
                    input_mode,
                    pointer,
                    Vec2::ZERO,
                )
            {
                for _ in 0..time_dabs {
                    state.dispatch(Action::SculptDabDeferred(dab));
                }
                submitted_dab = true;
            }
        }
        if submitted_dab {
            state.dispatch(Action::SyncSculptPreview);
        }
        stroke.last_pointer = pointer;
        ui.data_mut(|data| data.insert_temp(stroke_id, stroke));
        if primary_down {
            ui.ctx().request_repaint();
        }
    }

    let stroke_finished = (primary_released || !primary_down)
        && ui.data_mut(|data| {
            let existed = data.get_temp::<SculptViewportStroke>(stroke_id).is_some();
            if existed {
                data.remove::<SculptViewportStroke>(stroke_id);
            }
            existed
        });
    if stroke_finished && state.sculpt_brush.edits_geometry() {
        state.dispatch(Action::EndSculptStroke);
    }
}

const MASK_DAB_STRENGTH: f32 = 0.12;

fn morph_mask_vertices(
    state: &AppState,
    viewport: SculptDabViewport,
    pointer: Pos2,
) -> Vec<(u32, f32)> {
    if state.appearance_stack.selected_id.is_none() {
        return Vec::new();
    }
    let Some(ray) = viewport.camera.ray_from_screen(pointer, viewport.rect) else {
        return Vec::new();
    };
    let radius_points = state.sculpt_brush_radius_points.max(1.0);
    let Some(hit) = state.sculpt.raycast_visible_with_brush_radius(
        ray.origin.to_array(),
        ray.direction.to_array(),
        viewport.visible_targets,
        f64::from(
            viewport
                .camera
                .world_units_per_point_at(viewport.camera.target, viewport.rect.height())
                * radius_points,
        ),
    ) else {
        return Vec::new();
    };
    let center = glam::DVec3::from_array(hit.point_local);
    let world_per_point = viewport
        .camera
        .world_units_per_point_at(center.as_vec3(), viewport.rect.height())
        .max(1.0e-8);
    let radius = f64::from(world_per_point * radius_points);
    let falloff = state.sculpt.falloff_preset();
    state
        .sculpt
        .vertices_within(center.to_array(), radius)
        .into_iter()
        .filter_map(|(vertex, distance)| {
            let weight = falloff.weight(distance / radius) as f32;
            (weight > 0.0).then_some((vertex, weight))
        })
        .collect()
}

pub(super) const fn sculpt_brush_hint(brush: SculptBrush) -> &'static str {
    match brush {
        SculptBrush::Move => Shortcut::SculptGrabBrush.label(),
        SculptBrush::Smooth => "Shift",
        SculptBrush::Restore => Shortcut::SculptRestoreBrush.label(),
        SculptBrush::Mask => "Alt",
    }
}

pub(super) fn brush_size_key_step(
    ui: &Ui,
    radius: f32,
    range: std::ops::RangeInclusive<f32>,
) -> Option<f32> {
    if ui.ctx().egui_wants_keyboard_input() {
        return None;
    }
    let shrink = Shortcut::BrushSizeDown.pressed(ui);
    let grow = Shortcut::BrushSizeUp.pressed(ui);

    if shrink == grow {
        return None;
    }
    stepped_brush_radius(radius, grow, range)
}

pub(super) fn stepped_brush_radius(
    radius: f32,
    grow: bool,
    range: std::ops::RangeInclusive<f32>,
) -> Option<f32> {
    const STEP: f32 = 1.15;
    let scaled = if grow { radius * STEP } else { radius / STEP };
    let target = scaled.clamp(*range.start(), *range.end());
    ((target - radius).abs() > f32::EPSILON).then_some(target)
}

pub(super) fn handle_sculpt_brush_hotkeys(ui: &Ui, state: &mut AppState) {
    if ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    let grab = Shortcut::SculptGrabBrush.pressed(ui);
    let restore = Shortcut::SculptRestoreBrush.pressed(ui);
    if grab && state.sculpt_brush != SculptBrush::Move {
        state.dispatch(Action::SetSculptBrush(SculptBrush::Move));
    }
    if restore && state.sculpt_brush != SculptBrush::Restore {
        state.dispatch(Action::SetSculptBrush(SculptBrush::Restore));
    }
    if let Some(radius) = brush_size_key_step(ui, state.sculpt_brush_radius_points, 8.0..=220.0) {
        state.dispatch(Action::SetSculptBrushRadius(radius));
    }
}

pub(super) const fn sculpt_input_mode(
    modifiers: egui::Modifiers,
    brush: SculptBrush,
) -> SculptInputMode {
    if matches!(brush, SculptBrush::Mask) {
        SculptInputMode::Mask
    } else if modifiers.shift {
        SculptInputMode::Smooth
    } else if modifiers.alt && matches!(brush, SculptBrush::Restore) && !modifiers.ctrl {
        SculptInputMode::RestoreFit
    } else if modifiers.ctrl || modifiers.alt {
        SculptInputMode::Inflate
    } else {
        match brush {
            SculptBrush::Move => SculptInputMode::Grab,
            SculptBrush::Smooth => SculptInputMode::Smooth,
            SculptBrush::Restore => SculptInputMode::Restore,
            SculptBrush::Mask => SculptInputMode::Mask,
        }
    }
}

pub(super) const fn brush_shown_for(mode: SculptInputMode, selected: SculptBrush) -> SculptBrush {
    match mode {
        SculptInputMode::Smooth => SculptBrush::Smooth,
        SculptInputMode::Grab
        | SculptInputMode::Inflate
        | SculptInputMode::Restore
        | SculptInputMode::RestoreFit
        | SculptInputMode::Mask => selected,
    }
}

pub(super) fn displayed_sculpt_brush(ui: &Ui, state: &AppState) -> SculptBrush {
    if crate::sweep_gesture::sweep_active(ui, crate::ui_components::BrushSweeps::SCULPT.strength())
    {
        return state.sculpt_brush;
    }
    let modifiers = ui.input(|input| input.modifiers);
    brush_shown_for(
        sculpt_input_mode(modifiers, state.sculpt_brush),
        state.sculpt_brush,
    )
}

pub(super) fn handle_sculpt_brush_size_gesture(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
) -> bool {
    let update = handle_brush_size_gesture(
        ui,
        crate::ui_components::BrushSweeps::SCULPT.size(),
        viewport,
        state.sculpt_brush_radius_points,
        SCULPT_BRUSH_SIZE_SENSITIVITY,
        8.0..=220.0,
    );
    if let Some(radius) = update.radius {
        state.dispatch(Action::SetSculptBrushRadius(radius));
    }
    if update.consumed {
        return true;
    }

    let strength = crate::ui_components::handle_brush_strength_gesture(
        ui,
        crate::ui_components::BrushSweeps::SCULPT.strength(),
        viewport,
        state.sculpt_strength,
        BRUSH_STRENGTH_SENSITIVITY,
        0.01..=1.0,
    );
    if let Some(value) = strength.strength {
        state.dispatch(Action::SetSculptStrength(value));
    }
    strength.consumed
}

pub(super) fn make_sculpt_viewport_dab(
    state: &AppState,
    viewport: SculptDabViewport,
    stroke: &mut SculptViewportStroke,
    input_mode: SculptInputMode,
    sample_pointer: Pos2,
    pointer_delta: Vec2,
) -> Option<SculptDab> {
    if input_mode.is_paint_style() {
        let hit = viewport
            .camera
            .ray_from_screen(sample_pointer, viewport.rect)
            .and_then(|ray| {
                state.sculpt.raycast_visible(
                    ray.origin.to_array(),
                    ray.direction.to_array(),
                    viewport.visible_targets,
                )
            })?;
        stroke.center_local = hit.point_local;
    }
    let center_world = glam::DVec3::from_array(stroke.center_local).as_vec3();
    let world_per_point = viewport
        .camera
        .world_units_per_point_at(center_world, viewport.rect.height())
        .max(1.0e-8);
    let radius_local = f64::from(world_per_point * state.sculpt_brush_radius_points.max(1.0));
    let operation = match input_mode {
        SculptInputMode::Mask => return None,
        SculptInputMode::Smooth => SculptOperation::Smooth,
        SculptInputMode::Restore => SculptOperation::Restore,
        SculptInputMode::RestoreFit => SculptOperation::RestoreFit,
        SculptInputMode::Inflate => SculptOperation::Inflate {
            distance: f64::from(-pointer_delta.y * world_per_point),
        },
        SculptInputMode::Grab => {
            let translation = viewport.camera.world_drag_delta_at(
                center_world,
                pointer_delta,
                viewport.rect.height(),
            );
            SculptOperation::Grab {
                translation_local: translation.to_array().map(f64::from),
            }
        }
    };
    Some(SculptDab {
        center_local: stroke.center_local,
        radius_local,
        strength: f64::from(state.sculpt_strength),
        operation,
    })
}

pub(super) fn sculpt_dab_spacing(radius_points: f32) -> f32 {
    (radius_points.max(1.0) * SCULPT_DAB_SPACING_RADIUS_FRACTION).max(1.0)
}

pub(super) fn sculpt_input_samples(
    input_mode: SculptInputMode,
    start: Pos2,
    end: Pos2,
    radius_points: f32,
    distance_since_last_sample: &mut f32,
) -> Vec<Pos2> {
    if input_mode == SculptInputMode::Grab {
        *distance_since_last_sample = 0.0;
        return (end.distance(start) > f32::EPSILON)
            .then_some(end)
            .into_iter()
            .collect();
    }
    sculpt_spaced_samples(start, end, radius_points, distance_since_last_sample)
}

pub(super) fn sculpt_spaced_samples(
    start: Pos2,
    end: Pos2,
    radius_points: f32,
    distance_since_last_sample: &mut f32,
) -> Vec<Pos2> {
    let delta = end - start;
    let segment_length = delta.length();
    if !segment_length.is_finite() || segment_length <= f32::EPSILON {
        return Vec::new();
    }

    let spacing = sculpt_dab_spacing(radius_points);
    let previous = if distance_since_last_sample.is_finite() {
        distance_since_last_sample.clamp(0.0, spacing)
    } else {
        0.0
    };
    let total = previous + segment_length;
    let sample_count = ((total / spacing) + 1.0e-5).floor() as usize;
    let emitted = sample_count.min(MAX_SCULPT_INTERPOLATION_STEPS);
    let first_distance = (spacing - previous).max(0.0);
    let mut samples = Vec::with_capacity(emitted);
    for index in 0..emitted {
        let distance = first_distance + spacing * index as f32;
        samples.push(start + delta * (distance / segment_length).clamp(0.0, 1.0));
    }
    *distance_since_last_sample = total.rem_euclid(spacing);
    if *distance_since_last_sample <= 1.0e-4 || spacing - *distance_since_last_sample <= 1.0e-4 {
        *distance_since_last_sample = 0.0;
    }
    samples
}

pub(super) fn sculpt_smooth_time_dabs(accumulator_seconds: &mut f64, stable_dt: f32) -> usize {
    let elapsed = if stable_dt.is_finite() {
        f64::from(stable_dt).clamp(0.0, MAX_SCULPT_FRAME_DELTA_SECONDS)
    } else {
        0.0
    };
    *accumulator_seconds = accumulator_seconds.max(0.0) + elapsed;
    let available =
        ((*accumulator_seconds + 1.0e-9) / SCULPT_SMOOTH_DAB_INTERVAL_SECONDS).floor() as usize;
    let emitted = available.min(MAX_SCULPT_SMOOTH_DABS_PER_FRAME);
    *accumulator_seconds =
        (*accumulator_seconds - emitted as f64 * SCULPT_SMOOTH_DAB_INTERVAL_SECONDS).max(0.0);
    emitted
}

pub(super) fn paint_sculpt_brush_hud(ui: &Ui, state: &AppState, response: &Response) {
    let hover = response
        .hovered()
        .then(|| ui.input(|input| input.pointer.hover_pos()))
        .flatten();
    let Some(cursor) = crate::ui_components::brush_cursor(
        ui,
        hover,
        crate::ui_components::BrushSweeps::SCULPT.size(),
        Some((
            crate::ui_components::BrushSweeps::SCULPT.strength(),
            state.sculpt_strength,
        )),
    ) else {
        return;
    };
    let sizing =
        brush_size_gesture_anchor(ui, crate::ui_components::BrushSweeps::SCULPT.size()).is_some();
    let modifiers = ui.input(|input| input.modifiers);
    let color = if sizing || cursor.fill.is_some() {
        COLOR_TEXT
    } else if modifiers.shift {
        COLOR_SUCCESS
    } else if modifiers.ctrl {
        COLOR_WARNING
    } else {
        COLOR_PRIMARY
    };
    crate::ui_components::paint_brush_cursor(
        ui.painter(),
        cursor,
        state.sculpt_brush_radius_points,
        color,
    );
    crate::ui_components::hide_pointer(ui);
}

pub(super) const fn sculpt_visible_targets(state: &AppState) -> SculptTargets {
    state.sculpt.visible_targets()
}

pub(super) const fn sculpt_hit_targets(state: &AppState) -> SculptTargets {
    if matches!(state.sculpt_brush, crate::sculpt::SculptBrush::Mask) {
        return SculptTargets::ALL;
    }
    state.sculpt.editable_targets()
}

pub fn clear_sculpt_pointer_stroke(context: &egui::Context) {
    context.data_mut(|data| {
        data.remove::<SculptViewportStroke>(Id::new(SCULPT_DRAG_ID));
    });
    clear_brush_size_gesture(context, crate::ui_components::BrushSweeps::SCULPT.size());
    clear_brush_size_gesture(
        context,
        crate::ui_components::BrushSweeps::SCULPT.strength(),
    );
}
