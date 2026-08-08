use std::path::{Path, PathBuf};

use egui::{
    Align, Align2, Button, CentralPanel, Color32, ComboBox, DragValue, FontId, Frame, Id, Layout,
    Margin, Panel, Rect, ResizeDirection, Response, RichText, ScrollArea, Sense, Spinner, Stroke,
    TextEdit, TextureHandle, TextureOptions, Ui, UiBuilder, Vec2, ViewportCommand, WidgetInfo,
    WidgetType, containers::scroll_area::ScrollBarVisibility, pos2, vec2,
};

use crate::{
    diagnostics,
    i18n::{Locale, TextKey, morph_label_for, text},
    morphs::{MorphAvailability, MorphCategory, MorphCategoryFilter, MorphListFilter},
    shortcuts::Shortcut,
    state::PackageSlot,
    state::{
        Action, AppState, AttentionTarget, BaseViewMode, EditSourceMode, EyeState, FigureSex,
        JobStage, SaveSection, ScanSymmetry, StatusTone, Tab, VaMCatalogStatus, VarMetadataField,
    },
    theme::{
        ACTION_RADIUS, CAPSULE_RADIUS, COLOR_AXIS_X, COLOR_AXIS_Y, COLOR_AXIS_Z, COLOR_BG,
        COLOR_CLOSE_HOVER, COLOR_CLOSE_PRESSED, COLOR_DESTRUCTIVE, COLOR_FIELD, COLOR_MUTED,
        COLOR_PRIMARY, COLOR_SUCCESS, COLOR_SURFACE, COLOR_SURFACE_HOVER, COLOR_SURFACE_RAISED,
        COLOR_TEXT, COLOR_TOPBAR, COLOR_WARNING, CONTROL_H_DENSE, CONTROL_H_PRIMARY,
        CONTROL_HEIGHT, CONTROL_RADIUS, FONT_BODY, FONT_HEADING, FONT_SM, FONT_XS,
        INSPECTOR_ACTIVE_DIVIDER_WIDTH, INSPECTOR_DEFAULT_WIDTH, PANEL_INSET, PROGRESS_HEIGHT,
        SECTION_GAP, SECTION_LABEL_FONT_SIZE, SPACE_2, SPACE_3, SPACE_4, STATUS_BAR_HEIGHT,
        TOP_BAR_HEIGHT, clamp_inspector_width, disabled,
    },
    ui_components::{
        CapsuleFieldButton, FilledNumericSlider, Icon, MINI_POPUP_CONTENT_INSET_Y, animate_rect,
        animated_segmented_group, attention_flash, attention_widget, chips, color_capsule_picker,
        control_affordances, horizontal_resize_handle, icon_button, icon_button_size, paint_icon,
        paint_list_row_highlight, segment_button, switch, switch_row, tooltip,
    },
    vam_edit_sources::VaMEditSourceKind,
    window_control::{self, CaptionButton, NcLayout, NcRect},
};

const MORPH_FILTER_CAPSULE_WIDTH: f32 = 100.0;

const MORPH_LIST_FILTERS: [MorphListFilter; 2] = [MorphListFilter::All, MorphListFilter::Active];

const fn morph_list_filter_key(filter: MorphListFilter) -> TextKey {
    match filter {
        MorphListFilter::All => TextKey::MorphFilterAll,
        MorphListFilter::Active => TextKey::MorphFilterActive,
    }
}

const TOP_TAB_WIDTH: f32 = 112.0;
const TOP_TAB_HEIGHT: f32 = 32.0;
const TOP_TAB_GAP: f32 = 4.0;
const TITLE_BRAND_WIDTH: f32 = 146.0;
const TITLE_VAM_FIELD_WIDTH: f32 = 180.0;

const TITLE_LOCALE_WIDTH: f32 = 76.0;

pub(crate) const TITLE_LOCALE_CHROME: f32 = 36.0;

pub(crate) const TITLE_LOCALE_MAX_WIDTH: f32 = 190.0;
const TITLE_LOCALE_HEIGHT: f32 = 28.0;
const TITLE_WINDOW_BUTTON_WIDTH: f32 = 40.0;
const PRIMARY_FOOTER_HEIGHT: f32 = 112.0;
const ALIGNMENT_FOOTER_HEIGHT: f32 = 62.0;
const MORPH_ACTION_FOOTER_HEIGHT: f32 = 118.0;
const MORPH_ROW_HEIGHT: f32 = 46.0;
const MORPH_ROW_GAP: f32 = 1.0;
const MORPH_FILTER_LIST_GAP: f32 = 6.0;
const MORPH_APPLY_HEIGHT: f32 = CONTROL_H_PRIMARY;
const MORPH_FOOTER_BUTTON_GAP: f32 = 8.0;
const MORPH_ROW_HORIZONTAL_INSET: f32 = 8.0;
const MORPH_ROW_VERTICAL_INSET: f32 = 3.0;
const MORPH_ROW_LABEL_HEIGHT: f32 = 16.0;
const MORPH_ROW_CONTROL_HEIGHT: f32 = 22.0;
const MORPH_ROW_LINE_GAP: f32 = 1.0;
const MORPH_RESET_WIDTH: f32 = 24.0;
const MORPH_RESET_ICON_SIZE: f32 = 17.0;
const MORPH_CONTROL_GAP: f32 = 6.0;
const MORPH_STATUS_WIDTH: f32 = 96.0;
const TRANSFORM_AXIS_LABEL_WIDTH: f32 = 20.0;
const SEARCH_ICON_SLOT_WIDTH: f32 = 32.0;
const SEARCH_TEXT_INSET: f32 = 10.0;

const SKIN_SELECTOR_LIST_HEIGHT: f32 = 168.0;

const SKIN_SELECTOR_MIN_LIST_HEIGHT: f32 = CONTROL_HEIGHT * 2.0;

const SKIN_SELECTOR_FRAME_MARGIN: i8 = 4;

const SKIN_SELECTOR_FALLBACK_TAIL: f32 = VAM_SOURCE_STATUS_HEIGHT + SPACE_3 * 2.0;

const VAM_SOURCE_STATUS_HEIGHT: f32 = 18.0;

const VAM_EDIT_SOURCE_LIST_HEIGHT: f32 = 220.0;

const VAM_EDIT_SOURCE_MIN_LIST_HEIGHT: f32 = 132.0;

const VAM_EDIT_SOURCE_FRAME_MARGIN: i8 = 4;
const PRIMARY_ACTION_HEIGHT: f32 = CONTROL_H_PRIMARY;

const PLACE_HEAD_SIDE_INSET: f32 = 6.0;

const SLIDER_ROW_INSET: f32 = 4.0;

#[expect(
    clippy::cast_possible_truncation,
    reason = "corner radii are u8 in egui and this height is 42"
)]
const PLACE_HEAD_RADIUS: u8 = (crate::theme::PLACE_HEAD_HEIGHT as u8) / 2;
const SCALE_LINK_ICON_SIZE: f32 = 15.0;
const DIAGNOSTIC_LOG_VIEW_BYTES: usize = 1024 * 1024;

const DIAGNOSTIC_LOG_SIMPLE_ROWS: usize = 400;
const DIAGNOSTIC_LOG_MESSAGE_LIMIT: usize = 240;
const DIAGNOSTIC_LOG_MODAL_MAX_WIDTH: f32 = 980.0;
const DIAGNOSTIC_LOG_MODAL_MAX_HEIGHT: f32 = 720.0;
const STATUS_TOAST_HOLD_SECONDS: f64 = 2.6;
const STATUS_TOAST_FADE_IN_SECONDS: f64 = 0.16;
const STATUS_TOAST_FADE_OUT_SECONDS: f64 = 0.42;
const STATUS_PROGRESS_GAP: f32 = 2.0;
const CATALOG_PROGRESS_HOLD_SECONDS: f64 = 0.22;

const TOP_TABS: [(Tab, TextKey); 4] = [
    (Tab::Edit, TextKey::FaceMatch),
    (Tab::Morph, TextKey::DetailCorrection),
    (Tab::Texture, TextKey::TextureStage),
    (Tab::Result, TextKey::Save),
];

/// How long the reset button keeps offering to take itself back.
///
/// Long enough to notice what just happened and reach for the mouse, short
/// enough that the button is not lying about what it does the next time it is
/// glanced at.
const MORPH_RESET_UNDO_SECONDS: f64 = 3.0;

fn offer_morph_reset_undo(ui: &Ui, id: Id) {
    let now = ui.input(|input| input.time);
    ui.data_mut(|data| data.insert_temp(id.with("undo-until"), now + MORPH_RESET_UNDO_SECONDS));
}

fn forget_morph_reset_undo(ui: &Ui, id: Id) {
    ui.data_mut(|data| data.remove::<f64>(id.with("undo-until")));
}

/// Is the reset still within its take-it-back window?
///
/// Someone who hits this by accident and does not know Ctrl+Z has no other way
/// back, so the button offers itself as the way back — and names the shortcut
/// underneath, which is how the shortcut gets learned.
fn morph_reset_undo_is_offered(ui: &Ui, id: Id) -> bool {
    let until = ui.data(|data| data.get_temp::<f64>(id.with("undo-until")));
    let Some(until) = until else {
        return false;
    };
    let now = ui.input(|input| input.time);
    if now >= until {
        forget_morph_reset_undo(ui, id);
        return false;
    }
    // Without this the label would sit on "undo" until some other event happened
    // to repaint, which could be long after the offer has actually lapsed.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs_f64((until - now).max(0.05)));
    true
}

/// A small shortcut caption directly under a control.
fn paint_shortcut_hint_below(ui: &Ui, control: Rect, shortcut: &str) {
    ui.painter().text(
        pos2(control.center().x, control.bottom() + crate::theme::SPACE_1),
        Align2::CENTER_TOP,
        shortcut,
        FontId::proportional(FONT_XS),
        COLOR_MUTED,
    );
}

/// Whether a top tab can be entered right now.
///
/// Two of the four stand for a pair of inner tabs, so this cannot simply ask
/// `tab_available`. It is shared by the click path and the number keys because a
/// key that could enter a tab the button refuses would be a second, quieter set
/// of rules.
pub(crate) fn top_tab_available(state: &AppState, tab: Tab) -> bool {
    // Every stage past the fit can start from the G2 template when no scan has
    // been opened. The button has to say so, or the tab is reachable in state
    // and unclickable in the interface.
    let base = match tab {
        Tab::Edit => state.tab_available(Tab::Alignment) || state.tab_available(Tab::Edit),
        Tab::Morph | Tab::Texture | Tab::Result => {
            state.tab_available(tab) || state.can_enter_detail_from_template()
        }
        other => state.tab_available(other),
    };
    base && (!state.busy() || visible_tab_is_active(state, tab))
}

/// Where a top tab actually lands, which is not always the tab itself.
fn top_tab_target(state: &AppState, tab: Tab) -> Tab {
    if tab == Tab::Edit && !state.tab_available(Tab::Edit) {
        Tab::Alignment
    } else {
        tab
    }
}

/// Number keys 1..4 pick the top tabs, in the order they are drawn.
///
/// Ignored while a text field has the keyboard, or the digits would be swallowed
/// out of every search box and name field in the program.
fn handle_top_tab_keys(ui: &Ui, state: &mut AppState) {
    if ui.ctx().egui_wants_keyboard_input() {
        return;
    }
    const DIGITS: [egui::Key; 4] = [
        egui::Key::Num1,
        egui::Key::Num2,
        egui::Key::Num3,
        egui::Key::Num4,
    ];
    let pressed = ui.input(|input| {
        DIGITS
            .iter()
            .position(|key| input.key_pressed(*key) && input.modifiers.is_none())
    });
    if let Some(index) = pressed {
        let (tab, _) = TOP_TABS[index];
        if top_tab_available(state, tab) {
            state.dispatch(Action::RequestTab(top_tab_target(state, tab)));
        }
    }
}

