use std::ops::RangeInclusive;

use egui::{
    self, Align, Align2, Color32, CursorIcon, FontId, Id, Layout, Painter, Pos2, Rect, Response,
    Sense, Shape, Stroke, TextEdit, Ui, Vec2, Widget, WidgetInfo, WidgetType,
};
use vkit_core::sculpt::SculptFalloff;

use crate::i18n::{Locale, TextKey, text};

mod icons;

pub use icons::*;

pub(crate) const TOOLTIP_MAX_WIDTH: f32 = 260.0;

const TOOLTIP_SHORTCUT_GAP: f32 = 10.0;

pub const NO_SHORTCUT: Option<&str> = None;

pub fn tooltip(response: Response, body: &str, shortcut: Option<impl Into<String>>) -> Response {
    let Some(shortcut) = shortcut else {
        return response.on_hover_text(body);
    };
    let body = body.to_owned();
    let shortcut: String = shortcut.into();
    response.on_hover_ui(|ui| {
        ui.set_max_width(TOOLTIP_MAX_WIDTH);
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = TOOLTIP_SHORTCUT_GAP;
            ui.label(&body);
            ui.label(
                egui::RichText::new(format!("({shortcut})"))
                    .size(crate::theme::FONT_XS)
                    .color(crate::theme::COLOR_MUTED),
            );
        });
    })
}

const THUMBNAIL_TEXELS: u32 = 96;

#[derive(Clone)]
struct StampedTexture<S> {
    stamp: S,
    handle: egui::TextureHandle,
}

pub(crate) fn try_stamped_texture<S>(
    ui: &Ui,
    namespace: &'static str,
    key: u64,
    stamp: S,
    options: egui::TextureOptions,
    image: impl FnOnce() -> Option<egui::ColorImage>,
) -> Option<egui::TextureHandle>
where
    S: PartialEq + Clone + Send + Sync + 'static,
{
    let id = Id::new((namespace, key));
    let cached = ui.data(|data| data.get_temp::<StampedTexture<S>>(id));
    if let Some(cache) = &cached
        && cache.stamp == stamp
    {
        return Some(cache.handle.clone());
    }
    let image = image()?;
    if let Some(mut cache) = cached {
        cache.handle.set(image, options);
        cache.stamp = stamp;
        let handle = cache.handle.clone();
        ui.data_mut(|data| data.insert_temp(id, cache));
        return Some(handle);
    }
    let handle = ui
        .ctx()
        .load_texture(format!("{namespace}-{key}"), image, options);
    ui.data_mut(|data| {
        data.insert_temp(
            id,
            StampedTexture {
                stamp,
                handle: handle.clone(),
            },
        );
    });
    Some(handle)
}

pub(crate) fn stamped_texture<S>(
    ui: &Ui,
    namespace: &'static str,
    key: u64,
    stamp: S,
    options: egui::TextureOptions,
    image: impl FnOnce() -> egui::ColorImage,
) -> egui::TextureHandle
where
    S: PartialEq + Clone + Send + Sync + 'static,
{
    try_stamped_texture(ui, namespace, key, stamp, options, || Some(image()))
        .expect("a loader that always builds an image always fills the cache")
}

pub fn thumbnail_texture(
    ui: &Ui,
    namespace: &'static str,
    layer_id: u64,
    image: &crate::skin_preview::SkinImage,
) -> egui::TextureHandle {
    stamped_texture(
        ui,
        namespace,
        layer_id,
        image.revision,
        egui::TextureOptions::LINEAR,
        || thumbnail_color_image(image),
    )
}

fn thumbnail_color_image(image: &crate::skin_preview::SkinImage) -> egui::ColorImage {
    let longest = image.width.max(image.height);
    let view = (longest > THUMBNAIL_TEXELS)
        .then(|| vkit_core::pixels::RgbaView::new(&image.rgba8, image.width, image.height).ok())
        .flatten();
    let Some(view) = view else {
        return egui::ColorImage::from_rgba_unmultiplied(
            [image.width as usize, image.height as usize],
            &image.rgba8,
        );
    };
    let scale = f64::from(THUMBNAIL_TEXELS) / f64::from(longest);
    let width = ((f64::from(image.width) * scale).round() as u32).max(1);
    let height = ((f64::from(image.height) * scale).round() as u32).max(1);

    let resized = vkit_core::pixels::resize_rgba_box_premultiplied(view, width, height);
    egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], &resized)
}

pub fn cover_uv(target: Vec2, image: Vec2) -> Rect {
    let full = Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0));
    if target.x <= 0.0 || target.y <= 0.0 || image.x <= 0.0 || image.y <= 0.0 {
        return full;
    }
    let target_aspect = target.x / target.y;
    let image_aspect = image.x / image.y;
    if image_aspect > target_aspect {
        let span = target_aspect / image_aspect;
        let inset = (1.0 - span) * 0.5;
        Rect::from_min_max(egui::pos2(inset, 0.0), egui::pos2(inset + span, 1.0))
    } else {
        let span = image_aspect / target_aspect;
        let inset = (1.0 - span) * 0.5;
        Rect::from_min_max(egui::pos2(0.0, inset), egui::pos2(1.0, inset + span))
    }
}

pub fn paint_thumbnail_image(
    ui: &Ui,
    rect: Rect,
    namespace: &'static str,
    layer_id: u64,
    image: &crate::skin_preview::SkinImage,
) {
    let texture = thumbnail_texture(ui, namespace, layer_id, image);
    let uv = cover_uv(
        rect.size(),
        Vec2::new(image.width as f32, image.height as f32),
    );
    ui.painter().add(Shape::Rect(
        egui::epaint::RectShape::filled(rect, crate::theme::CONTROL_RADIUS, Color32::WHITE)
            .with_texture(texture.id(), uv),
    ));
}

pub fn paint_list_row_highlight(ui: &Ui, rect: Rect, selected: bool, hovered: bool) {
    let Some(fill) = list_row_fill(selected, hovered) else {
        return;
    };
    ui.painter()
        .rect_filled(rect, crate::theme::CONTROL_RADIUS, fill);
}

fn list_row_fill(selected: bool, hovered: bool) -> Option<Color32> {
    if selected {
        Some(crate::theme::COLOR_SURFACE_RAISED)
    } else if hovered {
        Some(crate::theme::COLOR_SURFACE_HOVER)
    } else {
        None
    }
}

const TRACK_HEIGHT: f32 = 5.0;
const VALUE_GAP: f32 = 4.0;

const VALUE_WIDTH: f32 = 40.0;

const SLIDER_ROW_HEIGHT: f32 = 22.0;

pub const COMPACT_COLOR_SWATCH_WIDTH: f32 = 56.0;
pub const COMPACT_COLOR_SWATCH_HEIGHT: f32 = 24.0;
pub const COMPACT_COLOR_SWATCH_RADIUS: f32 = 16.0;
const COLOR_PICKER_SV_EDGE: f32 = 136.0;
const COLOR_PICKER_HUE_HEIGHT: f32 = 18.0;
const COLOR_PICKER_COLUMN_GAP: f32 = 12.0;
const COLOR_PICKER_FIELD_LABEL_WIDTH: f32 = 24.0;
const COLOR_PICKER_FIELD_WIDTH: f32 = 72.0;
const COLOR_PICKER_FIELD_HEIGHT: f32 = 24.0;
const COLOR_PICKER_GRADIENT_STEPS: usize = 8;

pub const MINI_POPUP_CONTENT_INSET_X: f32 = 12.0;
pub const MINI_POPUP_CONTENT_INSET_Y: f32 = 10.0;
pub const MINI_HELP_CONTENT_INSET_X: f32 = MINI_POPUP_CONTENT_INSET_X;
pub const MINI_HELP_CONTENT_INSET_Y: f32 = MINI_POPUP_CONTENT_INSET_Y;
pub const BRUSH_FALLOFF_FIELD_WIDTH: f32 = 120.0;
pub const BRUSH_FALLOFF_COMPACT_WIDTH: f32 = 48.0;
pub const BRUSH_FALLOFF_FIELD_HEIGHT: f32 = 26.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct BrushSizeGestureUpdate {
    pub consumed: bool,
    pub radius: Option<f32>,
}

