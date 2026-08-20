use super::*;

pub(super) fn radial_background_geometry(rect: Rect) -> RadialBackgroundGeometry {
    RadialBackgroundGeometry {
        center: rect.center(),
        radius: rect.size().length() * 0.58,
    }
}

pub(super) fn paint_viewport_background(ui: &Ui, state: &AppState, rect: Rect) {
    paint_viewport_background_with_opacity(ui, state, rect, 1.0);
}

pub(super) fn paint_viewport_background_with_opacity(
    ui: &Ui,
    state: &AppState,
    rect: Rect,
    opacity: f32,
) {
    let opacity = opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }
    let fade = |color: Color32| color.gamma_multiply(opacity);
    let mut mesh = egui::Mesh::default();

    let painter = ui.painter().with_clip_rect(ui.clip_rect().intersect(rect));
    match state.viewport_background_mode {
        ViewportBackgroundMode::Flat => {
            painter.rect_filled(rect, 0.0, fade(crate::theme::COLOR_VIEWPORT_BG));
        }
        ViewportBackgroundMode::Vertical => {
            let top = fade(crate::theme::COLOR_VIEWPORT_BG_TOP);
            let bottom = fade(crate::theme::COLOR_VIEWPORT_BG_BOTTOM);
            for (position, color) in [
                (rect.left_top(), top),
                (rect.right_top(), top),
                (rect.left_bottom(), bottom),
                (rect.right_bottom(), bottom),
            ] {
                mesh.colored_vertex(position, color);
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(2, 1, 3);
            painter.add(egui::Shape::mesh(mesh));
        }
        ViewportBackgroundMode::Radial => {
            let segments = 40_u32;
            let geometry = radial_background_geometry(rect);
            mesh.colored_vertex(
                geometry.center,
                fade(crate::theme::COLOR_VIEWPORT_BG_CENTER),
            );
            for index in 0..segments {
                let angle = std::f32::consts::TAU * index as f32 / segments as f32;
                mesh.colored_vertex(
                    geometry.center + vec2(angle.cos(), angle.sin()) * geometry.radius,
                    fade(crate::theme::COLOR_VIEWPORT_BG_EDGE),
                );
            }
            for index in 0..segments {
                mesh.add_triangle(0, index + 1, (index + 1) % segments + 1);
            }
            painter.add(egui::Shape::mesh(mesh));
        }
    }
}

pub(super) const TEMPLATE_FADE_IN_SECS: f32 = 0.20;

pub(super) const TEMPLATE_FADE_OUT_SECS: f32 = 0.40;

pub(super) const TEMPLATE_FADE_MAX_FRAME_SECS: f32 = 0.1;

pub(super) fn template_fade_step(opacity: f32, target: f32, dt: f32) -> f32 {
    let rate = if target > opacity {
        1.0 / TEMPLATE_FADE_IN_SECS
    } else {
        1.0 / TEMPLATE_FADE_OUT_SECS
    };
    let step = dt.max(0.0) * rate;
    if target > opacity {
        (opacity + step).min(target)
    } else {
        (opacity - step).max(target)
    }
}

#[derive(Clone, Copy)]
pub(super) struct TemplateFadeState {
    opacity: f32,
    updated_at: f64,
    seen_generation: u64,
}

