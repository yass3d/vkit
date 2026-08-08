use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DetailHudTier {
    Full,

    Compact,

    TwoRow,

    ThreeRow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DetailHudPlan {
    pub(super) tier: DetailHudTier,
    pub(super) rect: Rect,
}

pub(super) const DETAIL_MODE_CAPSULE_WIDTH: f32 = 84.0;
pub(super) const DETAIL_MODE_CAPSULE_GAP: f32 = 8.0;

pub(super) fn detail_hud_left_base(viewport: Rect) -> f32 {
    viewport_tool_rail_rect(viewport).map_or(
        viewport.left() + crate::viewport_chrome::EDGE_INSET,
        |rail| rail.right() + crate::viewport_chrome::EDGE_INSET,
    )
}

pub(super) fn detail_hud_right_base(viewport: Rect) -> f32 {
    let gizmo = crate::viewport_chrome::slot(
        viewport,
        crate::viewport_chrome::ChromeAnchor::TopRight,
        crate::viewport::ORIENTATION_HUD_SIZE,
        0,
    );
    gizmo.map_or(
        viewport.right() - crate::viewport_chrome::EDGE_INSET,
        |gizmo| gizmo.left() - crate::viewport_chrome::STACK_GAP,
    )
}

pub(super) fn detail_hud_plan(viewport: Rect) -> Option<DetailHudPlan> {
    let left = detail_hud_left_base(viewport) + DETAIL_MODE_CAPSULE_WIDTH + DETAIL_MODE_CAPSULE_GAP;
    let right = detail_hud_right_base(viewport);
    let available = (right - left).max(0.0);
    let (tier, width, height) = if available >= DETAIL_HUD_OUTER_WIDTH {
        (
            DetailHudTier::Full,
            DETAIL_HUD_OUTER_WIDTH,
            DETAIL_HUD_HEIGHT,
        )
    } else if available >= DETAIL_HUD_COMPACT_MIN_WIDTH {
        (
            DetailHudTier::Compact,
            available.min(DETAIL_HUD_COMPACT_MAX_OUTER_WIDTH),
            DETAIL_HUD_HEIGHT,
        )
    } else if available >= DETAIL_HUD_TWO_ROW_MIN_WIDTH {
        (DetailHudTier::TwoRow, available, DETAIL_HUD_TWO_ROW_HEIGHT)
    } else if available >= DETAIL_HUD_MIN_WIDTH {
        (
            DetailHudTier::ThreeRow,
            available,
            DETAIL_HUD_THREE_ROW_HEIGHT,
        )
    } else {
        return None;
    };
    Some(DetailHudPlan {
        tier,
        rect: Rect::from_min_size(
            pos2(
                left + (available - width) * 0.5,
                viewport.top() + DETAIL_HEADER_MARGIN,
            ),
            vec2(width, height),
        ),
    })
}

pub(super) fn detail_hud_flex_numeric_width(content_width: f32, same_row_cluster: bool) -> f32 {
    let reserved = if same_row_cluster {
        DETAIL_HUD_COMPACT_CLUSTER_WIDTH + SPACE_2 * 7.0
    } else {
        SPACE_2
    };
    ((content_width - reserved) * 0.5)
        .clamp(DETAIL_HUD_MIN_NUMERIC_WIDTH, DETAIL_NUMERIC_CONTROL_WIDTH)
        .floor()
}

pub(super) fn detail_group_expanded_height() -> f32 {
    let row_count = SculptTarget::ALL.len() as f32;
    (DETAIL_GROUP_INSET_Y_TOP + DETAIL_GROUP_INSET_Y)
        + crate::theme::CONTROL_H_COMPACT
        + crate::theme::CONTROL_H_DENSE * (row_count + 1.0)
        + DETAIL_GROUP_ITEM_GAP * (row_count + 1.0)
}

pub(super) fn detail_group_collapsed_height() -> f32 {
    let row_count = SculptTarget::ALL.len() as f32;
    (DETAIL_GROUP_INSET_Y_TOP + DETAIL_GROUP_INSET_Y)
        + crate::theme::CONTROL_H_COMPACT
        + DETAIL_GROUP_ITEM_GAP
        + crate::theme::CONTROL_H_DENSE * row_count
}

pub(super) fn anchor_detail_panel_to_right_corner(pos: &mut Option<[f32; 2]>, collapsing: bool) {
    let Some(pos) = pos.as_mut() else {
        return;
    };
    let width_delta = DETAIL_GROUP_PANEL_WIDTH - DETAIL_GROUP_COLLAPSED_WIDTH;
    let height_delta = detail_group_expanded_height() - detail_group_collapsed_height();
    let sign = if collapsing { 1.0 } else { -1.0 };
    pos[0] += sign * width_delta;
    pos[1] += sign * height_delta;
}

const GROUP_PANEL_SIDE_MARGIN: f32 = 2.0 * crate::viewport_chrome::EDGE_INSET;
pub(super) fn detail_group_panel_rect(state: &AppState, viewport: Rect) -> Option<Rect> {
    let (width, height) = if state.sculpt_groups_collapsed {
        (
            (viewport.width() - GROUP_PANEL_SIDE_MARGIN).min(DETAIL_GROUP_COLLAPSED_WIDTH),
            detail_group_collapsed_height(),
        )
    } else {
        (
            (viewport.width() - GROUP_PANEL_SIDE_MARGIN).min(DETAIL_GROUP_PANEL_WIDTH),
            detail_group_expanded_height(),
        )
    };
    let minimum_width = if state.sculpt_groups_collapsed {
        72.0
    } else {
        184.0
    };
    if width < minimum_width || height > viewport.height() - 24.0 {
        return None;
    }

    const MARGIN: f32 = crate::viewport_chrome::EDGE_INSET;

    let default_min = pos2(
        viewport.right() - width - MARGIN,
        viewport.center().y - height * 0.5,
    );
    Some(crate::ui_components::island_rect(
        viewport,
        vec2(width, height),
        state.detail_group_panel_pos,
        default_min,
        MARGIN,
    ))
}

pub(super) fn detail_viewport_controls_contains(
    state: &AppState,
    viewport: Rect,
    pointer: Pos2,
) -> bool {
    if !state.is_detail_editing() {
        return false;
    }

    let in_header_band = pointer.y < detail_header_band_bottom(state, viewport)
        && pointer.x >= detail_hud_left_base(viewport)
        && pointer.x <= viewport.right() - crate::viewport_chrome::EDGE_INSET;
    in_header_band
        || (!state.is_texturing()
            && detail_group_panel_rect(state, viewport).is_some_and(|rect| rect.contains(pointer)))
}

pub(super) fn detail_header_content_height(state: &AppState, viewport: Rect) -> f32 {
    if state.is_texturing() {
        DETAIL_HUD_HEIGHT
    } else {
        detail_hud_plan(viewport).map_or(DETAIL_HUD_HEIGHT, |plan| plan.rect.height())
    }
}

pub(crate) fn detail_header_band_bottom(state: &AppState, viewport: Rect) -> f32 {
    viewport.top() + DETAIL_HEADER_MARGIN + detail_header_content_height(state, viewport)
}

pub(super) fn detail_header_bar_rect(state: &AppState, viewport: Rect) -> Option<Rect> {
    let top = viewport.top() + DETAIL_HEADER_MARGIN;
    let bar = Rect::from_min_max(
        pos2(detail_hud_left_base(viewport), top),
        pos2(
            viewport.right() - crate::viewport_chrome::EDGE_INSET,
            top + detail_header_content_height(state, viewport),
        ),
    );
    (bar.width() > DETAIL_MODE_CAPSULE_WIDTH + DETAIL_MODE_CAPSULE_GAP && bar.height() > 0.0)
        .then_some(bar)
}

pub fn draw_detail_workspace(ui: &mut Ui, state: &mut AppState, rect: Rect, title: &str) {
    if state.is_texturing() {
        let previous_ratio = state.texture_project.workspace_split_ratio;
        let mut ratio = previous_ratio;
        let (split_x, handle) = crate::ui_components::show_horizontal_split(
            ui,
            Id::new("vkit.texture.workspace-split"),
            rect,
            &mut ratio,
            350.0,
            260.0,
            |ui, model_rect, source_rect| {
                draw_result(ui, state, model_rect, title);
                crate::texture_ui::draw_source_workspace(ui, state, source_rect);
                (model_rect.right() + source_rect.left()) * 0.5
            },
        );
        // Drawn out here rather than inside the model half, so it can sit on
        // the divider the way the create tab's prompt does.
        crate::viewport::draw_texture_prompt_island(ui, state, rect, split_x);
        if (ratio - previous_ratio).abs() > f32::EPSILON {
            state.dispatch(Action::SetTextureWorkspaceSplit(ratio));
            ui.ctx().request_repaint();
        }
        if handle.dragged() {
            ui.ctx().request_repaint();
        }
    } else {
        draw_result(ui, state, rect, title);
    }
    draw_detail_header(ui, state, rect, rect);
}

pub(super) fn draw_detail_header(ui: &mut Ui, state: &mut AppState, viewport: Rect, body: Rect) {
    let Some(bar) = detail_header_bar_rect(state, viewport) else {
        return;
    };

    if state.is_texturing() {
        let content = bar;
        draw_detail_header_island(ui, content, "vkit.viewport.detail.header.paint");
        crate::texture_ui::draw_paint_header_content(
            ui,
            state,
            content.shrink2(vec2(SPACE_2, 0.0)),
        );
    } else if let Some(plan) = detail_hud_plan(viewport) {
        draw_detail_header_island(ui, plan.rect, "vkit.viewport.detail.header.sculpt");
        draw_detail_hud_tiers(ui, state, plan, body);
    }
}

pub(super) fn draw_detail_header_island(ui: &mut Ui, rect: Rect, id: &'static str) {
    ui.painter().rect_filled(
        rect,
        f32::from(crate::theme::RADIUS_POPOVER),
        COLOR_TOPBAR.gamma_multiply(0.96),
    );
    let _blocker = ui.interact(rect, Id::new(id), Sense::click_and_drag());
}

pub(super) fn draw_detail_viewport_controls(ui: &mut Ui, state: &mut AppState, viewport: Rect) {
    if !state.is_texturing()
        && let Some(rect) = detail_group_panel_rect(state, viewport)
    {
        draw_detail_group_panel(ui, state, rect);
    }
}

pub(super) fn draw_detail_hud_tiers(
    ui: &mut Ui,
    state: &mut AppState,
    plan: DetailHudPlan,
    viewport: Rect,
) {
    let rect = plan.rect;
    let id = Id::new("vkit.viewport.detail.top-hud");
    let content = rect.shrink2(vec2(DETAIL_HUD_INSET_X, DETAIL_HUD_INSET_Y));
    match plan.tier {
        DetailHudTier::Full => {
            let mut hud = detail_hud_row_ui(ui, id, content, 5.0);
            detail_hud_brush(&mut hud, state, false);
            detail_hud_numeric_controls(&mut hud, state, DETAIL_HUD_FULL_NUMERIC_WIDTH);
            draw_detail_separator(&mut hud);
            detail_hud_falloff(&mut hud, state, false);
            draw_detail_separator(&mut hud);
            detail_hud_toggles(&mut hud, state);
            draw_detail_separator(&mut hud);
            detail_hud_eye_control(&mut hud, state, viewport);
        }
        DetailHudTier::Compact => {
            let mut hud = detail_hud_row_ui(ui, id, content, SPACE_2);
            detail_hud_brush(&mut hud, state, true);
            let numeric_width = detail_hud_flex_numeric_width(content.width(), true);
            detail_hud_numeric_controls(&mut hud, state, numeric_width);
            detail_hud_falloff(&mut hud, state, true);
            detail_hud_toggles(&mut hud, state);
            detail_hud_eye_control(&mut hud, state, viewport);
        }
        DetailHudTier::TwoRow => {
            let first_row = Rect::from_min_size(
                content.min,
                vec2(content.width(), DETAIL_HUD_TWO_ROW_ROW_HEIGHT),
            );
            let second_row = Rect::from_min_size(
                pos2(content.left(), first_row.bottom() + SPACE_2),
                vec2(content.width(), DETAIL_HUD_TWO_ROW_ROW_HEIGHT),
            );
            let mut first = detail_hud_row_ui(ui, id.with("row-1"), first_row, SPACE_2);
            let numeric_width = detail_hud_flex_numeric_width(content.width(), false);
            detail_hud_numeric_controls(&mut first, state, numeric_width);
            let mut second = detail_hud_row_ui(ui, id.with("row-2"), second_row, SPACE_2);
            detail_hud_brush(&mut second, state, true);
            detail_hud_falloff(&mut second, state, true);
            detail_hud_toggles(&mut second, state);
            detail_hud_eye_control(&mut second, state, viewport);
        }
        DetailHudTier::ThreeRow => {
            let first_row = Rect::from_min_size(
                content.min,
                vec2(content.width(), DETAIL_HUD_TWO_ROW_ROW_HEIGHT),
            );
            let second_row = Rect::from_min_size(
                pos2(content.left(), first_row.bottom() + SPACE_2),
                vec2(content.width(), DETAIL_HUD_TWO_ROW_ROW_HEIGHT),
            );
            let third_row = Rect::from_center_size(
                pos2(
                    content.center().x,
                    second_row.bottom() + SPACE_2 + DETAIL_HUD_TWO_ROW_ROW_HEIGHT * 0.5,
                ),
                vec2(DETAIL_HUD_EYE_WIDTH, DETAIL_HUD_TWO_ROW_ROW_HEIGHT),
            );
            let mut first = detail_hud_row_ui(ui, id.with("row-1"), first_row, SPACE_2);
            let numeric_width = detail_hud_flex_numeric_width(content.width(), false);
            detail_hud_numeric_controls(&mut first, state, numeric_width);
            let mut second = detail_hud_row_ui(ui, id.with("row-2"), second_row, SPACE_2);
            detail_hud_brush(&mut second, state, true);
            detail_hud_falloff(&mut second, state, true);
            detail_hud_toggles(&mut second, state);
            let mut third = detail_hud_row_ui(ui, id.with("row-3"), third_row, SPACE_2);
            detail_hud_eye_control(&mut third, state, viewport);
        }
    }
}

pub(super) fn detail_hud_row_ui(ui: &mut Ui, salt: Id, row: Rect, gap: f32) -> Ui {
    let mut hud = ui.new_child(
        UiBuilder::new()
            .id_salt(salt)
            .max_rect(row)
            .layout(Layout::left_to_right(Align::Center)),
    );
    hud.spacing_mut().item_spacing.x = gap;
    hud
}

pub(super) fn detail_hud_numeric_controls(hud: &mut Ui, state: &mut AppState, control_width: f32) {
    let mut radius = state.sculpt_brush_radius_points;
    let radius_changed = detail_numeric_control(
        hud,
        text(state.locale, TextKey::Size),
        &mut radius,
        NumericFormat {
            range: 8.0..=220.0,
            decimals: 0,
            percent: false,
        },
        control_width,
        Some(crate::shortcuts::BRUSH_SIZE_HINT),
    );
    if radius_changed {
        state.dispatch(Action::SetSculptBrushRadius(radius));
    }

    let mut strength = state.sculpt_strength;

    let strength_key = if control_width >= DETAIL_HUD_WIDE_LABEL_CONTROL_WIDTH {
        TextKey::Strength
    } else {
        TextKey::StrengthShort
    };
    let strength_changed = detail_numeric_control(
        hud,
        text(state.locale, strength_key),
        &mut strength,
        NumericFormat {
            range: 0.01..=1.0,
            decimals: 0,
            percent: true,
        },
        control_width,
        Some(crate::shortcuts::BRUSH_STRENGTH_HINT),
    );
    if strength_changed {
        state.dispatch(Action::SetSculptStrength(strength));
    }
}

pub(super) fn detail_hud_falloff(hud: &mut Ui, state: &mut AppState, compact: bool) {
    let falloff = state.sculpt.falloff_preset();
    if let Some(preset) = sculpt_falloff_selector(hud, state.locale, falloff, compact) {
        state.dispatch(Action::SetSculptFalloff(preset));
    }
}

pub(super) fn detail_hud_brush(hud: &mut Ui, state: &mut AppState, compact: bool) {
    let shown = displayed_sculpt_brush(hud, state);
    if let Some(brush) = sculpt_brush_selector(hud, state.locale, shown, compact) {
        state.dispatch(Action::SetSculptBrush(brush));
    }
}

pub(super) fn detail_hud_toggles(hud: &mut Ui, state: &mut AppState) {
    let x = detail_hud_toggle_icon(
        hud,
        Icon::MirrorX,
        state.sculpt_x_symmetry,
        text(state.locale, TextKey::SculptXSymmetry),
        text(state.locale, TextKey::SculptXSymmetryTooltip),
        Some(crate::shortcuts::Shortcut::XSymmetry.label()),
    );
    if x.clicked() {
        state.dispatch(Action::SetSculptXSymmetry(!state.sculpt_x_symmetry));
    }
    let backface = state.sculpt.backface_masking();
    let mask = detail_hud_toggle_icon(
        hud,
        Icon::BackfaceProtection,
        backface,
        text(state.locale, TextKey::BackfaceProtection),
        text(state.locale, TextKey::BackfaceProtectionTooltip),
        None,
    );
    if mask.clicked() {
        state.dispatch(Action::ToggleSculptBackfaceMasking(!backface));
    }
    let connected = state.sculpt_connected_topology_only;
    let topology = detail_hud_toggle_icon(
        hud,
        Icon::ConnectedTopology,
        connected,
        text(state.locale, TextKey::AutoGroup),
        text(state.locale, TextKey::AutoGroupTooltip),
        None,
    );
    if topology.clicked() {
        state.dispatch(Action::SetSculptConnectedTopologyOnly(!connected));
    }
}

pub(super) fn detail_hud_toggle_icon(
    hud: &mut Ui,
    icon: Icon,
    active: bool,
    name: &str,
    explanation: &str,
    shortcut: Option<&str>,
) -> Response {
    let (rect, response) =
        hud.allocate_exact_size(Vec2::splat(DETAIL_HUD_TOGGLE_SIZE), Sense::click());
    // The key rides on the name line rather than a third line, so a toggle that
    // has one reads at a glance as "this has a key" without growing the card.
    let name = match shortcut {
        Some(shortcut) => format!("{name}  ({shortcut})"),
        None => name.to_owned(),
    };
    let explanation = explanation.to_owned();
    let response = response.on_hover_ui(|ui| {
        ui.set_max_width(crate::ui_components::TOOLTIP_MAX_WIDTH);
        ui.label(egui::RichText::new(&name).strong());
        ui.label(&explanation);
    });
    let radius = rect.height() * 0.5;
    if active {
        hud.painter()
            .rect_filled(rect, radius, crate::theme::COLOR_ACTIVE_BG);
    } else if response.hovered() {
        hud.painter()
            .rect_filled(rect, radius, COLOR_SURFACE_RAISED);
    }
    let icon_color = if active {
        crate::theme::COLOR_ACTIVE_INK
    } else if response.hovered() {
        COLOR_TEXT
    } else {
        crate::theme::disabled(COLOR_MUTED)
    };
    paint_icon(hud.painter(), rect.shrink(3.5), icon, icon_color);
    control_affordances(hud, &response, rect, radius);
    response
}

pub(super) fn detail_hud_eye_control(hud: &mut Ui, state: &mut AppState, viewport: Rect) {
    let tracking = state.sculpt_eye_tracking;

    let following = tracking && state.eye_gaze_mode == crate::state::EyeGazeMode::AutoCursor;
    let (eye_rect, eye_response) = hud.allocate_exact_size(
        vec2(DETAIL_HUD_EYE_WIDTH, DETAIL_HUD_TOGGLE_SIZE),
        Sense::click(),
    );
    hud.painter().rect_filled(
        eye_rect,
        eye_rect.height() * 0.5,
        if following {
            crate::theme::COLOR_WARNING_ACTIVE_BG
        } else {
            COLOR_SURFACE_RAISED
        },
    );
    let left = Rect::from_center_size(
        pos2(eye_rect.center().x - 11.0, eye_rect.center().y),
        vec2(18.0, 18.0),
    );
    let right = Rect::from_center_size(
        pos2(eye_rect.center().x + 11.0, eye_rect.center().y),
        vec2(18.0, 18.0),
    );
    paint_icon(hud.painter(), left, Icon::EyeOpen, COLOR_TEXT);
    paint_icon(hud.painter(), right, Icon::EyeOpen, COLOR_TEXT);
    let eye_response = eye_response.on_hover_text(text(state.locale, TextKey::AdjustEyeGaze));
    if eye_response.clicked() && !tracking {
        state.dispatch(Action::ToggleSculptEyeTracking(true));
    }
    draw_eye_gaze_popup(hud, state, &eye_response, viewport);
}
