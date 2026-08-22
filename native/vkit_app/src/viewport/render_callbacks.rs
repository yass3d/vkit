use super::*;

use crate::hair_physics::HairSimulation;

pub(super) fn add_mesh_callback(
    ui: &Ui,
    view: SceneView,
    scene_key: u64,
    mesh: Arc<SurfaceMesh>,
    grading: ViewportGrading,
    smooth_passes: u8,
    draw: MeshDraw,
) {
    let SceneView {
        rect,
        camera,
        transform,
    } = view;
    let MeshDraw {
        color,
        style,
        depth_scope,
    } = draw;
    let callback = MeshPaintCallback {
        spot: crate::renderer::SceneSpot::default(),
        frame_radius: camera.frame_radius,
        scene_key: scene_key ^ scene_salt(ui),
        mesh,
        view_projection: camera.view_projection(rect.width() / rect.height().max(1.0)),
        model: transform.matrix(),
        eye: camera.eye(),
        light_yaw_radians: grading.light_yaw_radians,
        light_preset: grading.preset,
        light_brightness: grading.brightness,
        tone_mapping: grading.tone_mapping,
        smooth_passes,
        color,
        style,
        depth_scope,
    };
    ui.painter()
        .add(callback.paint_callback(rect, crate::renderer::SceneSpot::of(ui, rect)));
}

pub(super) fn add_skin_callback(
    ui: &Ui,
    view: SceneView,
    scene_key: u64,
    mesh: Arc<SurfaceMesh>,
    skin: Arc<crate::skin_preview::SkinPreview>,
    state: &AppState,
    draw: SkinDraw,
) {
    let SceneView {
        rect,
        camera,
        transform,
    } = view;
    let SkinDraw {
        visibility: skin_visibility,
        show_tear_lacrimals,
        show_eyelashes,
        depth_scope,
    } = draw;
    let callback = SkinPaintCallback {
        spot: crate::renderer::SceneSpot::default(),
        frame_radius: camera.frame_radius,
        scene_key: scene_key ^ scene_salt(ui),
        mesh,
        skin,
        view_projection: camera.view_projection(rect.width() / rect.height().max(1.0)),
        model: transform.matrix(),
        eye: camera.eye(),
        light_yaw_radians: state.light_yaw_radians,
        light_preset: state.lighting_preset,
        light_brightness: state.light_brightness,
        tone_mapping: state.tone_mapping,
        depth_scope,
        skin_visibility,
        show_tear_lacrimals,
        show_eyelashes,
        smooth_passes: state.surface_smooth_passes,
    };
    ui.painter()
        .add(callback.paint_callback(rect, crate::renderer::SceneSpot::of(ui, rect)));
}

