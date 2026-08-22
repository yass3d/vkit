use super::*;
use crate::hair_project::HairTool;
use crate::ui_components::Icon;

pub(super) const HAIR_TOOLS: [(HairTool, Icon, TextKey, TextKey); 8] = [
    (
        HairTool::Pick,
        Icon::CursorPick,
        TextKey::HairToolPick,
        TextKey::HairToolPickHint,
    ),
    (
        HairTool::Plant,
        Icon::HairPlant,
        TextKey::HairToolPlant,
        TextKey::HairToolPlantHint,
    ),
    (
        HairTool::Grow,
        Icon::HairLength,
        TextKey::HairToolGrow,
        TextKey::HairToolGrowHint,
    ),
    (
        HairTool::Cut,
        Icon::Scissors,
        TextKey::HairToolCut,
        TextKey::HairToolCutHint,
    ),
    (
        HairTool::Erase,
        Icon::Eraser,
        TextKey::HairToolErase,
        TextKey::HairToolEraseHint,
    ),
    (
        HairTool::Comb,
        Icon::Comb,
        TextKey::HairToolComb,
        TextKey::HairToolCombHint,
    ),
    (
        HairTool::Pinch,
        Icon::Pinch,
        TextKey::HairToolPinch,
        TextKey::HairToolPinchHint,
    ),
    (
        HairTool::Puff,
        Icon::HairPuff,
        TextKey::HairToolPuff,
        TextKey::HairToolPuffHint,
    ),
];

#[derive(Clone, Copy, Debug)]
pub(super) struct HairHudPlan {
    pub(super) rect: Rect,
}

pub(super) fn hair_hud_plan(state: &AppState, viewport: Rect) -> Option<HairHudPlan> {
    if !state.is_hair_editing() {
        return None;
    }
    if state.hair_thumbnail.is_some() {
        return None;
    }
    let left = detail_hud_row_left(state, viewport);
    let right = detail_hud_right_base(viewport);
    let available = (right - left).max(0.0) - DETAIL_HUD_INSET_X * 2.0;

    let numeric_width = detail_hud_flex_numeric_width(available, true).max(96.0);
    let separators = (SPACE_2 + SPACE_1 + 1.0 + SPACE_1) * 2.0;
    let toggle_lane = separators + (DETAIL_HUD_TOGGLE_SIZE + SPACE_2) * 7.0;
    let content = numeric_width * 2.0 + SPACE_2 + toggle_lane;
    if available < content {
        return None;
    }

    let width = content + DETAIL_HUD_INSET_X * 2.0;
    let height = DETAIL_HUD_TWO_ROW_ROW_HEIGHT + DETAIL_HUD_INSET_Y * 2.0;
    let center = (left + right) * 0.5;
    let rect = Rect::from_min_size(
        pos2(center - width * 0.5, viewport.top() + DETAIL_HEADER_MARGIN),
        vec2(width, height),
    );
    Some(HairHudPlan { rect })
}

pub(crate) fn draw_hair_workspace(ui: &mut Ui, state: &mut AppState, whole: Rect, title: &str) {
    let (row, rect) = super::detail_hud::history_row_split(whole);
    super::detail_hud::draw_history_dial(ui, state, row);
    super::detail_hud::draw_model_panes(ui, state, rect, title);
    draw_hair_header(ui, state, rect);
    super::detail_hud::draw_split_view_toggle(ui, state, rect);
}

