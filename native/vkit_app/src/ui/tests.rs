use super::*;
use egui::style::ScrollStyle;

#[test]
fn catalog_progress_uses_stage_headroom_instead_of_freezing_at_reports() {
    assert_eq!(catalog_progress_soft_ceiling(0.02), 0.08);
    assert_eq!(catalog_progress_soft_ceiling(0.12), 0.54);
    assert_eq!(catalog_progress_soft_ceiling(0.60), 0.83);
    assert_eq!(catalog_progress_soft_ceiling(0.90), 0.96);
    assert_eq!(catalog_progress_soft_ceiling(0.97), 0.985);
}

#[test]
fn catalog_progress_catches_up_smoothly_and_finishes_monotonically() {
    let displayed = 0.12;
    let next = advance_catalog_progress(displayed, 0.60, 0.83, 0.016, false);
    assert!(next > displayed);
    assert!(next - displayed <= 0.016 * 0.45 + f32::EPSILON);

    let coasting = advance_catalog_progress(0.30, 0.12, 0.54, 0.05, false);
    assert!(coasting > 0.30);
    assert!(coasting <= 0.54);

    let finishing = advance_catalog_progress(0.91, 1.0, 1.0, 0.05, true);
    assert!(finishing > 0.91);
    assert!(finishing <= 1.0);
}

#[test]
fn titlebar_double_click_and_drag_are_mutually_exclusive() {
    assert_eq!(title_drag_gesture(false, false), None);
    assert_eq!(
        title_drag_gesture(false, true),
        Some(TitleDragGesture::StartDrag)
    );
    assert_eq!(
        title_drag_gesture(true, false),
        Some(TitleDragGesture::ToggleMaximized)
    );
    assert_eq!(
        title_drag_gesture(true, true),
        Some(TitleDragGesture::ToggleMaximized),
        "one gesture must never emit both native window commands"
    );
}

#[test]
fn resize_zones_keep_titlebar_controls_out_of_their_hit_regions() {
    let window = Rect::from_min_size(pos2(0.0, 0.0), vec2(1_600.0, 920.0));
    let controls = Rect::from_min_max(
        pos2(
            window.right() - TITLE_WINDOW_BUTTON_WIDTH * 3.0,
            window.top() + 6.0,
        ),
        pos2(window.right(), window.top() + TOP_BAR_HEIGHT),
    );
    let zones = window_resize_zones(window);

    assert_eq!(zones.len(), 8);
    for (zone, _) in zones {
        let has_positive_overlap = zone.left() < controls.right()
            && zone.right() > controls.left()
            && zone.top() < controls.bottom()
            && zone.bottom() > controls.top();
        assert!(
            !has_positive_overlap,
            "resize zone {zone:?} overlaps window controls {controls:?}"
        );
    }
}

#[test]
fn window_button_cells_tile_the_controls_strip_without_gaps() {
    let controls = Rect::from_min_size(
        pos2(1_480.0, 6.0),
        vec2(TITLE_WINDOW_BUTTON_WIDTH * 3.0, TOP_TAB_HEIGHT),
    );
    let cells = window_button_cells(controls);

    assert_eq!(cells[0].left(), controls.left());
    assert_eq!(cells[2].right(), controls.right());
    for pair in cells.windows(2) {
        assert_eq!(
            pair[0].right(),
            pair[1].left(),
            "caption button cells must tile without dead gaps"
        );
    }
    for cell in cells {
        assert_eq!(cell.width(), TITLE_WINDOW_BUTTON_WIDTH);
        assert_eq!(cell.height(), TOP_TAB_HEIGHT);
    }
}

#[test]
fn window_button_highlight_is_a_centered_square_inside_its_cell() {
    let controls = Rect::from_min_size(
        pos2(1_480.0, 6.0),
        vec2(TITLE_WINDOW_BUTTON_WIDTH * 3.0, TOP_TAB_HEIGHT),
    );
    for cell in window_button_cells(controls) {
        let highlight = window_button_highlight_rect(cell);
        assert_eq!(
            highlight.width(),
            highlight.height(),
            "caption highlights must be square, not a wide slab"
        );
        assert_eq!(highlight.center(), cell.center());
        assert!(cell.contains_rect(highlight));

        assert_eq!(
            highlight.height(),
            TOP_TAB_HEIGHT - crate::theme::SPACE_1 * 2.0
        );
    }
}

#[test]
fn readable_ink_flips_with_swatch_luminance() {
    assert_eq!(
        crate::ui_components::readable_ink(Color32::WHITE),
        crate::theme::COLOR_ACTIVE_INK
    );
    assert_eq!(
        crate::ui_components::readable_ink(Color32::BLACK),
        crate::theme::COLOR_TEXT
    );
}