fn visible_tab_is_active(state: &AppState, tab: Tab) -> bool {
    match tab {
        Tab::Edit => matches!(state.active_tab, Tab::Alignment | Tab::Edit),
        Tab::Morph => state.is_sculpting(),
        other => state.active_tab == other,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct InspectorShellRegions {
    body: Rect,
    footer: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MorphFooterButtons {
    undo: Rect,
    reset: Rect,
    apply: Rect,
}

#[derive(Clone, Copy, Debug, Default)]
struct StatusToastAnimation {
    revision: u64,
    started_at: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct CatalogProgressAnimation {
    displayed: f32,
    last_frame_at: f64,
    running: bool,
    completed_at: Option<f64>,
}

#[derive(Clone, Debug)]
struct PresentedProgress {
    fraction: f32,
    label: String,
    spinner: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PinFooterButtons {
    generate: Rect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TitleDragGesture {
    StartDrag,
    ToggleMaximized,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CompactXyzEdit {
    changed: [bool; 3],
    began: bool,
    committed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MorphFilterLayout {
    rect: Rect,
    category_rect: Rect,
    search_rect: Rect,
    scroll_to_top: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VisibleMorphRow {
    EyeClosure,
    Control(usize),
}

fn inspector_shell_regions(full: Rect, requested_footer_height: f32) -> InspectorShellRegions {
    let footer_height = requested_footer_height.clamp(0.0, full.height().max(0.0));
    let footer_top = full.bottom() - footer_height;
    InspectorShellRegions {
        body: Rect::from_min_max(full.min, pos2(full.right(), footer_top)),
        footer: Rect::from_min_max(pos2(full.left(), footer_top), full.max),
    }
}

fn show_inspector_shell(
    ui: &mut Ui,
    state: &mut AppState,
    id_salt: &'static str,
    footer_height: f32,
    scroll_body: bool,
    body_contents: impl FnOnce(&mut Ui, &mut AppState, Rect),
    footer_contents: impl FnOnce(&mut Ui, &mut AppState, Rect),
) {
    let full = ui.available_rect_before_wrap();
    let regions = inspector_shell_regions(full, footer_height);

    ui.scope_builder(
        UiBuilder::new()
            .id_salt((id_salt, "body"))
            .max_rect(regions.body),
        |ui| {
            ui.shrink_clip_rect(regions.body);
            ui.set_width(regions.body.width());
            publish_inspector_lane_limit(ui, regions.body.bottom());
            if scroll_body {
                ScrollArea::vertical()
                    .id_salt((id_salt, "scroll"))
                    .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                    .auto_shrink([false, false])
                    .max_height(regions.body.height())
                    .show_viewport(ui, |ui, viewport| {
                        let inner_width = ui.available_width().max(0.0);
                        ui.set_width(inner_width);
                        body_contents(ui, state, viewport);
                    });
            } else {
                body_contents(ui, state, regions.body);
            }
        },
    );

    let footer_area = Rect::from_min_max(
        pos2(regions.footer.left() + PANEL_INSET, regions.footer.top()),
        pos2(
            regions.footer.right() - PANEL_INSET,
            regions.footer.bottom(),
        ),
    );
    ui.scope_builder(
        UiBuilder::new()
            .id_salt((id_salt, "footer"))
            .max_rect(footer_area),
        |ui| {
            ui.shrink_clip_rect(regions.footer);
            ui.set_width(footer_area.width());
            footer_contents(ui, state, footer_area);
        },
    );
}

fn inspector_lane_limit_id() -> Id {
    Id::new("vkit.inspector.lane-limit")
}

fn publish_inspector_lane_limit(ui: &Ui, limit: f32) {
    ui.data_mut(|data| data.insert_temp(inspector_lane_limit_id(), limit));
}

fn inspector_list_budget(ui: &Ui, cursor_top: f32, footer: f32) -> Option<f32> {
    let limit = ui.data(|data| data.get_temp::<f32>(inspector_lane_limit_id()))?;
    Some((limit - cursor_top - footer).max(0.0))
}

fn morph_footer_buttons(footer: Rect) -> MorphFooterButtons {
    let footer = footer.shrink2(vec2(0.0, 8.0));
    let available = (footer.height() - MORPH_FOOTER_BUTTON_GAP).max(0.0);
    let authored_total = CONTROL_HEIGHT + MORPH_APPLY_HEIGHT;
    let scale = (available / authored_total).min(1.0);
    let reset_height = CONTROL_HEIGHT * scale;
    let apply_height = MORPH_APPLY_HEIGHT * scale;
    let action_gap = MORPH_FOOTER_BUTTON_GAP.min(footer.width());
    let action_width = ((footer.width() - action_gap).max(0.0) * 0.5).max(0.0);
    MorphFooterButtons {
        undo: Rect::from_min_size(footer.min, vec2(action_width, reset_height)),
        reset: Rect::from_min_size(
            pos2(footer.left() + action_width + action_gap, footer.top()),
            vec2(action_width, reset_height),
        ),
        apply: Rect::from_min_size(
            pos2(footer.left(), footer.bottom() - apply_height),
            vec2(footer.width(), apply_height),
        ),
    }
}

fn pin_footer_buttons(footer: Rect) -> PinFooterButtons {
    let inset = footer.shrink2(vec2(0.0, 8.0));
    let generate_height = PRIMARY_ACTION_HEIGHT.min(inset.height().max(0.0));
    PinFooterButtons {
        generate: Rect::from_min_size(
            pos2(inset.left(), inset.bottom() - generate_height),
            vec2(inset.width(), generate_height),
        ),
    }
}

pub fn draw(root: &mut Ui, state: &mut AppState) {
    let wants_keyboard_input = root.ctx().egui_wants_keyboard_input();

    let undo_requested = root.ctx().input(|input| {
        routes_global_undo(
            state.active_tab,
            input.modifiers.ctrl,
            input.key_pressed(Shortcut::Undo.key()),
            wants_keyboard_input,
        )
    });
    if undo_requested {
        if state.is_detail_editing() {
            crate::viewport::clear_sculpt_pointer_stroke(root.ctx());
        }
        state.dispatch(Action::Undo);
    }
    let x_symmetry_pressed = crate::shortcuts::Shortcut::XSymmetry.pressed(root);
    match routes_x_symmetry(state.active_tab, x_symmetry_pressed, wants_keyboard_input) {
        Some(XSymmetryTarget::Pins) => state.dispatch(Action::ToggleXMirror(!state.x_mirror)),
        Some(XSymmetryTarget::Brush) => {
            state.dispatch(Action::SetSculptXSymmetry(!state.sculpt_x_symmetry));
        }
        None => {}
    }

    let texture_tool_shortcut =
        routes_texture_tool_shortcut(state.active_tab, wants_keyboard_input)
            .then(|| {
                if Shortcut::TexturePinBrush.pressed(root) {
                    Some(crate::texture_project::TextureTool::PinPair)
                } else if Shortcut::TextureCloneBrush.pressed(root) {
                    Some(crate::texture_project::TextureTool::CloneStamp)
                } else {
                    None
                }
            })
            .flatten();
    if let Some(tool) = texture_tool_shortcut {
        state.dispatch(Action::SetTextureTool(tool));
    }

    if let Some(target) = state.take_pending_attention() {
        attention_flash(root.ctx(), attention_target_id(target));
    }
    draw_top_bar(root, state);

    draw_inspector(root, state);
    let workspace_rect = draw_workspace(root, state);

    crate::texture_ui::schedule_texture_auto_bake(root, state);
    draw_status_bar(root, state, workspace_rect);
    draw_export_toast(root, state, workspace_rect);
    draw_overwrite_confirmation(root, state);
    draw_symmetry_change_confirmation(root, state);
    draw_texture_overwrite_confirmation(root, state);
    draw_package_replace_confirmation(root, state);
    draw_recovery_prompt(root, state);
    draw_diagnostic_log_modal(root, state.locale);
    crate::settings::draw_settings_page(root, state);
    draw_window_resize_zones(root);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum XSymmetryTarget {
    Pins,

    Brush,
}

const fn routes_x_symmetry(
    active_tab: Tab,
    x_pressed: bool,
    wants_keyboard_input: bool,
) -> Option<XSymmetryTarget> {
    if !x_pressed || wants_keyboard_input {
        return None;
    }
    match active_tab {
        Tab::Edit => Some(XSymmetryTarget::Pins),
        Tab::Morph | Tab::Texture => Some(XSymmetryTarget::Brush),
        Tab::Alignment | Tab::Result => None,
    }
}

fn routes_texture_tool_shortcut(active_tab: Tab, wants_keyboard_input: bool) -> bool {
    active_tab == Tab::Texture && !wants_keyboard_input
}

pub(crate) fn attention_target_id(target: AttentionTarget) -> Id {
    match target {
        AttentionTarget::CustomHeadLoad => Id::new("vkit.attention.custom-head-load"),
        AttentionTarget::VaMRoot => Id::new("vkit.attention.vam-root"),
        AttentionTarget::SkinPanel => Id::new("vkit.attention.skin-panel"),
        AttentionTarget::SkinTextureMode => Id::new("vkit.attention.skin-texture-mode"),
        AttentionTarget::PackageCreator => Id::new("vkit.attention.package-creator"),
        AttentionTarget::PackageName => Id::new("vkit.attention.package-name"),
    }
}

pub(crate) fn attention_frame_for(
    ui: &Ui,
    target: AttentionTarget,
) -> Option<crate::ui_components::AttentionFrame> {
    crate::ui_components::attention_pulse_now(ui, attention_target_id(target))
}

const fn routes_global_undo(
    active_tab: Tab,
    ctrl_down: bool,
    z_pressed: bool,
    wants_keyboard_input: bool,
) -> bool {
    ctrl_down
        && z_pressed
        && !wants_keyboard_input
        && matches!(
            active_tab,
            Tab::Alignment | Tab::Edit | Tab::Morph | Tab::Texture
        )
}

fn draw_top_bar(root: &mut Ui, state: &mut AppState) {
    Panel::top("vkit.top")
        .exact_size(TOP_BAR_HEIGHT)
        .frame(Frame::new().fill(COLOR_TOPBAR).stroke(Stroke::NONE))
        .show(root, |ui| {
            let rect = ui.max_rect();
            // One layout, worked out once, then read from. Recomputing any of
            // these rects here — or rebuilding the struct to hand onward — is
            // how the capsule and the cell that makes room for it would come to
            // disagree about where the title bar puts them.
            let caption = caption_layout(
                rect,
                title_locale_width(ui),
                title_update_width(ui, state.locale),
            );
            let CaptionLayout {
                bar_rect: _,
                brand_rect,
                vam_rect,
                tabs_rect,
                tab_width,
                locale_rect,
                settings_rect,
                controls_rect,
                update_rect,
            } = caption;
            let active_tab_index = TOP_TABS
                .iter()
                .position(|(tab, _)| visible_tab_is_active(state, *tab))
                .unwrap_or(0);

            let thumb = animate_rect(
                ui,
                Id::new("vkit.top-tabs.thumb"),
                active_tab_index as u64,
                top_tab_cell(tabs_rect, tab_width, active_tab_index),
            );
            ui.painter()
                .rect_filled(thumb, CAPSULE_RADIUS, COLOR_SURFACE_RAISED);
            for (index, (tab, key)) in TOP_TABS.into_iter().enumerate() {
                let cell = top_tab_cell(tabs_rect, tab_width, index);
                let active = visible_tab_is_active(state, tab);

                let available = top_tab_available(state, tab);
                let response = ui.interact(
                    cell,
                    Id::new(("vkit.top-tabs", index)),
                    if available {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                );
                let label = text(state.locale, key);
                let color = if active {
                    COLOR_PRIMARY
                } else if !available {
                    disabled(COLOR_MUTED)
                } else if response.hovered() {
                    COLOR_TEXT
                } else {
                    COLOR_MUTED
                };

                // Just the name. The digit used to sit in front of it to make
                // the binding visible, but four numbers across the top of the
                // window is a lot of furniture for a shortcut, and the tooltip
                // says it just as well to anyone who goes looking.
                let fitted = crate::ui_components::ellipsize_to_width(
                    ui,
                    label,
                    cell.width(),
                    FontId::proportional(FONT_BODY),
                );
                ui.painter().text(
                    cell.center(),
                    Align2::CENTER_CENTER,
                    &fitted,
                    FontId::proportional(FONT_BODY),
                    color,
                );

                response.widget_info(|| {
                    WidgetInfo::selected(WidgetType::Button, available, active, label)
                });
                if available {
                    control_affordances(ui, &response, cell, cell.height() * 0.5);
                }
                let shortcut = (index + 1).to_string();
                let response = tooltip(response, label, Some(&shortcut));
                if available && response.clicked() {
                    state.dispatch(Action::RequestTab(top_tab_target(state, tab)));
                }
            }
            handle_top_tab_keys(ui, state);

            paint_title_brand(ui, brand_rect);
            match update_rect {
                Some(update_rect) => {
                    title_drag_region(
                        ui,
                        Rect::from_min_max(
                            brand_rect.min,
                            pos2(update_rect.left(), brand_rect.bottom()),
                        ),
                        "brand",
                    );
                    draw_title_update_capsule(ui, state, update_rect);
                }
                None => title_drag_region(ui, brand_rect, "brand"),
            }
            draw_title_vam_field(ui, state, vam_rect);
            if tabs_rect.left() > vam_rect.right() {
                title_drag_region(
                    ui,
                    Rect::from_min_max(
                        pos2(vam_rect.right(), rect.top() + 6.0),
                        pos2(tabs_rect.left(), rect.bottom()),
                    ),
                    "before-tabs",
                );
            }
            if locale_rect.left() > tabs_rect.right() {
                title_drag_region(
                    ui,
                    Rect::from_min_max(
                        pos2(tabs_rect.right(), rect.top() + 6.0),
                        pos2(locale_rect.left(), rect.bottom()),
                    ),
                    "after-tabs",
                );
            }

            let mut selected_locale = state.locale;
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(locale_rect)
                    .layout(Layout::top_down(Align::Max)),
                |ui| {
                    set_capsule_widget_radius(ui);
                    ui.spacing_mut().button_padding = vec2(7.0, 3.0);
                    ui.visuals_mut().widgets.inactive.bg_fill = COLOR_BG;
                    ui.visuals_mut().widgets.inactive.weak_bg_fill = COLOR_BG;
                    ui.visuals_mut().widgets.hovered.bg_fill = COLOR_FIELD;
                    ui.visuals_mut().widgets.active.bg_fill = COLOR_FIELD;
                    ui.set_min_width(locale_rect.width());
                    ComboBox::from_id_salt("vkit.locale")
                        .width(locale_rect.width())
                        .selected_text(selected_locale.selector_label())
                        .show_ui(ui, |ui| {
                            for locale in Locale::ALL {
                                ui.selectable_value(
                                    &mut selected_locale,
                                    locale,
                                    locale.selector_label(),
                                );
                            }
                        });
                },
            );
            if selected_locale != state.locale {
                state.dispatch(Action::SetLocale(selected_locale));
            }
            crate::settings::draw_settings_button(ui, state, settings_rect);
            publish_non_client_layout(ui, caption);
            draw_window_buttons(ui, state, controls_rect);
        });
}

mod asset_panels;
pub(crate) use asset_panels::*;
mod morph_list;
mod window_chrome;
pub(crate) use morph_list::*;
use window_chrome::*;

fn draw_status_bar(root: &mut Ui, state: &AppState, viewport_rect: Rect) {
    let rect = Rect::from_min_max(
        pos2(
            viewport_rect.left(),
            (viewport_rect.bottom() - STATUS_BAR_HEIGHT).max(viewport_rect.top()),
        ),
        viewport_rect.right_bottom(),
    );
    egui::Area::new(Id::new("vkit.status"))
        .order(egui::Order::Foreground)
        .fixed_pos(rect.min)
        .movable(false)
        .interactable(true)
        .show(root.ctx(), |ui| {
            ui.set_min_size(rect.size());
            ui.set_max_size(rect.size());
            let rect = Rect::from_min_size(ui.min_rect().min, rect.size());
            let catalog_progress = presented_vam_catalog_progress(ui, state);
            let presented_progress = state
                .progress
                .map(|progress| PresentedProgress {
                    fraction: progress.fraction,
                    label: format!(
                        "{}  {:>3.0}%",
                        progress_stage_text(state.locale, progress.stage),
                        progress.fraction * 100.0
                    ),
                    spinner: progress.stage == JobStage::Import,
                })
                .or(catalog_progress);
            let status_rect = if presented_progress.is_some() {
                Rect::from_min_max(
                    rect.min,
                    pos2(
                        rect.right(),
                        rect.bottom() - PROGRESS_HEIGHT - STATUS_PROGRESS_GAP,
                    ),
                )
            } else {
                rect
            };
            let quiet_asset_ready =
                matches!(state.status.key, TextKey::SkinReady) && presented_progress.is_none();
            let status_response = ui.interact(
                rect,
                Id::new("vkit.status.open-diagnostics"),
                Sense::click(),
            );

            let revealed = ui.ctx().animate_bool(
                Id::new("vkit.status.hover-reveal"),
                status_response.hovered(),
            );
            let opacity =
                status_toast_opacity(ui, state, quiet_asset_ready, presented_progress.is_some())
                    .max(revealed);
            if opacity > 0.0 {
                paint_status_gradient(ui, status_rect, opacity);
            }
            control_affordances(ui, &status_response, rect, 0.0);
            if status_response.clicked() {
                let log_text = diagnostics::read_recent(DIAGNOSTIC_LOG_VIEW_BYTES)
                    .unwrap_or_else(|error| error.to_string());
                ui.ctx().data_mut(|data| {
                    data.insert_temp(diagnostic_log_text_id(), log_text);
                    data.insert_temp(diagnostic_log_opened_id(), true);
                });
            }

            let quiet_asset_ready = quiet_asset_ready && revealed <= 0.0;
            let mut status_text = if quiet_asset_ready {
                String::new()
            } else {
                text(state.locale, state.status.key).to_owned()
            };
            if !quiet_asset_ready && let Some(detail) = &state.status.detail {
                status_text.push_str("  ");
                status_text.push_str(detail);
            }
            let progress_label_width = presented_progress
                .as_ref()
                .map_or(0.0, |_| 168.0_f32.min(status_rect.width() * 0.45));
            let status_clip = Rect::from_min_max(
                status_rect.min,
                pos2(
                    (status_rect.right() - progress_label_width).max(status_rect.left()),
                    status_rect.bottom(),
                ),
            );
            let text_rect = ui.painter().with_clip_rect(status_clip).text(
                pos2(status_rect.left() + 12.0, status_rect.center().y),
                Align2::LEFT_CENTER,
                status_text,
                FontId::proportional(FONT_SM),
                status_color(state.status.tone).gamma_multiply(opacity),
            );

            if let Some(progress) = presented_progress {
                let inline_left = (text_rect.right() + 10.0)
                    .min(status_rect.right() - progress_label_width - 24.0)
                    .max(status_rect.left() + 12.0);
                let mut label_left = inline_left;
                if progress.spinner {
                    let spinner_rect = Rect::from_center_size(
                        pos2(inline_left + 6.5, status_rect.center().y),
                        vec2(13.0, 13.0),
                    );
                    ui.put(spinner_rect, Spinner::new().size(12.0).color(COLOR_PRIMARY));
                    label_left = spinner_rect.right() + 6.0;
                }
                ui.painter().text(
                    pos2(label_left, status_rect.center().y),
                    Align2::LEFT_CENTER,
                    progress.label,
                    FontId::proportional(FONT_XS),
                    COLOR_PRIMARY.gamma_multiply(opacity),
                );
                let progress_rect = Rect::from_min_max(
                    pos2(rect.left(), rect.bottom() - PROGRESS_HEIGHT),
                    rect.max,
                );
                ui.painter()
                    .rect_filled(progress_rect, 0.0, COLOR_FIELD.gamma_multiply(opacity));
                let fill = Rect::from_min_max(
                    progress_rect.min,
                    pos2(
                        progress_rect.left() + progress_rect.width() * progress.fraction,
                        progress_rect.bottom(),
                    ),
                );
                ui.painter()
                    .rect_filled(fill, 0.0, COLOR_PRIMARY.gamma_multiply(opacity));
            }
        });
}

fn status_toast_opacity(ui: &Ui, state: &AppState, quiet: bool, force_visible: bool) -> f32 {
    if force_visible {
        return 1.0;
    }
    if quiet {
        return 0.0;
    }
    let now = ui.ctx().input(|input| input.time);
    let id = Id::new("vkit.status.toast-animation");
    let mut animation = ui
        .ctx()
        .data_mut(|data| data.get_temp::<StatusToastAnimation>(id))
        .unwrap_or_default();
    if animation.revision != state.status.revision {
        animation = StatusToastAnimation {
            revision: state.status.revision,
            started_at: now,
        };
        ui.ctx().data_mut(|data| data.insert_temp(id, animation));
    }
    let elapsed = (now - animation.started_at).max(0.0);
    let fade_in = STATUS_TOAST_FADE_IN_SECONDS;
    let fade_out_start = STATUS_TOAST_HOLD_SECONDS;
    let fade_out_end = fade_out_start + STATUS_TOAST_FADE_OUT_SECONDS;
    let opacity = if elapsed < fade_in {
        status_fade_curve((elapsed / fade_in) as f32)
    } else if elapsed < fade_out_start {
        1.0
    } else if elapsed < fade_out_end {
        1.0 - status_fade_curve(((elapsed - fade_out_start) / STATUS_TOAST_FADE_OUT_SECONDS) as f32)
    } else {
        0.0
    };
    if elapsed < fade_in || (fade_out_start..fade_out_end).contains(&elapsed) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));
    } else if elapsed < fade_out_start {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs_f64(fade_out_start - elapsed));
    }
    opacity
}

fn presented_vam_catalog_progress(ui: &Ui, state: &AppState) -> Option<PresentedProgress> {
    let now = ui.ctx().input(|input| input.time);
    let id = Id::new("vkit.vam-catalog.progress-animation");
    let mut animation = ui
        .ctx()
        .data_mut(|data| data.get_temp::<CatalogProgressAnimation>(id))
        .unwrap_or_default();
    let indexing = matches!(state.vam_catalog_status, VaMCatalogStatus::Indexing);
    let ready = matches!(state.vam_catalog_status, VaMCatalogStatus::Ready { .. });

    if indexing {
        if !animation.running {
            animation = CatalogProgressAnimation {
                displayed: 0.0,
                last_frame_at: now,
                running: true,
                completed_at: None,
            };
        }
        let delta_seconds = (now - animation.last_frame_at).clamp(0.0, 0.05) as f32;
        animation.last_frame_at = now;
        let reported = state.vam_catalog_progress.clamp(0.0, 0.99);
        animation.displayed = advance_catalog_progress(
            animation.displayed,
            reported,
            catalog_progress_soft_ceiling(reported),
            delta_seconds,
            false,
        );
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));
    } else if ready && animation.running {
        let delta_seconds = (now - animation.last_frame_at).clamp(0.0, 0.05) as f32;
        animation.last_frame_at = now;
        animation.displayed =
            advance_catalog_progress(animation.displayed, 1.0, 1.0, delta_seconds, true);
        if animation.displayed >= 1.0 {
            animation.running = false;
            animation.completed_at = Some(now);
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));
    } else if !ready {
        animation = CatalogProgressAnimation::default();
    }

    let holding_completion = animation
        .completed_at
        .is_some_and(|completed_at| now - completed_at < CATALOG_PROGRESS_HOLD_SECONDS);
    let visible = animation.running || holding_completion;
    ui.ctx().data_mut(|data| data.insert_temp(id, animation));
    visible.then(|| PresentedProgress {
        fraction: animation.displayed.clamp(0.0, 1.0),
        label: format!("{:>3.0}%", animation.displayed * 100.0),
        spinner: animation.running && indexing,
    })
}

fn catalog_progress_soft_ceiling(reported: f32) -> f32 {
    match reported {
        value if value < 0.08 => 0.08,
        value if value < 0.12 => 0.11,
        value if value < 0.55 => 0.54,
        value if value < 0.60 => 0.59,
        value if value < 0.84 => 0.83,
        value if value < 0.90 => 0.89,
        value if value < 0.97 => 0.96,
        _ => 0.985,
    }
}

fn advance_catalog_progress(
    displayed: f32,
    reported: f32,
    ceiling: f32,
    delta_seconds: f32,
    finishing: bool,
) -> f32 {
    if delta_seconds <= 0.0 {
        return displayed;
    }
    if finishing {
        return (displayed + delta_seconds * 1.8).min(1.0);
    }
    let catch_up = (reported - displayed).max(0.0) * delta_seconds * 8.0;
    let idle_motion = delta_seconds * 0.028;
    let maximum_step = delta_seconds * 0.45;
    (displayed + catch_up.max(idle_motion).min(maximum_step))
        .min(ceiling)
        .max(displayed)
}

fn status_fade_curve(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn paint_status_gradient(ui: &Ui, rect: Rect, opacity: f32) {
    let mut mesh = egui::Mesh::default();
    let top = Color32::from_black_alpha(42).gamma_multiply(opacity);
    let bottom = Color32::from_black_alpha(172).gamma_multiply(opacity);
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(2, 1, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
}

fn diagnostic_log_text_id() -> Id {
    Id::new("vkit.diagnostic-log.text")
}

fn diagnostic_log_opened_id() -> Id {
    Id::new("vkit.diagnostic-log.opened-this-frame")
}

fn diagnostic_log_show_system_id() -> Id {
    Id::new("vkit.diagnostic-log.show-system")
}

fn diagnostic_line_is_system(line: &str) -> bool {
    line.split('\t').nth(1) == Some("DEBUG")
}

fn simplify_diagnostic_log(raw: &str, locale: Locale) -> String {
    struct Entry {
        time: String,
        severity: &'static str,
        message: String,
        count: usize,
    }

    fn severity_tag(severity: &str) -> Option<&'static str> {
        match severity {
            "INFO" => Some(""),
            "WARN" => Some("!"),
            "ERROR" => Some("×"),
            _ => None,
        }
    }
    let mut entries: Vec<Entry> = Vec::new();
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(5, '\t');
        let timestamp = parts.next().unwrap_or_default();
        let (severity, message) = match parts.next().map(severity_tag) {
            Some(Some(tag)) => {
                let _module = parts.next();
                let event = parts.next().unwrap_or_default();
                let detail = parts.next().unwrap_or_default();

                let message = match crate::i18n::log_event_label(locale, event) {
                    Some(label) => {
                        let is_failure = tag != "INFO ";
                        if is_failure && !detail.is_empty() {
                            format!("{label} — {detail}")
                        } else if let Some(name) = diagnostic_detail_file_name(detail) {
                            format!("{label} — {name}")
                        } else {
                            label.to_owned()
                        }
                    }
                    None => if detail.is_empty() { event } else { detail }.to_owned(),
                };
                (tag, message)
            }
            Some(None) if diagnostic_line_is_system(line) => continue,

            _ => ("", line.to_owned()),
        };
        let mut message = message.replace('\t', "  ");
        if message.len() > DIAGNOSTIC_LOG_MESSAGE_LIMIT {
            let mut end = DIAGNOSTIC_LOG_MESSAGE_LIMIT;
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            message.truncate(end);
            message.push('…');
        }

        let time = iso_clock(timestamp).map_or_else(String::new, str::to_owned);
        match entries.last_mut() {
            Some(last) if last.severity == severity && last.message == message => {
                last.count += 1;
                last.time = time;
            }
            _ => entries.push(Entry {
                time,
                severity,
                message,
                count: 1,
            }),
        }
    }
    let start = entries.len().saturating_sub(DIAGNOSTIC_LOG_SIMPLE_ROWS);
    let mut visible = String::new();
    for entry in &entries[start..] {
        visible.push_str(&entry.time);
        visible.push_str("  ");

        if entry.severity.is_empty() {
            visible.push_str("  ");
        } else {
            visible.push_str(entry.severity);
            visible.push(' ');
        }
        visible.push_str(&entry.message);
        if entry.count > 1 {
            visible.push_str(&format!("  (x{})", entry.count));
        }
        visible.push('\n');
    }
    visible
}

fn diagnostic_detail_file_name(detail: &str) -> Option<&str> {
    let path = detail
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("path="))?;
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path).trim();
    (!name.is_empty()).then_some(name)
}

fn filter_diagnostic_log(raw: &str, show_system: bool, locale: Locale) -> String {
    if show_system {
        return system_diagnostic_log(raw);
    }
    simplify_diagnostic_log(raw, locale)
}

#[cfg(test)]
mod log_view_tests {
    use super::*;

    const RAW: &str = "2026-07-29T00:37:20.186\tINFO\tapp\tstartup\tversion=0.3.0\n\
                       2026-07-29T00:44:14.217\tERROR\truntime\tstatus\tskin load failed: no UV\n";

    #[test]
    fn the_system_view_drops_the_date_the_millis_and_the_severity_word() {
        let shown = system_diagnostic_log(RAW);
        assert!(shown.contains("00:37:20"), "{shown}");
        assert!(!shown.contains("2026-07-29"), "no date: {shown}");
        assert!(!shown.contains(".186"), "no milliseconds: {shown}");
        assert!(!shown.contains("INFO"), "no severity word: {shown}");
        assert!(!shown.contains("runtime"), "no subsystem: {shown}");
        assert!(shown.contains("startup"), "the event survives: {shown}");
        assert!(
            shown.contains("version=0.3.0"),
            "the detail survives: {shown}"
        );
    }

    #[test]
    fn a_failure_is_marked_and_carries_its_detail() {
        let shown = system_diagnostic_log(RAW);
        let line = shown
            .lines()
            .find(|line| line.contains("skin load failed"))
            .expect("the error line is shown");
        assert!(line.contains(" × "), "marked as a failure: {line}");
        assert!(line.contains("no UV"), "the reason is the payload: {line}");
    }

    #[test]
    fn a_line_the_writer_did_not_stamp_survives_whole() {
        let shown = system_diagnostic_log("thread 'main' panicked at src/x.rs:1:2\n");
        assert!(
            shown.contains("thread 'main' panicked at src/x.rs:1:2"),
            "{shown}"
        );
    }
}

fn iso_clock(timestamp: &str) -> Option<&str> {
    let bytes = timestamp.as_bytes();
    (bytes.len() >= 19
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[11..19]
            .iter()
            .all(|byte| byte.is_ascii_digit() || *byte == b':'))
    .then(|| &timestamp[11..19])
}

fn system_diagnostic_log(raw: &str) -> String {
    let mut visible = String::new();
    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(5, '\t');
        let timestamp = parts.next().unwrap_or_default();
        let severity = parts.next().unwrap_or_default();
        let Some(clock) = iso_clock(timestamp) else {
            visible.push_str(line);
            visible.push('\n');
            continue;
        };
        let _subsystem = parts.next();
        let event = parts.next().unwrap_or_default();
        let detail = parts.next().unwrap_or_default();
        visible.push_str(clock);
        visible.push_str(match severity {
            "WARN" => "  ! ",
            "ERROR" => "  × ",
            _ => "    ",
        });
        visible.push_str(event);
        if !detail.is_empty() {
            visible.push_str("  ");
            visible.push_str(&detail.replace('\t', "  "));
        }
        visible.push('\n');
    }
    visible
}