pub fn handle_brush_size_gesture(
    ui: &Ui,
    id: Id,
    viewport: Rect,
    current_radius: f32,
    sensitivity: f32,
    radius_range: RangeInclusive<f32>,
) -> BrushSizeGestureUpdate {
    let update = crate::sweep_gesture::handle_sweep(
        ui,
        id,
        crate::shortcuts::Shortcut::BrushSizeSweep,
        viewport,
        current_radius,
        sensitivity,
        Some(radius_range),
    );
    BrushSizeGestureUpdate {
        consumed: update.consumed,
        radius: update.value,
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BrushStrengthGestureUpdate {
    pub consumed: bool,
    pub strength: Option<f32>,
}

pub const BRUSH_STRENGTH_SENSITIVITY: f32 = 0.004;

pub const TEXTURE_BRUSH_SIZE_SENSITIVITY: f32 = 0.0008;

pub fn handle_brush_strength_gesture(
    ui: &Ui,
    id: Id,
    viewport: Rect,
    current_strength: f32,
    sensitivity: f32,
    strength_range: RangeInclusive<f32>,
) -> BrushStrengthGestureUpdate {
    let update = crate::sweep_gesture::handle_sweep(
        ui,
        id,
        crate::shortcuts::Shortcut::BrushStrengthSweep,
        viewport,
        current_strength,
        sensitivity,
        Some(strength_range),
    );
    BrushStrengthGestureUpdate {
        consumed: update.consumed,
        strength: update.value,
    }
}

pub fn island_rect(
    bounds: Rect,
    size: Vec2,
    dragged: Option<[f32; 2]>,
    default_min: Pos2,
    margin: f32,
) -> Rect {
    let desired = dragged.map_or(default_min, |[x, y]| Pos2::new(x, y));
    let max_x = (bounds.right() - size.x - margin).max(bounds.left() + margin);
    let max_y = (bounds.bottom() - size.y - margin).max(bounds.top() + margin);
    Rect::from_min_size(
        Pos2::new(
            desired.x.clamp(bounds.left() + margin, max_x),
            desired.y.clamp(bounds.top() + margin, max_y),
        ),
        size,
    )
}

pub fn island_move_handle(handle: &Response, rect: Rect, pos: &mut Option<[f32; 2]>) {
    if handle.dragged() {
        handle.ctx.set_cursor_icon(CursorIcon::Grabbing);
        let moved = rect.min + handle.drag_delta();
        *pos = Some([moved.x, moved.y]);
    } else if handle.hovered() {
        handle.ctx.set_cursor_icon(CursorIcon::Grab);
    }
}

pub fn ellipsize_to_width(ui: &Ui, text: &str, max_width: f32, font: FontId) -> String {
    if text.is_empty() || max_width <= 0.0 {
        return text.to_owned();
    }
    let width_of = |value: &str| {
        ui.painter()
            .layout_no_wrap(value.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
    };
    if width_of(text) <= max_width {
        return text.to_owned();
    }
    let budget = (max_width - width_of("…")).max(0.0);
    let mut fitted = String::new();
    for ch in text.chars() {
        let mut candidate = fitted.clone();
        candidate.push(ch);
        if width_of(&candidate) > budget {
            break;
        }
        fitted = candidate;
    }
    fitted.push('…');
    fitted
}

pub fn fit_combo<R>(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    width: f32,
    selected_text: &str,
    contents: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let reserved = {
        let spacing = ui.spacing();
        spacing.icon_width + spacing.button_padding.x * 2.0 + 4.0
    };
    let font = egui::TextStyle::Button.resolve(ui.style());
    let label = ellipsize_to_width(ui, selected_text, (width - reserved).max(0.0), font);
    let mut out = None;
    egui::ComboBox::from_id_salt(egui::Id::new(id_salt))
        .width(width)
        .selected_text(label)
        .show_ui(ui, |ui| {
            out = Some(contents(ui));
        });
    out
}

pub fn log_once(context: &egui::Context, id: Id, state: &str, record: impl FnOnce()) {
    let previous = context.data(|data| data.get_temp::<String>(id));
    if previous.as_deref() == Some(state) {
        return;
    }
    context.data_mut(|data| data.insert_temp(id, state.to_owned()));
    record();
}

pub fn brush_size_gesture_anchor(ui: &Ui, id: Id) -> Option<Pos2> {
    ui.data(|data| {
        data.get_temp::<crate::sweep_gesture::Sweep>(id)
            .map(|gesture| gesture.start_pointer)
    })
}

pub fn paint_clone_anchor(painter: &egui::Painter, at: Pos2) {
    const RADIUS: f32 = 5.5;
    const ARM: f32 = 9.0;
    for (width, color) in [
        (2.6, Color32::from_black_alpha(150)),
        (1.3, crate::theme::COLOR_PRIMARY),
    ] {
        painter.circle_stroke(at, RADIUS, Stroke::new(width, color));
        for axis in [Vec2::X, Vec2::Y] {
            painter.line_segment(
                [at + axis * RADIUS * 1.35, at + axis * ARM],
                Stroke::new(width, color),
            );
            painter.line_segment(
                [at - axis * RADIUS * 1.35, at - axis * ARM],
                Stroke::new(width, color),
            );
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrushSweeps(&'static str);

impl BrushSweeps {
    pub const SCULPT: Self = Self("vkit.viewport.sculpt.brush");

    pub const TEXTURE_SURFACE: Self = Self("vkit.viewport.texture.brush");

    pub const TEXTURE_CANVAS: Self = Self("vkit.texture.source.brush");

    pub const HAIR: Self = Self("vkit.viewport.hair.brush");

    pub const ALL: [Self; 4] = [
        Self::SCULPT,
        Self::TEXTURE_SURFACE,
        Self::TEXTURE_CANVAS,
        Self::HAIR,
    ];

    #[must_use]
    pub fn size(self) -> Id {
        Id::new((self.0, "size"))
    }

    #[must_use]
    pub fn strength(self) -> Id {
        Id::new((self.0, "strength"))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BrushCursor {
    pub at: Pos2,

    pub fill: Option<f32>,
}

pub fn brush_cursor(
    ui: &Ui,
    hover: Option<Pos2>,
    size_id: Id,
    strength: Option<(Id, f32)>,
) -> Option<BrushCursor> {
    if let Some((strength_id, value)) = strength
        && let Some(at) = brush_size_gesture_anchor(ui, strength_id)
    {
        return Some(BrushCursor {
            at,
            fill: Some(value.clamp(0.0, 1.0)),
        });
    }
    let at = brush_size_gesture_anchor(ui, size_id).or(hover)?;
    Some(BrushCursor { at, fill: None })
}

pub fn paint_brush_cursor(
    painter: &egui::Painter,
    cursor: BrushCursor,
    radius: f32,
    color: Color32,
) {
    if let Some(fill) = cursor.fill {
        painter.circle_filled(cursor.at, radius, color.gamma_multiply(0.12 + 0.58 * fill));
    }
    painter.circle_stroke(cursor.at, radius, Stroke::new(1.5, color));
}

pub fn clear_brush_size_gesture(context: &egui::Context, id: Id) {
    context.data_mut(|data| data.remove::<crate::sweep_gesture::Sweep>(id));
}

pub fn compact_brush_numeric_control(
    ui: &mut Ui,
    width: f32,
    label: &str,
    value: &mut f32,
    range: RangeInclusive<f32>,
    decimals: usize,
    shortcut: Option<&str>,
) -> Response {
    let width = width.min(ui.available_width().max(0.0)).max(0.0);
    let (rect, row) = ui.allocate_exact_size(
        Vec2::new(width, crate::theme::CONTROL_H_DENSE),
        Sense::hover(),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(("brush-numeric", label))
            .max_rect(rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = crate::theme::SPACE_2;
    let label_width = (width * 0.30).clamp(30.0, 44.0);
    child.add_sized(
        [label_width, crate::theme::CONTROL_H_DENSE],
        egui::Label::new(
            egui::RichText::new(label)
                .size(crate::theme::FONT_XS)
                .color(crate::theme::COLOR_MUTED),
        )
        .truncate(),
    );
    let slider = child.add(
        FilledNumericSlider::new(value, range)
            .percent()
            .decimals(decimals)
            .min_width(child.available_width()),
    );
    tooltip(row | slider, label, shortcut)
}

pub fn brush_falloff_selector(
    ui: &mut Ui,
    popup_id: Id,
    locale: Locale,
    current: SculptFalloff,
    compact: bool,
) -> Option<SculptFalloff> {
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let trigger_width = if compact {
        BRUSH_FALLOFF_COMPACT_WIDTH
    } else {
        BRUSH_FALLOFF_FIELD_WIDTH
    };
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(trigger_width, BRUSH_FALLOFF_FIELD_HEIGHT),
        Sense::click(),
    );
    let response = response.on_hover_text(text(locale, TextKey::FalloffTooltip));
    let fill = if open || response.hovered() {
        crate::theme::hover_fill(crate::theme::COLOR_FIELD)
    } else {
        crate::theme::COLOR_FIELD
    };
    ui.painter()
        .rect_filled(rect, crate::theme::SMALL_RADIUS, fill);
    ui.painter().rect_stroke(
        rect,
        crate::theme::SMALL_RADIUS,
        Stroke::new(
            1.0,
            if open {
                crate::theme::COLOR_HAIRLINE
            } else {
                crate::theme::COLOR_BORDER
            },
        ),
        egui::StrokeKind::Inside,
    );
    control_affordances(ui, &response, rect, f32::from(crate::theme::SMALL_RADIUS));
    let icon_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 7.0, rect.center().y - 8.0),
        Vec2::splat(16.0),
    );
    paint_icon(
        ui.painter(),
        icon_rect,
        brush_falloff_icon(current),
        crate::theme::COLOR_TEXT,
    );
    if !compact {
        ui.painter().text(
            Pos2::new(icon_rect.right() + 7.0, rect.center().y),
            Align2::LEFT_CENTER,
            text(locale, brush_falloff_text_key(current)),
            FontId::proportional(crate::theme::FONT_SM),
            crate::theme::COLOR_TEXT,
        );
    }
    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            Pos2::new(rect.right() - 10.0, rect.center().y),
            Vec2::splat(12.0),
        ),
        Icon::ChevronDown,
        crate::theme::COLOR_MUTED,
    );

    let mut selected = None;
    egui::Popup::menu(&response)
        .id(popup_id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(BRUSH_FALLOFF_FIELD_WIDTH);
            ui.spacing_mut().item_spacing.y = 3.0;
            for preset in [
                SculptFalloff::Smooth,
                SculptFalloff::Smoother,
                SculptFalloff::Sharp,
                SculptFalloff::Linear,
            ] {
                let row = brush_falloff_menu_row(
                    ui,
                    brush_falloff_icon(preset),
                    text(locale, brush_falloff_text_key(preset)),
                    preset == current,
                );
                if row.clicked() {
                    selected = Some(preset);
                    ui.close();
                }
            }
        });
    selected
}

pub(crate) fn brush_falloff_icon(falloff: SculptFalloff) -> Icon {
    match falloff {
        SculptFalloff::Smooth => Icon::FalloffSmooth,
        SculptFalloff::Smoother => Icon::FalloffSmoother,
        SculptFalloff::Sharp => Icon::FalloffSharp,
        SculptFalloff::Linear => Icon::FalloffLinear,
    }
}

fn brush_falloff_text_key(falloff: SculptFalloff) -> TextKey {
    match falloff {
        SculptFalloff::Smooth => TextKey::FalloffSmooth,
        SculptFalloff::Smoother => TextKey::FalloffSmoother,
        SculptFalloff::Sharp => TextKey::FalloffSharp,
        SculptFalloff::Linear => TextKey::FalloffLinear,
    }
}

/// One icon that stands for the chosen entry, and a menu behind it.
///
/// The falloff picker's row painter is reused, so a menu opened from a HUD
/// island looks like every other menu in the program.
pub fn hud_icon_menu<T: Copy + PartialEq>(
    ui: &mut Ui,
    id: Id,
    size: f32,
    current: T,
    entries: &[(T, Icon, String)],
    heading: &str,
) -> Option<T> {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let heading = heading.to_owned();
    let chosen_label = entries
        .iter()
        .find(|(value, _, _)| *value == current)
        .map(|(_, _, label)| label.clone())
        .unwrap_or_default();
    let response = response.on_hover_ui(|ui| {
        ui.set_max_width(TOOLTIP_MAX_WIDTH);
        ui.label(egui::RichText::new(&heading).strong());
        ui.label(&chosen_label);
    });
    let radius = rect.height() * 0.5;
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, radius, crate::theme::COLOR_SURFACE_RAISED);
    }
    let icon = entries
        .iter()
        .find(|(value, _, _)| *value == current)
        .map_or(Icon::ChevronDown, |(_, icon, _)| *icon);
    paint_icon(
        ui.painter(),
        rect.shrink(3.5),
        icon,
        if response.hovered() {
            crate::theme::COLOR_TEXT
        } else {
            crate::theme::COLOR_MUTED
        },
    );
    control_affordances(ui, &response, rect, radius);

    let mut selected = None;
    egui::Popup::menu(&response)
        .id(id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(BRUSH_FALLOFF_FIELD_WIDTH);
            ui.spacing_mut().item_spacing.y = 3.0;
            for (value, icon, label) in entries {
                if brush_falloff_menu_row(ui, *icon, label, *value == current).clicked() {
                    selected = Some(*value);
                    ui.close();
                }
            }
        });
    selected
}

fn brush_falloff_menu_row(ui: &mut Ui, icon: Icon, label: &str, selected: bool) -> Response {
    let (row, response) = ui.allocate_exact_size(
        Vec2::new(BRUSH_FALLOFF_FIELD_WIDTH, BRUSH_FALLOFF_FIELD_HEIGHT),
        Sense::click(),
    );
    paint_list_row_highlight(ui, row, selected, response.hovered());
    let icon_rect = Rect::from_min_size(
        Pos2::new(row.left() + 7.0, row.center().y - 8.0),
        Vec2::splat(16.0),
    );
    paint_icon(ui.painter(), icon_rect, icon, crate::theme::COLOR_TEXT);
    ui.painter().text(
        Pos2::new(icon_rect.right() + 7.0, row.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(crate::theme::FONT_SM),
        crate::theme::COLOR_TEXT,
    );
    control_affordances(ui, &response, row, f32::from(crate::theme::SMALL_RADIUS));
    response
}

pub fn horizontal_resize_handle(ui: &mut Ui, id: Id, rect: Rect) -> Response {
    let response = ui.interact(rect, id, Sense::click_and_drag());
    if response.hovered() || response.dragged() {
        ui.output_mut(|output| output.cursor_icon = CursorIcon::ResizeHorizontal);
    }
    response
}

pub fn vertical_resize_handle(ui: &mut Ui, id: Id, rect: Rect) -> Response {
    let response = ui.interact(rect, id, Sense::click_and_drag());
    if response.hovered() || response.dragged() {
        ui.output_mut(|output| output.cursor_icon = CursorIcon::ResizeVertical);
    }
    response
}

pub fn paint_texture_pin(painter: &Painter, tip: Pos2, opacity: f32, label: &str, invalid: bool) {
    let opacity = opacity.clamp(0.0, 1.0);
    let points = [
        [0.0, 0.0],
        [-4.7, -6.0],
        [-7.0, -10.3],
        [-6.2, -14.7],
        [-3.1, -18.0],
        [0.0, -19.0],
        [3.1, -18.0],
        [6.2, -14.7],
        [7.0, -10.3],
        [4.7, -6.0],
    ]
    .map(|offset| tip + Vec2::new(offset[0], offset[1]))
    .to_vec();
    let fill = if invalid {
        crate::theme::COLOR_DESTRUCTIVE.gamma_multiply(opacity)
    } else {
        crate::theme::COLOR_TEXTURE_PIN.gamma_multiply(opacity)
    };
    let outline = Color32::from_black_alpha((220.0 * opacity).round().clamp(0.0, 255.0) as u8);
    painter.add(Shape::convex_polygon(
        points,
        fill,
        Stroke::new(1.15, outline),
    ));
    let centre = tip - Vec2::new(0.0, 12.0);
    painter.text(
        centre,
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(crate::theme::FONT_XS),
        Color32::WHITE.gamma_multiply(opacity),
    );
}

pub fn show_horizontal_split<R>(
    ui: &mut Ui,
    id: Id,
    rect: Rect,
    ratio: &mut f32,
    minimum_leading: f32,
    minimum_trailing: f32,
    add_contents: impl FnOnce(&mut Ui, Rect, Rect) -> R,
) -> (R, Response) {
    let width = rect.width().max(1.0);
    let minimum_leading = minimum_leading.max(0.0).min(width * 0.5);
    let minimum_trailing = minimum_trailing.max(0.0).min(width * 0.5);
    let minimum_ratio = minimum_leading / width;
    let maximum_ratio = 1.0 - minimum_trailing / width;
    *ratio = ratio.clamp(minimum_ratio, maximum_ratio);
    let split_x = rect.left() + width * *ratio;
    let leading = Rect::from_min_max(rect.min, Pos2::new(split_x, rect.bottom()));
    let trailing = Rect::from_min_max(Pos2::new(split_x, rect.top()), rect.max);
    let result = add_contents(ui, leading, trailing);
    let grab = crate::theme::INSPECTOR_RESIZE_GRAB_RADIUS;
    let handle_rect = Rect::from_min_max(
        Pos2::new(split_x - grab, rect.top()),
        Pos2::new(split_x + grab, rect.bottom()),
    );
    let handle = horizontal_resize_handle(ui, id, handle_rect);
    if handle.dragged()
        && let Some(pointer) = handle.interact_pointer_pos()
    {
        *ratio = ((pointer.x - rect.left()) / width).clamp(minimum_ratio, maximum_ratio);
    }
    if handle.double_clicked() {
        *ratio = 0.5_f32.clamp(minimum_ratio, maximum_ratio);
    }
    (result, handle)
}

pub struct CapsuleFieldButton<'a> {
    text: &'a str,
    populated: bool,
    dark: bool,
    trailing_icon: Option<Icon>,
}

impl<'a> CapsuleFieldButton<'a> {
    pub const fn new(text: &'a str, populated: bool) -> Self {
        Self {
            text,
            populated,
            dark: false,
            trailing_icon: None,
        }
    }

    pub const fn dark(mut self) -> Self {
        self.dark = true;
        self
    }

    pub const fn with_trailing_icon(mut self, icon: Icon) -> Self {
        self.trailing_icon = Some(icon);
        self
    }
}

impl Widget for CapsuleFieldButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let desired = Vec2::new(ui.available_width().max(0.0), crate::theme::CONTROL_HEIGHT);
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click());
        let base_fill = if self.dark {
            crate::theme::COLOR_TITLE_FIELD
        } else {
            crate::theme::COLOR_FIELD
        };

        let fill = if response.is_pointer_button_down_on() {
            crate::theme::COLOR_SURFACE_RAISED
        } else if response.hovered() {
            crate::theme::COLOR_SURFACE_HOVER
        } else {
            base_fill
        };
        ui.painter()
            .rect_filled(rect, crate::theme::CAPSULE_RADIUS, fill);
        if self.dark {
            ui.painter().rect_stroke(
                rect,
                crate::theme::CAPSULE_RADIUS,
                Stroke::new(1.0, crate::theme::COLOR_SURFACE_RAISED),
                egui::StrokeKind::Inside,
            );
        }
        let ink = if self.populated {
            crate::theme::COLOR_TEXT
        } else {
            crate::theme::COLOR_MUTED
        };
        if let Some(icon) = self.trailing_icon {
            let icon_rect = Rect::from_center_size(
                Pos2::new(rect.right() - rect.height() * 0.5, rect.center().y),
                Vec2::splat(rect.height()),
            );
            let text_clip = Rect::from_min_max(
                Pos2::new(rect.left() + 10.0, rect.top()),
                Pos2::new(icon_rect.left(), rect.bottom()),
            );
            ui.painter().with_clip_rect(text_clip).text(
                Pos2::new(rect.left() + 10.0, rect.center().y),
                Align2::LEFT_CENTER,
                self.text,
                FontId::proportional(crate::theme::FONT_SM),
                ink,
            );
            paint_icon(
                ui.painter(),
                icon_rect.shrink(7.0),
                icon,
                if response.hovered() {
                    crate::theme::COLOR_TEXT
                } else {
                    crate::theme::COLOR_MUTED
                },
            );
        } else {
            ui.painter().with_clip_rect(rect.shrink(10.0)).text(
                rect.center(),
                Align2::CENTER_CENTER,
                self.text,
                FontId::proportional(crate::theme::FONT_SM),
                ink,
            );
        }
        response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, self.text));
        control_affordances(ui, &response, rect, rect.height() * 0.5);
        response
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericFormat {
    Decimal { decimals: usize },
    Percent { decimals: usize },
    Degrees { decimals: usize },
}

