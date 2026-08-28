use super::*;

pub(super) const VIEWPORT_TOOL_PANELS: [ViewportToolPanel; 6] = [
    ViewportToolPanel::Lighting,
    BACKGROUND_PANEL,
    ViewportToolPanel::Wireframe,
    ViewportToolPanel::Xray,
    ViewportToolPanel::Skin,
    ViewportToolPanel::Camera,
];

pub(super) fn draw_viewport_tools(
    ui: &mut Ui,
    state: &mut AppState,
    viewport: Rect,
    scope: &'static str,
) {
    let id = Id::new(("vkit.viewport.tools", scope));
    let measured_open_panel = state.viewport_tool_panel.and_then(|panel| {
        measure_viewport_tool_panel_rect(ui, state, viewport, panel, id.with("measure-open"))
    });
    dismiss_viewport_tool_panel_on_click_away(ui, state, viewport, measured_open_panel);
    for (index, panel) in VIEWPORT_TOOL_PANELS.into_iter().enumerate() {
        let Some(rect) = viewport_tool_button_rect(viewport, index) else {
            continue;
        };

        let available = true;
        let response = ui
            .interact(rect, id.with(("button", index)), Sense::click())
            .on_disabled_hover_text(text(state.locale, TextKey::HairNeedsFinishedHead));
        let open = state.viewport_tool_panel == Some(panel);
        let enabled = match panel {
            ViewportToolPanel::Lighting
            | ViewportToolPanel::Skin
            | ViewportToolPanel::Camera
            | ViewportToolPanel::BaseView => false,
            ViewportToolPanel::Wireframe => state.wireframe_visible,
            ViewportToolPanel::Xray => state.xray_visible,
        };

        let control_state = crate::theme::ControlState {
            hovered: response.hovered(),
            pressed: response.is_pointer_button_down_on(),
            active: open || enabled,
        };

        let attention = (panel == ViewportToolPanel::Skin)
            .then(|| crate::ui::attention_frame_for(ui, crate::state::AttentionTarget::SkinPanel))
            .flatten();
        let nudge = vec2(attention.map_or(0.0, |frame| frame.shake), 0.0);
        let fill = crate::theme::control_fill(crate::theme::COLOR_RAIL_IDLE, control_state);
        ui.painter().circle_filled(
            rect.center() + nudge,
            VIEWPORT_TOOL_SIZE * 0.5,
            attention.map_or(fill, |frame| {
                crate::ui_components::attention_tint(fill, frame)
            }),
        );
        paint_viewport_tool_icon(
            ui,
            rect.translate(nudge),
            panel,
            crate::theme::control_ink(control_state),
        );

        let tooltip = match panel {
            ViewportToolPanel::Lighting => TextKey::ViewportLightingTooltip,
            ViewportToolPanel::BaseView => TextKey::ViewportBackgroundTooltip,
            ViewportToolPanel::Wireframe => TextKey::ViewportWireframeTooltip,
            ViewportToolPanel::Xray => TextKey::ViewportXrayTooltip,
            ViewportToolPanel::Skin => TextKey::ViewportSkinTooltip,
            ViewportToolPanel::Camera => TextKey::CameraSettings,
        };
        let response = response.on_hover_text(text(state.locale, tooltip));
        control_affordances(ui, &response, rect, VIEWPORT_TOOL_SIZE * 0.5);
        if response.clicked() && available {
            let was_open = state.viewport_tool_panel == Some(panel);
            state.dispatch(Action::ToggleViewportToolPanel(panel));
            if matches!(panel, ViewportToolPanel::Skin)
                && !was_open
                && state.vam_root.is_some()
                && matches!(state.vam_catalog_status, VaMCatalogStatus::Unconfigured)
            {
                state.dispatch(Action::RefreshVaMCatalog);
            }
        }
    }

    let Some(panel_kind) = state.viewport_tool_panel else {
        return;
    };
    let panel_rect = if state.viewport_tool_panel == Some(panel_kind) {
        measured_open_panel
    } else {
        None
    }
    .or_else(|| {
        measure_viewport_tool_panel_rect(ui, state, viewport, panel_kind, id.with("measure-new"))
    });
    let Some(panel_rect) = panel_rect else {
        return;
    };

    let area = egui::Area::new(viewport_tool_panel_layer().id)
        .order(viewport_tool_panel_layer().order)
        .fixed_pos(panel_rect.min)
        .constrain(false)
        .interactable(true)
        .show(ui.ctx(), |ui| {
            ui.set_min_size(panel_rect.size());
            ui.set_max_size(panel_rect.size());
        });
    let mut overlay = ui.new_child(
        UiBuilder::new()
            .layer_id(area.response.layer_id)
            .id_salt(id.with(("panel-overlay", panel_kind)))
            .max_rect(panel_rect),
    );
    let ui = &mut overlay;
    ui.painter().rect_filled(
        panel_rect,
        f32::from(crate::theme::RADIUS_POPOVER),
        COLOR_TOPBAR,
    );
    let _blocker = ui.interact(
        panel_rect,
        id.with("panel-blocker"),
        Sense::click_and_drag(),
    );
    let inner = panel_rect.shrink2(vec2(MINI_POPUP_CONTENT_INSET_X, MINI_POPUP_CONTENT_INSET_Y));
    if inner.width() < 48.0 || inner.height() < 24.0 {
        return;
    }
    let mut panel_ui = ui.new_child(
        UiBuilder::new()
            .id_salt(id.with(("panel", panel_kind)))
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );

    panel_ui.set_clip_rect(panel_rect);
    let desired_height = ui.data(|data| {
        data.get_temp::<f32>(viewport_tool_panel_desired_height_cache_id(
            panel_kind,
            state.base_view_mode,
        ))
    });
    let clamped = desired_height.is_some_and(|desired| desired > panel_rect.height() + 0.5);
    if clamped {
        egui::ScrollArea::vertical()
            .id_salt(id.with(("panel-scroll", panel_kind)))
            .auto_shrink([false, false])
            .show(&mut panel_ui, |scroll_ui| {
                let width = scroll_ui.available_width().max(0.0);
                scroll_ui.set_min_width(width);
                scroll_ui.set_max_width(width);

                if let Some(placement) = viewport_tool_panel_placement(viewport, panel_kind) {
                    publish_panel_lane_limit(
                        scroll_ui,
                        placement.origin.y + placement.available_height,
                    );
                }
                draw_viewport_tool_panel_contents(scroll_ui, state, panel_kind);
            });
        return;
    }

    let width = panel_ui.available_width().max(0.0);
    panel_ui.set_min_width(width);
    panel_ui.set_max_width(width);
    if let Some(placement) = viewport_tool_panel_placement(viewport, panel_kind) {
        publish_panel_lane_limit(&panel_ui, placement.origin.y + placement.available_height);
    }
    draw_viewport_tool_panel_contents(&mut panel_ui, state, panel_kind);
}