#[test]
fn log_view_hides_debug_records_unless_the_system_toggle_is_on() {
    let raw = concat!(
        "2026-07-21T14:15:55.997Z\tINFO\truntime\tready\tDX12 initialized\n",
        "2026-07-21T14:15:57.689Z\tDEBUG\truntime\tpointer_release_synthesized\tcleared\n",
        "2026-07-21T14:15:58.001Z\tWARN\truntime\tfont_fallback\tno Korean font\n",
        "2026-07-21T14:15:58.002Z\tERROR\truntime\tjob_failed\tboom\n",
        "malformed line without severity field\n",
    );

    assert!(!diagnostic_line_is_system(
        "t\tINFO\truntime\tready\tmessage"
    ));
    assert!(diagnostic_line_is_system(
        "t\tDEBUG\truntime\tjob_progress\tmessage"
    ));
    assert!(!diagnostic_line_is_system("no severity field"));

    let visible = filter_diagnostic_log(raw, false, Locale::Korean);
    assert!(!visible.contains("pointer_release_synthesized"));

    assert!(
        visible.contains("14:15:55    DX12 initialized"),
        "{visible}"
    );
    assert!(
        visible.contains("14:15:58  ! 대체 폰트 사용 — no Korean font"),
        "{visible}"
    );
    assert!(
        visible.contains("14:15:58  × 작업 실패 — boom"),
        "{visible}"
    );

    assert!(
        visible.contains("malformed line without severity field"),
        "{visible}"
    );

    let unstamped = visible
        .lines()
        .find(|line| line.contains("malformed line"))
        .expect("the malformed line is shown");
    assert!(
        unstamped.starts_with(char::is_whitespace),
        "no time invented from the text: {unstamped:?}"
    );
    assert_eq!(visible.lines().count(), 4);

    let system = filter_diagnostic_log(raw, true, Locale::Korean);
    assert!(system.contains("pointer_release_synthesized"), "{system}");
    assert!(!system.contains("2026-07-21"), "no date: {system}");
    assert!(!system.contains("INFO"), "no severity word: {system}");

    let repetitive = concat!(
        "2026-07-21T14:20:01.000Z\tINFO\truntime\tscan\tindexing\n",
        "2026-07-21T14:20:02.000Z\tINFO\truntime\tscan\tindexing\n",
        "2026-07-21T14:20:03.000Z\tINFO\truntime\tscan\tindexing\n",
        "2026-07-21T14:20:04.000Z\tINFO\truntime\tready\tdone\n",
    );
    let collapsed = simplify_diagnostic_log(repetitive, Locale::Korean);
    assert_eq!(collapsed.lines().count(), 2);
    assert!(
        collapsed.contains("14:20:03    indexing  (x3)"),
        "{collapsed}"
    );
    assert!(collapsed.contains("14:20:04    done"), "{collapsed}");

    let long_message = "x".repeat(DIAGNOSTIC_LOG_MESSAGE_LIMIT * 2);
    let long_line =
        format!("2026-07-21T14:21:00.000Z\tERROR\tworkflow\tjob_failed\t{long_message}\n");
    let truncated = simplify_diagnostic_log(&long_line, Locale::Korean);
    assert!(truncated.contains('…'));
    assert!(truncated.lines().next().unwrap().len() < DIAGNOSTIC_LOG_MESSAGE_LIMIT + 32);
    let mut flood = String::new();
    for index in 0..(DIAGNOSTIC_LOG_SIMPLE_ROWS + 50) {
        flood.push_str(&format!(
            "2026-07-21T14:22:00.000Z\tINFO\truntime\tstep\tmessage {index}\n"
        ));
    }
    let capped = simplify_diagnostic_log(&flood, Locale::Korean);
    assert_eq!(capped.lines().count(), DIAGNOSTIC_LOG_SIMPLE_ROWS);
    assert!(capped.contains(&format!("message {}", DIAGNOSTIC_LOG_SIMPLE_ROWS + 49)));
    assert!(!capped.contains("message 0\n"));
}