impl Default for NumericFormat {
    fn default() -> Self {
        Self::Decimal { decimals: 2 }
    }
}

pub struct FilledNumericSlider<'a> {
    value: &'a mut f32,
    range: RangeInclusive<f32>,
    entry_range: Option<RangeInclusive<f32>>,
    format: NumericFormat,
    min_width: f32,
    value_width: f32,
    value_gap: f32,
    value_align: egui::Align,
}

impl<'a> FilledNumericSlider<'a> {
    pub fn new(value: &'a mut f32, range: RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            entry_range: None,
            format: NumericFormat::default(),
            min_width: 132.0,
            value_width: VALUE_WIDTH,
            value_gap: VALUE_GAP,
            value_align: egui::Align::LEFT,
        }
    }

    pub fn decimals(mut self, decimals: usize) -> Self {
        self.format = match self.format {
            NumericFormat::Decimal { .. } => NumericFormat::Decimal { decimals },
            NumericFormat::Percent { .. } => NumericFormat::Percent { decimals },
            NumericFormat::Degrees { .. } => NumericFormat::Degrees { decimals },
        };
        self
    }

    pub fn percent(mut self) -> Self {
        let decimals = match self.format {
            NumericFormat::Decimal { decimals }
            | NumericFormat::Percent { decimals }
            | NumericFormat::Degrees { decimals } => decimals,
        };
        self.format = NumericFormat::Percent { decimals };
        self
    }

    pub fn degrees(mut self) -> Self {
        let decimals = match self.format {
            NumericFormat::Decimal { decimals }
            | NumericFormat::Percent { decimals }
            | NumericFormat::Degrees { decimals } => decimals,
        };
        self.format = NumericFormat::Degrees { decimals };
        self
    }

    pub fn entry_range(mut self, range: RangeInclusive<f32>) -> Self {
        self.entry_range = Some(range);
        self
    }

    pub fn min_width(mut self, px: f32) -> Self {
        self.min_width = px.max(self.value_width + self.value_gap + 24.0);
        self
    }

    pub fn value_gap(mut self, px: f32) -> Self {
        self.value_gap = px.max(0.0);
        self
    }

    pub fn hide_value(mut self) -> Self {
        self.value_width = 0.0;
        self.value_gap = 0.0;
        self
    }

    pub fn value_lane(mut self, width: f32) -> Self {
        self.value_width = width;
        self
    }

    pub fn right_align_value(mut self) -> Self {
        self.value_align = egui::Align::RIGHT;
        self
    }
}

