use super::*;

pub(super) fn draw_title_vam_field(ui: &mut Ui, state: &mut AppState, rect: Rect) {
    let has_root = state.vam_root.is_some();
    let raw = state
        .vam_root
        .as_deref()
        .map(readable_windows_path)
        .unwrap_or_else(|| text(state.locale, TextKey::VaMFolder).to_owned());

    let label = crate::ui_components::ellipsize_to_width(
        ui,
        &raw,
        rect.width() - crate::theme::CONTROL_HEIGHT - 12.0,
        FontId::proportional(FONT_SM),
    );
    let mut field_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    set_capsule_widget_radius(&mut field_ui);

    let response = attention_widget(
        &mut field_ui,
        attention_target_id(AttentionTarget::VaMRoot),
        rect.size(),
        CapsuleFieldButton::new(&label, has_root)
            .dark()
            .with_trailing_icon(crate::ui_components::Icon::Folder),
    )
    .on_hover_text(text(state.locale, TextKey::VaMFolder));

    if guiding(state, crate::guidance::NextStep::ChooseVaMFolder) {
        guide_glow_over(ui, rect, capsule_radius_for(rect));
    }
    if response.clicked() {
        state.dispatch(Action::RequestVaMRootBrowse);
    }
}

pub(super) fn top_tab_cell(tabs_rect: Rect, tab_width: f32, index: usize) -> Rect {
    Rect::from_min_size(
        pos2(
            tabs_rect.left() + (tab_width + TOP_TAB_GAP) * index as f32,
            tabs_rect.top(),
        ),
        vec2(tab_width, TOP_TAB_HEIGHT),
    )
}