#[test]
fn skin_popover_texture_page_has_no_full_height_layout_residue() {
    egui::__run_test_ui(|ui| {
        let cell = Rect::from_min_size(ui.cursor().min, vec2(300.0, 720.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(cell));
        child.set_clip_rect(cell);
        child.set_width(cell.width());
        let mut state = AppState::default();
        state.base_view_mode = BaseViewMode::Texture;
        draw_viewport_skin_panel_contents(&mut child, &mut state);
        let used = child.min_rect();

        assert!(
            used.height() < 420.0,
            "skin popover content spans {} px; the flush search/refresh \
             row must not reserve leftover full-height space",
            used.height()
        );
        assert!(used.right() <= cell.right() + 0.5);
    });
}

#[test]
fn the_title_language_capsule_fits_every_language_name() {
    let context = egui::Context::default();
    if crate::theme::configure_context(&context, crate::i18n::Locale::Korean)
        .fonts
        .is_empty()
    {
        eprintln!("skipping: no Windows system fonts available");
        return;
    }
    let _ = context.run_ui(egui::RawInput::default(), |_| {});
    let _ = context.run_ui(egui::RawInput::default(), |root| {
        let width = crate::ui::title_locale_width(root);
        assert!(
            width <= crate::ui::TITLE_LOCALE_MAX_WIDTH,
            "the capsule is {width:.1}pt, past the ceiling that keeps it off the tabs"
        );
        for locale in crate::i18n::Locale::ALL {
            let label = root
                .painter()
                .layout_no_wrap(
                    locale.selector_label().to_owned(),
                    egui::FontId::proportional(crate::theme::FONT_BODY),
                    crate::theme::COLOR_TEXT,
                )
                .size()
                .x;
            assert!(
                label + crate::ui::TITLE_LOCALE_CHROME <= width + 0.5,
                "{:?} names itself {:?}, which needs {:.1}pt of capsule and has {width:.1}pt --                  the combo would grow into the settings button",
                locale,
                locale.selector_label(),
                label + crate::ui::TITLE_LOCALE_CHROME
            );
        }
    });
}

#[test]
fn a_full_width_save_action_centres_its_label() {
    use egui::epaint::Shape;

    let mut state = AppState::default();
    state.active_tab = Tab::Result;
    let locale = state.locale;
    let context = egui::Context::default();

    let report = crate::theme::configure_context(&context, locale);
    assert!(!report.fonts.is_empty(), "no fonts");
    let input = || egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 1000.0),
        )),
        ..Default::default()
    };
    fn walk(shape: &Shape, texts: &mut Vec<(String, Rect)>, rects: &mut Vec<Rect>) {
        match shape {
            Shape::Text(text) => texts.push((
                text.galley.text().to_owned(),
                Rect::from_min_size(text.pos, text.galley.size()),
            )),
            Shape::Rect(rect) => rects.push(rect.rect),
            Shape::Vec(children) => {
                for child in children {
                    walk(child, texts, rects);
                }
            }
            _ => {}
        }
    }

    for (section, key) in [
        (crate::state::SaveSection::Morph, TextKey::MorphSave),
        (crate::state::SaveSection::Package, TextKey::PackageSave),
    ] {
        state.save_section = section;

        let _ = context.run_ui(input(), |root| draw(root, &mut state));
        let output = context.run_ui(input(), |root| draw(root, &mut state));
        let mut texts: Vec<(String, Rect)> = Vec::new();
        let mut rects: Vec<Rect> = Vec::new();
        for shape in &output.shapes {
            walk(&shape.shape, &mut texts, &mut rects);
        }
        let label = text(locale, key);
        let placed = texts
            .iter()
            .filter(|(content, _)| content == label)
            .find_map(|(_, drawn)| {
                rects
                    .iter()
                    .filter(|rect| {
                        rect.contains_rect(*drawn)
                            && rect.width() > drawn.width() * 2.0
                            && (rect.height() - crate::theme::CONTROL_H_PRIMARY).abs() < 0.5
                    })
                    .min_by(|left, right| left.area().total_cmp(&right.area()))
                    .map(|capsule| (*drawn, *capsule))
            })
            .unwrap_or_else(|| panic!("{label} is not inside a control; saw {texts:?}"));
        let (drawn, capsule) = placed;

        let offset = (drawn.center().x - capsule.center().x).abs();
        assert!(
            offset < 1.0,
            "{label} sits {offset:.1}pt off centre in a {:.0}pt capsule",
            capsule.width()
        );
        assert!(
            (capsule.height() - crate::theme::CONTROL_H_PRIMARY).abs() < 0.5,
            "{label} is in a {:.0}pt capsule, not the primary {:.0}pt",
            capsule.height(),
            crate::theme::CONTROL_H_PRIMARY
        );
        eprintln!(
            "{label}: label centre {:.1}, capsule {:.0}x{:.0} centred {:.1}",
            drawn.center().x,
            capsule.width(),
            capsule.height(),
            capsule.center().x
        );
    }
}

#[test]
fn the_package_fields_read_as_a_version_and_a_licence_code() {
    use egui::epaint::Shape;

    let mut state = AppState::default();
    state.active_tab = Tab::Result;

    state.save_section = crate::state::SaveSection::Package;
    let locale = state.locale;
    let context = egui::Context::default();
    assert!(
        !crate::theme::configure_context(&context, locale)
            .fonts
            .is_empty()
    );
    let input = || egui::RawInput {
        screen_rect: Some(Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 1000.0),
        )),
        ..Default::default()
    };
    let _ = context.run_ui(input(), |root| draw(root, &mut state));
    let output = context.run_ui(input(), |root| draw(root, &mut state));

    let mut texts: Vec<(String, Rect)> = Vec::new();
    fn walk(shape: &Shape, texts: &mut Vec<(String, Rect)>) {
        match shape {
            Shape::Text(drawn) => texts.push((
                drawn.galley.text().to_owned(),
                Rect::from_min_size(drawn.pos, drawn.galley.size()),
            )),
            Shape::Vec(children) => {
                for child in children {
                    walk(child, texts);
                }
            }
            _ => {}
        }
    }
    for shape in &output.shapes {
        walk(&shape.shape, &mut texts);
    }
    let find = |needle: &str| {
        texts
            .iter()
            .find(|(content, _)| content == needle)
            .map(|(_, rect)| *rect)
            .unwrap_or_else(|| panic!("{needle} was not drawn; saw {texts:?}"))
    };

    let marker = find(text(locale, TextKey::PackageVersionMarker));
    let value = find("1");
    assert!(
        marker.right() < value.left(),
        "the marker is not in front of the value: {marker:?} then {value:?}"
    );
    assert!(
        (marker.center().y - value.center().y).abs() < 2.0,
        "the marker and the value are not on one line"
    );

    let license = find("CC BY");
    assert!(
        !texts.iter().any(|(content, rect)| {
            (rect.center().y - license.center().y).abs() < 2.0
                && rect.right() <= license.left()
                && !content.is_empty()
        }),
        "something is drawn to the left of the licence code; saw {texts:?}"
    );
    eprintln!("version {marker:?} then {value:?}; licence {license:?}");
}

#[test]
fn the_version_marker_is_a_label_and_never_the_value() {
    for (locale, marker) in [
        (Locale::English, "V"),
        (Locale::Korean, "버전"),
        (Locale::Japanese, "バージョン"),
    ] {
        assert_eq!(text(locale, TextKey::PackageVersionMarker), marker);
    }
    let mut state = AppState::default();
    state.dispatch(Action::SetVarMetadata(
        crate::state::VarMetadataField::Version,
        "V3".to_owned(),
    ));
    assert_eq!(
        state.var_version_text, "3",
        "the marker never enters the value"
    );
    assert_eq!(state.package_metadata().version, 3);
}