pub(super) fn draw_template_install_fade(ui: &Ui, state: &AppState, rect: Rect) {
    let id = Id::new("vkit.viewport.template-install-fade");
    let now = ui.input(|input| input.time);
    let generation = state.template_install_generation;
    let load_active = state.template_load_active();
    let target = if load_active { 1.0 } else { 0.0 };
    let mut fade = ui
        .data(|data| data.get_temp::<TemplateFadeState>(id))
        .unwrap_or(TemplateFadeState {
            opacity: 0.0,
            updated_at: now,
            seen_generation: generation,
        });
    if now > fade.updated_at {
        let dt = ((now - fade.updated_at) as f32).min(TEMPLATE_FADE_MAX_FRAME_SECS);
        fade.opacity = template_fade_step(fade.opacity, target, dt);
        fade.updated_at = now;
    }
    if fade.seen_generation != generation {
        fade.seen_generation = generation;
        fade.opacity = 1.0;
    }
    ui.data_mut(|data| data.insert_temp(id, fade));
    if (fade.opacity - target).abs() > f32::EPSILON {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
    paint_viewport_background_with_opacity(ui, state, rect, ease_in_out_cubic(fade.opacity));
}

pub(super) fn focus_release_progress(elapsed: Duration) -> f32 {
    (elapsed.as_secs_f32() / IMPORT_FOCUS_RELEASE_DURATION.as_secs_f32()).clamp(0.0, 1.0)
}

pub(super) fn paint_vignette(ui: &Ui, state: &AppState, rect: Rect) {
    let vignette = state.vignette;
    if !vignette.enabled || vignette.intensity <= 0.0 || !rect.is_positive() {
        return;
    }
    const RINGS: usize = 24;
    const SEGMENTS: usize = 64;
    let (center, half) = (rect.center(), rect.size() * 0.5);
    let aspect = half.x / half.y.max(f32::EPSILON);

    let reach = 2.0_f32.sqrt();
    let mut mesh = egui::Mesh::default();
    for ring in 0..=RINGS {
        let span = reach * ring as f32 / RINGS as f32;
        for segment in 0..SEGMENTS {
            let angle = std::f32::consts::TAU * segment as f32 / SEGMENTS as f32;
            let offset = [angle.cos() * span, angle.sin() * span];
            let alpha = (vignette.darkening(offset, aspect) * 255.0)
                .round()
                .clamp(0.0, 255.0);
            mesh.colored_vertex(
                center + vec2(offset[0] * half.x, offset[1] * half.y),
                Color32::from_black_alpha(alpha as u8),
            );
        }
    }
    for ring in 0..RINGS {
        for segment in 0..SEGMENTS {
            let next = (segment + 1) % SEGMENTS;
            let inner = (ring * SEGMENTS + segment) as u32;
            let inner_next = (ring * SEGMENTS + next) as u32;
            let outer = ((ring + 1) * SEGMENTS + segment) as u32;
            let outer_next = ((ring + 1) * SEGMENTS + next) as u32;
            mesh.add_triangle(inner, outer, inner_next);
            mesh.add_triangle(inner_next, outer, outer_next);
        }
    }
    ui.painter()
        .with_clip_rect(rect)
        .add(egui::Shape::mesh(mesh));
}

pub(super) fn paint_radial_focus_mask(ui: &Ui, rect: Rect, clear_radius: f32, alpha: u8) {
    if alpha == 0 {
        return;
    }
    let segments = 48_u32;
    let outer_radius = rect.size().length() * 0.62;
    let inner_radius = clear_radius.clamp(0.0, outer_radius);
    let mut mesh = egui::Mesh::default();
    for index in 0..segments {
        let angle = std::f32::consts::TAU * index as f32 / segments as f32;
        let direction = vec2(angle.cos(), angle.sin());
        mesh.colored_vertex(
            rect.center() + direction * inner_radius,
            Color32::TRANSPARENT,
        );
        mesh.colored_vertex(
            rect.center() + direction * outer_radius,
            Color32::from_black_alpha(alpha),
        );
    }
    for index in 0..segments {
        let next = (index + 1) % segments;
        let inner = index * 2;
        let outer = inner + 1;
        let next_inner = next * 2;
        let next_outer = next_inner + 1;
        mesh.add_triangle(inner, outer, next_inner);
        mesh.add_triangle(next_inner, outer, next_outer);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

pub(super) const IMPORT_FOCUS_APPEAR_SECS: f32 = 0.18;

pub(super) fn draw_import_focus_overlay(ui: &mut Ui, state: &AppState, rect: Rect) {
    let id = Id::new("vkit.viewport.import-focus");
    let appeared_id = id.with("appeared-at");
    if let Some(progress) = state.import_progress {
        let _blocker = ui.interact(rect, id.with("blocker"), Sense::click_and_drag());
        if state.template_load_active() {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
            return;
        }

        let now = ui.input(|input| input.time);
        let appeared = ui
            .data(|data| data.get_temp::<f64>(appeared_id))
            .unwrap_or(now);
        ui.data_mut(|data| data.insert_temp(appeared_id, appeared));
        let ramp = ease_in_out_cubic(
            (((now - appeared) as f32) / IMPORT_FOCUS_APPEAR_SECS).clamp(0.0, 1.0),
        );
        ui.painter().rect_filled(
            rect,
            0.0,
            Color32::from_black_alpha((116.0 * ramp).round() as u8),
        );
        paint_radial_focus_mask(
            ui,
            rect,
            rect.width().min(rect.height()) * 0.16,
            (92.0 * ramp).round() as u8,
        );
        if let Some(note) = progress.size_note(state.locale) {
            ui.painter().text(
                rect.center() + vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                note,
                FontId::proportional(FONT_SM),
                COLOR_MUTED.gamma_multiply(ramp),
            );
        }
        ui.painter().text(
            rect.center() - vec2(0.0, 18.0),
            Align2::CENTER_CENTER,
            progress.phase.label(state.locale),
            FontId::proportional(FONT_HEADING),
            COLOR_TEXT,
        );
        let spinner = Rect::from_center_size(rect.center() + vec2(0.0, 20.0), Vec2::splat(24.0));
        ui.put(spinner, Spinner::new().size(22.0).color(COLOR_PRIMARY));
        ui.ctx().request_repaint_after(Duration::from_millis(16));
        return;
    }
    ui.data_mut(|data| data.remove::<f64>(appeared_id));

    let Some(completion) = state.scan_import_completion else {
        return;
    };
    let progress = focus_release_progress(completion.completed_at.elapsed());
    if progress >= 1.0 {
        return;
    }
    let eased = 1.0 - (1.0 - progress).powi(3);
    let outer = rect.size().length() * 0.62;
    paint_radial_focus_mask(
        ui,
        rect,
        outer * eased,
        ((1.0 - progress) * 112.0).round() as u8,
    );
    ui.ctx().request_repaint_after(Duration::from_millis(16));
}

pub(super) const DROP_TARGET_INSET: f32 = 40.0;

pub(super) const DROP_TARGET_MAX: Vec2 = Vec2::new(320.0, 240.0);

pub(super) const DROP_TARGET_RADIUS: f32 = 18.0;

pub(super) const DROP_DASH: f32 = 9.0;
pub(super) const DROP_GAP: f32 = 7.0;

pub(super) const DROP_PLUS_ARM: f32 = 13.0;

pub(super) const DROP_LABEL_GAP: f32 = 26.0;

pub(super) fn draw_scan_drop_target(ui: &mut Ui, state: &mut AppState, rect: Rect) {
    let available = rect.shrink(DROP_TARGET_INSET);
    let frame = Rect::from_center_size(
        available.center(),
        vec2(
            available.width().min(DROP_TARGET_MAX.x),
            available.height().min(DROP_TARGET_MAX.y),
        ),
    );
    if frame.width() < 80.0 || frame.height() < 80.0 {
        paint_empty(ui, rect, text(state.locale, TextKey::AddHeadFile));
        return;
    }

    let response = ui.interact(frame, Id::new("vkit.viewport.scan.drop"), Sense::click());

    let carrying = ui.input(|input| !input.raw.hovered_files.is_empty());
    let lit = carrying || response.hovered();

    let calling = crate::ui_components::attention_progress(
        ui,
        crate::ui::attention_target_id(crate::state::AttentionTarget::CustomHeadLoad),
    );
    let color = match calling {
        Some(_) => COLOR_DESTRUCTIVE,
        None if lit => COLOR_TEXT,
        None => COLOR_MUTED,
    };
    if calling.is_some() {
        ui.ctx().request_repaint();
    }

    if carrying {
        ui.painter().rect_filled(
            frame,
            crate::theme::CONTROL_RADIUS,
            crate::theme::COLOR_SURFACE_HOVER,
        );
    }
    let weight = if lit || calling.is_some() { 2.0 } else { 1.0 };
    paint_dashed_rect(ui, frame, Stroke::new(weight, color));

    let center = pos2(frame.center().x, frame.center().y - DROP_LABEL_GAP * 0.5);
    let stroke = Stroke::new(2.0, color);
    ui.painter().line_segment(
        [
            pos2(center.x - DROP_PLUS_ARM, center.y),
            pos2(center.x + DROP_PLUS_ARM, center.y),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            pos2(center.x, center.y - DROP_PLUS_ARM),
            pos2(center.x, center.y + DROP_PLUS_ARM),
        ],
        stroke,
    );
    ui.painter().text(
        pos2(center.x, center.y + DROP_LABEL_GAP + DROP_PLUS_ARM),
        Align2::CENTER_CENTER,
        text(state.locale, TextKey::AddHeadFile),
        FontId::proportional(FONT_BODY),
        color,
    );
    ui.painter().text(
        pos2(
            center.x,
            center.y + DROP_LABEL_GAP + DROP_PLUS_ARM + FONT_BODY + 6.0,
        ),
        Align2::CENTER_CENTER,
        text(state.locale, TextKey::AddHeadFileHint),
        FontId::proportional(FONT_XS),
        COLOR_MUTED,
    );

    if response.clicked() {
        state.dispatch(Action::RequestOpenScan);
    }
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
}

pub(super) fn paint_dashed_rect(ui: &Ui, rect: Rect, stroke: Stroke) {
    let painter = ui.painter();
    let radius = DROP_TARGET_RADIUS
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5);

    for (centre, start_angle) in [
        (
            pos2(rect.right() - radius, rect.top() + radius),
            -std::f32::consts::FRAC_PI_2,
        ),
        (pos2(rect.right() - radius, rect.bottom() - radius), 0.0),
        (
            pos2(rect.left() + radius, rect.bottom() - radius),
            std::f32::consts::FRAC_PI_2,
        ),
        (
            pos2(rect.left() + radius, rect.top() + radius),
            std::f32::consts::PI,
        ),
    ] {
        let arc_length = radius * std::f32::consts::FRAC_PI_2;
        let step = DROP_DASH + DROP_GAP;
        let count = (arc_length / step).round().max(1.0);
        let stride = arc_length / count;
        let dash = (stride * DROP_DASH / step).min(stride);
        for index in 0..count as usize {
            let from = index as f32 * stride;
            let point_at = |along: f32| {
                let angle = start_angle + along / radius.max(1.0e-3);
                pos2(
                    centre.x + radius * angle.cos(),
                    centre.y + radius * angle.sin(),
                )
            };

            let mid = point_at(from + dash * 0.5);
            painter.line_segment([point_at(from), mid], stroke);
            painter.line_segment([mid, point_at(from + dash)], stroke);
        }
    }
    let corners = [
        (
            pos2(rect.left() + radius, rect.top()),
            pos2(rect.right() - radius, rect.top()),
        ),
        (
            pos2(rect.right(), rect.top() + radius),
            pos2(rect.right(), rect.bottom() - radius),
        ),
        (
            pos2(rect.right() - radius, rect.bottom()),
            pos2(rect.left() + radius, rect.bottom()),
        ),
        (
            pos2(rect.left(), rect.bottom() - radius),
            pos2(rect.left(), rect.top() + radius),
        ),
    ];
    for (from, to) in corners {
        let span = to - from;
        let length = span.length();
        if length <= 0.0 {
            continue;
        }
        let step = DROP_DASH + DROP_GAP;

        let count = (length / step).round().max(1.0);
        let stride = length / count;
        let dash = (stride * DROP_DASH / step).min(stride);
        let direction = span / length;
        for index in 0..count as usize {
            let start = index as f32 * stride;
            painter.line_segment(
                [from + direction * start, from + direction * (start + dash)],
                stroke,
            );
        }
    }
}

pub(super) fn paint_empty(ui: &Ui, rect: Rect, message: &str) {
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        message,
        FontId::proportional(FONT_BODY),
        COLOR_MUTED,
    );
}
