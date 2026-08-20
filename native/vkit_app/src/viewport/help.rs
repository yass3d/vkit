use super::*;

pub(super) const PIN_HELP_ROWS: [(TextKey, TextKey); 17] = [
    (TextKey::HelpPlace, TextKey::ShortcutPlace),
    (TextKey::HelpMove, TextKey::ShortcutMove),
    (TextKey::HelpDelete, TextKey::ShortcutDelete),
    (TextKey::HelpXSymmetry, TextKey::ShortcutXSymmetry),
    (TextKey::HelpOrbit, TextKey::ShortcutOrbit),
    (TextKey::HelpTrackball, TextKey::ShortcutTrackball),
    (TextKey::HelpLevelRoll, TextKey::ShortcutLevelRoll),
    (TextKey::HelpPan, TextKey::ShortcutPan),
    (TextKey::HelpZoom, TextKey::ShortcutZoom),
    (TextKey::HelpDragZoom, TextKey::ShortcutDragZoom),
    (TextKey::HelpFrameView, TextKey::ShortcutFrameView),
    (TextKey::HelpSnapView, TextKey::ShortcutSnapView),
    (TextKey::HelpStandardViews, TextKey::ShortcutStandardViews),
    (
        TextKey::HelpCameraProjection,
        TextKey::ShortcutCameraProjection,
    ),
    (TextKey::HelpLight, TextKey::ShortcutLight),
    (TextKey::HelpUndo, TextKey::ShortcutUndo),
    (TextKey::HelpRedo, TextKey::ShortcutRedo),
];
pub(super) const ALIGN_HELP_ROWS: [(TextKey, TextKey); 12] = [
    (TextKey::HelpSnapRotation, TextKey::ShortcutSnapRotation),
    (TextKey::HelpOrbit, TextKey::ShortcutOrbit),
    (TextKey::HelpPan, TextKey::ShortcutPan),
    (TextKey::HelpZoom, TextKey::ShortcutZoom),
    (TextKey::HelpDragZoom, TextKey::ShortcutDragZoom),
    (TextKey::HelpFrameView, TextKey::ShortcutFrameView),
    (TextKey::HelpSnapView, TextKey::ShortcutSnapView),
    (TextKey::HelpStandardViews, TextKey::ShortcutStandardViews),
    (
        TextKey::HelpCameraProjection,
        TextKey::ShortcutCameraProjection,
    ),
    (TextKey::HelpLight, TextKey::ShortcutLight),
    (TextKey::HelpUndo, TextKey::ShortcutUndo),
    (TextKey::HelpRedo, TextKey::ShortcutRedo),
];
pub(super) const HAIR_HELP_ROWS: [(TextKey, TextKey); 15] = [
    (TextKey::HairToolPlant, TextKey::HairToolPlantHint),
    (TextKey::HairToolGrow, TextKey::HairToolGrowHint),
    (TextKey::HairToolErase, TextKey::HairToolEraseHint),
    (TextKey::HairToolComb, TextKey::HairToolCombHint),
    (TextKey::HelpHairSmooth, TextKey::ShortcutHairSmooth),
    (TextKey::HelpHairPick, TextKey::ShortcutHairPick),
    (TextKey::HelpBrushSize, TextKey::ShortcutBrushSize),
    (TextKey::HairSegmentsShort, TextKey::ShortcutBrushStrength),
    (TextKey::HelpOrbit, TextKey::ShortcutOrbit),
    (TextKey::HelpPan, TextKey::ShortcutPan),
    (TextKey::HelpZoom, TextKey::ShortcutZoom),
    (TextKey::HelpDragZoom, TextKey::ShortcutDragZoom),
    (TextKey::HelpSnapView, TextKey::ShortcutSnapView),
    (TextKey::HelpStandardViews, TextKey::ShortcutStandardViews),
    (TextKey::HelpLight, TextKey::ShortcutLight),
];

