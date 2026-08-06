use super::*;
use crate::importers::{MeshImportPhase, MeshImportProgress};
use serde_json::json;
use std::sync::Arc;
use vkit_core::MIN_FIT_PAIRS;
use vkit_core::formats::{
    DazGeometry, Mesh, MorphTarget, ObjFace, ObjMorphSource, load_obj_morph_target,
};
use vkit_core::vam::{
    AssetLocator, G2UvTriangle, HairLookPatch, HairPartReference, HairPreset, SkinPreset, SkinSex,
    UvMaterialRegion,
};

static VAM_PAIR_TEST_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn assert_vertices_close(left: &[[f64; 3]], right: &[[f64; 3]]) {
    assert_eq!(left.len(), right.len());
    for (index, (left, right)) in left.iter().zip(right).enumerate() {
        for axis in 0..3 {
            assert!(
                (left[axis] - right[axis]).abs() <= 1.0e-12,
                "vertex {index} axis {axis}: {} != {}",
                left[axis],
                right[axis]
            );
        }
    }
}

fn tiny_template() -> DazGeometry {
    DazGeometry::new(
        "tiny".into(),
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        vec![vec![0, 1, 2]],
        vkit_core::formats::GroupTable {
            indices: vec![0],
            names: vec!["Head".into()],
        },
        vkit_core::formats::GroupTable {
            indices: vec![0],
            names: vec!["Face".into()],
        },
        json!({}),
    )
    .unwrap()
}

fn canonical_body_template() -> DazGeometry {
    let mut vertices = vec![[0.0, 0.0, 0.0]; VAM_SHARED_BODY_VERTEX_COUNT as usize];
    vertices[1] = [1.0, 0.0, 0.0];
    vertices[2] = [0.0, 1.0, 0.0];
    DazGeometry::new(
        "canonical-test".into(),
        vertices,
        vec![vec![0, 1, 2]],
        vkit_core::formats::GroupTable {
            indices: vec![0],
            names: vec!["Body".into()],
        },
        vkit_core::formats::GroupTable {
            indices: vec![0],
            names: vec!["Skin".into()],
        },
        json!({}),
    )
    .unwrap()
}

fn write_test_vam_pair(
    label: &str,
    metadata: &vkit_core::vam::VmiDocument,
    vmb: &[u8],
) -> (PathBuf, PathBuf) {
    write_test_vam_pair_at_route(label, "female", metadata, vmb)
}

fn write_test_vam_pair_at_route(
    label: &str,
    route_leaf: &str,
    metadata: &vkit_core::vam::VmiDocument,
    vmb: &[u8],
) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "vkit-state-vam-pair-{label}-{}-{}",
        std::process::id(),
        VAM_PAIR_TEST_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let route = root
        .join("Custom")
        .join("Atom")
        .join("Person")
        .join("Morphs")
        .join(route_leaf);
    std::fs::create_dir_all(&route).unwrap();
    let vmi_path = route.join("shape.vmi");
    let vmb_path = route.join("shape.vmb");
    std::fs::write(
        &vmi_path,
        vkit_core::vam::encode_vmi_pretty(metadata).unwrap(),
    )
    .unwrap();
    std::fs::write(&vmb_path, vmb).unwrap();
    (vmi_path, vmb_path)
}

fn tiny_output() -> OrderedObjMesh {
    OrderedObjMesh {
        vertices: tiny_template().vertices,
        faces: vec![ObjFace {
            vertex_indices: vec![0, 1, 2],
            group: Some("Face".into()),
            material: Some("Face".into()),
        }],
    }
}

fn tiny_morph() -> MorphTarget {
    let geometry = tiny_template();
    let mut target = geometry.to_ordered_obj(None).unwrap();
    target.vertices[1][1] -= 0.2;
    load_obj_morph_target(
        "eye_closed",
        ObjMorphSource {
            target: &target,
            base_vertices: &geometry.vertices,
            base_faces: &geometry.faces,
            unit_scale: Some(1.0),
        },
        1.0e-12,
        0.0,
        1.0,
        0.0,
    )
    .unwrap()
}

fn review_template() -> DazGeometry {
    let faces = vec![
        vec![0, 2, 4],
        vec![2, 1, 4],
        vec![1, 3, 4],
        vec![3, 0, 4],
        vec![2, 0, 5],
        vec![1, 2, 5],
        vec![3, 1, 5],
        vec![0, 3, 5],
    ];
    DazGeometry::new(
        "review".into(),
        vec![
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ],
        faces.clone(),
        vkit_core::formats::GroupTable {
            indices: vec![0; faces.len()],
            names: vec!["Head".into()],
        },
        vkit_core::formats::GroupTable {
            indices: vec![0; faces.len()],
            names: vec!["Face".into()],
        },
        json!({}),
    )
    .unwrap()
}

fn review_morph() -> MorphTarget {
    let geometry = review_template();
    let mut target = geometry.to_ordered_obj(None).unwrap();
    target.vertices[4][1] -= 0.2;
    load_obj_morph_target(
        "eye_closed",
        ObjMorphSource {
            target: &target,
            base_vertices: &geometry.vertices,
            base_faces: &geometry.faces,
            unit_scale: Some(1.0),
        },
        1.0e-12,
        0.0,
        1.0,
        0.0,
    )
    .unwrap()
}

fn ready_state() -> AppState {
    let mut state = AppState {
        scan_path: Some(PathBuf::from("scan.obj")),
        template_path: Some(PathBuf::from("Genesis2Female.dsf")),
        complete_pairs: MIN_FIT_PAIRS,
        ..Default::default()
    };
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    state.dispatch(Action::BeginGeneration);
    let ResultState::Running {
        job_id,
        source_revision,
    } = state.result
    else {
        panic!("generation did not start");
    };
    state.dispatch(Action::FinishGeneration {
        job_id,
        source_revision,
        outcome: GenerationOutcome::Success {
            output: tiny_output(),
            fit_reference_value: 0.0,
        },
        morph_available: true,
    });
    state
}

#[test]
fn refitting_carries_the_morph_work_across_it() {
    let mut state = ready_state();
    state.morph_library.replace_source(
        MorphSource::Curated,
        vec![MorphControl::new(
            "existing-control",
            "Existing control",
            MorphCategory::Eyes,
            MorphSource::Curated,
            Arc::new(tiny_morph()),
        )],
    );
    assert!(state.morph_library.set_value("existing-control", 0.75));

    state.dispatch(Action::BeginGeneration);
    let ResultState::Running {
        job_id,
        source_revision,
    } = state.result
    else {
        panic!("the refit did not start");
    };
    state.dispatch(Action::FinishGeneration {
        job_id,
        source_revision,
        outcome: GenerationOutcome::Success {
            output: tiny_output(),
            fit_reference_value: 0.0,
        },
        morph_available: true,
    });

    let kept = state
        .morph_library
        .controls()
        .iter()
        .find(|candidate| candidate.id == "existing-control")
        .map(|control| control.value);
    assert_eq!(kept, Some(0.75), "the refit spent the user's morph work");
}

#[test]
fn scanless_g2_base_enters_detail_while_pins_remain_unavailable() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));

    assert!(state.tab_available(Tab::Edit));
    assert!(!state.tab_available(Tab::Morph));
    state.dispatch(Action::RequestTab(Tab::Morph));

    assert_eq!(state.active_tab, Tab::Morph);
    assert!(state.tab_available(Tab::Edit));
    assert!(state.tab_available(Tab::Morph));
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        tiny_output().vertices
    );
}

#[test]
fn losing_the_shape_under_the_detail_stage_falls_back_instead_of_stranding() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    state.dispatch(Action::RequestTab(Tab::Morph));
    assert_eq!(state.active_tab, Tab::Morph);

    state.clear_generated_buffers();
    state.result = ResultState::Empty;
    state.settle_active_tab();

    assert_ne!(
        state.active_tab,
        Tab::Morph,
        "a stage that can no longer be entered must not stay selected"
    );
    assert!(
        state.tab_available(state.active_tab),
        "the fallback must itself be reachable"
    );
    assert_eq!(state.active_tab, state.create_landing_tab());

    state.active_tab = Tab::Morph;
    state.dispatch(Action::SetOverlayOpacity(0.4));
    assert_eq!(
        state.active_tab,
        Tab::Morph,
        "an unrelated dispatch must not relocate an already-stranded caller"
    );
}

#[test]
fn an_unrelated_action_never_relocates_the_active_stage() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    state.dispatch(Action::RequestTab(Tab::Morph));

    state.dispatch(Action::ToggleProjection);
    state.dispatch(Action::SetLocale(state.locale));

    assert_eq!(state.active_tab, Tab::Morph);
}

#[test]
fn create_flow_lands_on_the_split_once_the_g2_base_is_ready() {
    let mut state = AppState::default();
    assert_eq!(state.create_landing_tab(), Tab::Alignment);

    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    assert_eq!(state.create_landing_tab(), Tab::Edit);
}

#[test]
fn swapping_the_appearance_keeps_the_expression_on_the_new_face() {
    let mut state = ready_state();
    state.morph_library.replace_source(
        MorphSource::Curated,
        vec![MorphControl::new(
            "test.expression",
            "Test expression",
            MorphCategory::Mouth,
            MorphSource::Curated,
            Arc::new(tiny_morph()),
        )],
    );
    state.dispatch(Action::SetFaceMorph {
        id: "test.expression".into(),
        value: 0.75,
    });
    let expression = state
        .workspace
        .result_output
        .as_ref()
        .unwrap()
        .vertices
        .clone();

    state.pending_edit_carry = state.capture_edit_carry();
    assert!(
        state.pending_edit_carry.is_some(),
        "a morph value is an edit worth carrying"
    );

    state
        .install_direct_edit_output(Arc::new(tiny_output()), None, Vec::new())
        .unwrap();

    assert_eq!(
        state
            .morph_library
            .controls()
            .iter()
            .find(|control| control.id == "test.expression")
            .map(|control| control.value),
        Some(0.75),
        "the slider keeps the value across the swap"
    );
    assert_eq!(
        state.result_preview_phase,
        ResultPreviewPhase::Morph,
        "and the face is composed from it, without touching a slider to ask"
    );
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        expression
    );
}

#[test]
fn direct_morph_edit_reuses_the_selected_vmi_as_the_default_save_target() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    let selected = PathBuf::from(r"C:\VaM\Custom\Atom\Person\Morphs\female\Face.vmi");

    state
        .install_direct_edit_output(Arc::new(tiny_output()), Some(&selected), Vec::new())
        .unwrap();

    assert_eq!(state.output_path, selected.to_string_lossy());
    assert_eq!(state.output_topology, OutputTopology::VaM);

    let open_y = state.workspace.result_output.as_ref().unwrap().vertices[1][1];
    state.dispatch(Action::SetEyeClosure(0.5));
    assert_eq!(state.eye_closure, 0.5);
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices[1][1],
        open_y - 0.1
    );
}

#[test]
fn morph_compare_blends_toward_the_loaded_shape_without_moving_the_export() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    state.set_edit_source_mode(EditSourceMode::CustomMorph);
    state
        .install_direct_edit_output(Arc::new(tiny_output()), None, Vec::new())
        .unwrap();

    let origin_y = tiny_output().vertices[1][1];
    state.dispatch(Action::SetEyeClosure(0.5));
    state.active_tab = Tab::Result;
    let baked_y = state.workspace.result_output.as_ref().unwrap().vertices[1][1];
    assert_ne!(origin_y, baked_y);

    assert!(state.morph_compare_available());
    assert_eq!(state.morph_compare_blend, 1.0);
    assert!(
        state.morph_compare_vertices().is_none(),
        "the final shape is already on screen and needs no override"
    );

    state.dispatch(Action::SetMorphCompareBlend(0.0));
    let loaded = state
        .morph_compare_vertices()
        .expect("loaded-shape override");
    assert!((loaded[1][1] - origin_y).abs() <= 1.0e-12);

    state.dispatch(Action::SetMorphCompareBlend(0.5));
    let halfway = state.morph_compare_vertices().expect("halfway override");
    assert!((halfway[1][1] - (origin_y + baked_y) / 2.0).abs() <= 1.0e-12);

    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices[1][1],
        baked_y
    );
}

#[test]
fn morph_compare_is_absent_for_the_scan_head_flow() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    state.set_edit_source_mode(EditSourceMode::CustomMorph);
    state
        .install_direct_edit_output(Arc::new(tiny_output()), None, Vec::new())
        .unwrap();
    assert!(state.morph_compare_available());

    state.set_edit_source_mode(EditSourceMode::ScanHead);

    assert!(!state.morph_compare_available());
    assert!(state.custom_morph_origin.is_none());
}

#[test]
fn edit_source_query_survives_catalog_refresh() {
    let mut state = AppState {
        vam_root: Some(PathBuf::from(r"C:\VaM")),
        vam_edit_query: "jaw".to_owned(),
        ..Default::default()
    };

    state.dispatch(Action::RefreshVaMCatalog);

    assert_eq!(state.vam_edit_query, "jaw");
    assert!(matches!(
        state.vam_catalog_status,
        VaMCatalogStatus::Indexing
    ));
}

fn skin_preset(id: &str) -> SkinPreset {
    SkinPreset {
        stable_id: id.to_owned(),
        label: id.to_owned(),
        source: AssetLocator::UnresolvedVaMUrl(format!("preset://{id}")),
        sex: SkinSex::Female,
        textures: {
            let mut set = vkit_core::vam::SkinTextureSet::default();
            set.insert(
                vkit_core::vam::SkinRegion::Face,
                vkit_core::vam::SkinTextureChannel::Diffuse,
                AssetLocator::UnresolvedVaMUrl(format!("texture://{id}/face")),
            );
            set
        },
        auxiliary: vkit_core::vam::SkinAuxiliary::default(),
    }
}

fn hair_preset(id: &str) -> HairPreset {
    HairPreset {
        stable_id: id.to_owned(),
        label: id.to_owned(),
        source: AssetLocator::UnresolvedVaMUrl(format!("preset://{id}")),
        thumbnail: None,
        sex: SkinSex::Female,
        parts: vec![HairPartReference {
            reference: format!("Custom/Hair/Female/{id}.vam"),
            internal_id: "hair".to_owned(),
            geometry: AssetLocator::UnresolvedVaMUrl(format!("geometry://{id}")),
            appearance: None,
            preset_look: HairLookPatch::default(),
            preset_physics: Default::default(),
        }],
    }
}

fn hair_preview(id: &str) -> Arc<HairPreview> {
    Arc::new(HairPreview {
        preset_id: id.to_owned(),
        parts: Vec::new(),
        scalps: Vec::new(),
        skipped_parts: Vec::new(),
        body_capsules: Vec::new(),
    })
}

fn hair_preview_missing_a_part(id: &str) -> Arc<HairPreview> {
    Arc::new(HairPreview {
        preset_id: id.to_owned(),
        parts: Vec::new(),
        scalps: Vec::new(),
        skipped_parts: vec!["pack.var::hair/base.vab".to_owned()],
        body_capsules: Vec::new(),
    })
}

fn skin_uv_mapping() -> Arc<G2UvMapping> {
    Arc::new(G2UvMapping {
        source_path: PathBuf::from("femalecustom.obj"),
        coordinate_rms_cm: 0.0,
        coordinate_max_cm: 0.0,
        uncovered_triangles: 0,
        faces: Vec::new(),
        triangles: vec![G2UvTriangle {
            canonical_face_index: 0,
            canonical_triangle_index: 0,
            material_region: UvMaterialRegion::Face,
            on_head: true,
            position_indices: [0, 1, 2],
            uvs: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        }],
    })
}

fn skin_preview(revision: u64, color: [u8; 4]) -> Arc<SkinPreview> {
    use crate::skin_preview::{
        SkinChannel, SkinCorner, SkinImage, SkinPreviewGeometry, SkinTriangle,
    };

    let geometry = SkinPreviewGeometry::new(
        revision,
        vec![SkinTriangle {
            source_triangle_id: 0,
            channel: SkinChannel::Face,
            corners: [
                SkinCorner {
                    vertex_id: 0,
                    uv: [0.0, 0.0],
                },
                SkinCorner {
                    vertex_id: 1,
                    uv: [1.0, 0.0],
                },
                SkinCorner {
                    vertex_id: 2,
                    uv: [0.0, 1.0],
                },
            ],
        }],
    )
    .unwrap();
    let image = Arc::new(SkinImage::solid(revision, color));
    Arc::new(SkinPreview {
        revision,
        geometry: Arc::new(geometry),
        face: Arc::clone(&image),
        torso: Arc::clone(&image),
        sclera: Arc::clone(&image),
        iris: Arc::clone(&image),
        lacrimal: Arc::clone(&image),
        inner_mouth: Arc::clone(&image),
        teeth: Arc::clone(&image),
        gums: Arc::clone(&image),
        tongue: Arc::clone(&image),
        eyelashes: image,
        face_surface: crate::skin_preview::SkinSurfaceMap::neutral(revision.wrapping_add(1)),
        torso_surface: crate::skin_preview::SkinSurfaceMap::neutral(revision.wrapping_add(2)),
        mouth_surface_atlas: crate::skin_preview::SkinSurfaceMap::neutral_mouth_atlas(
            revision.wrapping_add(3),
        ),
        sclera_surface: crate::skin_preview::SkinSurfaceMap::neutral(revision.wrapping_add(4)),
        iris_surface: crate::skin_preview::SkinSurfaceMap::neutral(revision.wrapping_add(5)),
        lacrimal_surface: crate::skin_preview::SkinSurfaceMap::neutral(revision.wrapping_add(6)),
        auxiliary_colors: [[255; 4]; 8],
        auxiliary_textured: [true; 8],
    })
}

fn install_skin_presets(state: &mut AppState, ids: &[&str]) {
    state.vam_skin_presets = ids.iter().map(|id| skin_preset(id)).collect();
    state.vam_uv_mapping = Some(skin_uv_mapping());
}

#[test]
fn the_skin_worn_last_is_worn_again() {
    let mut state = AppState::default();
    install_skin_presets(&mut state, &["alice", "bob"]);
    state.last_skin_id = Some("bob".to_owned());

    state.apply_default_skin();
    assert_eq!(state.selected_skin_id.as_deref(), Some("bob"));
}

#[test]
fn a_starred_skin_outranks_where_the_user_left_off() {
    let mut state = AppState::default();
    install_skin_presets(&mut state, &["alice", "bob"]);
    state.default_skin_id = Some("alice".to_owned());
    state.last_skin_id = Some("bob".to_owned());

    state.apply_default_skin();
    assert_eq!(state.selected_skin_id.as_deref(), Some("alice"));
}

#[test]
fn restoring_a_skin_needs_the_uv_mapping_and_says_so_by_failing_without_it() {
    let mut without = AppState {
        vam_skin_presets: ["alice", "bob"].iter().map(|id| skin_preset(id)).collect(),
        last_skin_id: Some("bob".to_owned()),
        ..AppState::default()
    };
    without.apply_default_skin();
    assert_eq!(
        without.selected_skin_id, None,
        "without the mapping the restore cannot land -- which is why its          position in the catalog handler is the whole bug"
    );

    let mut with = AppState::default();
    install_skin_presets(&mut with, &["alice", "bob"]);
    with.last_skin_id = Some("bob".to_owned());
    with.apply_default_skin();
    assert_eq!(with.selected_skin_id.as_deref(), Some("bob"));
}

#[test]
fn a_session_with_no_star_and_no_history_stays_on_flat_colour() {
    let mut state = AppState::default();
    install_skin_presets(&mut state, &["alice", "bob"]);

    state.apply_default_skin();

    assert_eq!(state.selected_skin_id, None);
}

#[test]
fn a_vanished_last_skin_is_quiet_but_a_vanished_star_is_not() {
    let mut state = AppState::default();
    install_skin_presets(&mut state, &["alice"]);
    state.last_skin_id = Some("gone".to_owned());
    state.apply_default_skin();
    assert_eq!(state.selected_skin_id, None);
    assert_ne!(state.status.key, TextKey::DefaultSkinMissing);

    let mut starred = AppState::default();
    install_skin_presets(&mut starred, &["alice"]);
    starred.default_skin_id = Some("gone".to_owned());
    starred.apply_default_skin();
    assert_eq!(starred.status.key, TextKey::DefaultSkinMissing);
}

#[test]
fn result_and_morph_tabs_require_current_ready_data() {
    let mut state = AppState::default();
    state.dispatch(Action::RequestTab(Tab::Result));
    assert_eq!(state.active_tab, Tab::Alignment);
    assert_eq!(state.status.key, TextKey::ResultUnavailable);

    let state = ready_state();
    assert!(state.tab_available(Tab::Morph));
    assert!(state.tab_available(Tab::Result));
    assert_eq!(state.active_tab, Tab::Morph);
    assert!(!state.export_ready());

    let mut state = state;
    state.dispatch(Action::ApplySculpt);
    assert_eq!(state.active_tab, Tab::Morph);
    assert!(!state.export_ready());
    state.dispatch(Action::BakeMorph);
    assert!(state.tab_available(Tab::Result));
    assert_eq!(state.active_tab, Tab::Result);
    assert!(state.export_ready());
}