impl Widget for FilledNumericSlider<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let Self {
            value,
            range,
            entry_range,
            format,
            min_width,
            value_width,
            value_gap,
            value_align,
        } = self;
        let available = ui.available_width();

        let width = if available.is_finite() {
            available.max(0.0)
        } else {
            min_width
        };
        let (rect, mut response) =
            ui.allocate_exact_size(Vec2::new(width, SLIDER_ROW_HEIGHT), Sense::hover());
        let control_id = response.id;
        let effective_value_width = value_width.min((width - value_gap - 24.0).max(0.0));

        let value_rect = Rect::from_center_size(
            Pos2::new(rect.right() - effective_value_width * 0.5, rect.center().y),
            Vec2::new(effective_value_width, SLIDER_ROW_HEIGHT),
        );
        let track_rect = Rect::from_min_max(
            rect.min,
            Pos2::new(
                (value_rect.left() - value_gap).max(rect.left() + 24.0),
                rect.bottom(),
            ),
        );
        let track_response = ui.interact(
            track_rect,
            control_id.with("filled-slider-track"),
            Sense::click_and_drag(),
        );
        response |= track_response.clone();

        let min = *range.start();
        let max = *range.end();
        let entry_min = entry_range.as_ref().map_or(min, |range| *range.start());
        let entry_max = entry_range.as_ref().map_or(max, |range| *range.end());
        let span = (max - min).max(f32::EPSILON);
        let mut normalized = ((*value - min) / span).clamp(0.0, 1.0);
        if let Some(pointer) = track_response.interact_pointer_pos()
            && (track_response.dragged() || track_response.clicked())
        {
            normalized =
                ((pointer.x - track_rect.left()) / track_rect.width().max(1.0)).clamp(0.0, 1.0);
            let settled = (min + normalized * span).clamp(min, max);
            if (*value - settled).abs() > f32::EPSILON {
                *value = settled;
                response.mark_changed();
            }
            normalized = ((*value - min) / span).clamp(0.0, 1.0);
        }
        if track_response.hovered() {
            ui.output_mut(|output| output.cursor_icon = CursorIcon::ResizeHorizontal);
        }

        let center_y = track_rect.center().y;
        let rail = Rect::from_center_size(
            Pos2::new(track_rect.center().x, center_y),
            Vec2::new(track_rect.width(), TRACK_HEIGHT),
        );
        let fill = Rect::from_min_max(
            rail.min,
            Pos2::new(rail.left() + rail.width() * normalized, rail.bottom()),
        );
        let painter = ui.painter();
        painter.rect_filled(rail, TRACK_HEIGHT * 0.5, crate::theme::COLOR_TRACK);
        painter.rect_filled(fill, TRACK_HEIGHT * 0.5, crate::theme::COLOR_TRACK_FILL);

        if effective_value_width <= 0.0 {
            response.widget_info(|| {
                WidgetInfo::selected(WidgetType::Slider, true, *value != min, "numeric slider")
            });
            return response;
        }

        let text_id: Id = control_id.with("filled-slider-value");
        let current_display = match format {
            NumericFormat::Decimal { decimals } => format!("{:.*}", decimals, *value),
            NumericFormat::Percent { decimals } => format!("{:.*}%", decimals, *value * 100.0),
            NumericFormat::Degrees { decimals } => format!("{:.*}°", decimals, *value),
        };
        let was_focused = ui.memory(|memory| memory.has_focus(text_id));
        let mut text = if was_focused {
            ui.data(|data| {
                data.get_temp::<String>(text_id)
                    .unwrap_or_else(|| current_display.clone())
            })
        } else {
            current_display.clone()
        };
        let edit = egui::TextEdit::singleline(&mut text)
            .id(text_id)
            .frame(egui::Frame::NONE)
            .margin(Vec2::ZERO)
            .font(egui::FontId::proportional(crate::theme::FONT_BODY))
            .desired_width(effective_value_width)
            .horizontal_align(value_align)
            .vertical_align(egui::Align::Center);
        let edit_response = ui.put(value_rect, edit);
        if edit_response.changed() {
            let parsed = text
                .trim()
                .trim_end_matches(['%', '°'])
                .trim()
                .parse::<f32>()
                .ok();
            if let Some(parsed) = parsed.filter(|parsed| parsed.is_finite()) {
                let raw = match format {
                    NumericFormat::Decimal { .. } | NumericFormat::Degrees { .. } => parsed,
                    NumericFormat::Percent { .. } => parsed / 100.0,
                };
                let constrained = raw.clamp(entry_min, entry_max);
                if (*value - constrained).abs() > f32::EPSILON {
                    *value = constrained;
                    response.mark_changed();
                }
            }
        }
        if !edit_response.has_focus() {
            text = match format {
                NumericFormat::Decimal { decimals } => format!("{:.*}", decimals, *value),
                NumericFormat::Percent { decimals } => format!("{:.*}%", decimals, *value * 100.0),
                NumericFormat::Degrees { decimals } => format!("{:.*}°", decimals, *value),
            };
        }
        ui.data_mut(|data| data.insert_temp(text_id, text));

        response.widget_info(|| {
            WidgetInfo::selected(WidgetType::Slider, true, *value != min, "numeric slider")
        });
        response
    }
}

pub const MOTION_DURATION_SECS: f64 = 0.22;

pub(crate) fn ease_in_out_cubic(progress: f32) -> f32 {
    if progress < 0.5 {
        4.0 * progress * progress * progress
    } else {
        1.0 - (-2.0 * progress + 2.0).powi(3) * 0.5
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RectMotion {
    key: u64,
    from: Rect,
    to: Rect,
    started_at: f64,
}

impl RectMotion {
    fn sample(self, now: f64) -> (Rect, bool) {
        let progress = ((now - self.started_at) / MOTION_DURATION_SECS).clamp(0.0, 1.0) as f32;
        let eased = ease_in_out_cubic(progress);
        (
            Rect::from_min_max(
                self.from.min + (self.to.min - self.from.min) * eased,
                self.from.max + (self.to.max - self.from.max) * eased,
            ),
            progress >= 1.0,
        )
    }
}

pub fn animate_rect(ui: &Ui, id: Id, selection_key: u64, target: Rect) -> Rect {
    let now = ui.input(|input| input.time);
    let mut motion = ui
        .data(|data| data.get_temp::<RectMotion>(id))
        .unwrap_or(RectMotion {
            key: selection_key,
            from: target,
            to: target,
            started_at: now - MOTION_DURATION_SECS,
        });
    if motion.key != selection_key {
        let (current, _) = motion.sample(now);
        motion = RectMotion {
            key: selection_key,
            from: current,
            to: target,
            started_at: now,
        };
    } else if motion.to != target {
        motion = RectMotion {
            key: selection_key,
            from: target,
            to: target,
            started_at: now - MOTION_DURATION_SECS,
        };
    }
    let (rect, finished) = motion.sample(now);
    ui.data_mut(|data| data.insert_temp(id, motion));
    if !finished {
        ui.ctx().request_repaint();
    }
    rect
}

pub const ATTENTION_FLASH_SECS: f32 = 1.2;

const ATTENTION_SHAKE_SECS: f32 = 0.4;
const ATTENTION_SHAKE_AMPLITUDE: f32 = 3.5;
const ATTENTION_SHAKE_CYCLES: f32 = 3.0;
const ATTENTION_SHAKE_DECAY: f32 = 6.0;

const ATTENTION_BREATHS: f32 = 2.0;

const ATTENTION_PEAK_TINT: f32 = 0.45;

fn attention_pulse(t: f32) -> f32 {
    if !(0.0..ATTENTION_FLASH_SECS).contains(&t) {
        return 0.0;
    }
    (std::f32::consts::PI * ATTENTION_BREATHS * (t / ATTENTION_FLASH_SECS))
        .sin()
        .powi(2)
}

fn attention_shake_offset(t: f32) -> f32 {
    if !(0.0..ATTENTION_SHAKE_SECS).contains(&t) {
        return 0.0;
    }
    ATTENTION_SHAKE_AMPLITUDE
        * (-t * ATTENTION_SHAKE_DECAY).exp()
        * (std::f32::consts::TAU * ATTENTION_SHAKE_CYCLES * (t / ATTENTION_SHAKE_SECS)).sin()
}

#[derive(Clone, Copy)]
struct AttentionStart(f64);

fn attention_slot(id: Id) -> Id {
    id.with("attention-flash.start")
}

pub fn attention_flash(ctx: &egui::Context, id: Id) {
    let now = ctx.input(|input| input.time);
    ctx.data_mut(|data| data.insert_temp(attention_slot(id), AttentionStart(now)));
    ctx.request_repaint();
}

fn attention_fill(base: Color32, pulse: f32) -> Color32 {
    base.lerp_to_gamma(
        crate::theme::COLOR_DESTRUCTIVE,
        (pulse * ATTENTION_PEAK_TINT).clamp(0.0, 1.0),
    )
}

#[derive(Clone, Copy, Debug)]
pub struct AttentionFrame {
    pub pulse: f32,

    pub shake: f32,
}

pub fn attention_pulse_now(ui: &Ui, id: Id) -> Option<AttentionFrame> {
    let AttentionStart(started) =
        ui.data(|data| data.get_temp::<AttentionStart>(attention_slot(id)))?;
    let now = ui.input(|input| input.time);
    let elapsed = ((now - started) as f32).max(0.0);
    if elapsed >= ATTENTION_FLASH_SECS {
        ui.data_mut(|data| data.remove::<AttentionStart>(attention_slot(id)));
        return None;
    }
    ui.ctx().request_repaint();
    Some(AttentionFrame {
        pulse: attention_pulse(elapsed),
        shake: attention_shake_offset(elapsed),
    })
}

pub fn attention_tint(base: Color32, frame: AttentionFrame) -> Color32 {
    attention_fill(base, frame.pulse)
}

pub fn attention_progress(ui: &Ui, id: Id) -> Option<f32> {
    let AttentionStart(started) =
        ui.data(|data| data.get_temp::<AttentionStart>(attention_slot(id)))?;
    let elapsed = ((ui.input(|input| input.time) - started) as f32).max(0.0);
    (elapsed < ATTENTION_FLASH_SECS).then_some(elapsed / ATTENTION_FLASH_SECS)
}

pub fn attention_widget(ui: &mut Ui, id: Id, size: Vec2, widget: impl Widget) -> Response {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let Some(AttentionStart(started)) =
        ui.data(|data| data.get_temp::<AttentionStart>(attention_slot(id)))
    else {
        return ui.put(rect, widget);
    };
    let now = ui.input(|input| input.time);
    let t = ((now - started) as f32).max(0.0);
    if t >= ATTENTION_FLASH_SECS {
        ui.data_mut(|data| data.remove::<AttentionStart>(attention_slot(id)));
        return ui.put(rect, widget);
    }

    let pulse = attention_pulse(t);
    let layer = egui::LayerId::new(egui::Order::Middle, id.with("attention-layer"));
    let mut flashing = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
    let widgets = &mut flashing.visuals_mut().widgets;
    for visuals in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
    ] {
        visuals.bg_fill = attention_fill(visuals.bg_fill, pulse);
        visuals.weak_bg_fill = attention_fill(visuals.weak_bg_fill, pulse);
    }
    let response = flashing.put(rect, widget);
    let tremor = attention_shake_offset(t);
    if tremor != 0.0 {
        ui.ctx().transform_layer_shapes(
            layer,
            egui::emath::TSTransform::from_translation(Vec2::new(tremor, 0.0)),
        );
    }
    ui.ctx().request_repaint();
    response
}

pub fn control_affordances(ui: &Ui, response: &Response, ring_rect: Rect, radius: f32) {
    if response.enabled() && response.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            ring_rect,
            radius,
            crate::theme::focus_ring(),
            egui::StrokeKind::Outside,
        );
    }
}