pub(super) const DETAIL_HELP_ROWS: [(TextKey, TextKey); 16] = [
    (TextKey::SculptGrab, TextKey::ShortcutSculptGrab),
    (TextKey::HelpBrushSize, TextKey::ShortcutBrushSize),
    (TextKey::HelpBrushStrength, TextKey::ShortcutBrushStrength),
    (TextKey::SculptSmooth, TextKey::ShortcutSculptSmooth),
    (TextKey::SculptPushPull, TextKey::ShortcutSculptPushPull),
    (TextKey::SculptXSymmetry, TextKey::ShortcutXSymmetry),
    (TextKey::HelpOrbit, TextKey::ShortcutOrbit),
    (TextKey::HelpPan, TextKey::ShortcutPan),
    (TextKey::HelpZoom, TextKey::ShortcutZoom),
    (TextKey::HelpDragZoom, TextKey::ShortcutDragZoom),
    (TextKey::HelpSnapView, TextKey::ShortcutSnapView),
    (TextKey::HelpStandardViews, TextKey::ShortcutStandardViews),
    (
        TextKey::HelpCameraProjection,
        TextKey::ShortcutCameraProjection,
    ),
    (TextKey::HelpLight, TextKey::ShortcutLight),
    (TextKey::HelpUndo, TextKey::ShortcutUndo),
    (TextKey::HelpRedo, TextKey::ShortcutRedo),
];
pub(super) const PROJECTION_HELP_ROWS: [(TextKey, TextKey); 7] = [
    (
        TextKey::HelpProjectionPaint,
        TextKey::ShortcutProjectionPaint,
    ),
    (TextKey::HelpProjectionMove, TextKey::ShortcutProjectionMove),
    (
        TextKey::HelpProjectionRotate,
        TextKey::ShortcutProjectionRotate,
    ),
    (TextKey::HelpProjectionZoom, TextKey::ShortcutProjectionZoom),
    (TextKey::HelpBrushSize, TextKey::ShortcutBrushSize),
    (TextKey::HelpBrushStrength, TextKey::ShortcutBrushStrength),
    (TextKey::HelpProjectionDone, TextKey::ShortcutProjectionDone),
];
pub(super) const SAVE_HELP_ROWS: [(TextKey, TextKey); 9] = [
    (TextKey::HelpOrbit, TextKey::ShortcutOrbit),
    (TextKey::HelpPan, TextKey::ShortcutPan),
    (TextKey::HelpZoom, TextKey::ShortcutZoom),
    (TextKey::HelpDragZoom, TextKey::ShortcutDragZoom),
    (TextKey::HelpFrameView, TextKey::ShortcutFrameView),
    (TextKey::HelpSnapView, TextKey::ShortcutSnapView),
    (TextKey::HelpStandardViews, TextKey::ShortcutStandardViews),
    (
        TextKey::HelpCameraProjection,
        TextKey::ShortcutCameraProjection,
    ),
    (TextKey::HelpLight, TextKey::ShortcutLight),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum HelpScope {
    Tab(Tab),

    Projection,
}

pub(super) fn help_scope(state: &AppState) -> HelpScope {
    if state.is_texturing()
        && state.texture_project.active_tool == crate::texture_project::TextureTool::Projection
    {
        return HelpScope::Projection;
    }
    HelpScope::Tab(state.active_tab)
}

#[cfg(test)]
pub(crate) type BrushHelpCard = (&'static str, &'static [(TextKey, TextKey)], TextKey);

#[cfg(test)]
pub(crate) const BRUSH_HELP_CARDS: [BrushHelpCard; 3] = [
    (
        "sculpt and texture",
        &DETAIL_HELP_ROWS,
        TextKey::HelpBrushStrength,
    ),
    ("hair", &HAIR_HELP_ROWS, TextKey::HairSegmentsShort),
    (
        "projection",
        &PROJECTION_HELP_ROWS,
        TextKey::HelpBrushStrength,
    ),
];

pub(super) fn viewport_help_rows(scope: HelpScope) -> &'static [(TextKey, TextKey)] {
    match scope {
        HelpScope::Tab(Tab::Alignment) => &ALIGN_HELP_ROWS,
        HelpScope::Tab(Tab::Edit) => &PIN_HELP_ROWS,
        HelpScope::Tab(Tab::Morph | Tab::Texture) => &DETAIL_HELP_ROWS,
        HelpScope::Tab(Tab::Hair) => &HAIR_HELP_ROWS,
        HelpScope::Tab(Tab::Result) => &SAVE_HELP_ROWS,
        HelpScope::Projection => &PROJECTION_HELP_ROWS,
    }
}

pub(super) fn viewport_help_button_rect(viewport: Rect) -> Option<Rect> {
    crate::viewport_chrome::slot(viewport, ChromeAnchor::BottomLeft, Vec2::splat(28.0), 0)
}

pub(super) const HELP_FONT: f32 = FONT_BODY;

pub(super) const HELP_ROW_HEIGHT: f32 = 22.0;

