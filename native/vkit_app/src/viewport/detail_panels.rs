use super::*;

pub(super) fn draw_eye_gaze_popup(
    ui: &mut Ui,
    state: &mut AppState,
    response: &Response,
    viewport: Rect,
) {
    let popup_id = Id::new(EYE_GAZE_POPUP_ID);
    let open = egui::Popup::from_toggle_button_response(response)
        .id(popup_id)
        .frame(
            egui::Frame::new()
                .fill(COLOR_TOPBAR)
                .stroke(Stroke::new(1.0, COLOR_BORDER))
                .corner_radius(f32::from(crate::theme::RADIUS_POPOVER))
                .inner_margin(egui::Margin::same(8)),
        )
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(EYE_GAZE_GRID_SIZE);
            let (grid, grid_response) =
                ui.allocate_exact_size(Vec2::splat(EYE_GAZE_GRID_SIZE), Sense::click_and_drag());
            let mut gaze = viewport_eye_gaze(ui, state, viewport);
            if (grid_response.clicked() || grid_response.dragged())
                && let Some(pointer) = grid_response.interact_pointer_pos()
            {
                gaze = gaze_from_screen(pointer, grid);
                state.dispatch(Action::SetManualEyeGaze(gaze));
                if !state.sculpt_eye_tracking {
                    state.dispatch(Action::ToggleSculptEyeTracking(true));
                }
            }

            ui.painter()
                .rect_filled(grid, 8.0, COLOR_SURFACE_RAISED.gamma_multiply(0.82));
            ui.painter().rect_stroke(
                grid,
                8.0,
                Stroke::new(1.0, COLOR_BORDER),
                egui::StrokeKind::Inside,
            );
            for fraction in [0.25, 0.5, 0.75] {
                let x = egui::lerp(grid.x_range(), fraction);
                let y = egui::lerp(grid.y_range(), fraction);
                let stroke = Stroke::new(
                    if fraction == 0.5 { 1.1 } else { 0.7 },
                    if fraction == 0.5 {
                        COLOR_MUTED
                    } else {
                        COLOR_BORDER.gamma_multiply(0.7)
                    },
                );
                ui.painter()
                    .line_segment([pos2(x, grid.top()), pos2(x, grid.bottom())], stroke);
                ui.painter()
                    .line_segment([pos2(grid.left(), y), pos2(grid.right(), y)], stroke);
            }
            let marker = pos2(
                grid.center().x + gaze[0] * grid.width() * 0.5,
                grid.center().y - gaze[1] * grid.height() * 0.5,
            );
            ui.painter().circle_filled(marker, 5.0, COLOR_PRIMARY);
            ui.painter()
                .circle_stroke(marker, 5.0, Stroke::new(1.0, COLOR_TEXT));

            ui.add_space(SPACE_3);
            let gap = ui.spacing().item_spacing.x;
            let width = ((ui.available_width() - gap).max(0.0) * 0.5).max(0.0);
            ui.horizontal(|ui| {
                let auto = state.eye_gaze_mode == EyeGazeMode::AutoCursor;
                if ui
                    .add_sized(
                        [width, crate::theme::CONTROL_H_COMPACT],
                        egui::Button::new(text(state.locale, TextKey::EyeTrackingAuto))
                            .selected(auto)
                            .corner_radius(crate::theme::CONTROL_H_COMPACT * 0.5),
                    )
                    .clicked()
                {
                    state.dispatch(Action::SetEyeGazeMode(EyeGazeMode::AutoCursor));
                    if !state.sculpt_eye_tracking {
                        state.dispatch(Action::ToggleSculptEyeTracking(true));
                    }
                }
                if ui
                    .add_sized(
                        [width, crate::theme::CONTROL_H_COMPACT],
                        egui::Button::new(text(state.locale, TextKey::Reset))
                            .corner_radius(crate::theme::CONTROL_H_COMPACT * 0.5),
                    )
                    .clicked()
                {
                    state.dispatch(Action::ResetEyeGaze);
                }
            });
            ui.add_space(SPACE_2);
            if ui
                .add_sized(
                    [ui.available_width(), crate::theme::CONTROL_H_COMPACT],
                    egui::Button::new(text(state.locale, TextKey::EyeGazeFreeze))
                        .corner_radius(crate::theme::CONTROL_H_COMPACT * 0.5),
                )
                .on_hover_text(text(state.locale, TextKey::EyeGazeFreezeTooltip))
                .clicked()
            {
                state.dispatch(Action::FreezeEyeGaze(gaze));
            }
        })
        .is_some();

    let was_open_id = popup_id.with("was-open");
    let was_open = ui
        .ctx()
        .data_mut(|data| data.get_temp::<bool>(was_open_id).unwrap_or(false));
    ui.ctx()
        .data_mut(|data| data.insert_temp(was_open_id, open));
    if was_open && !open && state.sculpt_eye_tracking && eye_gaze_is_untouched(state) {
        state.dispatch(Action::ToggleSculptEyeTracking(false));
    }
}

fn eye_gaze_is_untouched(state: &AppState) -> bool {
    state.eye_gaze_mode == crate::state::EyeGazeMode::Manual
        && state.frozen_eye_gaze.is_none()
        && state.manual_eye_gaze == [0.0; 2]
}

pub(super) const DETAIL_HUD_WIDE_LABEL_CONTROL_WIDTH: f32 = 200.0;

pub(super) fn detail_numeric_label_width(text_width: f32, control_width: f32) -> f32 {
    let cap = if control_width >= DETAIL_HUD_WIDE_LABEL_CONTROL_WIDTH {
        64.0
    } else {
        44.0
    };
    (text_width + 2.0).min(cap)
}

pub(super) struct NumericFormat {
    pub range: std::ops::RangeInclusive<f32>,
    pub decimals: usize,
    pub percent: bool,
}