#[test]
fn the_texture_stage_draws_a_split_and_a_brush_island() {
    let split = Id::new("vkit.texture.workspace-split");
    let paint_island = Id::new("vkit.viewport.detail.header.paint");
    let sculpt_island = Id::new("vkit.viewport.detail.header.sculpt");

    for (tab, wants_split) in [(Tab::Texture, true), (Tab::Morph, false)] {
        let mut state = AppState::default();
        let context = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 1000.0),
            )),
            ..Default::default()
        };
        state.active_tab = tab;

        let _ = context.run_ui(input(), |root| draw(root, &mut state));
        let _ = context.run_ui(input(), |root| draw(root, &mut state));

        assert_eq!(
            context.read_response(split).is_some(),
            wants_split,
            "{tab:?} split handle"
        );
        assert_eq!(
            context.read_response(paint_island).is_some(),
            wants_split,
            "{tab:?} paint island"
        );
        assert_eq!(
            context.read_response(sculpt_island).is_some(),
            !wants_split,
            "{tab:?} sculpt island"
        );
    }
}

#[test]
fn every_tab_paints_on_a_cold_start() {
    for tab in [
        Tab::Alignment,
        Tab::Edit,
        Tab::Morph,
        Tab::Texture,
        Tab::Result,
    ] {
        let mut state = AppState::default();
        let context = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 1000.0),
            )),
            ..Default::default()
        };

        let _ = context.run_ui(input(), |root| draw(root, &mut state));

        state.active_tab = tab;
        let _ = context.run_ui(input(), |root| draw(root, &mut state));
        let _ = context.run_ui(input(), |root| draw(root, &mut state));
    }
}

#[test]
fn scale_link_icon_button_has_localized_accessible_selected_state() {
    for locale in Locale::ALL {
        for linked in [false, true] {
            let info = scale_link_widget_info(locale, true, linked);
            assert_eq!(info.typ, WidgetType::Button);
            assert!(info.enabled);
            assert_eq!(info.selected, Some(linked));
            assert_eq!(
                info.label.as_deref(),
                Some(text(locale, TextKey::ScaleLink))
            );
            assert!(!info.label.as_deref().unwrap_or_default().trim().is_empty());
        }
    }
}

#[test]
fn ctrl_z_routes_to_alignment_pins_and_consolidated_detail_without_text_input() {
    assert!(routes_global_undo(Tab::Alignment, true, true, false));
    assert!(routes_global_undo(Tab::Edit, true, true, false));
    assert!(routes_global_undo(Tab::Morph, true, true, false));
    assert!(routes_global_undo(Tab::Texture, true, true, false));
    assert!(!routes_global_undo(Tab::Result, true, true, false));
    assert!(!routes_global_undo(Tab::Morph, false, true, false));
    assert!(!routes_global_undo(Tab::Morph, true, false, false));
    assert!(!routes_global_undo(Tab::Morph, true, true, true));
}

#[test]
fn x_mirrors_whatever_the_stage_is_for() {
    assert_eq!(
        routes_x_symmetry(Tab::Edit, true, false),
        Some(XSymmetryTarget::Pins)
    );
    assert_eq!(
        routes_x_symmetry(Tab::Morph, true, false),
        Some(XSymmetryTarget::Brush)
    );

    assert_eq!(routes_x_symmetry(Tab::Alignment, true, false), None);
    assert_eq!(routes_x_symmetry(Tab::Result, true, false), None);

    assert_eq!(routes_x_symmetry(Tab::Edit, false, false), None);
    assert_eq!(routes_x_symmetry(Tab::Edit, true, true), None);
    assert_eq!(routes_x_symmetry(Tab::Morph, true, true), None);
}

#[test]
fn texture_tool_shortcuts_only_fire_in_the_paint_split() {
    assert!(routes_texture_tool_shortcut(Tab::Texture, false));
    assert!(!routes_texture_tool_shortcut(Tab::Morph, false));
    assert!(!routes_texture_tool_shortcut(Tab::Edit, false));
    assert!(!routes_texture_tool_shortcut(Tab::Result, false));
    assert!(!routes_texture_tool_shortcut(Tab::Texture, true));
}

#[test]
fn workflow_tabs_expose_four_visible_stages() {
    assert_eq!(
        TOP_TABS.map(|(tab, _)| tab),
        [Tab::Edit, Tab::Morph, Tab::Texture, Tab::Result]
    );
    assert!(!TOP_TABS.iter().any(|(tab, _)| *tab == Tab::Alignment));
    assert_eq!(TOP_TABS[1].1, TextKey::DetailCorrection);
    for locale in Locale::ALL {
        for (_, key) in TOP_TABS {
            assert!(!text(locale, key).trim().is_empty());
        }
    }
}