pub(super) fn publish_panel_lane_limit(ui: &Ui, limit: f32) {
    ui.data_mut(|data| data.insert_temp(panel_lane_limit_id(), limit));
}

pub(super) fn panel_lane_limit_id() -> Id {
    Id::new("vkit.viewport.tool-panel.lane-limit")
}

pub(super) fn viewport_tool_panel_layer() -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Middle,
        Id::new("vkit.viewport.tool-panel.layer"),
    )
}

pub(crate) fn panel_list_budget(ui: &Ui, cursor_top: f32, footer: f32) -> Option<f32> {
    let limit = ui.data(|data| data.get_temp::<f32>(panel_lane_limit_id()))?;
    Some((limit - cursor_top - footer).max(0.0))
}

pub(super) fn dismiss_viewport_tool_panel_on_click_away(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    measured_panel_rect: Option<Rect>,
) {
    let Some(panel) = state.viewport_tool_panel else {
        return;
    };
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return;
    };
    let clicked = ui.input(|input| input.pointer.any_click());
    if viewport_tool_panel_should_dismiss(viewport, measured_panel_rect, pointer, clicked) {
        state.dispatch(Action::ToggleViewportToolPanel(panel));
    }
}

pub(super) fn viewport_tool_panel_should_dismiss(
    viewport: Rect,
    panel_rect: Option<Rect>,
    pointer: Pos2,
    clicked: bool,
) -> bool {
    clicked && !viewport_tool_panel_contains(viewport, panel_rect, pointer)
}

pub(super) fn viewport_tool_panel_contains(
    viewport: Rect,
    panel_rect: Option<Rect>,
    pointer: Pos2,
) -> bool {
    let help_clicked =
        viewport_help_button_rect(viewport).is_some_and(|button| button.contains(pointer));
    (viewport_tool_rail_rect(viewport).is_some_and(|rail| rail.contains(pointer)) && !help_clicked)
        || panel_rect.is_some_and(|rect| rect.contains(pointer))
}

