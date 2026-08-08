use std::time::Duration;

use egui::{
    Align, Align2, Button, Color32, CursorIcon, FontId, Frame, Id, Key, Layout, Margin,
    PointerButton, Pos2, Rect, Response, RichText, ScrollArea, Sense, Stroke, TextureHandle,
    TextureOptions, Ui, UiBuilder, Vec2, containers::scroll_area::ScrollBarVisibility, pos2, vec2,
};
use vkit_core::texture_bake::{
    TextureBlendMode, TextureColorAdjustments, TextureWarpPin, apply_color_adjustments,
    map_source_point_to_g2,
};
use vkit_core::texture_mirror::FaceMirror;

use crate::{
    i18n::{Locale, TextKey, text},
    shortcuts::Shortcut,
    skin_preview::SkinImage,
    state::{Action, AppState},
    texture_project::{
        TextureBakeBase, TextureBakeQuality, TextureChannel, TextureLayer, TexturePbrConvention,
        TextureSourceMode, TextureTool,
    },
    theme::{
        CAPSULE_RADIUS, COLOR_BG, COLOR_BORDER, COLOR_FIELD, COLOR_MUTED, COLOR_PRIMARY,
        COLOR_SURFACE_HOVER, COLOR_SURFACE_RAISED, COLOR_TEXT, CONTROL_H_DENSE, CONTROL_RADIUS,
        FONT_SM, FONT_XS, PANEL_INSET, SPACE_1, SPACE_2, SPACE_3, SPACE_4,
    },
    ui_components::{
        BRUSH_FALLOFF_COMPACT_WIDTH, FilledNumericSlider, Icon, animated_segmented_group,
        brush_falloff_selector, clear_brush_size_gesture, compact_brush_numeric_control,
        control_affordances, handle_brush_size_gesture, icon_button, paint_icon,
        paint_list_row_highlight, paint_texture_pin, segment_button, switch_row,
    },
};

const TEXTURE_LAYER_ROW_HEIGHT: f32 = 48.0;

const TEXTURE_LAYER_ROW_GAP: f32 = SPACE_2;

const TEXTURE_RETOUCH_DIRECTION_WIDTH: f32 = 86.0;

const ADD_LAYER_EXTRA_GAP: f32 = SPACE_3;

const ADD_LAYER_ICON: f32 = 16.0;

pub(crate) const BRUSH_SPACING_FRACTION: f32 = 0.18;

const LAYER_PROPERTY_COMBO_WIDTH: f32 = 96.0;
const TEXTURE_LAYER_LIST_ROWS: f32 = 5.0;
const TEXTURE_THUMBNAIL_SIZE: f32 = 36.0;
const TEXTURE_PIN_HIT_RADIUS: f32 = 12.0;

const TEXTURE_BRUSH_CONTROL_WIDTH: f32 = 320.0;
const TEXTURE_BRUSH_SIZE_SENSITIVITY: f32 = 0.0008;
const TEXTURE_AUTO_BAKE_DEBOUNCE: Duration = Duration::from_millis(48);

#[derive(Clone, Copy)]
struct TextureAutoBakeTimer {
    revision: u64,
    ready_at: f64,
    requested: bool,
}

#[derive(Clone)]
struct MaskPreviewTextureCache {
    revision: u64,
    width: u32,
    height: u32,
    handle: TextureHandle,
}

const TEXTURE_THUMBNAIL_NS: &str = "vkit.texture.thumbnail";

pub fn draw_texture_inspector(ui: &mut Ui, state: &mut AppState) {
    Frame::new()
        .inner_margin(Margin {
            left: PANEL_INSET as i8,
            right: PANEL_INSET as i8,
            top: 2,
            bottom: PANEL_INSET as i8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width().max(0.0));
            draw_texture_inspector_contents(ui, state);
        });
}

fn draw_texture_inspector_contents(ui: &mut Ui, state: &mut AppState) {
    ui.add_space(SPACE_2);
    draw_layer_toolbar(ui, state);
    ui.add_space(SPACE_2);
    draw_layer_list(ui, state);

    let Some(layer) = state.texture_project.selected_layer().cloned() else {
        ui.add_space(SPACE_4);
        ui.label(
            RichText::new(text(state.locale, TextKey::AddTextureLayerHint))
                .size(FONT_SM)
                .color(COLOR_MUTED),
        );
        return;
    };
    ui.add_space(SPACE_3);
    draw_selected_layer_controls(ui, state, &layer);
    ui.add_space(SPACE_3);
    draw_bake_controls(ui, state);
}