#[test]
fn the_tab_strip_is_one_run_as_wide_as_its_cells() {
    let strip = Rect::from_min_size(pos2(100.0, 6.0), vec2(600.0, TOP_TAB_HEIGHT));
    let tab_width = 112.0;
    let cells: Vec<Rect> = (0..TOP_TABS.len())
        .map(|index| top_tab_cell(strip, tab_width, index))
        .collect();
    for pair in cells.windows(2) {
        assert_eq!(
            pair[1].left() - pair[0].right(),
            TOP_TAB_GAP,
            "stages sit in one evenly spaced run"
        );
    }

    assert_eq!(
        top_tab_strip_width(tab_width),
        cells[TOP_TABS.len() - 1].right() - strip.left()
    );
}

#[test]
fn create_button_fronts_the_alignment_phase() {
    let mut state = AppState::default();

    state.active_tab = Tab::Alignment;
    assert!(visible_tab_is_active(&state, Tab::Edit));
    assert!(!visible_tab_is_active(&state, Tab::Morph));
    assert!(!visible_tab_is_active(&state, Tab::Result));

    state.active_tab = Tab::Edit;
    assert!(visible_tab_is_active(&state, Tab::Edit));
}

#[test]
fn every_tab_renders_inside_the_minimum_inspector_in_every_locale() {
    for locale in Locale::ALL {
        for tab in [
            Tab::Alignment,
            Tab::Edit,
            Tab::Morph,
            Tab::Texture,
            Tab::Result,
        ] {
            egui::__run_test_ui(|ui| {
                let cell = Rect::from_min_size(
                    ui.cursor().min,
                    vec2(INSPECTOR_MIN_WIDTH - PANEL_INSET * 2.0, 520.0),
                );
                let mut child = ui.new_child(UiBuilder::new().max_rect(cell));
                child.set_clip_rect(cell);
                child.set_width(cell.width());
                child.set_height(cell.height());
                let mut state = AppState::default();
                state.locale = locale;
                state.active_tab = tab;
                match tab {
                    Tab::Alignment => draw_alignment_inspector(&mut child, &mut state),
                    Tab::Edit => draw_edit_inspector(&mut child, &mut state),
                    Tab::Morph | Tab::Texture => draw_morph_inspector(&mut child, &mut state),
                    Tab::Result => draw_result_inspector(&mut child, &mut state),
                }
                assert!(child.min_rect().right() <= cell.right() + 0.5);
                assert!(child.clip_rect().right() <= cell.right() + 0.5);
            });
        }
    }
}

#[test]
fn texture_layer_controls_fit_the_minimum_inspector_in_every_locale() {
    for locale in Locale::ALL {
        egui::__run_test_ui(|ui| {
            let cell = Rect::from_min_size(
                ui.cursor().min,
                vec2(INSPECTOR_MIN_WIDTH - PANEL_INSET * 2.0, 760.0),
            );
            let mut child = ui.new_child(UiBuilder::new().max_rect(cell));
            child.set_clip_rect(cell);
            child.set_width(cell.width());
            child.set_height(cell.height());
            let mut state = AppState::default();
            state.locale = locale;
            state.active_tab = Tab::Morph;
            for name in ["diffuse.png", "normal.png", "roughness.png"] {
                state.texture_project.add_image_layer(
                    std::path::PathBuf::from(name),
                    crate::texture_project::TextureSourceMode::LandmarkPins,
                );
            }
            draw_morph_inspector(&mut child, &mut state);
            assert!(
                child.min_rect().right() <= cell.right() + 0.5,
                "{locale:?}: content right {} exceeds cell right {}",
                child.min_rect().right(),
                cell.right()
            );
            assert!(child.clip_rect().right() <= cell.right() + 0.5);
        });
    }
}

#[test]
fn custom_morph_source_picker_fits_the_minimum_edit_inspector() {
    for locale in Locale::ALL {
        egui::__run_test_ui(|ui| {
            let cell = Rect::from_min_size(
                ui.cursor().min,
                vec2(INSPECTOR_MIN_WIDTH - PANEL_INSET * 2.0, 760.0),
            );
            let mut child = ui.new_child(UiBuilder::new().max_rect(cell));
            child.set_clip_rect(cell);
            child.set_width(cell.width());
            child.set_height(cell.height());
            let mut state = AppState::default();
            state.locale = locale;
            state.active_tab = Tab::Morph;
            state.edit_source_mode = EditSourceMode::CustomMorph;

            state.morph_look_find_open = true;
            draw_morph_inspector(&mut child, &mut state);
            assert!(
                child.min_rect().right() <= cell.right() + 0.5,
                "{locale:?}: content right {} exceeds cell right {}",
                child.min_rect().right(),
                cell.right()
            );
            assert!(child.clip_rect().right() <= cell.right() + 0.5);
        });
    }
}

use crate::theme::{INSPECTOR_MAX_WIDTH, INSPECTOR_MIN_WIDTH};

#[test]
fn opacity_row_uses_the_handleless_numeric_slider_without_overflow() {
    egui::__run_test_ui(|ui| {
        let cell = Rect::from_min_size(ui.cursor().min, vec2(344.0, CONTROL_HEIGHT));
        let mut child = ui.new_child(UiBuilder::new().max_rect(cell));
        child.set_width(cell.width());
        let mut opacity = 0.5;
        opacity_percent_row(&mut child, "Opacity", &mut opacity);
        assert!(child.min_rect().right() <= cell.right() + 0.5);
    });
}