pub(super) fn draw_viewport_tool_panel_contents(
    ui: &mut Ui,
    state: &mut AppState,
    panel: ViewportToolPanel,
) {
    let title = match panel {
        ViewportToolPanel::Lighting => TextKey::Lighting,
        ViewportToolPanel::BaseView => TextKey::Background,
        ViewportToolPanel::Wireframe => TextKey::WireframeSettings,
        ViewportToolPanel::Xray => TextKey::XraySettings,

        ViewportToolPanel::Skin => TextKey::SkinSettings,
        ViewportToolPanel::Camera => TextKey::CameraSettings,
    };
    draw_centered_viewport_panel_title(ui, text(state.locale, title));
    match panel {
        ViewportToolPanel::Lighting => draw_viewport_lighting_panel(ui, state),
        ViewportToolPanel::BaseView => draw_viewport_background_panel(ui, state),
        ViewportToolPanel::Wireframe => {
            let mut visible = state.wireframe_visible;
            let mut opacity = state.wireframe_opacity;
            let (visible_changed, opacity_changed) = ui
                .allocate_ui_with_layout(
                    vec2(ui.available_width().max(0.0), CONTROL_HEIGHT),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let visible_changed =
                            switch(ui, &mut visible, text(state.locale, TextKey::On)).changed();
                        let opacity_changed = ui
                            .add(
                                FilledNumericSlider::new(&mut opacity, 0.0..=1.0)
                                    .percent()
                                    .decimals(0)
                                    .min_width(120.0),
                            )
                            .changed();
                        (visible_changed, opacity_changed)
                    },
                )
                .inner;
            if visible_changed {
                state.dispatch(Action::ToggleWireframe(visible));
            }
            if opacity_changed {
                state.dispatch(Action::SetWireframeOpacity(opacity));
            }
            ui.add_space(SPACE_2);
            let mut color = state.wireframe_color_rgb;
            let color_changed = ui
                .horizontal(|ui| {
                    ui.add_sized(
                        [
                            (ui.available_width()
                                - crate::ui_components::COMPACT_COLOR_SWATCH_WIDTH
                                - ui.spacing().item_spacing.x)
                                .max(0.0),
                            CONTROL_HEIGHT,
                        ],
                        egui::Label::new(
                            RichText::new(text(state.locale, TextKey::WireframeColor))
                                .color(COLOR_MUTED),
                        )
                        .truncate(),
                    );
                    compact_color_picker(ui, &mut color).changed()
                })
                .inner;
            if color_changed {
                state.dispatch(Action::SetWireframeColor(color));
            }
        }
        ViewportToolPanel::Xray => {
            let mut visible = state.xray_visible;
            let mut opacity = state.xray_opacity;
            let (visible_changed, opacity_changed) = ui
                .allocate_ui_with_layout(
                    vec2(ui.available_width().max(0.0), CONTROL_HEIGHT),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        let visible_changed =
                            switch(ui, &mut visible, text(state.locale, TextKey::On)).changed();
                        let opacity_changed = ui
                            .add(
                                FilledNumericSlider::new(&mut opacity, 0.0..=1.0)
                                    .percent()
                                    .decimals(0)
                                    .min_width(120.0),
                            )
                            .changed();
                        (visible_changed, opacity_changed)
                    },
                )
                .inner;
            if visible_changed {
                state.dispatch(Action::ToggleXray(visible));
            }
            if opacity_changed {
                state.dispatch(Action::SetXrayOpacity(opacity));
            }
        }
        ViewportToolPanel::Skin => crate::ui::draw_viewport_skin_panel_contents(ui, state),
        ViewportToolPanel::Camera => draw_viewport_camera_panel(ui, state),
    }

    ui.allocate_space(Vec2::ZERO);
}

pub(super) fn draw_centered_viewport_panel_title(ui: &mut Ui, title: &str) {
    let width = ui.available_width().max(0.0);
    ui.allocate_ui_with_layout(vec2(width, 22.0), Layout::top_down(Align::Center), |ui| {
        ui.label(
            RichText::new(title)
                .size(FONT_BODY)
                .strong()
                .color(COLOR_TEXT),
        );
    });
    ui.add_space(SPACE_2);
}