pub(super) fn top_tab_strip_width(tab_width: f32) -> f32 {
    let count = TOP_TABS.len() as f32;
    tab_width * count + TOP_TAB_GAP * (count - 1.0)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CaptionLayout {
    pub(super) bar_rect: Rect,
    pub(super) brand_rect: Rect,
    pub(super) vam_rect: Rect,
    pub(super) tabs_rect: Rect,
    pub(super) tab_width: f32,
    pub(super) settings_rect: Rect,
    pub(super) controls_rect: Rect,
    pub(super) update_rect: Option<Rect>,
}

pub(super) fn publish_non_client_layout(ui: &Ui, bar: CaptionLayout) {
    let CaptionLayout {
        bar_rect,
        brand_rect: _,
        vam_rect,
        tabs_rect,
        tab_width,
        settings_rect,
        controls_rect,
        update_rect,
    } = bar;
    if !window_control::nc_subclass_active() {
        return;
    }
    let mut carve_outs = Vec::with_capacity(TOP_TABS.len() + 3);
    carve_outs.push(NcRect::from_egui(vam_rect));
    for index in 0..TOP_TABS.len() {
        carve_outs.push(NcRect::from_egui(top_tab_cell(tabs_rect, tab_width, index)));
    }
    if let Some(update_rect) = update_rect {
        carve_outs.push(NcRect::from_egui(update_rect));
    }
    carve_outs.push(NcRect::from_egui(settings_rect));
    window_control::publish_nc_layout(NcLayout {
        pixels_per_point: ui.ctx().pixels_per_point(),
        titlebar_height: bar_rect.bottom(),
        caption_buttons: window_button_cells(controls_rect).map(NcRect::from_egui),
        carve_outs,
    });
}

pub(super) const TOP_TAB_FLOOR: f32 = 56.0;

pub(super) fn caption_layout(
    bar: Rect,
    update_width: f32,
    title_end: Option<f32>,
) -> CaptionLayout {
    let layout = caption_layout_for(bar, update_width, title_end);
    if update_width > 0.0 && layout.tab_width < TOP_TAB_FLOOR {
        return caption_layout_for(bar, 0.0, title_end);
    }
    layout
}

fn caption_layout_for(bar: Rect, update_width: f32, title_end: Option<f32>) -> CaptionLayout {
    let controls_rect = Rect::from_min_max(
        pos2(
            bar.right() - TITLE_WINDOW_BUTTON_WIDTH * 3.0,
            bar.top() + 6.0,
        ),
        bar.max,
    );
    let settings_rect = Rect::from_min_size(
        pos2(
            controls_rect.left() - crate::theme::TITLE_SETTINGS_SIZE - 8.0,
            bar.top() + 8.0,
        ),
        Vec2::splat(crate::theme::TITLE_SETTINGS_SIZE),
    );
    let brand_rect = brand_cell(bar, update_width);
    let vam_rect = Rect::from_min_size(
        pos2(brand_rect.right() + 8.0, bar.top() + 6.0),
        vec2(TITLE_VAM_FIELD_WIDTH, TOP_TAB_HEIGHT),
    );

    let tabs_left = vam_rect.right() + crate::theme::TITLE_VAM_TAB_GAP;
    let tabs_area = Rect::from_min_max(
        pos2(tabs_left, bar.top()),
        pos2((settings_rect.left() - 8.0).max(tabs_left), bar.bottom()),
    );
    let tab_count = TOP_TABS.len() as f32;
    let total_gap = TOP_TAB_GAP * (tab_count - 1.0);
    let tab_width = ((tabs_area.width() - total_gap) / tab_count).clamp(0.0, TOP_TAB_WIDTH);

    let half = top_tab_strip_width(tab_width) * 0.5;
    let lower = tabs_area.left() + half;
    let center_x = bar
        .center()
        .x
        .clamp(lower, (tabs_area.right() - half).max(lower));
    let tabs_rect = Rect::from_center_size(
        pos2(center_x, tabs_area.center().y),
        vec2(top_tab_strip_width(tab_width), TOP_TAB_HEIGHT),
    );

    CaptionLayout {
        bar_rect: bar,
        brand_rect,
        vam_rect,
        tabs_rect,
        tab_width,
        settings_rect,
        controls_rect,
        update_rect: title_update_rect_after(brand_rect, update_width, title_end),
    }
}

pub(super) fn title_update_width(ui: &Ui, locale: crate::i18n::Locale) -> f32 {
    if crate::update_check::newer_release().is_none() {
        return 0.0;
    }
    TITLE_UPDATE_GAP + TITLE_UPDATE_DIAMETER + title_update_grown_width(ui, locale)
}

fn title_update_grown_width(ui: &Ui, locale: crate::i18n::Locale) -> f32 {
    let label = text(locale, TextKey::UpdateAvailable);
    ui.painter()
        .layout_no_wrap(label.to_owned(), FontId::proportional(FONT_SM), COLOR_TEXT)
        .size()
        .x
        + TITLE_UPDATE_PADDING
}

#[cfg(test)]
pub(super) fn title_update_rect(brand: Rect, width: f32) -> Option<Rect> {
    title_update_rect_after(brand, width, None)
}

pub(super) fn title_update_rect_after(
    brand: Rect,
    width: f32,
    title_end: Option<f32>,
) -> Option<Rect> {
    (width > 0.0).then(|| {
        let left = title_end.map_or(brand.right() - width + TITLE_UPDATE_GAP, |end| {
            (end + TITLE_UPDATE_GAP).min(brand.right() - width + TITLE_UPDATE_GAP)
        });
        Rect::from_min_size(
            pos2(left, brand.center().y - TITLE_UPDATE_DIAMETER * 0.5),
            Vec2::splat(TITLE_UPDATE_DIAMETER),
        )
    })
}

pub(super) fn brand_cell(bar: Rect, update_width: f32) -> Rect {
    Rect::from_min_size(
        pos2(bar.left() + 10.0, bar.top() + 6.0),
        vec2(TITLE_BRAND_WIDTH + update_width, TOP_TAB_HEIGHT),
    )
}

pub(super) fn title_text_end(ui: &Ui, brand: Rect) -> f32 {
    let mark_right = brand.left() + 14.0 + 12.0;
    let text = ui.painter().layout_no_wrap(
        crate::APP_TITLE.to_owned(),
        FontId::proportional(FONT_BODY),
        COLOR_TEXT,
    );
    mark_right + 8.0 + text.size().x
}

pub(super) const TITLE_UPDATE_GAP: f32 = 10.0;

const TITLE_UPDATE_DIAMETER: f32 = 22.0;

const TITLE_UPDATE_GLYPH: f32 = 20.0;

const TITLE_UPDATE_PADDING: f32 = 9.0;

const TITLE_UPDATE_EXPAND_SECONDS: f32 = 0.14;

pub(super) fn draw_title_update_capsule(ui: &mut Ui, state: &AppState, collapsed: Rect) {
    let Some(tag) = crate::update_check::newer_release() else {
        return;
    };
    let id = Id::new("vkit.title.update");
    let label = text(state.locale, TextKey::UpdateAvailable);
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), FontId::proportional(FONT_SM), COLOR_TEXT);

    let hovered = ui.rect_contains_pointer(collapsed);
    let openness =
        ui.ctx()
            .animate_bool_with_time(id.with("open"), hovered, TITLE_UPDATE_EXPAND_SECONDS);

    let grown = title_update_grown_width(ui, state.locale);
    let rect = Rect::from_min_size(
        collapsed.min,
        vec2(collapsed.width() + grown * openness, collapsed.height()),
    );
    let painter = if openness > 0.0 {
        ui.painter().clone().with_layer_id(egui::LayerId::new(
            egui::Order::Foreground,
            id.with("layer"),
        ))
    } else {
        ui.painter().clone()
    };

    let response = ui.interact(rect, id, Sense::click());
    let radius = rect.height() * 0.5;
    if openness > 0.0 {
        let fill = if hovered {
            COLOR_SURFACE_HOVER
        } else {
            COLOR_SURFACE_RAISED
        };
        painter.rect_filled(rect, radius, fill);
    }
    let ink = if hovered { COLOR_TEXT } else { COLOR_MUTED };
    paint_icon(
        &painter,
        Rect::from_center_size(collapsed.center(), Vec2::splat(TITLE_UPDATE_GLYPH)),
        Icon::UpdateAvailable,
        ink,
    );
    if openness > 0.0 {
        painter.galley(
            pos2(
                collapsed.right() + TITLE_UPDATE_PADDING * 0.5,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            ink,
        );
    }

    let response = response.on_hover_text(format!("{} \u{2192} {tag}", crate::APP_TITLE));
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        crate::settings::open_with_shell(crate::update_check::RELEASES_PAGE);
    }
}