#[test]
fn advancing_to_save_returns_the_eyes_to_their_neutral_gaze() {
    let mut state = ready_state();
    state.dispatch(Action::ApplySculpt);
    state.dispatch(Action::BakeMorph);
    state.dispatch(Action::RequestTab(Tab::Morph));
    state.dispatch(Action::ToggleSculptEyeTracking(true));
    state.dispatch(Action::SetManualEyeGaze([0.4, -0.3]));
    assert!(state.sculpt_eye_tracking || state.manual_eye_gaze != [0.0; 2]);

    state.dispatch(Action::RequestTab(Tab::Result));

    assert_eq!(state.active_tab, Tab::Result);
    assert!(!state.sculpt_eye_tracking);
    assert_eq!(state.manual_eye_gaze, [0.0; 2]);
}

#[test]
fn advancing_to_save_restores_the_texture_view_switches() {
    let mut state = ready_state();
    state.dispatch(Action::ApplySculpt);
    state.dispatch(Action::BakeMorph);
    state.dispatch(Action::RequestTab(Tab::Morph));
    state.texture_project.baked_preview_enabled = false;
    state.dispatch(Action::SetTextureHideVaMSkin(true));
    assert!(state.texture_project.hide_vam_skin_preview);

    state.dispatch(Action::RequestTab(Tab::Result));

    assert_eq!(state.active_tab, Tab::Result);
    assert!(state.texture_project.baked_preview_enabled);
    assert!(!state.texture_project.hide_vam_skin_preview);
}

#[test]
fn edit_tab_without_the_g2_base_reports_pending_and_stays_put() {
    let mut state = AppState::default();
    state.dispatch(Action::RequestTab(Tab::Edit));

    assert_ne!(state.active_tab, Tab::Edit);
    assert_eq!(state.status.key, TextKey::AlignmentPending);
    assert_eq!(state.status.tone, StatusTone::Warning);
}

#[test]
fn morph_category_action_clears_search_only_when_the_category_changes() {
    let mut state = AppState::default();
    state.dispatch(Action::SetMorphQuery("jaw".to_owned()));
    state.dispatch(Action::SetMorphCategoryFilter(MorphCategoryFilter::All));
    assert_eq!(state.morph_library.query, "jaw");

    state.dispatch(Action::SetMorphCategoryFilter(
        MorphCategoryFilter::Category(MorphCategory::Mouth),
    ));
    assert!(state.morph_library.query.is_empty());
    assert_eq!(
        state.morph_library.category_filter,
        MorphCategoryFilter::Category(MorphCategory::Mouth)
    );
}

#[test]
fn global_view_and_brush_preferences_have_safe_defaults_and_actions() {
    let mut state = AppState::default();
    assert_eq!(state.base_view_mode, BaseViewMode::Solid);
    assert_eq!(state.surface_smooth_passes, VAM_SMOOTH_PASSES_DEFAULT);
    assert_eq!(
        state.viewport_background_mode,
        ViewportBackgroundMode::Radial
    );
    assert_eq!(
        state.custom_head_solid_color_rgb,
        DEFAULT_CUSTOM_HEAD_SOLID_COLOR_RGB
    );
    assert_eq!(state.g2_solid_color_rgb, DEFAULT_G2_SOLID_COLOR_RGB);
    assert_eq!(state.wireframe_color_rgb, DEFAULT_WIREFRAME_COLOR_RGB);
    assert!(!state.wireframe_visible);
    assert!((state.wireframe_opacity - 0.32).abs() < f32::EPSILON);
    assert!(!state.xray_visible);
    assert!((state.xray_opacity - 0.28).abs() < f32::EPSILON);
    assert_eq!(state.viewport_tool_panel, None);
    assert!(state.x_mirror);
    assert!(state.sculpt_x_symmetry);
    assert!(state.sculpt.x_symmetry());
    assert!(!state.sculpt_connected_topology_only);
    assert!(!state.sculpt.connected_topology_only());

    assert!(!state.sculpt.backface_masking());
    assert_eq!(state.sculpt.falloff_preset(), SculptFalloff::Smooth);

    state.dispatch(Action::SetBaseViewMode(BaseViewMode::Texture));
    state.dispatch(Action::SetSurfaceSmoothPasses(u8::MAX));
    state.dispatch(Action::SetViewportBackgroundMode(
        ViewportBackgroundMode::Vertical,
    ));
    state.dispatch(Action::SetCustomHeadSolidColor([1, 2, 3]));
    state.dispatch(Action::SetG2SolidColor([4, 5, 6]));
    state.dispatch(Action::SetWireframeColor([7, 8, 9]));
    state.dispatch(Action::ToggleWireframe(true));
    state.dispatch(Action::SetWireframeOpacity(-1.0));
    state.dispatch(Action::ToggleXray(true));
    state.dispatch(Action::SetXrayOpacity(4.0));
    state.dispatch(Action::ToggleViewportToolPanel(ViewportToolPanel::Lighting));
    state.dispatch(Action::SetSculptXSymmetry(true));
    state.dispatch(Action::SetSculptConnectedTopologyOnly(true));
    assert_eq!(state.base_view_mode, BaseViewMode::Texture);
    assert_eq!(state.surface_smooth_passes, VAM_SMOOTH_PASSES_MAX);
    assert_eq!(
        state.viewport_background_mode,
        ViewportBackgroundMode::Vertical
    );
    assert_eq!(state.custom_head_solid_color_rgb, [1, 2, 3]);
    assert_eq!(state.g2_solid_color_rgb, [4, 5, 6]);
    assert_eq!(state.wireframe_color_rgb, [7, 8, 9]);
    assert!(state.wireframe_visible);
    assert_eq!(state.wireframe_opacity, 0.0);
    assert!(state.xray_visible);
    assert_eq!(state.xray_opacity, 1.0);
    assert_eq!(state.viewport_tool_panel, Some(ViewportToolPanel::Lighting));
    assert!(state.sculpt_x_symmetry);
    assert!(state.sculpt_connected_topology_only);
    assert!(state.sculpt.connected_topology_only());
    state.dispatch(Action::ToggleViewportToolPanel(ViewportToolPanel::Lighting));
    assert_eq!(state.viewport_tool_panel, None);

    state.dispatch(Action::ToggleSculptBackfaceMasking(true));
    state.dispatch(Action::SetSculptFalloff(SculptFalloff::Sharp));
    assert!(state.sculpt.backface_masking());
    assert_eq!(state.sculpt.falloff_preset(), SculptFalloff::Sharp);
}

#[test]
fn pin_x_symmetry_defaults_on_for_each_launch_state() {
    let mut state = AppState::default();
    state.dispatch(Action::ToggleXMirror(false));
    assert!(!state.x_mirror);
    assert!(AppState::default().x_mirror);
}

#[test]
fn manual_eye_gaze_clamps_resets_and_auto_cursor_is_explicit() {
    let mut state = AppState::default();
    assert_eq!(state.manual_eye_gaze, [0.0; 2]);
    assert_eq!(state.eye_gaze_mode, EyeGazeMode::Manual);

    state.dispatch(Action::SetEyeGazeMode(EyeGazeMode::AutoCursor));
    assert_eq!(state.eye_gaze_mode, EyeGazeMode::AutoCursor);
    state.dispatch(Action::SetManualEyeGaze([3.0, -2.0]));
    assert_eq!(state.manual_eye_gaze, [1.0, -1.0]);
    assert_eq!(state.eye_gaze_mode, EyeGazeMode::Manual);
    state.dispatch(Action::SetManualEyeGaze([f32::NAN, 0.25]));
    assert_eq!(state.manual_eye_gaze, [0.0, 0.25]);

    state.dispatch(Action::SetEyeGazeMode(EyeGazeMode::AutoCursor));
    state.dispatch(Action::ResetEyeGaze);
    assert_eq!(state.manual_eye_gaze, [0.0; 2]);
    assert_eq!(state.eye_gaze_mode, EyeGazeMode::Manual);
}

#[test]
fn import_progress_is_runtime_only_state() {
    let mut state = AppState::default();
    let active_path = PathBuf::from("head.obj");
    state.dispatch(Action::LoadScan(active_path.clone()));
    assert_eq!(
        state.import_progress,
        Some(ImportProgress::new(ImportPhase::MeshLoading, 0.0))
    );
    state.dispatch(Action::ReportImportProgress {
        path: PathBuf::from("stale.obj"),
        progress: ImportProgress::new(ImportPhase::Simplification, 0.8),
    });
    assert_eq!(
        state.import_progress,
        Some(ImportProgress::new(ImportPhase::MeshLoading, 0.0))
    );
    state.dispatch(Action::ReportImportProgress {
        path: active_path.clone(),
        progress: ImportProgress::new(ImportPhase::MeshLoading, 0.9),
    });
    state.dispatch(Action::ReportImportProgress {
        path: active_path.clone(),
        progress: ImportProgress::new(ImportPhase::Simplification, 0.4),
    });
    assert_eq!(
        state.import_progress,
        Some(ImportProgress::new(ImportPhase::Simplification, 0.9))
    );
    state.dispatch(Action::FinishScanImport {
        path: PathBuf::from("stale.obj"),
        outcome: Err("late worker".to_owned()),
    });
    assert!(state.busy());
    state.dispatch(Action::FinishScanImport {
        path: active_path,
        outcome: Err("expected test failure".to_owned()),
    });
    assert!(!state.busy());
    assert_eq!(state.import_progress, None);
    assert_eq!(state.scan_import_completion, None);
}

#[test]
fn successful_scan_import_exposes_a_nonpersistent_completion_receipt() {
    let path = std::env::temp_dir().join(format!(
        "vkit-import-completion-{}-{}.obj",
        std::process::id(),
        VAM_PAIR_TEST_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();

    let mut state = AppState::default();
    state.dispatch(Action::LoadScan(path.clone()));
    state.dispatch(Action::FinishScanImport {
        path: path.clone(),
        outcome: PreparedScan::load_with_progress(&path, |_| {}).map_err(|error| error.to_string()),
    });
    let first = state
        .scan_import_completion
        .expect("successful import receipt");
    assert_eq!(first.generation, 1);
    assert!(first.completed_at <= Instant::now());

    state.dispatch(Action::LoadScan(path.clone()));
    state.dispatch(Action::FinishScanImport {
        path: path.clone(),
        outcome: PreparedScan::load_with_progress(&path, |_| {}).map_err(|error| error.to_string()),
    });
    assert_eq!(
        state
            .scan_import_completion
            .map(|receipt| receipt.generation),
        Some(2)
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_heavy_import_says_why_it_is_slow() {
    use crate::state::types::HEAVY_MESH_TRIANGLES;

    let heavy = ImportProgress::with_size(
        ImportPhase::Simplification,
        0.4,
        Some(HEAVY_MESH_TRIANGLES * 3),
    );
    let note = heavy
        .size_note(Locale::Korean)
        .expect("a heavy mesh explains itself");
    assert!(note.contains("300"), "the count is in the note: {note}");

    assert!(
        ImportProgress::with_size(ImportPhase::Simplification, 0.4, Some(12_000))
            .size_note(Locale::Korean)
            .is_none(),
        "a small mesh needs no explanation"
    );
    assert!(
        ImportProgress::new(ImportPhase::MeshLoading, 0.1)
            .size_note(Locale::Korean)
            .is_none(),
        "an uncounted mesh says nothing rather than guessing"
    );
}

#[test]
fn importer_phases_map_to_korean_ready_state_phases() {
    assert_eq!(
        ImportProgress::from_mesh_import(MeshImportProgress {
            phase: MeshImportPhase::MeshLoading,
            progress: 0.35,
            source_triangles: None,
        }),
        ImportProgress::new(ImportPhase::MeshLoading, 0.35)
    );
    assert_eq!(
        ImportProgress::from_mesh_import(MeshImportProgress {
            phase: MeshImportPhase::Simplification,
            progress: 0.85,
            source_triangles: None,
        }),
        ImportProgress::new(ImportPhase::Simplification, 0.85)
    );
    assert_eq!(
        ImportPhase::MeshLoading.label(Locale::Korean),
        "메시 불러오는 중"
    );
}

#[test]
fn sculpt_connected_topology_preference_rehydrates_after_session_resets() {
    let mut state = AppState::default();
    state.dispatch(Action::SetSculptConnectedTopologyOnly(true));
    assert!(state.sculpt_connected_topology_only);
    assert!(state.sculpt.connected_topology_only());

    state.clear_generated_buffers();
    assert!(state.sculpt_connected_topology_only);
    assert!(state.sculpt.connected_topology_only());

    state.workspace.result_output = Some(Arc::new(tiny_output()));
    state.begin_sculpt_stage().unwrap();
    assert!(state.sculpt_connected_topology_only);
    assert!(state.sculpt.connected_topology_only());
}

#[test]
fn camera_actions_follow_the_active_view_and_linked_edit_contract() {
    let mut state = AppState::default();
    state.workspace.scan_camera.yaw = 0.9;
    state.workspace.template_camera.yaw = -0.7;
    state.dispatch(Action::ResetCamera);
    assert_eq!(state.workspace.scan_camera, state.workspace.template_camera);
    assert_eq!(
        state.workspace.scan_camera.yaw,
        crate::camera::DEFAULT_VIEW_YAW_RADIANS
    );

    state.dispatch(Action::ToggleProjection);
    assert_eq!(
        state.workspace.scan_camera.projection_mode,
        ProjectionMode::Orthographic
    );
    assert_eq!(
        state.workspace.template_camera.projection_mode,
        ProjectionMode::Orthographic
    );
    state.dispatch(Action::SetFov(999.0));
    assert!((state.workspace.scan_camera.fov_y_degrees() - 120.0).abs() < 1.0e-3);
    assert!((state.workspace.template_camera.fov_y_degrees() - 120.0).abs() < 1.0e-3);

    let mut detail = ready_state();
    let edit_projection = detail.workspace.scan_camera.projection_mode;
    detail.dispatch(Action::ToggleProjection);
    assert_eq!(
        detail.workspace.result_camera.projection_mode,
        ProjectionMode::Orthographic
    );
    assert_eq!(
        detail.workspace.scan_camera.projection_mode, edit_projection,
        "Detail camera actions must not mutate the edit cameras"
    );
}

#[test]
fn camera_reset_restores_default_fov_and_head_framing_from_any_fov_history() {
    let reset_from_fov = |initial_fov: f32| {
        let mut state = ready_state();
        state.active_tab = Tab::Result;
        state.dispatch(Action::SetFov(initial_fov));
        state.dispatch(Action::ResetCamera);
        (state.workspace.result_camera, state.result_head_bounds())
    };

    let (from_high, bounds) = reset_from_fov(120.0);
    let (from_low, _) = reset_from_fov(10.0);
    assert_eq!(
        from_high, from_low,
        "reset must produce an identical camera regardless of FOV/dolly history"
    );
    assert!(
        (from_high.fov_y_degrees() - crate::camera::DEFAULT_FOV_Y_DEGREES).abs() < 1.0e-4,
        "reset must restore the default FOV"
    );

    let mut framed = crate::camera::TurntableCamera::default();
    framed.frame(bounds.expect("ready state has a generated result head"));
    assert_eq!(from_high.distance, framed.distance);
    assert_eq!(from_high.target, framed.target);
}

#[test]
fn reaching_for_the_projection_stencil_keeps_the_head() {
    use crate::texture_project::TextureSourceMode;

    let mut state = ready_state();
    state.dispatch(Action::RequestTab(Tab::Texture));
    assert_eq!(state.active_tab, Tab::Texture, "the texture stage opened");
    assert!(state.workspace.result.is_some(), "a head to project onto");

    state.texture_project.add_image_layer(
        std::path::PathBuf::from("photo.png"),
        TextureSourceMode::LandmarkPins,
    );
    assert!(
        state.workspace.result.is_some(),
        "still a head after adding a layer"
    );

    state.dispatch(Action::SetTextureProjectionStencil(true));
    assert_eq!(state.active_tab, Tab::Texture, "still on the texture stage");
    assert!(
        state.workspace.result.is_some(),
        "the head vanished when the stencil came up"
    );
    assert!(
        state.texture_project.projection_stencil,
        "the stencil is up"
    );
}

#[test]
fn the_stencil_never_leaves_a_layer_holding_a_tool_it_refuses() {
    use crate::texture_project::{TextureSourceMode, TextureTool};

    let mut state = ready_state();
    state.dispatch(Action::RequestTab(Tab::Texture));
    state.texture_project.add_image_layer(
        std::path::PathBuf::from("atlas.png"),
        TextureSourceMode::MaterialUv,
    );

    state.dispatch(Action::SetTextureProjectionStencil(true));
    let tool = state.texture_project.active_tool;
    assert!(
        state
            .texture_project
            .selected_layer()
            .is_some_and(|layer| layer.source_mode.allows(tool)),
        "an atlas layer was left holding {tool:?}"
    );
    assert!(
        !state.texture_project.projection_stencil,
        "the stencil rose over a layer that cannot take the paint"
    );

    state.texture_project.add_image_layer(
        std::path::PathBuf::from("photo.png"),
        TextureSourceMode::LandmarkPins,
    );
    state.dispatch(Action::SetTextureProjectionStencil(true));
    assert_eq!(state.texture_project.active_tool, TextureTool::Projection);
    assert!(state.texture_project.projection_stencil);
}

#[test]
fn the_stencil_goes_down_when_its_owners_move_on() {
    use crate::texture_project::{TextureSourceMode, TextureTool};

    let mut state = ready_state();
    state.dispatch(Action::RequestTab(Tab::Texture));
    state.texture_project.add_image_layer(
        std::path::PathBuf::from("photo.png"),
        TextureSourceMode::LandmarkPins,
    );
    state.dispatch(Action::SetTextureProjectionStencil(true));
    assert!(state.texture_project.projection_stencil);

    state.dispatch(Action::SetTextureTool(TextureTool::MaskBrush));
    assert!(
        !state.texture_project.projection_stencil,
        "the mask brush left the stencil owning the pointer"
    );

    state.dispatch(Action::SetTextureProjectionStencil(true));
    assert!(state.texture_project.projection_stencil);
    state.dispatch(Action::RequestTab(Tab::Morph));
    assert_eq!(state.active_tab, Tab::Morph, "the morph stage is reachable");
    assert!(
        !state.texture_project.projection_stencil,
        "the stencil followed the user onto the sculpting stage"
    );
}

#[test]
fn undo_on_the_texture_stage_stays_on_the_texture_stage() {
    let mut state = ready_state();
    state.dispatch(Action::RequestTab(Tab::Texture));
    state.dispatch(Action::Undo);
    assert_eq!(
        state.active_tab,
        Tab::Texture,
        "undo fell through to pin undo and teleported"
    );
}

#[test]
fn a_source_pin_lands_only_while_texturing() {
    use crate::texture_project::{TextureSourceMode, TextureTool};

    let mut state = AppState::default();
    state.texture_project.add_image_layer(
        std::path::PathBuf::from("photo.png"),
        TextureSourceMode::LandmarkPins,
    );
    state.texture_project.set_active_tool(TextureTool::PinPair);

    state.active_tab = Tab::Morph;
    state.add_texture_source_pin([0.25, 0.25]);
    assert_eq!(
        state
            .texture_project
            .selected_layer()
            .map(|layer| layer.pins.len()),
        Some(0),
        "a pin landed while shaping"
    );

    state.active_tab = Tab::Texture;
    state.add_texture_source_pin([0.25, 0.25]);
    state.add_texture_source_pin([0.75, 0.5]);
    assert_eq!(
        state
            .texture_project
            .selected_layer()
            .map(|layer| layer.pins.len()),
        Some(2),
        "pins did not land while texturing"
    );

    state.move_texture_source_pin(0, [0.3, 0.3]);
    let moved = state
        .texture_project
        .selected_layer()
        .and_then(|layer| layer.pins.first().and_then(|pair| pair.source))
        .expect("a pin");
    assert!(
        (moved[0] - 0.3).abs() < 1.0e-4,
        "the pin did not move: {moved:?}"
    );
}

#[test]
fn a_painted_layer_passes_the_bake_gate_without_pins() {
    use crate::texture_project::{TextureLayerPaint, TextureSourceMode};
    use std::sync::Arc;

    let mut state = ready_state();
    state.vam_uv_mapping = Some(skin_uv_mapping());
    let id = state
        .texture_project
        .add_image_layer(PathBuf::from("logo.png"), TextureSourceMode::LandmarkPins);
    let layer = state
        .texture_project
        .layers
        .iter_mut()
        .find(|layer| layer.id == id)
        .expect("the layer");
    layer.image = Some(Arc::new(
        crate::skin_preview::SkinImage::new(1, 2, 2, vec![255; 16]).expect("an image"),
    ));
    assert!(
        !state.texture_auto_bake_ready(),
        "no pins and no paint: nothing to bake yet"
    );

    let layer = state
        .texture_project
        .layers
        .iter_mut()
        .find(|layer| layer.id == id)
        .expect("the layer");
    layer.painted = Some(TextureLayerPaint {
        width: 4,
        height: 4,
        rgba8: Arc::new(vec![0; 64]),
    });
    assert!(
        state.texture_auto_bake_ready(),
        "painting is the projection; the gate must accept it"
    );
}

#[test]
fn shaping_and_texturing_are_told_apart() {
    for (tab, sculpting, texturing) in [
        (Tab::Alignment, false, false),
        (Tab::Edit, false, false),
        (Tab::Morph, true, false),
        (Tab::Texture, false, true),
        (Tab::Result, false, false),
    ] {
        let state = AppState {
            active_tab: tab,
            ..AppState::default()
        };
        assert_eq!(state.is_sculpting(), sculpting, "{tab:?} sculpting");
        assert_eq!(state.is_texturing(), texturing, "{tab:?} texturing");
        assert_eq!(
            state.is_detail_editing(),
            sculpting || texturing,
            "{tab:?} detail workspace"
        );
    }
}

#[test]
fn hiding_the_vam_skin_gives_back_what_it_took() {
    let mut state = AppState {
        base_view_mode: BaseViewMode::Texture,
        ..AppState::default()
    };
    state.texture_project.bake_base = crate::texture_project::TextureBakeBase::CurrentSkin;

    state.dispatch(Action::SetTextureHideVaMSkin(true));
    assert!(state.texture_project.hide_vam_skin_preview);
    assert_eq!(state.base_view_mode, BaseViewMode::Solid);
    assert_eq!(
        state.texture_project.bake_base,
        crate::texture_project::TextureBakeBase::Transparent
    );

    state.dispatch(Action::SetTextureHideVaMSkin(true));

    state.dispatch(Action::SetTextureHideVaMSkin(false));
    assert!(!state.texture_project.hide_vam_skin_preview);
    assert_eq!(state.base_view_mode, BaseViewMode::Texture);
    assert_eq!(
        state.texture_project.bake_base,
        crate::texture_project::TextureBakeBase::CurrentSkin
    );
}

#[test]
fn unhiding_the_vam_skin_makes_it_available_again() {
    let mut state = AppState {
        skin_preview: Some(skin_preview(1, [200, 180, 170, 255])),
        ..AppState::default()
    };
    assert!(state.skin_base_available(), "a skin is worn to begin with");

    state.dispatch(Action::SetTextureHideVaMSkin(true));
    assert!(!state.skin_base_available(), "hidden");

    state.dispatch(Action::SetTextureHideVaMSkin(false));
    assert!(
        state.skin_base_available(),
        "the skin never came back after unhiding"
    );
}

#[test]
fn a_package_save_names_the_required_fields_it_found_empty() {
    let mut state = AppState::default();
    assert!(state.var_metadata.creator.is_empty(), "blank by default");
    assert!(state.var_metadata.package.is_empty(), "blank by default");

    state.dispatch(Action::SavePackage);
    assert_eq!(
        state.missing_package_fields,
        MissingPackageFields {
            creator: true,
            package: true
        },
        "both, not just the first one found"
    );
    assert!(state.package_fields_want_attention);

    state.dispatch(Action::SetVarMetadata(
        VarMetadataField::Creator,
        "Me".to_owned(),
    ));
    assert_eq!(
        state.missing_package_fields,
        MissingPackageFields {
            creator: false,
            package: true
        }
    );
}

#[test]
fn the_package_version_is_the_users_number() {
    let mut state = AppState::default();
    assert_eq!(state.package_metadata().version, 1);

    state.dispatch(Action::SetVarMetadata(
        VarMetadataField::Version,
        "3a".to_owned(),
    ));
    assert_eq!(state.var_version_text, "3");
    assert_eq!(state.package_metadata().version, 3);

    state.dispatch(Action::SetVarMetadata(
        VarMetadataField::Version,
        String::new(),
    ));
    assert!(state.var_version_text.is_empty());
    assert_eq!(state.package_metadata().version, 1);
}

#[test]
fn modified_only_filter_is_changed_only_through_the_library_api() {
    let mut state = AppState::default();
    assert!(!state.morph_library.modified_only());
    state.dispatch(Action::SetMorphListFilter(MorphListFilter::Active));
    assert!(state.morph_library.modified_only());
    state.dispatch(Action::SetMorphListFilter(MorphListFilter::All));
    assert!(!state.morph_library.modified_only());
}

#[test]
fn template_open_routes_vmi_and_vmb_to_the_safe_pair_importer() {
    for extension in ["vmi", "vmb"] {
        let original = PathBuf::from("accepted-template.dsf");
        let mut state = AppState {
            template_path: Some(original.clone()),
            ..Default::default()
        };
        let selected = PathBuf::from(format!(
            "C:/missing/Custom/Atom/Person/Morphs/female/sample.{extension}"
        ));
        state.dispatch(Action::LoadTemplate(selected));
        state.drive_pending_workspace_load();
        assert_eq!(state.status.key, TextKey::VaMMorphPairRejected);
        assert_eq!(state.template_path, Some(original));
    }
}

#[test]
fn sculpt_actions_update_preview_undo_reset_and_gate_export() {
    let mut state = ready_state();
    let baseline = state
        .workspace
        .result_output
        .as_ref()
        .unwrap()
        .vertices
        .clone();
    assert_eq!(state.active_tab, Tab::Morph);
    assert!(!state.export_ready());

    state.dispatch(Action::BeginSculptStroke {
        view_direction_local: None,
        brush_direction_local: None,
    });
    state.dispatch(Action::SculptDab(SculptDab {
        center_local: [0.0, 0.0, 0.0],
        radius_local: 4.0,
        strength: 1.0,
        operation: crate::sculpt::SculptOperation::Grab {
            translation_local: [0.0, 0.0, 0.25],
        },
    }));
    state.dispatch(Action::EndSculptStroke);
    assert_ne!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        baseline
    );
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().faces,
        tiny_output().faces
    );

    state.dispatch(Action::Undo);
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        baseline
    );
    state.dispatch(Action::ApplySculpt);
    assert_eq!(state.active_tab, Tab::Morph);
    assert!(!state.export_ready());
    state.dispatch(Action::BakeMorph);
    assert_eq!(state.active_tab, Tab::Result);
    assert!(state.export_ready());
    assert_eq!(state.status.key, TextKey::MorphApplied);
}

#[test]
fn resetting_the_settings_is_not_undone_by_the_next_autosave() {
    let mut state = AppState::default();
    assert!(state.tooltips_enabled);
    state.dispatch(Action::SetTooltipsEnabled(false));
    assert!(!state.tooltips_enabled);
    assert!(!state.settings_reset);

    state.dispatch(Action::ResetAllSettings);
    assert!(state.settings_reset);

    assert!(!state.tooltips_enabled);
    state.dispatch(Action::SetTooltipsEnabled(true));
    assert!(state.settings_reset);
}

#[test]
fn a_restore_stroke_is_one_undo_step() {
    for operation in [
        crate::sculpt::SculptOperation::Restore,
        crate::sculpt::SculptOperation::RestoreFit,
    ] {
        let mut state = ready_state();

        state.dispatch(Action::BeginSculptStroke {
            view_direction_local: None,
            brush_direction_local: None,
        });
        state.dispatch(Action::SculptDab(SculptDab {
            center_local: [0.0, 0.0, 0.0],
            radius_local: 4.0,
            strength: 1.0,
            operation: crate::sculpt::SculptOperation::Grab {
                translation_local: [0.0, 0.0, 0.25],
            },
        }));
        state.dispatch(Action::EndSculptStroke);
        let sculpted = state
            .workspace
            .result_output
            .as_ref()
            .unwrap()
            .vertices
            .clone();
        let history = state.sculpt.history_len();

        state.dispatch(Action::BeginSculptStroke {
            view_direction_local: None,
            brush_direction_local: None,
        });
        for _ in 0..3 {
            state.dispatch(Action::SculptDabDeferred(SculptDab {
                center_local: [0.0, 0.0, 0.0],
                radius_local: 4.0,
                strength: 0.5,
                operation,
            }));
            state.dispatch(Action::SyncSculptPreview);
        }
        state.dispatch(Action::EndSculptStroke);
        assert_ne!(
            state.workspace.result_output.as_ref().unwrap().vertices,
            sculpted,
            "{operation:?} changed nothing to undo"
        );
        assert_eq!(
            state.sculpt.history_len(),
            history + 1,
            "{operation:?} did not land in the history as exactly one step"
        );

        state.dispatch(Action::Undo);
        assert_eq!(
            state.workspace.result_output.as_ref().unwrap().vertices,
            sculpted,
            "Ctrl+Z did not take back the {operation:?} stroke"
        );
    }
}

#[test]
fn sculpt_undo_cancels_an_in_flight_stroke_and_restores_the_preview() {
    let mut state = ready_state();
    let baseline = state
        .workspace
        .result_output
        .as_ref()
        .unwrap()
        .vertices
        .clone();
    state.dispatch(Action::BeginSculptStroke {
        view_direction_local: None,
        brush_direction_local: None,
    });
    state.dispatch(Action::SculptDab(SculptDab {
        center_local: [0.0, 0.0, 0.0],
        radius_local: 4.0,
        strength: 1.0,
        operation: crate::sculpt::SculptOperation::Grab {
            translation_local: [0.0, 0.0, 0.25],
        },
    }));
    assert_ne!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        baseline
    );
    assert_eq!(state.sculpt.history_len(), 0);

    state.dispatch(Action::Undo);

    assert_eq!(state.active_tab, Tab::Morph);
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        baseline
    );
    assert_eq!(state.sculpt.history_len(), 0);
    assert!(!state.sculpt.has_changes());
}