pub struct SliderCell {
    pub label: Response,
    pub slider: Response,
    pub reset_clicked: bool,
}

pub fn slider_cell(
    ui: &mut Ui,
    label: egui::RichText,
    touched: bool,
    reset_enabled: bool,
    reset_tooltip: impl Into<egui::WidgetText>,
    add_slider: impl FnOnce(&mut Ui) -> Response,
) -> SliderCell {
    let mut reset_clicked = false;
    let mut label_response = None;
    let mut slider_response = None;
    egui::Frame::new()
        .fill(if touched {
            crate::theme::COLOR_FIELD
        } else {
            egui::Color32::TRANSPARENT
        })
        .corner_radius(crate::theme::CONTROL_RADIUS)
        .inner_margin(egui::Margin {
            left: 8,
            right: 4,
            top: 4,
            bottom: 4,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width().max(0.0));
            ui.spacing_mut().item_spacing.y = 2.0;
            label_response = Some(ui.label(label));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let reset_lane = icon_button_size(ui) + ui.spacing().item_spacing.x;
                let width = (ui.available_width() - reset_lane).max(0.0);
                slider_response = Some(
                    ui.scope(|ui| {
                        ui.set_max_width(width);
                        add_slider(ui)
                    })
                    .inner,
                );
                ui.add_enabled_ui(reset_enabled, |ui| {
                    reset_clicked = icon_button(ui, Icon::BrushRestore, reset_tooltip).clicked();
                });
            });
        });
    SliderCell {
        label: label_response.expect("the cell always draws its label"),
        slider: slider_response.expect("the cell always draws its slider"),
        reset_clicked,
    }
}

pub fn icon_button_size(ui: &Ui) -> f32 {
    ui.spacing().interact_size.y.clamp(22.0, 28.0)
}

const ICON_GLYPH_INSET: f32 = 0.22;

pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: impl Into<egui::WidgetText>) -> Response {
    let size = icon_button_size(ui);
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        if response.hovered() {
            ui.painter().rect_filled(
                rect,
                f32::from(crate::theme::RADIUS_S),
                visuals.weak_bg_fill,
            );
        }
        let color = if response.enabled() {
            visuals.fg_stroke.color
        } else {
            crate::theme::disabled(crate::theme::COLOR_ICON)
        };

        paint_icon(
            ui.painter(),
            rect.shrink(size * ICON_GLYPH_INSET),
            icon,
            color,
        );
        control_affordances(ui, &response, rect, f32::from(crate::theme::RADIUS_S));
    }
    response.on_hover_text(tooltip)
}

pub const SWITCH_TRACK_WIDTH: f32 = 36.0;

const SWITCH_ROW_INSET: f32 = 4.0;
pub const SWITCH_TRACK_HEIGHT: f32 = 18.0;
const SWITCH_THUMB_MARGIN: f32 = 3.0;

pub fn switch(ui: &mut Ui, on: &mut bool, label: impl Into<egui::WidgetText>) -> Response {
    switch_impl(ui, on, label, false, None)
}

pub fn switch_row(ui: &mut Ui, on: &mut bool, label: impl Into<egui::WidgetText>) -> Response {
    let width = ui.available_width().max(0.0);
    switch_impl(ui, on, label, true, Some(width))
}

fn switch_impl(
    ui: &mut Ui,
    on: &mut bool,
    label: impl Into<egui::WidgetText>,
    label_first: bool,
    justify_width: Option<f32>,
) -> Response {
    let gap = ui.spacing().item_spacing.x;
    let inset = if justify_width.is_some() {
        SWITCH_ROW_INSET
    } else {
        0.0
    };
    let outer_width = justify_width.unwrap_or_else(|| ui.available_width());
    let label_width_budget = (outer_width - SWITCH_TRACK_WIDTH - gap - inset * 2.0).max(0.0);
    let galley = label.into().into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        label_width_budget,
        egui::TextStyle::Body,
    );
    let label_width = if galley.is_empty() {
        0.0
    } else {
        gap + galley.size().x
    };
    let size = Vec2::new(
        justify_width.unwrap_or(SWITCH_TRACK_WIDTH + label_width),
        crate::theme::CONTROL_H_COMPACT.max(galley.size().y),
    );
    let id = ui.auto_id_with(("vkit.switch-row", galley.text()));
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let mut response = ui.interact(rect, id, Sense::click());
    let keyboard_toggle = response.has_focus()
        && ui.input(|input| {
            input.key_pressed(egui::Key::Space) || input.key_pressed(egui::Key::Enter)
        });
    if (response.clicked() || keyboard_toggle) && ui.is_enabled() {
        *on = !*on;
        response.mark_changed();
    }
    let state = *on;
    let enabled = ui.is_enabled();
    let label_text = galley.text().to_owned();
    response.widget_info(move || {
        WidgetInfo::selected(WidgetType::Checkbox, enabled, state, label_text.clone())
    });

    if ui.is_rect_visible(rect) {
        let enabled = ui.is_enabled();
        let recipe = |color: Color32| {
            if enabled {
                color
            } else {
                crate::theme::disabled(color)
            }
        };
        let track_left = if justify_width.is_some() {
            rect.right() - inset - SWITCH_TRACK_WIDTH
        } else if label_first {
            rect.left() + label_width
        } else {
            rect.left()
        };
        let track = Rect::from_min_size(
            Pos2::new(track_left, rect.center().y - SWITCH_TRACK_HEIGHT * 0.5),
            Vec2::new(SWITCH_TRACK_WIDTH, SWITCH_TRACK_HEIGHT),
        );
        let (track_fill, thumb_fill) = if *on {
            (crate::theme::COLOR_PRIMARY, crate::theme::COLOR_BG)
        } else {
            (crate::theme::COLOR_SURFACE_HOVER, crate::theme::COLOR_MUTED)
        };
        let thumb_side = SWITCH_TRACK_HEIGHT - SWITCH_THUMB_MARGIN * 2.0;
        let thumb_left = if *on {
            track.right() - SWITCH_THUMB_MARGIN - thumb_side
        } else {
            track.left() + SWITCH_THUMB_MARGIN
        };
        let thumb_target = Rect::from_min_size(
            Pos2::new(thumb_left, track.top() + SWITCH_THUMB_MARGIN),
            Vec2::splat(thumb_side),
        );
        let thumb = animate_rect(
            ui,
            response.id.with("switch-thumb"),
            u64::from(*on),
            thumb_target,
        );
        let painter = ui.painter();
        painter.rect_filled(track, track.height() * 0.5, recipe(track_fill));
        painter.circle_filled(thumb.center(), thumb.width() * 0.5, recipe(thumb_fill));
        if !galley.is_empty() {
            let text_left = if justify_width.is_some() {
                rect.left() + inset
            } else if label_first {
                rect.left()
            } else {
                track.right() + gap
            };
            let text_pos = Pos2::new(text_left, rect.center().y - galley.size().y * 0.5);
            painter.galley(text_pos, galley, recipe(crate::theme::COLOR_TEXT));
        }
        control_affordances(ui, &response, track, track.height() * 0.5);
    }
    response
}

const CHIP_HEIGHT: f32 = crate::theme::CONTROL_H_DENSE;

pub fn chips(ui: &mut Ui, id: Id, active: Option<usize>, labels: &[&str]) -> (Rect, Option<usize>) {
    let mut clicked = None;
    let mut bounds = Rect::NOTHING;
    let mut selected_rect = None;

    let thumb_slot = Some(ui.painter().add(Shape::Noop));
    let layout_rect = ui
        .horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(crate::theme::SPACE_2, crate::theme::SPACE_2);
            for (index, label) in labels.iter().enumerate() {
                let selected = active == Some(index);
                let (text_color, fill) = (
                    if selected {
                        crate::theme::COLOR_BG
                    } else {
                        crate::theme::COLOR_MUTED
                    },
                    if selected {
                        Color32::TRANSPARENT
                    } else {
                        crate::theme::COLOR_SURFACE_RAISED
                    },
                );
                let response = ui.add(
                    egui::Button::new(egui::RichText::new(*label).color(text_color))
                        .wrap_mode(egui::TextWrapMode::Extend)
                        .fill(fill)
                        .stroke(Stroke::NONE)
                        .corner_radius(crate::theme::CAPSULE_RADIUS)
                        .min_size(Vec2::new(0.0, CHIP_HEIGHT)),
                );
                control_affordances(ui, &response, response.rect, CHIP_HEIGHT * 0.5);
                bounds = bounds.union(response.rect);
                if selected {
                    selected_rect = Some((index, response.rect));
                }
                if response.clicked() {
                    clicked = Some(index);
                }
            }
        })
        .response
        .rect;
    if let (Some(slot), Some((selected_index, target))) = (thumb_slot, selected_rect) {
        let thumb = animate_rect(ui, id.with("chips-thumb"), selected_index as u64, target);
        ui.painter().set(
            slot,
            Shape::rect_filled(thumb, thumb.height() * 0.5, crate::theme::COLOR_TEXT),
        );
    }
    let rect = if labels.is_empty() {
        layout_rect
    } else {
        bounds
    };
    (rect, clicked)
}

