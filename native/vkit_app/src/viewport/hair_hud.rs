use super::*;
use crate::hair_project::HairTool;
use crate::ui_components::Icon;

pub(super) const HAIR_TOOLS: [(HairTool, Icon, TextKey, TextKey); 10] = [
    (
        HairTool::Pick,
        Icon::CursorPick,
        TextKey::HairToolPick,
        TextKey::HairToolPickHint,
    ),
    (
        HairTool::Vertex,
        Icon::HairVertex,
        TextKey::HairToolVertex,
        TextKey::HairToolVertexHint,
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
    (
        HairTool::Rigidity,
        Icon::HairRigidity,
        TextKey::HairToolRigidity,
        TextKey::HairToolRigidityHint,
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

    const NUMERIC_CONTROLS: f32 = 2.0;
    const NUMERIC_FLOOR: f32 = 64.0;

    let toggle_lane = hair_toggle_lane();
    let numeric_room = (available - toggle_lane - SPACE_2).max(0.0) / NUMERIC_CONTROLS;
    let numeric_width = detail_hud_flex_numeric_width(available, true)
        .max(NUMERIC_FLOOR)
        .min(numeric_room.max(NUMERIC_FLOOR));
    let content = numeric_width * NUMERIC_CONTROLS + SPACE_2 + toggle_lane;

    if available < NUMERIC_FLOOR * NUMERIC_CONTROLS + SPACE_2 + toggle_lane {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HairSwitch {
    MirrorEdit,
    AutoPart,
    PartTint,
    ShowColliders,
    ShowPoints,
    ViewportPhysics,
    ShowStreams,
    HideStrands,
}

impl HairSwitch {
    fn of(self, state: &mut AppState) -> &mut bool {
        match self {
            Self::MirrorEdit => &mut state.hair_mirror_edit,
            Self::AutoPart => &mut state.hair_auto_part,
            Self::PartTint => &mut state.hair_part_tint,
            Self::ShowColliders => &mut state.hair_show_colliders,
            Self::ShowPoints => &mut state.hair_show_points,
            Self::ViewportPhysics => &mut state.hair_viewport_physics,
            Self::ShowStreams => &mut state.hair_show_streams,
            Self::HideStrands => &mut state.hair_hide_strands,
        }
    }
}

pub(super) const HAIR_SWITCHES: &[(HairSwitch, Icon, TextKey, TextKey, bool)] = &[
    (
        HairSwitch::MirrorEdit,
        Icon::MirrorX,
        TextKey::HairMirrorEdit,
        TextKey::HairMirrorEditHint,
        true,
    ),
    (
        HairSwitch::AutoPart,
        Icon::ConnectedTopology,
        TextKey::HairAutoPart,
        TextKey::HairAutoPartHint,
        false,
    ),
    (
        HairSwitch::PartTint,
        Icon::VennThree,
        TextKey::HairPartTint,
        TextKey::HairPartTintHint,
        true,
    ),
    (
        HairSwitch::ShowColliders,
        Icon::BodyCapsules,
        TextKey::HairShowColliders,
        TextKey::HairShowCollidersHint,
        false,
    ),
    (
        HairSwitch::ShowPoints,
        Icon::CrosshairBox,
        TextKey::HairShowPoints,
        TextKey::HairShowPointsHint,
        false,
    ),
    (
        HairSwitch::ViewportPhysics,
        Icon::GlobeGravity,
        TextKey::HairViewportPhysics,
        TextKey::HairViewportPhysicsHint,
        false,
    ),
    (
        HairSwitch::ShowStreams,
        Icon::HairStream,
        TextKey::HairShowStreams,
        TextKey::HairShowStreamsHint,
        false,
    ),
    (
        HairSwitch::HideStrands,
        Icon::EyeClosed,
        TextKey::HairHideStrands,
        TextKey::HairHideStrandsHint,
        false,
    ),
];

pub(super) fn hair_toggle_lane() -> f32 {
    let separators = HAIR_SWITCHES.iter().filter(|entry| entry.4).count() as f32;
    separators * (SPACE_2 + SPACE_1 + 1.0 + SPACE_1)
        + HAIR_SWITCHES.len() as f32 * (DETAIL_HUD_TOGGLE_SIZE + SPACE_2)
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

    match brush_slot(state.hair_project.active_tool) {
        BrushSlot::Strength => {
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
        }
        BrushSlot::Segments => {
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
        BrushSlot::Empty => hud.add_space(numeric_width),
    }

    for &(switch, icon, label, hint, separator) in HAIR_SWITCHES {
        if separator {
            draw_detail_separator(&mut hud);
        }
        let on = *switch.of(state);
        let response = detail_hud_toggle_icon(
            &mut hud,
            icon,
            on,
            text(state.locale, label),
            text(state.locale, hint),
            None,
        );
        if response.clicked() {
            *switch.of(state) = !on;
        }
    }
}

fn toolbox_columns(state: &AppState) -> usize {
    state.hair_toolbox_columns.clamp(1, 2) as usize
}

fn toolbox_slots() -> usize {
    HAIR_TOOLS.len() + 1
}

pub(super) fn hair_toolbox_rect(state: &AppState, viewport: Rect) -> Option<Rect> {
    if !state.is_hair_editing() || state.hair_thumbnail.is_some() {
        return None;
    }
    super::toolbox::toolbox_rect(
        viewport,
        toolbox_slots(),
        toolbox_columns(state),
        state.hair_toolbox_pos,
        None,
    )
}

pub(super) fn draw_hair_toolbox(ui: &mut Ui, state: &mut AppState, viewport: Rect) {
    let Some(rect) = hair_toolbox_rect(state, viewport) else {
        return;
    };
    let slots = toolbox_slots();
    let mut position = state.hair_toolbox_pos;
    let mut columns = state.hair_toolbox_columns;
    let cells = super::toolbox::draw_toolbox(
        ui,
        Id::new("vkit.viewport.hair.toolbox"),
        rect,
        slots,
        &mut position,
        &mut columns,
    );
    state.hair_toolbox_pos = position;
    state.hair_toolbox_columns = columns;

    let active = state.hair_project.active_tool;
    let mut chosen: Option<HairTool> = None;
    let mut mirror = false;
    for (slot, mut cell_ui) in cells.into_iter().enumerate() {
        if let Some((tool, icon, name, hint)) = HAIR_TOOLS.get(slot).copied() {
            let shortcut = tool_shortcut(tool).map(|key| key.label_now(&cell_ui));
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
            let shortcut = crate::shortcuts::Shortcut::HairMirrorPart.label_now(&cell_ui);
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
        HairTool::Vertex => None,
        HairTool::Puff => Some(Shortcut::HairPuffTool),
        HairTool::Pinch => Some(Shortcut::HairPinchTool),
        HairTool::Rigidity => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrushSlot {
    Segments,
    Strength,
    Empty,
}

#[must_use]
pub(crate) const fn brush_slot(tool: crate::hair_project::HairTool) -> BrushSlot {
    use crate::hair_project::HairTool;
    match tool {
        HairTool::Plant => BrushSlot::Segments,
        HairTool::Comb
        | HairTool::Pinch
        | HairTool::Cut
        | HairTool::Grow
        | HairTool::Puff
        | HairTool::Rigidity => BrushSlot::Strength,
        HairTool::Erase | HairTool::Pick | HairTool::Vertex => BrushSlot::Empty,
    }
}