pub(super) fn background_mode_key(mode: ViewportBackgroundMode) -> TextKey {
    match mode {
        ViewportBackgroundMode::Radial => TextKey::BackgroundRadial,
        ViewportBackgroundMode::Vertical => TextKey::BackgroundVertical,
        ViewportBackgroundMode::Flat => TextKey::BackgroundFlat,
    }
}

pub(super) fn draw_viewport_background_panel(ui: &mut Ui, state: &mut AppState) {
    let selected = BACKGROUND_MODES
        .iter()
        .position(|mode| state.viewport_background_mode == *mode)
        .unwrap_or(0);
    animated_segmented_group(
        ui,
        "vkit.viewport.background-mode",
        BACKGROUND_MODES.len(),
        selected,
        |ui, segment_width| {
            for mode in BACKGROUND_MODES {
                if segment_button(
                    ui,
                    segment_width,
                    text(state.locale, background_mode_key(mode)),
                    state.viewport_background_mode == mode,
                )
                .clicked()
                {
                    state.dispatch(Action::SetViewportBackgroundMode(mode));
                }
            }
        },
    );
    super::reference_panel::draw_reference_section(ui, state);
}

pub(super) fn relevant_viewport_camera(state: &AppState) -> TurntableCamera {
    state.workspace.stage_camera(state.active_tab)
}

pub(super) fn draw_viewport_camera_panel(ui: &mut Ui, state: &mut AppState) {
    let camera = relevant_viewport_camera(state);
    let current = camera.projection_mode;
    let mut requested = current;
    animated_segmented_group(
        ui,
        "vkit.viewport.projection",
        2,
        usize::from(current == ProjectionMode::Orthographic),
        |ui, segment_width| {
            for (mode, key) in [
                (ProjectionMode::Perspective, TextKey::Perspective),
                (ProjectionMode::Orthographic, TextKey::Orthographic),
            ] {
                if segment_button(ui, segment_width, text(state.locale, key), current == mode)
                    .clicked()
                {
                    requested = mode;
                }
            }
        },
    );
    if requested != current {
        state.dispatch(Action::ToggleProjection);
    }

    ui.add_space(SPACE_2);
    ui.label(
        RichText::new(text(state.locale, TextKey::Fov))
            .size(FONT_XS)
            .color(COLOR_MUTED),
    );
    let mut fov = camera.fov_y_degrees();
    let fov_response = ui.add_enabled(
        current == ProjectionMode::Perspective,
        FilledNumericSlider::new(&mut fov, 10.0..=120.0)
            .decimals(1)
            .min_width(140.0),
    );
    if fov_response.changed() {
        state.dispatch(Action::SetFov(fov));
    }

    ui.add_space(SPACE_2);
    ui.label(
        RichText::new(text(state.locale, TextKey::CameraRoll))
            .size(FONT_XS)
            .color(COLOR_MUTED),
    );
    let mut roll = camera.roll_degrees();
    let roll_response = ui.add(
        FilledNumericSlider::new(&mut roll, -180.0..=180.0)
            .decimals(1)
            .min_width(140.0),
    );
    if roll_response.changed() {
        state.dispatch(Action::SetCameraRoll(roll));
    }

    ui.add_space(SPACE_3);
    let reset = ui.add_sized(
        [ui.available_width().max(0.0), crate::theme::CONTROL_H_DENSE],
        egui::Button::new(text(state.locale, TextKey::ResetCamera))
            .corner_radius(crate::theme::CONTROL_H_DENSE * 0.5),
    );
    let reset = crate::ui_components::tooltip(
        reset,
        text(state.locale, TextKey::ResetCamera),
        Some("Home / Numpad 0 / Numpad ."),
    );
    if reset.clicked() {
        state.dispatch(Action::ResetCamera);
    }
}