#[test]
fn restore_brush_basis_installs_with_the_stage_and_dabs_converge_the_preview() {
    let mut state = ready_state();

    assert!(state.sculpt.has_restore_basis());
    let basis = state
        .workspace
        .template_geometry
        .as_ref()
        .unwrap()
        .to_ordered_obj(None)
        .unwrap()
        .vertices;

    state.dispatch(Action::BeginSculptStroke {
        view_direction_local: None,
        brush_direction_local: None,
    });
    state.dispatch(Action::SculptDab(SculptDab {
        center_local: [0.0, 0.0, 0.0],
        radius_local: 4.0,
        strength: 1.0,
        operation: crate::sculpt::SculptOperation::Grab {
            translation_local: [0.0, 0.0, 0.25],
        },
    }));
    state.dispatch(Action::EndSculptStroke);
    assert_ne!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        basis
    );

    state.dispatch(Action::BeginSculptStroke {
        view_direction_local: None,
        brush_direction_local: None,
    });
    for _ in 0..40 {
        state.dispatch(Action::SculptDabDeferred(SculptDab {
            center_local: [0.0, 0.0, 0.25],
            radius_local: 6.0,
            strength: 1.0,
            operation: crate::sculpt::SculptOperation::Restore,
        }));
    }
    state.dispatch(Action::SyncSculptPreview);
    state.dispatch(Action::EndSculptStroke);

    let timing = state.sculpt_dab_timing.expect("dab timing receipt");
    assert!(timing.serial >= 1);
    assert!(timing.dab_ms >= 0.0);
    let restored = &state.workspace.result_output.as_ref().unwrap().vertices;
    for (after, target) in restored.iter().zip(&basis) {
        for axis in 0..3 {
            assert!(
                (after[axis] - target[axis]).abs() < 1.0e-3,
                "restore must converge the preview to the template basis"
            );
        }
    }
}

#[test]
fn sculpt_reset_then_global_undo_restores_the_pre_reset_mesh() {
    let mut state = ready_state();

    state.dispatch(Action::SetSculptXSymmetry(false));
    let baseline = state
        .workspace
        .result_output
        .as_ref()
        .unwrap()
        .vertices
        .clone();
    state.dispatch(Action::BeginSculptStroke {
        view_direction_local: None,
        brush_direction_local: None,
    });
    state.dispatch(Action::SculptDab(SculptDab {
        center_local: [0.0, 0.0, 0.0],
        radius_local: 4.0,
        strength: 1.0,
        operation: crate::sculpt::SculptOperation::Grab {
            translation_local: [0.2, 0.0, 0.0],
        },
    }));
    state.dispatch(Action::EndSculptStroke);
    let sculpted = state
        .workspace
        .result_output
        .as_ref()
        .unwrap()
        .vertices
        .clone();
    assert_ne!(sculpted, baseline);
    assert_eq!(state.sculpt.history_len(), 1);

    state.dispatch(Action::RequestTab(Tab::Morph));
    assert_eq!(state.active_tab, Tab::Morph);
    assert_eq!(state.sculpt.history_len(), 1);
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        sculpted
    );

    state.dispatch(Action::RequestTab(Tab::Morph));
    state.dispatch(Action::ResetSculpt);
    assert!(state.sculpt.is_ready());
    assert_eq!(state.sculpt.history_len(), 2);
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        baseline
    );

    state.dispatch(Action::Undo);
    assert_eq!(state.active_tab, Tab::Morph);
    assert_eq!(state.sculpt.history_len(), 1);
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        sculpted
    );
}

#[test]
fn morph_reset_then_undo_restores_control_values_and_eye_closure_bit_exactly() {
    let mut state = ready_state();
    state.morph_library.replace_source(
        MorphSource::Curated,
        vec![MorphControl::new(
            "test.reset.undo",
            "Test reset undo",
            MorphCategory::Eyes,
            MorphSource::Curated,
            Arc::new(tiny_morph()),
        )],
    );
    state.dispatch(Action::SetFaceMorph {
        id: "test.reset.undo".into(),
        value: 0.375,
    });
    state.dispatch(Action::SetEyeClosure(0.625));
    let before_values = state
        .morph_library
        .controls()
        .iter()
        .map(|control| (control.id.clone(), control.value.to_bits()))
        .collect::<BTreeMap<_, _>>();
    let before_eye = state.eye_closure.to_bits();
    let before_preview_serial = state.morph_preview_timing.unwrap().serial;

    state.dispatch(Action::ResetMorphs);
    assert!(state.morph_reset_undo.is_some());
    assert_eq!(
        state.morph_preview_timing.unwrap().serial,
        before_preview_serial + 1
    );
    state.dispatch(Action::Undo);

    let restored_values = state
        .morph_library
        .controls()
        .iter()
        .map(|control| (control.id.clone(), control.value.to_bits()))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(restored_values, before_values);
    assert_eq!(state.eye_closure.to_bits(), before_eye);
    assert!(state.morph_reset_undo.is_none());
    assert_eq!(
        state.morph_preview_timing.unwrap().serial,
        before_preview_serial + 2
    );
}

#[test]
fn morph_edit_after_reset_invalidates_the_pending_reset_undo() {
    let mut state = ready_state();
    state.morph_library.replace_source(
        MorphSource::Curated,
        vec![MorphControl::new(
            "test.reset.invalidate",
            "Test reset invalidate",
            MorphCategory::Eyes,
            MorphSource::Curated,
            Arc::new(tiny_morph()),
        )],
    );
    state.dispatch(Action::SetFaceMorph {
        id: "test.reset.invalidate".into(),
        value: 0.25,
    });
    state.dispatch(Action::ResetMorphs);
    assert!(state.morph_reset_undo.is_some());

    state.dispatch(Action::SetFaceMorph {
        id: "test.reset.invalidate".into(),
        value: 0.75,
    });
    assert!(state.morph_reset_undo.is_none());

    state.dispatch(Action::Undo);
    assert_eq!(
        state
            .morph_library
            .controls()
            .iter()
            .find(|control| control.id == "test.reset.invalidate")
            .unwrap()
            .value,
        0.0
    );
}

#[test]
fn morph_adjustments_step_back_one_at_a_time_under_ctrl_z() {
    let mut state = ready_state();
    state.morph_library.replace_source(
        MorphSource::Curated,
        vec![
            MorphControl::new(
                "m.a",
                "A",
                MorphCategory::Eyes,
                MorphSource::Curated,
                Arc::new(tiny_morph()),
            ),
            MorphControl::new(
                "m.b",
                "B",
                MorphCategory::Eyes,
                MorphSource::Curated,
                Arc::new(tiny_morph()),
            ),
        ],
    );
    let value_of = |state: &AppState, id: &str| {
        state
            .morph_library
            .controls()
            .iter()
            .find(|control| control.id == id)
            .unwrap()
            .value
    };
    state.dispatch(Action::SetFaceMorph {
        id: "m.a".into(),
        value: 0.5,
    });

    state.dispatch(Action::SetFaceMorph {
        id: "m.b".into(),
        value: 0.8,
    });
    assert_eq!(value_of(&state, "m.a"), 0.5);
    assert_eq!(value_of(&state, "m.b"), 0.8);

    state.dispatch(Action::Undo);
    assert_eq!(
        value_of(&state, "m.b"),
        0.0,
        "Ctrl+Z reverts the most recent morph adjustment"
    );
    assert_eq!(value_of(&state, "m.a"), 0.5);

    state.dispatch(Action::Undo);
    assert_eq!(
        value_of(&state, "m.a"),
        0.0,
        "Ctrl+Z keeps stepping back through earlier morph adjustments"
    );
}

#[test]
fn detail_undo_reverts_sculpt_before_the_earlier_morph_edit() {
    let mut state = ready_state();
    state.morph_library.replace_source(
        MorphSource::Curated,
        vec![MorphControl::new(
            "m.order",
            "Order",
            MorphCategory::Eyes,
            MorphSource::Curated,
            Arc::new(tiny_morph()),
        )],
    );
    let morph_value = |state: &AppState| {
        state
            .morph_library
            .controls()
            .iter()
            .find(|control| control.id == "m.order")
            .unwrap()
            .value
    };

    state.dispatch(Action::SetFaceMorph {
        id: "m.order".into(),
        value: 0.5,
    });
    state.dispatch(Action::BeginSculptStroke {
        view_direction_local: None,
        brush_direction_local: None,
    });
    state.dispatch(Action::SculptDab(SculptDab {
        center_local: [0.0, 0.0, 0.0],
        radius_local: 4.0,
        strength: 1.0,
        operation: crate::sculpt::SculptOperation::Grab {
            translation_local: [0.0, 0.0, 0.25],
        },
    }));
    state.dispatch(Action::EndSculptStroke);
    assert_eq!(state.sculpt.history_len(), 1);
    assert_eq!(morph_value(&state), 0.5);

    state.dispatch(Action::Undo);
    assert_eq!(
        state.sculpt.history_len(),
        0,
        "the newer sculpt stroke undoes before the older morph edit"
    );
    assert_eq!(
        morph_value(&state),
        0.5,
        "the earlier morph adjustment is preserved by the first undo"
    );

    state.dispatch(Action::Undo);
    assert_eq!(
        morph_value(&state),
        0.0,
        "the older morph edit is reverted by the next undo"
    );
    assert_eq!(state.sculpt.history_len(), 0);
}

#[test]
fn detail_undo_reverts_morph_made_after_a_sculpt_stroke() {
    let mut state = ready_state();
    state.morph_library.replace_source(
        MorphSource::Curated,
        vec![MorphControl::new(
            "m.after",
            "After",
            MorphCategory::Eyes,
            MorphSource::Curated,
            Arc::new(tiny_morph()),
        )],
    );
    let morph_value = |state: &AppState| {
        state
            .morph_library
            .controls()
            .iter()
            .find(|control| control.id == "m.after")
            .unwrap()
            .value
    };

    state.dispatch(Action::BeginSculptStroke {
        view_direction_local: None,
        brush_direction_local: None,
    });
    state.dispatch(Action::SculptDab(SculptDab {
        center_local: [0.0, 0.0, 0.0],
        radius_local: 4.0,
        strength: 1.0,
        operation: crate::sculpt::SculptOperation::Grab {
            translation_local: [0.0, 0.0, 0.25],
        },
    }));
    state.dispatch(Action::EndSculptStroke);
    state.dispatch(Action::SetFaceMorph {
        id: "m.after".into(),
        value: 0.7,
    });
    assert_eq!(state.sculpt.history_len(), 1);
    assert_eq!(morph_value(&state), 0.7);

    state.dispatch(Action::Undo);
    assert_eq!(
        morph_value(&state),
        0.0,
        "the newer morph edit undoes before the older sculpt stroke"
    );
    assert_eq!(
        state.sculpt.history_len(),
        1,
        "the earlier sculpt stroke is preserved by the first undo"
    );

    state.dispatch(Action::Undo);
    assert_eq!(
        state.sculpt.history_len(),
        0,
        "the older sculpt stroke is reverted by the next undo"
    );
}

#[test]
fn no_op_resets_create_no_morph_or_sculpt_history() {
    let mut state = ready_state();
    state.dispatch(Action::ResetMorphs);
    state.dispatch(Action::ResetSculpt);

    assert!(state.morph_reset_undo.is_none());
    assert_eq!(state.sculpt.history_len(), 0);
}