pub fn toggle_chip(ui: &mut Ui, id: Id, label: &str, on: bool, width: f32) -> Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, CHIP_HEIGHT), Sense::hover());
    let (ink, fill) = if on {
        (crate::theme::COLOR_BG, crate::theme::COLOR_TEXT)
    } else {
        (
            crate::theme::COLOR_MUTED,
            crate::theme::COLOR_SURFACE_RAISED,
        )
    };
    ui.painter()
        .rect_filled(rect, crate::theme::CAPSULE_RADIUS, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(crate::theme::FONT_SM),
        ink,
    );
    let response = ui.interact(rect, id, Sense::click());
    control_affordances(ui, &response, rect, CHIP_HEIGHT * 0.5);
    response
}

pub fn animated_segmented_group<R>(
    ui: &mut Ui,
    id_salt: &'static str,
    count: usize,
    selected_index: usize,
    add_contents: impl FnOnce(&mut Ui, f32) -> R,
) -> R {
    animated_segmented_group_with(ui, id_salt, count, selected_index, false, add_contents)
}

pub fn animated_segmented_group_circular<R>(
    ui: &mut Ui,
    id_salt: &'static str,
    count: usize,
    selected_index: usize,
    add_contents: impl FnOnce(&mut Ui, f32) -> R,
) -> R {
    animated_segmented_group_with(ui, id_salt, count, selected_index, true, add_contents)
}

fn animated_segmented_group_with<R>(
    ui: &mut Ui,
    id_salt: &'static str,
    count: usize,
    selected_index: usize,
    circular_marker: bool,
    add_contents: impl FnOnce(&mut Ui, f32) -> R,
) -> R {
    let count = count.max(1);
    let (rect, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width().max(0.0), crate::theme::CONTROL_H),
        Sense::hover(),
    );
    let inner = rect.shrink2(Vec2::new(2.0, 2.0));
    let gap = 2.0;
    let segment_width =
        ((inner.width() - gap * count.saturating_sub(1) as f32).max(0.0) / count as f32).max(0.0);
    let clamped_index = selected_index.min(count - 1);
    let selected = clamped_index as f32;
    let target = Rect::from_min_size(
        Pos2::new(inner.left() + selected * (segment_width + gap), inner.top()),
        Vec2::new(segment_width, inner.height()),
    );
    let active_rect = animate_rect(
        ui,
        Id::new((id_salt, "active-capsule")),
        clamped_index as u64,
        target,
    );
    ui.painter().rect_filled(
        rect,
        f32::from(crate::theme::CAPSULE_RADIUS),
        crate::theme::COLOR_FIELD,
    );
    if circular_marker {
        ui.painter().circle_filled(
            active_rect.center(),
            active_rect.width().min(active_rect.height()) * 0.5,
            crate::theme::COLOR_SURFACE_RAISED,
        );
    } else {
        ui.painter().rect_filled(
            active_rect,
            f32::from(crate::theme::CAPSULE_RADIUS),
            crate::theme::COLOR_SURFACE_RAISED,
        );
    }

    let mut group_ui = ui.new_child(
        egui::UiBuilder::new()
            .id_salt((id_salt, "segments"))
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center)),
    );
    group_ui.spacing_mut().item_spacing.x = gap;
    add_contents(&mut group_ui, segment_width)
}

const SEGMENT_LABEL_INSET: f32 = 6.0;

pub fn segment_button(ui: &mut Ui, width: f32, label: &str, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(width, ui.available_height().max(0.0)),
        Sense::click(),
    );
    let color = if selected {
        crate::theme::COLOR_PRIMARY
    } else {
        crate::theme::COLOR_MUTED
    };

    let font = egui::FontId::proportional(crate::theme::FONT_BODY);
    let available = (rect.width() - SEGMENT_LABEL_INSET * 2.0).max(1.0);
    let galley = ui.painter().layout(
        label.to_owned(),
        font,
        color,
        if ui
            .painter()
            .layout_no_wrap(
                label.to_owned(),
                egui::FontId::proportional(crate::theme::FONT_BODY),
                color,
            )
            .size()
            .x
            > available
        {
            available
        } else {
            f32::INFINITY
        },
    );
    ui.painter()
        .with_clip_rect(rect)
        .galley(rect.center() - galley.size() * 0.5, galley, color);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, label)
    });
    control_affordances(ui, &response, rect, rect.height() * 0.5);
    response
}

pub fn compact_color_picker(ui: &mut Ui, color: &mut [u8; 3]) -> Response {
    compact_color_picker_sized(ui, color, COMPACT_COLOR_SWATCH_WIDTH)
}

pub fn compact_color_picker_sized(ui: &mut Ui, color: &mut [u8; 3], width: f32) -> Response {
    let width = width
        .clamp(0.0, COMPACT_COLOR_SWATCH_WIDTH)
        .min(ui.available_width().max(0.0));
    color_picker_button(
        ui,
        color,
        Vec2::new(width, COMPACT_COLOR_SWATCH_HEIGHT),
        COMPACT_COLOR_SWATCH_RADIUS,
        None,
    )
}

pub fn readable_ink(fill: Color32) -> Color32 {
    let luminance =
        0.2126 * f32::from(fill.r()) + 0.7152 * f32::from(fill.g()) + 0.0722 * f32::from(fill.b());
    if luminance > 140.0 {
        crate::theme::COLOR_ACTIVE_INK
    } else {
        crate::theme::COLOR_TEXT
    }
}

pub fn color_capsule_picker(ui: &mut Ui, color: &mut [u8; 3], label: &str, size: Vec2) -> Response {
    color_picker_button(
        ui,
        color,
        size,
        capsule_radius_for_height(size.y),
        Some(label),
    )
}

fn capsule_radius_for_height(height: f32) -> f32 {
    (height * 0.5).max(0.0)
}

fn color_picker_button(
    ui: &mut Ui,
    color: &mut [u8; 3],
    size: Vec2,
    radius: f32,
    label: Option<&str>,
) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());
    let popup_id = response.id.with("compact-color-picker.popup");
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let fill = Color32::from_rgb(color[0], color[1], color[2]);
    let border = if open || response.hovered() {
        crate::theme::COLOR_HAIRLINE_STRONG
    } else {
        crate::theme::COLOR_HAIRLINE
    };
    ui.painter().rect_stroke(
        rect.expand(1.0),
        radius,
        Stroke::new(1.0, Color32::from_black_alpha(150)),
        egui::StrokeKind::Outside,
    );
    ui.painter().rect_filled(rect, radius, fill);
    ui.painter().rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    if let Some(label) = label {
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(crate::theme::FONT_SM),
            readable_ink(fill),
        );
    }
    control_affordances(ui, &response, rect, radius);

    egui::Popup::from_toggle_button_response(&response)
        .id(popup_id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            let changed = ui
                .horizontal(|ui| {
                    ui.add_space(MINI_POPUP_CONTENT_INSET_X);
                    let changed = ui
                        .vertical(|ui| {
                            ui.add_space(MINI_POPUP_CONTENT_INSET_Y);
                            let changed = compact_color_picker_popup(ui, popup_id, color);
                            ui.add_space(MINI_POPUP_CONTENT_INSET_Y);
                            changed
                        })
                        .inner;
                    ui.add_space(MINI_POPUP_CONTENT_INSET_X);
                    changed
                })
                .inner;
            if changed {
                response.mark_changed();
            }
        });

    response
}

fn compact_color_picker_popup(ui: &mut Ui, popup_id: Id, color: &mut [u8; 3]) -> bool {
    let before = *color;
    let hex_id = popup_id.with("hex-value");
    let hex_field_id = popup_id.with("hex-field");
    let hex_focused = ui.ctx().memory(|memory| memory.has_focus(hex_field_id));
    let mut hex = ui
        .ctx()
        .data(|data| data.get_temp::<String>(hex_id))
        .unwrap_or_else(|| format_color_hex(*color));
    if !hex_focused {
        hex = format_color_hex(*color);
    }

    ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
    ui.horizontal_top(|ui| {
        let mut hsva =
            egui::ecolor::HsvaGamma::from(Color32::from_rgb(color[0], color[1], color[2]));
        let hsv_changed = ui
            .vertical(|ui| {
                let mut changed = color_sv_control(ui, &mut hsva);
                changed |= color_hue_control(ui, &mut hsva);
                changed
            })
            .inner;
        if hsv_changed {
            let value = Color32::from(hsva);
            *color = [value.r(), value.g(), value.b()];
            hex = format_color_hex(*color);
        }

        ui.add_space(COLOR_PICKER_COLUMN_GAP);
        ui.vertical(|ui| {
            let mut rgb = *color;
            let mut rgb_changed = false;
            for (label, value) in ["R", "G", "B"].into_iter().zip(&mut rgb) {
                rgb_changed |= compact_color_value_row(ui, label, value);
            }
            if rgb_changed {
                *color = rgb;
                hex = format_color_hex(*color);
            }

            let hex_response = ui
                .horizontal(|ui| {
                    ui.add_sized(
                        [COLOR_PICKER_FIELD_LABEL_WIDTH, COLOR_PICKER_FIELD_HEIGHT],
                        egui::Label::new("Hex"),
                    );
                    ui.add_sized(
                        [COLOR_PICKER_FIELD_WIDTH, COLOR_PICKER_FIELD_HEIGHT],
                        TextEdit::singleline(&mut hex)
                            .id(hex_field_id)
                            .char_limit(7)
                            .font(egui::TextStyle::Monospace)
                            .horizontal_align(Align::Center),
                    )
                })
                .inner;
            if hex_response.changed()
                && let Some(parsed) = parse_color_hex(&hex)
            {
                *color = parsed;
            }
        });
    });
    ui.ctx().data_mut(|data| data.insert_temp(hex_id, hex));
    *color != before
}