fn draw_diagnostic_log_modal(root: &mut Ui, locale: Locale) {
    let text_id = diagnostic_log_text_id();
    let opened_id = diagnostic_log_opened_id();
    let show_system_id = diagnostic_log_show_system_id();
    let Some(log_text) = root.ctx().data(|data| data.get_temp::<String>(text_id)) else {
        return;
    };
    let opened_this_frame = root
        .ctx()
        .data(|data| data.get_temp::<bool>(opened_id))
        .unwrap_or(false);
    let mut show_system = root
        .ctx()
        .data(|data| data.get_temp::<bool>(show_system_id))
        .unwrap_or(false);
    let content = root.ctx().content_rect().size();
    let width = (content.x - 64.0).clamp(320.0, DIAGNOSTIC_LOG_MODAL_MAX_WIDTH);
    let height = (content.y - 96.0).clamp(240.0, DIAGNOSTIC_LOG_MODAL_MAX_HEIGHT);
    let modal = egui::Modal::new(Id::new("vkit.diagnostic-log.modal"))
        .frame(
            Frame::new()
                .fill(COLOR_SURFACE_RAISED)
                .stroke(Stroke::NONE)
                .corner_radius(CONTROL_RADIUS)
                .inner_margin(Margin::same(12)),
        )
        .show(root.ctx(), |ui| {
            ui.set_width(width);
            ui.set_height(height);
            ui.horizontal(|ui| {
                switch(ui, &mut show_system, text(locale, TextKey::SystemLog));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(
                            Button::new(text(locale, TextKey::CopyAll))
                                .min_size(vec2(88.0, CONTROL_H_DENSE))
                                .corner_radius(CAPSULE_RADIUS),
                        )
                        .clicked()
                    {
                        ui.ctx().copy_text(log_text.clone());
                    }
                });
            });
            ui.add_space(SPACE_2);
            let visible_text = filter_diagnostic_log(&log_text, show_system, locale);
            let rows = visible_text.lines().count().max(1);
            let log_height = (height - 40.0).max(120.0);
            ScrollArea::both()
                .id_salt("vkit.diagnostic-log.scroll")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .max_height(log_height)
                .show(ui, |ui| {
                    let copy_requested = ui.input_mut(|input| {
                        let shortcut = input.consume_key(egui::Modifiers::COMMAND, egui::Key::C);
                        let mut copy_event = false;
                        input.events.retain(|event| {
                            if matches!(event, egui::Event::Copy) {
                                copy_event = true;
                                return false;
                            }
                            true
                        });
                        shortcut || copy_event
                    });
                    let output = TextEdit::multiline(&mut visible_text.as_str())
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(rows)
                        .frame(Frame::NONE)
                        .show(ui);
                    if copy_requested {
                        let selected = output
                            .cursor_range
                            .map(|range| {
                                let [start, end] = range.sorted_cursors();
                                (start.index.0, end.index.0)
                            })
                            .filter(|(start, end)| end > start)
                            .and_then(|(start, end)| {
                                let selection: String =
                                    visible_text.chars().skip(start).take(end - start).collect();
                                (!selection.is_empty()).then_some(selection)
                            });

                        ui.ctx()
                            .copy_text(selected.unwrap_or_else(|| visible_text.clone()));
                    }
                });
        });
    if modal.should_close() && !opened_this_frame {
        root.ctx().data_mut(|data| {
            data.remove::<String>(text_id);
            data.remove::<bool>(opened_id);
        });
    } else {
        root.ctx().data_mut(|data| {
            data.insert_temp(text_id, log_text);
            data.insert_temp(opened_id, false);
        });
    }
    root.ctx().data_mut(|data| {
        data.insert_temp(show_system_id, show_system);
    });
}

fn draw_inspector(root: &mut Ui, state: &mut AppState) {
    let panel_id = Id::new("vkit.inspector");
    let preferred_width = clamp_inspector_width(state.inspector_width);
    let shown = Panel::right(panel_id)
        .exact_size(preferred_width)
        .resizable(false)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(COLOR_SURFACE)
                .stroke(Stroke::NONE)
                .inner_margin(Margin::same(PANEL_INSET as i8)),
        )
        .show(root, |ui| match state.active_tab {
            Tab::Alignment => draw_alignment_inspector(ui, state),
            Tab::Edit => draw_edit_inspector(ui, state),
            Tab::Result => draw_result_inspector(ui, state),
            Tab::Morph | Tab::Texture => draw_morph_inspector(ui, state),
        });

    let divider_x = shown.response.rect.left();
    let resize_rect = Rect::from_min_max(
        pos2(
            divider_x - crate::theme::INSPECTOR_RESIZE_GRAB_RADIUS,
            shown.response.rect.top(),
        ),
        pos2(
            divider_x + crate::theme::INSPECTOR_RESIZE_GRAB_RADIUS,
            shown.response.rect.bottom(),
        ),
    );
    let resize_response =
        horizontal_resize_handle(root, panel_id.with("manual_resize"), resize_rect);
    if resize_response.dragged()
        && let Some(pointer) = resize_response.interact_pointer_pos()
    {
        state.inspector_width = clamp_inspector_width(shown.response.rect.right() - pointer.x);
        root.ctx().request_repaint();
    }

    if resize_response.dragged() || resize_response.hovered() {
        let divider_color = if resize_response.dragged() {
            COLOR_PRIMARY
        } else {
            COLOR_MUTED
        };
        root.painter().vline(
            shown.response.rect.left(),
            shown.response.rect.y_range(),
            Stroke::new(INSPECTOR_ACTIVE_DIVIDER_WIDTH, divider_color),
        );
    }

    if resize_response.double_clicked() {
        state.inspector_width = INSPECTOR_DEFAULT_WIDTH;
        root.ctx().request_repaint();
    }
}

fn draw_alignment_inspector(ui: &mut Ui, state: &mut AppState) {
    show_inspector_shell(
        ui,
        state,
        "alignment.inspector",
        ALIGNMENT_FOOTER_HEIGHT,
        true,
        |ui, state, _viewport| {
            ui.add_enabled_ui(!state.busy(), |ui| {
                if state.edit_source_mode == EditSourceMode::ScanHead {
                    edit_align_section(ui, state);
                    alignment_view_section(ui, state);
                } else {
                    edit_source_section(ui, state);
                    ui.add_space(SPACE_4);
                }
            });
        },
        |ui, state, footer| {
            let button_rect = primary_action_rect(footer);
            let scan_flow = state.edit_source_mode == EditSourceMode::ScanHead;
            let enabled = !state.busy() && state.workspace.template_geometry.is_some();
            let response = next_step_button(
                ui,
                button_rect,
                text(
                    state.locale,
                    if scan_flow {
                        TextKey::OverlayDone
                    } else {
                        TextKey::EditDetails
                    },
                ),
                enabled,
            );
            if response.clicked() {
                state.dispatch(if scan_flow {
                    Action::RequestTab(Tab::Edit)
                } else {
                    Action::BeginDirectEdit
                });
            }
        },
    );
}

fn guide_glow_under(ui: &Ui, rect: Rect, radius: u8) {
    let time = ui.input(|input| input.time);
    crate::guidance::glow_under(ui.painter(), rect, radius, time);

    ui.ctx().request_repaint();
}

fn guide_glow_over(ui: &Ui, rect: Rect, radius: u8) {
    let time = ui.input(|input| input.time);
    crate::guidance::glow_over(ui.painter(), rect, radius, time);
    ui.ctx().request_repaint();
}

fn guiding(state: &AppState, step: crate::guidance::NextStep) -> bool {
    crate::guidance::next_step(state) == Some(step)
}

fn draw_edit_inspector(ui: &mut Ui, state: &mut AppState) {
    show_inspector_shell(
        ui,
        state,
        "edit.inspector",
        PRIMARY_FOOTER_HEIGHT,
        true,
        |ui, state, _viewport| {
            ui.add_enabled_ui(!state.busy(), |ui| {
                edit_source_section(ui, state);
                ui.add_space(SPACE_3);

                let has_scan = state.scan_path.is_some();

                let (label, tooltip, action) = if has_scan {
                    (
                        TextKey::PlaceHead,
                        TextKey::PlaceHeadTooltip,
                        Action::RequestTab(Tab::Alignment),
                    )
                } else {
                    (
                        TextKey::AddHeadFileAction,
                        TextKey::AddHeadFileActionTooltip,
                        Action::RequestOpenScan,
                    )
                };

                let width = (ui.available_width() - PLACE_HEAD_SIDE_INSET * 2.0).max(0.0);
                let (row, _) = ui.allocate_exact_size(
                    vec2(ui.available_width(), crate::theme::PLACE_HEAD_HEIGHT),
                    egui::Sense::hover(),
                );
                let go_rect = Rect::from_center_size(
                    row.center(),
                    vec2(width, crate::theme::PLACE_HEAD_HEIGHT),
                );
                let leading = guiding(state, crate::guidance::NextStep::PlaceHead)
                    || (!has_scan && guiding(state, crate::guidance::NextStep::LoadScan));
                if leading {
                    guide_glow_under(ui, go_rect, PLACE_HEAD_RADIUS);
                }
                let go = ui
                    .scope(|ui| {
                        if leading {
                            let widgets = &mut ui.visuals_mut().widgets;
                            for visuals in [&mut widgets.inactive, &mut widgets.hovered] {
                                visuals.bg_fill = visuals.bg_fill.gamma_multiply(0.35);
                                visuals.weak_bg_fill = visuals.weak_bg_fill.gamma_multiply(0.35);
                            }
                        }
                        ui.put(
                            go_rect,
                            Button::new(
                                RichText::new(text(state.locale, label)).size(FONT_HEADING),
                            )
                            .corner_radius(PLACE_HEAD_RADIUS),
                        )
                    })
                    .inner;
                if go.on_hover_text(text(state.locale, tooltip)).clicked() {
                    state.dispatch(action);
                }
                ui.add_space(SPACE_3);

                edit_view_section(ui, state);
                edit_pins_section(ui, state);

                if state.scan_path.is_some() {
                    ui.add_space(SPACE_3);
                    alignment_geometry_section(ui, state);
                }
            });
        },
        |ui, state, footer| {
            let busy = state.busy();

            let buttons = pin_footer_buttons(footer);
            let generate = if busy {
                ui.put(
                    buttons.generate,
                    Button::new(
                        RichText::new(text(state.locale, TextKey::Cancel))
                            .size(FONT_HEADING)
                            .color(COLOR_TEXT),
                    )
                    .min_size(buttons.generate.size())
                    .fill(COLOR_DESTRUCTIVE)
                    .stroke(Stroke::NONE)
                    .corner_radius(capsule_radius_for(buttons.generate)),
                )
            } else {
                let step = if state.scan_path.is_some() {
                    TextKey::Generate
                } else {
                    TextKey::ContinueStep
                };
                let button = next_step_button(ui, buttons.generate, text(state.locale, step), true);
                // Over, not under. This button fills its whole rect opaquely, so
                // a glow laid down first was covered the instant it painted —
                // the step that most wants pointing at was the one step that
                // never pulsed.
                if crate::guidance::next_step(state) == Some(crate::guidance::NextStep::Generate) {
                    guide_glow_over(ui, buttons.generate, capsule_radius_for(buttons.generate));
                }
                if step == TextKey::ContinueStep {
                    button.on_hover_text(text(state.locale, TextKey::ContinueStepTooltip))
                } else {
                    button
                }
            };
            if generate.clicked() {
                state.dispatch(if busy {
                    Action::CancelGeneration
                } else if state.scan_path.is_some() {
                    Action::BeginGeneration
                } else {
                    Action::RequestTab(Tab::Morph)
                });
            }
        },
    );
}