#[test]
fn sculpt_morph_round_trip_is_reversible_idempotent_and_save_exact() {
    let mut state = ready_state();
    state.dispatch(Action::BeginSculptStroke {
        view_direction_local: None,
        brush_direction_local: None,
    });
    state.dispatch(Action::SculptDab(SculptDab {
        center_local: [0.0, 0.0, 0.0],
        radius_local: 4.0,
        strength: 1.0,
        operation: crate::sculpt::SculptOperation::Grab {
            translation_local: [0.0, 0.0, 0.25],
        },
    }));
    state.dispatch(Action::EndSculptStroke);
    let sculpted = state.sculpt.working_mesh().unwrap().vertices.clone();
    assert_eq!(state.sculpt.history_len(), 1);

    state.dispatch(Action::ApplySculpt);
    state.dispatch(Action::SetEyeClosure(0.5));
    let first_morph = state
        .workspace
        .result_output
        .as_ref()
        .unwrap()
        .vertices
        .clone();
    assert_ne!(first_morph, sculpted);

    state.dispatch(Action::RequestTab(Tab::Morph));
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        first_morph,
        "the merged Detail preview must keep active morph values visible"
    );
    assert_eq!(state.sculpt.working_mesh().unwrap().vertices, sculpted);
    assert_eq!(state.sculpt.history_len(), 1);
    assert_eq!(state.eye_closure, 0.5);

    state.dispatch(Action::RequestTab(Tab::Morph));
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        first_morph
    );
    state.dispatch(Action::RequestTab(Tab::Morph));
    state.dispatch(Action::RequestTab(Tab::Morph));
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        first_morph,
        "composition accumulated instead of restarting from S"
    );

    state.dispatch(Action::BakeMorph);
    assert_eq!(state.result_preview_phase, ResultPreviewPhase::Save);
    assert!(state.export_ready());
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        first_morph
    );
}

#[test]
fn sculpt_apply_failure_is_transactional_and_preserves_the_working_session() {
    let mut state = ready_state();
    state.dispatch(Action::BeginSculptStroke {
        view_direction_local: None,
        brush_direction_local: None,
    });
    state.dispatch(Action::SculptDab(SculptDab {
        center_local: [0.0, 0.0, 0.0],
        radius_local: 4.0,
        strength: 1.0,
        operation: crate::sculpt::SculptOperation::Grab {
            translation_local: [0.0, 0.0, 0.25],
        },
    }));
    state.dispatch(Action::EndSculptStroke);
    let sculpted = state.sculpt.working_mesh().unwrap().vertices.clone();
    let history_len = state.sculpt.history_len();

    state.workspace.result = None;
    state.dispatch(Action::ApplySculpt);

    assert_eq!(state.active_tab, Tab::Morph);
    assert_eq!(state.status.key, TextKey::GenerationFailed);
    assert!(!state.sculpt.is_applied());
    assert_eq!(state.sculpt.history_len(), history_len);
    assert_eq!(state.sculpt.working_mesh().unwrap().vertices, sculpted);
}

#[test]
fn sculpt_apply_failure_does_not_finalize_an_in_flight_stroke() {
    let mut state = ready_state();
    state.dispatch(Action::BeginSculptStroke {
        view_direction_local: None,
        brush_direction_local: None,
    });
    state.dispatch(Action::SculptDab(SculptDab {
        center_local: [0.0, 0.0, 0.0],
        radius_local: 4.0,
        strength: 1.0,
        operation: crate::sculpt::SculptOperation::Grab {
            translation_local: [0.0, 0.0, 0.25],
        },
    }));
    assert_eq!(state.sculpt.history_len(), 0);

    state.workspace.result = None;
    state.dispatch(Action::ApplySculpt);

    assert_eq!(state.active_tab, Tab::Morph);
    assert_eq!(state.status.key, TextKey::GenerationFailed);
    assert_eq!(state.sculpt.history_len(), 0);
    state.dispatch(Action::Undo);
    assert!(!state.sculpt.has_changes());
}

#[test]
fn detail_navigation_failure_keeps_the_sculpt_stage_intact() {
    let mut state = ready_state();
    assert!(state.sculpt.is_ready());
    state.workspace.result = None;

    state.dispatch(Action::RequestTab(Tab::Morph));

    assert_eq!(state.active_tab, Tab::Morph);
    assert!(state.sculpt.is_ready());
    assert_eq!(state.status.key, TextKey::GenerationFailed);
}

#[test]
fn loading_lazy_morph_blocks_morph_commit() {
    let mut state = ready_state();
    state.dispatch(Action::RequestTab(Tab::Morph));
    state.vam_catalog_revision = 8;
    state.vam_morph_faces = state
        .workspace
        .template_geometry
        .as_ref()
        .map(|geometry| Arc::new(geometry.faces.clone()));
    let cached = CachedBuiltinMorph {
        internal_name: "PHMLoadingGate".into(),
        label: "Loading Gate".into(),
        category: VaMMorphCategory::Mouth,
        minimum: 0.0,
        maximum: 1.0,
        default: 0.0,
        deltas: Arc::new(vec![(1, [0.0, -0.2, 0.0])]),
        unsupported_formula_count: 0,
    };
    state.install_vam_controls(std::slice::from_ref(&cached));
    state
        .vam_cached_morphs
        .insert(cached.internal_name.clone(), cached);
    state.dispatch(Action::SetFaceMorph {
        id: "PHMLoadingGate".into(),
        value: 0.5,
    });
    assert!(state.morph_library.has_loading_controls());

    state.dispatch(Action::BakeMorph);

    assert_eq!(state.active_tab, Tab::Morph);
    assert!(state.sculpt.is_ready());
    assert_eq!(state.status.key, TextKey::MorphLoading);
}

#[test]
fn late_lazy_morph_completion_recomposes_without_invalidating_sculpt() {
    let mut state = ready_state();
    let preview = Arc::clone(state.workspace.result_output.as_ref().unwrap());
    state.vam_catalog_revision = 9;
    let cached = CachedBuiltinMorph {
        internal_name: "PHMLateResolve".into(),
        label: "Late Resolve".into(),
        category: VaMMorphCategory::Mouth,
        minimum: 0.0,
        maximum: 1.0,
        default: 0.0,
        deltas: Arc::new(vec![(1, [0.0, -0.2, 0.0])]),
        unsupported_formula_count: 0,
    };
    state.install_vam_controls(std::slice::from_ref(&cached));
    assert!(state.morph_library.begin_resolution("PHMLateResolve", 0.5));

    state.dispatch(Action::FinishVaMMorph {
        catalog_revision: 9,
        geometry_revision: state.revision,
        control_id: "PHMLateResolve".into(),
        outcome: Ok(VaMMorphPayload {
            target: Arc::new(tiny_morph()),
            unsupported_formula_count: 0,
        }),
    });

    assert_eq!(state.active_tab, Tab::Morph);
    assert!(state.sculpt.is_ready());
    assert!(!state.morph_preview_dirty);
    assert!(!Arc::ptr_eq(
        &preview,
        state.workspace.result_output.as_ref().unwrap()
    ));
    state.dispatch(Action::ApplySculpt);
    assert_eq!(state.active_tab, Tab::Morph);
    assert!(!state.morph_preview_dirty);
}

#[test]
fn eye_follow_uses_hidden_sparse_morphs_without_touching_export_output() {
    let mut state = ready_state();
    let baseline = state
        .workspace
        .result_output
        .as_ref()
        .unwrap()
        .vertices
        .clone();
    let cached = CachedBuiltinMorph {
        internal_name: "PHMEyelidsTopUpL".into(),
        label: "Eyelids Top Up L".into(),
        category: VaMMorphCategory::Eyes,
        minimum: 0.0,
        maximum: 1.0,
        default: 0.0,
        deltas: Arc::new(vec![(0, [0.0, 0.25, 0.0])]),
        unsupported_formula_count: 0,
    };
    state
        .vam_cached_morphs
        .insert(cached.internal_name.clone(), cached);

    let preview = state
        .sculpt_eye_follow_vertices(20.0, 0.0)
        .expect("left eyelid preview");
    assert_eq!(preview[0][1], baseline[0][1] + 0.25);
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        baseline
    );
}

#[test]
fn geometry_change_stales_result_and_returns_to_edit() {
    let mut state = ready_state();
    state.dispatch(Action::RequestTab(Tab::Morph));
    assert_eq!(state.active_tab, Tab::Morph);
    state.dispatch(Action::SetScale([1.25, 1.0, 1.0]));
    assert_eq!(state.transform.scale_xyz, [1.25, 1.0, 1.0]);
    assert_eq!(state.active_tab, Tab::Alignment);
    assert!(matches!(state.result, ResultState::Stale { .. }));
    assert!(!state.tab_available(Tab::Result));
    assert!(!state.tab_available(Tab::Morph));
    assert!(state.workspace.result.is_none());
    assert!(state.workspace.fitted_result.is_none());
    assert!(state.workspace.result_output.is_none());
}

#[test]
fn scale_axes_are_independent_and_invalid_vectors_are_rejected_atomically() {
    let mut state = AppState::default();
    state.dispatch(Action::SetScale([1.25, 0.75, 1.5]));
    assert_eq!(state.transform.scale_xyz, [1.25, 0.75, 1.5]);

    state.dispatch(Action::SetScale([2.0, 0.0, 3.0]));
    assert_eq!(state.transform.scale_xyz, [1.25, 0.75, 1.5]);

    state.dispatch(Action::SetScale([2.0, f64::NAN, 3.0]));
    assert_eq!(state.transform.scale_xyz, [1.25, 0.75, 1.5]);
}

#[test]
fn progress_is_clamped_monotonic_and_job_scoped() {
    let mut state = AppState {
        scan_path: Some(PathBuf::from("scan.obj")),
        template_path: Some(PathBuf::from("Genesis2Female.dsf")),
        complete_pairs: MIN_FIT_PAIRS,
        ..Default::default()
    };
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.dispatch(Action::BeginGeneration);
    let ResultState::Running { job_id, .. } = state.result else {
        panic!("generation did not start");
    };
    state.dispatch(Action::ReportProgress {
        job_id,
        source_revision: state.revision,
        progress: Progress::new(JobStage::Fit, 0.7),
    });
    state.dispatch(Action::ReportProgress {
        job_id,
        source_revision: state.revision,
        progress: Progress::new(JobStage::Align, 0.2),
    });
    state.dispatch(Action::ReportProgress {
        job_id: job_id + 1,
        source_revision: state.revision,
        progress: Progress::new(JobStage::Export, 1.0),
    });
    assert_eq!(state.progress.map(|value| value.fraction), Some(0.7));
}

#[test]
fn locale_switch_does_not_change_geometry_or_workflow() {
    let mut state = ready_state();
    let revision = state.revision;
    let result = state.result;
    let tab = state.active_tab;
    state.dispatch(Action::SetLocale(Locale::English));
    assert_eq!(state.locale, Locale::English);
    assert_eq!(state.revision, revision);
    assert_eq!(state.result, result);
    assert_eq!(state.active_tab, tab);
}

#[test]
fn dropped_non_obj_does_not_replace_scan() {
    let mut state = AppState::default();
    state.dispatch(Action::LoadScan(PathBuf::from("not-a-mesh.png")));
    assert!(state.scan_path.is_none());
    assert_eq!(state.status.key, TextKey::NeedScan);
}

#[test]
fn default_output_is_blank_until_the_user_chooses_a_destination() {
    let state = AppState::default();
    assert!(state.output_path.is_empty());

    assert_eq!(state.output_topology, OutputTopology::VaM);
    assert!(state.vam_export_bone_correction);
}

#[test]
fn vam_default_destination_uses_group_folder_and_sanitized_morph_filename() {
    let directory = tempfile::tempdir().expect("temporary VaM root");
    std::fs::create_dir_all(directory.path().join("Custom").join("Atom").join("Person"))
        .expect("minimal VaM layout");
    let mut state = AppState {
        vam_root: Some(directory.path().to_path_buf()),
        ..Default::default()
    };

    state.dispatch(Action::SetVaMExportDisplayName("Head/Fit".to_owned()));
    state.dispatch(Action::SetVaMExportGroup("   ".to_owned()));
    assert!(
        state
            .output_path
            .ends_with("Custom\\Atom\\Person\\Morphs\\female\\Vkit\\Head_Fit.vmi"),
        "unexpected derived destination: {}",
        state.output_path
    );

    state.dispatch(Action::SetVaMExportDisplayName(String::new()));
    assert!(
        state
            .output_path
            .ends_with("Morphs\\female\\Vkit\\Vkit_Morph.vmi"),
        "unexpected derived destination: {}",
        state.output_path
    );

    state.dispatch(Action::SetOutputPath("D:\\out\\custom.vmi".to_owned()));
    state.dispatch(Action::SetVaMExportDisplayName("Renamed".to_owned()));
    assert_eq!(state.output_path, "D:\\out\\custom.vmi");
}

#[test]
fn vam_export_bone_correction_is_session_owned_and_user_configurable() {
    let mut state = AppState::default();
    state.dispatch(Action::SetVaMExportBoneCorrection(false));
    assert!(!state.vam_export_bone_correction);

    assert!(!state.vam_export_bone_correction);
}

#[test]
fn g2_counts_alone_do_not_classify_female_or_male() {
    assert_eq!(
        TemplateTopologyKind::from_counts(DAZ_G2F_VERTEX_COUNT, DAZ_G2F_FACE_COUNT),
        TemplateTopologyKind::Unknown
    );
    assert_eq!(
        TemplateTopologyKind::from_counts(VAM_FEMALE_VERTEX_COUNT, VAM_FEMALE_FACE_COUNT),
        TemplateTopologyKind::VaMFemale
    );
    assert_eq!(
        TemplateTopologyKind::from_counts(VAM_MALE_VERTEX_COUNT, VAM_MALE_FACE_COUNT),
        TemplateTopologyKind::VaMMale
    );
    assert_eq!(
        TemplateTopologyKind::from_counts(1, 1),
        TemplateTopologyKind::Unknown
    );

    let state = AppState {
        figure_sex: FigureSex::Male,
        ..Default::default()
    };
    assert_eq!(state.output_topology, OutputTopology::VaM);
    assert_eq!(state.figure_sex, FigureSex::Male);
}

#[test]
fn embedded_assets_bind_by_topology_for_both_sexes() {
    assert!(should_request_g2_embedded_assets(
        Some(FigureSex::Female),
        true
    ));
    assert!(should_request_g2_embedded_assets(
        Some(FigureSex::Male),
        true
    ));

    assert!(!should_request_g2_embedded_assets(None, true));
    assert!(!should_request_g2_embedded_assets(
        Some(FigureSex::Female),
        false
    ));
    assert!(!should_request_g2_embedded_assets(
        Some(FigureSex::Male),
        false
    ));
}

#[test]
fn explicit_provider_candidates_keep_selection_before_environment() {
    let state = AppState {
        vam_geometry_base_path: Some(PathBuf::from(r"F:\chosen\female.obj")),
        ..Default::default()
    };
    let candidates = state
        .provider_discovery_context()
        .explicit_candidates(GeometrySex::Female);
    assert_eq!(
        candidates.first(),
        Some(&PathBuf::from(r"F:\chosen\female.obj"))
    );

    assert!(candidates.len() <= 2);
}

#[test]
fn provenance_maps_every_tier_to_localized_text() {
    for tier in [
        GeometryBaseTier::UnityBundle,
        GeometryBaseTier::DazAnchorPair,
        GeometryBaseTier::UnityAnchorPair,
        GeometryBaseTier::UnverifiedUserObj,
    ] {
        let provenance = VaMGeometryBaseProvenance {
            tier,
            neutrality: None,
        };
        for locale in Locale::ALL {
            assert!(!crate::i18n::text(locale, provenance.text_key()).is_empty());
        }
    }
}

#[test]
#[ignore = "requires separately licensed VKIT_G2_DSF and VKIT_VAM_FEMALE_BASE files"]
fn user_owned_vam_female_template_installs_a_canonical_proxy_transactionally() {
    let anchor_path = env::var_os("VKIT_G2_DSF")
        .map(PathBuf::from)
        .expect("set VKIT_G2_DSF");
    let base_path = env::var_os("VKIT_VAM_FEMALE_BASE")
        .map(PathBuf::from)
        .expect("set VKIT_VAM_FEMALE_BASE");
    let mut state = AppState::default();

    state.load_template(anchor_path);
    state.drive_pending_workspace_load();
    assert_eq!(
        state.detected_template_topology.classification,
        TemplateTopologyKind::DazG2Female
    );
    assert_eq!(state.output_topology, OutputTopology::Daz);

    state.set_vam_geometry_base(base_path.clone());
    assert!(
        state
            .vam_geometry_provider
            .as_deref()
            .is_some_and(|provider| provider.sex() == GeometrySex::Female)
    );

    state.load_template(base_path.clone());
    state.drive_pending_workspace_load();
    assert_eq!(state.template_path.as_deref(), Some(base_path.as_path()));
    assert_eq!(
        state.detected_template_topology,
        DetectedTemplateTopology::from_counts(VAM_FEMALE_VERTEX_COUNT, VAM_FEMALE_FACE_COUNT,)
    );
    assert_eq!(state.output_topology, OutputTopology::VaM);
    assert_eq!(state.figure_sex, FigureSex::Female);
    assert!(
        state
            .vam_geometry_provider
            .as_deref()
            .is_some_and(|provider| provider.sex() == GeometrySex::Female)
    );
    let canonical = state
        .workspace
        .template_geometry
        .as_deref()
        .expect("VaM import installs its canonical DAZ working proxy");
    assert_eq!(canonical.vertices.len(), DAZ_G2F_VERTEX_COUNT);
    assert_eq!(canonical.faces.len(), DAZ_G2F_FACE_COUNT);
    assert!(
        matches_canonical_g2_topology(canonical.vertices.len(), &canonical.faces)
            .expect("canonical topology digest")
    );
}

#[test]
fn failed_ordered_template_import_preserves_existing_template_and_morph_value() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(tiny_template())
        .expect("existing template");
    state.template_path = Some(PathBuf::from("existing.dsf"));
    state.detected_template_topology =
        DetectedTemplateTopology::from_counts(DAZ_G2F_VERTEX_COUNT, DAZ_G2F_FACE_COUNT);
    state.output_topology = OutputTopology::VaM;
    state.figure_sex = FigureSex::Male;
    state.morph_library.replace_source(
        MorphSource::Curated,
        vec![MorphControl::new(
            "existing-control",
            "Existing control",
            MorphCategory::Eyes,
            MorphSource::Curated,
            Arc::new(tiny_morph()),
        )],
    );
    assert!(state.morph_library.set_value("existing-control", 0.75));
    let existing = Arc::clone(
        state
            .workspace
            .template_geometry
            .as_ref()
            .expect("existing geometry"),
    );

    let directory = tempfile::tempdir().expect("temporary import directory");
    let rejected = directory.path().join("not-g2.obj");
    std::fs::write(&rejected, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").expect("write rejected OBJ");
    state.load_template(rejected);
    state.drive_pending_workspace_load();

    assert!(Arc::ptr_eq(
        state
            .workspace
            .template_geometry
            .as_ref()
            .expect("old template retained"),
        &existing
    ));
    assert_eq!(state.template_path, Some(PathBuf::from("existing.dsf")));
    assert_eq!(
        state
            .morph_library
            .controls()
            .iter()
            .find(|control| control.id == "existing-control")
            .map(|control| control.value),
        Some(0.75)
    );
    assert_eq!(
        state.detected_template_topology,
        DetectedTemplateTopology::from_counts(DAZ_G2F_VERTEX_COUNT, DAZ_G2F_FACE_COUNT)
    );
    assert_eq!(state.output_topology, OutputTopology::VaM);
    assert_eq!(state.figure_sex, FigureSex::Male);
    assert_eq!(state.status.key, TextKey::GenerationFailed);
    assert_eq!(state.status.tone, StatusTone::Error);
    assert!(
        state
            .status
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("expected 21556 vertices, got 3"))
    );
}

#[test]
fn linked_axis_scale_and_coalesced_alignment_undo_restore_prior_transform() {
    let mut state = AppState::default();
    state.dispatch(Action::SetScale([1.25, 0.75, 1.5]));
    state.dispatch(Action::BeginAlignmentTransform);
    state.dispatch(Action::SetScaleAxis {
        axis: 0,
        value: 2.5,
    });
    state.dispatch(Action::CommitAlignmentTransform);
    assert_eq!(state.transform.scale_xyz, [2.5, 1.5, 3.0]);
    assert_eq!(state.alignment_undo_len(), 2);

    state.dispatch(Action::Undo);
    assert_eq!(state.transform.scale_xyz, [1.25, 0.75, 1.5]);
    state.dispatch(Action::SetScaleLinked(false));
    state.dispatch(Action::SetScaleAxis {
        axis: 1,
        value: 2.0,
    });
    assert_eq!(state.transform.scale_xyz, [1.25, 2.0, 1.5]);
}