#[expect(
    clippy::too_many_arguments,
    reason = "one call per pane; every argument is a distinct thing to draw"
)]
pub(super) fn add_hair_callback(
    ui: &Ui,
    state: &AppState,
    rect: Rect,
    mesh: Arc<SurfaceMesh>,
    preview: Arc<crate::hair_preview::HairPreview>,
    scene_key: u64,
    view: ResultRenderView,
    simulate: HairSimulation,
) {
    for (index, scalp) in preview.scalps.iter().enumerate() {
        let callback = ScalpPaintCallback {
            spot: crate::renderer::SceneSpot::default(),
            scene_key: scene_key ^ (HAIR_SCALP_SCENE_KEY + index as u64) ^ scene_salt(ui),
            head: Arc::clone(&mesh),
            part: scalp.clone(),
            view_projection: view
                .camera
                .view_projection(rect.width() / rect.height().max(1.0)),
            model: ModelTransform::default().matrix(),
            eye: view.camera.eye(),
            light_yaw_radians: view.grading.light_yaw_radians,
            light_preset: view.grading.preset,
            light_brightness: view.grading.brightness,
            tone_mapping: view.grading.tone_mapping,
        };
        ui.painter()
            .add(callback.paint_callback(rect, crate::renderer::SceneSpot::of(ui, rect)));
    }

    let settling = state.hair_settle_seconds > 0.0;

    let simulate_hair = simulate;
    let pump_frames = hair_pump_wanted(simulate_hair, state.hair_simulation_seconds);
    let callback = HairPaintCallback {
        spot: crate::renderer::SceneSpot::default(),
        scene_key: scene_key ^ scene_salt(ui),
        mesh,
        preview,
        view_projection: view
            .camera
            .view_projection(rect.width() / rect.height().max(1.0)),
        model: ModelTransform::default().matrix(),
        eye: view.camera.eye(),
        light_yaw_radians: view.grading.light_yaw_radians,
        light_preset: view.grading.preset,
        light_brightness: view.grading.brightness,
        tone_mapping: view.grading.tone_mapping,
        time_seconds: ui.input(|input| input.time),
        settle_gravity: if pump_frames && settling {
            crate::state::HAIR_SETTLE_GRAVITY
        } else {
            0.0
        },
        simulate_hair,
        solve: pump_frames,

        viewport_pixels: {
            let scale = ui.ctx().pixels_per_point();
            [rect.width() * scale, rect.height() * scale]
        },
        frame: ui.ctx().cumulative_pass_nr(),
    };
    ui.painter()
        .add(callback.paint_callback(rect, crate::renderer::SceneSpot::of(ui, rect)));
    if pump_frames {
        ui.ctx().request_repaint_after(Duration::from_millis(16));
    }
}

pub(super) const fn hair_pump_wanted(simulate: HairSimulation, budget_seconds: f32) -> bool {
    !matches!(simulate, HairSimulation::Off) && budget_seconds > 0.0
}

pub(super) fn sculpt_eye_look_preview(state: &AppState, gaze: [f32; 2]) -> Option<EyeLookPreview> {
    if !state.sculpt_eye_tracking {
        return None;
    }
    let template = state.workspace.template_geometry.as_deref()?;
    let result = state.workspace.result_output.as_deref()?;
    let eye_transform = |id: &str, fallback: [f64; 3], left_eye: bool| {
        let base_center = template
            .bone(id)
            .map(|bone| CoreVec3::from(bone.center_point))
            .unwrap_or_else(|| CoreVec3::from(fallback));
        let center =
            transfer_rig_point_idw(base_center, &template.vertices, &result.vertices).ok()?;
        let [yaw, pitch] = eye_angles_from_gaze(gaze, left_eye);
        Some((
            ModelTransform::from_components_with_pivot(
                1.0,
                [0.0; 3],
                [-pitch, yaw, 0.0],
                [center.x, center.y, center.z],
            ),
            pitch,
        ))
    };
    let (left, left_pitch) = eye_transform("lEye", [3.13, 168.03, 5.828_937], true)?;
    let (right, right_pitch) = eye_transform("rEye", [-3.13, 168.03, 5.828_937], false)?;
    Some(EyeLookPreview {
        left,
        right,
        left_pitch,
        right_pitch,
    })
}

pub(super) fn gaze_from_screen(pointer: Pos2, rect: Rect) -> [f32; 2] {
    let half_width = (rect.width() * 0.5).max(1.0);
    let half_height = (rect.height() * 0.5).max(1.0);
    [
        ((pointer.x - rect.center().x) / half_width).clamp(-1.0, 1.0),
        ((rect.center().y - pointer.y) / half_height).clamp(-1.0, 1.0),
    ]
}

pub(super) fn viewport_eye_gaze(ui: &Ui, state: &AppState, viewport: Rect) -> [f32; 2] {
    if state.eye_gaze_mode == EyeGazeMode::Manual {
        return state.manual_eye_gaze;
    }
    let cache_id = Id::new("vkit.viewport.eye-gaze.auto");
    if let Some(pointer) = ui.input(|input| input.pointer.hover_pos())
        && viewport.contains(pointer)
    {
        let gaze = gaze_from_screen(pointer, viewport);
        ui.data_mut(|data| data.insert_temp(cache_id, gaze));
        return gaze;
    }
    ui.data(|data| data.get_temp::<[f32; 2]>(cache_id))
        .unwrap_or(state.manual_eye_gaze)
}