fn edit_source_section(ui: &mut Ui, state: &mut AppState) {
    set_capsule_widget_radius(ui);
    ui.spacing_mut().item_spacing.y = SPACE_2;

    if let Some(scan_name) = state.scan_name().map(str::to_owned)
        && ui.add(CapsuleFieldButton::new(&scan_name, true)).clicked()
    {
        state.dispatch(Action::RequestOpenScan);
    }

    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), CONTROL_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            animated_segmented_group(
                ui,
                "vkit.figure.sex",
                2,
                usize::from(state.figure_sex == FigureSex::Male),
                |ui, segment_width| {
                    for (value, key) in [
                        (FigureSex::Female, TextKey::Female),
                        (FigureSex::Male, TextKey::Male),
                    ] {
                        if segment_button(
                            ui,
                            segment_width,
                            text(state.locale, key),
                            state.figure_sex == value,
                        )
                        .clicked()
                        {
                            state.dispatch(Action::SetFigureSex(value));
                        }
                    }
                },
            );
        },
    );
    section_end(ui);
}

fn draw_vam_edit_source_picker(
    ui: &mut Ui,
    state: &mut AppState,
    list_height_override: Option<f32>,
) {
    capsule_search_field(
        ui,
        "vkit.vam-edit-source.search",
        &mut state.vam_edit_query,
        text(state.locale, TextKey::AppearanceSearch),
        !state.vam_edit_sources.is_empty(),
    );

    let query = state.vam_edit_query.trim().to_lowercase();
    let mut hidden_for_other_figure = 0_usize;
    let filtered = state
        .vam_edit_sources
        .iter()
        .enumerate()
        .filter_map(|(index, source)| {
            if source.kind != VaMEditSourceKind::AppearancePreset {
                return None;
            }
            if !(query.is_empty()
                || source.label.to_lowercase().contains(&query)
                || source.stable_id.to_lowercase().contains(&query))
            {
                return None;
            }
            if !state.look_suits_current_figure(source) {
                hidden_for_other_figure += 1;
                return None;
            }
            Some(index)
        })
        .collect::<Vec<_>>();
    let mut selected = None;
    let mut cleared = false;

    let footer_reserve =
        f32::from(VAM_EDIT_SOURCE_FRAME_MARGIN) * 2.0 + SECTION_GAP + SPACE_4 + SPACE_2;

    let list_height = list_height_override.unwrap_or_else(|| {
        inspector_list_budget(ui, ui.cursor().top(), footer_reserve)
            .map_or(VAM_EDIT_SOURCE_LIST_HEIGHT, |budget| {
                budget.max(VAM_EDIT_SOURCE_MIN_LIST_HEIGHT)
            })
    });
    Frame::new()
        .fill(COLOR_FIELD)
        .stroke(Stroke::NONE)
        .corner_radius(CAPSULE_RADIUS)
        .inner_margin(Margin::same(VAM_EDIT_SOURCE_FRAME_MARGIN))
        .show(ui, |ui| {
            ui.set_width(ui.available_width().max(0.0));
            ScrollArea::vertical()
                .id_salt("vkit.vam-edit-source.list")
                .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                .max_height(list_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().max(0.0));

                    let from_base = state.selected_vam_edit_source_id.is_none();
                    if selectable_list_row(ui, text(state.locale, TextKey::BaseFace), from_base)
                        .clicked()
                        && !from_base
                    {
                        cleared = true;
                    }
                    if filtered.is_empty() {
                        let label =
                            if matches!(state.vam_catalog_status, VaMCatalogStatus::Indexing) {
                                text(state.locale, TextKey::VaMIndexing)
                            } else {
                                text(state.locale, TextKey::NoMatchingMorphs)
                            };
                        ui.add_sized(
                            [ui.available_width(), CONTROL_HEIGHT],
                            egui::Label::new(RichText::new(label).color(COLOR_MUTED)),
                        );
                    }
                    for index in &filtered {
                        let source = &state.vam_edit_sources[*index];
                        let selected_row = state.selected_vam_edit_source_id.as_deref()
                            == Some(source.stable_id.as_str());
                        let response = vam_edit_source_row(
                            ui,
                            state.locale,
                            &source.label,
                            selected_row,
                            &source.path.to_string_lossy(),
                            source.resolves_nothing(),
                        );
                        if response.clicked() {
                            selected = Some(source.stable_id.clone());
                        }
                    }

                    if hidden_for_other_figure > 0 {
                        ui.add_space(SPACE_2);
                        ui.add_sized(
                            [ui.available_width(), CONTROL_HEIGHT],
                            egui::Label::new(
                                RichText::new(format!(
                                    "{} ({hidden_for_other_figure})",
                                    text(state.locale, TextKey::LooksHiddenForOtherFigure)
                                ))
                                .color(COLOR_MUTED),
                            ),
                        );
                    }
                });
        });
    if cleared {
        state.dispatch(Action::SelectBaseFace);
    }
    if let Some(id) = selected {
        state.dispatch(Action::SelectVaMEditSource(id));
    }
}

fn selectable_list_row(ui: &mut Ui, label: &str, selected: bool) -> Response {
    selectable_list_row_styled(ui, label, selected, true)
}

fn selectable_list_row_styled(
    ui: &mut Ui,
    label: &str,
    selected: bool,
    available: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width().max(0.0), CONTROL_HEIGHT),
        Sense::click(),
    );
    paint_list_row_highlight(ui, rect, selected, response.hovered());
    let ink = if available {
        COLOR_TEXT
    } else {
        COLOR_DESTRUCTIVE
    };
    ui.painter().with_clip_rect(rect).text(
        pos2(rect.left() + SPACE_3, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(FONT_SM),
        ink,
    );
    if !available {
        ui.painter().text(
            pos2(rect.right() - SPACE_3, rect.center().y),
            Align2::RIGHT_CENTER,
            "!",
            FontId::proportional(FONT_SM),
            COLOR_DESTRUCTIVE,
        );
    }
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, selected, label));
    control_affordances(ui, &response, rect, CONTROL_RADIUS as f32);
    response
}

fn vam_edit_source_row(
    ui: &mut Ui,
    locale: Locale,
    label: &str,
    selected: bool,
    tooltip: &str,
    resolves_nothing: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width().max(0.0), CONTROL_HEIGHT),
        Sense::click(),
    );
    paint_list_row_highlight(ui, rect, selected, response.hovered());

    // A look with a few unresolved morph references is drawn like any other.
    // We know one of its files is gone; we cannot know what that file would
    // have contributed to the face, so there is nothing here to warn about
    // that would not be guesswork -- and guesswork in the destructive colour,
    // on row after row, buries the file path that the tooltip is for. The
    // detail is in vkit.log. Only a look where nothing resolves is dimmed:
    // that one certainly opens as the untouched base face.
    let label_ink = if resolves_nothing {
        COLOR_MUTED
    } else {
        COLOR_TEXT
    };
    let label_rect = Rect::from_min_max(
        pos2(rect.left() + 10.0, rect.top()),
        pos2(rect.right() - 8.0, rect.bottom()),
    );
    ui.painter().with_clip_rect(label_rect).text(
        pos2(label_rect.left(), label_rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(FONT_SM),
        label_ink,
    );
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, selected, label));
    control_affordances(ui, &response, rect, CONTROL_RADIUS as f32);
    if resolves_nothing {
        response.on_hover_text(format!(
            "{}\n{tooltip}",
            text(locale, TextKey::SourceMorphMissing)
        ))
    } else {
        response.on_hover_text(tooltip)
    }
}

fn edit_align_section(ui: &mut Ui, state: &mut AppState) {
    draw_transform_grid(ui, state);
    ui.add_space(SPACE_3);
    let gap = ui.spacing().item_spacing.x;
    let width = ((ui.available_width() - gap).max(0.0) * 0.5).max(0.0);
    ui.horizontal(|ui| {
        if ui
            .add_sized(
                [width, CONTROL_HEIGHT],
                Button::new(text(state.locale, TextKey::PlacementReset))
                    .corner_radius(CAPSULE_RADIUS),
            )
            .clicked()
        {
            state.dispatch(Action::PlacementReset);
        }
        if ui
            .add_sized(
                [width, CONTROL_HEIGHT],
                Button::new(text(state.locale, TextKey::AutoAlign)).corner_radius(CAPSULE_RADIUS),
            )
            .clicked()
        {
            state.dispatch(Action::AutoAlign);
        }
    });
    section_end(ui);
}

fn draw_transform_grid(ui: &mut Ui, state: &mut AppState) {
    const HEADER_HEIGHT: f32 = 20.0;
    const GAP: f32 = 4.0;
    let width = ui.available_width().max(0.0);
    let height = HEADER_HEIGHT + GAP + CONTROL_HEIGHT * 3.0 + GAP * 2.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let axis_label_width = transform_grid_axis_label_width(rect.width());
    let column_width = transform_grid_column_width(rect.width(), GAP);
    let mut position = state.transform.translation_cm;
    let mut rotation = state.transform.rotation_degrees;
    let mut scale = state.transform.scale_xyz;
    let mut edits = [CompactXyzEdit::default(); 3];

    for (column, label) in [TextKey::Position, TextKey::Rotation, TextKey::Scale]
        .into_iter()
        .enumerate()
    {
        let header = Rect::from_min_size(
            pos2(
                rect.left() + axis_label_width + GAP + column as f32 * (column_width + GAP),
                rect.top(),
            ),
            vec2(column_width, HEADER_HEIGHT),
        );
        let label_offset = if column == 2 { -8.0 } else { 0.0 };
        ui.painter().text(
            pos2(header.center().x + label_offset, header.center().y),
            Align2::CENTER_CENTER,
            text(state.locale, label),
            FontId::proportional(FONT_SM),
            COLOR_MUTED,
        );
        if column == 2 {
            let link_rect = Rect::from_center_size(
                pos2(
                    header.center().x + 30.0_f32.min(column_width * 0.35),
                    header.center().y,
                ),
                Vec2::splat(HEADER_HEIGHT),
            );
            let response = ui.interact(link_rect, Id::new("vkit.scale.link.grid"), Sense::click());
            response.widget_info(|| {
                scale_link_widget_info(state.locale, ui.is_enabled(), state.scale_linked)
            });
            paint_rotated_chain_icon(ui, link_rect, state.scale_linked, response.hovered());
            if response.clicked() {
                state.dispatch(Action::SetScaleLinked(!state.scale_linked));
            }
            response.on_hover_text(text(state.locale, TextKey::ScaleLinkTooltip));
        }
    }

    for axis in 0..3 {
        let row_top = rect.top() + HEADER_HEIGHT + GAP + axis as f32 * (CONTROL_HEIGHT + GAP);
        let axis_rect = Rect::from_min_size(
            pos2(rect.left(), row_top),
            vec2(axis_label_width, CONTROL_HEIGHT),
        );
        ui.painter().text(
            axis_rect.center(),
            Align2::CENTER_CENTER,
            ["X", "Y", "Z"][axis],
            FontId::proportional(FONT_SM),
            [COLOR_AXIS_X, COLOR_AXIS_Y, COLOR_AXIS_Z][axis],
        );
        for (column, edit) in edits.iter_mut().enumerate() {
            let cell = Rect::from_min_size(
                pos2(
                    rect.left() + axis_label_width + GAP + column as f32 * (column_width + GAP),
                    row_top,
                ),
                vec2(column_width, CONTROL_HEIGHT),
            );
            let (values, speed): (&mut [f64; 3], f64) = match column {
                0 => (&mut position, 0.01),
                1 => (&mut rotation, 0.5),
                _ => (&mut scale, 0.01),
            };
            let response = transform_grid_value(ui, cell, axis, &mut values[axis], speed);
            edit.changed[axis] = response.changed();
            edit.began |= response.drag_started() || response.gained_focus();
            edit.committed |= response.drag_stopped() || response.lost_focus();
        }
    }

    for edit in edits {
        begin_alignment_edit_if_needed(state, edit);
    }
    for axis in 0..3 {
        if edits[0].changed[axis] {
            state.dispatch(Action::SetPosition {
                axis,
                value_cm: position[axis],
            });
        }
        if edits[1].changed[axis] {
            state.dispatch(Action::SetRotation {
                axis,
                value_degrees: rotation[axis],
            });
        }
        if edits[2].changed[axis] {
            state.dispatch(Action::SetScaleAxis {
                axis,
                value: scale[axis],
            });
        }
    }
    for edit in edits {
        commit_alignment_edit_if_needed(state, edit);
    }
}

fn transform_grid_column_width(available_width: f32, gap: f32) -> f32 {
    let gap = gap.max(0.0);
    ((available_width.max(0.0) - transform_grid_axis_label_width(available_width) - gap * 3.0)
        .max(0.0)
        / 3.0)
        .max(0.0)
}

fn transform_grid_axis_label_width(available_width: f32) -> f32 {
    TRANSFORM_AXIS_LABEL_WIDTH.min(available_width.max(0.0))
}

fn transform_grid_value(
    ui: &mut Ui,
    rect: Rect,
    axis: usize,
    value: &mut f64,
    speed: f64,
) -> Response {
    let mut child = ui.new_child(
        UiBuilder::new()
            .id_salt(("transform.grid", axis, rect.left().to_bits()))
            .max_rect(rect)
            .layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
    );
    child.shrink_clip_rect(rect);
    child.visuals_mut().widgets.inactive.bg_fill = COLOR_FIELD;
    child.visuals_mut().widgets.inactive.weak_bg_fill = COLOR_FIELD;
    child.add_sized(
        rect.size(),
        DragValue::new(value).speed(speed).max_decimals(4),
    )
}

fn paint_rotated_chain_icon(ui: &Ui, rect: Rect, linked: bool, hovered: bool) {
    let color = if linked || hovered {
        COLOR_TEXT
    } else {
        COLOR_MUTED
    };
    let side = SCALE_LINK_ICON_SIZE.min(rect.width()).min(rect.height());
    paint_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(side)),
        Icon::Chain,
        color,
    );
}

fn scale_link_widget_info(locale: Locale, enabled: bool, linked: bool) -> WidgetInfo {
    WidgetInfo::selected(
        WidgetType::Button,
        enabled,
        linked,
        text(locale, TextKey::ScaleLink),
    )
}

fn begin_alignment_edit_if_needed(state: &mut AppState, edit: CompactXyzEdit) {
    if edit.began {
        state.dispatch(Action::BeginAlignmentTransform);
    }
}

fn commit_alignment_edit_if_needed(state: &mut AppState, edit: CompactXyzEdit) {
    if edit.committed {
        state.dispatch(Action::CommitAlignmentTransform);
    }
}

fn alignment_geometry_section(ui: &mut Ui, state: &mut AppState) {
    section_heading(ui, text(state.locale, TextKey::ScanSymmetry));

    if state.scan_symmetry == ScanSymmetry::Original {
        ui.horizontal(|ui| {
            let (icon_rect, _) =
                ui.allocate_exact_size(Vec2::splat(FONT_BODY), egui::Sense::hover());
            paint_icon(ui.painter(), icon_rect, Icon::Caution, COLOR_MUTED);
            ui.add_space(crate::theme::SPACE_1);
            ui.label(
                RichText::new(text(state.locale, TextKey::SymmetrySuggestion))
                    .size(FONT_SM)
                    .color(COLOR_MUTED),
            );
        });
        ui.add_space(SPACE_2);
    }

    let urging =
        state.placed_head && state.scan_symmetry == ScanSymmetry::Original && !state.busy();
    let group_rect = Rect::from_min_size(
        ui.next_widget_position(),
        vec2(ui.available_width(), crate::theme::CONTROL_H),
    );
    animated_segmented_group(
        ui,
        "vkit.alignment.scan-symmetry",
        3,
        match state.scan_symmetry {
            ScanSymmetry::Original => 0,
            ScanSymmetry::NegativeX => 1,
            ScanSymmetry::PositiveX => 2,
        },
        |ui, segment_width| {
            let options = [
                (
                    ScanSymmetry::Original,
                    text(state.locale, TextKey::Original),
                ),
                (ScanSymmetry::NegativeX, text(state.locale, TextKey::Left)),
                (ScanSymmetry::PositiveX, text(state.locale, TextKey::Right)),
            ];
            for (value, label) in options {
                if segment_button(ui, segment_width, label, state.scan_symmetry == value).clicked()
                {
                    state.dispatch(Action::RequestScanSymmetry(value));
                }
            }
        },
    );
    if urging {
        guide_glow_over(ui, group_rect, CAPSULE_RADIUS);
    }

    section_end(ui);

    section_heading(ui, text(state.locale, TextKey::ScanReference));
    animated_segmented_group(
        ui,
        "vkit.alignment.eye-state",
        2,
        usize::from(state.eye_state == EyeState::Closed),
        |ui, segment_width| {
            for (value, key) in [
                (EyeState::Open, TextKey::EyesOpen),
                (EyeState::Closed, TextKey::EyesClosed),
            ] {
                if segment_button(
                    ui,
                    segment_width,
                    text(state.locale, key),
                    state.eye_state == value,
                )
                .clicked()
                {
                    state.dispatch(Action::SetEyeState(value));
                }
            }
        },
    );
    section_end(ui);

    section_heading(ui, text(state.locale, TextKey::ScanFidelity));
    let mut fidelity = state.scan_fidelity;

    let changed = ui
        .allocate_ui_with_layout(
            vec2(ui.available_width(), CONTROL_HEIGHT),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.add_space(SLIDER_ROW_INSET);
                let width = (ui.available_width() - SLIDER_ROW_INSET).max(1.0);
                ui.add(
                    crate::ui_components::FilledNumericSlider::new(
                        &mut fidelity,
                        *vkit_core::fit::SCAN_FIDELITY_RANGE.start() as f32
                            ..=*vkit_core::fit::SCAN_FIDELITY_RANGE.end() as f32,
                    )
                    .decimals(2)
                    .min_width(width),
                )
                .on_hover_text(text(state.locale, TextKey::ScanFidelityTooltip))
                .changed()
            },
        )
        .inner;
    if changed {
        state.dispatch(Action::SetScanFidelity(fidelity));
    }
    section_end(ui);
}