#[test]
fn active_symmetry_refreshes_on_transaction_commit_cancel_and_undo() {
    let source = Arc::new(
        crate::scene::SurfaceMesh::new(
            Mesh::new(
                vec![
                    [-1.0, -1.0, 0.0],
                    [1.0, -1.0, 0.0],
                    [-1.0, 0.0, 0.04],
                    [1.0, 0.0, 0.0],
                    [-1.0, 1.0, 0.03],
                    [1.0, 1.0, 0.02],
                ],
                vec![[0, 1, 2], [1, 3, 2], [2, 3, 4], [3, 5, 4]],
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let mut state = AppState::default();
    state.workspace.scan_source = Some(Arc::clone(&source));
    state.workspace.scan = Some(source);
    state.scan_symmetry = ScanSymmetry::PositiveX;
    state.refresh_active_scan_symmetry();
    let identity_vertices = state.workspace.scan.as_ref().unwrap().mesh.vertices.clone();

    assert!(state.begin_alignment_transform());
    state.dispatch(Action::SetRotation {
        axis: 2,
        value_degrees: 90.0,
    });
    assert_vertices_close(
        &state.workspace.scan.as_ref().unwrap().mesh.vertices,
        &identity_vertices,
    );
    assert!(state.commit_alignment_transform());
    let quarter_turn_vertices = state.workspace.scan.as_ref().unwrap().mesh.vertices.clone();
    assert_ne!(quarter_turn_vertices, identity_vertices);

    assert!(state.begin_alignment_transform());
    state.dispatch(Action::SetRotation {
        axis: 2,
        value_degrees: 180.0,
    });
    assert!(state.cancel_alignment_transform());
    assert_eq!(state.transform.rotation_degrees[2], 90.0);
    assert_vertices_close(
        &state.workspace.scan.as_ref().unwrap().mesh.vertices,
        &quarter_turn_vertices,
    );

    state.dispatch(Action::SetRotation {
        axis: 2,
        value_degrees: 0.0,
    });
    assert_vertices_close(
        &state.workspace.scan.as_ref().unwrap().mesh.vertices,
        &identity_vertices,
    );
    assert!(state.undo_alignment_transform());
    assert_eq!(state.transform.rotation_degrees[2], 90.0);
    assert_vertices_close(
        &state.workspace.scan.as_ref().unwrap().mesh.vertices,
        &quarter_turn_vertices,
    );
}

#[test]
fn existing_output_requires_confirmation_before_export_is_queued() {
    let mut state = ready_state();
    state.dispatch(Action::ApplySculpt);
    state.dispatch(Action::BakeMorph);

    let existing = std::env::temp_dir().join(format!(
        "vkit-existing-output-{}-{}.vmi",
        std::process::id(),
        VAM_PAIR_TEST_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&existing, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    assert!(existing.is_file());
    state.dispatch(Action::SetOutputPath(
        existing.to_string_lossy().into_owned(),
    ));
    state.dispatch(Action::ExportResult);
    assert_eq!(state.take_export_request(), None);
    assert_eq!(
        state.overwrite_confirmation_path(),
        Some(existing.as_path())
    );

    state.dispatch(Action::ConfirmOverwrite);
    assert_eq!(state.take_export_request(), Some(state.revision));
    let _ = std::fs::remove_file(&existing);
}

#[test]
fn zero_pairs_unlock_the_automatic_generation_path() {
    let mut state = AppState {
        scan_path: Some(PathBuf::from("scan.obj")),
        template_path: Some(PathBuf::from("Genesis2Female.dsf")),
        output_path: String::new(),
        ..Default::default()
    };
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.dispatch(Action::BeginGeneration);
    assert!(state.busy());
}

#[test]
fn incomplete_only_pins_do_not_block_the_automatic_generation_path() {
    let mut state = AppState {
        scan_path: Some(PathBuf::from("scan.obj")),
        template_path: Some(PathBuf::from("Genesis2Female.dsf")),
        ..Default::default()
    };
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.workspace.pins.add(
        MeshSide::Scan,
        SurfaceEndpoint {
            triangle: 0,
            barycentric: [1.0, 0.0, 0.0],
        },
    );
    state.dispatch(Action::BeginGeneration);
    assert!(state.busy());
}

#[test]
fn suggested_alignment_matches_face_scale_and_center_without_hiding_the_transform() {
    let mut state = AppState::default();
    state.workspace.scan = Some(Arc::new(
        crate::scene::SurfaceMesh::new(
            Mesh::new(
                vec![[10.0, 20.0, 0.0], [12.0, 20.0, 0.0], [10.0, 22.0, 0.0]],
                vec![[0, 1, 2]],
            )
            .unwrap(),
        )
        .unwrap(),
    ));
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    let transform = state.suggested_alignment().unwrap();
    assert!(
        transform
            .scale_xyz
            .into_iter()
            .all(|scale| (scale - 0.5).abs() < 1.0e-12)
    );
    assert!((transform.translation_cm[0] + 10.5).abs() < 1.0e-6);
    assert!((transform.translation_cm[1] + 20.5).abs() < 1.0e-6);
    assert!(transform.translation_cm[2].abs() < 1.0e-6);
    assert_eq!(transform.rotation_degrees, [0.0; 3]);
}

#[test]
fn suggested_alignment_uses_face_width_and_ignores_lower_neck_crop() {
    fn box_vertices(minimum: [f64; 3], maximum: [f64; 3]) -> Vec<[f64; 3]> {
        vec![
            [minimum[0], minimum[1], minimum[2]],
            [maximum[0], minimum[1], minimum[2]],
            [minimum[0], maximum[1], minimum[2]],
            [maximum[0], maximum[1], minimum[2]],
            [minimum[0], minimum[1], maximum[2]],
            [maximum[0], minimum[1], maximum[2]],
            [minimum[0], maximum[1], maximum[2]],
            [maximum[0], maximum[1], maximum[2]],
        ]
    }

    let triangles = vec![
        [0, 1, 2],
        [1, 3, 2],
        [4, 6, 5],
        [5, 6, 7],
        [0, 4, 1],
        [1, 4, 5],
        [2, 3, 6],
        [3, 7, 6],
        [0, 2, 4],
        [2, 6, 4],
        [1, 5, 3],
        [3, 5, 7],
    ];
    let template_faces = triangles
        .iter()
        .map(|triangle| triangle.to_vec())
        .collect::<Vec<Vec<u32>>>();
    let template = DazGeometry::new(
        "alignment-box".into(),
        box_vertices([-5.0, 100.0, -10.0], [5.0, 130.0, 10.0]),
        template_faces,
        vkit_core::formats::GroupTable {
            indices: vec![0; triangles.len()],
            names: vec!["Head".into()],
        },
        vkit_core::formats::GroupTable {
            indices: vec![0; triangles.len()],
            names: vec!["Face".into()],
        },
        json!({}),
    )
    .unwrap();

    let align = |minimum_y: f64| {
        let scan = Mesh::new(
            box_vertices([-1.0, minimum_y, -2.0], [1.0, 2.0, 2.0]),
            triangles.clone(),
        )
        .unwrap();
        let mut state = AppState::default();
        state.workspace.scan = Some(Arc::new(crate::scene::SurfaceMesh::new(scan).unwrap()));
        state
            .workspace
            .load_template_geometry(template.clone())
            .unwrap();
        let transform = state.suggested_alignment().unwrap();
        let pivot = state
            .workspace
            .scan
            .as_ref()
            .unwrap()
            .facial_focus_bounds()
            .center()
            .as_dvec3()
            .to_array();
        let model = crate::scene::ModelTransform::from_components_with_pivot(
            transform.scale_xyz[0],
            transform.translation_cm,
            transform.rotation_degrees,
            pivot,
        );
        (
            transform,
            model.point_to_world(glam::DVec3::new(1.0, 2.0, 2.0)),
        )
    };

    let (full_neck, full_crown) = align(-2.0);
    let (short_neck, short_crown) = align(-1.0);
    assert_eq!(full_neck.scale_xyz, [5.0; 3]);
    assert_eq!(short_neck.scale_xyz, [5.0; 3]);
    assert!(full_crown.abs_diff_eq(glam::DVec3::new(5.0, 130.0, 10.0), 1.0e-12));
    assert!(short_crown.abs_diff_eq(full_crown, 1.0e-12));
}

#[test]
fn placement_reset_restores_the_canonical_zero_rotation_baseline() {
    let mut state = AppState::default();
    state.workspace.scan = Some(Arc::new(
        crate::scene::SurfaceMesh::new(
            Mesh::new(
                vec![[10.0, 20.0, 0.0], [12.0, 20.0, 0.0], [10.0, 22.0, 0.0]],
                vec![[0, 1, 2]],
            )
            .unwrap(),
        )
        .unwrap(),
    ));
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    let suggested = state.suggested_alignment().unwrap();
    state.transform = ScanTransform {
        scale_xyz: [3.0, 4.0, 5.0],
        translation_cm: [7.0, 8.0, 9.0],
        rotation_degrees: [15.0, -20.0, 35.0],
    };
    state.dispatch(Action::PlacementReset);
    assert_eq!(state.transform, suggested);
    assert_eq!(state.transform.rotation_degrees, [0.0; 3]);
}

#[test]
fn auto_match_preserves_rotation_and_nonuniform_scale_ratios() {
    let mut state = AppState::default();
    state.workspace.scan = Some(Arc::new(
        crate::scene::SurfaceMesh::new(
            Mesh::new(
                vec![[10.0, 20.0, 0.0], [12.0, 20.0, 0.0], [10.0, 22.0, 0.0]],
                vec![[0, 1, 2]],
            )
            .unwrap(),
        )
        .unwrap(),
    ));
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    let before = ScanTransform {
        scale_xyz: [3.0, 4.0, 5.0],
        translation_cm: [7.0, 8.0, 9.0],
        rotation_degrees: [15.0, -20.0, 35.0],
    };
    state.transform = before;

    state.dispatch(Action::AutoAlign);

    assert_eq!(state.transform.rotation_degrees, before.rotation_degrees);
    let correction = state.transform.scale_xyz[0] / before.scale_xyz[0];
    for axis in 0..3 {
        assert!(
            (state.transform.scale_xyz[axis] - before.scale_xyz[axis] * correction).abs() < 1.0e-12
        );
    }
    assert_ne!(state.transform.translation_cm, before.translation_cm);
}

#[test]
fn alignment_opacities_are_independent_and_accept_the_full_range() {
    let mut state = AppState::default();
    assert_eq!(state.alignment_opacity, 1.0);
    assert_eq!(state.alignment_g2_opacity, 1.0);
    state.dispatch(Action::SetAlignmentOpacity(-1.0));
    assert_eq!(state.alignment_opacity, 0.0);
    assert_eq!(state.alignment_g2_opacity, 1.0);
    state.dispatch(Action::SetAlignmentOpacity(1.0));
    assert_eq!(state.alignment_opacity, 1.0);
    state.dispatch(Action::SetAlignmentOpacity(2.0));
    assert_eq!(state.alignment_opacity, 1.0);
    state.dispatch(Action::SetAlignmentG2Opacity(-1.0));
    assert_eq!(state.alignment_g2_opacity, 0.0);
    assert_eq!(state.alignment_opacity, 1.0);
    state.dispatch(Action::SetAlignmentG2Opacity(2.0));
    assert_eq!(state.alignment_g2_opacity, 1.0);
}

#[test]
fn automatic_matching_is_always_on() {
    let state = AppState::default();
    assert!(state.resolved_automatic_matching());
}

#[test]
fn result_part_toggles_are_display_only() {
    let mut state = ready_state();
    let revision = state.revision;
    let result_before = Arc::clone(state.workspace.result_output.as_ref().unwrap());
    state.dispatch(Action::ToggleResultTearLacrimals(false));
    state.dispatch(Action::ToggleResultEyelashes(false));
    assert!(!state.show_result_tear_lacrimals);
    assert!(!state.show_result_eyelashes);
    assert_eq!(state.revision, revision);
    assert!(Arc::ptr_eq(
        &result_before,
        state.workspace.result_output.as_ref().unwrap()
    ));
}

#[test]
fn dialog_actions_create_one_shot_intents() {
    let mut state = AppState::default();
    state.dispatch(Action::RequestOpenScan);
    assert_eq!(state.take_dialog_intent(), Some(DialogIntent::OpenScan));
    assert_eq!(state.take_dialog_intent(), None);
}

#[test]
fn export_requires_a_committed_morph_and_is_a_one_shot_request() {
    let mut state = ready_state();
    state.dispatch(Action::ExportResult);
    assert_eq!(state.take_export_request(), None);
    assert_eq!(state.status.key, TextKey::ResultUnavailable);

    state.dispatch(Action::ApplySculpt);
    state.dispatch(Action::BakeMorph);
    assert!(state.export_ready());
    state.output_path = "final.vmi".to_owned();
    state.dispatch(Action::ExportResult);
    assert!(state.busy());
    assert_eq!(state.progress, Some(Progress::new(JobStage::Export, 0.0)));
    let source_revision = state.take_export_request().unwrap();
    assert_eq!(state.take_export_request(), None);
    state.dispatch(Action::ReportExportProgress {
        source_revision,
        fraction: 0.4,
    });
    assert_eq!(state.progress.map(|value| value.fraction), Some(0.4));
    state.dispatch(Action::ReportExportProgress {
        source_revision: source_revision + 1,
        fraction: 0.9,
    });
    assert_eq!(state.progress.map(|value| value.fraction), Some(0.4));
    state.dispatch(Action::FinishExport {
        source_revision,
        outcome: ExportOutcome::Success {
            receipt: ExportReceipt {
                committed_paths: vec![PathBuf::from("final.vmi")],
            },
        },
    });
    assert!(!state.busy());
    assert!(state.progress.is_none());
    assert_eq!(state.output_path, "final.vmi");
    assert_eq!(state.status.key, TextKey::ExportComplete);
}

#[test]
fn exporting_blocks_direct_pin_mutations_without_staling_the_committed_result() {
    let mut state = ready_state();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    state.dispatch(Action::ApplySculpt);
    state.dispatch(Action::BakeMorph);
    state.output_path = "final.vmi".to_owned();
    state.dispatch(Action::ExportResult);
    let revision = state.revision;
    let result = state.result;
    let eye_closure = state.eye_closure;
    let endpoint = SurfaceEndpoint {
        triangle: 0,
        barycentric: [1.0, 0.0, 0.0],
    };

    assert_eq!(state.add_surface_pin(MeshSide::Scan, endpoint), None);
    assert!(
        state
            .add_surface_pins(MeshSide::Scan, [endpoint])
            .is_empty()
    );
    assert!(!state.delete_surface_pin(MeshSide::Scan, 0));
    assert!(!state.begin_surface_pin_drag(MeshSide::Scan, 0));
    assert!(!state.move_surface_pin(MeshSide::Scan, 0, endpoint));
    state.dispatch(Action::SetEyeClosure(0.5));
    assert_eq!(state.revision, revision);
    assert_eq!(state.result, result);
    assert_eq!(state.eye_closure, eye_closure);
    assert!(state.export_ready());
    assert!(state.workspace.result_output.is_some());
}

#[test]
fn failed_export_retains_the_committed_mesh_and_allows_retry() {
    let mut state = ready_state();
    state.dispatch(Action::ApplySculpt);
    state.dispatch(Action::BakeMorph);
    let committed = Arc::clone(state.workspace.result_output.as_ref().unwrap());
    state.output_path = "final.vmi".to_owned();
    state.dispatch(Action::ExportResult);
    let source_revision = state.take_export_request().unwrap();
    state.dispatch(Action::FinishExport {
        source_revision,
        outcome: ExportOutcome::Failed("disk full".to_owned()),
    });

    assert!(!state.busy());
    assert!(state.progress.is_none());
    assert!(state.export_ready());
    assert!(Arc::ptr_eq(
        &committed,
        state.workspace.result_output.as_ref().unwrap()
    ));
    assert_eq!(state.status.key, TextKey::ExportFailed);

    state.dispatch(Action::ExportResult);
    assert_eq!(state.take_export_request(), Some(source_revision));
}

#[test]
fn changing_a_morph_after_apply_relocks_export() {
    let mut state = ready_state();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    state.dispatch(Action::ApplySculpt);
    state.dispatch(Action::BakeMorph);
    assert!(state.export_ready());
    state.dispatch(Action::SetEyeClosure(0.5));
    assert!(!state.export_ready());
    assert_eq!(state.active_tab, Tab::Morph);
    state.dispatch(Action::BakeMorph);
    assert!(state.export_ready());
}

#[test]
fn completion_rejects_wrong_job_and_stale_revision() {
    let mut state = AppState {
        scan_path: Some(PathBuf::from("scan.obj")),
        template_path: Some(PathBuf::from("Genesis2Female.dsf")),
        complete_pairs: MIN_FIT_PAIRS,
        ..Default::default()
    };
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.dispatch(Action::BeginGeneration);
    let (job_id, source_revision) = state.active_job().unwrap();
    state.dispatch(Action::FinishGeneration {
        job_id: job_id + 1,
        source_revision,
        outcome: GenerationOutcome::Success {
            output: tiny_output(),
            fit_reference_value: 0.0,
        },
        morph_available: false,
    });
    assert!(state.busy());

    state.revision = source_revision + 1;
    state.dispatch(Action::FinishGeneration {
        job_id,
        source_revision,
        outcome: GenerationOutcome::Success {
            output: tiny_output(),
            fit_reference_value: 0.0,
        },
        morph_available: false,
    });
    assert!(matches!(state.result, ResultState::Stale { .. }));
    assert!(state.workspace.result.is_none());
}

#[test]
fn eye_state_change_preserves_unrelated_pins_and_requires_only_affected_review() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(review_template())
        .unwrap();
    state.workspace.eye_morph = Some(Arc::new(review_morph()));
    let endpoint = |triangle| SurfaceEndpoint {
        triangle,
        barycentric: [0.6, 0.2, 0.2],
    };
    state.add_surface_pin(MeshSide::Scan, endpoint(0));
    state.add_surface_pin(MeshSide::Template, endpoint(0));
    state.add_surface_pin(MeshSide::Scan, endpoint(4));
    state.add_surface_pin(MeshSide::Template, endpoint(4));

    state.dispatch(Action::SetEyeState(EyeState::Closed));
    assert_eq!(state.workspace.pins.pairs().len(), 2);
    assert!(state.workspace.pins.requires_review(0));
    assert!(!state.workspace.pins.requires_review(1));
    assert_eq!(state.status.key, TextKey::ReviewEyePins);

    assert!(state.begin_surface_pin_drag(MeshSide::Template, 0));
    assert!(state.move_surface_pin(MeshSide::Template, 0, endpoint(0)));
    assert_eq!(state.workspace.pins.review_count(), 0);
}

#[test]
fn the_guidance_advances_one_step_at_a_time() {
    use crate::guidance::{NextStep, next_step};

    let mut state = AppState::default();

    assert_eq!(next_step(&state), Some(NextStep::ChooseVaMFolder));

    state.vam_root = Some(PathBuf::from(r"C:\VaM"));
    assert_eq!(next_step(&state), None);

    state
        .workspace
        .load_template_geometry(review_template())
        .unwrap();
    assert_eq!(next_step(&state), Some(NextStep::LoadScan));

    state.scan_path = Some(PathBuf::from("head.obj"));
    assert_eq!(next_step(&state), Some(NextStep::PlaceHead));

    state.dispatch(Action::RequestTab(Tab::Alignment));
    assert_eq!(next_step(&state), Some(NextStep::PlacePins));

    state.complete_pairs = 3;
    assert_eq!(next_step(&state), Some(NextStep::Generate));

    state.result = ResultState::Ready {
        source_revision: 0,
        morph_available: true,
    };
    assert_eq!(next_step(&state), None);
}

#[test]
fn the_guidance_goes_quiet_while_the_session_is_busy() {
    use crate::guidance::{NextStep, next_step};

    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(review_template())
        .unwrap();
    state.scan_path = Some(PathBuf::from("head.obj"));
    assert_eq!(next_step(&state), Some(NextStep::PlaceHead));

    state.result = ResultState::Running {
        job_id: 1,
        source_revision: 0,
    };
    assert!(state.busy());
    assert_eq!(next_step(&state), None);
}

#[test]
fn symmetry_applies_freely_until_a_pin_exists_and_then_asks_first() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(review_template())
        .unwrap();
    let endpoint = |triangle| SurfaceEndpoint {
        triangle,
        barycentric: [0.6, 0.2, 0.2],
    };

    state.dispatch(Action::RequestScanSymmetry(ScanSymmetry::PositiveX));
    assert_eq!(state.scan_symmetry, ScanSymmetry::PositiveX);
    assert_eq!(state.pending_symmetry_change, None);

    state.add_surface_pin(MeshSide::Scan, endpoint(0));
    state.add_surface_pin(MeshSide::Template, endpoint(0));
    state.add_surface_pin(MeshSide::Scan, endpoint(4));
    assert_eq!(state.workspace.pins.pairs().len(), 2);

    state.dispatch(Action::RequestScanSymmetry(ScanSymmetry::NegativeX));
    assert_eq!(state.pending_symmetry_change, Some(ScanSymmetry::NegativeX));
    assert_eq!(
        state.scan_symmetry,
        ScanSymmetry::PositiveX,
        "the scan must not have been mirrored while the question is on screen"
    );
    assert_eq!(state.workspace.pins.pairs().len(), 2);

    state.dispatch(Action::CancelScanSymmetry);
    assert_eq!(state.pending_symmetry_change, None);
    assert_eq!(state.scan_symmetry, ScanSymmetry::PositiveX);
    assert_eq!(state.workspace.pins.pairs().len(), 2);

    state.dispatch(Action::RequestScanSymmetry(ScanSymmetry::NegativeX));
    state.dispatch(Action::ConfirmScanSymmetry);
    assert_eq!(state.scan_symmetry, ScanSymmetry::NegativeX);
    assert!(state.workspace.pins.pairs().is_empty());
    assert_eq!(state.pending_symmetry_change, None);
}

#[test]
fn eye_and_symmetry_changes_before_pins_or_results_do_not_show_stale_warning() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(review_template())
        .unwrap();
    state.workspace.eye_morph = Some(Arc::new(review_morph()));

    state.dispatch(Action::SetEyeState(EyeState::Closed));
    assert_eq!(state.eye_state, EyeState::Closed);
    assert!(matches!(state.result, ResultState::Empty));
    assert_ne!(state.status.key, TextKey::ResultStale);

    state.dispatch(Action::RequestScanSymmetry(ScanSymmetry::PositiveX));
    assert_eq!(state.scan_symmetry, ScanSymmetry::PositiveX);
    assert!(matches!(state.result, ResultState::Empty));
    assert_ne!(state.status.key, TextKey::ResultStale);
    assert_ne!(state.status.key, TextKey::PinsReset);
}

#[test]
fn scan_properties_do_not_navigate_away_from_create() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(review_template())
        .unwrap();
    state.workspace.eye_morph = Some(Arc::new(review_morph()));
    state.active_tab = Tab::Edit;

    state.dispatch(Action::RequestScanSymmetry(ScanSymmetry::PositiveX));
    assert_eq!(state.active_tab, Tab::Edit);
    state.dispatch(Action::SetEyeState(EyeState::Closed));
    assert_eq!(state.active_tab, Tab::Edit);
}

#[test]
fn initial_alignment_changes_do_not_claim_an_uncreated_result_is_stale() {
    let mut state = AppState::default();
    let previous_status = state.status.clone();
    let previous_revision = state.revision;

    assert!(state.apply_alignment_transform(ScanTransform {
        translation_cm: [1.0, 0.0, 0.0],
        ..ScanTransform::default()
    }));

    assert_eq!(state.revision, previous_revision + 1);
    assert!(matches!(state.result, ResultState::Empty));
    assert_eq!(state.status, previous_status);
}

#[test]
fn alignment_changes_still_mark_an_existing_result_stale() {
    let mut state = ready_state();

    assert!(state.apply_alignment_transform(ScanTransform {
        translation_cm: [1.0, 0.0, 0.0],
        ..ScanTransform::default()
    }));

    assert!(matches!(state.result, ResultState::Stale { .. }));

    assert_ne!(state.status.key, TextKey::ResultStale);
}

#[test]
#[ignore = "requires the user's VaM installation"]
fn vam_root_installs_the_runtime_template_and_default_morph_destination() {
    let root = PathBuf::from(std::env::var_os("VKIT_VAM_ROOT").expect("set VKIT_VAM_ROOT"));
    let mut state = AppState::default();
    state.dispatch(Action::SetVaMRoot(root));

    state.drive_pending_workspace_load();

    assert_eq!(
        state
            .workspace
            .template_geometry
            .as_deref()
            .map(|geometry| geometry.vertices.len()),
        Some(DAZ_G2_VERTEX_COUNT),
        "VaM root load failed: {:?}",
        state.status
    );
    assert!(
        state
            .vam_geometry_provider
            .as_deref()
            .is_some_and(VaMGeometryProvider::uses_vam_derived_anchor)
    );

    let provenance = state
        .vam_geometry_base_provenance
        .expect("root enrollment records provenance");
    assert!(
        matches!(
            provenance.tier,
            GeometryBaseTier::UnityBundle | GeometryBaseTier::UnityAnchorPair
        ),
        "expected a Tier-1-verified enrollment, got {provenance:?}"
    );
    if let Some(neutrality) = provenance.neutrality {
        assert!(neutrality.rms_cm <= vkit_core::vam::NEUTRAL_BASIS_RMS_LIMIT_CM);
    }
    assert_eq!(
        state
            .workspace
            .template_geometry
            .as_deref()
            .map(|geometry| geometry.faces.len()),
        Some(DAZ_G2_FACE_COUNT),
        "the working template must be the canonical quad stream"
    );
    let template = state.workspace.template.as_deref().unwrap();
    assert!(
        template.visible_triangle_ids.len() < template.mesh.triangles.len(),
        "VaM body faces leaked into the head-only template view"
    );
    assert_eq!(state.eye_state, EyeState::Open);
    assert_eq!(state.eye_closure, 0.0);
    assert!(!state.morph_library.controls().is_empty());

    assert!(
        state
            .output_path
            .ends_with("Morphs\\female\\Vkit\\Vkit_Morph.vmi")
    );

    state.dispatch(Action::SetFigureSex(FigureSex::Male));
    assert_eq!(
        state
            .vam_geometry_provider
            .as_deref()
            .map(VaMGeometryProvider::sex),
        Some(GeometrySex::Male)
    );
    assert!(
        state
            .output_path
            .ends_with("Morphs\\male\\Vkit\\Vkit_Morph.vmi")
    );

    state.dispatch(Action::SetVaMExportDisplayName("Head Fit".to_owned()));
    assert!(
        state
            .output_path
            .ends_with("Morphs\\male\\Vkit\\Head Fit.vmi")
    );
    state.dispatch(Action::SetVaMExportGroup("Look: Head".to_owned()));
    assert!(
        state
            .output_path
            .ends_with("Morphs\\male\\Look_ Head\\Head Fit.vmi")
    );
}

#[test]
#[ignore = "requires the user's VaM installation"]
fn male_bundle_template_supports_the_eye_closed_alignment_preview() {
    let root = PathBuf::from(std::env::var_os("VKIT_VAM_ROOT").expect("set VKIT_VAM_ROOT"));
    let mut state = AppState::default();
    state.dispatch(Action::SetVaMRoot(root));
    state.drive_pending_workspace_load();
    state.dispatch(Action::SetFigureSex(FigureSex::Male));
    state.drive_pending_workspace_load();
    assert_eq!(state.figure_sex, FigureSex::Male);
    assert_eq!(
        state
            .workspace
            .template_geometry
            .as_deref()
            .map(|geometry| geometry.vertices.len()),
        Some(DAZ_G2_VERTEX_COUNT),
        "male VaM enrollment failed: {:?}",
        state.status
    );

    assert!(
        state.workspace.eye_morph.is_some(),
        "male canonical-topology template must bind the shared eye-close asset"
    );
    assert!(
        !state.morph_library.controls().is_empty(),
        "curated controls must bind on the male figure"
    );
    state.dispatch(Action::SetEyeState(EyeState::Closed));
    assert_eq!(
        state.eye_state,
        EyeState::Closed,
        "the align-tab eye-closed preview must work with figure sex = Male: {:?}",
        state.status
    );
}

#[test]
fn eye_state_change_preserves_every_camera_view_parameter() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(review_template())
        .unwrap();
    state.workspace.eye_morph = Some(Arc::new(review_morph()));
    state.workspace.scan_camera.yaw = 0.71;
    state.workspace.scan_camera.pitch = -0.42;
    state.workspace.scan_camera.distance = 6.5;
    state.workspace.scan_camera.target = glam::Vec3::new(1.0, 2.0, 3.0);
    state.workspace.template_camera.yaw = -0.87;
    state.workspace.template_camera.pitch = 0.38;
    state.workspace.template_camera.distance = 9.0;
    state.workspace.template_camera.target = glam::Vec3::new(-4.0, 5.0, -6.0);
    state.workspace.result_camera.yaw = 1.13;
    state.workspace.result_camera.orthographic_scale = 7.0;
    let scan_camera = state.workspace.scan_camera;
    let template_camera = state.workspace.template_camera;
    let result_camera = state.workspace.result_camera;

    state.dispatch(Action::SetEyeState(EyeState::Closed));

    assert_eq!(state.workspace.scan_camera, scan_camera);
    assert_eq!(state.workspace.template_camera, template_camera);
    assert_eq!(state.workspace.result_camera, result_camera);
}

