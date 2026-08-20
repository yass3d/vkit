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

const DETAIL_SPLIT_SLOT_SIZE: f32 = 26.0;
const DETAIL_SPLIT_SLOT_GAP: f32 = 6.0;

pub(super) fn detail_split_slot_rect(state: &AppState, viewport: Rect) -> Option<Rect> {
    if !state.is_detail_editing() && !state.is_hair_editing() {
        return None;
    }
    let top = viewport.top() + DETAIL_HEADER_MARGIN;
    let rect = Rect::from_min_size(
        pos2(
            detail_hud_left_base(viewport),
            top + (DETAIL_HUD_HEIGHT - DETAIL_SPLIT_SLOT_SIZE) * 0.5,
        ),
        vec2(DETAIL_SPLIT_SLOT_SIZE, DETAIL_SPLIT_SLOT_SIZE),
    );
    viewport.contains_rect(rect).then_some(rect)
}

pub(super) fn detail_hud_row_left(state: &AppState, viewport: Rect) -> f32 {
    detail_split_slot_rect(state, viewport).map_or_else(
        || detail_hud_left_base(viewport),
        |slot| slot.right() + DETAIL_SPLIT_SLOT_GAP,
    )
}

pub(super) fn detail_hud_plan(state: &AppState, viewport: Rect) -> Option<DetailHudPlan> {
    let left =
        detail_hud_row_left(state, viewport) + DETAIL_MODE_CAPSULE_WIDTH + DETAIL_MODE_CAPSULE_GAP;
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
        + crate::theme::CONTROL_H_DENSE * row_count
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
    if state.is_hair_editing() {
        let in_header = super::hair_hud::hair_hud_plan(state, viewport)
            .is_some_and(|plan| plan.rect.contains(pointer));
        let in_toolbox = super::hair_hud::hair_toolbox_rect(state, viewport)
            .is_some_and(|rect| rect.contains(pointer));
        return in_header || in_toolbox;
    }
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
        detail_hud_plan(state, viewport).map_or(DETAIL_HUD_HEIGHT, |plan| plan.rect.height())
    }
}

pub(crate) fn detail_header_band_bottom(state: &AppState, viewport: Rect) -> f32 {
    viewport.top() + DETAIL_HEADER_MARGIN + detail_header_content_height(state, viewport)
}

pub(super) fn detail_header_bar_rect(state: &AppState, viewport: Rect) -> Option<Rect> {
    let top = viewport.top() + DETAIL_HEADER_MARGIN;
    let bar = Rect::from_min_max(
        pos2(detail_hud_row_left(state, viewport), top),
        pos2(
            viewport.right() - crate::viewport_chrome::EDGE_INSET,
            top + detail_header_content_height(state, viewport),
        ),
    );
    (bar.width() > DETAIL_MODE_CAPSULE_WIDTH + DETAIL_MODE_CAPSULE_GAP && bar.height() > 0.0)
        .then_some(bar)
}

pub fn draw_detail_workspace(ui: &mut Ui, state: &mut AppState, whole: Rect, title: &str) {
    let (row, rect) = history_row_split(whole);
    draw_history_dial(ui, state, row);
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
                draw_model_panes(ui, state, model_rect, title);
                crate::texture_ui::draw_source_workspace(ui, state, source_rect);
                (model_rect.right() + source_rect.left()) * 0.5
            },
        );
        crate::viewport::draw_texture_prompt_island(ui, state, rect, split_x);
        if (ratio - previous_ratio).abs() > f32::EPSILON {
            state.dispatch(Action::SetTextureWorkspaceSplit(ratio));
            ui.ctx().request_repaint();
        }
        if handle.dragged() {
            ui.ctx().request_repaint();
        }
    } else {
        draw_model_panes(ui, state, rect, title);
    }
    draw_detail_header(ui, state, rect, rect);
    draw_split_view_toggle(ui, state, rect);
}

pub(super) fn draw_split_view_toggle(ui: &mut Ui, state: &mut AppState, viewport: Rect) {
    use crate::ui_components::Icon;

    let Some(cell) = detail_split_slot_rect(state, viewport) else {
        return;
    };
    let size = cell.width();
    draw_detail_header_island(ui, cell, "vkit.viewport.detail.split-slot");
    let response = ui
        .interact(
            cell,
            Id::new("vkit.viewport.split-toggle"),
            egui::Sense::click(),
        )
        .on_hover_ui(|ui| {
            ui.set_max_width(crate::ui_components::TOOLTIP_MAX_WIDTH);
            ui.label(
                egui::RichText::new(crate::i18n::text(
                    state.locale,
                    crate::i18n::TextKey::SplitModelViewName,
                ))
                .strong(),
            );
            ui.label(crate::i18n::text(
                state.locale,
                crate::i18n::TextKey::SplitModelViewTooltip,
            ));
        });
    let on = state.split_model_view;
    if on || response.hovered() {
        ui.painter().rect_filled(
            cell.shrink(2.0),
            crate::theme::CONTROL_RADIUS,
            if on {
                crate::theme::COLOR_PRIMARY
            } else {
                crate::theme::COLOR_SURFACE_HOVER
            },
        );
    }
    let icon = if state.is_texturing() {
        Icon::SplitRows
    } else {
        Icon::SplitColumns
    };
    crate::ui_components::paint_icon(
        ui.painter(),
        cell.shrink(size * 0.22),
        icon,
        if on {
            crate::theme::COLOR_BG
        } else {
            crate::theme::COLOR_TEXT
        },
    );
    if response.clicked() {
        state.dispatch(Action::SetSplitModelView(!on));
    }
}