fn alignment_view_section(ui: &mut Ui, state: &mut AppState) {
    section_heading(ui, text(state.locale, TextKey::Opacity));
    let mut opacity = state.alignment_opacity;
    if opacity_percent_row(ui, text(state.locale, TextKey::OpenScan), &mut opacity) {
        state.dispatch(Action::SetAlignmentOpacity(opacity));
    }
    let mut g2_opacity = state.alignment_g2_opacity;
    if opacity_percent_row(ui, text(state.locale, TextKey::G2Head), &mut g2_opacity) {
        state.dispatch(Action::SetAlignmentG2Opacity(g2_opacity));
    }
    section_end(ui);
}

fn edit_view_section(ui: &mut Ui, state: &mut AppState) {
    section_heading(ui, text(state.locale, TextKey::View));
    let mut numbers = state.pin_numbers;
    if toggle_option_row(
        ui,
        &mut numbers,
        text(state.locale, TextKey::Numbers),
        text(state.locale, TextKey::NumbersTooltip),
        None,
    ) {
        state.dispatch(Action::TogglePinNumbers(numbers));
    }
    section_end(ui);
}

fn edit_pins_section(ui: &mut Ui, state: &mut AppState) {
    section_heading(ui, text(state.locale, TextKey::PinPlacement));
    let mut mirror = state.x_mirror;
    if toggle_option_row(
        ui,
        &mut mirror,
        text(state.locale, TextKey::XMirror),
        text(state.locale, TextKey::XMirrorTooltip),
        Some(crate::shortcuts::Shortcut::XSymmetry.label()),
    ) {
        state.dispatch(Action::ToggleXMirror(mirror));
    }
    section_end(ui);
}

fn draw_result_inspector(ui: &mut Ui, state: &mut AppState) {
    show_inspector_shell(
        ui,
        state,
        "export.inspector",
        0.0,
        true,
        |ui, state, _viewport| {
            let sections = [
                (SaveSection::Morph, TextKey::MorphSave),
                (SaveSection::Texture, TextKey::TextureSaveSection),
                (SaveSection::Package, TextKey::PackageSection),
            ];
            let selected = sections
                .iter()
                .position(|(section, _)| *section == state.save_section)
                .unwrap_or(0);
            animated_segmented_group(
                ui,
                "vkit.save.section",
                sections.len(),
                selected,
                |ui, segment_width| {
                    for (section, key) in sections {
                        if segment_button(
                            ui,
                            segment_width,
                            text(state.locale, key),
                            state.save_section == section,
                        )
                        .clicked()
                        {
                            state.dispatch(Action::SetSaveSection(section));
                        }
                    }
                },
            );
            ui.add_space(SPACE_3);
            match state.save_section {
                SaveSection::Morph => draw_morph_export_section(ui, state),
                SaveSection::Texture => crate::texture_ui::draw_texture_export_section(ui, state),
                SaveSection::Package => draw_package_section(ui, state),
            }
        },
        |_ui, _state, _footer| {},
    );
}

fn draw_morph_export_section(ui: &mut Ui, state: &mut AppState) {
    draw_vam_export_metadata(ui, state);

    ui.add_space(SPACE_3);
    let width = ui.available_width();

    if capsule_action(ui, width, text(state.locale, TextKey::MorphSave), true).clicked() {
        state.dispatch(Action::ExportResult);
    }

    if let Some(hint) = state.last_morph_save_hint {
        ui.add_space(SPACE_2);
        ui.label(
            RichText::new(text(state.locale, hint))
                .size(FONT_SM)
                .color(COLOR_WARNING),
        );
    }
    section_end(ui);
}

fn hint_with_requirement(locale: Locale, hint: TextKey, required: bool) -> String {
    let mark = if required {
        TextKey::FieldRequired
    } else {
        TextKey::FieldOptional
    };
    format!("{}  {}", text(locale, hint), text(locale, mark))
}

fn without_verbatim_prefix(path: &str) -> &str {
    path.strip_prefix(r"\\?\").unwrap_or(path)
}

const VAM_METADATA_GROUP_ID: &str = "vkit.vam-metadata.group";
const VAM_METADATA_REGION_ID: &str = "vkit.vam-metadata.region";

fn draw_vam_export_metadata(ui: &mut Ui, state: &mut AppState) {
    set_capsule_widget_radius(ui);
    ui.spacing_mut().item_spacing.y = SPACE_2;

    let mut display_name = state.vam_export_display_name.clone();
    if capsule_metadata_field(
        ui,
        "vkit.vam-metadata.name",
        &mut display_name,
        text(state.locale, TextKey::MorphNameHint),
    ) {
        state.dispatch(Action::SetVaMExportDisplayName(display_name));
    }

    let mut group = state.vam_export_group.clone();
    if capsule_metadata_field_with_suggestions(
        ui,
        VAM_METADATA_GROUP_ID,
        &mut group,
        text(state.locale, TextKey::MorphGroupHint),
        &state.vam_morph_groups,
        text(state.locale, TextKey::DirectEntry),
        &[VAM_METADATA_REGION_ID],
    ) {
        state.dispatch(Action::SetVaMExportGroup(group));
    }
    let mut region = state.vam_export_region.clone();
    if capsule_metadata_field_with_suggestions(
        ui,
        VAM_METADATA_REGION_ID,
        &mut region,
        text(state.locale, TextKey::MorphRegionHint),
        &state.vam_morph_regions,
        text(state.locale, TextKey::DirectEntry),
        &[VAM_METADATA_GROUP_ID],
    ) {
        state.dispatch(Action::SetVaMExportRegion(region));
    }

    {
        ui.add_space(SPACE_3);
        section_divider(ui);
        ui.add_space(SPACE_3);
        let mut pose_control = state.vam_export_is_pose_control;
        if toggle_option_row(
            ui,
            &mut pose_control,
            text(state.locale, TextKey::VaMMetadataPoseMorph),
            text(state.locale, TextKey::VaMMetadataPoseMorphTooltip),
            None,
        ) {
            state.dispatch(Action::SetVaMExportIsPoseControl(pose_control));
        }
        let mut bone_correction = state.vam_export_bone_correction;
        if toggle_option_row(
            ui,
            &mut bone_correction,
            text(state.locale, TextKey::VaMMetadataBoneCorrection),
            text(state.locale, TextKey::VaMMetadataBoneCorrectionTooltip),
            None,
        ) {
            state.dispatch(Action::SetVaMExportBoneCorrection(bone_correction));
        }
    }
    section_end(ui);
}

fn section_divider(ui: &mut Ui) {
    let width = ui.available_width().max(0.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
    ui.painter().rect_filled(
        Rect::from_min_max(
            pos2(rect.left(), rect.center().y - 0.5),
            pos2(rect.right(), rect.center().y + 0.5),
        ),
        0.0,
        crate::theme::COLOR_HAIRLINE,
    );
}

pub(crate) fn capsule_metadata_field(
    ui: &mut Ui,
    id_salt: &str,
    value: &mut String,
    placeholder: &str,
) -> bool {
    let width = ui.available_width().max(0.0);
    capsule_metadata_edit(ui, id_salt, value, placeholder, width)
}

fn capsule_metadata_field_with_suggestions(
    ui: &mut Ui,
    id_salt: &str,
    value: &mut String,
    placeholder: &str,
    suggestions: &[String],
    direct_entry_label: &str,
    siblings: &[&str],
) -> bool {
    let row_width = ui.available_width().max(0.0);
    let (mut changed, field_rect) = capsule_metadata_edit_sized(
        ui,
        id_salt,
        value,
        placeholder,
        row_width,
        METADATA_DROPDOWN_CHEVRON_SLOT,
        CapsuleFieldStyle::default(),
    );
    let chevron_rect = Rect::from_min_size(
        pos2(
            field_rect.right() - METADATA_DROPDOWN_CHEVRON_SLOT,
            field_rect.top(),
        ),
        vec2(METADATA_DROPDOWN_CHEVRON_SLOT, field_rect.height()),
    );
    let chevron = ui.interact(
        chevron_rect,
        Id::new(("capsule.metadata.chevron", id_salt)),
        Sense::click(),
    );
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            chevron_rect.center(),
            Vec2::splat(METADATA_DROPDOWN_CHEVRON_GLYPH),
        ),
        Icon::ChevronDown,
        if chevron.hovered() {
            COLOR_TEXT
        } else {
            COLOR_MUTED
        },
    );
    let chevron = chevron.on_hover_text(direct_entry_label);

    let field_id = Id::new(("capsule.metadata.input", id_salt));
    let menu_id = metadata_dropdown_id(id_salt);
    let row_height = CONTROL_H_DENSE;
    let opened = egui::Popup::from_toggle_button_response(&chevron)
        .id(menu_id)
        .anchor(field_rect)
        .align(egui::RectAlign::BOTTOM_START)
        .align_alternatives(&[])
        .gap(METADATA_DROPDOWN_GAP)
        .width(field_rect.width())
        .frame(
            Frame::new()
                .fill(COLOR_FIELD)
                .stroke(Stroke::NONE)
                .corner_radius(CAPSULE_RADIUS)
                .inner_margin(Margin::same(METADATA_DROPDOWN_MARGIN)),
        )
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .show(|ui| {
            let width = ui.available_width().max(0.0);
            ui.set_width(width);
            ScrollArea::vertical()
                .id_salt(("capsule.metadata.menu-scroll", id_salt))
                .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                .max_height(row_height * METADATA_DROPDOWN_VISIBLE_ROWS)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().max(0.0));
                    ui.spacing_mut().item_spacing.y = 1.0;
                    if metadata_dropdown_row(ui, row_height, direct_entry_label, false) {
                        ui.ctx().memory_mut(|memory| memory.request_focus(field_id));
                    }
                    for suggestion in suggestions {
                        if metadata_dropdown_row(ui, row_height, suggestion, suggestion == &*value)
                        {
                            *value = suggestion.clone();
                            changed = true;
                        }
                    }
                });
        })
        .is_some();
    if opened {
        for sibling in siblings {
            egui::Popup::close_id(ui.ctx(), metadata_dropdown_id(sibling));
        }
    }
    changed
}

fn metadata_dropdown_id(id_salt: &str) -> Id {
    Id::new(("capsule.metadata.menu", id_salt))
}

fn metadata_dropdown_row(ui: &mut Ui, height: f32, label: &str, selected: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width().max(0.0), height), Sense::click());
    paint_list_row_highlight(ui, rect, selected, response.hovered());
    ui.painter().with_clip_rect(rect).text(
        pos2(rect.left() + SPACE_3, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(FONT_SM),
        COLOR_TEXT,
    );
    response.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, selected, label));
    response.clicked()
}

fn capsule_metadata_edit(
    ui: &mut Ui,
    id_salt: &str,
    value: &mut String,
    placeholder: &str,
    width: f32,
) -> bool {
    capsule_metadata_edit_sized(
        ui,
        id_salt,
        value,
        placeholder,
        width,
        0.0,
        CapsuleFieldStyle::default(),
    )
    .0
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CapsuleFieldStyle {
    missing: bool,
    attention: Option<crate::ui_components::AttentionFrame>,
}

impl CapsuleFieldStyle {
    fn required(ui: &Ui, missing: bool, target: AttentionTarget) -> Self {
        Self {
            missing,
            attention: missing.then(|| attention_frame_for(ui, target)).flatten(),
        }
    }

    fn fill(self) -> Color32 {
        let base = if self.missing {
            COLOR_FIELD.lerp_to_gamma(COLOR_DESTRUCTIVE, 0.28)
        } else {
            COLOR_FIELD
        };
        self.attention.map_or(base, |frame| {
            crate::ui_components::attention_tint(base, frame)
        })
    }

    fn nudge(self) -> Vec2 {
        vec2(self.attention.map_or(0.0, |frame| frame.shake), 0.0)
    }
}

fn capsule_metadata_edit_styled(
    ui: &mut Ui,
    id_salt: &str,
    value: &mut String,
    placeholder: &str,
    width: f32,
    style: CapsuleFieldStyle,
) -> bool {
    capsule_metadata_edit_sized(ui, id_salt, value, placeholder, width, 0.0, style).0
}

fn capsule_metadata_edit_sized(
    ui: &mut Ui,
    id_salt: &str,
    value: &mut String,
    placeholder: &str,
    width: f32,
    trailing: f32,
    style: CapsuleFieldStyle,
) -> (bool, Rect) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, CONTROL_HEIGHT), Sense::hover());
    ui.painter()
        .rect_filled(rect.translate(style.nudge()), CAPSULE_RADIUS, style.fill());
    let edit_rect = Rect::from_min_max(
        pos2(rect.left() + SEARCH_TEXT_INSET, rect.top()),
        pos2(
            (rect.right() - SEARCH_TEXT_INSET - trailing).max(rect.left()),
            rect.bottom(),
        ),
    );
    let mut edit_ui = ui.new_child(
        UiBuilder::new()
            .id_salt(("capsule.metadata.edit", id_salt))
            .max_rect(edit_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    constrain_morph_child_clip(&mut edit_ui, edit_rect);
    let changed = edit_ui
        .add_sized(
            edit_rect.size(),
            TextEdit::singleline(value)
                .id(Id::new(("capsule.metadata.input", id_salt)))
                .hint_text(placeholder)
                .desired_width(f32::INFINITY)
                .frame(Frame::NONE)
                .vertical_align(Align::Center)
                .margin(Margin::same(0)),
        )
        .changed();
    (changed, rect)
}

pub(crate) fn toggle_option_row(
    ui: &mut Ui,
    on: &mut bool,
    label: &str,
    tooltip: &str,
    shortcut: Option<&str>,
) -> bool {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width().max(0.0), CONTROL_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| crate::ui_components::tooltip(switch_row(ui, on, label), tooltip, shortcut).changed(),
    )
    .inner
}

const TOAST_SLIDE_SECONDS: f32 = 0.24;
const TOAST_HOLD_SECONDS: f32 = 2.0;
const TOAST_FADE_SECONDS: f32 = 0.55;

const TOAST_SLIDE_POINTS: f32 = 22.0;
const TOAST_TOP_MARGIN: f32 = 18.0;

fn draw_export_toast(root: &mut Ui, state: &AppState, viewport: Rect) {
    let Some(toast) = state.toast else {
        return;
    };
    if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
        return;
    }
    let id = Id::new("vkit.toast.raised-at");
    let now = root.ctx().input(|input| input.time);

    let raised_at = match root.ctx().data(|data| data.get_temp::<(u64, f64)>(id)) {
        Some((serial, at)) if serial == toast.serial => at,
        _ => {
            root.ctx()
                .data_mut(|data| data.insert_temp(id, (toast.serial, now)));
            now
        }
    };
    let elapsed = (now - raised_at) as f32;
    let total = TOAST_SLIDE_SECONDS + TOAST_HOLD_SECONDS + TOAST_FADE_SECONDS;
    if elapsed >= total {
        return;
    }

    root.ctx().request_repaint();

    let entering = (elapsed / TOAST_SLIDE_SECONDS).clamp(0.0, 1.0);

    let slide = entering * entering * (3.0 - 2.0 * entering);
    let leaving =
        ((elapsed - TOAST_SLIDE_SECONDS - TOAST_HOLD_SECONDS) / TOAST_FADE_SECONDS).clamp(0.0, 1.0);
    let opacity = slide * (1.0 - leaving);
    if opacity <= 0.001 {
        return;
    }

    let (icon, accent) = match toast.tone {
        StatusTone::Success => (Icon::Check, COLOR_SUCCESS),
        _ => (Icon::Cross, COLOR_DESTRUCTIVE),
    };
    let label = text(state.locale, toast.key);
    let font = egui::FontId::proportional(FONT_BODY);
    let galley = root
        .painter()
        .layout_no_wrap(label.to_owned(), font.clone(), COLOR_TEXT);
    let glyph = FONT_BODY + 3.0;
    let width = galley.size().x + glyph + SPACE_3 * 3.0;
    let height = CONTROL_H_DENSE;
    let resting = viewport.top() + TOAST_TOP_MARGIN;
    let capsule = Rect::from_min_size(
        pos2(
            viewport.center().x - width * 0.5,
            resting - TOAST_SLIDE_POINTS * (1.0 - slide),
        ),
        vec2(width, height),
    );

    let painter = root.painter().with_clip_rect(viewport);
    let fade = |color: Color32| color.gamma_multiply(opacity);
    painter.rect_filled(capsule, height * 0.5, fade(COLOR_SURFACE_RAISED));

    painter.rect_stroke(
        capsule,
        height * 0.5,
        Stroke::new(1.0, fade(accent)),
        egui::StrokeKind::Inside,
    );
    let glyph_rect = Rect::from_center_size(
        pos2(capsule.left() + SPACE_3 + glyph * 0.5, capsule.center().y),
        Vec2::splat(glyph),
    );
    crate::ui_components::paint_icon(&painter, glyph_rect, icon, fade(accent));
    painter.galley(
        pos2(
            glyph_rect.right() + SPACE_3,
            capsule.center().y - galley.size().y * 0.5,
        ),
        galley,
        fade(COLOR_TEXT),
    );
}