pub(super) fn paint_title_brand(ui: &Ui, rect: Rect) {
    let mark = Rect::from_center_size(pos2(rect.left() + 14.0, rect.center().y), Vec2::splat(24.0));
    let texture = title_logo_texture(ui);
    ui.painter().image(
        texture.id(),
        mark,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    ui.painter().text(
        pos2(mark.right() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        crate::APP_TITLE,
        FontId::proportional(FONT_BODY),
        COLOR_TEXT,
    );
}

pub(super) fn title_logo_texture(ui: &Ui) -> TextureHandle {
    let id = Id::new("vkit.title.logo.texture");
    if let Some(texture) = ui.ctx().data(|data| data.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    let decoded = image::load_from_memory(include_bytes!("../../resources/logo.png"))
        .expect("embedded Vkit logo must be a valid image")
        .to_rgba8();
    let size = [decoded.width() as usize, decoded.height() as usize];
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, decoded.as_raw());
    let texture = ui
        .ctx()
        .load_texture("vkit-title-logo", color_image, TextureOptions::LINEAR);
    ui.ctx()
        .data_mut(|data| data.insert_temp(id, texture.clone()));
    texture
}

pub(super) fn title_drag_region(ui: &mut Ui, rect: Rect, id_salt: &'static str) {
    if window_control::nc_subclass_active() {
        return;
    }
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let response = ui.interact(
        rect,
        Id::new(("vkit.title.drag", id_salt)),
        Sense::click_and_drag(),
    );
    match title_drag_gesture(response.double_clicked(), response.drag_started()) {
        Some(TitleDragGesture::ToggleMaximized) => {
            let maximized = ui
                .ctx()
                .input(|input| input.viewport().maximized.unwrap_or(false));
            ui.ctx()
                .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        }
        Some(TitleDragGesture::StartDrag) => {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        None => {}
    }
}

pub(super) const fn title_drag_gesture(
    double_clicked: bool,
    drag_started: bool,
) -> Option<TitleDragGesture> {
    if double_clicked {
        Some(TitleDragGesture::ToggleMaximized)
    } else if drag_started {
        Some(TitleDragGesture::StartDrag)
    } else {
        None
    }
}

pub(super) fn window_button_cells(rect: Rect) -> [Rect; 3] {
    [0.0_f32, 1.0, 2.0].map(|index| {
        Rect::from_min_size(
            pos2(rect.left() + TITLE_WINDOW_BUTTON_WIDTH * index, rect.top()),
            vec2(TITLE_WINDOW_BUTTON_WIDTH, TOP_TAB_HEIGHT),
        )
    })
}

pub(super) fn draw_window_buttons(ui: &mut Ui, state: &AppState, rect: Rect) {
    let maximized = ui
        .ctx()
        .input(|input| input.viewport().maximized.unwrap_or(false));
    let cells = window_button_cells(rect);
    let icons = [
        WindowControlIcon::Minimize,
        if maximized {
            WindowControlIcon::Restore
        } else {
            WindowControlIcon::Maximize
        },
        WindowControlIcon::Close,
    ];

    if window_control::nc_subclass_active() {
        let hovered = window_control::hovered_caption_button();
        let pressed = window_control::pressed_caption_button();
        for ((cell, icon), button) in cells.iter().zip(icons).zip(CaptionButton::ALL) {
            let is_hovered = hovered == Some(button);
            let is_pressed = pressed == Some(button);
            paint_window_button(ui, *cell, icon, is_hovered, is_pressed && is_hovered);
        }
        return;
    }

    let minimize = ui
        .put(
            cells[0],
            Button::new("").frame(false).min_size(cells[0].size()),
        )
        .on_hover_text(text(state.locale, TextKey::WindowMinimize));
    let maximize = ui
        .put(
            cells[1],
            Button::new("").frame(false).min_size(cells[1].size()),
        )
        .on_hover_text(text(
            state.locale,
            if maximized {
                TextKey::WindowRestore
            } else {
                TextKey::WindowMaximize
            },
        ));
    let close = ui
        .put(
            cells[2],
            Button::new("").frame(false).min_size(cells[2].size()),
        )
        .on_hover_text(text(state.locale, TextKey::WindowClose));
    for (cell, (icon, response)) in cells
        .iter()
        .zip(icons.into_iter().zip([&minimize, &maximize, &close]))
    {
        paint_window_button(
            ui,
            *cell,
            icon,
            response.hovered(),
            response.is_pointer_button_down_on(),
        );
    }
    if minimize.clicked() {
        ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
    }
    if maximize.clicked() {
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    }
    if close.clicked() {
        ui.ctx().send_viewport_cmd(ViewportCommand::Close);
    }
}

pub(super) fn window_button_highlight_rect(cell: Rect) -> Rect {
    let side = cell.width().min(cell.height()) - crate::theme::SPACE_1 * 2.0;
    Rect::from_center_size(cell.center(), Vec2::splat(side.max(0.0)))
}

pub(super) fn paint_window_button(
    ui: &Ui,
    cell: Rect,
    icon: WindowControlIcon,
    hovered: bool,
    pressed: bool,
) {
    let close = matches!(icon, WindowControlIcon::Close);
    let background = if pressed {
        Some(if close {
            COLOR_CLOSE_PRESSED
        } else {
            COLOR_SURFACE_RAISED
        })
    } else if hovered {
        Some(if close {
            COLOR_CLOSE_HOVER
        } else {
            COLOR_SURFACE_HOVER
        })
    } else {
        None
    };
    if let Some(fill) = background {
        ui.painter().rect_filled(
            window_button_highlight_rect(cell),
            crate::theme::RADIUS_M,
            fill,
        );
    }
    let color = if close && (hovered || pressed) {
        COLOR_PRIMARY
    } else if hovered || pressed {
        COLOR_TEXT
    } else {
        COLOR_MUTED
    };
    paint_window_control_icon(ui, cell, icon, color);
}

#[derive(Clone, Copy)]
pub(super) enum WindowControlIcon {
    Minimize,
    Maximize,
    Restore,
    Close,
}

pub(super) fn paint_window_control_icon(
    ui: &Ui,
    cell: Rect,
    icon: WindowControlIcon,
    color: Color32,
) {
    let glyph = match icon {
        WindowControlIcon::Minimize => Icon::WindowMinimize,
        WindowControlIcon::Maximize => Icon::WindowMaximize,
        WindowControlIcon::Restore => Icon::WindowRestore,
        WindowControlIcon::Close => Icon::WindowClose,
    };

    paint_icon(
        ui.painter(),
        Rect::from_center_size(cell.center(), Vec2::splat(24.0)),
        glyph,
        color,
    );
}

pub(super) fn draw_window_resize_zones(root: &mut Ui) {
    const GRAB: f32 = 6.0;
    if window_control::nc_subclass_active() {
        return;
    }
    let maximized = root
        .ctx()
        .input(|input| input.viewport().maximized.unwrap_or(false));
    if maximized {
        return;
    }
    let rect = root.max_rect();
    if rect.width() < GRAB * 2.0 || rect.height() < GRAB * 2.0 {
        return;
    }
    for (index, (zone, direction)) in window_resize_zones(rect).into_iter().enumerate() {
        let response = root
            .interact(zone, Id::new(("vkit.window.resize", index)), Sense::drag())
            .on_hover_cursor(resize_cursor(direction));
        if response.drag_started() {
            root.ctx()
                .send_viewport_cmd(ViewportCommand::BeginResize(direction));
        }
    }
}

pub(super) const fn resize_cursor(direction: ResizeDirection) -> egui::CursorIcon {
    match direction {
        ResizeDirection::East | ResizeDirection::West => egui::CursorIcon::ResizeHorizontal,
        ResizeDirection::North | ResizeDirection::South => egui::CursorIcon::ResizeVertical,
        ResizeDirection::NorthEast | ResizeDirection::SouthWest => egui::CursorIcon::ResizeNeSw,
        ResizeDirection::NorthWest | ResizeDirection::SouthEast => egui::CursorIcon::ResizeNwSe,
    }
}

pub(super) fn window_resize_zones(rect: Rect) -> [(Rect, ResizeDirection); 8] {
    const GRAB: f32 = 6.0;

    let vertical_top = (rect.top() + TOP_BAR_HEIGHT).min(rect.bottom() - GRAB);
    [
        (
            Rect::from_min_max(rect.min, pos2(rect.left() + GRAB, rect.top() + GRAB)),
            ResizeDirection::NorthWest,
        ),
        (
            Rect::from_min_max(
                pos2(rect.left() + GRAB, rect.top()),
                pos2(rect.right() - GRAB, rect.top() + GRAB),
            ),
            ResizeDirection::North,
        ),
        (
            Rect::from_min_max(
                pos2(rect.right() - GRAB, rect.top()),
                pos2(rect.right(), rect.top() + GRAB),
            ),
            ResizeDirection::NorthEast,
        ),
        (
            Rect::from_min_max(
                pos2(rect.right() - GRAB, vertical_top),
                pos2(rect.right(), rect.bottom() - GRAB),
            ),
            ResizeDirection::East,
        ),
        (
            Rect::from_min_max(pos2(rect.right() - GRAB, rect.bottom() - GRAB), rect.max),
            ResizeDirection::SouthEast,
        ),
        (
            Rect::from_min_max(
                pos2(rect.left() + GRAB, rect.bottom() - GRAB),
                pos2(rect.right() - GRAB, rect.bottom()),
            ),
            ResizeDirection::South,
        ),
        (
            Rect::from_min_max(
                pos2(rect.left(), rect.bottom() - GRAB),
                pos2(rect.left() + GRAB, rect.bottom()),
            ),
            ResizeDirection::SouthWest,
        ),
        (
            Rect::from_min_max(
                pos2(rect.left(), vertical_top),
                pos2(rect.left() + GRAB, rect.bottom() - GRAB),
            ),
            ResizeDirection::West,
        ),
    ]
}