#[test]
fn morph_rows_fit_narrow_default_and_wide_inspectors_without_width_feedback() {
    let inspector_widths = [
        INSPECTOR_MIN_WIDTH,
        INSPECTOR_DEFAULT_WIDTH,
        INSPECTOR_MAX_WIDTH,
    ];
    let mut previous_slider = 0.0;
    for inspector_width in inspector_widths {
        let panel_content = inspector_width - PANEL_INSET * 2.0;
        let row_content = panel_content - MORPH_ROW_HORIZONTAL_INSET * 2.0;
        let first = morph_control_widths(row_content);
        let second = morph_control_widths(row_content);

        assert_eq!(first, second, "row width must not depend on label content");
        assert!(
            (first.total() - row_content).abs() < f32::EPSILON,
            "morph controls must consume, but never exceed, the authored row width"
        );
        assert_eq!(first.reset, MORPH_RESET_WIDTH);
        assert!(first.slider >= previous_slider);
        previous_slider = first.slider;
    }
}

#[test]
fn release_size_morph_regions_and_sticky_actions_are_disjoint() {
    let inspector_content_width = INSPECTOR_MAX_WIDTH - PANEL_INSET * 2.0;
    let inspector_content_height = 1_440.0 - TOP_BAR_HEIGHT - STATUS_BAR_HEIGHT;
    let full = Rect::from_min_size(
        pos2(PANEL_INSET, TOP_BAR_HEIGHT),
        vec2(inspector_content_width, inspector_content_height),
    );
    let regions = inspector_shell_regions(full, MORPH_ACTION_FOOTER_HEIGHT);
    let buttons = morph_footer_buttons(regions.footer);

    assert_eq!(regions.body.bottom(), regions.footer.top());
    assert_eq!(regions.body.x_range(), regions.footer.x_range());
    assert_eq!(buttons.undo.size(), buttons.reset.size());
    assert_eq!(buttons.undo.y_range(), buttons.reset.y_range());
    assert!(buttons.undo.right() < buttons.reset.left());
    assert!(buttons.reset.top() >= regions.footer.top());
    assert!(buttons.reset.bottom() <= buttons.apply.top());
    assert!(buttons.apply.bottom() <= regions.footer.bottom());
    assert!(
        buttons.reset.bottom() + MORPH_FOOTER_BUTTON_GAP <= buttons.apply.top(),
        "sticky reset and apply controls must retain a visible gap"
    );
}

#[test]
fn every_inspector_footer_contract_partitions_without_overlap() {
    let full = Rect::from_min_size(pos2(17.0, 29.0), vec2(472.0, 900.0));
    for footer_height in [
        ALIGNMENT_FOOTER_HEIGHT,
        PRIMARY_FOOTER_HEIGHT,
        MORPH_ACTION_FOOTER_HEIGHT,
    ] {
        let regions = inspector_shell_regions(full, footer_height);

        assert_eq!(regions.body.min, full.min);
        assert_eq!(regions.footer.max, full.max);
        assert_eq!(regions.body.x_range(), full.x_range());
        assert_eq!(regions.footer.x_range(), full.x_range());
        assert_eq!(regions.body.bottom(), regions.footer.top());
        assert_eq!(regions.body.intersect(regions.footer).height(), 0.0);
        assert_eq!(
            regions.body.height() + regions.footer.height(),
            full.height()
        );
        assert_eq!(regions.footer.height(), footer_height);
    }
}

#[test]
fn short_inspector_gives_footer_the_available_height_without_inverted_regions() {
    let full = Rect::from_min_size(pos2(9.0, 11.0), vec2(320.0, 44.0));
    let regions = inspector_shell_regions(full, ALIGNMENT_FOOTER_HEIGHT);

    assert_eq!(regions.body.height(), 0.0);
    assert_eq!(regions.footer, full);
    assert!(regions.body.width() >= 0.0 && regions.body.height() >= 0.0);
    assert!(regions.footer.width() >= 0.0 && regions.footer.height() >= 0.0);
    assert_eq!(regions.body.bottom(), regions.footer.top());
}

#[test]
fn the_create_footer_gives_its_width_to_the_one_step_it_holds() {
    let footer = Rect::from_min_size(pos2(12.0, 20.0), vec2(344.0, PRIMARY_FOOTER_HEIGHT));
    let buttons = pin_footer_buttons(footer);
    assert!(buttons.generate.bottom() <= footer.bottom());
    assert!(buttons.generate.top() >= footer.top());
    assert_eq!(buttons.generate.x_range(), footer.x_range());
}

#[test]
fn inspector_shell_reclaims_scroll_lane_until_content_overflows() {
    egui::__run_test_ui(|ui| {
        ui.set_width(360.0);
        ui.set_height(420.0);
        let mut state = AppState::default();
        let mut content_width = 0.0;
        show_inspector_shell(
            ui,
            &mut state,
            "test.inspector",
            PRIMARY_FOOTER_HEIGHT,
            true,
            |ui, _state, _viewport| content_width = ui.available_width(),
            |_ui, _state, _footer| {},
        );

        assert!(content_width > 0.0);
        assert!(content_width > 360.0 - ScrollStyle::solid().allocated_width());
        assert!(content_width <= 360.0);
    });
}

#[test]
fn morph_width_budgets_never_exceed_narrow_rows() {
    for available in [40.0, 96.0, 180.0, 344.0] {
        let widths = morph_control_widths(available);
        assert!(widths.slider >= 0.0 && widths.reset >= 0.0);
        assert!(widths.total() <= available + f32::EPSILON);
    }
}