pub(crate) fn title_locale_width(ui: &Ui) -> f32 {
    let widest = Locale::ALL
        .iter()
        .map(|locale| {
            ui.painter()
                .layout_no_wrap(
                    locale.selector_label().to_owned(),
                    egui::FontId::proportional(crate::theme::FONT_BODY),
                    COLOR_TEXT,
                )
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    (widest + TITLE_LOCALE_CHROME).clamp(TITLE_LOCALE_WIDTH, TITLE_LOCALE_MAX_WIDTH)
}

fn draw_recovery_prompt(root: &mut Ui, state: &mut AppState) {
    if state.pending_recovery.is_none() {
        return;
    }
    let locale = state.locale;
    let modal = egui::Modal::new(Id::new("vkit.recovery.modal"))
        .frame(
            Frame::new()
                .fill(COLOR_SURFACE_RAISED)
                .stroke(Stroke::NONE)
                .corner_radius(CONTROL_RADIUS)
                .inner_margin(Margin::same(18)),
        )
        .show(root.ctx(), |ui| {
            ui.set_width(
                380.0_f32
                    .min(root.ctx().content_rect().width() - 48.0)
                    .max(0.0),
            );
            ui.label(
                RichText::new(text(locale, TextKey::RecoveryTitle))
                    .size(SECTION_LABEL_FONT_SIZE)
                    .strong()
                    .color(COLOR_TEXT),
            );
            ui.add_space(SPACE_3);
            ui.label(RichText::new(text(locale, TextKey::RecoveryBody)).color(COLOR_MUTED));
            ui.add_space(SPACE_4);
            let spacing = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - spacing).max(0.0) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                let discard = ui.add_sized(
                    [width, CONTROL_HEIGHT],
                    Button::new(text(locale, TextKey::RecoveryDiscard))
                        .corner_radius(ACTION_RADIUS),
                );
                let restore = ui.add_sized(
                    [width, CONTROL_HEIGHT],
                    Button::new(
                        RichText::new(text(locale, TextKey::RecoveryRestore)).color(COLOR_TEXT),
                    )
                    .fill(COLOR_SURFACE_HOVER)
                    .stroke(Stroke::NONE)
                    .corner_radius(ACTION_RADIUS),
                );
                (discard.clicked(), restore.clicked())
            })
            .inner
        });
    let (discard, restore) = modal.inner;
    if restore {
        state.dispatch(Action::RestoreSession);
    } else if discard {
        state.dispatch(Action::DiscardSession);
    }
}

fn draw_overwrite_confirmation(root: &mut Ui, state: &mut AppState) {
    let Some(path) = state.pending_overwrite_path.clone() else {
        return;
    };
    let mut paths = state.overwrite_confirmation_paths();
    if paths.is_empty() {
        paths.push(path);
    }
    let locale = state.locale;
    let modal = egui::Modal::new(Id::new("vkit.overwrite.modal"))
        .frame(
            Frame::new()
                .fill(COLOR_SURFACE_RAISED)
                .stroke(Stroke::NONE)
                .corner_radius(CONTROL_RADIUS)
                .inner_margin(Margin::same(18)),
        )
        .show(root.ctx(), |ui| {
            ui.set_width(
                360.0_f32
                    .min(root.ctx().content_rect().width() - 48.0)
                    .max(0.0),
            );
            ui.label(
                RichText::new(text(locale, TextKey::OverwriteTitle))
                    .size(SECTION_LABEL_FONT_SIZE)
                    .strong()
                    .color(COLOR_TEXT),
            );
            ui.add_space(SPACE_3);
            ui.label(RichText::new(text(locale, TextKey::OverwriteBody)).color(COLOR_MUTED));
            ui.add_space(SPACE_3);
            for path in &paths {
                ui.add_sized(
                    [ui.available_width(), CONTROL_HEIGHT],
                    egui::Label::new(
                        RichText::new(readable_windows_path(path))
                            .size(FONT_SM)
                            .color(COLOR_TEXT),
                    )
                    .truncate(),
                )
                .on_hover_text(readable_windows_path(path));
            }
            ui.add_space(SPACE_4);
            let spacing = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - spacing).max(0.0) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                let cancel = ui.add_sized(
                    [width, CONTROL_HEIGHT],
                    Button::new(text(locale, TextKey::Cancel)).corner_radius(ACTION_RADIUS),
                );
                let overwrite = destructive_button(ui, width, text(locale, TextKey::Overwrite));
                (cancel.clicked(), overwrite.clicked())
            })
            .inner
        });
    let (cancel, confirm) = modal.inner;
    if confirm {
        state.dispatch(Action::ConfirmOverwrite);
    } else if cancel || modal.should_close() {
        state.dispatch(Action::CancelOverwrite);
    }
}

fn draw_texture_overwrite_confirmation(root: &mut Ui, state: &mut AppState) {
    if state.pending_texture_overwrite.is_empty() {
        return;
    }
    let locale = state.locale;
    let doomed: Vec<String> = state
        .pending_texture_overwrite
        .iter()
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect();
    let modal = egui::Modal::new(Id::new("vkit.texture.overwrite"))
        .frame(
            Frame::new()
                .fill(COLOR_SURFACE_RAISED)
                .stroke(Stroke::NONE)
                .corner_radius(CONTROL_RADIUS)
                .inner_margin(Margin::same(18)),
        )
        .show(root.ctx(), |ui| {
            ui.set_width(
                420.0_f32
                    .min(root.ctx().content_rect().width() - 48.0)
                    .max(0.0),
            );
            ui.label(
                RichText::new(text(locale, TextKey::TextureOverwriteTitle))
                    .size(SECTION_LABEL_FONT_SIZE)
                    .strong()
                    .color(COLOR_TEXT),
            );
            ui.add_space(SPACE_2);
            for name in doomed.iter().take(8) {
                ui.label(RichText::new(name).color(COLOR_MUTED).size(FONT_XS));
            }
            ui.add_space(SPACE_4);
            let spacing = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - spacing).max(0.0) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                let cancel = ui.add_sized(
                    [width, CONTROL_HEIGHT],
                    Button::new(text(locale, TextKey::Cancel)).corner_radius(ACTION_RADIUS),
                );
                let confirm = destructive_button(ui, width, text(locale, TextKey::Save));
                (cancel.clicked(), confirm.clicked())
            })
            .inner
        });
    let (cancel, confirm) = modal.inner;
    if confirm {
        state.dispatch(Action::ConfirmTextureOverwrite);
    } else if cancel || modal.should_close() {
        state.dispatch(Action::CancelTextureOverwrite);
    }
}

fn draw_symmetry_change_confirmation(root: &mut Ui, state: &mut AppState) {
    if state.pending_symmetry_change.is_none() {
        return;
    }
    let locale = state.locale;
    let modal = egui::Modal::new(Id::new("vkit.symmetry.confirm"))
        .frame(
            Frame::new()
                .fill(COLOR_SURFACE_RAISED)
                .stroke(Stroke::NONE)
                .corner_radius(CONTROL_RADIUS)
                .inner_margin(Margin::same(18)),
        )
        .show(root.ctx(), |ui| {
            ui.set_width(
                380.0_f32
                    .min(root.ctx().content_rect().width() - 48.0)
                    .max(0.0),
            );
            ui.label(
                RichText::new(text(locale, TextKey::SymmetryChangeTitle))
                    .size(SECTION_LABEL_FONT_SIZE)
                    .strong()
                    .color(COLOR_TEXT),
            );
            ui.add_space(SPACE_4);
            let spacing = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - spacing).max(0.0) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                let cancel = ui.add_sized(
                    [width, CONTROL_HEIGHT],
                    Button::new(text(locale, TextKey::Cancel)).corner_radius(ACTION_RADIUS),
                );
                let confirm = ui.add_sized(
                    [width, CONTROL_HEIGHT],
                    Button::new(
                        RichText::new(text(locale, TextKey::SymmetryChangeConfirm))
                            .color(COLOR_TEXT),
                    )
                    .fill(COLOR_DESTRUCTIVE)
                    .stroke(Stroke::NONE)
                    .corner_radius(ACTION_RADIUS),
                );
                (cancel.clicked(), confirm.clicked())
            })
            .inner
        });
    let (cancel, confirm) = modal.inner;
    if confirm {
        state.dispatch(Action::ConfirmScanSymmetry);
    } else if cancel || modal.should_close() {
        state.dispatch(Action::CancelScanSymmetry);
    }
}

fn draw_morph_inspector(ui: &mut Ui, state: &mut AppState) {
    let footer_height = if state.is_texturing() {
        ALIGNMENT_FOOTER_HEIGHT
    } else {
        MORPH_ACTION_FOOTER_HEIGHT
    };
    show_inspector_shell(
        ui,
        state,
        "morph.inspector",
        footer_height,
        false,
        |ui, state, _viewport| {
            ui.add_enabled_ui(!state.busy(), |ui| {
                ui.add_space(SPACE_3);
                if state.is_sculpting() {
                    let look_find = state.morph_look_find_open;
                    let selection = animated_segmented_group(
                        ui,
                        "vkit.morph.subview",
                        2,
                        usize::from(!look_find),
                        |ui, segment_width| {
                            let look = segment_button(
                                ui,
                                segment_width,
                                text(state.locale, TextKey::LookFind),
                                look_find,
                            );
                            let morph = segment_button(
                                ui,
                                segment_width,
                                text(state.locale, TextKey::MorphEdit),
                                !look_find,
                            );
                            (look.clicked(), morph.clicked())
                        },
                    );
                    if selection.0 {
                        state.set_look_browser_open(true);
                    } else if selection.1 {
                        state.set_look_browser_open(false);
                    }
                    ui.add_space(MORPH_FILTER_LIST_GAP);
                    if state.morph_look_find_open {
                        set_capsule_widget_radius(ui);
                        draw_vam_edit_source_picker(ui, state, None);
                    } else {
                        ui.add_space(SPACE_3);

                        let filters = draw_morph_filters(ui, state);
                        debug_assert!(filters.category_rect.bottom() <= filters.search_rect.top());
                        ui.add_space(MORPH_FILTER_LIST_GAP);
                        debug_assert!(
                            ui.available_rect_before_wrap().top() + f32::EPSILON
                                >= filters.rect.bottom(),
                            "morph list must begin below its filters"
                        );
                        let list_height = ui.available_height().max(0.0);
                        ScrollArea::vertical()
                            .id_salt("vkit.morph.list-scroll")
                            .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                            .auto_shrink([false, false])
                            .max_height(list_height)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width().max(0.0));
                                if filters.scroll_to_top {
                                    ui.scroll_to_rect(ui.min_rect(), Some(Align::Min));
                                }
                                draw_face_morph_list(ui, state);
                            });
                    }
                } else {
                    ScrollArea::vertical()
                        .id_salt("vkit.texture.inspector-scroll")
                        .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                        .auto_shrink([false, true])
                        .max_height(ui.available_height().max(0.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width().max(0.0));
                            crate::texture_ui::draw_texture_inspector(ui, state);
                        });
                }
            });
        },
        |ui, state, footer| {
            if state.is_texturing() {
                let enabled = !state.busy() && state.tab_available(Tab::Result);
                let response = next_step_button(
                    ui,
                    primary_action_rect(footer),
                    text(state.locale, TextKey::Next),
                    enabled,
                );
                if response.clicked() {
                    state.dispatch(Action::RequestTab(Tab::Result));
                }
                return;
            }
            let buttons = morph_footer_buttons(footer);
            let busy = state.busy();
            let undo = ui
                .add_enabled_ui(!busy, |ui| {
                    ui.put(
                        buttons.undo,
                        Button::new(text(state.locale, TextKey::Undo))
                            .min_size(buttons.undo.size())
                            .corner_radius(capsule_radius_for(buttons.undo)),
                    )
                })
                .inner;
            let undo = crate::ui_components::tooltip(
                undo,
                text(state.locale, TextKey::Undo),
                Some(crate::shortcuts::Shortcut::Undo.label()),
            );
            if undo.clicked() {
                state.dispatch(Action::Undo);
            }
            let reset_id = Id::new("vkit.morph.reset-all");
            let offer_undo = morph_reset_undo_is_offered(ui, reset_id);
            let reset_response = ui.interact(
                buttons.reset,
                reset_id,
                if busy { Sense::hover() } else { Sense::click() },
            );
            let reset = paint_reset_capsule_button(
                ui,
                reset_response,
                text(
                    state.locale,
                    if offer_undo {
                        TextKey::UndoMorphReset
                    } else {
                        TextKey::ResetMorphs
                    },
                ),
                !busy,
            );
            if offer_undo {
                paint_shortcut_hint_below(ui, buttons.reset, "Ctrl+Z");
            }
            if reset.clicked() {
                if offer_undo {
                    state.dispatch(Action::Undo);
                    forget_morph_reset_undo(ui, reset_id);
                } else {
                    state.dispatch(Action::ResetMorphs);
                    offer_morph_reset_undo(ui, reset_id);
                }
            }

            if state.morph_look_find_open {
                return;
            }
            let apply =
                next_step_button(ui, buttons.apply, text(state.locale, TextKey::Next), !busy);
            if apply.clicked() {
                state.dispatch(Action::BakeMorph);
            }
        },
    );
}

#[cfg(test)]
impl MorphControlWidths {
    fn total(self) -> f32 {
        self.slider + self.value_reset_gap + self.reset
    }
}

fn morph_control_widths(available: f32) -> MorphControlWidths {
    let available = available.max(0.0);
    let reset = MORPH_RESET_WIDTH.min(available);
    let after_reset = available - reset;
    let value_reset_gap = MORPH_CONTROL_GAP.min(after_reset);
    let slider = after_reset - value_reset_gap;
    MorphControlWidths {
        slider,
        value_reset_gap,
        reset,
    }
}

fn morph_row_columns(content: Rect) -> MorphRowColumns {
    let widths = morph_control_widths(content.width());
    let controls_top = content.top() + MORPH_ROW_LABEL_HEIGHT + MORPH_ROW_LINE_GAP;
    MorphRowColumns {
        primary: Rect::from_min_size(content.min, vec2(widths.slider, content.height())),
        reset: Rect::from_min_size(
            pos2(
                content.left() + widths.slider + widths.value_reset_gap,
                controls_top,
            ),
            vec2(widths.reset, MORPH_ROW_CONTROL_HEIGHT.min(content.height())),
        ),
    }
}

fn draw_face_morph_list(ui: &mut Ui, state: &mut AppState) {
    let visible = visible_morph_rows(state);
    if visible.is_empty() {
        ui.add_space(SECTION_GAP);
        ui.with_layout(Layout::top_down(Align::Center), |ui| {
            ui.label(
                RichText::new(text(state.locale, TextKey::NoMatchingMorphs)).color(COLOR_MUTED),
            );
        });
        return;
    }

    let row_pitch = MORPH_ROW_HEIGHT + MORPH_ROW_GAP;
    let total_height = MORPH_ROW_HEIGHT + row_pitch * visible.len().saturating_sub(1) as f32;
    let (list_rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width().max(0.0), total_height),
        Sense::hover(),
    );

    let clip = ui.clip_rect();
    let first =
        (((clip.top() - list_rect.top()).max(0.0) / row_pitch).floor() as usize).min(visible.len());
    let end = (((clip.bottom() - list_rect.top()).max(0.0) / row_pitch).ceil() as usize)
        .saturating_add(1)
        .min(visible.len());

    for (visible_row, row) in visible.iter().copied().enumerate().take(end).skip(first) {
        let cell = Rect::from_min_size(
            pos2(
                list_rect.left(),
                list_rect.top() + visible_row as f32 * row_pitch,
            ),
            vec2(list_rect.width(), MORPH_ROW_HEIGHT),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .id_salt(("morph.control", visible_row))
                .max_rect(cell),
        );
        constrain_morph_child_clip(&mut child, cell);
        child.set_width(cell.width());
        let (
            id,
            label,
            value,
            minimum,
            maximum,
            numeric_minimum,
            numeric_maximum,
            default,
            availability,
        ) = match row {
            VisibleMorphRow::EyeClosure => (
                "vkit.eye_closure".to_owned(),
                text(state.locale, TextKey::EyeClosure).to_owned(),
                state.eye_closure,
                0.0,
                1.0,
                0.0,
                1.0,
                state.fit_reference_value.unwrap_or(0.0) as f32,
                MorphAvailability::Resolved,
            ),
            VisibleMorphRow::Control(control_index) => {
                let control = &state.morph_library.controls()[control_index];
                let (minimum, maximum) = control.target.slider_bounds(f64::from(control.value));
                let (numeric_minimum, numeric_maximum) = control.target.entry_bounds();

                let value = state
                    .eyelid_control_live_value(&control.id)
                    .unwrap_or(control.value);
                (
                    control.id.clone(),
                    morph_label_for(state.morph_name_locale(), &control.id, &control.label)
                        .into_owned(),
                    value,
                    minimum as f32,
                    maximum as f32,
                    numeric_minimum as f32,
                    numeric_maximum as f32,
                    control.target.default as f32,
                    control.availability,
                )
            }
        };
        let changed = draw_morph_row(
            &mut child,
            state.locale,
            &id,
            &label,
            value,
            MorphRange {
                slider: minimum..=maximum,
                numeric: numeric_minimum..=numeric_maximum,
                default,
            },
            availability,
        );
        match (row, changed) {
            (VisibleMorphRow::EyeClosure, Some(value)) => {
                state.dispatch(Action::SetEyeClosure(value));
            }
            (VisibleMorphRow::Control(_), Some(value)) => {
                if let Some(gaze) = state.gaze_for_eyelid_control_value(&id, value) {
                    state.dispatch(Action::SetManualEyeGaze(gaze));
                } else {
                    state.dispatch(Action::SetFaceMorph { id, value });
                }
            }
            (_, None) => {}
        }
    }
}