pub(super) fn viewport_help_column_widths(ui: &Ui, scope: HelpScope, locale: Locale) -> (f32, f32) {
    let measure = |key| {
        ui.painter()
            .layout_no_wrap(
                text(locale, key).to_owned(),
                FontId::proportional(HELP_FONT),
                Color32::WHITE,
            )
            .size()
            .x
    };
    let rows = viewport_help_rows(scope);
    let function_width = rows
        .iter()
        .map(|(function, _)| measure(*function))
        .fold(0.0, f32::max);
    let shortcut_width = rows
        .iter()
        .map(|(_, shortcut)| measure(*shortcut))
        .fold(0.0, f32::max);
    (function_width, shortcut_width)
}

pub(super) fn viewport_help_popup_rect(
    ui: &Ui,
    viewport: Rect,
    scope: HelpScope,
    locale: Locale,
) -> Option<Rect> {
    let button = viewport_help_button_rect(viewport)?;
    let rows = viewport_help_rows(scope);
    let (function_width, shortcut_width) = viewport_help_column_widths(ui, scope, locale);
    let intrinsic_width = MINI_HELP_CONTENT_INSET_X * 2.0 + function_width + 24.0 + shortcut_width;
    let width = intrinsic_width
        .max(220.0)
        .min((viewport.width() - 32.0).max(0.0));
    let height = MINI_HELP_CONTENT_INSET_Y * 2.0 + rows.len() as f32 * HELP_ROW_HEIGHT;
    let top = (button.top() - 8.0 - height).max(viewport.top() + 8.0);
    let rect = Rect::from_min_size(pos2(viewport.left() + 16.0, top), vec2(width, height));
    (width >= 220.0 && height <= viewport.height() - 16.0).then_some(rect)
}

pub(super) fn viewport_help_contains(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    pointer: Pos2,
) -> bool {
    viewport_help_button_rect(viewport).is_some_and(|rect| rect.contains(pointer))
        || (state.help_visible
            && viewport_help_popup_rect(ui, viewport, help_scope(state), state.locale)
                .is_some_and(|rect| rect.contains(pointer)))
}

pub(super) fn draw_viewport_help(
    ui: &mut Ui,
    state: &mut AppState,
    viewport: Rect,
    draw_button: bool,
) {
    let id = Id::new("vkit.viewport.help");
    if state.help_visible
        && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
        && ui.input(|input| input.pointer.any_click())
        && !viewport_help_contains(ui, state, viewport, pointer)
    {
        state.dispatch(Action::ToggleHelp);
    }
    if state.help_visible
        && let Some(panel_rect) =
            viewport_help_popup_rect(ui, viewport, help_scope(state), state.locale)
    {
        ui.painter().rect_filled(
            panel_rect,
            SMALL_RADIUS as f32,
            COLOR_TOPBAR.gamma_multiply(0.94),
        );
        let _blocker = ui.interact(
            panel_rect,
            id.with("panel-blocker"),
            Sense::click_and_drag(),
        );
        let painter = ui.painter().with_clip_rect(panel_rect);
        let (function_width, _) = viewport_help_column_widths(ui, help_scope(state), state.locale);
        let shortcut_x = panel_rect.left() + MINI_HELP_CONTENT_INSET_X + function_width + 24.0;
        for (index, (function, shortcut)) in viewport_help_rows(help_scope(state))
            .iter()
            .copied()
            .enumerate()
        {
            let y = panel_rect.top()
                + MINI_HELP_CONTENT_INSET_Y
                + HELP_ROW_HEIGHT * 0.5
                + index as f32 * HELP_ROW_HEIGHT;
            painter.text(
                pos2(panel_rect.left() + MINI_HELP_CONTENT_INSET_X, y),
                Align2::LEFT_CENTER,
                text(state.locale, function),
                FontId::proportional(HELP_FONT),
                COLOR_TEXT,
            );
            painter.text(
                pos2(shortcut_x, y),
                Align2::LEFT_CENTER,
                text(state.locale, shortcut),
                FontId::proportional(HELP_FONT),
                COLOR_MUTED,
            );
        }
    }

    if !draw_button {
        return;
    }
    let Some(button_rect) = viewport_help_button_rect(viewport) else {
        return;
    };
    let response = ui.interact(button_rect, id.with("button"), Sense::click());
    ui.painter().circle_filled(
        button_rect.center(),
        14.0,
        if state.help_visible || response.hovered() {
            crate::theme::hover_fill(COLOR_VIEWPORT_TOOL)
        } else {
            COLOR_VIEWPORT_TOOL
        },
    );
    ui.painter().text(
        button_rect.center(),
        Align2::CENTER_CENTER,
        "?",
        FontId::proportional(FONT_HEADING),
        COLOR_TEXT,
    );
    control_affordances(ui, &response, button_rect, button_rect.height() * 0.5);
    if response.clicked() {
        state.dispatch(Action::ToggleHelp);
    }
}