#[test]
fn transform_grid_reserves_a_narrow_axis_label_column() {
    for available in [180.0, 344.0, 600.0] {
        let field = transform_grid_column_width(available, 4.0);
        let axis = transform_grid_axis_label_width(available);
        assert_eq!(axis, TRANSFORM_AXIS_LABEL_WIDTH);
        assert!((field * 3.0 + axis + 12.0 - available).abs() < f32::EPSILON);
    }
}

#[test]
fn capsule_selectors_wrap_instead_of_overflowing_narrow_rows() {
    egui::__run_test_ui(|ui| {
        let cell = Rect::from_min_size(ui.cursor().min, vec2(64.0, 200.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(cell));
        child.set_width(cell.width());
        let labels = [
            "All",
            "Eyes",
            "Brows",
            "Nose",
            "Mouth",
            "Jaw",
            "Cheeks",
            "Ears",
            "Expression",
        ];
        let (rect, _) = chips(&mut child, Id::new("test.chips"), Some(0), &labels);
        assert!(rect.height() > 28.0);
        assert!(rect.right() <= cell.right() + f32::EPSILON);
    });
}

#[test]
fn morph_filter_cursor_finishes_before_the_virtual_list_region() {
    egui::__run_test_ui(|ui| {
        ui.set_width(INSPECTOR_MAX_WIDTH - PANEL_INSET * 2.0);
        ui.set_height(900.0);
        let mut state = AppState::default();
        let filters = draw_morph_filters(ui, &mut state);
        ui.add_space(MORPH_FILTER_LIST_GAP);
        let list = ui.available_rect_before_wrap();

        assert!(filters.category_rect.bottom() <= filters.search_rect.top());
        assert!(list.top() >= filters.rect.bottom() + MORPH_FILTER_LIST_GAP);
    });
}

#[test]
fn skin_and_morph_search_share_one_exact_capsule_geometry() {
    egui::__run_test_ui(|ui| {
        ui.set_width(360.0);
        let mut skin = String::new();
        let skin_rect = capsule_search_field(ui, "test.skin", &mut skin, "스킨 검색", true).rect;
        let mut morph = String::new();
        let morph_rect = capsule_search_field(ui, "test.morph", &mut morph, "모프 검색", true).rect;
        assert_eq!(skin_rect.width(), morph_rect.width());
        assert_eq!(skin_rect.height(), CONTROL_HEIGHT);
        assert_eq!(morph_rect.height(), CONTROL_HEIGHT);
    });
}

#[test]
fn eye_closure_behaves_like_a_searchable_eye_morph_row() {
    let mut state = AppState::default();
    assert_eq!(
        visible_morph_rows(&state),
        vec![VisibleMorphRow::EyeClosure]
    );

    state.morph_library.category_filter = MorphCategoryFilter::Category(MorphCategory::Mouth);
    assert!(visible_morph_rows(&state).is_empty());

    state.morph_library.category_filter = MorphCategoryFilter::Category(MorphCategory::Eyes);
    state.morph_library.query = "눈 감기".to_owned();
    assert_eq!(
        visible_morph_rows(&state),
        vec![VisibleMorphRow::EyeClosure]
    );

    state.locale = Locale::English;
    state.morph_library.query = "close eyes".to_owned();
    assert_eq!(
        visible_morph_rows(&state),
        vec![VisibleMorphRow::EyeClosure]
    );
}

#[test]
fn morph_child_cells_preserve_the_ancestor_scroll_clip() {
    egui::__run_test_ui(|ui| {
        let parent_clip = Rect::from_min_size(pos2(20.0, 20.0), vec2(300.0, 200.0));
        ui.set_clip_rect(parent_clip);
        let cell = Rect::from_min_size(pos2(30.0, 190.0), vec2(280.0, 80.0));
        let mut child = ui.new_child(UiBuilder::new().max_rect(cell));

        constrain_morph_child_clip(&mut child, cell);

        assert_eq!(child.clip_rect(), parent_clip.intersect(cell));
        assert!(child.clip_rect().bottom() <= parent_clip.bottom());
    });
}

#[test]
fn two_line_morph_content_fits_inside_each_virtual_row() {
    let authored_content_height = MORPH_ROW_LABEL_HEIGHT
        + MORPH_ROW_LINE_GAP
        + MORPH_ROW_CONTROL_HEIGHT
        + MORPH_ROW_VERTICAL_INSET * 2.0
        + 1.0;
    assert!(authored_content_height <= MORPH_ROW_HEIGHT);
    assert_eq!(MORPH_ROW_HEIGHT + MORPH_ROW_GAP, 47.0);
}

#[test]
fn exact_inspector_width_clips_oversized_content_instead_of_auto_expanding() {
    egui::__run_test_ui(|ui| {
        ui.set_width(1_200.0);
        let shown = Panel::right("test.fixed.inspector")
            .exact_size(INSPECTOR_MIN_WIDTH)
            .resizable(false)
            .show(ui, |ui| {
                ui.allocate_space(vec2(2_000.0, 12.0));
            });
        assert!((shown.response.rect.width() - INSPECTOR_MIN_WIDTH).abs() < 0.5);
    });
}

#[test]
fn morph_reset_hitbox_is_stable_at_every_supported_inspector_width() {
    for inspector_width in [
        INSPECTOR_MIN_WIDTH,
        INSPECTOR_DEFAULT_WIDTH,
        INSPECTOR_MAX_WIDTH,
    ] {
        let row_content = inspector_width - PANEL_INSET * 2.0 - MORPH_ROW_HORIZONTAL_INSET * 2.0;
        let widths = morph_control_widths(row_content);
        let content = Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2(
                row_content,
                MORPH_ROW_LABEL_HEIGHT + MORPH_ROW_LINE_GAP + MORPH_ROW_CONTROL_HEIGHT,
            ),
        );
        let columns = morph_row_columns(content);
        assert_eq!(widths.reset, MORPH_RESET_WIDTH);
        assert_eq!(columns.reset.height(), MORPH_ROW_CONTROL_HEIGHT);
        assert_eq!(
            columns.reset.center().y,
            content.top()
                + MORPH_ROW_LABEL_HEIGHT
                + MORPH_ROW_LINE_GAP
                + MORPH_ROW_CONTROL_HEIGHT * 0.5
        );
        assert_eq!(
            columns.primary.right() + widths.value_reset_gap,
            columns.reset.left()
        );
    }
}