pub fn draw_source_workspace(ui: &mut Ui, state: &mut AppState, rect: Rect) {
    let mut source_ui = ui.new_child(
        UiBuilder::new()
            .id_salt("vkit.texture.source-pane")
            .max_rect(rect),
    );
    source_ui.set_clip_rect(rect);
    let ui = &mut source_ui;
    ui.painter().rect_filled(rect, 0.0, COLOR_BG);
    // No hover text on the canvas itself. It covers the whole 2D view, so the
    // tip fired the moment the tab opened and parked itself over the top tabs
    // -- a hint nobody asked for, in front of the thing they were reaching for.
    // The help button in the corner already says all of this.
    let response = ui.interact(
        rect,
        Id::new("vkit.texture.source-workspace"),
        Sense::click_and_drag(),
    );
    let Some(layer) = state.texture_project.selected_layer().cloned() else {
        paint_empty_source(ui, state.locale, rect);
        return;
    };

    let projected_route = state.texture_project.active_tool == TextureTool::Projection
        || (layer.painted.is_some() && state.texture_project.active_tool != TextureTool::PinPair);
    if projected_route {
        draw_projection_canvas(ui, state, rect, &response, &layer);
        return;
    }
    let Some(image) = layer.edited_image.as_ref().or(layer.image.as_ref()) else {
        paint_source_status(ui, state.locale, rect, &layer);
        return;
    };
    let bounds = rect.shrink(18.0);
    let mut zoom = layer.source_view_zoom;
    let mut center = layer.source_view_center;
    let initial_image_rect = source_image_rect(bounds, image.width, image.height, zoom, center);
    let navigation = handle_source_navigation(
        ui,
        &response,
        bounds,
        initial_image_rect,
        &mut zoom,
        &mut center,
    );
    let image_rect = source_image_rect(bounds, image.width, image.height, zoom, center);
    let texture = adjusted_source_texture_handle(ui, &layer, image);
    ui.painter().image(
        texture.id(),
        image_rect,
        Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    if state.texture_project.mask_preview_enabled
        && let Some(mask_preview) = layer.mask_preview.as_deref()
    {
        let mask_texture = mask_preview_texture_handle(ui, layer.id, mask_preview);
        ui.painter().image(
            mask_texture.id(),
            image_rect,
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    ui.painter().rect_stroke(
        image_rect,
        0.0,
        Stroke::new(1.0, COLOR_BORDER),
        egui::StrokeKind::Inside,
    );

    let header_blocked = ui
        .input(|input| input.pointer.interact_pos())
        .is_some_and(|pointer| pointer.y < crate::viewport::detail_header_band_bottom(state, rect));

    let pin_mode = state.texture_project.active_tool == TextureTool::PinPair;
    if pin_mode && !navigation.panning && !header_blocked {
        handle_source_pins(ui, state, &response, image_rect, &layer);
    }
    if !texture_paint_tool(state.texture_project.active_tool) {
        clear_brush_size_gesture(
            ui.ctx(),
            crate::ui_components::BrushSweeps::TEXTURE_CANVAS.size(),
        );
        clear_brush_size_gesture(
            ui.ctx(),
            crate::ui_components::BrushSweeps::TEXTURE_CANVAS.strength(),
        );
    }
    let brush_size = texture_paint_tool(state.texture_project.active_tool).then(|| {
        handle_brush_size_gesture(
            ui,
            crate::ui_components::BrushSweeps::TEXTURE_CANVAS.size(),
            image_rect,
            state.texture_project.mask_brush_radius,
            TEXTURE_BRUSH_SIZE_SENSITIVITY,
            0.002..=0.25,
        )
    });
    if let Some(radius) = brush_size.and_then(|update| update.radius) {
        state.dispatch(Action::SetTextureMaskBrushRadius(radius));
    }
    // The flat canvas paints with the same brush as the surface does, so it
    // answers to the same two sweeps. It had only ever heard of the first.
    let brush_strength = texture_paint_tool(state.texture_project.active_tool).then(|| {
        crate::ui_components::handle_brush_strength_gesture(
            ui,
            crate::ui_components::BrushSweeps::TEXTURE_CANVAS.strength(),
            image_rect,
            state.texture_project.mask_brush_opacity,
            crate::ui_components::BRUSH_STRENGTH_SENSITIVITY,
            0.01..=1.0,
        )
    });
    if let Some(opacity) = brush_strength.and_then(|update| update.strength) {
        state.dispatch(Action::SetTextureMaskBrushOpacity(opacity));
    }
    let brush_input_blocked = brush_size.is_some_and(|update| update.consumed)
        || brush_strength.is_some_and(|update| update.consumed);
    let texture_actions = if navigation.panning || header_blocked {
        Vec::new()
    } else {
        handle_source_texture_tools(
            ui,
            state,
            &response,
            image_rect,
            &layer,
            brush_input_blocked,
        )
    };
    if pin_mode {
        paint_source_pins(ui, state, image_rect);
    }
    let view_changed = navigation.changed;
    let layer_id = layer.id;
    drop(layer);
    if view_changed {
        state.dispatch(Action::SetTextureSourceView {
            id: layer_id,
            zoom,
            center,
        });
    }
    for action in texture_actions {
        state.dispatch(action);
    }
}

pub fn draw_texture_export_section(ui: &mut Ui, state: &mut AppState) {
    if state.texture_project.layers.is_empty() && state.texture_project.baked.is_none() {
        ui.label(
            egui::RichText::new(text(state.locale, TextKey::NoTextureImage))
                .size(crate::theme::FONT_SM)
                .color(crate::theme::COLOR_MUTED),
        );
        return;
    }

    let mut name = state.texture_project.export_prefix.clone();
    if crate::ui::capsule_metadata_field(
        ui,
        "vkit.texture.export-name",
        &mut name,
        text(state.locale, TextKey::TextureNamePlaceholder),
    ) {
        state.dispatch(Action::SetTextureExportPrefix(name));
    }
    ui.add_space(SPACE_2);

    if texture_bundle_has_material_maps(state) {
        let gloss = state.texture_project.output_pbr == TexturePbrConvention::GlossinessSmoothness;
        let selection = animated_segmented_group(
            ui,
            "vkit.texture.output-pbr",
            2,
            usize::from(gloss),
            |ui, segment_width| {
                let metal = segment_button(
                    ui,
                    segment_width,
                    text(state.locale, TextKey::TextureMetalRough),
                    !gloss,
                );
                let glossy = segment_button(
                    ui,
                    segment_width,
                    text(state.locale, TextKey::TextureGlossSmooth),
                    gloss,
                );
                (metal.clicked(), glossy.clicked())
            },
        );
        if selection.0 {
            state.dispatch(Action::SetTextureOutputPbr(
                TexturePbrConvention::MetallicRoughness,
            ));
        } else if selection.1 {
            state.dispatch(Action::SetTextureOutputPbr(
                TexturePbrConvention::GlossinessSmoothness,
            ));
        }
        ui.add_space(SPACE_2);
    }

    if state.texture_export_bake_pending() {
        let label = if state.texture_project.bake_loading {
            text(state.locale, TextKey::BakingTextures)
        } else {
            text(state.locale, TextKey::BakeTexturesForSave)
        };
        let width = ui.available_width();
        if crate::ui::capsule_action(ui, width, label, !state.texture_project.bake_loading)
            .clicked()
        {
            state.dispatch(Action::RequestTextureBake(TextureBakeQuality::Export));
        }
    } else {
        let width = ui.available_width();
        if crate::ui::capsule_action(
            ui,
            width,
            text(state.locale, TextKey::TextureSaveSection),
            state.can_save_textures(),
        )
        .clicked()
        {
            state.dispatch(Action::SaveTextures);
        }
    }
    ui.add_space(SPACE_3);
}

fn texture_bundle_has_material_maps(state: &AppState) -> bool {
    let governed = |channel: &TextureChannel| {
        !matches!(channel, TextureChannel::Diffuse | TextureChannel::Mask)
    };
    state
        .texture_project
        .baked
        .as_ref()
        .is_some_and(|baked| baked.images.keys().any(governed))
        || state
            .texture_project
            .layers
            .iter()
            .any(|layer| governed(&layer.channel))
}

fn draw_layer_toolbar(ui: &mut Ui, state: &mut AppState) {
    crate::ui::section_heading(ui, text(state.locale, TextKey::TextureLayers));

    if !state.texture_project.layers.is_empty() {
        ui.add_space(SPACE_2);
        // What anyone tuning a decal wants to see is the decal, not the decal
        // on top of whichever VaM skin happens to be loaded. This drops the
        // skin preset and leaves the layers over a plain base -- the same view
        // the skin settings call the layer state -- and puts it back when
        // switched off, including the bake base it had to move out of the way.
        let mut layers_only = state.texture_project.hide_vam_skin_preview;
        if switch_row(
            ui,
            &mut layers_only,
            text(state.locale, TextKey::ShowLayersOnly),
        )
        .changed()
        {
            state.dispatch(Action::SetTextureHideVaMSkin(layers_only));
        }
    }
}

fn draw_layer_list(ui: &mut Ui, state: &mut AppState) {
    let drag_id = Id::new("vkit.texture.layer-drag");

    let rows = ((state.texture_project.layers.len() + 1) as f32).min(TEXTURE_LAYER_LIST_ROWS);
    let height = TEXTURE_LAYER_ROW_HEIGHT * rows + TEXTURE_LAYER_ROW_GAP * rows.max(1.0);
    Frame::new()
        .fill(COLOR_FIELD)
        .corner_radius(CONTROL_RADIUS)
        .inner_margin(Margin::same(4))
        .show(ui, |ui| {
            ScrollArea::vertical()
                .id_salt("vkit.texture.layer-list")
                .scroll_bar_visibility(ScrollBarVisibility::VisibleWhenNeeded)
                .max_height(height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_width(ui.available_width().max(0.0));

                    ui.spacing_mut().item_spacing.y = TEXTURE_LAYER_ROW_GAP;
                    let rows = state.texture_project.layers.clone();
                    for (index, layer) in rows.iter().enumerate() {
                        draw_layer_row(ui, state, layer, index, drag_id);
                    }

                    ui.add_space(ADD_LAYER_EXTRA_GAP);
                    draw_add_layer_slot(ui, state);
                });
        });
    if ui.input(|input| input.pointer.any_released()) {
        ui.data_mut(|data| data.remove::<u64>(drag_id));
        ui.data_mut(|data| {
            data.remove::<bool>(Id::new("vkit.texture.visibility-sweep"));
        });
    }
}

fn draw_layer_row(
    ui: &mut Ui,
    state: &mut AppState,
    layer: &TextureLayer,
    index: usize,
    drag_id: Id,
) {
    let selected = state.texture_project.selected_layer_id == Some(layer.id);
    let (rect, row) = ui.allocate_exact_size(
        vec2(ui.available_width().max(0.0), TEXTURE_LAYER_ROW_HEIGHT),
        Sense::click_and_drag(),
    );
    paint_list_row_highlight(ui, rect, selected, row.hovered());

    let thumb = Rect::from_center_size(
        pos2(
            rect.left() + SPACE_2 + TEXTURE_THUMBNAIL_SIZE * 0.5,
            rect.center().y,
        ),
        Vec2::splat(TEXTURE_THUMBNAIL_SIZE),
    );

    if let Some(paint) = layer.painted.as_ref() {
        paint_projection_thumbnail(
            ui,
            thumb,
            layer.id,
            paint,
            state.texture_project.edit_transaction_active(),
        );
    } else {
        paint_thumbnail(
            ui,
            thumb,
            layer.id,
            layer
                .edited_image
                .as_deref()
                .or(layer.image.as_deref())
                .or_else(|| state.texture_project.baked_layer_raster(layer.id)),
        );
    }

    if let Some(badge) = layer.source_mode.badge() {
        let corner = Rect::from_center_size(
            pos2(thumb.right() - 6.0, thumb.bottom() - 6.0),
            Vec2::splat(13.0),
        );
        ui.painter().circle_filled(corner.center(), 7.5, COLOR_BG);
        paint_icon(ui.painter(), corner, badge, COLOR_TEXT);
    }
    let button_size = 24.0;
    let eye_rect = Rect::from_center_size(
        pos2(thumb.right() + SPACE_2 + button_size * 0.5, rect.center().y),
        Vec2::splat(button_size),
    );

    let _eye = icon_hit(
        ui,
        eye_rect,
        if layer.visible {
            Icon::EyeOpen
        } else {
            Icon::EyeClosed
        },
        "visibility",
        layer.id,
        layer.visible,
    )
    .on_hover_text(text(
        state.locale,
        if layer.visible {
            TextKey::TooltipHide
        } else {
            TextKey::TooltipShow
        },
    ));

    let sweep_id = Id::new("vkit.texture.visibility-sweep");
    let pointer_on_eye = ui
        .input(|input| input.pointer.interact_pos())
        .is_some_and(|pointer| eye_rect.contains(pointer));
    if ui.input(|input| input.pointer.primary_pressed()) && pointer_on_eye {
        let target = !layer.visible;
        ui.data_mut(|data| data.insert_temp(sweep_id, target));
        state.dispatch(Action::SetTextureLayerVisible {
            id: layer.id,
            visible: target,
        });
    } else if let Some(target) = ui.data(|data| data.get_temp::<bool>(sweep_id))
        && ui.input(|input| input.pointer.primary_down())
        && pointer_on_eye
        && layer.visible != target
    {
        state.dispatch(Action::SetTextureLayerVisible {
            id: layer.id,
            visible: target,
        });
    }

    let reset_rect = Rect::from_center_size(
        pos2(rect.right() - SPACE_2 - button_size * 0.5, rect.center().y),
        Vec2::splat(button_size),
    );

    let slider_left = eye_rect.right() + SPACE_2;
    let slider_right = reset_rect.left() - SPACE_2;
    let slider_rect = Rect::from_min_max(
        pos2(slider_left, rect.center().y - CONTROL_H_DENSE * 0.5),
        pos2(
            slider_right.max(slider_left),
            rect.center().y + CONTROL_H_DENSE * 0.5,
        ),
    );

    let mut opacity = if layer.visible { layer.opacity } else { 0.0 };
    let slider = ui
        .put(
            slider_rect,
            FilledNumericSlider::new(&mut opacity, 0.0..=1.0)
                .percent()
                .decimals(0)
                .min_width(slider_rect.width().max(1.0)),
        )
        .on_hover_text(text(state.locale, TextKey::LayerOpacity));
    let dragging = slider.dragged() || slider.drag_stopped() || slider.has_focus();
    if slider.changed() && dragging {
        if opacity > 0.0 {
            if !layer.visible {
                state.dispatch(Action::SetTextureLayerVisible {
                    id: layer.id,
                    visible: true,
                });
            }
            state.dispatch(Action::SetTextureLayerOpacity {
                id: layer.id,
                opacity,
            });
        } else if layer.visible {
            state.dispatch(Action::SetTextureLayerVisible {
                id: layer.id,
                visible: false,
            });
        }
    }
    let reset = icon_hit(ui, reset_rect, Icon::Refresh, "reset", layer.id, false)
        .on_hover_text(text(state.locale, TextKey::ResetSourceRetouch));
    if reset.clicked() {
        state.dispatch(Action::ResetTextureLayer(layer.id));
    }

    let sweeping = ui.data(|data| data.get_temp::<bool>(sweep_id)).is_some();
    let pointer_over_controls =
        sweeping || pointer_on_eye || reset.hovered() || slider.hovered() || slider.dragged();
    if row.clicked() && !pointer_over_controls {
        state.dispatch(Action::SelectTextureLayer(layer.id));
    }
    if row.drag_started() && !pointer_over_controls {
        state.dispatch(Action::SelectTextureLayer(layer.id));
        ui.data_mut(|data| data.insert_temp(drag_id, layer.id));
    }
    let dragged = ui.data(|data| data.get_temp::<u64>(drag_id));
    let pointer = ui.input(|input| input.pointer.interact_pos());
    let pointer_in_row = pointer.is_some_and(|pointer| rect.contains(pointer));
    if dragged.is_some() && pointer_in_row {
        let pointer_y = pointer.map_or(rect.center().y, |pointer| pointer.y);
        let after = pointer_y >= rect.center().y;
        let insertion_index = index + usize::from(after);
        let line_y = if after { rect.bottom() } else { rect.top() };
        ui.painter().line_segment(
            [
                pos2(rect.left() + SPACE_2, line_y),
                pos2(rect.right() - SPACE_2, line_y),
            ],
            Stroke::new(2.0, COLOR_PRIMARY),
        );
        if ui.input(|input| input.pointer.any_released())
            && let Some(id) = dragged
        {
            state.dispatch(Action::MoveTextureLayerTo {
                id,
                insertion_index,
            });
            ui.data_mut(|data| data.remove::<u64>(drag_id));
        }
    }
    if !pointer_over_controls {
        if dragged == Some(layer.id) {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        } else if row.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
    }
}

fn draw_add_layer_slot(ui: &mut Ui, state: &mut AppState) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width().max(0.0), TEXTURE_LAYER_ROW_HEIGHT),
        Sense::hover(),
    );
    let [left, right] = split_row_in_two(rect);

    if add_layer_half(
        ui,
        state,
        left,
        TextKey::AddTextureLayer,
        TextKey::AddTextureLayerTooltip,
        Icon::Picture,
    ) {
        state.dispatch(Action::RequestTextureImageBrowse(
            TextureSourceMode::LandmarkPins,
        ));
    }
    if add_layer_half(
        ui,
        state,
        right,
        TextKey::AddG2UvTextureLayer,
        TextKey::AddG2UvTextureLayerTooltip,
        Icon::HeadTexture,
    ) {
        state.dispatch(Action::RequestTextureImageBrowse(
            TextureSourceMode::MaterialUv,
        ));
    }
}

fn split_row_in_two(rect: Rect) -> [Rect; 2] {
    let middle = rect.center().x;
    [
        Rect::from_min_max(rect.min, pos2(middle, rect.max.y)),
        Rect::from_min_max(pos2(middle, rect.min.y), rect.max),
    ]
}

fn add_layer_half(
    ui: &mut Ui,
    state: &AppState,
    rect: Rect,
    label: TextKey,
    hint: TextKey,
    glyph: Icon,
) -> bool {
    let caption = text(state.locale, label);
    // The tip used to repeat the caption, which tells someone who does not
    // already know the difference between these two buttons exactly nothing.
    // It says what the layer is *for* now.
    let response = crate::ui_components::tooltip(
        ui.interact(rect, ui.id().with(label as u32), Sense::click()),
        text(state.locale, hint),
        None,
    );

    let hovered = response.hovered();
    if hovered {
        ui.painter()
            .rect_filled(rect, CONTROL_RADIUS, COLOR_SURFACE_HOVER);
    }
    let color = if hovered { COLOR_TEXT } else { COLOR_MUTED };
    let font = FontId::proportional(FONT_XS);
    let caption_width = ui
        .painter()
        .layout_no_wrap(caption.to_owned(), font.clone(), color)
        .rect
        .width();
    let group = (ADD_LAYER_ICON + SPACE_1 + caption_width).min(rect.width() - SPACE_1 * 2.0);
    let start = rect.center().x - group * 0.5;

    paint_icon(
        ui.painter(),
        Rect::from_center_size(
            pos2(start + ADD_LAYER_ICON * 0.5, rect.center().y),
            Vec2::splat(ADD_LAYER_ICON),
        ),
        glyph,
        color,
    );
    ui.painter().text(
        pos2(start + ADD_LAYER_ICON + SPACE_1, rect.center().y),
        Align2::LEFT_CENTER,
        caption,
        font,
        color,
    );
    response.clicked()
}