#[test]
fn eye_state_change_still_marks_an_existing_result_stale() {
    let mut state = ready_state();
    state.dispatch(Action::SetEyeState(EyeState::Closed));

    assert!(matches!(state.result, ResultState::Stale { .. }));
    assert_ne!(state.status.key, TextKey::ResultStale);
}

#[test]
fn create_pins_without_a_custom_head_reports_a_localized_error_status() {
    let mut state = AppState::default();
    state.dispatch(Action::BeginGeneration);

    assert_eq!(state.status.key, TextKey::NeedScan);
    assert_eq!(state.status.tone, StatusTone::Error);
}

#[test]
fn morph_preview_bakes_from_open_and_closed_fit_references_without_mutating_fit() {
    let mut open = ready_state();
    open.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    let fitted_open = open.workspace.fitted_result.as_ref().unwrap().clone();
    open.dispatch(Action::SetEyeClosure(0.5));
    assert_eq!(open.eye_closure, 0.5);
    assert_eq!(open.workspace.fitted_result.as_ref().unwrap(), &fitted_open);
    assert_eq!(
        open.workspace.result_output.as_ref().unwrap().vertices[1][1],
        fitted_open.vertices[1][1] - 0.1
    );
    open.dispatch(Action::SetEyeClosure(0.0));
    assert_eq!(
        open.workspace.result_output.as_ref().unwrap().as_ref(),
        fitted_open.as_ref()
    );

    let mut closed = AppState {
        scan_path: Some(PathBuf::from("scan.obj")),
        template_path: Some(PathBuf::from("Genesis2Female.dsf")),
        complete_pairs: MIN_FIT_PAIRS,
        ..Default::default()
    };
    closed
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    closed.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    closed.dispatch(Action::BeginGeneration);
    let (job_id, source_revision) = closed.active_job().unwrap();
    let mut closed_output = tiny_output();
    closed_output.vertices[1][1] -= 0.2;
    closed.dispatch(Action::FinishGeneration {
        job_id,
        source_revision,
        outcome: GenerationOutcome::Success {
            output: closed_output,
            fit_reference_value: 1.0,
        },
        morph_available: true,
    });
    closed.dispatch(Action::SetEyeClosure(0.0));
    assert_eq!(
        closed.workspace.result_output.as_ref().unwrap().vertices[1][1],
        0.0
    );
    assert_eq!(closed.fit_reference_value, Some(1.0));
    let fitted_closed = closed.workspace.fitted_result.as_ref().unwrap().clone();
    closed.dispatch(Action::SetEyeClosure(1.0));
    assert_eq!(
        closed.workspace.result_output.as_ref().unwrap().as_ref(),
        fitted_closed.as_ref()
    );
}

#[test]
fn clearing_skin_selection_discards_only_render_state_and_pending_work() {
    let mut state = ready_state();
    install_skin_presets(&mut state, &["skin-a"]);
    let geometry_revision = state.revision;
    let result = state.result;
    let output = Arc::clone(state.workspace.result_output.as_ref().unwrap());

    state.dispatch(Action::SelectVaMSkin(Some("skin-a".to_owned())));
    let request = state.take_skin_work().unwrap();
    state.dispatch(Action::FinishVaMSkin {
        request_id: request.request_id,
        preset_id: request.preset.stable_id,
        outcome: Ok(skin_preview(10, [10, 20, 30, 255])),
    });
    assert!(state.skin_preview.is_some());
    assert_eq!(state.base_view_mode, BaseViewMode::Texture);

    state.dispatch(Action::SelectVaMSkin(None));

    assert_eq!(state.selected_skin_id, None);
    assert!(!state.skin_preview_loading);
    assert!(state.skin_preview.is_none());
    assert!(state.take_skin_work().is_none());
    assert_eq!(state.base_view_mode, BaseViewMode::Solid);
    assert_eq!(state.revision, geometry_revision);
    assert_eq!(state.result, result);
    assert!(Arc::ptr_eq(
        &output,
        state.workspace.result_output.as_ref().unwrap()
    ));
}

#[test]
fn hair_preset_replacement_is_transactional_and_restores_the_complete_prior_set() {
    let mut state = ready_state();
    state.vam_hair_presets = vec![hair_preset("full-a"), hair_preset("full-b")];
    let geometry_revision = state.revision;
    let output = Arc::clone(state.workspace.result_output.as_ref().unwrap());

    state.dispatch(Action::SelectVaMHair(Some("full-a".to_owned())));
    let first = state.take_hair_work().unwrap();
    state.dispatch(Action::FinishVaMHair {
        request_id: first.request_id,
        preset_id: first.preset.stable_id,
        outcome: Ok(hair_preview("full-a")),
    });
    let complete_prior = Arc::clone(state.hair_preview.as_ref().unwrap());

    state.dispatch(Action::SelectVaMHair(Some("full-b".to_owned())));
    let replacement = state.take_hair_work().unwrap();
    state.dispatch(Action::FinishVaMHair {
        request_id: replacement.request_id,
        preset_id: replacement.preset.stable_id,
        outcome: Err("one part uses unsupported geometry".to_owned()),
    });

    assert_eq!(state.selected_hair_id.as_deref(), Some("full-a"));
    assert!(Arc::ptr_eq(
        state.hair_preview.as_ref().unwrap(),
        &complete_prior
    ));
    assert_eq!(state.status.key, TextKey::HairLoadFailed);
    assert_eq!(state.revision, geometry_revision);
    assert!(Arc::ptr_eq(
        state.workspace.result_output.as_ref().unwrap(),
        &output
    ));
}

#[test]
fn detail_editing_can_start_from_the_bare_g2_base() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    state.set_edit_source_mode(EditSourceMode::CustomMorph);
    assert!(state.selected_vam_edit_source_id.is_none());
    assert!(!state.tab_available(Tab::Morph));

    state.dispatch(Action::BeginDirectEdit);

    assert_eq!(state.active_tab, Tab::Morph);
    assert!(state.tab_available(Tab::Morph));
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        tiny_template().vertices,
        "the base shape is the template itself"
    );
}

#[test]
fn a_hairstyle_missing_a_mesh_part_is_shown_and_says_so() {
    let mut state = ready_state();
    state.vam_hair_presets = vec![hair_preset("mixed")];

    state.dispatch(Action::SelectVaMHair(Some("mixed".to_owned())));
    let request = state.take_hair_work().unwrap();
    state.dispatch(Action::FinishVaMHair {
        request_id: request.request_id,
        preset_id: request.preset.stable_id,
        outcome: Ok(hair_preview_missing_a_part("mixed")),
    });

    assert_eq!(state.selected_hair_id.as_deref(), Some("mixed"));
    assert!(state.hair_preview.is_some());
    assert_eq!(state.status.key, TextKey::HairPartsSkipped);
    assert_eq!(state.status.tone, StatusTone::Warning);
    assert!(
        state
            .status
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("base.vab"))
    );
}

#[test]
fn selecting_valid_skin_queues_exactly_one_preview_request() {
    let mut state = AppState::default();
    install_skin_presets(&mut state, &["skin-a"]);
    let mapping = Arc::clone(state.vam_uv_mapping.as_ref().unwrap());

    state.dispatch(Action::SelectVaMSkin(Some("skin-a".to_owned())));
    state.dispatch(Action::SelectVaMSkin(Some("skin-a".to_owned())));

    let request = state.take_skin_work().unwrap();
    assert_eq!(request.request_id, 1);
    assert_eq!(request.preset.stable_id, "skin-a");
    assert!(Arc::ptr_eq(&request.mapping, &mapping));
    assert!(state.take_skin_work().is_none());
    assert_eq!(state.selected_skin_id.as_deref(), Some("skin-a"));
    assert!(state.skin_preview_loading);
}

#[test]
fn skin_selection_rejects_opposite_and_unclassified_presets_by_default() {
    let mut state = AppState::default();
    let mut male = skin_preset("male-skin");
    male.sex = SkinSex::Male;
    let mut unknown = skin_preset("unknown-skin");
    unknown.sex = SkinSex::Unknown;
    state.vam_skin_presets = vec![male, unknown];
    state.vam_uv_mapping = Some(skin_uv_mapping());

    state.dispatch(Action::SelectVaMSkin(Some("male-skin".to_owned())));
    assert!(state.take_skin_work().is_none());
    assert_eq!(state.selected_skin_id, None);
    assert_eq!(state.status.key, TextKey::SkinLoadFailed);

    state.dispatch(Action::SelectVaMSkin(Some("unknown-skin".to_owned())));
    assert!(state.take_skin_work().is_none());
    assert_eq!(state.selected_skin_id, None);
    assert_eq!(state.status.key, TextKey::SkinLoadFailed);
}

#[test]
fn stale_skin_completion_cannot_replace_newer_selection() {
    let mut state = AppState::default();
    install_skin_presets(&mut state, &["skin-a", "skin-b"]);

    state.dispatch(Action::SelectVaMSkin(Some("skin-a".to_owned())));
    let old_request = state.take_skin_work().unwrap();
    state.dispatch(Action::SelectVaMSkin(Some("skin-b".to_owned())));
    let current_request = state.take_skin_work().unwrap();
    let old_preview = skin_preview(20, [255, 0, 0, 255]);
    let current_preview = skin_preview(21, [0, 255, 0, 255]);

    state.dispatch(Action::FinishVaMSkin {
        request_id: old_request.request_id,
        preset_id: old_request.preset.stable_id,
        outcome: Ok(Arc::clone(&old_preview)),
    });
    assert_eq!(state.selected_skin_id.as_deref(), Some("skin-b"));
    assert!(state.skin_preview_loading);
    assert!(state.skin_preview.is_none());

    state.dispatch(Action::FinishVaMSkin {
        request_id: current_request.request_id,
        preset_id: current_request.preset.stable_id,
        outcome: Ok(Arc::clone(&current_preview)),
    });
    assert!(!state.skin_preview_loading);
    assert!(Arc::ptr_eq(
        state.skin_preview.as_ref().unwrap(),
        &current_preview
    ));
    assert!(!Arc::ptr_eq(
        state.skin_preview.as_ref().unwrap(),
        &old_preview
    ));
}

#[test]
fn failed_skin_preview_leaves_result_and_geometry_revisions_untouched() {
    let mut state = ready_state();
    install_skin_presets(&mut state, &["skin-a"]);
    let geometry_revision = state.revision;
    let result = state.result;
    let fitted = Arc::clone(state.workspace.fitted_result.as_ref().unwrap());
    let output = Arc::clone(state.workspace.result_output.as_ref().unwrap());
    let result_surface = Arc::clone(state.workspace.result.as_ref().unwrap());

    state.dispatch(Action::SelectVaMSkin(Some("skin-a".to_owned())));
    let request = state.take_skin_work().unwrap();
    state.dispatch(Action::FinishVaMSkin {
        request_id: request.request_id,
        preset_id: request.preset.stable_id,
        outcome: Err("broken texture".to_owned()),
    });

    assert_eq!(state.status.key, TextKey::SkinLoadFailed);
    assert_eq!(state.revision, geometry_revision);
    assert_eq!(state.result, result);
    assert!(Arc::ptr_eq(
        &fitted,
        state.workspace.fitted_result.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        &output,
        state.workspace.result_output.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        &result_surface,
        state.workspace.result.as_ref().unwrap()
    ));
}

#[test]
fn successful_skin_preview_never_changes_committed_export_geometry() {
    let mut state = ready_state();
    state.dispatch(Action::ApplySculpt);
    state.dispatch(Action::BakeMorph);
    install_skin_presets(&mut state, &["skin-a"]);
    let geometry_revision = state.revision;
    let result = state.result;
    let fitted = Arc::clone(state.workspace.fitted_result.as_ref().unwrap());
    let export_output = Arc::clone(state.workspace.result_output.as_ref().unwrap());
    assert!(state.export_ready());

    state.dispatch(Action::SelectVaMSkin(Some("skin-a".to_owned())));
    let request = state.take_skin_work().unwrap();
    let preview = skin_preview(30, [50, 60, 70, 255]);
    state.dispatch(Action::FinishVaMSkin {
        request_id: request.request_id,
        preset_id: request.preset.stable_id,
        outcome: Ok(Arc::clone(&preview)),
    });

    assert!(Arc::ptr_eq(state.skin_preview.as_ref().unwrap(), &preview));
    assert_eq!(state.revision, geometry_revision);
    assert_eq!(state.result, result);
    assert!(state.export_ready());
    assert!(Arc::ptr_eq(
        &fitted,
        state.workspace.fitted_result.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        &export_output,
        state.workspace.result_output.as_ref().unwrap()
    ));
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().as_ref(),
        export_output.as_ref()
    );
}