pub(crate) fn visible_morph_rows(state: &AppState) -> Vec<VisibleMorphRow> {
    let mut visible = Vec::new();
    let query_is_empty = state.morph_library.query.trim().is_empty();
    let eye_category_matches = matches!(
        state.morph_library.category_filter,
        MorphCategoryFilter::All | MorphCategoryFilter::Category(MorphCategory::Eyes)
    );
    let eye_is_modified =
        (state.eye_closure - state.fit_reference_value.unwrap_or(0.0) as f32).abs() > f32::EPSILON;
    if eye_category_matches
        && (!state.morph_library.modified_only() || eye_is_modified)
        && (query_is_empty
            || localized_morph_query_matches(
                &state.morph_library.query,
                text(state.locale, TextKey::EyeClosure),
                "Close Eyes Eye Closure",
                "vkit.eye_closure",
            ))
    {
        visible.push(VisibleMorphRow::EyeClosure);
    }
    visible.extend(
        state
            .morph_library
            .controls()
            .iter()
            .enumerate()
            .filter_map(|(index, control)| {
                // Category, the modified-only pile and the left/right side all
                // live in the library. This list used to decide them again on
                // its own, which is why the left/right toggle appeared to do
                // nothing: the flag was set and only the library's own filter
                // read it, and nothing draws from that.
                if !state.morph_library.control_passes_non_text_filters(control) {
                    return None;
                }
                if query_is_empty {
                    return Some(VisibleMorphRow::Control(index));
                }
                let localized =
                    morph_label_for(state.morph_name_locale(), &control.id, &control.label);
                let matches = localized_morph_query_matches(
                    &state.morph_library.query,
                    &localized,
                    &control.label,
                    &control.id,
                ) || {
                    state.locale == Locale::Japanese
                        && localized_morph_query_matches(
                            &state.morph_library.query,
                            &morph_label_for(Locale::Korean, &control.id, &control.label),
                            "",
                            "",
                        )
                };
                matches.then_some(VisibleMorphRow::Control(index))
            }),
    );
    visible
}

fn localized_morph_query_matches(query: &str, localized: &str, raw: &str, id: &str) -> bool {
    let query = normalize_ui_search(query);
    query.is_empty()
        || normalize_ui_search(localized).contains(&query)
        || normalize_ui_search(raw).contains(&query)
        || normalize_ui_search(id).contains(&query)
}

fn normalize_ui_search(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Clone, Debug)]
struct MorphRange {
    slider: std::ops::RangeInclusive<f32>,
    numeric: std::ops::RangeInclusive<f32>,

    default: f32,
}

fn draw_morph_row(
    ui: &mut Ui,
    locale: Locale,
    id: &str,
    label: &str,
    mut value: f32,
    range: MorphRange,
    availability: MorphAvailability,
) -> Option<f32> {
    let (minimum, maximum) = (*range.slider.start(), *range.slider.end());
    let (numeric_minimum, numeric_maximum) = (*range.numeric.start(), *range.numeric.end());
    let default = range.default;
    let can_edit = matches!(
        availability,
        MorphAvailability::Resolved | MorphAvailability::Unresolved
    );
    let (row_rect, row_response) = ui.allocate_exact_size(
        vec2(ui.available_width().max(0.0), MORPH_ROW_HEIGHT),
        Sense::hover(),
    );
    let inherited_clip = ui.clip_rect();
    ui.set_clip_rect(inherited_clip.intersect(row_rect));
    let frame_rect = row_rect.shrink2(vec2(0.0, 0.5));
    let is_modified = (value - default).abs() > f32::EPSILON;
    if is_modified {
        ui.painter().rect_filled(
            frame_rect,
            CONTROL_RADIUS,
            if row_response.hovered() {
                crate::theme::hover_fill(COLOR_FIELD)
            } else {
                COLOR_FIELD
            },
        );
    }
    let content_rect =
        frame_rect.shrink2(vec2(MORPH_ROW_HORIZONTAL_INSET, MORPH_ROW_VERTICAL_INSET));
    let columns = morph_row_columns(content_rect);

    let status = morph_availability_status(locale, availability);
    let status_width = status
        .map(|_| MORPH_STATUS_WIDTH.min(columns.primary.width() * 0.34))
        .unwrap_or(0.0);
    let status_gap = if status.is_some() {
        MORPH_CONTROL_GAP.min((columns.primary.width() - status_width).max(0.0))
    } else {
        0.0
    };
    let label_rect = Rect::from_min_size(
        columns.primary.min,
        vec2(
            (columns.primary.width() - status_width - status_gap).max(0.0),
            MORPH_ROW_LABEL_HEIGHT,
        ),
    );
    let mut label_ui = ui.new_child(
        UiBuilder::new()
            .id_salt(("morph.label", id))
            .max_rect(label_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    constrain_morph_child_clip(&mut label_ui, label_rect);
    label_ui
        .add(
            egui::Label::new(RichText::new(label).size(FONT_BODY).color(if can_edit {
                COLOR_TEXT
            } else {
                COLOR_MUTED
            }))
            .truncate(),
        )
        .on_hover_text(label);

    if let Some((status, color)) = status {
        let status_rect = Rect::from_min_size(
            pos2(
                columns.primary.right() - status_width,
                columns.primary.top(),
            ),
            vec2(status_width, MORPH_ROW_LABEL_HEIGHT),
        );
        let mut status_ui = ui.new_child(
            UiBuilder::new()
                .id_salt(("morph.status", id))
                .max_rect(status_rect)
                .layout(Layout::right_to_left(Align::Center)),
        );
        constrain_morph_child_clip(&mut status_ui, status_rect);
        status_ui
            .add(egui::Label::new(RichText::new(status).size(FONT_XS).color(color)).truncate());
    }

    let controls_top = columns.primary.top() + MORPH_ROW_LABEL_HEIGHT + MORPH_ROW_LINE_GAP;
    let slider_rect = Rect::from_min_size(
        pos2(columns.primary.left(), controls_top),
        vec2(columns.primary.width(), MORPH_ROW_CONTROL_HEIGHT),
    );
    let reset_rect = columns.reset;

    let mut slider_ui = ui.new_child(
        UiBuilder::new()
            .id_salt(("morph.slider", id))
            .max_rect(slider_rect)
            .layout(Layout::centered_and_justified(egui::Direction::TopDown)),
    );
    constrain_morph_child_clip(&mut slider_ui, slider_rect);
    if !can_edit {
        slider_ui.disable();
    }
    let slider_response = slider_ui.add(
        FilledNumericSlider::new(&mut value, minimum..=maximum)
            .entry_range(numeric_minimum..=numeric_maximum)
            .decimals(3)
            .min_width(slider_rect.width()),
    );

    let reset_enabled = can_edit && (value - default).abs() > f32::EPSILON;
    let mut reset_ui = ui.new_child(
        UiBuilder::new()
            .id_salt(("morph.reset", id))
            .max_rect(reset_rect)
            .layout(Layout::centered_and_justified(egui::Direction::TopDown)),
    );
    constrain_morph_child_clip(&mut reset_ui, reset_rect);
    if !reset_enabled {
        reset_ui.disable();
    }
    let reset_response = reset_ui
        .add(Button::new("").frame(false).min_size(reset_rect.size()))
        .on_hover_text(text(locale, TextKey::ResetMorphs));
    paint_reset_icon(
        ui,
        reset_response.rect,
        reset_response.hovered(),
        reset_enabled,
    );

    ui.set_clip_rect(inherited_clip);
    if reset_response.clicked() {
        Some(default)
    } else if slider_response.changed() {
        Some(value)
    } else {
        None
    }
}

fn paint_reset_icon(ui: &Ui, rect: Rect, hovered: bool, enabled: bool) {
    let color = if !enabled {
        disabled(COLOR_MUTED)
    } else if hovered {
        COLOR_TEXT
    } else {
        COLOR_MUTED
    };
    let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(MORPH_RESET_ICON_SIZE));
    paint_icon(ui.painter(), icon_rect, Icon::Refresh, color);
}

const fn morph_availability_status(
    locale: Locale,
    availability: MorphAvailability,
) -> Option<(&'static str, Color32)> {
    match availability {
        MorphAvailability::Resolved | MorphAvailability::Unresolved => None,
        MorphAvailability::Loading => Some((text(locale, TextKey::MorphLoading), COLOR_PRIMARY)),
        MorphAvailability::Failed => {
            Some((text(locale, TextKey::MorphLoadFailed), COLOR_DESTRUCTIVE))
        }
    }
}

const fn morph_category_key(category: MorphCategory) -> TextKey {
    match category {
        MorphCategory::Eyes => TextKey::MorphCategoryEyes,
        MorphCategory::Brows => TextKey::MorphCategoryBrows,
        MorphCategory::Nose => TextKey::MorphCategoryNose,
        MorphCategory::Mouth => TextKey::MorphCategoryMouth,
        MorphCategory::Jaw => TextKey::MorphCategoryJaw,
        MorphCategory::Cheeks => TextKey::MorphCategoryCheeks,
        MorphCategory::Cheekbones => TextKey::MorphCategoryCheeks,
        MorphCategory::Ears => TextKey::MorphCategoryEars,
        MorphCategory::Head => TextKey::MorphCategoryHead,
        MorphCategory::Body => TextKey::MorphCategoryBody,
        MorphCategory::Expression => TextKey::MorphCategoryExpression,
    }
}

fn draw_workspace(root: &mut Ui, state: &mut AppState) -> Rect {
    CentralPanel::default()
        .frame(Frame::new().fill(COLOR_BG))
        .show(root, |ui| match state.active_tab {
            Tab::Alignment => draw_alignment_workspace(ui, state),
            Tab::Edit => draw_edit_workspace(ui, state),
            Tab::Result | Tab::Morph | Tab::Texture => draw_single_workspace(ui, state),
        })
        .response
        .rect
}

fn draw_alignment_workspace(ui: &mut Ui, state: &mut AppState) {
    let rect = ui.max_rect();
    crate::viewport::draw_alignment(ui, state, rect);
}

fn draw_edit_workspace(ui: &mut Ui, state: &mut AppState) {
    let rect = ui.max_rect();
    let gap = 1.0;
    let width = (rect.width() - gap) * 0.5;
    let scan_rect = Rect::from_min_size(rect.min, vec2(width, rect.height()));
    let g2_rect = Rect::from_min_max(pos2(scan_rect.right() + gap, rect.top()), rect.max);
    crate::viewport::draw_edit(ui, state, scan_rect, g2_rect);
}

fn draw_single_workspace(ui: &mut Ui, state: &mut AppState) {
    let title = match state.active_tab {
        Tab::Result => text(state.locale, TextKey::ResultPreview),
        Tab::Morph => text(state.locale, TextKey::DetailCorrection),
        Tab::Texture => text(state.locale, TextKey::TextureStage),

        Tab::Alignment | Tab::Edit => text(state.locale, TextKey::ResultPreview),
    };
    let rect = ui.max_rect();

    if state.is_detail_editing() {
        crate::viewport::draw_detail_workspace(ui, state, rect, title);
    } else {
        crate::viewport::draw_result(ui, state, rect, title);
    }
}

fn set_capsule_widget_radius(ui: &mut Ui) {
    let widgets = &mut ui.style_mut().visuals.widgets;
    widgets.noninteractive.corner_radius = CAPSULE_RADIUS.into();
    widgets.inactive.corner_radius = CAPSULE_RADIUS.into();
    widgets.hovered.corner_radius = CAPSULE_RADIUS.into();
    widgets.active.corner_radius = CAPSULE_RADIUS.into();
    widgets.open.corner_radius = CAPSULE_RADIUS.into();
}

const fn footer_primary_ink(enabled: bool, hovered: bool) -> Color32 {
    if !enabled {
        COLOR_MUTED
    } else if hovered {
        COLOR_BG
    } else {
        COLOR_TEXT
    }
}

fn footer_primary_button(ui: &mut Ui, rect: Rect, label: &str, enabled: bool) -> Response {
    let response = ui.interact(
        rect,
        ui.id().with("footer-primary-action"),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = enabled && response.hovered();
    let fill = if !enabled {
        COLOR_FIELD
    } else if hovered {
        COLOR_PRIMARY
    } else {
        COLOR_SURFACE_RAISED
    };
    ui.painter()
        .rect_filled(rect, capsule_radius_for(rect), fill);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(FONT_HEADING),
        footer_primary_ink(enabled, hovered),
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));
    control_affordances(ui, &response, rect, rect.height() * 0.5);
    response
}

fn next_step_button(ui: &mut Ui, rect: Rect, label: &str, enabled: bool) -> Response {
    let response = footer_primary_button(ui, rect, label, enabled);
    paint_chevron(
        ui,
        rect,
        footer_primary_ink(enabled, enabled && response.hovered()),
    );
    response
}

fn primary_action_rect(footer: Rect) -> Rect {
    let height = PRIMARY_ACTION_HEIGHT.min((footer.height() - 16.0).max(0.0));
    Rect::from_min_size(
        pos2(footer.left(), footer.bottom() - 8.0 - height),
        vec2(footer.width(), height),
    )
}

fn capsule_radius_for(rect: Rect) -> u8 {
    (rect.height() * 0.5).clamp(0.0, u8::MAX as f32).round() as u8
}

pub(crate) fn paint_reset_capsule_button(
    ui: &Ui,
    response: Response,
    label: &str,
    enabled: bool,
) -> Response {
    let fill = if enabled && response.hovered() {
        COLOR_DESTRUCTIVE
    } else if enabled {
        COLOR_SURFACE_RAISED
    } else {
        disabled(COLOR_SURFACE_RAISED)
    };
    ui.painter()
        .rect_filled(response.rect, capsule_radius_for(response.rect), fill);
    ui.painter().text(
        response.rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(FONT_SM),
        if enabled {
            COLOR_TEXT
        } else {
            disabled(COLOR_MUTED)
        },
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));
    response
}

fn paint_chevron(ui: &Ui, rect: Rect, color: Color32) {
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            pos2(rect.right() - 18.0, rect.center().y),
            Vec2::splat(18.0),
        ),
        Icon::ChevronRight,
        color,
    );
}

fn opacity_percent_row(ui: &mut Ui, label: &str, value: &mut f32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let spacing = ui.spacing().item_spacing.x.max(0.0);
        let label_width = OPACITY_LABEL_WIDTH.min(ui.available_width());
        let slider_width = (ui.available_width() - label_width - spacing).max(0.0);
        ui.add_sized(
            [label_width, CONTROL_HEIGHT],
            egui::Label::new(RichText::new(label).color(COLOR_MUTED)),
        );
        changed = ui
            .allocate_ui_with_layout(
                vec2(slider_width, CONTROL_HEIGHT),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add(
                        FilledNumericSlider::new(value, 0.0..=1.0)
                            .percent()
                            .decimals(0)
                            .min_width(slider_width),
                    )
                },
            )
            .inner
            .changed();
    });
    changed
}

const OPACITY_LABEL_WIDTH: f32 = 96.0;

const METADATA_DROPDOWN_CHEVRON_SLOT: f32 = 28.0;

const METADATA_DROPDOWN_CHEVRON_GLYPH: f32 = 14.0;

const METADATA_DROPDOWN_GAP: f32 = 4.0;

const METADATA_DROPDOWN_MARGIN: i8 = 4;

const METADATA_DROPDOWN_VISIBLE_ROWS: f32 = 4.0;
pub(crate) fn section_heading(ui: &mut Ui, title: &str) {
    let width = ui.available_width().max(0.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, CONTROL_H_DENSE), Sense::hover());
    let font = FontId::proportional(SECTION_LABEL_FONT_SIZE);
    let galley = ui
        .painter()
        .layout_no_wrap(title.to_owned(), font, COLOR_TEXT);
    let label_rect = Rect::from_center_size(rect.center(), galley.size());
    let gap = 10.0;
    let line_length = 72.0_f32
        .min((label_rect.left() - rect.left() - gap).max(0.0))
        .min((rect.right() - label_rect.right() - gap).max(0.0));
    let stroke = Stroke::new(1.0, crate::theme::COLOR_BORDER);
    if line_length > 0.0 {
        ui.painter().line_segment(
            [
                pos2(label_rect.left() - gap - line_length, rect.center().y),
                pos2(label_rect.left() - gap, rect.center().y),
            ],
            stroke,
        );
        ui.painter().line_segment(
            [
                pos2(label_rect.right() + gap, rect.center().y),
                pos2(label_rect.right() + gap + line_length, rect.center().y),
            ],
            stroke,
        );
    }
    ui.painter().galley(label_rect.min, galley, COLOR_TEXT);
}

fn section_end(ui: &mut Ui) {
    ui.add_space(SECTION_GAP);
}

const fn status_color(tone: StatusTone) -> Color32 {
    match tone {
        StatusTone::Neutral => COLOR_MUTED,
        StatusTone::Info => COLOR_PRIMARY,
        StatusTone::Success => COLOR_SUCCESS,
        StatusTone::Warning => COLOR_WARNING,
        StatusTone::Error => COLOR_DESTRUCTIVE,
    }
}

const fn progress_stage_text(locale: Locale, stage: JobStage) -> &'static str {
    match (locale, stage) {
        (_, JobStage::Import) => text(locale, TextKey::StageOptimizeHead),
        (_, JobStage::Prepare) => text(locale, TextKey::StagePrepareInput),
        (_, JobStage::Align) => text(locale, TextKey::StageAlignScan),
        (_, JobStage::Fit) => text(locale, TextKey::StageFitShape),
        (_, JobStage::Validate) => text(locale, TextKey::StageValidate),
        (_, JobStage::Export) => text(locale, TextKey::StagePrepareResult),
    }
}

#[cfg(test)]
mod tests;