pub(super) fn eye_angles_from_gaze(gaze: [f32; 2], left_eye: bool) -> [f64; 2] {
    crate::state::eye_angles_from_gaze(gaze, left_eye)
}

pub(super) fn grouped_skin_depth_sequence(callback_count: usize) -> Vec<RenderDepthScope> {
    (0..callback_count)
        .map(|index| {
            if index == 0 {
                RenderDepthScope::ResetBeforeDraw
            } else {
                RenderDepthScope::Shared
            }
        })
        .collect()
}

pub(super) fn add_sculpt_result_callbacks(
    ui: &Ui,
    rect: Rect,
    state: &AppState,
    view: ResultRenderView,
    eye_gaze: [f32; 2],
    result_mesh: Arc<SurfaceMesh>,
) {
    let visible = sculpt_visible_targets(state);
    let active = state.sculpt.editable_targets();
    let mut surfaces = state.workspace.result_sculpt_surfaces().collect::<Vec<_>>();
    surfaces.retain(|surface| visible.contains(sculpt_group_target(surface.group)));
    let eye_preview =
        sculpt_eye_look_preview(state, eye_gaze).zip(state.workspace.result_eye_surfaces());
    if eye_preview.is_some() {
        surfaces.retain(|surface| surface.group != SculptSurfaceGroup::Eyes);
    }
    let has_grouped_surfaces = !surfaces.is_empty() || eye_preview.is_some();

    let skin = (state.base_view_mode == BaseViewMode::Texture)
        .then(|| state.active_skin_preview())
        .flatten();

    if let Some(skin) = skin {
        if has_grouped_surfaces {
            let eye_callback_count =
                usize::from(visible.contains(SculptTarget::Eyes) && eye_preview.is_some()) * 2;
            let mut depth_sequence =
                grouped_skin_depth_sequence(surfaces.len() + eye_callback_count).into_iter();
            for surface in &surfaces {
                if sculpt_group_needs_solid(&skin, surface.group) {
                    let _ = depth_sequence.next();
                    add_mesh_callback(
                        ui,
                        SceneView {
                            rect,
                            camera: view.camera,
                            transform: ModelTransform::default(),
                        },
                        sculpt_group_scene_key(surface.group),
                        Arc::clone(&surface.mesh),
                        view.grading,
                        view.smooth_passes,
                        MeshDraw {
                            color: color_array(g2_solid_color(state), 1.0),
                            style: RenderStyle::Solid,
                            depth_scope: RenderDepthScope::Shared,
                        },
                    );
                    continue;
                }
                add_skin_callback(
                    ui,
                    SceneView {
                        rect,
                        camera: view.camera,
                        transform: ModelTransform::default(),
                    },
                    sculpt_group_scene_key(surface.group),
                    Arc::clone(&surface.mesh),
                    Arc::clone(&skin),
                    state,
                    SkinDraw {
                        visibility: sculpt_skin_visibility(visible),
                        show_tear_lacrimals: true,
                        show_eyelashes: true,
                        depth_scope: depth_sequence
                            .next()
                            .expect("grouped skin depth sequence covers every surface"),
                    },
                );
            }
            if visible.contains(SculptTarget::Eyes)
                && let Some((eye_preview, (left_eye, right_eye))) = &eye_preview
            {
                for (scene_key, mesh, transform) in [
                    (
                        RESULT_SCULPT_LEFT_EYE_SCENE_KEY,
                        Arc::clone(left_eye),
                        eye_preview.left,
                    ),
                    (
                        RESULT_SCULPT_RIGHT_EYE_SCENE_KEY,
                        Arc::clone(right_eye),
                        eye_preview.right,
                    ),
                ] {
                    add_skin_callback(
                        ui,
                        SceneView {
                            rect,
                            camera: view.camera,
                            transform,
                        },
                        scene_key,
                        mesh,
                        Arc::clone(&skin),
                        state,
                        SkinDraw {
                            visibility: sculpt_skin_visibility(visible),
                            show_tear_lacrimals: true,
                            show_eyelashes: true,
                            depth_scope: depth_sequence
                                .next()
                                .expect("grouped skin depth sequence covers both eyes"),
                        },
                    );
                }
            }
        } else {
            add_skin_callback(
                ui,
                SceneView {
                    rect,
                    camera: view.camera,
                    transform: ModelTransform::default(),
                },
                RESULT_SKIN_SCENE_KEY,
                Arc::clone(&result_mesh),
                skin,
                state,
                SkinDraw {
                    visibility: sculpt_skin_visibility(visible),
                    show_tear_lacrimals: true,
                    show_eyelashes: true,
                    depth_scope: RenderDepthScope::ResetBeforeDraw,
                },
            );
        }
    } else if !has_grouped_surfaces {
        if visible.contains(SculptTarget::HeadSkin) {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera: view.camera,
                    transform: ModelTransform::default(),
                },
                RESULT_SCENE_KEY,
                Arc::clone(&result_mesh),
                view.grading,
                view.smooth_passes,
                MeshDraw {
                    color: color_array(g2_solid_color(state), 1.0),
                    style: RenderStyle::Solid,
                    depth_scope: RenderDepthScope::Shared,
                },
            );
        }
    } else {
        for surface in &surfaces {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera: view.camera,
                    transform: ModelTransform::default(),
                },
                sculpt_group_scene_key(surface.group),
                Arc::clone(&surface.mesh),
                view.grading,
                view.smooth_passes,
                MeshDraw {
                    color: color_array(g2_solid_color(state), 1.0),
                    style: RenderStyle::Solid,
                    depth_scope: RenderDepthScope::Shared,
                },
            );
        }
        if visible.contains(SculptTarget::Eyes)
            && let Some((eye_preview, (left_eye, right_eye))) = &eye_preview
        {
            for (scene_key, mesh, transform) in [
                (
                    RESULT_SCULPT_LEFT_EYE_SCENE_KEY,
                    Arc::clone(left_eye),
                    eye_preview.left,
                ),
                (
                    RESULT_SCULPT_RIGHT_EYE_SCENE_KEY,
                    Arc::clone(right_eye),
                    eye_preview.right,
                ),
            ] {
                add_mesh_callback(
                    ui,
                    SceneView {
                        rect,
                        camera: view.camera,
                        transform,
                    },
                    scene_key,
                    mesh,
                    view.grading,
                    view.smooth_passes,
                    MeshDraw {
                        color: color_array(g2_solid_color(state), 1.0),
                        style: RenderStyle::Solid,
                        depth_scope: RenderDepthScope::Shared,
                    },
                );
            }
        }
    }

    let wire_alpha = overlay_alpha(state.wireframe_opacity);
    if state.wireframe_visible && wire_alpha > 0.0 {
        if !has_grouped_surfaces {
            if !visible.contains(SculptTarget::HeadSkin) {
                return;
            }
            let (color, alpha) = sculpt_group_wire_style(
                wireframe_color(state),
                active,
                SculptSurfaceGroup::HeadSkin,
                wire_alpha,
            );
            add_colored_result_wire_callback(
                ui,
                rect,
                RESULT_SCENE_KEY,
                Arc::clone(&result_mesh),
                view,
                color,
                alpha,
            );
        } else {
            for surface in &surfaces {
                let (color, alpha) = sculpt_group_wire_style(
                    wireframe_color(state),
                    active,
                    surface.group,
                    wire_alpha,
                );
                add_colored_result_wire_callback(
                    ui,
                    rect,
                    sculpt_group_scene_key(surface.group),
                    Arc::clone(&surface.mesh),
                    view,
                    color,
                    alpha,
                );
            }
        }
    }

    let xray_alpha = overlay_alpha(state.xray_opacity);
    if state.xray_visible && xray_alpha > 0.0 {
        if !has_grouped_surfaces {
            if !visible.contains(SculptTarget::HeadSkin) {
                return;
            }
            add_result_xray_callback(
                ui,
                rect,
                RESULT_SCENE_KEY,
                result_mesh,
                view,
                XrayDraw {
                    color: g2_solid_color(state),
                    alpha: xray_alpha,
                    depth_scope: RenderDepthScope::ResetBeforeDraw,
                },
            );
        } else {
            for (index, surface) in surfaces.iter().enumerate() {
                add_result_xray_callback(
                    ui,
                    rect,
                    sculpt_group_scene_key(surface.group),
                    Arc::clone(&surface.mesh),
                    view,
                    XrayDraw {
                        color: g2_solid_color(state),
                        alpha: xray_alpha,
                        depth_scope: if index == 0 {
                            RenderDepthScope::ResetBeforeDraw
                        } else {
                            RenderDepthScope::Shared
                        },
                    },
                );
            }
        }
    }
}