#[test]
fn vam_catalog_refresh_does_not_invalidate_geometry_revision() {
    let mut state = AppState {
        revision: 41,
        ..AppState::default()
    };
    state.dispatch(Action::SetVaMRoot(PathBuf::from(r"C:\VaM")));

    assert_eq!(state.revision, 41);
    assert_eq!(state.vam_root, Some(PathBuf::from(r"C:\VaM")));
    assert_eq!(state.vam_catalog_status, VaMCatalogStatus::Unconfigured);
    assert!(state.take_vam_appearance_work().is_none());

    state.dispatch(Action::RefreshVaMCatalog);
    assert_eq!(state.vam_catalog_status, VaMCatalogStatus::Indexing);
    assert!(matches!(
        state.take_vam_appearance_work(),
        Some(VaMWorkRequest::ScanAppearance {
            catalog_revision: 2,
            requested_root,
            ..
        }) if requested_root == Path::new(r"C:\VaM")
    ));
}

#[test]
fn sex_switch_without_a_vam_root_has_no_morph_source_to_ask() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();

    state.dispatch(Action::SetFigureSex(FigureSex::Male));

    assert_eq!(state.vam_catalog_status, VaMCatalogStatus::Unconfigured);
    assert!(state.take_vam_appearance_work().is_none());
    assert!(state.take_vam_morph_work().is_none());
    assert_eq!(
        state.vam_morph_cache_status,
        VaMMorphCacheStatus::Unavailable
    );
}

#[test]
fn skin_catalog_refresh_never_queues_a_morph_bank() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.vam_root = Some(PathBuf::from(r"C:\VaM"));

    state.dispatch(Action::RefreshVaMCatalog);

    assert!(state.take_vam_appearance_work().is_some());
    assert!(state.take_vam_morph_work().is_none());
}

#[test]
fn appearance_and_installed_morph_cache_are_queued_on_independent_lanes() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.dispatch(Action::SetFigureSex(FigureSex::Male));
    state.dispatch(Action::SetVaMRoot(PathBuf::from(r"C:\VaM")));

    state.dispatch(Action::RefreshVaMCatalog);

    assert!(matches!(
        state.take_vam_appearance_work(),
        Some(VaMWorkRequest::ScanAppearance {
            catalog_revision: 2,
            ..
        })
    ));
    assert!(matches!(
        state.take_vam_morph_work(),
        Some(VaMWorkRequest::LoadMorphCache {
            catalog_revision: 2,
            figure_sex: SkinSex::Male,
            ..
        })
    ));

    state.dispatch(Action::FinishVaMCatalog {
        catalog_revision: 2,
        outcome: Ok(VaMCatalogPayload {
            canonical_root: PathBuf::from(r"C:\VaM"),
            skin_presets: vec![skin_preset("skin-ready-first")],
            hair_presets: Vec::new(),
            shared_scalp: None,
            builtin_hair_scalps: std::sync::Arc::new(Vec::new()),
            edit_sources: Vec::new(),
            morph_index: std::sync::Arc::new(crate::vam_morph_index::VaMMorphIndex::default()),
            package_index: std::sync::Arc::new(vkit_core::vam::PackageIndex::default()),
            morph_groups: Vec::new(),
            morph_regions: Vec::new(),
            uv_mapping: None,
            appearance_warnings: Vec::new(),
        }),
    });

    assert_eq!(state.vam_skin_presets.len(), 1);
    assert_eq!(
        state.vam_catalog_status,
        VaMCatalogStatus::Ready {
            morph_count: 0,
            skin_count: 1,
        }
    );
    assert!(state.take_vam_morph_work().is_none());
}

#[test]
fn a_morph_cache_failure_is_reported_and_leaves_the_appearance_catalog_ready() {
    let mut state = AppState::default();
    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    state.dispatch(Action::SetFigureSex(FigureSex::Male));
    state.dispatch(Action::SetVaMRoot(PathBuf::from(r"C:\VaM")));
    state.dispatch(Action::RefreshVaMCatalog);
    let _ = state.take_vam_appearance_work();
    state.dispatch(Action::FinishVaMCatalog {
        catalog_revision: 2,
        outcome: Ok(VaMCatalogPayload {
            canonical_root: PathBuf::from(r"C:\VaM"),
            skin_presets: vec![skin_preset("skin-survives-cache")],
            hair_presets: Vec::new(),
            shared_scalp: None,
            builtin_hair_scalps: std::sync::Arc::new(Vec::new()),
            edit_sources: Vec::new(),
            morph_index: std::sync::Arc::new(crate::vam_morph_index::VaMMorphIndex::default()),
            package_index: std::sync::Arc::new(vkit_core::vam::PackageIndex::default()),
            morph_groups: Vec::new(),
            morph_regions: Vec::new(),
            uv_mapping: None,
            appearance_warnings: Vec::new(),
        }),
    });
    let _ = state.take_vam_morph_work();

    state.dispatch(Action::FinishVaMMorphCatalog {
        catalog_revision: 2,
        outcome: Err("the selected folder has no VaM morph bank".to_owned()),
    });

    assert_eq!(state.vam_skin_presets.len(), 1);
    assert_eq!(state.status.tone, StatusTone::Error);
    assert!(
        state
            .status
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("morph bank")),
        "{:?}",
        state.status
    );
    assert_eq!(
        state.vam_morph_cache_status,
        VaMMorphCacheStatus::Missing {
            detail: "the selected folder has no VaM morph bank".to_owned(),
        }
    );
    assert_eq!(
        state.vam_catalog_status,
        VaMCatalogStatus::Ready {
            morph_count: 0,
            skin_count: 1,
        }
    );
}

#[test]
fn cached_builtin_stays_sparse_until_first_slider_gesture() {
    let mut state = ready_state();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    state.vam_catalog_revision = 8;
    state.vam_morph_faces = state
        .workspace
        .template_geometry
        .as_ref()
        .map(|geometry| Arc::new(geometry.faces.clone()));
    let cached = CachedBuiltinMorph {
        internal_name: "PHMCacheOnly".into(),
        label: "Cache Only".into(),
        category: VaMMorphCategory::Mouth,
        minimum: 0.0,
        maximum: 1.0,
        default: 0.0,
        deltas: Arc::new(vec![(1, [0.0, -0.2, 0.0])]),
        unsupported_formula_count: 2,
    };
    state.install_vam_controls(std::slice::from_ref(&cached));
    state
        .vam_cached_morphs
        .insert(cached.internal_name.clone(), cached);

    state.dispatch(Action::RequestTab(Tab::Morph));
    state.dispatch(Action::SetFaceMorph {
        id: "PHMCacheOnly".into(),
        value: 0.5,
    });
    let request = state.take_vam_work().expect("lazy cache resolve request");
    let VaMWorkRequest::ResolveCachedMorph {
        catalog_revision,
        geometry_revision,
        control_id,
        descriptor,
        vertex_count,
        canonical_faces,
    } = request
    else {
        panic!("expected cached morph resolution");
    };
    assert_eq!(catalog_revision, 8);
    assert_eq!(control_id, "PHMCacheOnly");
    assert_eq!(descriptor.deltas.len(), 1);
    let target = MorphTarget::from_sparse_deltas_shared(
        "cache-only",
        vertex_count,
        descriptor.deltas.as_slice(),
        Arc::clone(&canonical_faces),
        MorphAuthoring::vam(descriptor.minimum, descriptor.maximum, descriptor.default),
    )
    .unwrap();
    state.dispatch(Action::FinishVaMMorph {
        catalog_revision,
        geometry_revision,
        control_id,
        outcome: Ok(VaMMorphPayload {
            target: Arc::new(target),
            unsupported_formula_count: descriptor.unsupported_formula_count,
        }),
    });
    let control = state
        .morph_library
        .controls()
        .iter()
        .find(|control| control.id == "PHMCacheOnly")
        .unwrap();
    assert_eq!(
        control.availability,
        crate::morphs::MorphAvailability::Resolved
    );
    assert_eq!(control.unsupported_formula_count, 2);
    assert!((state.workspace.result_output.as_ref().unwrap().vertices[1][1] + 0.1).abs() < 1.0e-9);
}

#[test]
fn freezing_a_gaze_reaches_the_eyelid_controls_the_bank_names() {
    let mut state = ready_state();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    state.vam_catalog_revision = 13;
    state.vam_morph_faces = state
        .workspace
        .template_geometry
        .as_ref()
        .map(|geometry| Arc::new(geometry.faces.clone()));
    let lids = ["PHMEyelidsTopDownL", "PHMEyeLidsBottomDownL"]
        .into_iter()
        .map(|internal_name| CachedBuiltinMorph {
            internal_name: internal_name.to_owned(),
            label: internal_name.to_owned(),
            category: VaMMorphCategory::Eyes,
            minimum: 0.0,
            maximum: 1.0,
            default: 0.0,
            deltas: Arc::new(vec![(1, [0.0, -0.2, 0.0])]),
            unsupported_formula_count: 0,
        })
        .collect::<Vec<_>>();
    state.install_vam_controls(&lids);
    for cached in lids {
        state
            .vam_cached_morphs
            .insert(cached.internal_name.clone(), cached);
    }
    let lid_value = |state: &AppState, id: &str| {
        state
            .morph_library
            .controls()
            .iter()
            .find(|control| control.id == id)
            .map(|control| control.value)
            .expect("the lid control is in the library")
    };

    state.dispatch(Action::FreezeEyeGaze([0.0, -1.0]));

    assert_eq!(state.frozen_eye_gaze, Some([0.0, -1.0]));
    assert!(
        lid_value(&state, "PHMEyelidsTopDownL") > 0.0,
        "an eye looking down brings its upper lid with it"
    );
    assert!(
        lid_value(&state, "PHMEyeLidsBottomDownL") > 0.0,
        "and drags its lower lid down harder"
    );

    state.dispatch(Action::ResetEyeGaze);
    assert!(state.frozen_eye_gaze.is_none());
    assert!(lid_value(&state, "PHMEyelidsTopDownL").abs() < f32::EPSILON);
    assert!(lid_value(&state, "PHMEyeLidsBottomDownL").abs() < f32::EPSILON);
}

#[test]
fn skin_catalog_refresh_preserves_builtin_morphs_and_geometry() {
    let mut state = ready_state();
    state.workspace.eye_morph = Some(Arc::new(tiny_morph()));
    state.vam_root = Some(PathBuf::from(r"C:\VaM"));
    state.morph_library.replace_source(
        MorphSource::VaMBuiltin,
        vec![MorphControl::new(
            "old-vam",
            "Old VaM",
            MorphCategory::Mouth,
            MorphSource::VaMBuiltin,
            Arc::new(tiny_morph()),
        )],
    );
    assert!(state.morph_library.set_value("old-vam", 1.0));
    state.rebuild_morph_preview(state.eye_closure).unwrap();
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices[1][1],
        -0.2
    );
    let preview_before = state
        .workspace
        .result_output
        .as_ref()
        .unwrap()
        .vertices
        .clone();

    state.dispatch(Action::RefreshVaMCatalog);

    assert!(
        state
            .morph_library
            .controls()
            .iter()
            .any(|control| control.id == "old-vam")
    );
    assert_eq!(
        state.workspace.result_output.as_ref().unwrap().vertices,
        preview_before
    );
    assert!(state.take_vam_morph_work().is_none());
    assert_eq!(state.vam_catalog_status, VaMCatalogStatus::Indexing);
}

#[test]
fn explicit_figure_sex_requeues_the_matching_vam_bank_without_touching_geometry() {
    let mut state = AppState {
        revision: 17,
        ..AppState::default()
    };
    state.dispatch(Action::SetVaMRoot(PathBuf::from(r"C:\VaM")));
    state.dispatch(Action::RefreshVaMCatalog);
    let _ = state.take_vam_work();

    state.dispatch(Action::SetFigureSex(FigureSex::Male));

    assert_eq!(state.figure_sex, FigureSex::Male);
    assert_eq!(state.revision, 17);
    assert!(matches!(
        state.take_vam_work(),
        Some(VaMWorkRequest::ScanAppearance {
            catalog_revision: 3,
            figure_sex: SkinSex::Male,
            ..
        })
    ));
}

#[test]
fn stale_vam_catalog_completion_cannot_replace_a_new_root() {
    let mut state = AppState::default();
    state.dispatch(Action::SetVaMRoot(PathBuf::from(r"F:\OldVaM")));
    state.dispatch(Action::RefreshVaMCatalog);
    state.dispatch(Action::SetVaMRoot(PathBuf::from(r"F:\NewVaM")));
    state.dispatch(Action::RefreshVaMCatalog);

    state.dispatch(Action::FinishVaMCatalog {
        catalog_revision: 1,
        outcome: Err("old scan failed late".to_owned()),
    });

    assert_eq!(state.vam_root, Some(PathBuf::from(r"F:\NewVaM")));
    assert_eq!(state.vam_catalog_status, VaMCatalogStatus::Indexing);
    assert!(matches!(
        state.take_vam_appearance_work(),
        Some(VaMWorkRequest::ScanAppearance {
            catalog_revision: 4,
            requested_root,
            ..
        }) if requested_root == Path::new(r"F:\NewVaM")
    ));
}

#[test]
fn vam_pair_import_rejects_missing_template_and_pose_metadata() {
    let shape_metadata = vkit_core::vam::build_shape_vmi("shape", "Shape", 1);
    let shape_vmb = vkit_core::vam::encode_vmb_daz_cm(
        &[vkit_core::vam::SparseDelta {
            vertex_index: 7,
            delta_cm: [0.0, 0.1, 0.0],
        }],
        VAM_SHARED_BODY_VERTEX_COUNT as usize,
    )
    .unwrap();
    let (shape_vmi_path, _) = write_test_vam_pair("missing-template", &shape_metadata, &shape_vmb);
    let state = AppState::default();
    assert!(
        state
            .prepare_vam_morph_pair(&shape_vmi_path)
            .unwrap_err()
            .contains("Select your VaM folder first")
    );
    std::fs::remove_dir_all(shape_vmi_path.ancestors().nth(6).unwrap()).unwrap();

    let mut metadata = vkit_core::vam::build_shape_vmi("pose", "Pose", 1);
    metadata.is_pose_control = Some(vkit_core::vam::VmiBooleanScalar::Bool(true));
    let vmb = vkit_core::vam::encode_vmb_daz_cm(
        &[vkit_core::vam::SparseDelta {
            vertex_index: 7,
            delta_cm: [0.0, 0.1, 0.0],
        }],
        VAM_SHARED_BODY_VERTEX_COUNT as usize,
    )
    .unwrap();
    let (vmi_path, _) = write_test_vam_pair("pose", &metadata, &vmb);
    let mut state = AppState::default();
    state.workspace.template_geometry = Some(Arc::new(canonical_body_template()));
    let error = state.prepare_vam_morph_pair(&vmi_path).unwrap_err();
    assert!(error.contains("pose control"));
    std::fs::remove_dir_all(vmi_path.ancestors().nth(6).unwrap()).unwrap();
}

#[test]
fn vam_pair_import_keeps_the_shared_body_when_the_morph_reaches_past_it() {
    let metadata = vkit_core::vam::build_shape_vmi("tail", "Appended tail", 2);
    let vmb = vkit_core::vam::encode_vmb_daz_cm(
        &[
            vkit_core::vam::SparseDelta {
                vertex_index: 7,
                delta_cm: [0.0, 0.2, 0.0],
            },
            vkit_core::vam::SparseDelta {
                vertex_index: VAM_SHARED_BODY_VERTEX_COUNT,
                delta_cm: [0.0, 0.1, 0.0],
            },
        ],
        vkit_core::vam::VAM_FEMALE_VERTEX_COUNT,
    )
    .unwrap();
    let (vmi_path, _) = write_test_vam_pair("tail", &metadata, &vmb);
    let mut state = AppState::default();
    state.workspace.template_geometry = Some(Arc::new(canonical_body_template()));
    let (_, detail) = state.prepare_vam_morph_pair(&vmi_path).unwrap();

    assert!(detail.contains("1 shared-body deltas"), "{detail}");
    std::fs::remove_dir_all(vmi_path.ancestors().nth(6).unwrap()).unwrap();
}

#[test]
fn vam_pair_import_rejects_a_delta_outside_the_whole_figure() {
    let metadata = vkit_core::vam::build_shape_vmi("oob", "Out of bounds", 1);
    let vmb = vkit_core::vam::encode_vmb_daz_cm(
        &[vkit_core::vam::SparseDelta {
            vertex_index: vkit_core::vam::VAM_FEMALE_VERTEX_COUNT as u32,
            delta_cm: [0.0, 0.1, 0.0],
        }],
        vkit_core::vam::VAM_FEMALE_VERTEX_COUNT + 1,
    )
    .unwrap();
    let (vmi_path, _) = write_test_vam_pair("oob", &metadata, &vmb);
    let mut state = AppState::default();
    state.workspace.template_geometry = Some(Arc::new(canonical_body_template()));
    let error = state.prepare_vam_morph_pair(&vmi_path).unwrap_err();
    assert!(error.contains("only 24928 vertices"), "{error}");
    std::fs::remove_dir_all(vmi_path.ancestors().nth(6).unwrap()).unwrap();
}

#[test]
fn vam_pair_import_rejects_unclassified_route_and_raw_count_mismatch() {
    let metadata = vkit_core::vam::build_shape_vmi("shape", "Shape", 1);
    let vmb = vkit_core::vam::encode_vmb_daz_cm(
        &[vkit_core::vam::SparseDelta {
            vertex_index: 7,
            delta_cm: [0.0, 0.1, 0.0],
        }],
        VAM_SHARED_BODY_VERTEX_COUNT as usize,
    )
    .unwrap();
    let (unknown_path, _) =
        write_test_vam_pair_at_route("unknown-route", "female_backup", &metadata, &vmb);
    let mut state = AppState::default();
    state.workspace.template_geometry = Some(Arc::new(canonical_body_template()));
    assert!(
        state
            .prepare_vam_morph_pair(&unknown_path)
            .unwrap_err()
            .contains("exact Custom/Atom/Person/Morphs")
    );
    std::fs::remove_dir_all(unknown_path.ancestors().nth(6).unwrap()).unwrap();

    let mismatched_metadata = vkit_core::vam::build_shape_vmi("mismatch", "Mismatch", 2);
    let (mismatched_path, _) = write_test_vam_pair("raw-count", &mismatched_metadata, &vmb);
    let error = state.prepare_vam_morph_pair(&mismatched_path).unwrap_err();
    assert!(error.contains("VMI declares 2 deltas"));
    assert!(error.contains("VMB header declares 1"));
    std::fs::remove_dir_all(mismatched_path.ancestors().nth(6).unwrap()).unwrap();
}

#[test]
fn genital_pair_import_requires_matching_figure_and_user_owned_provider() {
    let metadata = vkit_core::vam::build_shape_vmi("graft", "Graft", 1);
    let local_vmb = vkit_core::vam::encode_vmb_daz_cm(
        &[vkit_core::vam::SparseDelta {
            vertex_index: 0,
            delta_cm: [0.25, 0.0, 0.0],
        }],
        3_372,
    )
    .unwrap();
    let (female_path, _) =
        write_test_vam_pair_at_route("female-genital", "female_genitalia", &metadata, &local_vmb);
    let mut state = AppState::default();
    state.workspace.template_geometry = Some(Arc::new(canonical_body_template()));
    let error = state.prepare_vam_morph_pair(&female_path).unwrap_err();
    assert!(error.contains("validated user-owned VaM full-body geometry provider"));
    std::fs::remove_dir_all(female_path.ancestors().nth(6).unwrap()).unwrap();

    let (male_path, _) =
        write_test_vam_pair_at_route("male-genital", "male_genitalia", &metadata, &local_vmb);
    let error = state.prepare_vam_morph_pair(&male_path).unwrap_err();
    assert!(error.contains("current figure"));
    std::fs::remove_dir_all(male_path.ancestors().nth(6).unwrap()).unwrap();
}

#[test]
fn edit_tab_opens_before_a_scan_once_the_g2_base_is_ready() {
    let mut state = AppState::default();

    assert!(!state.tab_available(Tab::Edit));
    state.dispatch(Action::RequestTab(Tab::Edit));
    assert_ne!(state.active_tab, Tab::Edit);

    state
        .workspace
        .load_template_geometry(tiny_template())
        .unwrap();
    assert!(state.tab_available(Tab::Edit));
    state.dispatch(Action::RequestTab(Tab::Edit));
    assert_eq!(state.active_tab, Tab::Edit);
    assert!(state.scan_path.is_none());
}