fn draw_hair_header(ui: &mut Ui, state: &mut AppState, viewport: Rect) {
    let Some(plan) = hair_hud_plan(state, viewport) else {
        return;
    };
    draw_detail_header_island(ui, plan.rect, "vkit.viewport.hair.header");

    let content = plan
        .rect
        .shrink2(vec2(DETAIL_HUD_INSET_X, DETAIL_HUD_INSET_Y));
    let mut hud = detail_hud_row_ui(ui, Id::new("vkit.viewport.hair.top-hud"), content, SPACE_2);

    let selected = state
        .hair_project
        .selected_part()
        .map(|part| (part.id, part.segments));
    let (part_id, segments) = selected.unwrap_or((0, crate::hair_project::DEFAULT_HAIR_SEGMENTS));

    // The brush's shape sizes the brush, so it stands before the size.
    let entries: Vec<_> = crate::hair_brush::HairBrushShape::ALL
        .into_iter()
        .map(|shape| {
            let icon = match shape {
                crate::hair_brush::HairBrushShape::Circle => Icon::BrushCircle,
                crate::hair_brush::HairBrushShape::Wide => Icon::BrushBarWide,
                crate::hair_brush::HairBrushShape::Tall => Icon::BrushBarTall,
            };
            (
                shape,
                icon,
                text(state.locale, shape.label_key()).to_owned(),
            )
        })
        .collect();
    if let Some(shape) = crate::ui_components::hud_icon_menu(
        &mut hud,
        Id::new("vkit.viewport.hair.brush-shape"),
        super::DETAIL_HUD_TOGGLE_SIZE,
        state.hair_brush_shape,
        &entries,
        text(state.locale, TextKey::HairBrushShape),
    ) {
        state.dispatch(Action::SetHairBrushShape(shape));
    }
    let follow = hud
        .add_enabled_ui(state.hair_brush_shape.is_bar(), |hud| {
            detail_hud_toggle_icon(
                hud,
                Icon::BrushFollowStroke,
                state.hair_brush_follow_stroke,
                text(state.locale, TextKey::HairBrushFollowStroke),
                text(state.locale, TextKey::HairBrushFollowStroke),
                None,
            )
        })
        .inner;
    if follow.clicked() {
        state.dispatch(Action::SetHairBrushFollowStroke(
            !state.hair_brush_follow_stroke,
        ));
    }
    draw_detail_separator(&mut hud);

    let numeric_width = detail_hud_flex_numeric_width(content.width(), true).max(96.0);
    let mut radius = state.hair_brush_radius_points;
    if detail_numeric_control(
        &mut hud,
        text(state.locale, TextKey::Size),
        &mut radius,
        NumericFormat {
            range: crate::viewport::hair_input::HAIR_BRUSH_RADIUS_RANGE,
            decimals: 0,
            percent: false,
        },
        numeric_width,
        Some(crate::shortcuts::BRUSH_SIZE_HINT),
    ) {
        state.dispatch(Action::SetHairBrushRadius(radius));
    }

    if shapes_existing_hair(state.hair_project.active_tool) {
        let mut strength = state.hair_brush_strength;
        if detail_numeric_control(
            &mut hud,
            text(state.locale, TextKey::Strength),
            &mut strength,
            NumericFormat {
                range: 0.05..=1.0,
                decimals: 0,
                percent: true,
            },
            numeric_width,
            Some(crate::shortcuts::BRUSH_STRENGTH_HINT),
        ) {
            state.dispatch(Action::SetHairBrushStrength(strength));
        }
    } else {
        let mut value = segments as f32;
        let changed = hud
            .add_enabled_ui(selected.is_some(), |hud| {
                detail_numeric_control(
                    hud,
                    text(state.locale, TextKey::HairSegmentsShort),
                    &mut value,
                    NumericFormat {
                        range: 2.0..=crate::hair_project::MAX_HAIR_SEGMENTS as f32,
                        decimals: 0,
                        percent: false,
                    },
                    numeric_width,
                    None,
                )
            })
            .inner;
        if changed && selected.is_some() {
            state.dispatch(Action::SetHairPartSegments {
                id: part_id,
                segments: value.round() as usize,
            });
        }
    }

    draw_detail_separator(&mut hud);
    let mirror = detail_hud_toggle_icon(
        &mut hud,
        Icon::MirrorX,
        state.hair_mirror_edit,
        text(state.locale, TextKey::HairMirrorEdit),
        text(state.locale, TextKey::HairMirrorEditHint),
        None,
    );
    if mirror.clicked() {
        state.hair_mirror_edit = !state.hair_mirror_edit;
    }
    let auto_part = detail_hud_toggle_icon(
        &mut hud,
        Icon::ConnectedTopology,
        state.hair_auto_part,
        text(state.locale, TextKey::HairAutoPart),
        text(state.locale, TextKey::HairAutoPartHint),
        None,
    );
    if auto_part.clicked() {
        state.hair_auto_part = !state.hair_auto_part;
    }

    draw_detail_separator(&mut hud);
    let tint = detail_hud_toggle_icon(
        &mut hud,
        Icon::VennThree,
        state.hair_part_tint,
        text(state.locale, TextKey::HairPartTint),
        text(state.locale, TextKey::HairPartTintHint),
        None,
    );
    if tint.clicked() {
        state.hair_part_tint = !state.hair_part_tint;
    }
    let points = detail_hud_toggle_icon(
        &mut hud,
        Icon::CrosshairBox,
        state.hair_show_points,
        text(state.locale, TextKey::HairShowPoints),
        text(state.locale, TextKey::HairShowPointsHint),
        None,
    );
    if points.clicked() {
        state.hair_show_points = !state.hair_show_points;
    }
    let physics = detail_hud_toggle_icon(
        &mut hud,
        Icon::GlobeGravity,
        state.hair_viewport_physics,
        text(state.locale, TextKey::HairViewportPhysics),
        text(state.locale, TextKey::HairViewportPhysicsHint),
        None,
    );
    if physics.clicked() {
        state.hair_viewport_physics = !state.hair_viewport_physics;
    }
    let streams = detail_hud_toggle_icon(
        &mut hud,
        Icon::HairStream,
        state.hair_show_streams,
        text(state.locale, TextKey::HairShowStreams),
        text(state.locale, TextKey::HairShowStreamsHint),
        None,
    );
    if streams.clicked() {
        state.hair_show_streams = !state.hair_show_streams;
    }
    let strands = detail_hud_toggle_icon(
        &mut hud,
        Icon::EyeClosed,
        state.hair_hide_strands,
        text(state.locale, TextKey::HairHideStrands),
        text(state.locale, TextKey::HairHideStrandsHint),
        None,
    );
    if strands.clicked() {
        state.hair_hide_strands = !state.hair_hide_strands;
    }
}