pub(super) const fn sculpt_group_target(group: SculptSurfaceGroup) -> SculptTarget {
    match group {
        SculptSurfaceGroup::HeadSkin => SculptTarget::HeadSkin,
        SculptSurfaceGroup::TearLacrimal => SculptTarget::Tear,
        SculptSurfaceGroup::Eyelashes => SculptTarget::Eyelashes,
        SculptSurfaceGroup::Eyes => SculptTarget::Eyes,
        SculptSurfaceGroup::Lips => SculptTarget::Lips,
        SculptSurfaceGroup::InnerMouth => SculptTarget::InnerMouth,
        SculptSurfaceGroup::TeethTongue => SculptTarget::TeethTongue,
    }
}

pub(super) fn skin_channel_is_visible(image: &crate::skin_preview::SkinImage) -> bool {
    image
        .rgba8
        .chunks_exact(4)
        .any(|pixel| pixel[3] > SKIN_CHANNEL_VISIBLE_ALPHA)
}

pub(super) const SKIN_CHANNEL_VISIBLE_ALPHA: u8 = 8;

pub(super) fn sculpt_groups_without_skin_coverage(
    skin: &crate::skin_preview::SkinPreview,
) -> [bool; 2] {
    [
        !skin_channel_is_visible(&skin.lacrimal),
        !skin_channel_is_visible(&skin.eyelashes),
    ]
}