#[test]
fn every_morph_category_has_a_localized_filter_key() {
    let keys = MorphCategory::ALL.map(morph_category_key);
    assert_eq!(keys.len(), MorphCategory::ALL.len());
    for key in keys {
        assert!(!text(Locale::Korean, key).is_empty());
        assert!(!text(Locale::English, key).is_empty());
    }
    assert_eq!(
        morph_category_key(MorphCategory::Head),
        TextKey::MorphCategoryHead
    );
    assert_eq!(
        morph_category_key(MorphCategory::Cheekbones),
        TextKey::MorphCategoryCheeks
    );
}

#[test]
fn skin_search_matches_labels_and_stable_ids_case_insensitively() {
    assert!(skin_matches_query("", "Natural Face", "creator.skin.01"));
    assert!(skin_matches_query(
        "natural",
        "Natural Face",
        "creator.skin.01"
    ));
    assert!(skin_matches_query(
        "CREATOR.SKIN",
        "Natural Face",
        "creator.skin.01"
    ));
    assert!(!skin_matches_query(
        "freckles",
        "Natural Face",
        "creator.skin.01"
    ));
}

#[test]
fn skin_display_name_strips_only_a_nonempty_preset_prefix() {
    assert_eq!(display_skin_label("Preset_Natural Face"), "Natural Face");
    assert_eq!(display_skin_label("Natural Face"), "Natural Face");
    assert_eq!(
        display_skin_label("preset_Natural Face"),
        "preset_Natural Face"
    );
    assert_eq!(display_skin_label("Preset_"), "Preset_");
}

#[test]
fn localized_morph_search_keeps_raw_label_and_stable_id_aliases() {
    let localized = morph_label_for(Locale::Korean, "vendor.PHMJawLineDepth", "Jaw Line Depth");
    assert!(localized_morph_query_matches(
        "턱선",
        &localized,
        "Jaw Line Depth",
        "vendor.PHMJawLineDepth"
    ));
    assert!(localized_morph_query_matches(
        "jaw line",
        &localized,
        "Jaw Line Depth",
        "vendor.PHMJawLineDepth"
    ));
    assert!(localized_morph_query_matches(
        "phmjawline",
        &localized,
        "Jaw Line Depth",
        "vendor.PHMJawLineDepth"
    ));
}

#[test]
fn japanese_morph_search_matches_ja_en_id_and_korean_queries() {
    let id = "builtin:PHMJawLineDepth";
    let raw = "Jaw Line Depth";
    let localized = morph_label_for(Locale::Japanese, id, raw);
    assert_eq!(localized, "フェイスラインの奥行き");
    assert!(localized_morph_query_matches(
        "フェイスライン",
        &localized,
        raw,
        id
    ));
    assert!(localized_morph_query_matches("奥行き", &localized, raw, id));
    assert!(localized_morph_query_matches(
        "jaw line", &localized, raw, id
    ));
    assert!(localized_morph_query_matches(
        "PHMJawLine",
        &localized,
        raw,
        id
    ));

    assert!(!localized_morph_query_matches("턱선", &localized, raw, id));

    let korean = morph_label_for(Locale::Korean, id, raw);
    assert!(localized_morph_query_matches("턱선", &korean, "", ""));
}

#[test]
fn verbatim_windows_paths_are_presented_without_the_kernel_prefix() {
    assert_eq!(
        readable_windows_path(Path::new(r"\\?\D:\Games\VaM")),
        r"D:\Games\VaM"
    );
    assert_eq!(
        readable_windows_path(Path::new(r"\\?\UNC\server\share\VaM")),
        r"\\server\share\VaM"
    );
}

#[test]
fn no_tab_name_outgrows_its_cell_at_the_full_tab_width() {
    for locale in Locale::ALL {
        let context = egui::Context::default();
        if crate::theme::configure_context(&context, locale)
            .fonts
            .is_empty()
        {
            eprintln!("skipping: no Windows system fonts available");
            return;
        }
        let _ = context.run_ui(egui::RawInput::default(), |ui| {
            for (_, key) in super::TOP_TABS {
                let label = text(locale, key);
                let fitted = crate::ui_components::ellipsize_to_width(
                    ui,
                    label,
                    super::TOP_TAB_WIDTH,
                    egui::FontId::proportional(crate::theme::FONT_BODY),
                );
                assert_eq!(
                    fitted, label,
                    "{locale:?} calls this tab {label:?}, which does not fit                      the {} points a tab gets",
                    super::TOP_TAB_WIDTH
                );
            }
        });
    }
}