pub(super) const PANE_SPLIT_GAP: f32 = 2.0;

const PANE_MINIMUM_EXTENT: f32 = 160.0;

pub(super) fn draw_model_panes(ui: &mut Ui, state: &mut AppState, rect: Rect, title: &str) {
    use crate::viewport::ViewPane;

    let stacked = state.is_texturing();
    let extent = if stacked { rect.height() } else { rect.width() };
    let framing = state.hair_thumbnail.is_some();
    if !state.split_model_view || framing || extent < PANE_MINIMUM_EXTENT * 2.0 + PANE_SPLIT_GAP {
        state.active_view_pane = ViewPane::Primary;
        draw_result(ui, state, rect, title);
        return;
    }
    let lowest = PANE_MINIMUM_EXTENT / (extent - PANE_SPLIT_GAP);
    let ratio = state.model_split_ratio.clamp(lowest, 1.0 - lowest);
    let (first, second) = pane_rects(rect, stacked, ratio);
    crate::viewport::resolve_active_pane(ui, state, first, second);
    crate::viewport::draw_result_pane(ui, state, first, title, ViewPane::Primary, rect);
    crate::viewport::draw_result_pane(ui, state, second, title, ViewPane::Split, rect);
    draw_pane_divider(ui, state, rect, first, stacked, ratio, lowest);
    crate::viewport::draw_result_floating_chrome(ui, state, rect);
}

pub(super) fn pane_rects(rect: Rect, stacked: bool, ratio: f32) -> (Rect, Rect) {
    let extent = if stacked { rect.height() } else { rect.width() };
    let usable = (extent - PANE_SPLIT_GAP).max(0.0);
    let leading = usable * ratio.clamp(0.0, 1.0);
    let trailing = usable - leading;
    if stacked {
        (
            Rect::from_min_size(rect.min, egui::vec2(rect.width(), leading)),
            Rect::from_min_size(
                egui::pos2(rect.left(), rect.top() + leading + PANE_SPLIT_GAP),
                egui::vec2(rect.width(), trailing),
            ),
        )
    } else {
        (
            Rect::from_min_size(rect.min, egui::vec2(leading, rect.height())),
            Rect::from_min_size(
                egui::pos2(rect.left() + leading + PANE_SPLIT_GAP, rect.top()),
                egui::vec2(trailing, rect.height()),
            ),
        )
    }
}

fn draw_pane_divider(
    ui: &mut Ui,
    state: &mut AppState,
    rect: Rect,
    first: Rect,
    stacked: bool,
    ratio: f32,
    lowest: f32,
) {
    let grab = crate::theme::INSPECTOR_RESIZE_GRAB_RADIUS;
    let (handle_rect, id) = if stacked {
        (
            Rect::from_min_max(
                egui::pos2(rect.left(), first.bottom() - grab),
                egui::pos2(rect.right(), first.bottom() + PANE_SPLIT_GAP + grab),
            ),
            Id::new("vkit.viewport.pane-divider.rows"),
        )
    } else {
        (
            Rect::from_min_max(
                egui::pos2(first.right() - grab, rect.top()),
                egui::pos2(first.right() + PANE_SPLIT_GAP + grab, rect.bottom()),
            ),
            Id::new("vkit.viewport.pane-divider.columns"),
        )
    };
    let handle = if stacked {
        crate::ui_components::vertical_resize_handle(ui, id, handle_rect)
    } else {
        crate::ui_components::horizontal_resize_handle(ui, id, handle_rect)
    };

    let usable = (if stacked { rect.height() } else { rect.width() }) - PANE_SPLIT_GAP;
    let mut moved = ratio;
    if handle.dragged()
        && let Some(pointer) = handle.interact_pointer_pos()
    {
        let along = if stacked {
            pointer.y - rect.top()
        } else {
            pointer.x - rect.left()
        };
        moved = (along / usable.max(1.0)).clamp(lowest, 1.0 - lowest);
    }
    if handle.double_clicked() {
        moved = 0.5;
    }
    if (moved - state.model_split_ratio).abs() > f32::EPSILON {
        state.dispatch(Action::SetModelSplitRatio(moved));
        ui.ctx().request_repaint();
    }
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
    } else if let Some(plan) = detail_hud_plan(state, viewport) {
        draw_detail_header_island(ui, plan.rect, "vkit.viewport.detail.header.sculpt");
        draw_detail_hud_tiers(ui, state, plan, body);
    }
}