#[test]
fn template_install_bumps_the_reveal_generation_and_tracks_load_activity() {
    use super::template_ingest::{TemplateSexEvidence, TemplateSexReceipt};

    let mut state = AppState::default();
    assert_eq!(state.template_install_generation, 0);
    assert!(!state.template_load_active());

    let directory = tempfile::tempdir().expect("temporary import directory");
    let rejected = directory.path().join("not-g2.obj");
    std::fs::write(&rejected, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").expect("write rejected OBJ");
    state.load_template(rejected);
    assert!(
        state.template_load_active(),
        "requested load must be active"
    );
    let job = state
        .take_workspace_load_request()
        .expect("queued template job");
    assert!(state.template_load_active(), "running load must be active");
    let path = job.path().to_path_buf();
    let outcome = job.run();
    state.dispatch(Action::FinishWorkspaceLoad { path, outcome });
    assert!(!state.template_load_active());
    assert_eq!(
        state.template_install_generation, 0,
        "a rejected import must not signal a scene reveal"
    );

    let prepared = PreparedTemplateLoad {
        geometry: tiny_template(),
        detected: DetectedTemplateTopology::from_counts(3, 1),
        detected_sex: None,
        sex_receipt: TemplateSexReceipt {
            detected: None,
            effective: FigureSex::Female,
            evidence: TemplateSexEvidence::CurrentSelectionFallback,
            conflicting_file_name: false,
            structural_ambiguity: false,
        },
        normalization: None,
        provider: None,
        provider_path: None,
        provider_provenance: None,
    };
    state.commit_prepared_template(PathBuf::from("tiny.dsf"), prepared);
    assert_eq!(state.status.key, TextKey::TemplateLoaded);
    assert_eq!(state.template_install_generation, 1);
}

#[test]
fn template_commit_with_a_configured_root_prewarms_the_appearance_catalog() {
    use super::template_ingest::{TemplateSexEvidence, TemplateSexReceipt};

    let prepared = || PreparedTemplateLoad {
        geometry: tiny_template(),
        detected: DetectedTemplateTopology::from_counts(3, 1),
        detected_sex: None,
        sex_receipt: TemplateSexReceipt {
            detected: None,
            effective: FigureSex::Female,
            evidence: TemplateSexEvidence::CurrentSelectionFallback,
            conflicting_file_name: false,
            structural_ambiguity: false,
        },
        normalization: None,
        provider: None,
        provider_path: None,
        provider_provenance: None,
    };

    let mut state = AppState::default();
    state.commit_prepared_template(PathBuf::from("tiny.dsf"), prepared());
    assert!(state.take_vam_appearance_work().is_none());
    assert!(matches!(
        state.vam_catalog_status,
        VaMCatalogStatus::Unconfigured
    ));

    let mut state = AppState {
        vam_root: Some(PathBuf::from(r"D:\Games\VaM")),
        ..AppState::default()
    };
    state.commit_prepared_template(PathBuf::from("tiny.dsf"), prepared());
    let request = state
        .take_vam_appearance_work()
        .expect("commit must queue the appearance scan");
    let VaMWorkRequest::ScanAppearance {
        template_geometry, ..
    } = request
    else {
        panic!("expected an appearance scan request");
    };
    assert!(
        template_geometry.is_some(),
        "the eager scan must carry the committed template for UV mapping"
    );
    assert!(matches!(
        state.vam_catalog_status,
        VaMCatalogStatus::Indexing
    ));
}

fn lru_preset(identifier: &str) -> vkit_core::vam::SkinPreset {
    vkit_core::vam::SkinPreset {
        stable_id: identifier.to_owned(),
        label: identifier.to_owned(),
        source: vkit_core::vam::AssetLocator::File(PathBuf::from("preset.vap")),
        sex: SkinSex::Female,
        textures: {
            let mut set = vkit_core::vam::SkinTextureSet::default();
            set.insert(
                vkit_core::vam::SkinRegion::Face,
                vkit_core::vam::SkinTextureChannel::Diffuse,
                AssetLocator::UnresolvedVaMUrl("texture://fixture/face".to_owned()),
            );
            set
        },
        auxiliary: Default::default(),
    }
}

fn lru_preview(revision: u64) -> Arc<SkinPreview> {
    use crate::skin_preview::{
        SkinChannel, SkinCorner, SkinImage, SkinPreviewGeometry, SkinSurfaceMap, SkinTriangle,
    };
    let solid = |offset: u64| Arc::new(SkinImage::solid(revision * 100 + offset, [200; 4]));
    let geometry = Arc::new(
        SkinPreviewGeometry::new(
            revision,
            vec![SkinTriangle {
                source_triangle_id: 0,
                channel: SkinChannel::Face,
                corners: [
                    SkinCorner {
                        vertex_id: 0,
                        uv: [0.0, 0.0],
                    },
                    SkinCorner {
                        vertex_id: 1,
                        uv: [1.0, 0.0],
                    },
                    SkinCorner {
                        vertex_id: 2,
                        uv: [0.0, 1.0],
                    },
                ],
            }],
        )
        .unwrap(),
    );
    Arc::new(SkinPreview {
        revision,
        geometry,
        face: solid(0),
        torso: solid(1),
        sclera: solid(2),
        iris: solid(3),
        lacrimal: solid(4),
        inner_mouth: solid(5),
        teeth: solid(6),
        gums: solid(7),
        tongue: solid(8),
        eyelashes: solid(9),
        face_surface: SkinSurfaceMap::neutral(revision * 100 + 20),
        torso_surface: SkinSurfaceMap::neutral(revision * 100 + 21),
        mouth_surface_atlas: SkinSurfaceMap::neutral_mouth_atlas(revision * 100 + 22),
        sclera_surface: SkinSurfaceMap::neutral(revision * 100 + 23),
        iris_surface: SkinSurfaceMap::neutral(revision * 100 + 24),
        lacrimal_surface: SkinSurfaceMap::neutral(revision * 100 + 25),
        auxiliary_colors: [[255; 4]; 8],
        auxiliary_textured: [false; 8],
    })
}

#[test]
#[allow(clippy::field_reassign_with_default)]
fn reselecting_a_recent_skin_reuses_the_cached_preview_without_a_worker() {
    let mut state = AppState::default();
    state.vam_skin_presets = vec![lru_preset("skin-a"), lru_preset("skin-b")];
    state.vam_uv_mapping = Some(Arc::new(vkit_core::vam::G2UvMapping {
        source_path: PathBuf::from("femalecustom.obj"),
        coordinate_rms_cm: 0.0,
        coordinate_max_cm: 0.0,
        uncovered_triangles: 0,
        faces: Vec::new(),
        triangles: Vec::new(),
    }));

    state.dispatch(Action::SelectVaMSkin(Some("skin-a".to_owned())));
    let request = state
        .take_skin_work()
        .expect("first selection needs a worker");
    let preview_a = lru_preview(request.request_id);
    state.dispatch(Action::FinishVaMSkin {
        request_id: request.request_id,
        preset_id: "skin-a".to_owned(),
        outcome: Ok(Arc::clone(&preview_a)),
    });
    assert!(state.skin_preview.is_some());

    state.dispatch(Action::SelectVaMSkin(Some("skin-b".to_owned())));
    let request_b = state
        .take_skin_work()
        .expect("second selection needs a worker");
    state.dispatch(Action::FinishVaMSkin {
        request_id: request_b.request_id,
        preset_id: "skin-b".to_owned(),
        outcome: Ok(lru_preview(request_b.request_id)),
    });

    state.dispatch(Action::SelectVaMSkin(Some("skin-a".to_owned())));
    assert!(state.take_skin_work().is_none());
    assert!(!state.skin_preview_loading);
    let reinstalled = state.skin_preview.as_ref().expect("cached preview");
    assert!(Arc::ptr_eq(reinstalled, &preview_a));
    assert_eq!(state.status.key, TextKey::SkinReady);

    state.invalidate_vam_catalog_assets();
    state.vam_skin_presets = vec![lru_preset("skin-a")];
    state.vam_uv_mapping = Some(Arc::new(vkit_core::vam::G2UvMapping {
        source_path: PathBuf::from("femalecustom.obj"),
        coordinate_rms_cm: 0.0,
        coordinate_max_cm: 0.0,
        uncovered_triangles: 0,
        faces: Vec::new(),
        triangles: Vec::new(),
    }));
    state.dispatch(Action::SelectVaMSkin(Some("skin-a".to_owned())));
    assert!(state.take_skin_work().is_some());
}

#[test]
fn texture_layer_reducer_keeps_stack_mask_and_source_retouch_coherent() {
    let mut state = AppState::default();
    let lower = state
        .texture_project
        .add_image_layer(PathBuf::from("lower.png"), TextureSourceMode::LandmarkPins);
    let upper = state
        .texture_project
        .add_image_layer(PathBuf::from("upper.png"), TextureSourceMode::LandmarkPins);
    state.texture_project.finish_decode(
        upper,
        Ok(Arc::new(
            crate::skin_preview::SkinImage::new(7, 4, 4, [80, 90, 100, 255].repeat(16)).unwrap(),
        )),
    );

    assert_eq!(state.texture_project.layers[0].id, upper);
    state.dispatch(Action::MoveTextureLayerTo {
        id: lower,
        insertion_index: 0,
    });
    assert_eq!(state.texture_project.layers[0].id, lower);
    state.dispatch(Action::SelectTextureLayer(upper));
    state.dispatch(Action::AddTextureMaskDab {
        id: upper,
        uv: [0.5, 0.5],
        source: Some([0.5, 0.5]),
        subtract: true,
    });
    state.dispatch(Action::AddTextureRetouchDab {
        id: upper,
        point: [0.5, 0.5],
        tool: TextureTool::DodgeBurn,
        reverse: false,
    });
    let layer = state
        .texture_project
        .layers
        .iter()
        .find(|layer| layer.id == upper)
        .unwrap();
    assert!(layer.mask.is_some());
    assert!(layer.edited_image.is_some());
    assert!(state.texture_project.dirty);

    state.dispatch(Action::SetTexturePinOpacity(0.0));
    state.dispatch(Action::SetTextureMaskPreviewEnabled(true));
    assert_eq!(state.texture_project.pin_opacity, 0.15);
    assert!(state.texture_project.mask_preview_enabled);
}

#[test]
fn the_face_boundary_softness_reaches_the_top_of_its_own_slider() {
    for resolution in [1024_u32, 2048, 4096] {
        let mut state = AppState::default();
        state.texture_project.resolution = resolution;
        let top = state.texture_project.max_boundary_feather_pixels();
        assert!(
            top > 128,
            "resolution {resolution} should allow a deeper dissolve than the old flat cap"
        );

        state.dispatch(Action::SetTextureBoundaryFeather(top));
        assert_eq!(
            state.texture_project.boundary_feather_pixels, top,
            "the slider's own maximum has to survive the round trip at {resolution}"
        );

        state.dispatch(Action::SetTextureBoundaryFeather(u16::MAX));
        assert_eq!(state.texture_project.boundary_feather_pixels, top);
    }
}

#[test]
fn a_look_for_the_other_figure_is_refused_rather_than_switching_figures() {
    let mut state = AppState {
        figure_sex: FigureSex::Female,
        active_tab: Tab::Morph,
        ..Default::default()
    };
    let look = |stable_id: &str, sex: Option<vkit_core::vam::SkinSex>| VaMEditSource {
        stable_id: stable_id.to_owned(),
        label: stable_id.to_owned(),
        path: std::path::PathBuf::from(format!("{stable_id}.vap")),
        sex,
        kind: VaMEditSourceKind::AppearancePreset,
        missing_morphs: 0,
    };
    let male_look = look("male-look", Some(vkit_core::vam::SkinSex::Male));
    let unlabelled = look("no-figure-look", None);
    assert!(!state.look_suits_current_figure(&male_look));

    assert!(state.look_suits_current_figure(&unlabelled));

    state.vam_edit_sources = vec![male_look.clone()];
    state.dispatch(Action::SelectVaMEditSource(male_look.stable_id.clone()));
    assert_eq!(
        state.figure_sex,
        FigureSex::Female,
        "the figure must not move"
    );
    assert_eq!(state.active_tab, Tab::Morph, "and the stage must not move");
    assert!(state.selected_vam_edit_source_id.is_none());
    assert!(state.pending_direct_edit_source.is_none());
}

#[test]
fn the_mask_overlay_follows_the_mask_brush() {
    let mut state = AppState::default();
    state.dispatch(Action::SetTextureTool(TextureTool::MaskBrush));
    assert!(state.texture_project.mask_preview_enabled);
    state.dispatch(Action::SetTextureTool(TextureTool::CloneStamp));
    assert!(!state.texture_project.mask_preview_enabled);
    state.dispatch(Action::SetTextureTool(TextureTool::MaskBrush));
    assert!(state.texture_project.mask_preview_enabled);
}

#[test]
fn hiding_every_layer_leaves_nothing_composited() {
    let mut state = AppState::default();
    let id = state
        .texture_project
        .add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    assert!(!state.texture_composite_is_empty());

    state.dispatch(Action::SetTextureLayerVisible { id, visible: false });
    assert!(state.texture_composite_is_empty());

    state.texture_project.baked_resolution = 1024;
    state.settle_empty_texture_composite();
    assert!(state.texture_project.baked.is_none());
    assert!(!state.texture_project.dirty);
    assert_eq!(state.texture_project.baked_resolution, 0);
}

#[test]
fn a_preview_bake_is_never_mistaken_for_the_export_bundle() {
    let mut state = AppState::default();
    state
        .texture_project
        .add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    state.texture_project.resolution = 4096;

    assert!(state.texture_export_bake_pending());

    state.texture_project.dirty = false;
    state.texture_project.baked_resolution = crate::texture_project::PREVIEW_BAKE_RESOLUTION;
    assert!(
        state.texture_export_bake_pending(),
        "the working preview is not the bundle"
    );
    assert!(!state.can_save_textures());

    state.texture_project.baked_resolution = 4096;
    assert!(state.texture_project.baked.is_none());
    assert!(state.texture_export_bake_pending());

    state.texture_project.mark_dirty();
    assert!(state.texture_export_bake_pending());
}

#[test]
fn baking_onto_a_missing_skin_points_at_the_panel_that_supplies_one() {
    use crate::texture_project::TextureBakeBase;
    let mut state = AppState::default();

    state.dispatch(Action::SetTextureBakeBase(TextureBakeBase::CurrentSkin));
    assert_eq!(
        state.texture_project.bake_base,
        TextureBakeBase::Transparent
    );
    assert_eq!(state.status.key, TextKey::SkinPresetRequired);
    assert_eq!(
        state.take_pending_attention(),
        Some(AttentionTarget::SkinPanel)
    );

    state.viewport_tool_panel = Some(crate::state::ViewportToolPanel::Skin);
    state.dispatch(Action::SetTextureBakeBase(TextureBakeBase::CurrentSkin));
    assert_eq!(
        state.take_pending_attention(),
        Some(AttentionTarget::SkinTextureMode)
    );
}

#[test]
fn hiding_the_vam_skin_drops_the_view_and_the_bake_to_the_flat_base() {
    use crate::texture_project::TextureBakeBase;
    let mut state = AppState::default();
    state.texture_project.bake_base = TextureBakeBase::CurrentSkin;
    state.base_view_mode = BaseViewMode::Texture;

    state.dispatch(Action::SetTextureHideVaMSkin(true));

    assert_eq!(state.base_view_mode, BaseViewMode::Solid);
    assert_eq!(
        state.texture_project.bake_base,
        TextureBakeBase::Transparent
    );
}

#[test]
fn swapping_the_vam_skin_restages_the_painted_composite() {
    let mut state = AppState::default();
    state
        .texture_project
        .add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    state.texture_project.dirty = false;

    state.dispatch(Action::SelectVaMSkin(None));
    assert!(
        state.texture_project.dirty,
        "a skin change leaves the painted composite stale"
    );

    let mut bare = AppState::default();
    bare.dispatch(Action::SelectVaMSkin(None));
    assert!(!bare.texture_project.dirty);
}

#[test]
fn the_projection_stencil_takes_over_the_pin_brush_while_it_is_up() {
    let mut state = ready_state();
    state.dispatch(Action::RequestTab(Tab::Texture));
    state
        .texture_project
        .add_image_layer(PathBuf::from("photo.png"), TextureSourceMode::LandmarkPins);
    state.dispatch(Action::SetTextureTool(TextureTool::MaskBrush));
    assert!(state.texture_project.mask_preview_enabled);

    state.dispatch(Action::SetTextureProjectionStencil(true));
    assert!(state.texture_project.projection_stencil);
    assert_eq!(state.texture_project.active_tool, TextureTool::Projection);
    assert!(!state.texture_project.mask_preview_enabled);

    state.dispatch(Action::SetTextureProjectionStencil(false));
    assert!(!state.texture_project.projection_stencil);

    assert_eq!(state.texture_project.active_tool, TextureTool::PinPair);
}

#[test]
fn hiding_the_vam_skin_is_a_view_pref_that_rebakes_without_touching_export() {
    use crate::texture_project::TextureBakeBase;
    let mut state = AppState::default();
    assert!(!state.texture_project.hide_vam_skin_preview);
    let export_base = state.texture_project.bake_base;
    state.texture_project.dirty = false;

    state.dispatch(Action::SetTextureHideVaMSkin(true));
    assert!(state.texture_project.hide_vam_skin_preview);

    assert!(state.texture_project.dirty);

    assert_eq!(state.texture_project.bake_base, export_base);
    assert_eq!(
        state.texture_project.bake_base,
        TextureBakeBase::Transparent
    );

    state.dispatch(Action::SetTextureHideVaMSkin(false));
    assert!(!state.texture_project.hide_vam_skin_preview);
}

#[test]
fn hiding_the_vam_skin_survives_an_empty_composite() {
    let mut state = AppState {
        skin_preview: Some(skin_preview(1, [200, 160, 140, 255])),
        ..AppState::default()
    };
    assert!(
        state.active_skin_preview().is_some(),
        "the selected skin shows while it is not hidden"
    );

    state.dispatch(Action::SetTextureHideVaMSkin(true));
    assert!(
        state.active_skin_preview().is_none(),
        "the hidden skin came back once there was no composite to hide it in"
    );

    state.dispatch(Action::SetTextureHideVaMSkin(false));
    assert!(state.active_skin_preview().is_some());
}

#[test]
fn leaving_the_look_browser_keeps_the_look_it_loaded() {
    let mut state = AppState::default();
    state.set_look_browser_open(true);
    assert!(state.morph_look_find_open);

    let first = state.morphs_before_browsing.clone();
    state.set_look_browser_open(true);
    assert_eq!(
        state.morphs_before_browsing.is_some(),
        first.is_some(),
        "the snapshot is taken on the way in, once"
    );

    state.selected_vam_edit_source_id = Some("some-look".to_owned());
    state.set_look_browser_open(false);
    assert!(!state.morph_look_find_open, "leaving closes the browser");
    assert!(
        state.morphs_before_browsing.is_none(),
        "the snapshot is dropped rather than restored"
    );
    assert_eq!(
        state.selected_vam_edit_source_id.as_deref(),
        Some("some-look"),
        "the look that was picked stays picked"
    );
}

#[test]
fn the_face_uv_index_answers_what_the_scan_answered() {
    use vkit_core::vam::{G2UvMapping, G2UvTriangle, UvMaterialRegion};
    let triangle = |canonical: u32, region: UvMaterialRegion| G2UvTriangle {
        canonical_face_index: canonical,
        canonical_triangle_index: canonical,
        material_region: region,
        on_head: region == UvMaterialRegion::Face,
        position_indices: [0, 1, 2],
        uvs: [[0.0, 0.0]; 3],
    };
    let mapping = Arc::new(G2UvMapping {
        source_path: std::path::PathBuf::from("test"),
        coordinate_rms_cm: 0.0,
        coordinate_max_cm: 0.0,
        faces: Vec::new(),
        triangles: vec![
            triangle(7, UvMaterialRegion::Torso),
            triangle(7, UvMaterialRegion::Face),
            triangle(9, UvMaterialRegion::Face),
            triangle(7, UvMaterialRegion::Face),
        ],
        uncovered_triangles: 0,
    });

    let mut state = AppState::default();
    assert!(state.face_uv_rows.is_none(), "no mapping, no index");
    state.vam_uv_mapping = Some(Arc::clone(&mapping));
    state.rebuild_face_uv_rows();

    let index = state
        .face_uv_rows
        .clone()
        .expect("index follows the mapping");
    for canonical in [7_u32, 9] {
        let scanned = mapping
            .triangles
            .iter()
            .position(|candidate| {
                candidate.canonical_triangle_index == canonical
                    && candidate.material_region == UvMaterialRegion::Face
            })
            .expect("the scan finds what it is looking for");
        assert_eq!(index.get(&canonical).copied(), Some(scanned), "{canonical}");
    }

    assert_eq!(index.get(&11), None);

    state.vam_uv_mapping = None;
    state.rebuild_face_uv_rows();
    assert!(state.face_uv_rows.is_none(), "the index leaves with it");
}