fn draw_selected_layer_controls(ui: &mut Ui, state: &mut AppState, layer: &TextureLayer) {
    ui.scope(|ui| {
        let mut channel = layer.channel;
        let mut blend = layer.blend_mode;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = SPACE_2;

            let combo_width = LAYER_PROPERTY_COMBO_WIDTH;
            crate::ui_components::fit_combo(
                ui,
                ("texture-layer-channel", layer.id),
                combo_width,
                channel_display(channel),
                |ui| {
                    for candidate in TextureChannel::ALL {
                        ui.selectable_value(&mut channel, candidate, channel_display(candidate));
                    }
                },
            );
            crate::ui_components::fit_combo(
                ui,
                ("texture-blend", layer.id),
                combo_width,
                blend_label(state.locale, blend),
                |ui| {
                    for candidate in [
                        TextureBlendMode::Normal,
                        TextureBlendMode::Multiply,
                        TextureBlendMode::Screen,
                        TextureBlendMode::Overlay,
                    ] {
                        ui.selectable_value(
                            &mut blend,
                            candidate,
                            blend_label(state.locale, candidate),
                        );
                    }
                },
            );

            let trailing = crate::ui_components::icon_button_size(ui);
            ui.add_space((ui.available_width() - trailing).max(0.0));
            if icon_button(ui, Icon::Trash, text(state.locale, TextKey::DeleteLayer)).clicked() {
                state.dispatch(Action::RemoveTextureLayer(layer.id));
            }
        });
        if channel != layer.channel {
            state.dispatch(Action::SetTextureLayerChannel {
                id: layer.id,
                channel,
            });
        }
        if blend != layer.blend_mode {
            state.dispatch(Action::SetTextureLayerBlendMode {
                id: layer.id,
                blend_mode: blend,
            });
        }

        if layer.channel == TextureChannel::Normal {
            let mut strength = layer.normal_strength;
            if ui
                .add(
                    FilledNumericSlider::new(&mut strength, 0.0..=3.0)
                        .decimals(2)
                        .min_width(ui.available_width()),
                )
                .on_hover_text(text(state.locale, TextKey::TextureNormalStrength))
                .changed()
            {
                state.dispatch(Action::SetTextureLayerNormalStrength {
                    id: layer.id,
                    strength,
                });
            }
        } else if !layer.channel.is_color() {
            let mut invert = layer.scalar_invert;
            if switch_row(
                ui,
                &mut invert,
                text(state.locale, TextKey::TextureInvertScalar),
            )
            .changed()
            {
                state.dispatch(Action::SetTextureLayerScalarInvert {
                    id: layer.id,
                    invert,
                });
            }
        }

        crate::ui::section_heading(ui, text(state.locale, TextKey::ImageMirror));
        let mut mirror = layer.mirror;
        animated_segmented_group(
            ui,
            "vkit.texture.mirror",
            3,
            match mirror {
                FaceMirror::Off => 0,
                FaceMirror::ToNegativeX => 1,
                FaceMirror::ToPositiveX => 2,
            },
            |ui, segment_width| {
                for (value, key) in [
                    (FaceMirror::Off, TextKey::ImageMirrorOff),
                    (FaceMirror::ToNegativeX, TextKey::ImageMirrorToLeft),
                    (FaceMirror::ToPositiveX, TextKey::ImageMirrorToRight),
                ] {
                    if segment_button(ui, segment_width, text(state.locale, key), mirror == value)
                        .clicked()
                    {
                        mirror = value;
                    }
                }
            },
        );
        if mirror != layer.mirror {
            state.dispatch(Action::SetTextureLayerMirror {
                id: layer.id,
                mirror,
            });
        }

        if layer.channel.is_color() {
            crate::ui::section_heading(ui, text(state.locale, TextKey::ImageAdjustments));
            draw_adjustments(ui, state, layer);
            let has_anchor = state
                .texture_project
                .layers
                .iter()
                .position(|candidate| candidate.id == layer.id)
                .is_some_and(|index| {
                    state.texture_project.layers[index + 1..]
                        .iter()
                        .any(|candidate| {
                            candidate.visible
                                && candidate.channel == TextureChannel::Diffuse
                                && (candidate.image.is_some() || candidate.edited_image.is_some())
                        })
                });
            if has_anchor
                && ui
                    .add_sized(
                        [ui.available_width(), CONTROL_H_DENSE],
                        Button::new(text(state.locale, TextKey::MatchToneToLayerBelow))
                            .corner_radius(CAPSULE_RADIUS),
                    )
                    .clicked()
            {
                state.dispatch(Action::MatchTextureLayerColor(layer.id));
            }
        }
        if matches!(
            state.texture_project.active_tool,
            TextureTool::MaskBrush
                | TextureTool::CloneStamp
                | TextureTool::DodgeBurn
                | TextureTool::Sponge
        ) || layer.mask.is_some()
            || layer.edited_image.is_some()
        {
            draw_mask_brush_controls(ui, state, layer);
        }
    });
}

fn draw_adjustments(ui: &mut Ui, state: &mut AppState, layer: &TextureLayer) {
    let mut adjustments = layer.adjustments;
    let original = adjustments;
    let reset = text(state.locale, TextKey::Reset);
    let mut dragging = false;
    for (label, value, range) in [
        (TextKey::Exposure, &mut adjustments.exposure, -3.0..=3.0_f32),
        (TextKey::Contrast, &mut adjustments.contrast, -1.0..=1.0),
        (TextKey::Saturation, &mut adjustments.saturation, -1.0..=1.0),
        (TextKey::Hue, &mut adjustments.hue_degrees, -180.0..=180.0),
        (
            TextKey::Temperature,
            &mut adjustments.temperature,
            -1.0..=1.0,
        ),
    ] {
        dragging |= correction_slider(ui, text(state.locale, label), value, range, 0.0, reset);
    }
    if adjustments != original {
        state.dispatch(Action::SetTextureLayerAdjustments {
            id: layer.id,
            adjustments,
        });
    }

    let drag_id = Id::new(("vkit.texture.adjust-drag", layer.id));
    let was_dragging = ui
        .data(|data| data.get_temp::<bool>(drag_id))
        .unwrap_or(false);
    if dragging != was_dragging {
        state.dispatch(if dragging {
            Action::BeginTextureEdit
        } else {
            Action::EndTextureEdit
        });
        ui.data_mut(|data| data.insert_temp(drag_id, dragging));
    }
}

fn draw_mask_brush_controls(ui: &mut Ui, state: &mut AppState, layer: &TextureLayer) {
    if layer.mask.is_some()
        && ui
            .add_sized(
                [ui.available_width(), CONTROL_H_DENSE],
                Button::new(text(state.locale, TextKey::ClearLayerMask))
                    .corner_radius(CAPSULE_RADIUS),
            )
            .clicked()
    {
        state.dispatch(Action::ClearTextureLayerMask(layer.id));
    }

    if state.texture_project.active_tool == TextureTool::CloneStamp
        && state.texture_project.clone_sample.is_none()
    {
        ui.label(
            RichText::new(text(state.locale, TextKey::CloneSampleHint))
                .size(FONT_XS)
                .color(COLOR_MUTED),
        );
    }
}

fn draw_bake_controls(ui: &mut Ui, state: &mut AppState) {
    crate::ui::section_heading(ui, text(state.locale, TextKey::BakeBase));
    ui.add_space(SPACE_2);

    let skin_selected = state.texture_project.bake_base == TextureBakeBase::CurrentSkin;
    let selection = animated_segmented_group(
        ui,
        "vkit.texture.bake-base",
        2,
        usize::from(skin_selected),
        |ui, segment_width| {
            let face_only = segment_button(
                ui,
                segment_width,
                text(state.locale, TextKey::TransparentOverlay),
                !skin_selected,
            )
            .on_hover_text(text(state.locale, TextKey::TransparentOverlayTooltip));
            let with_skin = segment_button(
                ui,
                segment_width,
                text(state.locale, TextKey::CurrentSkinBake),
                skin_selected,
            )
            .on_hover_text(text(state.locale, TextKey::CurrentSkinBakeTooltip));
            (face_only.clicked(), with_skin.clicked())
        },
    );
    if selection.0 {
        state.dispatch(Action::SetTextureBakeBase(TextureBakeBase::Transparent));
    } else if selection.1 {
        state.dispatch(Action::SetTextureBakeBase(TextureBakeBase::CurrentSkin));
    }

    let mut resolution = state.texture_project.resolution;
    let resolution_width = ui.available_width();
    let resolution_selected = format!("{resolution} × {resolution}");
    crate::ui_components::fit_combo(
        ui,
        "texture-bake-resolution",
        resolution_width,
        &resolution_selected,
        |ui| {
            for candidate in crate::texture_project::TEXTURE_RESOLUTIONS {
                ui.selectable_value(
                    &mut resolution,
                    candidate,
                    format!("{candidate} × {candidate}"),
                );
            }
        },
    );
    if resolution != state.texture_project.resolution {
        state.dispatch(Action::SetTextureResolution(resolution));
    }

    let feather_max_pixels = f32::from(state.texture_project.max_boundary_feather_pixels());
    let mut feather =
        f32::from(state.texture_project.boundary_feather_pixels) / feather_max_pixels.max(1.0);
    ui.horizontal(|ui| {
        let label_width = (ui.available_width() * 0.34).clamp(64.0, 96.0);
        let label = ui.add_sized(
            [label_width, CONTROL_H_DENSE],
            egui::Label::new(
                RichText::new(text(state.locale, TextKey::BoundaryFeather))
                    .size(FONT_XS)
                    .color(COLOR_MUTED),
            ),
        );

        crate::ui_components::tooltip(
            label,
            text(state.locale, TextKey::BoundaryFeatherTooltip),
            None,
        );
        ui.add(
            FilledNumericSlider::new(&mut feather, 0.0..=1.0)
                .percent()
                .decimals(0)
                .min_width(ui.available_width()),
        );
    });
    let feather = (feather.clamp(0.0, 1.0) * feather_max_pixels).round() as u16;
    if feather != state.texture_project.boundary_feather_pixels {
        state.dispatch(Action::SetTextureBoundaryFeather(feather));
    }

    if state.texture_project.bake_loading {
        ui.label(
            RichText::new(text(state.locale, TextKey::Baking))
                .size(FONT_XS)
                .color(COLOR_MUTED),
        );
    }
    if let Some(error) = state.texture_project.bake_error.as_deref() {
        ui.label(
            RichText::new(error)
                .size(FONT_XS)
                .color(crate::theme::COLOR_DESTRUCTIVE),
        );
    }
}

pub(crate) fn schedule_texture_auto_bake(ui: &Ui, state: &mut AppState) {
    let id = Id::new("vkit.texture.auto-bake");
    if !state.texture_project.dirty || !state.texture_auto_bake_ready() {
        ui.data_mut(|data| data.remove::<TextureAutoBakeTimer>(id));

        if state.texture_project.dirty {
            state.settle_empty_texture_composite();
        }
        return;
    }

    let revision = state.texture_project.edit_revision();
    let now = ui.input(|input| input.time);
    let mid_stroke = state.texture_project.edit_transaction_active();
    // A stroke in motion never bakes either way: every new dab pushes the deadline out. What
    // differs is what happens when the pointer holds still with the button down. A paint brush
    // falls through to the ready check, so a stroke that pauses for the debounce catches the
    // preview up mid-drag — Dodge/Burn and Sponge have no live overlay anywhere and are otherwise
    // invisible until the button comes up. Pin drags and stencil placement, whose whole gesture is
    // one continuous adjustment, stay deferred until the transaction closes.
    if mid_stroke && !state.texture_project.active_tool.is_paint_brush() {
        let timer = TextureAutoBakeTimer {
            revision,
            ready_at: now + TEXTURE_AUTO_BAKE_DEBOUNCE.as_secs_f64(),
            requested: false,
        };
        ui.data_mut(|data| data.insert_temp(id, timer));
        ui.ctx().request_repaint_after(TEXTURE_AUTO_BAKE_DEBOUNCE);
        return;
    }
    let timer = ui.data(|data| data.get_temp::<TextureAutoBakeTimer>(id));
    let timer = match timer {
        Some(timer) if timer.revision == revision => timer,
        previous => {
            let debounced = now + TEXTURE_AUTO_BAKE_DEBOUNCE.as_secs_f64();
            let timer = TextureAutoBakeTimer {
                revision,
                // Mid-stroke each dab pushes the deadline out, so only a pause bakes. Between
                // strokes the earliest pending deadline wins instead, so a run of edits cannot
                // postpone the preview indefinitely.
                ready_at: if mid_stroke {
                    debounced
                } else {
                    previous.map_or(debounced, |timer| timer.ready_at.min(debounced))
                },
                requested: false,
            };
            ui.data_mut(|data| data.insert_temp(id, timer));
            timer
        }
    };
    if timer.requested {
        return;
    }
    if state.texture_project.bake_loading {
        ui.ctx().request_repaint_after(TEXTURE_AUTO_BAKE_DEBOUNCE);
    } else if now >= timer.ready_at {
        ui.data_mut(|data| {
            data.insert_temp(
                id,
                TextureAutoBakeTimer {
                    requested: true,
                    ..timer
                },
            )
        });
        state.dispatch(Action::RequestTextureBake(TextureBakeQuality::Preview));
    } else {
        ui.ctx()
            .request_repaint_after(Duration::from_secs_f64((timer.ready_at - now).max(0.001)));
    }
}