pub(super) fn sculpt_group_needs_solid(
    skin: &crate::skin_preview::SkinPreview,
    group: SculptSurfaceGroup,
) -> bool {
    let [tear_blank, lashes_blank] = sculpt_groups_without_skin_coverage(skin);
    match group {
        SculptSurfaceGroup::TearLacrimal => tear_blank,
        SculptSurfaceGroup::Eyelashes => lashes_blank,
        _ => false,
    }
}

pub(super) fn sculpt_skin_visibility(visible: SculptTargets) -> SkinVisibilityGroups {
    let mut groups = SkinVisibilityGroups::NONE;
    for (target, group) in [
        (SculptTarget::HeadSkin, SkinVisibilityGroup::HeadSkin),
        (SculptTarget::Lips, SkinVisibilityGroup::HeadSkin),
        (SculptTarget::Tear, SkinVisibilityGroup::Tear),
        (SculptTarget::Eyelashes, SkinVisibilityGroup::Eyelashes),
        (SculptTarget::Eyes, SkinVisibilityGroup::Eyes),
        (SculptTarget::TeethTongue, SkinVisibilityGroup::TeethTongue),
        (SculptTarget::InnerMouth, SkinVisibilityGroup::InnerMouth),
    ] {
        if visible.contains(target) {
            groups = groups.with(group);
        }
    }
    groups
}