pub(super) fn draw_detail_header_island(ui: &mut Ui, rect: Rect, id: &'static str) {
    let radius = crate::theme::capsule_radius(rect.height().min(44.0));
    ui.painter()
        .rect_filled(rect, radius, COLOR_TOPBAR.gamma_multiply(0.96));
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

const HISTORY_ROW_HEIGHT: f32 = 12.0;

const HISTORY_SLOT_WIDTH: f32 = 70.0;

const HISTORY_SLOT_INSET: f32 = 0.5;

const HISTORY_CURRENT_FILL: egui::Color32 = egui::Color32::from_rgb(0x6b, 0x6b, 0x6b);

const HISTORY_FLING_DECAY: f32 = 0.90;
const HISTORY_FLING_REST: f32 = 8.0;

const HISTORY_SLIDE_SLOP: f32 = 4.0;

#[derive(Clone, Copy, Debug, Default)]
struct HistoryRunner {
    offset: f32,
    velocity: f32,
}

pub(super) fn history_row_split(rect: Rect) -> (Rect, Rect) {
    let height = HISTORY_ROW_HEIGHT.min(rect.height());
    (
        Rect::from_min_size(rect.min, vec2(rect.width(), height)),
        Rect::from_min_max(pos2(rect.left(), rect.top() + height), rect.max),
    )
}

pub(super) fn draw_history_dial(ui: &mut Ui, state: &mut AppState, row: Rect) {
    let (back, forward) = state.history_position();
    let steps = back + forward;
    if steps == 0 || row.width() < HISTORY_SLOT_WIDTH || row.height() <= 0.0 {
        return;
    }

    let id = Id::new("vkit.viewport.history-strip");
    let pitch = HISTORY_SLOT_WIDTH;
    #[expect(clippy::cast_precision_loss, reason = "step counts")]
    let content = pitch * (steps + 1) as f32;
    let bar = Rect::from_min_size(row.min, vec2(content.min(row.width()), row.height()));
    let overflow = (content - bar.width()).max(0.0);
    let response = ui.interact(bar, id, egui::Sense::click_and_drag());

    let mut runner = ui
        .data(|data| data.get_temp::<HistoryRunner>(id))
        .unwrap_or_default();
    let seconds = ui.input(|input| input.stable_dt).clamp(1.0e-3, 0.1);
    if response.dragged() {
        let travel = ui.input(|input| input.pointer.delta().x);
        runner.offset += travel;
        runner.velocity = travel / seconds;
    } else if runner.velocity.abs() > HISTORY_FLING_REST {
        runner.offset += runner.velocity * seconds;
        runner.velocity *= HISTORY_FLING_DECAY;
        ui.ctx().request_repaint();
    } else {
        runner.velocity = 0.0;
    }
    let slid = response.dragged() || runner.velocity != 0.0;

    #[expect(clippy::cast_precision_loss, reason = "step counts")]
    let here = back as f32 * pitch;
    if !slid && overflow > 0.0 {
        let visible = -runner.offset..=(-runner.offset + bar.width() - pitch);
        if !visible.contains(&here) {
            runner.offset = (bar.width() - pitch).mul_add(0.5, -here);
        }
    }
    runner.offset = runner.offset.clamp(-overflow, 0.0);
    if runner.offset <= -overflow || runner.offset >= 0.0 {
        runner.velocity = 0.0;
    }
    ui.data_mut(|data| data.insert_temp(id, runner));

    let radius = bar.height() * 0.5;
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let pressed =
        response.clicked() && !slid && response.drag_delta().length() <= HISTORY_SLIDE_SLOP;
    let mut target = back;
    let painter = ui.painter().with_clip_rect(bar);
    for index in 0..=steps {
        #[expect(clippy::cast_precision_loss, reason = "step counts")]
        let left = bar.left() + runner.offset + index as f32 * pitch;
        let slot = Rect::from_min_size(pos2(left, bar.top()), vec2(pitch, bar.height()));
        if slot.right() < bar.left() - 1.0 || slot.left() > bar.right() + 1.0 {
            continue;
        }
        let aimed = pointer.is_some_and(|point| slot.contains(point) && bar.contains(point));
        if aimed && pressed {
            target = index;
        }
        painter.rect_filled(
            slot.shrink2(vec2(HISTORY_SLOT_INSET, 0.0)),
            radius,
            if index == back {
                HISTORY_CURRENT_FILL
            } else if aimed {
                crate::theme::COLOR_SURFACE_HOVER
            } else if index < back {
                crate::theme::COLOR_SURFACE_RAISED
            } else {
                crate::theme::COLOR_SURFACE
            },
        );
    }
    let mut at = back;
    while at > target {
        state.dispatch(Action::Undo);
        let now = state.history_position().0;
        if now >= at {
            break;
        }
        at = now;
    }
    while at < target {
        state.dispatch(Action::Redo);
        let now = state.history_position().0;
        if now <= at {
            break;
        }
        at = now;
    }
}