pub(crate) fn draw_paint_header_content(ui: &mut Ui, state: &mut AppState, content: Rect) {
    let mut row = ui.new_child(
        UiBuilder::new()
            .id_salt("vkit.detail.header.paint")
            .max_rect(content)
            .layout(Layout::left_to_right(Align::Center)),
    );
    row.spacing_mut().item_spacing.x = SPACE_2;

    let tools = state
        .texture_project
        .selected_layer()
        .map_or(TextureSourceMode::default().available_tools(), |layer| {
            layer.source_mode.available_tools()
        });
    for (index, tool) in tools.iter().copied().enumerate() {
        let (cell, response) =
            row.allocate_exact_size(Vec2::splat(CONTROL_H_DENSE), Sense::click());
        let response = crate::ui_components::tooltip(
            response,
            text(state.locale, texture_tool_text_key(tool)),
            texture_tool_shortcut(tool).map(Shortcut::label),
        );
        let active = state.texture_project.active_tool == tool;
        if active || response.hovered() {
            row.painter().rect_filled(
                cell,
                CONTROL_RADIUS,
                if active {
                    Color32::WHITE
                } else {
                    COLOR_SURFACE_RAISED
                },
            );
        }
        paint_icon(
            row.painter(),
            cell.shrink(6.0),
            tool_icon(tool),
            if active { COLOR_BG } else { COLOR_TEXT },
        );
        if response.clicked() {
            state.dispatch(Action::SetTextureTool(tool));
        }
        let _ = index;
    }

    let (sep, _) = row.allocate_exact_size(vec2(SPACE_2 + 1.0, CONTROL_H_DENSE), Sense::hover());
    row.painter().vline(
        sep.center().x,
        egui::Rangef::new(sep.top() + 4.0, sep.bottom() - 4.0),
        Stroke::new(1.0, COLOR_BORDER),
    );
    texture_brush_controls(&mut row, state);
}

fn texture_brush_controls(hud: &mut Ui, state: &mut AppState) {
    hud.spacing_mut().item_spacing.x = SPACE_2;
    let icon_width = CONTROL_H_DENSE;
    let active_tool = state.texture_project.active_tool;
    if active_tool == TextureTool::Projection {
        let (icon_rect, _) = hud.allocate_exact_size(Vec2::splat(CONTROL_H_DENSE), Sense::hover());
        paint_icon(
            hud.painter(),
            icon_rect.shrink(5.0),
            tool_icon(active_tool),
            COLOR_TEXT,
        );

        let numeric_width = (hud.available_width() * 0.28).clamp(0.0, TEXTURE_BRUSH_CONTROL_WIDTH);
        let mut radius = state.texture_project.mask_brush_radius;
        if compact_brush_numeric_control(
            hud,
            numeric_width,
            text(state.locale, TextKey::Size),
            &mut radius,
            0.002..=0.25,
            1,
            Some(crate::shortcuts::BRUSH_SIZE_HINT),
        )
        .changed()
        {
            state.dispatch(Action::SetTextureMaskBrushRadius(radius));
        }
        let mut strength = state.texture_project.mask_brush_opacity;
        if compact_brush_numeric_control(
            hud,
            numeric_width,
            text(state.locale, TextKey::Strength),
            &mut strength,
            0.01..=1.0,
            0,
            Some(crate::shortcuts::BRUSH_STRENGTH_HINT),
        )
        .changed()
        {
            state.dispatch(Action::SetTextureMaskBrushOpacity(strength));
        }
        if let Some(falloff) = brush_falloff_selector(
            hud,
            Id::new("vkit.texture.stencil.falloff"),
            state.locale,
            state.texture_project.mask_brush_falloff,
            true,
        ) {
            state.dispatch(Action::SetTextureMaskBrushFalloff(falloff));
        }

        if hud
            .add(
                Button::new(text(state.locale, TextKey::ProjectDone))
                    .corner_radius(CAPSULE_RADIUS)
                    .min_size(vec2(0.0, CONTROL_H_DENSE)),
            )
            .on_hover_text(text(state.locale, TextKey::ProjectDoneTooltip))
            .clicked()
        {
            state.dispatch(Action::SetTextureProjectionStencil(false));
        }
        return;
    }
    if active_tool == TextureTool::PinPair {
        let (icon_rect, _) = hud.allocate_exact_size(Vec2::splat(CONTROL_H_DENSE), Sense::hover());
        paint_icon(
            hud.painter(),
            icon_rect.shrink(5.0),
            tool_icon(active_tool),
            COLOR_TEXT,
        );

        let control_width = hud
            .available_width()
            .clamp(0.0, TEXTURE_BRUSH_CONTROL_WIDTH + 32.0);
        let mut opacity = state.texture_project.pin_opacity;
        if compact_brush_numeric_control(
            hud,
            control_width,
            text(state.locale, TextKey::PinOpacity),
            &mut opacity,
            0.0..=1.0,
            0,
            None,
        )
        .changed()
        {
            state.dispatch(Action::SetTexturePinOpacity(opacity));
        }

        if state.can_broadcast_texture_pins() {
            let transfer = hud
                .add(
                    Button::new(text(state.locale, TextKey::TransferPins))
                        .corner_radius(CAPSULE_RADIUS)
                        .min_size(vec2(0.0, CONTROL_H_DENSE)),
                )
                .on_hover_text(text(state.locale, TextKey::TransferPinsTooltip));
            if transfer.clicked() {
                state.broadcast_texture_pins();
            }
        }
        return;
    }
    let mask_toggle_width = if active_tool == TextureTool::MaskBrush {
        CONTROL_H_DENSE + SPACE_2
    } else {
        0.0
    };
    let numeric_width = ((hud.available_width()
        - icon_width
        - BRUSH_FALLOFF_COMPACT_WIDTH
        - mask_toggle_width
        - SPACE_2 * 3.0)
        * 0.5)
        .clamp(0.0, TEXTURE_BRUSH_CONTROL_WIDTH);
    let (icon_rect, _) = hud.allocate_exact_size(Vec2::splat(CONTROL_H_DENSE), Sense::hover());
    paint_icon(
        hud.painter(),
        icon_rect.shrink(5.0),
        tool_icon(state.texture_project.active_tool),
        COLOR_TEXT,
    );
    let mut radius = state.texture_project.mask_brush_radius;
    if compact_brush_numeric_control(
        hud,
        numeric_width,
        text(state.locale, TextKey::Size),
        &mut radius,
        0.002..=0.25,
        1,
        Some(crate::shortcuts::BRUSH_SIZE_HINT),
    )
    .changed()
    {
        state.dispatch(Action::SetTextureMaskBrushRadius(radius));
    }
    let mut opacity = state.texture_project.mask_brush_opacity;
    if compact_brush_numeric_control(
        hud,
        numeric_width,
        text(state.locale, TextKey::Strength),
        &mut opacity,
        0.01..=1.0,
        0,
        Some(crate::shortcuts::BRUSH_STRENGTH_HINT),
    )
    .changed()
    {
        state.dispatch(Action::SetTextureMaskBrushOpacity(opacity));
    }
    if let Some(falloff) = brush_falloff_selector(
        hud,
        Id::new("vkit.texture.brush.falloff"),
        state.locale,
        state.texture_project.mask_brush_falloff,
        true,
    ) {
        state.dispatch(Action::SetTextureMaskBrushFalloff(falloff));
    }
    if active_tool == TextureTool::MaskBrush {
        let enabled = state.texture_project.mask_preview_enabled;
        let preview = texture_hud_toggle_icon(
            hud,
            Icon::BackfaceProtection,
            enabled,
            text(state.locale, TextKey::TextureMaskPreviewTooltip),
        );
        if preview.clicked() {
            state.dispatch(Action::SetTextureMaskPreviewEnabled(!enabled));
        }
    }

    let directions = match active_tool {
        TextureTool::DodgeBurn => Some((TextKey::BrushDodge, TextKey::BrushBurn)),
        TextureTool::Sponge => Some((TextKey::BrushSaturate, TextKey::BrushDesaturate)),
        _ => None,
    };
    if let Some((forward, inverse)) = directions {
        let alt = hud.input(|input| input.modifiers.alt);
        let reverse = state.texture_project.retouch_reverse ^ alt;
        let (rect, _) = hud.allocate_exact_size(
            vec2(TEXTURE_RETOUCH_DIRECTION_WIDTH, CONTROL_H_DENSE),
            Sense::hover(),
        );
        let label = text(state.locale, if reverse { inverse } else { forward });
        if crate::ui::island_capsule_button(hud, rect, label, false).clicked() {
            state.dispatch(Action::SetTextureRetouchReverse(!reverse ^ alt));
        }
    }
}

fn texture_hud_toggle_icon(hud: &mut Ui, icon: Icon, active: bool, tooltip: &str) -> Response {
    let (rect, response) = hud.allocate_exact_size(Vec2::splat(CONTROL_H_DENSE), Sense::click());
    let response = response.on_hover_text(tooltip);
    let radius = rect.height() * 0.5;
    if active {
        hud.painter()
            .rect_filled(rect, radius, crate::theme::COLOR_ACTIVE_BG);
    } else if response.hovered() {
        hud.painter()
            .rect_filled(rect, radius, COLOR_SURFACE_RAISED);
    }
    paint_icon(
        hud.painter(),
        rect.shrink(3.5),
        icon,
        if active {
            crate::theme::COLOR_ACTIVE_INK
        } else if response.hovered() {
            COLOR_TEXT
        } else {
            crate::theme::disabled(COLOR_MUTED)
        },
    );
    control_affordances(hud, &response, rect, radius);
    response
}

const fn texture_paint_tool(tool: TextureTool) -> bool {
    tool.is_paint_brush()
}

fn handle_source_pins(
    ui: &Ui,
    state: &mut AppState,
    response: &Response,
    image_rect: Rect,
    layer: &TextureLayer,
) {
    if layer.source_mode != TextureSourceMode::LandmarkPins
        || state.texture_project.active_tool != TextureTool::PinPair
    {
        return;
    }
    let drag_id = Id::new("vkit.texture.source-pin-drag");
    let pointer = ui.input(|input| input.pointer.interact_pos());
    let nearest = pointer.and_then(|pointer| nearest_source_pin(layer, image_rect, pointer));
    if ui.input(|input| input.pointer.button_pressed(PointerButton::Primary))
        && response.hovered()
        && let Some(index) = nearest
    {
        state.dispatch(Action::BeginTextureEdit);
        ui.data_mut(|data| data.insert_temp(drag_id, index));
    }
    if ui.input(|input| input.pointer.button_down(PointerButton::Primary))
        && let Some(index) = ui.data(|data| data.get_temp::<usize>(drag_id))
        && let Some(pointer) = pointer
    {
        state.move_texture_source_pin(index, normalized_image_point(image_rect, pointer));
        ui.ctx().request_repaint();
    }
    if ui.input(|input| input.pointer.button_released(PointerButton::Primary)) {
        ui.data_mut(|data| data.remove::<usize>(drag_id));
        state.dispatch(Action::EndTextureEdit);
    }
    if response.clicked_by(PointerButton::Primary)
        && nearest.is_none()
        && let Some(pointer) = pointer
        && image_rect.contains(pointer)
    {
        state.add_texture_source_pin(normalized_image_point(image_rect, pointer));
    }
    if response.clicked_by(PointerButton::Secondary)
        && let Some(index) = nearest
    {
        state.remove_texture_pin(index);
    }
}