fn draw_package_contents(ui: &mut Ui, state: &mut AppState) {
    let locale = state.locale;
    let row = ui.available_width().max(0.0);

    // Two routes, one choice. They used to be an independent switch beside an
    // always-visible file list, so a package could ask for the current head
    // *and* carry attachments -- and when the head had not been touched the
    // export refused the whole thing with "identical to the loaded G2
    // template". Picking one now means not the other, and the failure has
    // nowhere left to come from.
    let from_head = state.package_from_this_head;
    let choice = crate::ui_components::animated_segmented_group(
        ui,
        "vkit.package.source",
        2,
        usize::from(!from_head),
        |ui, segment_width| {
            let head = segment_button(
                ui,
                segment_width,
                text(locale, TextKey::PackageFromThisHead),
                from_head,
            );
            let files = segment_button(
                ui,
                segment_width,
                text(locale, TextKey::PackageFromFiles),
                !from_head,
            );
            (head.clicked(), files.clicked())
        },
    );
    if choice.0 && !from_head {
        state.dispatch(Action::SetPackageFromThisHead(true));
    } else if choice.1 && from_head {
        state.dispatch(Action::SetPackageFromThisHead(false));
    }

    if !state.package_from_this_head {
        ui.add_space(SPACE_3);
        draw_package_file_lists(ui, state, row);
    }
    ui.add_space(SPACE_3);
    ui.label(
        RichText::new(text(locale, TextKey::PackageSaveTo))
            .size(FONT_XS)
            .color(COLOR_MUTED),
    );
    let destination = state
        .package_output_directory()
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    let mut edited = destination.clone();
    if capsule_metadata_field(ui, "vkit.package.directory", &mut edited, "")
        && edited != destination
    {
        let trimmed = edited.trim().to_owned();
        state.dispatch(Action::SetPackageDirectory(
            (!trimmed.is_empty()).then(|| PathBuf::from(trimmed)),
        ));
    }
}

/// The attachments, as two lists that look like lists.
///
/// A bare run of rows under a button reads as "some rows happened"; a titled
/// card with a count, a rule, and its own empty line reads as a place things
/// were put. Same rows, but you can tell whether the file went in.
fn draw_package_file_lists(ui: &mut Ui, state: &mut AppState, row: f32) {
    let locale = state.locale;
    let mut picked = None;
    let mut removed = None;
    for (slot, title, add) in [
        (
            PackageSlot::Morph,
            TextKey::PackageMorphFiles,
            TextKey::PackageAddMorphs,
        ),
        (
            PackageSlot::Texture,
            TextKey::PackageTextureFiles,
            TextKey::PackageAddTextures,
        ),
    ] {
        let files = match slot {
            PackageSlot::Morph => state.package_morphs.clone(),
            PackageSlot::Texture => state.package_textures.clone(),
        };
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(text(locale, title))
                    .size(FONT_XS)
                    .color(COLOR_MUTED),
            );
            if !files.is_empty() {
                ui.label(
                    RichText::new(format!("{}", files.len()))
                        .size(FONT_XS)
                        .color(COLOR_TEXT),
                );
            }
        });
        ui.add_space(crate::theme::SPACE_1);

        if files.is_empty() {
            ui.label(
                RichText::new(text(locale, TextKey::PackageListEmpty))
                    .size(FONT_XS)
                    .color(crate::theme::disabled(COLOR_MUTED)),
            );
        } else {
            for (index, file) in files.iter().enumerate() {
                if draw_package_row(ui, row, &file.label, slot, index) {
                    removed = Some((slot, index));
                }
            }
        }
        ui.add_space(SPACE_2);
        if capsule_action(ui, row, text(locale, add), true).clicked() {
            picked = Some(slot);
        }
        ui.add_space(SPACE_3);
    }

    if let Some(slot) = picked
        && let Some(paths) = crate::dialogs::pick_package_files(state, slot)
        && !paths.is_empty()
    {
        state.dispatch(Action::AddPackageFiles(slot, paths));
    }
    if let Some((slot, index)) = removed {
        state.dispatch(Action::RemovePackageFile(slot, index));
    }

    if state.package_morphs.is_empty() && state.package_textures.is_empty() {
        ui.label(
            RichText::new(text(locale, TextKey::PackageNothingToPack))
                .size(FONT_XS)
                .color(COLOR_WARNING),
        );
        ui.add_space(SPACE_2);
    }
}

fn draw_package_row(ui: &mut Ui, width: f32, label: &str, slot: PackageSlot, index: usize) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, CONTROL_H_DENSE), Sense::hover());
    paint_list_row_highlight(ui, rect, false, response.hovered());
    let cross = Rect::from_center_size(
        pos2(rect.right() - CONTROL_H_DENSE * 0.5, rect.center().y),
        Vec2::splat(CONTROL_H_DENSE),
    );
    let text_clip = Rect::from_min_max(
        pos2(rect.left() + SPACE_3, rect.top()),
        pos2(cross.left(), rect.bottom()),
    );
    ui.painter().with_clip_rect(text_clip).text(
        pos2(text_clip.left(), rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(FONT_SM),
        COLOR_TEXT,
    );
    if !response.hovered() {
        return false;
    }
    let pressed = ui.interact(
        cross,
        Id::new(("vkit.package.remove", slot as u8, index)),
        Sense::click(),
    );
    let tint = if pressed.hovered() {
        COLOR_DESTRUCTIVE
    } else {
        COLOR_MUTED
    };
    paint_icon(ui.painter(), cross.shrink(SPACE_2), Icon::Cross, tint);
    pressed.clicked()
}

fn draw_package_section(ui: &mut Ui, state: &mut AppState) {
    set_capsule_widget_radius(ui);
    ui.spacing_mut().item_spacing.y = SPACE_2;

    if std::mem::take(&mut state.package_fields_want_attention) {
        for (missing, target) in [
            (
                state.missing_package_fields.creator,
                AttentionTarget::PackageCreator,
            ),
            (
                state.missing_package_fields.package,
                AttentionTarget::PackageName,
            ),
        ] {
            if missing {
                crate::ui_components::attention_flash(ui.ctx(), attention_target_id(target));
            }
        }
    }

    let spacing = ui.spacing().item_spacing.x;
    let row = ui.available_width().max(0.0);

    let marker = text(state.locale, TextKey::PackageVersionMarker);
    let marker_width = ui
        .painter()
        .layout_no_wrap(
            marker.to_owned(),
            egui::FontId::proportional(FONT_SM),
            COLOR_MUTED,
        )
        .size()
        .x;
    let version_width =
        (marker_width + spacing + PACKAGE_VERSION_DIGITS_WIDTH).min((row - spacing).max(0.0));
    ui.horizontal(|ui| {
        ui.add_sized(
            [marker_width, CONTROL_HEIGHT],
            egui::Label::new(RichText::new(marker).size(FONT_SM).color(COLOR_MUTED)),
        );
        let mut version = state.var_version_text.clone();
        if capsule_metadata_edit_styled(
            ui,
            "vkit.package.version",
            &mut version,
            "1",
            (version_width - marker_width - spacing).max(0.0),
            CapsuleFieldStyle::default(),
        ) {
            state.dispatch(Action::SetVarMetadata(VarMetadataField::Version, version));
        }
        let mut creator = state.var_metadata.creator.clone();
        if capsule_metadata_edit_styled(
            ui,
            "vkit.package.creator",
            &mut creator,
            &hint_with_requirement(state.locale, TextKey::PackageCreatorHint, true),
            (row - version_width - spacing).max(0.0),
            CapsuleFieldStyle::required(
                ui,
                state.missing_package_fields.creator,
                AttentionTarget::PackageCreator,
            ),
        ) {
            state.dispatch(Action::SetVarMetadata(VarMetadataField::Creator, creator));
        }
    });

    let mut package = state.var_metadata.package.clone();
    if capsule_metadata_edit_styled(
        ui,
        "vkit.package.name",
        &mut package,
        &hint_with_requirement(state.locale, TextKey::PackageNameHint, true),
        row,
        CapsuleFieldStyle::required(
            ui,
            state.missing_package_fields.package,
            AttentionTarget::PackageName,
        ),
    ) {
        state.dispatch(Action::SetVarMetadata(VarMetadataField::Package, package));
    }

    for (field, hint, current) in [
        (
            VarMetadataField::Description,
            TextKey::PackageDescriptionHint,
            state.var_metadata.description.clone(),
        ),
        (
            VarMetadataField::Credits,
            TextKey::PackageCreditsHint,
            state.var_metadata.credits.clone(),
        ),
        (
            VarMetadataField::Instructions,
            TextKey::PackageInstructionsHint,
            state.var_metadata.instructions.clone(),
        ),
        (
            VarMetadataField::PromotionalLink,
            TextKey::PackageLinkHint,
            state.var_metadata.promotional_link.clone(),
        ),
    ] {
        let mut value = current;
        if capsule_metadata_field(
            ui,
            &format!("vkit.package.{field:?}"),
            &mut value,
            &hint_with_requirement(state.locale, hint, false),
        ) {
            state.dispatch(Action::SetVarMetadata(field, value));
        }
    }

    let licenses = vkit_core::vam::VAR_LICENSES;
    let selected = licenses
        .iter()
        .position(|license| *license == state.var_metadata.license)
        .unwrap_or(0);
    let mut license = licenses[selected];
    ui.allocate_ui_with_layout(
        vec2(row, CONTROL_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            crate::ui_components::fit_combo(
                ui,
                "vkit.package.license",
                row,
                licenses[selected],
                |ui| {
                    for option in licenses {
                        if license_row(ui, option, text(state.locale, license_meaning_key(option)))
                            .clicked()
                        {
                            license = option;
                            ui.close();
                        }
                    }
                },
            );
        },
    );
    if license != licenses[selected] {
        state.dispatch(Action::SetVarMetadata(
            VarMetadataField::License,
            license.to_owned(),
        ));
    }

    ui.add_space(SPACE_3);
    section_divider(ui);
    ui.add_space(SPACE_3);

    draw_package_contents(ui, state);

    ui.add_space(SPACE_2);

    let ready =
        state.vam_root.is_some() && (state.export_ready() || state.texture_project.baked.is_some());
    if capsule_action(ui, row, text(state.locale, TextKey::PackageSave), ready).clicked() {
        state.dispatch(Action::SavePackage);
    }

    for (missing, key) in [
        (
            state.missing_package_fields.creator,
            TextKey::PackageCreatorRequired,
        ),
        (
            state.missing_package_fields.package,
            TextKey::PackageNameRequired,
        ),
    ] {
        if missing {
            ui.label(
                RichText::new(text(state.locale, key))
                    .size(FONT_SM)
                    .color(COLOR_DESTRUCTIVE),
            );
        }
    }

    if let Some(package) = state.last_package_path.clone() {
        ui.add_space(SPACE_3);
        ui.label(
            RichText::new(text(state.locale, TextKey::PackageSavedTo))
                .size(FONT_XS)
                .color(COLOR_MUTED),
        );
        let shown = package.display().to_string();
        let link = ui.add(
            egui::Label::new(
                RichText::new(without_verbatim_prefix(&shown))
                    .size(FONT_XS)
                    .color(COLOR_WARNING)
                    .underline(),
            )
            .sense(Sense::click()),
        );
        if link.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        ui.add_space(SPACE_2);

        ui.label(
            RichText::new(text(state.locale, TextKey::PackageNeedsVamRestart))
                .size(FONT_BODY)
                .strong()
                .color(COLOR_WARNING),
        );
        if link.clicked()
            && let Some(folder) = package.parent()
        {
            let folder = folder.display().to_string();
            crate::settings::open_with_shell(without_verbatim_prefix(&folder));
        }
    }
    section_end(ui);
}

const PACKAGE_VERSION_DIGITS_WIDTH: f32 = 44.0;

fn license_row(ui: &mut Ui, code: &str, meaning: &str) -> Response {
    let width = ui.available_width().max(0.0);
    let (rect, response) = ui.allocate_exact_size(vec2(width, CONTROL_H_DENSE), Sense::click());
    paint_list_row_highlight(ui, rect, false, response.hovered());
    let font = egui::TextStyle::Button.resolve(ui.style());
    ui.painter().text(
        rect.left_center() + vec2(SPACE_2, 0.0),
        Align2::LEFT_CENTER,
        code,
        font.clone(),
        COLOR_TEXT,
    );
    ui.painter().text(
        rect.right_center() - vec2(SPACE_2, 0.0),
        Align2::RIGHT_CENTER,
        meaning,
        egui::FontId::proportional(FONT_XS),
        COLOR_MUTED,
    );
    response
}

const fn license_meaning_key(license: &str) -> TextKey {
    match license.as_bytes() {
        b"CC BY-SA" => TextKey::LicenseBySaTooltip,
        b"CC BY-ND" => TextKey::LicenseByNdTooltip,
        b"CC BY-NC" => TextKey::LicenseByNcTooltip,
        b"CC BY-NC-SA" => TextKey::LicenseByNcSaTooltip,
        b"CC BY-NC-ND" => TextKey::LicenseByNcNdTooltip,
        b"PC EA" => TextKey::LicensePcEaTooltip,
        _ => TextKey::LicenseByTooltip,
    }
}

fn draw_package_replace_confirmation(root: &mut Ui, state: &mut AppState) {
    let Some(path) = state.pending_package_replace.clone() else {
        return;
    };
    let locale = state.locale;
    let modal = egui::Modal::new(Id::new("vkit.package.replace"))
        .frame(
            Frame::new()
                .fill(COLOR_SURFACE_RAISED)
                .stroke(Stroke::NONE)
                .corner_radius(CONTROL_RADIUS)
                .inner_margin(Margin::same(18)),
        )
        .show(root.ctx(), |ui| {
            ui.set_width(
                360.0_f32
                    .min(root.ctx().content_rect().width() - 48.0)
                    .max(0.0),
            );
            ui.label(
                RichText::new(text(locale, TextKey::PackageReplaceTitle))
                    .size(SECTION_LABEL_FONT_SIZE)
                    .strong()
                    .color(COLOR_TEXT),
            );
            ui.add_space(SPACE_3);
            ui.label(RichText::new(text(locale, TextKey::PackageReplaceBody)).color(COLOR_MUTED));
            ui.add_space(SPACE_3);

            ui.add_sized(
                [ui.available_width(), CONTROL_HEIGHT],
                egui::Label::new(
                    RichText::new(
                        path.file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                    )
                    .size(FONT_SM)
                    .color(COLOR_TEXT),
                )
                .truncate(),
            );
            ui.add_space(SPACE_4);
            let spacing = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - spacing).max(0.0) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                let cancel = ui.add_sized(
                    [width, CONTROL_HEIGHT],
                    Button::new(text(locale, TextKey::Cancel)).corner_radius(ACTION_RADIUS),
                );
                let replace = destructive_button(ui, width, text(locale, TextKey::Overwrite));
                (cancel.clicked(), replace.clicked())
            })
            .inner
        });
    let (cancel, confirm) = modal.inner;
    if confirm {
        state.dispatch(Action::ReplacePackage);
    } else if cancel || modal.should_close() {
        state.dispatch(Action::KeepPackage);
    }
}

pub(crate) fn island_capsule_button(
    ui: &mut Ui,
    rect: Rect,
    label: &str,
    destructive: bool,
) -> Response {
    let response = ui.interact(rect, ui.id().with(("island-button", label)), Sense::click());
    let hovered = response.hovered() || response.is_pointer_button_down_on();
    let fill = if hovered {
        if destructive {
            COLOR_DESTRUCTIVE
        } else {
            COLOR_SURFACE_HOVER
        }
    } else {
        COLOR_SURFACE_RAISED
    };
    ui.painter().rect_filled(rect, CAPSULE_RADIUS, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(FONT_SM),
        COLOR_TEXT,
    );
    crate::ui_components::control_affordances(ui, &response, rect, f32::from(CAPSULE_RADIUS));
    response
}

pub(crate) fn island_labelled_slider(
    ui: &mut Ui,
    rect: Rect,
    label: &str,
    value: &mut f32,
) -> bool {
    let label_width = ISLAND_SLIDER_LABEL_WIDTH.min(rect.width() * 0.45);
    ui.painter().text(
        pos2(rect.left(), rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(FONT_SM),
        COLOR_MUTED,
    );
    let track = Rect::from_min_max(pos2(rect.left() + label_width, rect.top()), rect.max);
    ui.put(
        track,
        crate::ui_components::FilledNumericSlider::new(value, 0.0..=1.0)
            .percent()
            .decimals(0)
            .min_width(track.width().max(1.0)),
    )
    .changed()
}

const ISLAND_SLIDER_LABEL_WIDTH: f32 = 84.0;

fn destructive_button(ui: &mut Ui, width: f32, label: &str) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(width, CONTROL_HEIGHT), Sense::click());
    let fill = if response.hovered() || response.is_pointer_button_down_on() {
        COLOR_DESTRUCTIVE
    } else {
        COLOR_SURFACE_HOVER
    };
    paint_centred_capsule(ui, rect, label, fill, COLOR_TEXT, ACTION_RADIUS);
    control_affordances(ui, &response, rect, ACTION_RADIUS as f32);
    response
}

pub(crate) fn capsule_action(ui: &mut Ui, width: f32, label: &str, enabled: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        vec2(width, CONTROL_H_PRIMARY),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = enabled && (response.hovered() || response.is_pointer_button_down_on());
    let fill = if !enabled {
        COLOR_SURFACE
    } else if hovered {
        COLOR_SURFACE_HOVER
    } else {
        COLOR_SURFACE_RAISED
    };
    let ink = if enabled {
        COLOR_TEXT
    } else {
        crate::theme::disabled(COLOR_TEXT)
    };
    paint_centred_capsule(ui, rect, label, fill, ink, CAPSULE_RADIUS);
    if enabled {
        control_affordances(ui, &response, rect, f32::from(CAPSULE_RADIUS));
    }
    response
}

fn paint_centred_capsule(
    ui: &Ui,
    rect: Rect,
    label: &str,
    fill: Color32,
    ink: Color32,
    radius: impl Into<egui::CornerRadius>,
) {
    ui.painter().rect_filled(rect, radius, fill);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        egui::TextStyle::Button.resolve(ui.style()),
        ink,
    );
}