fn compact_color_value_row(ui: &mut Ui, label: &str, value: &mut u8) -> bool {
    ui.horizontal(|ui| {
        ui.add_sized(
            [COLOR_PICKER_FIELD_LABEL_WIDTH, COLOR_PICKER_FIELD_HEIGHT],
            egui::Label::new(label),
        );
        ui.add_sized(
            [COLOR_PICKER_FIELD_WIDTH, COLOR_PICKER_FIELD_HEIGHT],
            egui::DragValue::new(value).range(0..=255).speed(0.5),
        )
        .changed()
    })
    .inner
}

fn color_sv_control(ui: &mut Ui, hsva: &mut egui::ecolor::HsvaGamma) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::splat(COLOR_PICKER_SV_EDGE), Sense::click_and_drag());
    let before = (hsva.s, hsva.v);
    if let Some(pointer) = response.interact_pointer_pos() {
        hsva.s = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        hsva.v = ((rect.bottom() - pointer.y) / rect.height()).clamp(0.0, 1.0);
    }
    if ui.is_rect_visible(rect) {
        let mut mesh = egui::Mesh::default();
        let row = COLOR_PICKER_GRADIENT_STEPS + 1;
        for y in 0..=COLOR_PICKER_GRADIENT_STEPS {
            let value = 1.0 - y as f32 / COLOR_PICKER_GRADIENT_STEPS as f32;
            for x in 0..=COLOR_PICKER_GRADIENT_STEPS {
                let saturation = x as f32 / COLOR_PICKER_GRADIENT_STEPS as f32;
                mesh.colored_vertex(
                    Pos2::new(
                        rect.left() + rect.width() * saturation,
                        rect.top() + rect.height() * (1.0 - value),
                    ),
                    egui::ecolor::HsvaGamma {
                        h: hsva.h,
                        s: saturation,
                        v: value,
                        a: 1.0,
                    }
                    .into(),
                );
                if x < COLOR_PICKER_GRADIENT_STEPS && y < COLOR_PICKER_GRADIENT_STEPS {
                    let top_left = (y * row + x) as u32;
                    mesh.add_triangle(top_left, top_left + 1, top_left + row as u32);
                    mesh.add_triangle(
                        top_left + 1,
                        top_left + row as u32,
                        top_left + row as u32 + 1,
                    );
                }
            }
        }
        ui.painter().add(Shape::mesh(mesh));
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, crate::theme::COLOR_HAIRLINE),
            egui::StrokeKind::Inside,
        );
        let center = Pos2::new(
            rect.left() + rect.width() * hsva.s,
            rect.bottom() - rect.height() * hsva.v,
        );
        let selected: Color32 = (*hsva).into();
        ui.painter().circle(
            center,
            8.0,
            Color32::TRANSPARENT,
            Stroke::new(1.4, color_marker_stroke(selected)),
        );
    }
    before != (hsva.s, hsva.v)
}

fn color_hue_control(ui: &mut Ui, hsva: &mut egui::ecolor::HsvaGamma) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(COLOR_PICKER_SV_EDGE, COLOR_PICKER_HUE_HEIGHT),
        Sense::click_and_drag(),
    );
    let before = hsva.h;
    if let Some(pointer) = response.interact_pointer_pos() {
        hsva.h = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
    }
    if ui.is_rect_visible(rect) {
        let mut mesh = egui::Mesh::default();
        let steps = 24_u32;
        for step in 0..=steps {
            let hue = step as f32 / steps as f32;
            let x = rect.left() + rect.width() * hue;
            let color: Color32 = egui::ecolor::HsvaGamma {
                h: hue,
                s: 1.0,
                v: 1.0,
                a: 1.0,
            }
            .into();
            mesh.colored_vertex(Pos2::new(x, rect.top()), color);
            mesh.colored_vertex(Pos2::new(x, rect.bottom()), color);
            if step < steps {
                let index = step * 2;
                mesh.add_triangle(index, index + 1, index + 2);
                mesh.add_triangle(index + 1, index + 2, index + 3);
            }
        }
        ui.painter().add(Shape::mesh(mesh));
        ui.painter().rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, crate::theme::COLOR_HAIRLINE),
            egui::StrokeKind::Inside,
        );
        let x = rect.left() + rect.width() * hsva.h;
        ui.painter().line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(1.5, Color32::WHITE),
        );
    }
    before != hsva.h
}