fn handle_source_texture_tools(
    ui: &Ui,
    state: &mut AppState,
    response: &Response,
    image_rect: Rect,
    layer: &TextureLayer,
    brush_input_blocked: bool,
) -> Vec<Action> {
    let tool = state.texture_project.active_tool;
    if !matches!(
        tool,
        TextureTool::MaskBrush
            | TextureTool::CloneStamp
            | TextureTool::DodgeBurn
            | TextureTool::Sponge
    ) {
        return Vec::new();
    }

    let pointer = ui.input(|input| input.pointer.interact_pos());
    let cursor = crate::ui_components::brush_cursor(
        ui,
        pointer,
        crate::ui_components::BrushSweeps::TEXTURE_CANVAS.size(),
        Some((
            crate::ui_components::BrushSweeps::TEXTURE_CANVAS.strength(),
            state.texture_project.mask_brush_opacity,
        )),
    );
    let alt = ui.input(|input| input.modifiers.alt);
    if let Some(cursor) = cursor
        && image_rect.contains(cursor.at)
    {
        let radius =
            state.texture_project.mask_brush_radius * image_rect.width().min(image_rect.height());
        let color = if alt && tool.alt_inverts() {
            crate::theme::COLOR_DESTRUCTIVE
        } else {
            Color32::WHITE
        };
        crate::ui_components::paint_brush_cursor(ui.painter(), cursor, radius.max(2.0), color);
    }
    if matches!(tool, TextureTool::CloneStamp)
        && let Some(sample) = state.texture_project.clone_sample
    {
        let center = pos2(
            image_rect.left() + sample[0] * image_rect.width(),
            image_rect.top() + sample[1] * image_rect.height(),
        );
        crate::ui_components::paint_clone_anchor(ui.painter(), center);
    }
    if brush_input_blocked {
        if state.texture_project.edit_transaction_active() {
            state.dispatch(Action::EndTextureEdit);
        }
        return Vec::new();
    }
    let down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));
    let pressed = ui.input(|input| input.pointer.button_pressed(PointerButton::Primary));
    let released = ui.input(|input| input.pointer.button_released(PointerButton::Primary));
    let drag_id = Id::new(("vkit.texture.paint-drag", layer.id, tool as u8));
    if released {
        ui.data_mut(|data| data.remove::<Pos2>(drag_id));
        state.dispatch(Action::EndTextureEdit);
        return Vec::new();
    }
    let Some(pointer) = pointer.filter(|point| image_rect.contains(*point)) else {
        return Vec::new();
    };
    if !down || !response.hovered() {
        return Vec::new();
    }
    if !state.texture_project.edit_transaction_active() {
        state.dispatch(Action::BeginTextureEdit);
    }
    let source = normalized_image_point(image_rect, pointer);
    if matches!(tool, TextureTool::CloneStamp) && alt && pressed {
        ui.data_mut(|data| data.remove::<Pos2>(drag_id));
        return vec![Action::SetTextureCloneSample(source)];
    }
    if matches!(tool, TextureTool::CloneStamp) && alt {
        return Vec::new();
    }
    let spacing = (state.texture_project.mask_brush_radius
        * image_rect.width().min(image_rect.height())
        * BRUSH_SPACING_FRACTION)
        .max(1.0);

    let stroke_points = brush_stroke_points(
        ui.data(|data| data.get_temp::<Pos2>(drag_id)),
        pointer,
        spacing,
    );
    let Some(&last) = stroke_points.last() else {
        return Vec::new();
    };
    let warp_pins = (tool == TextureTool::MaskBrush
        && layer.source_mode == TextureSourceMode::LandmarkPins)
        .then(|| {
            layer
                .pins
                .iter()
                .filter_map(|pair| {
                    pair.source
                        .zip(pair.target)
                        .map(|(source, target)| TextureWarpPin {
                            source,
                            target_uv: target.uv,
                        })
                })
                .collect::<Vec<_>>()
        });
    let reverse = alt ^ state.texture_project.retouch_reverse;
    let actions = stroke_points
        .iter()
        .filter_map(|point| {
            let source = normalized_image_point(image_rect, *point);
            if tool == TextureTool::MaskBrush {
                let uv = mask_uv_for_canvas_point(layer, warp_pins.as_deref(), source)?;
                Some(Action::AddTextureMaskDab {
                    id: layer.id,
                    uv,
                    source: Some(source),

                    subtract: layer.mask_stroke_subtracts(alt),
                })
            } else {
                Some(Action::AddTextureRetouchDab {
                    id: layer.id,
                    point: source,
                    tool,
                    reverse,
                })
            }
        })
        .collect::<Vec<_>>();
    if !actions.is_empty() {
        ui.data_mut(|data| data.insert_temp(drag_id, last));
        ui.ctx().request_repaint();
    }
    actions
}

/// Where a point on the 2D canvas lands in G2 UV space.
///
/// A G2-space layer shows the flat atlas itself — a `MaterialUv` layer always, a
/// `ScanMesh` layer once `adopt_scan_atlases` hands it the transferred atlas — so the
/// canvas *is* the UV square and only the vertical flip stands between them. A
/// landmark layer shows the photograph, which reaches UV only through its pin warp.
fn mask_uv_for_canvas_point(
    layer: &TextureLayer,
    warp_pins: Option<&[TextureWarpPin]>,
    source: [f32; 2],
) -> Option<[f32; 2]> {
    match layer.source_mode {
        TextureSourceMode::LandmarkPins => {
            map_source_point_to_g2(warp_pins?, source).ok().flatten()
        }
        TextureSourceMode::MaterialUv | TextureSourceMode::ScanMesh => {
            Some([source[0], 1.0 - source[1]])
        }
    }
}

pub(crate) fn brush_stroke_points(
    previous: Option<Pos2>,
    pointer: Pos2,
    spacing: f32,
) -> Vec<Pos2> {
    const MAX_STEPS: usize = 256;
    let spacing = spacing.max(0.5);
    let Some(previous) = previous else {
        return vec![pointer];
    };
    let delta = pointer - previous;
    let distance = delta.length();
    if distance < spacing {
        return Vec::new();
    }
    let steps = ((distance / spacing).floor() as usize).min(MAX_STEPS);
    let direction = delta / distance;
    (1..=steps)
        .map(|step| previous + direction * (spacing * step as f32))
        .collect()
}

fn paint_source_pins(ui: &Ui, state: &AppState, image_rect: Rect) {
    let Some(layer) = state.texture_project.selected_layer() else {
        return;
    };
    for (index, pair) in layer.pins.iter().enumerate() {
        let Some(point) = pair.source else {
            continue;
        };
        let center = pos2(
            image_rect.left() + point[0] * image_rect.width(),
            image_rect.top() + point[1] * image_rect.height(),
        );
        let invalid = layer.pin_pair_invalid(index);
        paint_texture_pin(
            ui.painter(),
            center,
            state.texture_project.pin_opacity,
            &(index + 1).to_string(),
            invalid,
        );
    }
}

fn nearest_source_pin(layer: &TextureLayer, image_rect: Rect, pointer: Pos2) -> Option<usize> {
    layer
        .pins
        .iter()
        .enumerate()
        .filter_map(|(index, pair)| {
            let point = pair.source?;
            let screen = pos2(
                image_rect.left() + point[0] * image_rect.width(),
                image_rect.top() + point[1] * image_rect.height(),
            );
            let distance = screen.distance(pointer);
            (distance <= TEXTURE_PIN_HIT_RADIUS).then_some((distance, index))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, index)| index)
}

fn normalized_image_point(rect: Rect, point: Pos2) -> [f32; 2] {
    [
        ((point.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
        ((point.y - rect.top()) / rect.height()).clamp(0.0, 1.0),
    ]
}

#[derive(Clone, Copy, Debug, Default)]
struct SourceNavigation {
    changed: bool,
    panning: bool,
}

fn fitted_image_size(bounds: Rect, width: u32, height: u32) -> Vec2 {
    let aspect = width as f32 / height.max(1) as f32;
    let bounds_aspect = bounds.width() / bounds.height().max(1.0);
    if aspect >= bounds_aspect {
        vec2(bounds.width(), bounds.width() / aspect)
    } else {
        vec2(bounds.height() * aspect, bounds.height())
    }
}

fn source_image_rect(bounds: Rect, width: u32, height: u32, zoom: f32, center: [f32; 2]) -> Rect {
    let size = fitted_image_size(bounds, width, height) * zoom.clamp(1.0, 32.0);
    let min = bounds.center() - vec2(center[0] * size.x, center[1] * size.y);
    Rect::from_min_size(min, size)
}

fn clamp_source_center(bounds: Rect, image_size: Vec2, center: &mut [f32; 2]) {
    for (axis, (bounds_edge, image_edge)) in [
        (0, (bounds.width(), image_size.x)),
        (1, (bounds.height(), image_size.y)),
    ] {
        center[axis] = if image_edge <= bounds_edge {
            0.5
        } else {
            let half_visible = (bounds_edge * 0.5 / image_edge).clamp(0.0, 0.5);
            center[axis].clamp(half_visible, 1.0 - half_visible)
        };
    }
}

fn handle_source_navigation(
    ui: &Ui,
    response: &Response,
    bounds: Rect,
    image_rect: Rect,
    zoom: &mut f32,
    center: &mut [f32; 2],
) -> SourceNavigation {
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let hovered = pointer.is_some_and(|pointer| bounds.contains(pointer));
    let middle_down = ui.input(|input| input.pointer.button_down(PointerButton::Middle));
    let primary_down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));
    let space_down = ui.input(|input| input.key_down(Key::Space));
    let panning = response.dragged_by(PointerButton::Middle)
        || (space_down && response.dragged_by(PointerButton::Primary))
        || (hovered && (middle_down || (space_down && primary_down)));
    if panning || (hovered && space_down) {
        ui.ctx().set_cursor_icon(if panning {
            CursorIcon::Grabbing
        } else {
            CursorIcon::Grab
        });
    }

    let mut changed = false;
    if panning {
        let delta = ui.input(|input| input.pointer.delta());
        if delta != Vec2::ZERO {
            center[0] -= delta.x / image_rect.width().max(1.0);
            center[1] -= delta.y / image_rect.height().max(1.0);
            clamp_source_center(bounds, image_rect.size(), center);
            changed = true;
        }
    }

    let scroll = if hovered {
        ui.input(|input| input.smooth_scroll_delta.y)
    } else {
        0.0
    };
    if scroll.abs() > f32::EPSILON
        && let Some(pointer) = pointer
    {
        changed |= apply_source_zoom(bounds, image_rect, pointer, scroll, zoom, center);
    }
    if response.double_clicked_by(PointerButton::Middle) {
        *zoom = 1.0;
        *center = [0.5, 0.5];
        changed = true;
    }
    SourceNavigation { changed, panning }
}

fn apply_source_zoom(
    bounds: Rect,
    image_rect: Rect,
    pointer: Pos2,
    scroll: f32,
    zoom: &mut f32,
    center: &mut [f32; 2],
) -> bool {
    let source = [
        (pointer.x - image_rect.left()) / image_rect.width().max(1.0),
        (pointer.y - image_rect.top()) / image_rect.height().max(1.0),
    ];
    let old_zoom = *zoom;
    let new_zoom = (old_zoom * (scroll * 0.0025).exp()).clamp(1.0, 32.0);
    if (new_zoom - old_zoom).abs() <= f32::EPSILON {
        return false;
    }
    *zoom = new_zoom;

    let new_size = image_rect.size() * (new_zoom / old_zoom.max(1.0));
    center[0] = (bounds.center().x - pointer.x + source[0] * new_size.x) / new_size.x.max(1.0);
    center[1] = (bounds.center().y - pointer.y + source[1] * new_size.y) / new_size.y.max(1.0);
    clamp_source_center(bounds, new_size, center);
    true
}