pub(super) fn draw_viewport_lighting_panel(ui: &mut Ui, state: &mut AppState) {
    const PRESET_ROW_HEIGHT: f32 = 28.0;
    const PRESET_ROW_GAP: f32 = 2.0;
    let presets = LightingPreset::ALL;
    let selected_index = presets
        .iter()
        .position(|preset| state.lighting_preset == *preset)
        .unwrap_or(0);
    let width = ui.available_width().max(0.0);
    let rows = presets.len() as f32;
    let (group_rect, _) = ui.allocate_exact_size(
        vec2(
            width,
            PRESET_ROW_HEIGHT * rows + PRESET_ROW_GAP * (rows - 1.0),
        ),
        Sense::hover(),
    );
    let row_rect = |index: usize| {
        Rect::from_min_size(
            pos2(
                group_rect.left(),
                group_rect.top() + index as f32 * (PRESET_ROW_HEIGHT + PRESET_ROW_GAP),
            ),
            vec2(group_rect.width(), PRESET_ROW_HEIGHT),
        )
    };
    let thumb = animate_rect(
        ui,
        Id::new("vkit.viewport.lighting-preset-thumb"),
        selected_index as u64,
        row_rect(selected_index),
    );
    ui.painter()
        .rect_filled(thumb, PRESET_ROW_HEIGHT * 0.5, COLOR_SURFACE_RAISED);
    for (index, preset) in presets.into_iter().enumerate() {
        let row = row_rect(index);
        let selected = state.lighting_preset == preset;
        let response = ui.interact(
            row,
            Id::new(("vkit.viewport.lighting-preset", index)),
            Sense::click(),
        );
        let swatch = Rect::from_min_size(
            pos2(row.left() + 6.0, row.center().y - 10.0),
            vec2(42.0, 20.0),
        );
        paint_lighting_swatch(ui, swatch, preset);
        ui.painter().text(
            pos2(swatch.right() + 8.0, row.center().y),
            Align2::LEFT_CENTER,
            crate::settings::lighting_preset_label(preset, state.locale),
            FontId::proportional(FONT_BODY),
            if selected {
                COLOR_PRIMARY
            } else if response.hovered() {
                COLOR_TEXT
            } else {
                COLOR_MUTED
            },
        );
        control_affordances(ui, &response, row, PRESET_ROW_HEIGHT * 0.5);
        if response.clicked() {
            state.dispatch(Action::SetLightingPreset(preset));
        }
    }
    ui.add_space(SPACE_3);
    let mut brightness = state.light_brightness;
    let row_width = ui.available_width().max(0.0);
    let mut changed = false;
    ui.allocate_ui_with_layout(
        vec2(row_width, CONTROL_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let (icon_rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
            paint_icon(ui.painter(), icon_rect, Icon::LightBulb, COLOR_TEXT);
            changed |= ui
                .add(
                    FilledNumericSlider::new(
                        &mut brightness,
                        MIN_LIGHT_BRIGHTNESS..=MAX_LIGHT_BRIGHTNESS,
                    )
                    .percent()
                    .decimals(0)
                    .min_width(120.0),
                )
                .on_hover_text(text(state.locale, TextKey::Brightness))
                .changed();
        },
    );
    if changed {
        state.dispatch(Action::SetLightBrightness(brightness));
    }

    let mut target_degrees = state.light_yaw_radians.to_degrees().rem_euclid(360.0);
    let row_width = ui.available_width().max(0.0);
    let mut rotation_changed = false;
    ui.allocate_ui_with_layout(
        vec2(row_width, CONTROL_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let (icon_rect, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
            paint_icon(ui.painter(), icon_rect, Icon::LightRotation, COLOR_TEXT);
            let slider = ui.add(
                FilledNumericSlider::new(&mut target_degrees, 0.0..=360.0)
                    .decimals(0)
                    .degrees()
                    .min_width(120.0),
            );
            rotation_changed |= slider.changed();
            crate::ui_components::tooltip(
                slider,
                text(state.locale, TextKey::LightRotation),
                Some(text(state.locale, TextKey::LightRotationGesture)),
            );
        },
    );
    if rotation_changed {
        state.dispatch(Action::RotateLight(light_rotation_delta_radians(
            state.light_yaw_radians,
            target_degrees,
        )));
    }
}

pub(super) fn light_rotation_delta_radians(current_radians: f32, target_degrees: f32) -> f32 {
    target_degrees.to_radians() - current_radians
}

pub(super) fn viewport_tool_panel_height_cache_id(
    panel: ViewportToolPanel,
    mode: BaseViewMode,
) -> Id {
    Id::new((
        "vkit.viewport.tool-panel.measured-height",
        panel,
        mode as u8,
    ))
}

pub(super) fn viewport_tool_panel_desired_height_cache_id(
    panel: ViewportToolPanel,
    mode: BaseViewMode,
) -> Id {
    Id::new(("vkit.viewport.tool-panel.desired-height", panel, mode as u8))
}

pub(super) fn viewport_tool_panel_measure_revision(
    ui: &Ui,
    state: &AppState,
    panel: ViewportToolPanel,
    width: f32,
) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    panel.hash(&mut hasher);

    std::mem::discriminant(&state.locale).hash(&mut hasher);
    width.to_bits().hash(&mut hasher);
    (state.base_view_mode as u8).hash(&mut hasher);
    match panel {
        ViewportToolPanel::Skin => {
            crate::ui::panel_observed_tail(ui, crate::ui::SKIN_PANEL_TAIL)
                .to_bits()
                .hash(&mut hasher);
            state.vam_skin_presets.len().hash(&mut hasher);
            std::mem::discriminant(&state.vam_catalog_status).hash(&mut hasher);
            state.vam_uv_mapping.is_some().hash(&mut hasher);
            state.skin_preview_loading.hash(&mut hasher);
            state.selected_skin_id.is_some().hash(&mut hasher);
            state.skin_preview.is_some().hash(&mut hasher);
            (state.status.key == TextKey::SkinLoadFailed).hash(&mut hasher);
        }
        ViewportToolPanel::BaseView => {
            state.reference_board.images().len().hash(&mut hasher);
            state.reference_board.selected().is_some().hash(&mut hasher);
        }
        ViewportToolPanel::Lighting
        | ViewportToolPanel::Wireframe
        | ViewportToolPanel::Xray
        | ViewportToolPanel::Camera => {}
    }
    hasher.finish()
}

pub(super) fn viewport_tool_panel_measure_revision_cache_id(panel: ViewportToolPanel) -> Id {
    Id::new(("vkit.viewport.tool-panel.measure-revision", panel))
}

pub(super) fn measure_viewport_tool_panel_rect(
    ui: &mut Ui,
    state: &mut AppState,
    viewport: Rect,
    panel: ViewportToolPanel,
    id: Id,
) -> Option<Rect> {
    let placement = viewport_tool_panel_placement(viewport, panel)?;
    let revision = viewport_tool_panel_measure_revision(ui, state, panel, placement.width);
    let revision_id = viewport_tool_panel_measure_revision_cache_id(panel);
    if ui.data(|data| data.get_temp::<u64>(revision_id)) == Some(revision)
        && let Some(desired) = ui.data(|data| {
            data.get_temp::<f32>(viewport_tool_panel_desired_height_cache_id(
                panel,
                state.base_view_mode,
            ))
        })
    {
        let measured_height = desired.min(placement.available_height);
        ui.data_mut(|data| {
            data.insert_temp(
                viewport_tool_panel_height_cache_id(panel, state.base_view_mode),
                measured_height,
            );
        });
        return viewport_tool_panel_rect_with_height(viewport, panel, measured_height);
    }
    let padding_y = viewport_tool_panel_padding_y(panel)?;
    let padding_x = viewport_tool_panel_padding_x();
    let inner_width = (placement.width - padding_x * 2.0).max(0.0);
    let measure_origin = placement.origin + vec2(padding_x, padding_y);
    let mut measure_ui = ui.new_child(
        UiBuilder::new()
            .layer_id(viewport_tool_panel_layer())
            .id_salt(id.with(panel))
            .max_rect(Rect::from_min_size(
                measure_origin,
                vec2(inner_width, 4096.0),
            ))
            .layout(Layout::top_down(Align::Min))
            .invisible(),
    );
    measure_ui.set_min_width(inner_width);
    measure_ui.set_max_width(inner_width);
    publish_panel_lane_limit(&measure_ui, placement.origin.y + placement.available_height);
    draw_viewport_tool_panel_contents(&mut measure_ui, state, panel);

    let content_bottom = measure_ui
        .min_rect()
        .bottom()
        .max(measure_ui.cursor().top());
    let content_height = (content_bottom - measure_origin.y).max(0.0).ceil();
    let desired_height = (content_height + padding_y * 2.0).max(48.0);

    let measured_height = desired_height.min(placement.available_height);
    ui.data_mut(|data| {
        data.insert_temp(
            viewport_tool_panel_height_cache_id(panel, state.base_view_mode),
            measured_height,
        );
        data.insert_temp(
            viewport_tool_panel_desired_height_cache_id(panel, state.base_view_mode),
            desired_height,
        );
        data.insert_temp(revision_id, revision);
    });
    viewport_tool_panel_rect_with_height(viewport, panel, measured_height)
}

pub(super) fn cached_viewport_tool_panel_rect(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
) -> Option<Rect> {
    let panel = state.viewport_tool_panel?;
    let placement = viewport_tool_panel_placement(viewport, panel)?;
    let measured_height = ui.data(|data| {
        data.get_temp::<f32>(viewport_tool_panel_height_cache_id(
            panel,
            state.base_view_mode,
        ))
    });

    let height = measured_height.unwrap_or(placement.available_height);
    viewport_tool_panel_rect_with_height(viewport, panel, height)
}

pub(super) fn viewport_tools_contains(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    pointer: Pos2,
) -> bool {
    viewport_tool_panel_contains(
        viewport,
        cached_viewport_tool_panel_rect(ui, state, viewport),
        pointer,
    ) || viewport_help_contains(ui, state, viewport, pointer)
}

pub(super) fn viewport_tools_pointer_blocked(ui: &Ui, state: &AppState, viewport: Rect) -> bool {
    let (pointer, clicked, primary_down) = ui.input(|input| {
        (
            input.pointer.hover_pos(),
            input.pointer.any_click(),
            input.pointer.button_down(PointerButton::Primary),
        )
    });
    pointer.is_some_and(|pointer| {
        viewport_tools_should_block_pointer(ui, state, viewport, pointer, clicked, primary_down)
    })
}

pub(super) fn viewport_tools_should_block_pointer(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    pointer: Pos2,
    clicked: bool,
    primary_down: bool,
) -> bool {
    viewport_tools_contains(ui, state, viewport, pointer)
        || detail_viewport_controls_contains(state, viewport, pointer)
        || eye_gaze_popup_should_block_pointer(ui, pointer, clicked)
        || crate::viewport::viewport_chrome_covers(ui, pointer)
        || help_card_spends_pointer(state.help_visible, clicked, primary_down)
}

pub(super) const fn help_card_spends_pointer(
    help_visible: bool,
    clicked: bool,
    primary_down: bool,
) -> bool {
    help_visible && (clicked || primary_down)
}

pub(super) fn eye_gaze_popup_should_block_pointer(ui: &Ui, pointer: Pos2, clicked: bool) -> bool {
    let popup_id = Id::new(EYE_GAZE_POPUP_ID);
    egui::Popup::is_id_open(ui.ctx(), popup_id)
        && (clicked
            || ui
                .memory(|memory| memory.area_rect(popup_id))
                .is_some_and(|rect| rect.contains(pointer)))
}

pub(super) fn paint_viewport_tool_icon(
    ui: &Ui,
    rect: Rect,
    panel: ViewportToolPanel,
    color: Color32,
) {
    let icon = match panel {
        ViewportToolPanel::Lighting => Icon::LightBulb,
        ViewportToolPanel::BaseView => Icon::Picture,
        ViewportToolPanel::Wireframe => Icon::Wireframe,
        ViewportToolPanel::Xray => Icon::Xray,
        ViewportToolPanel::Skin => Icon::HeadTexture,
        ViewportToolPanel::Camera => Icon::Camera,
    };
    paint_icon(ui.painter(), rect.shrink(6.0), icon, color);
}

pub(super) fn paint_lighting_swatch(ui: &Ui, rect: Rect, preset: LightingPreset) {
    let (left, right) = preset.swatch();
    let strips = 12;
    for index in 0..strips {
        let t = index as f32 / (strips - 1) as f32;
        let mix = |a: u8, b: u8| ((f32::from(a) * (1.0 - t) + f32::from(b) * t).round()) as u8;
        let color = Color32::from_rgb(
            mix(left[0], right[0]),
            mix(left[1], right[1]),
            mix(left[2], right[2]),
        );
        let x0 = egui::lerp(rect.left()..=rect.right(), index as f32 / strips as f32);
        let x1 = egui::lerp(
            rect.left()..=rect.right(),
            (index + 1) as f32 / strips as f32,
        );
        ui.painter().rect_filled(
            Rect::from_min_max(pos2(x0, rect.top()), pos2(x1 + 0.5, rect.bottom())),
            if index == 0 || index + 1 == strips {
                5.0
            } else {
                0.0
            },
            color,
        );
    }
}