pub(super) fn detail_numeric_control(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    format: NumericFormat,
    width: f32,
    shortcut: Option<&str>,
) -> bool {
    let NumericFormat {
        range,
        decimals,
        percent,
    } = format;
    let mut changed = false;
    let text_width = ui
        .painter()
        .layout_no_wrap(
            label.to_owned(),
            FontId::proportional(crate::theme::BODY_FONT_SIZE),
            COLOR_MUTED,
        )
        .size()
        .x;
    let label_width = detail_numeric_label_width(text_width, width);
    ui.allocate_ui_with_layout(
        vec2(width, crate::theme::CONTROL_H_DENSE),
        Layout::left_to_right(Align::Center),
        |ui| {
            let label_response = ui.add_sized(
                [label_width, 20.0],
                egui::Label::new(
                    RichText::new(label)
                        .size(crate::theme::BODY_FONT_SIZE)
                        .color(COLOR_MUTED),
                )
                .truncate()
                .sense(Sense::hover()),
            );
            let slider = FilledNumericSlider::new(value, range)
                .decimals(decimals)
                .min_width(180.0)
                .value_gap(0.0);
            let slider = ui.add(if percent { slider.percent() } else { slider });
            changed = slider.changed();
            crate::ui_components::tooltip(label_response | slider, label, shortcut);
        },
    );
    changed
}