fn draw_projection_canvas(
    ui: &mut Ui,
    state: &mut AppState,
    rect: Rect,
    response: &Response,
    layer: &crate::texture_project::TextureLayer,
) {
    let bounds = rect.shrink(18.0);
    let edge = bounds.width().min(bounds.height()).max(1.0);
    let canvas = Rect::from_center_size(bounds.center(), Vec2::splat(edge));
    ui.painter().rect_filled(canvas, 0.0, COLOR_BG);
    if let Some(paint) = layer.painted.as_ref() {
        let texture = projection_canvas_texture_handle(ui, layer, paint);
        ui.painter().image(
            texture.id(),
            canvas,
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if layer.mirror != FaceMirror::Off {
            ui.painter().image(
                texture.id(),
                canvas,
                Rect::from_min_max(pos2(1.0, 0.0), pos2(0.0, 1.0)),
                Color32::WHITE,
            );
        }
    }

    if state.texture_project.mask_preview_enabled
        && let Some(mask_preview) = layer.mask_preview.as_deref()
    {
        let mask_texture = mask_preview_texture_handle(ui, layer.id, mask_preview);
        ui.painter().image(
            mask_texture.id(),
            canvas,
            Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    let previewed = paint_stencil_projection_preview(ui, state, canvas, layer);
    ui.painter().rect_stroke(
        canvas,
        0.0,
        Stroke::new(1.0, COLOR_BORDER),
        egui::StrokeKind::Inside,
    );
    if layer.painted.is_none() && !previewed {
        ui.painter().text(
            canvas.center(),
            Align2::CENTER_CENTER,
            text(state.locale, TextKey::ProjectionCanvasHint),
            FontId::proportional(FONT_SM),
            COLOR_MUTED,
        );
    }
    handle_projection_canvas_tools(ui, state, response, canvas, layer);
}

fn paint_stencil_projection_preview(
    ui: &Ui,
    state: &AppState,
    canvas: Rect,
    layer: &crate::texture_project::TextureLayer,
) -> bool {
    if !state.texture_project.projection_stencil {
        crate::viewport::forget_stencil_projection(ui);
        return false;
    }
    let Some(projection) = crate::viewport::stencil_projection(ui) else {
        return false;
    };
    let Some(image) = layer.edited_image.as_ref().or(layer.image.as_ref()) else {
        return false;
    };
    let texture = adjusted_source_texture_handle(ui, layer, image);
    let centre = [projection.stencil.center().x, projection.stencil.center().y];
    let size = [projection.stencil.width(), projection.stencil.height()];
    let tint =
        Color32::WHITE.gamma_multiply(state.texture_project.projection_opacity.clamp(0.0, 1.0));

    let mut mesh = egui::epaint::Mesh::with_texture(texture.id());
    for triangle in projection.triangles.iter() {
        let mut corners = [egui::epaint::Vertex {
            pos: Pos2::ZERO,
            uv: Pos2::ZERO,
            color: tint,
        }; 3];
        let mut inside = true;
        for (corner, vertex) in corners.iter_mut().enumerate() {
            let Some(source) =
                projection
                    .placement
                    .source_at(triangle.screen[corner], centre, size)
            else {
                inside = false;
                break;
            };
            *vertex = egui::epaint::Vertex {
                pos: pos2(
                    canvas.left() + triangle.uv[corner][0] * canvas.width(),
                    canvas.top() + (1.0 - triangle.uv[corner][1]) * canvas.height(),
                ),
                uv: pos2(source[0], source[1]),
                color: tint,
            };
        }
        if !inside {
            continue;
        }
        let base = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
        mesh.vertices.extend_from_slice(&corners);
        mesh.add_triangle(base, base + 1, base + 2);
    }
    if mesh.is_empty() {
        return false;
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    true
}

fn handle_projection_canvas_tools(
    ui: &Ui,
    state: &mut AppState,
    response: &Response,
    canvas: Rect,
    layer: &crate::texture_project::TextureLayer,
) {
    let tool = state.texture_project.active_tool;
    let editing = matches!(
        tool,
        TextureTool::MaskBrush
            | TextureTool::CloneStamp
            | TextureTool::DodgeBurn
            | TextureTool::Sponge
    );
    if !editing {
        clear_brush_size_gesture(
            ui.ctx(),
            crate::ui_components::BrushSweeps::TEXTURE_CANVAS.size(),
        );
        clear_brush_size_gesture(
            ui.ctx(),
            crate::ui_components::BrushSweeps::TEXTURE_CANVAS.strength(),
        );
        return;
    }
    let alt = ui.input(|input| input.modifiers.alt);

    // This canvas is where anyone who works flat spends their time, and it had
    // neither sweep. F and Shift+F worked over the model and did nothing here,
    // which is the wrong way round for someone who paints in UV space.
    let size = handle_brush_size_gesture(
        ui,
        crate::ui_components::BrushSweeps::TEXTURE_CANVAS.size(),
        canvas,
        state.texture_project.mask_brush_radius,
        TEXTURE_BRUSH_SIZE_SENSITIVITY,
        0.002..=0.25,
    );
    if let Some(radius) = size.radius {
        state.dispatch(Action::SetTextureMaskBrushRadius(radius));
    }
    let strength = crate::ui_components::handle_brush_strength_gesture(
        ui,
        crate::ui_components::BrushSweeps::TEXTURE_CANVAS.strength(),
        canvas,
        state.texture_project.mask_brush_opacity,
        crate::ui_components::BRUSH_STRENGTH_SENSITIVITY,
        0.01..=1.0,
    );
    if let Some(opacity) = strength.strength {
        state.dispatch(Action::SetTextureMaskBrushOpacity(opacity));
    }

    let pointer = ui.input(|input| input.pointer.interact_pos());
    let hovering = response.hovered().then_some(pointer).flatten();
    if let Some(cursor) = crate::ui_components::brush_cursor(
        ui,
        hovering,
        crate::ui_components::BrushSweeps::TEXTURE_CANVAS.size(),
        Some((
            crate::ui_components::BrushSweeps::TEXTURE_CANVAS.strength(),
            state.texture_project.mask_brush_opacity,
        )),
    ) && canvas.contains(cursor.at)
    {
        let radius = state.texture_project.mask_brush_radius * canvas.width();
        let color = if alt && tool.alt_inverts() {
            crate::theme::COLOR_DESTRUCTIVE
        } else {
            Color32::WHITE
        };
        crate::ui_components::paint_brush_cursor(ui.painter(), cursor, radius.max(2.0), color);
    }
    // A sweep owns the pointer while it runs, or the same drag would paint.
    if size.consumed || strength.consumed {
        return;
    }
    if matches!(tool, TextureTool::CloneStamp)
        && let Some(sample) = state.texture_project.clone_sample
    {
        let center = pos2(
            canvas.left() + sample[0] * canvas.width(),
            canvas.top() + sample[1] * canvas.height(),
        );
        crate::ui_components::paint_clone_anchor(ui.painter(), center);
    }
    let down = ui.input(|input| input.pointer.button_down(PointerButton::Primary));
    let pressed = ui.input(|input| input.pointer.button_pressed(PointerButton::Primary));
    let released = ui.input(|input| input.pointer.button_released(PointerButton::Primary));
    let drag_id = Id::new(("vkit.texture.canvas-drag", layer.id, tool as u8));
    if released {
        ui.data_mut(|data| data.remove::<Pos2>(drag_id));
        state.dispatch(Action::EndTextureEdit);
        return;
    }
    let Some(pointer) = pointer.filter(|point| canvas.contains(*point)) else {
        return;
    };
    if !down || !response.hovered() {
        return;
    }
    if !state.texture_project.edit_transaction_active() {
        state.dispatch(Action::BeginTextureEdit);
    }
    let point = [
        ((pointer.x - canvas.left()) / canvas.width()).clamp(0.0, 1.0),
        ((pointer.y - canvas.top()) / canvas.height()).clamp(0.0, 1.0),
    ];
    if matches!(tool, TextureTool::CloneStamp) && alt && pressed {
        ui.data_mut(|data| data.remove::<Pos2>(drag_id));
        state.dispatch(Action::SetTextureCloneSample(point));
        return;
    }
    if matches!(tool, TextureTool::CloneStamp) && alt {
        return;
    }
    let spacing =
        (state.texture_project.mask_brush_radius * canvas.width() * BRUSH_SPACING_FRACTION)
            .max(1.0);
    let stroke_points = brush_stroke_points(
        ui.data(|data| data.get_temp::<Pos2>(drag_id)),
        pointer,
        spacing,
    );
    let Some(&last) = stroke_points.last() else {
        return;
    };
    let subtract = layer.mask_stroke_subtracts(alt);
    for stroke_point in stroke_points {
        let uv = [
            ((stroke_point.x - canvas.left()) / canvas.width()).clamp(0.0, 1.0),
            ((stroke_point.y - canvas.top()) / canvas.height()).clamp(0.0, 1.0),
        ];
        if tool == TextureTool::MaskBrush {
            state.dispatch(Action::AddTextureMaskDab {
                id: layer.id,
                uv: [uv[0], 1.0 - uv[1]],
                source: Some(uv),
                subtract,
            });
        } else {
            let reverse = alt ^ state.texture_project.retouch_reverse;
            state.dispatch(Action::AddTextureRetouchDab {
                id: layer.id,
                point: uv,
                tool,
                reverse,
            });
        }
    }
    ui.data_mut(|data| data.insert_temp(drag_id, last));
    ui.ctx().request_repaint();
}

#[derive(Clone)]
struct ProjectionCanvasTextureCache {
    revision: u64,
    width: u32,
    height: u32,
    handle: TextureHandle,
}

fn projection_canvas_texture_handle(
    ui: &Ui,
    layer: &TextureLayer,
    paint: &crate::texture_project::TextureLayerPaint,
) -> TextureHandle {
    let size = [paint.width as usize, paint.height as usize];
    let id = Id::new(("vkit.texture.projection-canvas", layer.id));
    if let Some(mut cache) = ui.data(|data| data.get_temp::<ProjectionCanvasTextureCache>(id)) {
        if cache.revision != paint.revision
            || cache.width != paint.width
            || cache.height != paint.height
        {
            // A dab touches a few hundred pixels of a 2048/4096-square atlas. Patch the box the
            // strokes reported and leave the rest of the upload alone.
            let patch = (cache.width == paint.width && cache.height == paint.height)
                .then(|| paint_atlas_region_since(layer, cache.revision, paint.revision))
                .flatten();
            match patch {
                Some([min_x, min_y, max_x, max_y]) => cache.handle.set_partial(
                    [min_x as usize, min_y as usize],
                    rgba_region_color_image(
                        &paint.rgba8,
                        paint.width,
                        [min_x, min_y, max_x, max_y],
                    ),
                    TextureOptions::LINEAR,
                ),
                None => cache.handle.set(
                    egui::ColorImage::from_rgba_unmultiplied(size, &paint.rgba8),
                    TextureOptions::LINEAR,
                ),
            }
            cache.revision = paint.revision;
            cache.width = paint.width;
            cache.height = paint.height;
            ui.data_mut(|data| data.insert_temp(id, cache.clone()));
        }
        return cache.handle;
    }
    let handle = ui.ctx().load_texture(
        format!("vkit-projection-canvas-{}", layer.id),
        egui::ColorImage::from_rgba_unmultiplied(size, &paint.rgba8),
        TextureOptions::LINEAR,
    );
    let cache = ProjectionCanvasTextureCache {
        revision: paint.revision,
        width: paint.width,
        height: paint.height,
        handle: handle.clone(),
    };
    ui.data_mut(|data| data.insert_temp(id, cache));
    handle
}

/// The union of the boxes the atlas reported between two revisions, or `None` when the history
/// does not join them and the whole atlas has to be re-uploaded.
fn paint_atlas_region_since(layer: &TextureLayer, from: u64, to: u64) -> Option<[u32; 4]> {
    region_union_since(&layer.painted_regions, from, to)
}

fn mask_preview_texture_handle(ui: &Ui, layer_id: u64, image: &SkinImage) -> TextureHandle {
    let id = Id::new(("vkit.texture.mask-preview", layer_id));
    if let Some(mut cache) = ui.data(|data| data.get_temp::<MaskPreviewTextureCache>(id)) {
        if cache.revision != image.revision
            || cache.width != image.width
            || cache.height != image.height
        {
            cache.handle.set(
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width as usize, image.height as usize],
                    &image.rgba8,
                ),
                TextureOptions::LINEAR,
            );
            cache.revision = image.revision;
            cache.width = image.width;
            cache.height = image.height;
            ui.data_mut(|data| data.insert_temp(id, cache.clone()));
        }
        return cache.handle;
    }
    let handle = ui.ctx().load_texture(
        format!("vkit-mask-preview-{layer_id}"),
        egui::ColorImage::from_rgba_unmultiplied(
            [image.width as usize, image.height as usize],
            &image.rgba8,
        ),
        TextureOptions::LINEAR,
    );
    ui.data_mut(|data| {
        data.insert_temp(
            id,
            MaskPreviewTextureCache {
                revision: image.revision,
                width: image.width,
                height: image.height,
                handle: handle.clone(),
            },
        )
    });
    handle
}

pub(crate) fn projection_stencil_rect(state: &AppState, viewport: Rect) -> Option<Rect> {
    let layer = state.texture_project.selected_layer()?;
    let image = layer.edited_image.as_ref().or(layer.image.as_ref())?;
    if image.width == 0 || image.height == 0 {
        return None;
    }

    let bounds = viewport.shrink(24.0);
    if bounds.width() <= 1.0 || bounds.height() <= 1.0 {
        return None;
    }
    let scale = (bounds.width() / image.width as f32).min(bounds.height() / image.height as f32);
    let size = vec2(image.width as f32 * scale, image.height as f32 * scale);
    Some(Rect::from_center_size(bounds.center(), size))
}

pub(crate) fn paint_projection_stencil(ui: &Ui, state: &AppState, rect: Rect) {
    let Some(layer) = state.texture_project.selected_layer() else {
        return;
    };
    let Some(image) = layer.edited_image.as_ref().or(layer.image.as_ref()) else {
        return;
    };
    let texture = adjusted_source_texture_handle(ui, layer, image);
    let alpha = state.texture_project.projection_opacity.clamp(0.0, 1.0);
    let corners = projection_stencil_corners(state, rect);
    let tint = Color32::WHITE.gamma_multiply(alpha);

    ui.painter()
        .add(egui::Shape::mesh(stencil_quad(texture.id(), corners, tint)));

    for index in 0..4 {
        ui.painter().line_segment(
            [corners[index], corners[(index + 1) % 4]],
            Stroke::new(1.0, COLOR_PRIMARY),
        );
    }
}

fn stencil_quad(texture: egui::TextureId, corners: [Pos2; 4], tint: Color32) -> egui::epaint::Mesh {
    let mut mesh = egui::epaint::Mesh::with_texture(texture);

    for (corner, uv) in corners
        .into_iter()
        .zip([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
    {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: corner,
            uv: pos2(uv[0], uv[1]),
            color: tint,
        });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    mesh
}

pub(crate) fn projection_stencil_corners(state: &AppState, rect: Rect) -> [Pos2; 4] {
    let placement = state.texture_project.projection_placement;
    let centre = rect.center() + egui::vec2(placement.offset[0], placement.offset[1]);
    let half = egui::vec2(
        rect.width() * placement.scale * 0.5,
        rect.height() * placement.scale * 0.5,
    );
    let (sine, cosine) = placement.rotation.sin_cos();
    [
        egui::vec2(-half.x, -half.y),
        egui::vec2(half.x, -half.y),
        egui::vec2(half.x, half.y),
        egui::vec2(-half.x, half.y),
    ]
    .map(|offset| {
        centre
            + egui::vec2(
                offset.x * cosine - offset.y * sine,
                offset.x * sine + offset.y * cosine,
            )
    })
}

#[derive(Clone)]
struct AdjustedTextureCache {
    revision: u64,
    width: u32,
    height: u32,
    adjustments: TextureColorAdjustments,
    handle: TextureHandle,
}

fn adjusted_source_texture_handle(
    ui: &Ui,
    layer: &TextureLayer,
    image: &SkinImage,
) -> TextureHandle {
    let adjustments = if layer.channel.is_color() {
        layer.adjustments
    } else {
        TextureColorAdjustments::default()
    };
    let build = || {
        let size = [image.width as usize, image.height as usize];
        if adjustments == TextureColorAdjustments::default() {
            egui::ColorImage::from_rgba_unmultiplied(size, &image.rgba8)
        } else {
            let mut rgba = image.rgba8.as_ref().clone();
            apply_color_adjustments(&mut rgba, adjustments);
            egui::ColorImage::from_rgba_unmultiplied(size, &rgba)
        }
    };
    let id = Id::new(("vkit.texture.source-view", layer.id));
    if let Some(mut cache) = ui.data(|data| data.get_temp::<AdjustedTextureCache>(id)) {
        if cache.revision != image.revision
            || cache.width != image.width
            || cache.height != image.height
            || cache.adjustments != adjustments
        {
            let patch = (cache.width == image.width
                && cache.height == image.height
                && cache.adjustments == adjustments)
                .then(|| painted_region_since(layer, cache.revision, image.revision))
                .flatten();
            match patch {
                Some([min_x, min_y, max_x, max_y]) => {
                    cache.handle.set_partial(
                        [min_x as usize, min_y as usize],
                        region_color_image(image, [min_x, min_y, max_x, max_y], adjustments),
                        TextureOptions::LINEAR,
                    );
                }
                None => cache.handle.set(build(), TextureOptions::LINEAR),
            }
            cache.revision = image.revision;
            cache.width = image.width;
            cache.height = image.height;
            cache.adjustments = adjustments;
            ui.data_mut(|data| data.insert_temp(id, cache.clone()));
        }
        return cache.handle;
    }
    let handle = ui.ctx().load_texture(
        format!("vkit-texture-source-{}", layer.id),
        build(),
        TextureOptions::LINEAR,
    );
    ui.data_mut(|data| {
        data.insert_temp(
            id,
            AdjustedTextureCache {
                revision: image.revision,
                width: image.width,
                height: image.height,
                adjustments,
                handle: handle.clone(),
            },
        )
    });
    handle
}

fn painted_region_since(layer: &TextureLayer, from: u64, to: u64) -> Option<[u32; 4]> {
    region_union_since(&layer.edited_regions, from, to)
}

fn region_union_since(
    regions: &std::collections::VecDeque<(u64, [u32; 4])>,
    from: u64,
    to: u64,
) -> Option<[u32; 4]> {
    if to <= from {
        return None;
    }
    let mut expected = from.wrapping_add(1);
    let mut bounds: Option<[u32; 4]> = None;
    for (revision, region) in regions.iter().skip_while(|(revision, _)| *revision <= from) {
        if *revision != expected {
            return None;
        }
        expected = revision.wrapping_add(1);
        bounds = Some(match bounds {
            Some(current) => [
                current[0].min(region[0]),
                current[1].min(region[1]),
                current[2].max(region[2]),
                current[3].max(region[3]),
            ],
            None => *region,
        });
    }
    (expected == to.wrapping_add(1)).then_some(bounds).flatten()
}

fn region_color_image(
    image: &SkinImage,
    region: [u32; 4],
    adjustments: TextureColorAdjustments,
) -> egui::ColorImage {
    let (size, mut rgba) = crop_region(&image.rgba8, image.width, region);
    if adjustments != TextureColorAdjustments::default() {
        apply_color_adjustments(&mut rgba, adjustments);
    }
    egui::ColorImage::from_rgba_unmultiplied(size, &rgba)
}

fn rgba_region_color_image(rgba8: &[u8], width: u32, region: [u32; 4]) -> egui::ColorImage {
    let (size, rgba) = crop_region(rgba8, width, region);
    egui::ColorImage::from_rgba_unmultiplied(size, &rgba)
}

fn crop_region(
    rgba8: &[u8],
    source_width: u32,
    [min_x, min_y, max_x, max_y]: [u32; 4],
) -> ([usize; 2], Vec<u8>) {
    let width = (max_x - min_x + 1) as usize;
    let height = (max_y - min_y + 1) as usize;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in min_y..=max_y {
        let row = (y as usize * source_width as usize + min_x as usize) * 4;
        rgba.extend_from_slice(&rgba8[row..row + width * 4]);
    }
    ([width, height], rgba)
}

/// The layer row is 36 points tall. Downscaling once to this edge is far cheaper than handing egui
/// a 2048/4096-square atlas and letting it sample a thumbnail out of it.
const PROJECTION_THUMBNAIL_EDGE: u32 = 128;

/// The most often a stroke that never opened an undo transaction can force the downscale.
const PROJECTION_THUMBNAIL_MIN_INTERVAL: f64 = 0.25;

#[derive(Clone)]
struct ProjectionThumbnailCache {
    revision: u64,
    refreshed_at: f64,
    handle: TextureHandle,
}

fn paint_projection_thumbnail(
    ui: &Ui,
    rect: Rect,
    layer_id: u64,
    paint: &crate::texture_project::TextureLayerPaint,
    stroke_in_progress: bool,
) {
    ui.painter().rect_filled(rect, CONTROL_RADIUS, COLOR_BG);
    let texture = projection_thumbnail_handle(ui, layer_id, paint, stroke_in_progress);
    ui.painter().image(
        texture.id(),
        rect,
        Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    ui.painter().rect_stroke(
        rect,
        CONTROL_RADIUS,
        Stroke::new(1.0, COLOR_BORDER),
        egui::StrokeKind::Inside,
    );
}

fn projection_thumbnail_handle(
    ui: &Ui,
    layer_id: u64,
    paint: &crate::texture_project::TextureLayerPaint,
    stroke_in_progress: bool,
) -> TextureHandle {
    let id = Id::new(("vkit.texture.projection-thumbnail", layer_id));
    let now = ui.input(|input| input.time);
    let cached = ui.data(|data| data.get_temp::<ProjectionThumbnailCache>(id));
    if let Some(cache) = &cached {
        if cache.revision == paint.revision {
            return cache.handle.clone();
        }
        // Downscaling reads the whole atlas, so hold it for the stroke: a 36-point thumbnail one
        // stroke behind is invisible, and releasing the pointer refreshes it.
        if stroke_in_progress {
            return cache.handle.clone();
        }
        // A stroke that never opened a transaction would otherwise land here per dab. Floor the
        // rate, and ask for the frame that catches up so the row cannot sit stale once the
        // pointer goes quiet.
        let held_for = now - cache.refreshed_at;
        if held_for < PROJECTION_THUMBNAIL_MIN_INTERVAL {
            ui.ctx().request_repaint_after(Duration::from_secs_f64(
                PROJECTION_THUMBNAIL_MIN_INTERVAL - held_for,
            ));
            return cache.handle.clone();
        }
    }
    let image = projection_thumbnail_image(paint);
    match cached {
        Some(mut cache) => {
            cache.handle.set(image, TextureOptions::LINEAR);
            cache.revision = paint.revision;
            cache.refreshed_at = now;
            let handle = cache.handle.clone();
            ui.data_mut(|data| data.insert_temp(id, cache));
            handle
        }
        None => {
            let handle = ui.ctx().load_texture(
                format!("vkit-projection-thumbnail-{layer_id}"),
                image,
                TextureOptions::LINEAR,
            );
            ui.data_mut(|data| {
                data.insert_temp(
                    id,
                    ProjectionThumbnailCache {
                        revision: paint.revision,
                        refreshed_at: now,
                        handle: handle.clone(),
                    },
                )
            });
            handle
        }
    }
}

/// Subsampled, not averaged: the GPU was already picking single texels out of the atlas for a
/// 36-point row, so this looks the same and reads 16k pixels instead of the whole 4-to-16-megapixel
/// atlas. A box filter here would cost more than the upload it is saving.
fn projection_thumbnail_image(
    paint: &crate::texture_project::TextureLayerPaint,
) -> egui::ColorImage {
    let edge = PROJECTION_THUMBNAIL_EDGE.min(paint.width).min(paint.height);
    if edge == 0 || paint.rgba8.len() != paint.width as usize * paint.height as usize * 4 {
        return egui::ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 0]);
    }
    let mut rgba = Vec::with_capacity(edge as usize * edge as usize * 4);
    for y in 0..edge {
        let row = (u64::from(y) * u64::from(paint.height) / u64::from(edge)) as usize
            * paint.width as usize
            * 4;
        for x in 0..edge {
            let offset =
                row + (u64::from(x) * u64::from(paint.width) / u64::from(edge)) as usize * 4;
            rgba.extend_from_slice(&paint.rgba8[offset..offset + 4]);
        }
    }
    egui::ColorImage::from_rgba_unmultiplied([edge as usize, edge as usize], &rgba)
}

fn paint_thumbnail(ui: &Ui, rect: Rect, layer_id: u64, image: Option<&SkinImage>) {
    ui.painter().rect_filled(rect, CONTROL_RADIUS, COLOR_BG);
    if let Some(image) = image {
        crate::ui_components::paint_thumbnail_image(
            ui,
            rect,
            TEXTURE_THUMBNAIL_NS,
            layer_id,
            image,
        );
    } else {
        paint_icon(
            ui.painter(),
            rect.shrink(8.0),
            Icon::HeadTexture,
            COLOR_MUTED,
        );
    }
    ui.painter().rect_stroke(
        rect,
        CONTROL_RADIUS,
        Stroke::new(1.0, COLOR_BORDER),
        egui::StrokeKind::Inside,
    );
}

fn icon_hit(
    ui: &Ui,
    rect: Rect,
    icon: Icon,
    salt: &'static str,
    layer_id: u64,
    active: bool,
) -> Response {
    let response = ui.interact(rect, ui.id().with((salt, layer_id)), Sense::click());

    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CONTROL_RADIUS, COLOR_SURFACE_HOVER);
    }
    paint_icon(
        ui.painter(),
        rect.shrink(5.0),
        icon,
        if response.hovered() || active {
            COLOR_TEXT
        } else {
            COLOR_MUTED
        },
    );
    response
}