fn color_marker_stroke(color: Color32) -> Color32 {
    let luminance =
        0.2126 * color.r() as f32 + 0.7152 * color.g() as f32 + 0.0722 * color.b() as f32;
    if luminance > 140.0 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

fn format_color_hex(color: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

fn parse_color_hex(value: &str) -> Option<[u8; 3]> {
    let digits = value.trim().strip_prefix('#').unwrap_or(value.trim());
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some([
        u8::from_str_radix(&digits[0..2], 16).ok()?,
        u8::from_str_radix(&digits[2..4], 16).ok()?,
        u8::from_str_radix(&digits[4..6], 16).ok()?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(context: &egui::Context, id: Id, at: Pos2) {
        context.data_mut(|data| {
            data.insert_temp(
                id,
                crate::sweep_gesture::Sweep {
                    start_pointer: at,
                    start_value: 0.0,
                },
            );
        });
    }

    #[test]
    fn every_painting_surface_answers_to_both_sweeps() {
        let mut seen = std::collections::HashSet::new();
        for surface in BrushSweeps::ALL {
            assert_ne!(
                surface.size(),
                surface.strength(),
                "{surface:?} sweeps one value under two names"
            );
            assert!(
                seen.insert(surface.size()) && seen.insert(surface.strength()),
                "{surface:?} shares an id with another surface, so one sweep                  would end the other"
            );
        }
        assert_eq!(seen.len(), BrushSweeps::ALL.len() * 2);
    }

    #[test]
    fn both_sweeps_pin_the_ring_and_only_one_of_them_fills_it() {
        let context = egui::Context::default();
        let size = Id::new("test.size");
        let strength = Id::new("test.strength");
        let hover = Pos2::new(500.0, 500.0);
        let anchor = Pos2::new(100.0, 100.0);

        let _ = context.run_ui(egui::RawInput::default(), |ui| {
            let idle = brush_cursor(ui, Some(hover), size, Some((strength, 0.7)))
                .expect("a hovering pointer has a ring");
            assert_eq!(idle.at, hover);
            assert_eq!(idle.fill, None);

            pin(ui.ctx(), size, anchor);
            let sizing = brush_cursor(ui, Some(hover), size, Some((strength, 0.7)))
                .expect("a size sweep has a ring");
            assert_eq!(
                sizing.at, anchor,
                "the ring must not be dragged by the sweep"
            );
            assert_eq!(
                sizing.fill, None,
                "a size sweep says how wide, and the ring is already saying it"
            );

            pin(ui.ctx(), strength, anchor);
            let sweeping = brush_cursor(ui, Some(hover), size, Some((strength, 0.7)))
                .expect("a strength sweep has a ring");
            assert_eq!(sweeping.at, anchor);
            assert_eq!(sweeping.fill, Some(0.7));

            let plain = brush_cursor(ui, Some(hover), Id::new("test.other"), None)
                .expect("a ring with no sweeps");
            assert_eq!(plain.at, hover);
            assert_eq!(plain.fill, None);
        });
    }

    #[test]
    fn the_fill_stays_inside_the_ring_and_never_vanishes() {
        let context = egui::Context::default();
        let strength = Id::new("test.strength.range");
        let _ = context.run_ui(egui::RawInput::default(), |ui| {
            pin(ui.ctx(), strength, Pos2::ZERO);
            for (given, expected) in [(-4.0, 0.0), (0.0, 0.0), (0.5, 0.5), (1.0, 1.0), (9.0, 1.0)] {
                let cursor = brush_cursor(
                    ui,
                    None,
                    Id::new("test.size.range"),
                    Some((strength, given)),
                )
                .expect("a pinned sweep has a ring even with no pointer");
                assert_eq!(
                    cursor.fill,
                    Some(expected),
                    "{given} has to land inside the ring"
                );
            }
        });
    }

    fn icon_button_size_for_test() -> f32 {
        let context = egui::Context::default();
        let mut size = 0.0;
        let _ = context.run_ui(egui::RawInput::default(), |ui| {
            size = icon_button_size(ui);
        });
        size
    }

    #[test]
    fn an_icon_button_draws_small_and_stays_easy_to_hit() {
        use egui::epaint::Shape;

        let context = egui::Context::default();
        let mut painted = egui::Rect::NOTHING;
        let mut allocated = egui::Rect::NOTHING;
        let output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(200.0, 200.0),
                )),
                ..Default::default()
            },
            |ui| {
                let response = icon_button(ui, Icon::Refresh, "reset");
                allocated = response.rect;
            },
        );
        fn walk(shape: &Shape, bounds: &mut egui::Rect) {
            match shape {
                Shape::Vec(children) => {
                    for child in children {
                        walk(child, bounds);
                    }
                }

                Shape::Path(path) => {
                    for point in &path.points {
                        *bounds = bounds.union(egui::Rect::from_min_size(*point, egui::Vec2::ZERO));
                    }
                }
                Shape::Circle(circle) => {
                    *bounds = bounds.union(egui::Rect::from_center_size(
                        circle.center,
                        egui::Vec2::splat(circle.radius * 2.0),
                    ));
                }
                _ => {}
            }
        }
        for shape in &output.shapes {
            walk(&shape.shape, &mut painted);
        }
        assert!(painted.is_positive(), "the icon drew nothing");

        let box_inset = allocated.width() * ICON_GLYPH_INSET;
        let glyph_box = allocated.shrink(box_inset);
        assert!(
            glyph_box.contains_rect(painted),
            "the glyph {painted:?} spills out of its box {glyph_box:?}"
        );

        assert!(
            (allocated.width() - icon_button_size_for_test()).abs() < 0.01,
            "the hit rect shrank to {:.1}pt",
            allocated.width()
        );
        eprintln!(
            "icon ink {:.1}pt in a {:.1}pt box in a {:.1}pt button",
            painted.width(),
            glyph_box.width(),
            allocated.width()
        );
    }

    #[test]
    fn cover_uv_crops_the_long_side_and_keeps_a_square_source_whole() {
        let box_square = Vec2::new(64.0, 64.0);

        let whole = cover_uv(box_square, Vec2::new(100.0, 100.0));
        assert_eq!(whole.min, Pos2::ZERO);
        assert_eq!(whole.max, egui::pos2(1.0, 1.0));

        let wide = cover_uv(box_square, Vec2::new(200.0, 100.0));
        assert!((wide.min.x - 0.25).abs() < 1e-6 && (wide.max.x - 0.75).abs() < 1e-6);
        assert_eq!((wide.min.y, wide.max.y), (0.0, 1.0));
        let tall = cover_uv(box_square, Vec2::new(100.0, 200.0));
        assert!((tall.min.y - 0.25).abs() < 1e-6 && (tall.max.y - 0.75).abs() < 1e-6);
        assert_eq!((tall.min.x, tall.max.x), (0.0, 1.0));

        assert_eq!(
            cover_uv(Vec2::new(0.0, 10.0), Vec2::new(10.0, 10.0)).max,
            egui::pos2(1.0, 1.0)
        );
    }

    #[test]
    fn a_list_row_reads_the_same_in_every_list() {
        let selected = list_row_fill(true, false).expect("a selected row is filled");
        let hovered = list_row_fill(false, true).expect("a hovered row is filled");

        assert_eq!(
            list_row_fill(false, false),
            None,
            "an idle row paints nothing"
        );
        assert_ne!(selected, hovered, "selection must not look like hover");
        assert_eq!(
            list_row_fill(true, true),
            Some(selected),
            "hovering a selected row must keep reading as selected"
        );
    }

    #[test]
    fn rect_motion_eases_with_visible_acceleration_and_deceleration() {
        let motion = RectMotion {
            key: 0,
            from: Rect::from_min_size(Pos2::ZERO, Vec2::splat(10.0)),
            to: Rect::from_min_size(Pos2::new(100.0, 0.0), Vec2::splat(10.0)),
            started_at: 0.0,
        };
        let (start, start_done) = motion.sample(0.0);
        assert_eq!(start.min.x, 0.0);
        assert!(!start_done);
        let (quarter, _) = motion.sample(MOTION_DURATION_SECS * 0.25);
        assert!(quarter.min.x > 0.0 && quarter.min.x < 25.0);
        let (three_quarter, _) = motion.sample(MOTION_DURATION_SECS * 0.75);
        assert!(three_quarter.min.x > 75.0 && three_quarter.min.x < 100.0);
        let (end, end_done) = motion.sample(MOTION_DURATION_SECS);
        assert_eq!(end.min.x, 100.0);
        assert!(end_done);
    }

    #[test]
    fn animate_rect_snaps_on_first_sight_then_glides_from_current() {
        egui::__run_test_ui(|ui| {
            let id = Id::new("test.animate-rect");
            let first = Rect::from_min_size(Pos2::ZERO, Vec2::splat(20.0));
            assert_eq!(animate_rect(ui, id, 0, first), first);

            let shifted = Rect::from_min_size(Pos2::new(40.0, 0.0), Vec2::splat(20.0));
            assert_eq!(animate_rect(ui, id, 0, shifted), shifted);

            let second = Rect::from_min_size(Pos2::new(60.0, 0.0), Vec2::splat(20.0));
            let retargeted = animate_rect(ui, id, 1, second);
            assert_eq!(retargeted, shifted);
        });
    }

    #[test]
    fn attention_pulse_breathes_twice_between_silent_endpoints() {
        assert_eq!(attention_pulse(0.0), 0.0);
        assert!(attention_pulse(-0.1) == 0.0 && attention_pulse(ATTENTION_FLASH_SECS) == 0.0);
        assert!(attention_pulse(ATTENTION_FLASH_SECS + 1.0) == 0.0);

        assert!((attention_pulse(ATTENTION_FLASH_SECS * 0.25) - 1.0).abs() < 1.0e-5);
        assert!((attention_pulse(ATTENTION_FLASH_SECS * 0.75) - 1.0).abs() < 1.0e-5);
        assert!(attention_pulse(ATTENTION_FLASH_SECS * 0.5) < 1.0e-5);
        for step in 0..=48 {
            let t = ATTENTION_FLASH_SECS * step as f32 / 48.0;
            assert!((0.0..=1.0).contains(&attention_pulse(t)));
        }
    }

    #[test]
    fn attention_shake_is_bounded_damped_and_settles_quickly() {
        assert_eq!(attention_shake_offset(0.0), 0.0);
        assert_eq!(attention_shake_offset(ATTENTION_SHAKE_SECS), 0.0);
        assert_eq!(attention_shake_offset(1.0), 0.0);
        let mut peak = 0.0_f32;
        let mut signs = 0;
        let mut last_sign = 0;
        for step in 1..200 {
            let t = ATTENTION_SHAKE_SECS * step as f32 / 200.0;
            let offset = attention_shake_offset(t);
            assert!(offset.abs() <= ATTENTION_SHAKE_AMPLITUDE);
            peak = peak.max(offset.abs());
            let sign = if offset > 0.01 {
                1
            } else if offset < -0.01 {
                -1
            } else {
                0
            };
            if sign != 0 && sign != last_sign {
                signs += 1;
                last_sign = sign;
            }
        }
        assert!(peak > 1.5, "shake must be visible, peaked at {peak}");
        assert!(signs >= 4, "shake must oscillate, saw {signs} sign changes");

        assert!(attention_shake_offset(ATTENTION_SHAKE_SECS * 0.95).abs() < 1.0);
    }

    #[test]
    fn attention_widget_state_machine_arms_then_expires() {
        egui::__run_test_ui(|ui| {
            let id = Id::new("test.attention-flash");
            let slot = attention_slot(id);
            assert!(
                ui.data(|data| data.get_temp::<AttentionStart>(slot))
                    .is_none()
            );
            attention_flash(ui.ctx(), id);
            assert!(
                ui.data(|data| data.get_temp::<AttentionStart>(slot))
                    .is_some()
            );

            let _ = attention_widget(ui, id, Vec2::new(96.0, 24.0), egui::Button::new("x"));
            assert!(
                ui.data(|data| data.get_temp::<AttentionStart>(slot))
                    .is_some()
            );

            ui.data_mut(|data| {
                data.insert_temp(slot, AttentionStart(-f64::from(ATTENTION_FLASH_SECS) - 1.0));
            });
            let _ = attention_widget(ui, id, Vec2::new(96.0, 24.0), egui::Button::new("x"));
            assert!(
                ui.data(|data| data.get_temp::<AttentionStart>(slot))
                    .is_none()
            );
        });
    }

    #[test]
    fn attention_flash_never_moves_the_allocated_layout() {
        egui::__run_test_ui(|ui| {
            let id = Id::new("test.attention-layout");
            let size = Vec2::new(96.0, 24.0);

            let calm = attention_widget(ui, id, size, egui::Button::new("x"));
            assert_eq!(calm.rect.size(), size);

            let now = ui.input(|input| input.time);
            let mid_shake = 0.1_f32;
            assert!(attention_shake_offset(mid_shake).abs() > 0.5);
            ui.data_mut(|data| {
                data.insert_temp(
                    attention_slot(id),
                    AttentionStart(now - f64::from(mid_shake)),
                );
            });
            let after_baseline = ui.cursor().min;
            let flashing = attention_widget(ui, id, size, egui::Button::new("x"));

            assert_eq!(flashing.rect.size(), size);
            assert_eq!(flashing.rect.min.x, calm.rect.min.x);
            assert_eq!(flashing.rect.min.y, after_baseline.y);
            assert_eq!(
                ui.cursor().min.y - after_baseline.y,
                calm.rect.height() + ui.spacing().item_spacing.y
            );
        });
    }

    #[test]
    fn compact_color_hex_round_trips_and_rejects_partial_input() {
        assert_eq!(format_color_hex([77, 132, 110]), "#4D846E");
        assert_eq!(parse_color_hex("#4D846E"), Some([77, 132, 110]));
        assert_eq!(parse_color_hex("4d846e"), Some([77, 132, 110]));
        assert_eq!(parse_color_hex("#4D84"), None);
        assert_eq!(parse_color_hex("#GG846E"), None);
    }

    #[test]
    fn min_width_reserves_track_space() {
        let mut value = 0.5;
        let slider = FilledNumericSlider::new(&mut value, 0.0..=1.0).min_width(1.0);
        assert!(slider.min_width >= VALUE_WIDTH + VALUE_GAP + 24.0);
    }

    #[test]
    fn hidden_value_slider_can_fit_a_compact_toolbar_slot() {
        let mut value = 0.5;
        let slider = FilledNumericSlider::new(&mut value, 0.0..=1.0)
            .hide_value()
            .min_width(32.0);
        assert_eq!(slider.min_width, 32.0);
    }

    #[test]
    fn compact_color_tokens_are_a_low_rectangular_swatch_with_shared_popup_insets() {
        assert_eq!(COMPACT_COLOR_SWATCH_HEIGHT, 24.0);
        const { assert!(COMPACT_COLOR_SWATCH_WIDTH > COMPACT_COLOR_SWATCH_HEIGHT) };
        assert_eq!(COMPACT_COLOR_SWATCH_RADIUS, 16.0);
        assert_eq!(MINI_POPUP_CONTENT_INSET_X, MINI_HELP_CONTENT_INSET_X);
        assert_eq!(MINI_POPUP_CONTENT_INSET_Y, MINI_HELP_CONTENT_INSET_Y);
    }

    #[test]
    fn a_thumbnail_slot_is_per_list_and_per_layer() {
        let key = |namespace, id| Id::new((namespace, id));
        assert_ne!(key("a", 1_u64), key("a", 2));
        assert_ne!(key("a", 1_u64), key("b", 1));
        assert_eq!(key("a", 1_u64), key("a", 1));
    }

    #[test]
    fn the_thumbnail_is_downscaled_but_keeps_its_aspect() {
        let image = crate::skin_preview::SkinImage::new(1, 2048, 1024, vec![0_u8; 2048 * 1024 * 4])
            .unwrap();
        let thumbnail = thumbnail_color_image(&image);
        assert_eq!(thumbnail.size, [THUMBNAIL_TEXELS as usize, 48]);

        let small = crate::skin_preview::SkinImage::new(1, 8, 4, vec![0_u8; 8 * 4 * 4]).unwrap();
        assert_eq!(thumbnail_color_image(&small).size, [8, 4]);
    }
}