const TOOLBOX_GRAB_H: f32 = 14.0;
const TOOLBOX_INSET: f32 = 6.0;
const TOOLBOX_CORNER: f32 = 12.0;

fn toolbox_columns(state: &AppState) -> usize {
    state.hair_toolbox_columns.clamp(1, 2) as usize
}

pub(super) fn hair_toolbox_rect(state: &AppState, viewport: Rect) -> Option<Rect> {
    if !state.is_hair_editing() || state.hair_thumbnail.is_some() {
        return None;
    }
    let columns = toolbox_columns(state);
    let icons = HAIR_TOOLS.len() + 1;
    let rows = icons.div_ceil(columns) as f32;
    let width = columns as f32 * DETAIL_HUD_TOGGLE_SIZE
        + (columns as f32 - 1.0) * SPACE_2
        + TOOLBOX_INSET * 2.0;
    let height = TOOLBOX_GRAB_H
        + rows * DETAIL_HUD_TOGGLE_SIZE
        + (rows - 1.0) * SPACE_2
        + TOOLBOX_INSET * 2.0;
    if height > viewport.height() - 24.0 {
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
        state.hair_toolbox_pos,
        default_min,
        MARGIN,
    ))
}

pub(super) fn draw_hair_toolbox(ui: &mut Ui, state: &mut AppState, viewport: Rect) {
    let Some(rect) = hair_toolbox_rect(state, viewport) else {
        return;
    };
    let id = Id::new("vkit.viewport.hair.toolbox");
    let _blocker = ui.interact(rect, id.with("blocker"), Sense::click_and_drag());
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(
        rect,
        f32::from(crate::theme::RADIUS_POPOVER),
        COLOR_TOPBAR.gamma_multiply(0.96),
    );

    let content = rect.shrink(TOOLBOX_INSET);
    let grab_rect = Rect::from_min_size(content.min, vec2(content.width(), TOOLBOX_GRAB_H));
    let grab = ui.interact(grab_rect, id.with("grab"), Sense::drag());
    let pill = Rect::from_center_size(grab_rect.center(), vec2(26.0, 5.0));
    let pill_color = if grab.hovered() || grab.dragged() {
        crate::theme::COLOR_TEXT.gamma_multiply(0.55)
    } else {
        crate::theme::COLOR_MUTED.gamma_multiply(0.5)
    };
    painter.rect_filled(pill, 2.5, pill_color);
    crate::ui_components::island_move_handle(&grab, rect, &mut state.hair_toolbox_pos);

    let columns = toolbox_columns(state);
    let corner = Rect::from_min_max(rect.max - Vec2::splat(TOOLBOX_CORNER), rect.max);
    let corner_grab = ui.interact(corner, id.with("corner"), Sense::drag());
    if corner_grab.hovered() || corner_grab.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeNwSe);
    }
    if corner_grab.dragged()
        && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
    {
        let one = DETAIL_HUD_TOGGLE_SIZE + TOOLBOX_INSET * 2.0;
        let two = DETAIL_HUD_TOGGLE_SIZE * 2.0 + SPACE_2 + TOOLBOX_INSET * 2.0;
        let desired = pointer.x - rect.left();
        let wanted: u8 = if desired < (one + two) * 0.5 { 1 } else { 2 };
        if wanted != state.hair_toolbox_columns {
            state.hair_toolbox_columns = wanted;
        }
    }
    for step in 1..=3 {
        let offset = step as f32 * 3.0;
        painter.line_segment(
            [
                pos2(rect.right() - offset, rect.bottom() - 3.0),
                pos2(rect.right() - 3.0, rect.bottom() - offset),
            ],
            egui::Stroke::new(
                1.0,
                if corner_grab.hovered() || corner_grab.dragged() {
                    crate::theme::COLOR_TEXT.gamma_multiply(0.6)
                } else {
                    crate::theme::COLOR_MUTED.gamma_multiply(0.45)
                },
            ),
        );
    }

    let active = state.hair_project.active_tool;
    let mut chosen: Option<HairTool> = None;
    let mut mirror = false;
    for (index, slot) in (0..HAIR_TOOLS.len() + 1).enumerate() {
        let column = index % columns;
        let row = index / columns;
        let cell = Rect::from_min_size(
            pos2(
                content.left() + column as f32 * (DETAIL_HUD_TOGGLE_SIZE + SPACE_2),
                content.top() + TOOLBOX_GRAB_H + row as f32 * (DETAIL_HUD_TOGGLE_SIZE + SPACE_2),
            ),
            Vec2::splat(DETAIL_HUD_TOGGLE_SIZE),
        );
        let mut cell_ui = ui.new_child(
            egui::UiBuilder::new()
                .id_salt(id.with(("cell", index)))
                .max_rect(cell)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        cell_ui.shrink_clip_rect(rect);
        if let Some((tool, icon, name, hint)) = HAIR_TOOLS.get(slot).copied() {
            let shortcut = tool_shortcut(tool).map(|key| key.label_now(ui));
            if detail_hud_toggle_icon(
                &mut cell_ui,
                icon,
                active == tool,
                text(state.locale, name),
                text(state.locale, hint),
                shortcut.as_deref(),
            )
            .clicked()
            {
                chosen = Some(tool);
            }
        } else {
            let shortcut = crate::shortcuts::Shortcut::HairMirrorPart.label_now(ui);
            if detail_hud_toggle_icon(
                &mut cell_ui,
                Icon::MirrorPart,
                false,
                text(state.locale, TextKey::HairMirrorPart),
                "",
                Some(shortcut.as_str()),
            )
            .clicked()
            {
                mirror = true;
            }
        }
    }
    if let Some(tool) = chosen {
        state.dispatch(Action::SetHairTool(tool));
    }
    if mirror && let Some(part_id) = state.hair_project.selected_part_id {
        state.dispatch(Action::MirrorHairPart(part_id));
    }
}

const fn tool_shortcut(tool: HairTool) -> Option<crate::shortcuts::Shortcut> {
    use crate::shortcuts::Shortcut;
    match tool {
        HairTool::Pick => Some(Shortcut::HairPickTool),
        HairTool::Plant => Some(Shortcut::HairPlantTool),
        HairTool::Grow => Some(Shortcut::HairGrowTool),
        HairTool::Cut => Some(Shortcut::HairCutTool),
        HairTool::Erase => Some(Shortcut::HairEraseTool),
        HairTool::Comb => Some(Shortcut::HairCombBrush),
        HairTool::Puff => Some(Shortcut::HairPuffTool),
        HairTool::Pinch => Some(Shortcut::HairPinchTool),
    }
}

pub(crate) const fn shapes_existing_hair(tool: crate::hair_project::HairTool) -> bool {
    use crate::hair_project::HairTool;
    matches!(
        tool,
        HairTool::Comb | HairTool::Pinch | HairTool::Cut | HairTool::Grow | HairTool::Puff
    )
}