pub(super) const fn sculpt_group_scene_key(group: SculptSurfaceGroup) -> u64 {
    match group {
        SculptSurfaceGroup::HeadSkin => RESULT_SCULPT_HEAD_SCENE_KEY,
        SculptSurfaceGroup::TearLacrimal => RESULT_SCULPT_TEAR_SCENE_KEY,
        SculptSurfaceGroup::Eyelashes => RESULT_SCULPT_LASH_SCENE_KEY,
        SculptSurfaceGroup::Eyes => RESULT_SCULPT_EYES_SCENE_KEY,
        SculptSurfaceGroup::Lips => RESULT_SCULPT_LIPS_SCENE_KEY,
        SculptSurfaceGroup::InnerMouth => RESULT_SCULPT_INNER_MOUTH_SCENE_KEY,
        SculptSurfaceGroup::TeethTongue => RESULT_SCULPT_MOUTH_SCENE_KEY,
    }
}

pub(super) fn overlay_alpha(opacity: f32) -> f32 {
    if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub(super) fn result_scan_overlay_alpha(state: &AppState) -> Option<f32> {
    let alpha = overlay_alpha(state.overlay_opacity);
    (state.active_tab == Tab::Result && alpha > 0.0).then_some(alpha)
}

pub(super) const fn sculpt_group_wire_style(
    wire_color: Color32,
    active: SculptTargets,
    group: SculptSurfaceGroup,
    opacity: f32,
) -> (Color32, f32) {
    if active.contains(sculpt_group_target(group)) {
        (wire_color, opacity)
    } else {
        (wire_color, opacity * SCULPT_INACTIVE_WIRE_FADE)
    }
}

pub(super) const SCULPT_INACTIVE_WIRE_FADE: f32 = 0.42;

pub(super) fn render_pass_plan(
    base_mode: BaseViewMode,
    texture_compatible: bool,
    texture_available: bool,
    wire_visible: bool,
    wire_opacity: f32,
    xray_visible: bool,
    xray_opacity: f32,
) -> RenderPassPlan {
    let textured = base_mode == BaseViewMode::Texture && texture_compatible && texture_available;
    RenderPassPlan {
        solid: !textured,
        textured,
        wire: wire_visible && overlay_alpha(wire_opacity) > 0.0,
        xray: xray_visible && overlay_alpha(xray_opacity) > 0.0,
    }
}

pub(super) fn add_result_composed_callbacks(
    ui: &Ui,
    rect: Rect,
    state: &AppState,
    view: ResultRenderView,
    result_mesh: Arc<SurfaceMesh>,
) {
    let mut meshes = vec![(RESULT_SCENE_KEY, Arc::clone(&result_mesh))];
    for (scene_key, enabled, part) in [
        (
            RESULT_TEAR_LACRIMAL_SCENE_KEY,
            state.show_result_tear_lacrimals,
            state.workspace.result_tear_lacrimals.clone(),
        ),
        (
            RESULT_EYELASH_SCENE_KEY,
            state.show_result_eyelashes,
            state.workspace.result_eyelashes.clone(),
        ),
    ] {
        if enabled && let Some(part) = part {
            meshes.push((scene_key, part));
        }
    }
    let skin = state.active_skin_preview();
    let plan = render_pass_plan(
        state.base_view_mode,
        true,
        skin.is_some(),
        state.wireframe_visible,
        state.wireframe_opacity,
        state.xray_visible,
        state.xray_opacity,
    );
    if plan.textured {
        add_skin_callback(
            ui,
            SceneView {
                rect,
                camera: view.camera,
                transform: ModelTransform::default(),
            },
            RESULT_SKIN_SCENE_KEY,
            Arc::clone(&result_mesh),
            skin.expect("textured render plan requires an available skin"),
            state,
            SkinDraw {
                visibility: SkinVisibilityGroups::ALL,
                show_tear_lacrimals: state.show_result_tear_lacrimals,
                show_eyelashes: state.show_result_eyelashes,
                depth_scope: RenderDepthScope::ResetBeforeDraw,
            },
        );
    } else if plan.solid {
        for (scene_key, mesh) in &meshes {
            add_mesh_callback(
                ui,
                SceneView {
                    rect,
                    camera: view.camera,
                    transform: ModelTransform::default(),
                },
                *scene_key,
                Arc::clone(mesh),
                view.grading,
                view.smooth_passes,
                MeshDraw {
                    color: color_array(g2_solid_color(state), 1.0),
                    style: RenderStyle::Solid,
                    depth_scope: RenderDepthScope::Shared,
                },
            );
        }
    }
    if plan.wire {
        let alpha = overlay_alpha(state.wireframe_opacity);
        for (scene_key, mesh) in &meshes {
            add_colored_result_wire_callback(
                ui,
                rect,
                *scene_key,
                Arc::clone(mesh),
                view,
                wireframe_color(state),
                alpha,
            );
        }
    }
    if plan.xray {
        let alpha = overlay_alpha(state.xray_opacity);
        for (index, (scene_key, mesh)) in meshes.iter().enumerate() {
            add_result_xray_callback(
                ui,
                rect,
                *scene_key,
                Arc::clone(mesh),
                view,
                XrayDraw {
                    color: g2_solid_color(state),
                    alpha,
                    depth_scope: if index == 0 {
                        RenderDepthScope::ResetBeforeDraw
                    } else {
                        RenderDepthScope::Shared
                    },
                },
            );
        }
    }
}

pub(super) fn add_colored_result_wire_callback(
    ui: &Ui,
    rect: Rect,
    scene_key: u64,
    mesh: Arc<SurfaceMesh>,
    view: ResultRenderView,
    color: Color32,
    alpha: f32,
) {
    add_mesh_callback(
        ui,
        SceneView {
            rect,
            camera: view.camera,
            transform: ModelTransform::default(),
        },
        scene_key ^ VIEWPORT_WIRE_KEY_MASK,
        mesh,
        view.grading,
        view.smooth_passes,
        MeshDraw {
            color: color_array(color, alpha),
            style: RenderStyle::Wire,
            depth_scope: RenderDepthScope::Shared,
        },
    );
}

#[derive(Clone, Copy, Debug)]
pub(super) struct XrayDraw {
    color: Color32,
    alpha: f32,
    depth_scope: RenderDepthScope,
}

pub(super) fn add_result_xray_callback(
    ui: &Ui,
    rect: Rect,
    scene_key: u64,
    mesh: Arc<SurfaceMesh>,
    view: ResultRenderView,
    draw: XrayDraw,
) {
    let XrayDraw {
        color,
        alpha,
        depth_scope,
    } = draw;
    add_mesh_callback(
        ui,
        SceneView {
            rect,
            camera: view.camera,
            transform: ModelTransform::default(),
        },
        scene_key ^ VIEWPORT_XRAY_KEY_MASK,
        mesh,
        view.grading,
        view.smooth_passes,
        MeshDraw {
            color: color_array(color, alpha),
            style: RenderStyle::Xray,
            depth_scope,
        },
    );
}

pub(super) fn scan_transform(state: &AppState) -> ModelTransform {
    let pivot_local = state
        .workspace
        .scan
        .as_deref()
        .map(|scan| scan.facial_focus_bounds().center().as_dvec3().to_array())
        .unwrap_or([0.0; 3]);
    ModelTransform::from_components_xyz_with_pivot(
        state.transform.scale_xyz,
        state.transform.translation_cm,
        state.transform.rotation_degrees,
        pivot_local,
    )
}

#[cfg(test)]
pub(super) const fn alignment_scan_style() -> RenderStyle {
    RenderStyle::Solid
}

#[cfg(test)]
pub(super) const fn alignment_template_style() -> RenderStyle {
    RenderStyle::Solid
}

#[cfg(test)]
pub(super) const fn alignment_template_depth_scope() -> RenderDepthScope {
    RenderDepthScope::Shared
}

#[cfg(test)]
pub(super) const fn alignment_scan_depth_scope() -> RenderDepthScope {
    RenderDepthScope::ResetBeforeDraw
}

pub(super) fn alignment_layer_alpha(opacity: f32) -> f32 {
    opacity.clamp(0.0, 1.0)
}