fn correction_slider(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
    reset_tooltip: &str,
) -> bool {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), CONTROL_H_DENSE),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = SPACE_1;
            let label_width = 52.0_f32.min(ui.available_width() * 0.3);
            ui.add_sized(
                [label_width, CONTROL_H_DENSE],
                egui::Label::new(RichText::new(label).size(FONT_XS).color(COLOR_MUTED)),
            );

            let value_width = 44.0;
            let reset_width = CONTROL_H_DENSE;
            let spacing = ui.spacing().item_spacing.x;
            let track_width =
                (ui.available_width() - value_width - reset_width - spacing * 2.0).max(36.0);

            let dragging = ui
                .allocate_ui_with_layout(
                    vec2(track_width, CONTROL_H_DENSE),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.add(
                            FilledNumericSlider::new(value, range)
                                .hide_value()
                                .min_width(track_width),
                        )
                        .dragged()
                    },
                )
                .inner;
            ui.add_sized(
                [value_width, CONTROL_H_DENSE],
                egui::Label::new(
                    RichText::new(format!("{:.2}", *value))
                        .size(FONT_XS)
                        .color(COLOR_TEXT),
                ),
            );
            if icon_button(ui, Icon::Refresh, reset_tooltip).clicked() {
                *value = default;
            }
            dragging
        },
    )
    .inner
}