pub(super) fn draw_detail_group_panel(ui: &mut Ui, state: &mut AppState, rect: Rect) {
    if state.sculpt_groups_collapsed {
        draw_collapsed_detail_groups(ui, state, rect);
        return;
    }
    let id = Id::new("vkit.viewport.detail.groups");
    let _blocker = ui.interact(rect, id.with("blocker"), Sense::click_and_drag());

    let mut host = ui.new_child(
        UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    egui::Frame::new()
        .fill(COLOR_TOPBAR.gamma_multiply(0.96))
        .corner_radius(f32::from(crate::theme::RADIUS_POPOVER))
        .inner_margin(egui::Margin {
            left: DETAIL_GROUP_INSET_X as i8,
            right: DETAIL_GROUP_INSET_X as i8,
            top: DETAIL_GROUP_INSET_Y_TOP as i8,
            bottom: DETAIL_GROUP_INSET_Y as i8,
        })
        .show(&mut host, |ui| {
            ui.set_width(rect.width() - DETAIL_GROUP_INSET_X * 2.0);
            ui.spacing_mut().item_spacing.y = DETAIL_GROUP_ITEM_GAP;

            let (header_rect, _) = ui.allocate_exact_size(
                vec2(ui.available_width(), crate::theme::CONTROL_H_COMPACT),
                Sense::hover(),
            );
            let title = ui.put(
                header_rect,
                egui::Label::new(
                    RichText::new(text(state.locale, TextKey::TransformGroups))
                        .size(FONT_SM)
                        .strong()
                        .color(COLOR_TEXT),
                )
                .selectable(false)
                .sense(Sense::drag()),
            );
            crate::ui_components::island_move_handle(
                &title,
                rect,
                &mut state.detail_group_panel_pos,
            );

            let chevron_rect = Rect::from_min_size(
                pos2(header_rect.right() - 24.0, header_rect.center().y - 12.0),
                Vec2::splat(24.0),
            );
            let collapse = detail_icon_toggle(
                &mut ui.new_child(UiBuilder::new().max_rect(chevron_rect)),
                Icon::ChevronRight,
                false,
                text(state.locale, TextKey::CollapseTransformGroups),
            );
            if collapse.clicked() {
                anchor_detail_panel_to_right_corner(&mut state.detail_group_panel_pos, true);
                state.dispatch(Action::SetSculptGroupsCollapsed(true));
            }
            for target in SculptTarget::ALL {
                draw_detail_group_row(ui, state, target);
            }
        });
}

pub(super) fn draw_collapsed_detail_groups(ui: &mut Ui, state: &mut AppState, rect: Rect) {
    let id = Id::new("vkit.viewport.detail.groups.collapsed");
    ui.painter().rect_filled(
        rect,
        f32::from(crate::theme::RADIUS_POPOVER),
        COLOR_TOPBAR.gamma_multiply(0.96),
    );
    let _blocker = ui.interact(rect, id.with("blocker"), Sense::click_and_drag());
    let content_rect = Rect::from_min_max(
        rect.min + vec2(DETAIL_GROUP_INSET_X, DETAIL_GROUP_INSET_Y_TOP),
        rect.max - vec2(DETAIL_GROUP_INSET_X, DETAIL_GROUP_INSET_Y),
    );
    let mut panel = ui.new_child(
        UiBuilder::new()
            .id_salt(id)
            .max_rect(content_rect)
            .layout(Layout::top_down(Align::Min)),
    );
    panel.spacing_mut().item_spacing.y = 0.0;

    let (handle_row, _) =
        panel.allocate_exact_size(vec2(panel.available_width(), 24.0), Sense::hover());
    let chevron_rect = Rect::from_min_size(
        pos2(handle_row.left(), handle_row.center().y - 12.0),
        Vec2::splat(24.0),
    );
    let grab_rect = Rect::from_min_max(
        pos2(chevron_rect.right() + 4.0, handle_row.top()),
        handle_row.max,
    );
    let grab = panel.interact(grab_rect, id.with("grab"), Sense::drag());
    let pill = Rect::from_center_size(grab_rect.center(), vec2(grab_rect.width().min(26.0), 5.0));
    let pill_color = if grab.hovered() || grab.dragged() {
        COLOR_TEXT.gamma_multiply(0.55)
    } else {
        COLOR_MUTED.gamma_multiply(0.5)
    };
    panel.painter().rect_filled(pill, 2.5, pill_color);
    crate::ui_components::island_move_handle(&grab, rect, &mut state.detail_group_panel_pos);
    let expand = detail_icon_toggle(
        &mut panel.new_child(UiBuilder::new().max_rect(chevron_rect)),
        Icon::ChevronLeft,
        false,
        text(state.locale, TextKey::ExpandTransformGroups),
    );
    if expand.clicked() {
        anchor_detail_panel_to_right_corner(&mut state.detail_group_panel_pos, false);
        state.dispatch(Action::SetSculptGroupsCollapsed(false));
    }
    panel.add_space(DETAIL_GROUP_ITEM_GAP);

    for (index, target) in SculptTarget::ALL.into_iter().enumerate() {
        let editable = state.sculpt.editable_targets().contains(target);
        let previous_editable = index > 0
            && state
                .sculpt
                .editable_targets()
                .contains(SculptTarget::ALL[index - 1]);
        let next_editable = index + 1 < SculptTarget::ALL.len()
            && state
                .sculpt
                .editable_targets()
                .contains(SculptTarget::ALL[index + 1]);
        let label = text(state.locale, sculpt_target_text_key(target));
        let (row, response) = panel.allocate_exact_size(
            vec2(
                panel.available_width().max(0.0),
                crate::theme::CONTROL_H_DENSE,
            ),
            Sense::click(),
        );
        let fill = if editable {
            crate::theme::COLOR_ACTIVE_BG
        } else if response.hovered() {
            crate::theme::COLOR_SURFACE_HOVER
        } else {
            Color32::TRANSPARENT
        };
        let radius = crate::theme::CONTROL_RADIUS;
        let corners = if editable {
            CornerRadius {
                nw: if previous_editable { 0 } else { radius },
                ne: if previous_editable { 0 } else { radius },
                sw: if next_editable { 0 } else { radius },
                se: if next_editable { 0 } else { radius },
            }
        } else {
            CornerRadius::same(radius)
        };
        panel.painter().rect_filled(row, corners, fill);
        panel.painter().with_clip_rect(row.shrink(4.0)).text(
            row.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(FONT_XS),
            if editable {
                crate::theme::COLOR_ACTIVE_INK
            } else {
                COLOR_TEXT
            },
        );
        let response = response.on_hover_text(label);
        if response.clicked() {
            state.dispatch(Action::SetSculptTarget {
                target,
                enabled: !editable,
            });
        }
    }
}

pub(super) const fn sculpt_target_text_key(target: SculptTarget) -> TextKey {
    match target {
        SculptTarget::HeadSkin => TextKey::Skin,
        SculptTarget::Tear => TextKey::SculptTear,
        SculptTarget::Eyelashes => TextKey::Eyelashes,
        SculptTarget::Eyes => TextKey::SculptEyes,
        SculptTarget::Lips => TextKey::SculptLips,
        SculptTarget::TeethTongue => TextKey::SculptTeethTongue,
        SculptTarget::InnerMouth => TextKey::SculptInnerMouth,
    }
}

pub(super) fn draw_detail_group_row(ui: &mut Ui, state: &mut AppState, target: SculptTarget) {
    let drag_state_id = Id::new(DETAIL_GROUP_PAINT);
    let label = sculpt_target_text_key(target);
    let visible = state.sculpt.visible_targets().contains(target);
    let editable = state.sculpt.editable_targets().contains(target);
    let width = ui.available_width().max(0.0);
    let (row, row_response) = ui.allocate_exact_size(
        vec2(width, crate::theme::CONTROL_H_DENSE),
        Sense::click_and_drag(),
    );
    if editable {
        ui.painter().rect_filled(
            row,
            row.height() * 0.5,
            COLOR_VIEWPORT_TOOL.gamma_multiply(0.88),
        );
    }
    let mut row_ui = ui.new_child(
        UiBuilder::new()
            .id_salt(("detail-group", target as u8))
            .max_rect(row.shrink2(vec2(8.0, 3.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    row_ui.spacing_mut().item_spacing.x = 4.0;
    let label_width = (row_ui.available_width() - 58.0).max(48.0);
    let label_response = row_ui.add_sized(
        [label_width, 24.0],
        egui::Label::new(
            RichText::new(text(state.locale, label))
                .size(FONT_SM)
                .color(if editable { COLOR_TEXT } else { COLOR_MUTED }),
        )
        .truncate()
        .selectable(false)
        .sense(Sense::click_and_drag()),
    );
    let visibility = detail_icon_toggle(
        &mut row_ui,
        if visible {
            Icon::EyeOpen
        } else {
            Icon::EyeClosed
        },
        visible,
        text(
            state.locale,
            if visible {
                TextKey::TooltipHide
            } else {
                TextKey::TooltipShow
            },
        ),
    );
    if visibility.clicked() {
        state.sculpt.set_visible_target_enabled(target, !visible);
        ui.ctx().request_repaint();
    }
    let lock = detail_icon_toggle(
        &mut row_ui,
        transform_group_editability_icon(editable),
        editable,
        text(
            state.locale,
            if editable {
                TextKey::TooltipLock
            } else {
                TextKey::TooltipUnlock
            },
        ),
    );
    let pointer_down = ui.input(|input| input.pointer.primary_down());
    let drag_started = row_response.drag_started() || label_response.drag_started();
    if drag_started {
        ui.ctx()
            .data_mut(|data| data.insert_temp(drag_state_id, !editable));
    } else if !pointer_down {
        ui.ctx().data_mut(|data| data.remove::<bool>(drag_state_id));
    }
    let pointer_over_row = ui
        .input(|input| input.pointer.interact_pos())
        .is_some_and(|pointer| row.contains(pointer));
    let painted_value = pointer_down
        .then(|| ui.ctx().data(|data| data.get_temp::<bool>(drag_state_id)))
        .flatten()
        .filter(|_| pointer_over_row && !visibility.hovered() && !lock.hovered());
    if let Some(enabled) = painted_value {
        if enabled != editable {
            state.dispatch(Action::SetSculptTarget { target, enabled });
        }
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        ui.ctx().request_repaint();
        return;
    }

    let solo_click = crate::shortcuts::Shortcut::ListSoloHold.held(ui)
        && (row_response.clicked() || label_response.clicked() || lock.clicked());
    if solo_click {
        state.sculpt.toggle_solo_target(target);
        ui.ctx().request_repaint();
        return;
    }
    let toggle_editability = lock.clicked()
        || label_response.clicked()
        || (row_response.clicked() && !visibility.clicked());
    if toggle_editability {
        state.dispatch(Action::SetSculptTarget {
            target,
            enabled: !editable,
        });
    }
    control_affordances(ui, &row_response, row, 15.0);
    control_affordances(ui, &label_response, row, 15.0);

    let solo_hint = text(
        state.locale,
        if state.sculpt.soloed_target() == Some(target) {
            TextKey::SoloViewRestoreHint
        } else {
            TextKey::SoloViewHint
        },
    );
    crate::ui_components::tooltip(row_response, solo_hint, Some("Shift"));
    crate::ui_components::tooltip(label_response, solo_hint, Some("Shift"));
}

pub(super) fn detail_icon_toggle(
    ui: &mut Ui,
    icon: Icon,
    active: bool,
    tooltip: impl Into<egui::WidgetText>,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            f32::from(crate::theme::RADIUS_M),
            crate::theme::hover_fill(COLOR_VIEWPORT_TOOL),
        );
    }
    paint_icon(
        ui.painter(),
        rect.shrink(3.0),
        icon,
        if active {
            COLOR_TEXT
        } else {
            crate::theme::disabled(COLOR_MUTED)
        },
    );
    control_affordances(ui, &response, rect, f32::from(crate::theme::RADIUS_M));
    response.on_hover_text(tooltip)
}

pub(super) const TEXTURE_TARGET_PIN_DRAG: &str = "vkit.texture.target-pin-drag";

pub(super) const TEXTURE_TARGET_MASK_DRAG: &str = "vkit.texture.target-mask-drag";

pub(super) const DETAIL_GROUP_PAINT: &str = "vkit.viewport.detail.group-paint";

pub(super) fn clear_stale_detail_pointer_state(ui: &Ui, state: &mut AppState) {
    let mut interrupted = false;
    if !texture_target_pin_mode(state) {
        let id = Id::new(TEXTURE_TARGET_PIN_DRAG);
        if ui.data(|data| data.get_temp::<usize>(id).is_some()) {
            ui.data_mut(|data| data.remove::<usize>(id));
            interrupted = true;
        }
    }
    if !texture_paint_mode(state) {
        let id = Id::new(TEXTURE_TARGET_MASK_DRAG);
        if ui.data(|data| data.get_temp::<Pos2>(id).is_some()) {
            ui.data_mut(|data| data.remove::<Pos2>(id));
            interrupted = true;
        }
    }

    if state.is_texturing() {
        ui.data_mut(|data| data.remove::<bool>(Id::new(DETAIL_GROUP_PAINT)));
    }
    if !state.is_hair_editing() {
        super::hair_input::clear_hair_pointer_state(ui.ctx());
    }
    if interrupted && state.texture_project.edit_transaction_active() {
        state.dispatch(Action::EndTextureEdit);
    }
}

pub(super) fn texture_target_pin_mode(state: &AppState) -> bool {
    state.is_texturing()
        && state.texture_project.has_editable_layer()
        && state.texture_project.active_tool == TextureTool::PinPair
        && state
            .texture_project
            .selected_layer()
            .is_some_and(|layer| layer.source_mode == TextureSourceMode::LandmarkPins)
}

pub(super) fn projection_stencil_mode(state: &AppState) -> bool {
    state.texture_project.projection_stencil()
        && state.is_texturing()
        && state.texture_project.has_editable_layer()
        && state.texture_project.active_tool == TextureTool::Projection
        && state.texture_project.selected_layer().is_some_and(|layer| {
            layer.source_mode == TextureSourceMode::LandmarkPins
                && (layer.edited_image.is_some() || layer.image.is_some())
        })
}

pub(super) fn texture_paint_mode(state: &AppState) -> bool {
    state.is_texturing()
        && state.texture_project.has_editable_layer()
        && state.texture_project.active_tool.is_paint_brush()
        && state.texture_project.selected_layer().is_some()
}

#[derive(Clone, Copy)]
pub(super) struct TextureSurfaceHit {
    triangle_index: u32,
    barycentric: [f64; 3],
}

#[derive(Clone, Copy)]
struct HeadPickTriangle {
    triangle_index: u32,
    is_face: bool,
    corners: [glam::DVec3; 3],
}

struct HeadPickCandidates {
    mesh: Arc<SurfaceMesh>,
    mapping: Arc<vkit_core::vam::G2UvMapping>,
    triangles: Arc<Vec<HeadPickTriangle>>,
}

thread_local! {
    static HEAD_PICK_CANDIDATES: std::cell::RefCell<Option<HeadPickCandidates>> =
        const { std::cell::RefCell::new(None) };
}

fn head_pick_candidates(
    mesh: &Arc<SurfaceMesh>,
    mapping: &Arc<vkit_core::vam::G2UvMapping>,
) -> Arc<Vec<HeadPickTriangle>> {
    HEAD_PICK_CANDIDATES.with(|cell| {
        let mut held = cell.borrow_mut();
        if let Some(cached) = held.as_ref()
            && Arc::ptr_eq(&cached.mesh, mesh)
            && Arc::ptr_eq(&cached.mapping, mapping)
        {
            return Arc::clone(&cached.triangles);
        }
        let triangles = Arc::new(
            mapping
                .triangles
                .iter()
                .filter(|triangle| triangle.on_head)
                .filter_map(|mapped| {
                    let triangle_index = mapped.canonical_triangle_index;
                    let triangle = mesh.mesh.triangles.get(triangle_index as usize)?;
                    Some(HeadPickTriangle {
                        triangle_index,
                        is_face: mapped.material_region == vkit_core::vam::UvMaterialRegion::Face,
                        corners: triangle.map(|vertex| {
                            glam::DVec3::from_array(mesh.mesh.vertices[vertex as usize])
                        }),
                    })
                })
                .collect::<Vec<_>>(),
        );
        *held = Some(HeadPickCandidates {
            mesh: Arc::clone(mesh),
            mapping: Arc::clone(mapping),
            triangles: Arc::clone(&triangles),
        });
        triangles
    })
}

pub(super) fn texture_surface_hit(state: &AppState, ray: Ray3) -> Option<TextureSurfaceHit> {
    let result = state.workspace.result.as_ref()?;
    let mapping = state.vam_uv_mapping.as_ref()?;
    let origin = ray.origin;
    let direction = ray.direction;
    head_pick_candidates(result, mapping)
        .iter()
        .filter_map(|candidate| {
            let [a, b, c] = candidate.corners;
            ray_triangle(origin, direction, a, b, c, true).map(|(distance, barycentric)| {
                (
                    distance,
                    candidate.is_face,
                    TextureSurfaceHit {
                        triangle_index: candidate.triangle_index,
                        barycentric,
                    },
                )
            })
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .filter(|(_, is_face, _)| *is_face)
        .map(|(_, _, hit)| hit)
}

pub(super) fn handle_texture_paint_interaction(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
    input_blocked: bool,
) {
    let drag_id = Id::new(TEXTURE_TARGET_MASK_DRAG);
    if input_blocked {
        ui.data_mut(|data| data.remove::<Pos2>(drag_id));
        state.dispatch(Action::EndTextureEdit);
        return;
    }
    let size_update = handle_brush_size_gesture(
        ui,
        crate::ui_components::BrushSweeps::TEXTURE_SURFACE.size(),
        viewport,
        state.texture_project.mask_brush_radius,
        TEXTURE_BRUSH_SIZE_SENSITIVITY,
        0.002..=0.25,
    );
    if let Some(radius) = size_update.radius {
        state.dispatch(Action::SetTextureMaskBrushRadius(radius));
    }
    if size_update.consumed {
        return;
    }

    let strength_update = crate::ui_components::handle_brush_strength_gesture(
        ui,
        crate::ui_components::BrushSweeps::TEXTURE_SURFACE.strength(),
        viewport,
        state.texture_project.mask_brush_opacity,
        BRUSH_STRENGTH_SENSITIVITY,
        0.01..=1.0,
    );
    if let Some(opacity) = strength_update.strength {
        state.dispatch(Action::SetTextureMaskBrushOpacity(opacity));
    }
    if strength_update.consumed {
        return;
    }
    if let Some(radius) =
        brush_size_key_step(ui, state.texture_project.mask_brush_radius, 0.002..=0.25)
    {
        state.dispatch(Action::SetTextureMaskBrushRadius(radius));
    }
    if ui.input(|input| input.pointer.button_released(PointerButton::Primary)) {
        ui.data_mut(|data| data.remove::<Pos2>(drag_id));
        state.dispatch(Action::EndTextureEdit);
        return;
    }
    let pressed = ui.input(|input| input.pointer.button_pressed(PointerButton::Primary));
    if !ui.input(|input| input.pointer.button_down(PointerButton::Primary)) || !response.hovered() {
        return;
    }
    if !state.texture_project.edit_transaction_active() {
        state.dispatch(Action::BeginTextureEdit);
    }
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return;
    };
    let spacing = (state.texture_project.mask_brush_radius
        * texture_brush_points_per_uv(ui, state, viewport, camera)
        * crate::texture_ui::BRUSH_SPACING_FRACTION)
        .max(1.0);
    let reverse = ui.input(|input| input.modifiers.alt);
    let tool = state.texture_project.active_tool;
    if matches!(tool, TextureTool::CloneStamp) && reverse {
        if pressed
            && let Some(hit) = camera
                .ray_from_screen(pointer, viewport)
                .and_then(|ray| texture_surface_hit(state, ray))
        {
            state.set_texture_clone_sample_at_surface(hit.triangle_index, hit.barycentric);
            ui.data_mut(|data| data.remove::<Pos2>(drag_id));
        }
        return;
    }

    let stroke_points = crate::texture_ui::brush_stroke_points(
        ui.data(|data| data.get_temp::<Pos2>(drag_id)),
        pointer,
        spacing,
    );
    let Some(&last) = stroke_points.last() else {
        return;
    };
    let retouch_reverse = reverse ^ state.texture_project.retouch_reverse;
    for point in stroke_points {
        let Some(hit) = camera
            .ray_from_screen(point, viewport)
            .and_then(|ray| texture_surface_hit(state, ray))
        else {
            continue;
        };
        if tool == TextureTool::MaskBrush {
            state.add_texture_mask_dab_at_surface(hit.triangle_index, hit.barycentric, reverse);
        } else {
            state.add_texture_retouch_dab_at_surface(
                hit.triangle_index,
                hit.barycentric,
                retouch_reverse,
            );
        }
    }
    ui.data_mut(|data| data.insert_temp(drag_id, last));
    ui.ctx().request_repaint();
}

pub(super) fn measure_texture_brush_points_per_uv(
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
    pointer: Pos2,
) -> Option<f32> {
    let hit = camera
        .ray_from_screen(pointer, viewport)
        .and_then(|ray| texture_surface_hit(state, ray))?;
    let uvs = state
        .vam_uv_mapping
        .as_deref()?
        .triangles
        .iter()
        .find(|triangle| {
            triangle.canonical_triangle_index == hit.triangle_index
                && triangle.material_region == vkit_core::vam::UvMaterialRegion::Face
        })?
        .uvs;
    let result = state.workspace.result.as_deref()?;
    let corners = *result.mesh.triangles.get(hit.triangle_index as usize)?;
    let mut screen = [Pos2::ZERO; 3];
    for (slot, vertex) in screen.iter_mut().zip(corners) {
        let point = glam::DVec3::from_array(*result.mesh.vertices.get(vertex as usize)?);
        *slot = camera.project(point.as_vec3(), viewport)?.screen;
    }
    let screen_area = (screen[1].x - screen[0].x)
        .mul_add(
            screen[2].y - screen[0].y,
            -((screen[1].y - screen[0].y) * (screen[2].x - screen[0].x)),
        )
        .abs();
    let uv_area = (uvs[1][0] - uvs[0][0])
        .mul_add(
            uvs[2][1] - uvs[0][1],
            -((uvs[1][1] - uvs[0][1]) * (uvs[2][0] - uvs[0][0])),
        )
        .abs();
    (screen_area > 0.0 && uv_area > 1.0e-12).then(|| (screen_area / uv_area).sqrt())
}

/// How many screen points a UV unit spans, measured where the camera is
/// looking rather than where the pointer happens to be.
///
/// The brush paints in texture space, so its footprint in UV does not change as
/// the pointer crosses the face — but the span of a UV unit ON SCREEN does,
/// with the local triangle's foreshortening and with how densely that patch is
/// laid out in the atlas. Measuring at the pointer made the ring breathe as it
/// moved, which is what a hand reads as the brush resizing itself. Measuring
/// where the camera points keeps the ring still while the pointer moves and
/// still lets it grow when the view comes closer, which is the part that is
/// worth tracking.
fn texture_brush_points_per_uv(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
) -> f32 {
    let id = Id::new(TEXTURE_BRUSH_UV_SCALE_ID);
    if let Some(measured) =
        measure_texture_brush_points_per_uv(state, viewport, camera, viewport.center())
    {
        ui.data_mut(|data| data.insert_temp(id, measured));
        return measured;
    }
    ui.data(|data| data.get_temp::<f32>(id))
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .unwrap_or_else(|| viewport.width().min(viewport.height()))
}

pub(super) fn paint_texture_brush_cursor(
    ui: &Ui,
    state: &AppState,
    response: &Response,
    viewport: Rect,
    camera: TurntableCamera,
) {
    let hover = response
        .hovered()
        .then(|| ui.input(|input| input.pointer.hover_pos()))
        .flatten();
    let Some(cursor) = crate::ui_components::brush_cursor(
        ui,
        hover,
        crate::ui_components::BrushSweeps::TEXTURE_SURFACE.size(),
        Some((
            crate::ui_components::BrushSweeps::TEXTURE_SURFACE.strength(),
            state.texture_project.mask_brush_opacity,
        )),
    ) else {
        return;
    };
    let radius = (state.texture_project.mask_brush_radius
        * texture_brush_points_per_uv(ui, state, viewport, camera))
    .max(2.0);
    let reverse = crate::shortcuts::Shortcut::TextureInvertHold.held(ui)
        && state.texture_project.active_tool.alt_inverts();
    let color = if reverse {
        crate::theme::COLOR_DESTRUCTIVE
    } else {
        Color32::WHITE
    };
    crate::ui_components::paint_brush_cursor(ui.painter(), cursor, radius, color);
    crate::ui_components::hide_pointer(ui);
}

fn stencil_brush_radius_points(state: &AppState, stencil: Rect) -> f32 {
    let shorter = stencil.width().min(stencil.height()).max(1.0);
    (state.texture_project.mask_brush_radius * shorter).max(4.0)
}

pub(super) fn paint_stencil_brush_cursor(
    ui: &Ui,
    state: &AppState,
    response: &Response,
    stencil: Rect,
) {
    let hover = response
        .hovered()
        .then(|| ui.input(|input| input.pointer.latest_pos()))
        .flatten();
    let Some(cursor) = crate::ui_components::brush_cursor(
        ui,
        hover,
        crate::ui_components::BrushSweeps::TEXTURE_SURFACE.size(),
        Some((
            crate::ui_components::BrushSweeps::TEXTURE_SURFACE.strength(),
            state.texture_project.mask_brush_opacity,
        )),
    ) else {
        return;
    };
    let erase = crate::shortcuts::Shortcut::TextureInvertHold.held(ui)
        && state.texture_project.active_tool.alt_inverts();
    let color = if erase {
        crate::theme::COLOR_DESTRUCTIVE
    } else {
        Color32::WHITE
    };
    crate::ui_components::paint_brush_cursor(
        ui.painter(),
        cursor,
        stencil_brush_radius_points(state, stencil),
        color,
    );
    if stencil.contains(cursor.at) {
        crate::ui_components::hide_pointer(ui);
    }
}

const STENCIL_STROKE_LAST: &str = "vkit.texture.stencil-stroke-last";

const STENCIL_STROKE_TRIANGLES: &str = "vkit.texture.stencil-stroke-triangles";
fn project_face_triangles(
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
) -> Vec<vkit_core::texture_bake::ProjectedTriangle> {
    let Some(result) = state.workspace.result.as_ref() else {
        return Vec::new();
    };
    let Some(mapping) = state.vam_uv_mapping.as_ref() else {
        return Vec::new();
    };
    let vertex_count = result.mesh.vertices.len();
    let mut computed = vec![false; vertex_count];
    let mut screens = vec![[f32::NAN; 2]; vertex_count];
    let mut triangles = Vec::new();
    for mapped in mapping
        .triangles
        .iter()
        .filter(|triangle| triangle.material_region == vkit_core::vam::UvMaterialRegion::Face)
    {
        let Some(indices) = result
            .mesh
            .triangles
            .get(mapped.canonical_triangle_index as usize)
        else {
            continue;
        };
        let mut screen = [[0.0_f32; 2]; 3];
        let mut visible = true;
        for (corner, vertex) in indices.iter().enumerate() {
            let index = *vertex as usize;
            if index >= vertex_count {
                visible = false;
                break;
            }
            if !computed[index] {
                computed[index] = true;
                if let Some(point) = result.mesh.vertices.get(index) {
                    let world = glam::Vec3::new(point[0] as f32, point[1] as f32, point[2] as f32);
                    if let Some(projected) = camera.project(world, viewport) {
                        screens[index] = [projected.screen.x, projected.screen.y];
                    }
                }
            }
            let projected = screens[index];
            if projected[0].is_nan() {
                visible = false;
                break;
            }
            screen[corner] = projected;
        }
        if !visible {
            continue;
        }

        let area = (screen[1][0] - screen[0][0]) * (screen[2][1] - screen[0][1])
            - (screen[2][0] - screen[0][0]) * (screen[1][1] - screen[0][1]);
        if area >= 0.0 {
            continue;
        }
        triangles.push(vkit_core::texture_bake::ProjectedTriangle {
            screen,
            uv: mapped.uvs,
        });
    }
    triangles
}

fn stamp_projection_dab(
    state: &mut AppState,
    stencil: Rect,
    triangles: &[vkit_core::texture_bake::ProjectedTriangle],
    centre: Pos2,
    radius: f32,
    erase: bool,
) -> usize {
    let Some(source) = state
        .texture_project
        .selected_layer()
        .and_then(|layer| layer.edited_image.clone().or_else(|| layer.image.clone()))
    else {
        return 0;
    };
    let near: Vec<vkit_core::texture_bake::ProjectedTriangle> = triangles
        .iter()
        .filter(|triangle| {
            let xs = triangle.screen.map(|point| point[0]);
            let ys = triangle.screen.map(|point| point[1]);
            let left = xs.iter().copied().fold(f32::INFINITY, f32::min);
            let right = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
            let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            left - radius <= centre.x
                && right + radius >= centre.x
                && top - radius <= centre.y
                && bottom + radius >= centre.y
        })
        .copied()
        .collect();
    if near.is_empty() {
        return 0;
    }
    let stencil_centre = [stencil.center().x, stencil.center().y];
    let stencil_size = [stencil.width(), stencil.height()];
    let placement = state.texture_project.projection_placement;
    let brush = vkit_core::texture_bake::ProjectionBrush {
        centre: [centre.x, centre.y],
        radius,

        falloff: state.texture_project.mask_brush_falloff,
        opacity: state.texture_project.mask_brush_opacity.clamp(0.0, 1.0),
        erase,
    };
    state
        .texture_project
        .stamp_projection(&source, &near, brush, |screen| {
            placement.source_at(screen, stencil_centre, stencil_size)
        })
}

pub(super) fn handle_projection_stencil(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
    input_blocked: bool,
) {
    if input_blocked {
        ui.data_mut(|data| {
            data.remove::<Pos2>(Id::new(STENCIL_STROKE_LAST));
            data.remove::<std::sync::Arc<Vec<vkit_core::texture_bake::ProjectedTriangle>>>(
                Id::new(STENCIL_STROKE_TRIANGLES),
            );
        });

        if state.texture_project.edit_transaction_active() {
            state.dispatch(Action::EndTextureEdit);
        }
        return;
    }
    if crate::shortcuts::Shortcut::CancelStencil.pressed(ui) {
        state.dispatch(Action::SetTextureProjectionStencil(false));
        return;
    }

    if response.dragged_by(PointerButton::Secondary) {
        let delta = response.drag_delta();
        let placement = state.texture_project.projection_placement;
        let moved = if ui.input(|input| input.modifiers.shift) {
            let span = viewport.width().max(1.0) / 3.0;
            placement.rotated(delta.x / span * std::f32::consts::FRAC_PI_2)
        } else {
            placement.panned([delta.x, delta.y])
        };
        state.dispatch(Action::SetTextureProjectionPlacement(moved));
        return;
    }
    let wheel = ui.input(|input| input.smooth_scroll_delta.y);
    if wheel.abs() > 0.1
        && let Some(pointer) = ui.input(|input| input.pointer.interact_pos())
        && viewport.contains(pointer)
        && let Some(stencil) = crate::texture_ui::projection_stencil_rect(state, viewport)
    {
        let placement = state.texture_project.projection_placement;
        let zoomed = placement.zoomed(
            (wheel * 0.005).exp(),
            [pointer.x, pointer.y],
            [stencil.center().x, stencil.center().y],
        );
        state.dispatch(Action::SetTextureProjectionPlacement(zoomed));
        return;
    }

    let size_update = handle_brush_size_gesture(
        ui,
        crate::ui_components::BrushSweeps::TEXTURE_SURFACE.size(),
        viewport,
        state.texture_project.mask_brush_radius,
        TEXTURE_BRUSH_SIZE_SENSITIVITY,
        0.002..=0.25,
    );
    if let Some(radius) = size_update.radius {
        state.dispatch(Action::SetTextureMaskBrushRadius(radius));
    }
    if size_update.consumed {
        return;
    }
    let strength_update = crate::ui_components::handle_brush_strength_gesture(
        ui,
        crate::ui_components::BrushSweeps::TEXTURE_SURFACE.strength(),
        viewport,
        state.texture_project.mask_brush_opacity,
        BRUSH_STRENGTH_SENSITIVITY,
        0.01..=1.0,
    );
    if let Some(opacity) = strength_update.strength {
        state.dispatch(Action::SetTextureMaskBrushOpacity(opacity));
    }
    if strength_update.consumed {
        return;
    }
    if let Some(radius) =
        brush_size_key_step(ui, state.texture_project.mask_brush_radius, 0.002..=0.25)
    {
        state.dispatch(Action::SetTextureMaskBrushRadius(radius));
    }

    let last_id = Id::new(STENCIL_STROKE_LAST);
    let cache_id = Id::new(STENCIL_STROKE_TRIANGLES);
    if ui.input(|input| input.pointer.button_released(PointerButton::Primary)) {
        ui.data_mut(|data| {
            data.remove::<Pos2>(last_id);
            data.remove::<std::sync::Arc<Vec<vkit_core::texture_bake::ProjectedTriangle>>>(
                cache_id,
            );
        });
        if state.texture_project.edit_transaction_active() {
            state.dispatch(Action::EndTextureEdit);
        }
        return;
    }
    if !ui.input(|input| input.pointer.button_down(PointerButton::Primary)) || !response.hovered() {
        return;
    }
    if !state.texture_project.edit_transaction_active() {
        state.dispatch(Action::BeginTextureEdit);
    }
    let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
        return;
    };
    let Some(stencil) = crate::texture_ui::projection_stencil_rect(state, viewport) else {
        return;
    };

    let radius = stencil_brush_radius_points(state, stencil);

    let spacing = (radius * crate::texture_ui::BRUSH_SPACING_FRACTION).max(1.0);
    let stroke_points = crate::texture_ui::brush_stroke_points(
        ui.data(|data| data.get_temp::<Pos2>(last_id)),
        pointer,
        spacing,
    );
    let Some(&last) = stroke_points.last() else {
        return;
    };

    let triangles = ui
        .data(|data| {
            data.get_temp::<std::sync::Arc<Vec<vkit_core::texture_bake::ProjectedTriangle>>>(
                cache_id,
            )
        })
        .unwrap_or_else(|| {
            let projected = std::sync::Arc::new(project_face_triangles(state, viewport, camera));
            ui.data_mut(|data| data.insert_temp(cache_id, std::sync::Arc::clone(&projected)));
            projected
        });
    let erase = crate::shortcuts::Shortcut::TextureInvertHold.held(ui)
        && state.texture_project.active_tool.alt_inverts();
    for point in stroke_points {
        stamp_projection_dab(state, stencil, &triangles, point, radius, erase);
    }
    ui.data_mut(|data| data.insert_temp(last_id, last));
    ui.ctx().request_repaint();
}

pub(super) const PROJECTION_ISLAND_HEIGHT: f32 = 40.0;

pub(super) const PROJECTION_ISLAND: crate::responsive::Responsive = crate::responsive::Responsive {
    min: 120.0,
    ideal: 340.0,
    fraction: 0.9,
};

pub(super) fn draw_projection_stencil_island(ui: &mut Ui, state: &mut AppState, viewport: Rect) {
    let Some(width) = PROJECTION_ISLAND.resolve(viewport.width()) else {
        return;
    };
    let island = Rect::from_center_size(
        pos2(
            viewport.center().x,
            viewport.center().y + viewport.height() * 0.28,
        ),
        vec2(width, PROJECTION_ISLAND_HEIGHT),
    );
    draw_detail_header_island(ui, island, "vkit.viewport.projection-island");
    let content = island.shrink2(vec2(SPACE_3, 5.0));
    let button_width = 92.0_f32.min(content.width() * 0.4);
    let slider_rect = Rect::from_min_max(
        content.min,
        pos2(content.right() - button_width - SPACE_3, content.bottom()),
    );
    let mut opacity = state.texture_project.projection_opacity;
    if ui
        .put(
            slider_rect,
            FilledNumericSlider::new(&mut opacity, 0.05..=1.0)
                .percent()
                .decimals(0)
                .min_width(slider_rect.width().max(1.0)),
        )
        .on_hover_text(text(state.locale, TextKey::Opacity))
        .changed()
    {
        state.dispatch(Action::SetTextureProjectionOpacity(opacity));
    }
    let button_rect = Rect::from_min_max(
        pos2(content.right() - button_width, content.top()),
        content.max,
    );

    if ui
        .put(
            button_rect,
            egui::Button::new(text(state.locale, TextKey::ProjectDone))
                .corner_radius(crate::theme::CAPSULE_RADIUS),
        )
        .on_hover_text(text(state.locale, TextKey::ProjectDoneTooltip))
        .clicked()
    {
        state.dispatch(Action::SetTextureProjectionStencil(false));
    }
}

pub(super) fn handle_texture_target_pin_interaction(
    ui: &Ui,
    state: &mut AppState,
    viewport: Rect,
    response: &Response,
    camera: TurntableCamera,
    input_blocked: bool,
) {
    let drag_id = Id::new(TEXTURE_TARGET_PIN_DRAG);
    if input_blocked {
        ui.data_mut(|data| data.remove::<usize>(drag_id));

        if state.texture_project.edit_transaction_active() {
            state.dispatch(Action::EndTextureEdit);
        }
        return;
    }
    let pointer = ui.input(|input| input.pointer.interact_pos());
    let nearest =
        pointer.and_then(|pointer| nearest_texture_target_pin(state, viewport, camera, pointer));
    if ui.input(|input| input.pointer.button_pressed(PointerButton::Primary))
        && response.hovered()
        && let Some(index) = nearest
    {
        state.dispatch(Action::BeginTextureEdit);
        ui.data_mut(|data| data.insert_temp(drag_id, index));
    }
    let hit = |state: &AppState, pointer: Pos2| {
        let ray = camera.ray_from_screen(pointer, viewport)?;
        texture_surface_hit(state, ray)
    };
    if ui.input(|input| input.pointer.button_down(PointerButton::Primary))
        && let Some(index) = ui.data(|data| data.get_temp::<usize>(drag_id))
        && let Some(pointer) = pointer
        && let Some(hit) = hit(state, pointer)
    {
        state.move_texture_target_pin(index, hit.triangle_index, hit.barycentric);
        ui.ctx().request_repaint();
    }
    if ui.input(|input| input.pointer.button_released(PointerButton::Primary)) {
        ui.data_mut(|data| data.remove::<usize>(drag_id));
        state.dispatch(Action::EndTextureEdit);
    }
    if response.clicked_by(PointerButton::Primary)
        && nearest.is_none()
        && let Some(pointer) = pointer
        && let Some(hit) = hit(state, pointer)
    {
        state.add_texture_target_pin(hit.triangle_index, hit.barycentric);
    }
    if response.clicked_by(PointerButton::Secondary)
        && let Some(index) = nearest
    {
        state.remove_texture_pin(index);
    }
}

pub(super) fn texture_target_pin_screen_positions(
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
) -> Vec<(usize, Pos2, bool)> {
    let Some(layer) = state.texture_project.selected_layer() else {
        return Vec::new();
    };
    let Some(result) = state.workspace.result.as_deref() else {
        return Vec::new();
    };
    layer
        .pins
        .iter()
        .enumerate()
        .filter_map(|(index, pair)| {
            let target = pair.target?;
            let triangle = result.mesh.triangles.get(target.triangle_index as usize)?;
            let point = triangle.iter().zip(target.barycentric).fold(
                glam::DVec3::ZERO,
                |sum, (&vertex, weight)| {
                    sum + glam::DVec3::from_array(result.mesh.vertices[vertex as usize]) * weight
                },
            );
            let projected = camera.project(point.as_vec3(), viewport)?;
            (projected.depth > 0.0).then_some((
                index,
                projected.screen,
                layer.pin_pair_invalid(index),
            ))
        })
        .collect()
}

pub(super) fn nearest_texture_target_pin(
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
    pointer: Pos2,
) -> Option<usize> {
    texture_target_pin_screen_positions(state, viewport, camera)
        .into_iter()
        .filter_map(|(index, screen, _)| {
            let distance = screen.distance(pointer);
            (distance <= PIN_HIT_RADIUS).then_some((distance, index))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, index)| index)
}

pub(super) fn paint_clone_anchor_on_surface(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
) {
    if state.texture_project.active_tool != TextureTool::CloneStamp {
        return;
    }
    let Some((triangle_index, barycentric)) = state.texture_project.clone_sample_surface else {
        return;
    };
    let Some(result) = state.workspace.result.as_deref() else {
        return;
    };
    let Some(triangle) = result.mesh.triangles.get(triangle_index as usize) else {
        return;
    };
    let point =
        triangle
            .iter()
            .zip(barycentric)
            .fold(glam::DVec3::ZERO, |sum, (&vertex, weight)| {
                sum + glam::DVec3::from_array(result.mesh.vertices[vertex as usize]) * weight
            });
    let Some(projected) = camera.project(point.as_vec3(), viewport) else {
        return;
    };
    if projected.depth > 0.0 {
        let painter = ui
            .painter()
            .with_clip_rect(ui.clip_rect().intersect(viewport));
        crate::ui_components::paint_clone_anchor(&painter, projected.screen);
    }
}

pub(super) fn paint_texture_target_pins(
    ui: &Ui,
    state: &AppState,
    viewport: Rect,
    camera: TurntableCamera,
) {
    let painter = ui
        .painter()
        .with_clip_rect(ui.clip_rect().intersect(viewport));
    for (index, center, invalid) in texture_target_pin_screen_positions(state, viewport, camera) {
        paint_texture_pin(
            &painter,
            center,
            state.texture_project.pin_opacity,
            &(index + 1).to_string(),
            invalid,
        );
    }
}