const fn blend_label(locale: Locale, mode: TextureBlendMode) -> &'static str {
    text(
        locale,
        match mode {
            TextureBlendMode::Normal => TextKey::BlendNormal,
            TextureBlendMode::Multiply => TextKey::BlendMultiply,
            TextureBlendMode::Screen => TextKey::BlendScreen,
            TextureBlendMode::Overlay => TextKey::BlendOverlay,
        },
    )
}

const fn channel_display(channel: TextureChannel) -> &'static str {
    match channel {
        TextureChannel::Diffuse => "Diffuse",
        TextureChannel::Normal => "Normal",
        TextureChannel::Roughness => "Roughness",
        TextureChannel::Glossiness => "Glossiness",
        TextureChannel::Smoothness => "Smoothness",
        TextureChannel::Metallic => "Metallic",
        TextureChannel::Specular => "Specular",
        TextureChannel::Mask => "Mask",
    }
}

const fn texture_tool_text_key(tool: TextureTool) -> TextKey {
    match tool {
        TextureTool::PinPair => TextKey::TextureToolPinPair,
        TextureTool::Projection => TextKey::ProjectImage,
        TextureTool::MaskBrush => TextKey::TextureToolMask,
        TextureTool::CloneStamp => TextKey::TextureToolClone,
        TextureTool::DodgeBurn => TextKey::TextureToolDodgeBurn,
        TextureTool::Sponge => TextKey::TextureToolSponge,
    }
}

const fn texture_tool_shortcut(tool: TextureTool) -> Option<Shortcut> {
    match tool {
        TextureTool::PinPair => Some(Shortcut::TexturePinBrush),
        TextureTool::CloneStamp => Some(Shortcut::TextureCloneBrush),
        TextureTool::Projection
        | TextureTool::MaskBrush
        | TextureTool::DodgeBurn
        | TextureTool::Sponge => None,
    }
}

const fn tool_icon(tool: TextureTool) -> Icon {
    match tool {
        TextureTool::PinPair => Icon::TexturePin,
        TextureTool::Projection => Icon::Projector,
        TextureTool::MaskBrush => Icon::TextureMask,
        TextureTool::CloneStamp => Icon::CloneStamp,
        TextureTool::DodgeBurn => Icon::DodgeBurn,
        TextureTool::Sponge => Icon::TextureSponge,
    }
}

fn paint_empty_source(ui: &Ui, locale: Locale, rect: Rect) {
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text(locale, TextKey::SelectTextureLayer),
        FontId::proportional(FONT_SM),
        COLOR_MUTED,
    );
}

fn paint_source_status(ui: &Ui, locale: Locale, rect: Rect, layer: &TextureLayer) {
    let label = if layer.loading {
        text(locale, TextKey::LoadingTextureImage)
    } else if let Some(error) = layer.load_error.as_deref() {
        error
    } else if layer.source_mode == TextureSourceMode::ScanMesh {
        text(locale, TextKey::ScanTextureAlignment)
    } else {
        text(locale, TextKey::NoTextureImage)
    };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(FONT_SM),
        COLOR_MUTED,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_direction_button_always_switches_to_the_other_one() {
        for (resting, alt) in [(false, false), (true, false), (false, true), (true, true)] {
            let shown_reverse = resting ^ alt;
            let pressed = !shown_reverse ^ alt;

            assert_ne!(pressed ^ alt, shown_reverse, "resting {resting}, alt {alt}");
        }
    }

    #[test]
    fn every_brush_direction_is_named_in_words() {
        for key in [
            TextKey::BrushDodge,
            TextKey::BrushBurn,
            TextKey::BrushSaturate,
            TextKey::BrushDesaturate,
        ] {
            for locale in crate::i18n::Locale::ALL {
                let label = text(locale, key);
                assert!(!label.trim().is_empty(), "{key:?} in {locale:?}");
                assert!(
                    !label.contains('↑') && !label.contains('↓'),
                    "{key:?} in {locale:?} still uses an arrow: {label}"
                );
            }
        }
    }

    #[test]
    fn the_stencil_quad_builds_without_tripping_epaint() {
        let corners = [
            pos2(10.0, 10.0),
            pos2(110.0, 20.0),
            pos2(100.0, 120.0),
            pos2(0.0, 110.0),
        ];
        let mesh = stencil_quad(
            egui::TextureId::Managed(7),
            corners,
            Color32::WHITE.gamma_multiply(0.5),
        );
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices.len(), 6);

        let uvs: Vec<_> = mesh.vertices.iter().map(|vertex| vertex.uv).collect();
        assert_eq!(
            uvs,
            vec![
                pos2(0.0, 0.0),
                pos2(1.0, 0.0),
                pos2(1.0, 1.0),
                pos2(0.0, 1.0)
            ]
        );
        assert!(
            mesh.vertices
                .iter()
                .zip(corners)
                .all(|(vertex, corner)| vertex.pos == corner)
        );
    }

    #[test]
    fn the_pbr_choice_waits_for_a_map_it_can_govern() {
        let mut state = AppState::default();
        assert!(
            !texture_bundle_has_material_maps(&state),
            "nothing authored yet"
        );

        let id = state.texture_project.add_image_layer(
            std::path::PathBuf::from("face.png"),
            TextureSourceMode::LandmarkPins,
        );
        assert!(
            !texture_bundle_has_material_maps(&state),
            "a diffuse layer alone is not a material set"
        );

        for channel in [TextureChannel::Mask, TextureChannel::Diffuse] {
            state
                .texture_project
                .layers
                .iter_mut()
                .find(|layer| layer.id == id)
                .expect("the layer")
                .channel = channel;
            assert!(
                !texture_bundle_has_material_maps(&state),
                "{channel:?} is written the same either way"
            );
        }

        for channel in [
            TextureChannel::Normal,
            TextureChannel::Roughness,
            TextureChannel::Metallic,
            TextureChannel::Glossiness,
            TextureChannel::Smoothness,
            TextureChannel::Specular,
        ] {
            state
                .texture_project
                .layers
                .iter_mut()
                .find(|layer| layer.id == id)
                .expect("the layer")
                .channel = channel;
            assert!(
                texture_bundle_has_material_maps(&state),
                "{channel:?} is what the convention decides"
            );
        }
    }

    #[test]
    fn a_stroke_lays_dabs_along_the_segment_it_travelled() {
        assert_eq!(
            brush_stroke_points(None, pos2(10.0, 10.0), 4.0),
            vec![pos2(10.0, 10.0)]
        );

        assert!(brush_stroke_points(Some(pos2(0.0, 0.0)), pos2(3.0, 0.0), 4.0).is_empty());

        let points = brush_stroke_points(Some(pos2(0.0, 0.0)), pos2(20.0, 0.0), 4.0);
        assert_eq!(points.len(), 5);
        assert_eq!(points[0], pos2(4.0, 0.0));
        assert_eq!(points[4], pos2(20.0, 0.0));

        assert!(brush_stroke_points(Some(pos2(0.0, 0.0)), pos2(1.0e6, 0.0), 1.0).len() <= 256);
    }

    #[test]
    fn source_zoom_keeps_the_sample_under_the_pointer_anchored() {
        let bounds = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let image_rect = Rect::from_min_size(pos2(100.0, 100.0), vec2(600.0, 400.0));
        let pointer = pos2(520.0, 260.0);
        let source = [
            (pointer.x - image_rect.left()) / image_rect.width(),
            (pointer.y - image_rect.top()) / image_rect.height(),
        ];
        let mut zoom = 1.0;
        let mut center = [0.5, 0.5];

        assert!(apply_source_zoom(
            bounds,
            image_rect,
            pointer,
            240.0,
            &mut zoom,
            &mut center,
        ));

        let new_size = image_rect.size() * zoom;
        let new_min = bounds.center() - vec2(center[0] * new_size.x, center[1] * new_size.y);
        let anchored = new_min + vec2(source[0] * new_size.x, source[1] * new_size.y);
        assert!(anchored.distance(pointer) < 0.01);
    }

    #[test]
    fn source_center_clamp_prevents_empty_space_when_panning() {
        let bounds = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0));
        let mut center = [-4.0, 6.0];
        clamp_source_center(bounds, vec2(800.0, 600.0), &mut center);
        assert_eq!(center, [0.25, 0.75]);

        clamp_source_center(bounds, vec2(200.0, 150.0), &mut center);
        assert_eq!(center, [0.5, 0.5]);
    }
}
