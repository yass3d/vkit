use super::history::TEXTURE_UNDO_BYTES;
use super::kernels::{
    build_baked_preview, exact_linear_to_srgb, layer_raster_cache_matches, linear_to_srgb,
    preview_face_is_coarser_than, rasterize_scan_layer, srgb_to_linear,
};
use super::*;

#[test]
fn the_export_reaches_past_the_preview_only_when_the_preview_is_coarser() {
    assert!(
        preview_face_is_coarser_than((2048, 2048), 4096),
        "a 2048 preview cannot answer for a 4096 export"
    );
    assert!(!preview_face_is_coarser_than((2048, 2048), 2048));
    assert!(!preview_face_is_coarser_than((2048, 2048), 1024));
    assert!(!preview_face_is_coarser_than((4096, 4096), 4096));
}

#[test]
fn a_painted_layer_carries_its_adjustment_once() {
    let painted = vec![120_u8, 120, 120, 255];
    let adjustments = TextureColorAdjustments {
        exposure: 0.6,
        contrast: 0.35,
        ..TextureColorAdjustments::default()
    };

    let mut once = painted.clone();
    apply_color_adjustments(&mut once, adjustments);
    let mut twice = once.clone();
    apply_color_adjustments(&mut twice, adjustments);

    assert_ne!(
        once, twice,
        "the adjustment moves pixels, so applying it twice is visible"
    );
    assert_ne!(
        once, painted,
        "and applying it once is not a no-op either, or the test proves nothing"
    );
}

#[test]
fn a_painted_atlas_keeps_the_resolution_it_was_authored_at() {
    let mut project = TextureProject {
        resolution: 2048,
        ..Default::default()
    };
    let mut layer = TextureLayer::image(
        1,
        PathBuf::from("face.png"),
        TextureSourceMode::LandmarkPins,
    );
    layer.painted = Some(TextureLayerPaint {
        revision: next_paint_revision(),
        width: 2048,
        height: 2048,
        rgba8: Arc::new(vec![0; 2048 * 2048 * 4]),
    });
    project.layers.push(layer);
    project.selected_layer_id = Some(1);

    project.resolution = 4096;
    let source = SkinImage::new(0, 4, 4, [200, 180, 170, 255].repeat(16)).unwrap();
    project.stamp_projection(
        &source,
        &[],
        vkit_core::texture_bake::ProjectionBrush {
            centre: [0.5, 0.5],
            radius: 0.1,
            falloff: SculptFalloff::Smooth,
            opacity: 1.0,
            erase: false,
        },
        |_| None,
    );

    let paint = project.layers[0].painted.as_ref().expect("atlas survives");
    assert_eq!(
        (paint.width, paint.height),
        (2048, 2048),
        "raising the export size must not resample painted pixels into detail nobody drew"
    );
}

#[test]
fn a_retouch_dab_records_the_box_it_touched() {
    let mut project = TextureProject::default();
    let mut layer =
        TextureLayer::image(1, PathBuf::from("face.png"), TextureSourceMode::MaterialUv);
    layer.image = Some(Arc::new(
        SkinImage::new(0, 64, 64, [120, 120, 120, 255].repeat(64 * 64)).unwrap(),
    ));
    project.layers.push(layer);
    project.mask_brush_radius = 0.05;

    project.apply_retouch_dab(1, TextureTool::DodgeBurn, [0.5, 0.5], false);
    let layer = &project.layers[0];
    let (revision, region) = *layer.edited_regions.back().expect("a dab was recorded");
    assert_eq!(
        revision,
        layer.edited_image.as_ref().unwrap().revision,
        "the box is tagged with the revision it produced"
    );

    assert!(region[0] > 24 && region[2] < 40, "{region:?}");
    assert!(region[1] > 24 && region[3] < 40, "{region:?}");
}

#[test]
fn a_dab_on_the_projected_atlas_records_the_box_it_touched() {
    let mut project = TextureProject::default();
    let mut layer = TextureLayer::image(
        1,
        PathBuf::from("face.png"),
        TextureSourceMode::LandmarkPins,
    );
    let before = next_paint_revision();
    layer.painted = Some(TextureLayerPaint {
        revision: next_paint_revision(),
        width: 64,
        height: 64,
        rgba8: Arc::new([120, 120, 120, 255].repeat(64 * 64)),
    });
    project.layers.push(layer);
    project.mask_brush_radius = 0.05;

    project.apply_retouch_dab(1, TextureTool::DodgeBurn, [0.5, 0.5], false);
    let layer = &project.layers[0];
    let paint = layer.painted.as_ref().expect("the atlas is still there");
    let (revision, region) = *layer.painted_regions.back().expect("a dab was recorded");
    assert_eq!(
        revision, paint.revision,
        "the box is tagged with the revision it produced"
    );
    assert!(
        paint.revision > before,
        "a dab has to move the atlas revision or no texture cache will refresh"
    );

    assert!(region[0] > 24 && region[2] < 40, "{region:?}");
    assert!(region[1] > 24 && region[3] < 40, "{region:?}");
}

#[test]
fn a_stamp_asks_the_canvas_to_redraw_the_whole_atlas() {
    let mut project = TextureProject::default();
    let mut layer = TextureLayer::image(
        1,
        PathBuf::from("face.png"),
        TextureSourceMode::LandmarkPins,
    );
    layer.painted = Some(TextureLayerPaint {
        revision: next_paint_revision(),
        width: 64,
        height: 64,
        rgba8: Arc::new(vec![0; 64 * 64 * 4]),
    });
    layer.painted_regions.push_back((1, [0, 0, 1, 1]));
    project.layers.push(layer);
    project.selected_layer_id = Some(1);

    let source = SkinImage::new(0, 4, 4, [200, 180, 170, 255].repeat(16)).unwrap();
    let triangle = vkit_core::texture_bake::ProjectedTriangle {
        screen: [[0.45, 0.45], [0.55, 0.45], [0.5, 0.55]],
        uv: [[0.2, 0.2], [0.8, 0.2], [0.5, 0.8]],
    };
    let painted = project.stamp_projection(
        &source,
        std::slice::from_ref(&triangle),
        vkit_core::texture_bake::ProjectionBrush {
            centre: [0.5, 0.5],
            radius: 0.2,
            falloff: SculptFalloff::Smooth,
            opacity: 1.0,
            erase: false,
        },
        |_| Some([0.5, 0.5]),
    );
    assert!(painted > 0, "the stamp has to reach the atlas");
    assert!(
        project.layers[0].painted_regions.is_empty(),
        "a stamp reports no box, so the history must not claim the atlas is patchable"
    );
}

#[test]
fn the_resampled_skin_face_is_only_offered_back_at_its_own_resolution() {
    let project = TextureProject {
        base_face: Some(CachedBaseFace {
            preview_revision: 7,
            resolution: PREVIEW_BAKE_RESOLUTION,
            from_preview_face: true,
            image: Arc::new(SkinImage::new(7, 1, 1, vec![1, 2, 3, 4]).unwrap()),
        }),
        ..Default::default()
    };
    assert!(
        project.cached_base_face(PREVIEW_BAKE_RESOLUTION).is_some(),
        "the next preview bake must not resample the skin again"
    );
    assert!(
        project.cached_base_face(4096).is_none(),
        "an export must never composite over a preview-sized base"
    );
}

#[test]
fn the_preview_build_reuses_a_resample_the_bake_already_paid_for() {
    let mapping = G2UvMapping {
        source_path: PathBuf::new(),
        coordinate_rms_cm: 0.0,
        coordinate_max_cm: 0.0,
        uncovered_triangles: 0,
        faces: Vec::new(),
        triangles: vec![vkit_core::vam::G2UvTriangle {
            canonical_face_index: 0,
            canonical_triangle_index: 0,
            material_region: UvMaterialRegion::Face,
            on_head: true,
            position_indices: [0, 1, 2],
            uvs: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        }],
    };
    let mut base = neutral_preview(1, &mapping, [200, 100, 50]).unwrap();
    let face = (0..16 * 16)
        .flat_map(|index| [(index % 251) as u8, 40, 90, 255])
        .collect::<Vec<_>>();
    base.face = Arc::new(SkinImage::new(1, 16, 16, face).unwrap());

    let mut images = BTreeMap::new();
    images.insert(
        TextureChannel::Diffuse,
        Arc::new(SkinImage::new(2, 4, 4, [255, 0, 0, 128].repeat(16)).unwrap()),
    );

    let resampled = Arc::new(
        SkinImage::new(
            1,
            4,
            4,
            resize_rgba_box(
                RgbaView {
                    rgba8: &base.face.rgba8,
                    width: base.face.width,
                    height: base.face.height,
                },
                4,
                4,
            ),
        )
        .unwrap(),
    );

    let resampled_here =
        build_baked_preview(3, &mapping, Some(&base), None, [0, 0, 0], &images).unwrap();
    let handed_over = build_baked_preview(
        3,
        &mapping,
        Some(&base),
        Some(&resampled),
        [0, 0, 0],
        &images,
    )
    .unwrap();
    assert_eq!(
        resampled_here.face.rgba8, handed_over.face.rgba8,
        "reusing the bake's own resample has to be pixel-identical to redoing it"
    );

    let wrong_size = Arc::new(SkinImage::new(1, 2, 2, vec![9; 16]).unwrap());
    let refused = build_baked_preview(
        3,
        &mapping,
        Some(&base),
        Some(&wrong_size),
        [0, 0, 0],
        &images,
    )
    .unwrap();
    assert_eq!(
        resampled_here.face.rgba8, refused.face.rgba8,
        "a resample of the wrong size must be ignored, not stretched over the face"
    );
}

fn one_stroke(layer_id: u64, tool: TextureTool, size: impl Into<RasterSize>) -> StrokeCoverage {
    StrokeCoverage::new(layer_id, tool, size.into())
}

#[test]
fn the_stencil_reads_back_the_corners_it_is_drawn_at() {
    let centre = [400.0_f32, 300.0];
    let size = [200.0_f32, 100.0];
    for placement in [
        StencilPlacement::default(),
        StencilPlacement::default().panned([37.0, -18.0]),
        StencilPlacement {
            scale: 2.5,
            ..StencilPlacement::default()
        },
        StencilPlacement {
            rotation: std::f32::consts::FRAC_PI_3,
            ..StencilPlacement::default()
        },
        StencilPlacement {
            offset: [-60.0, 25.0],
            scale: 0.6,
            rotation: -0.9,
        },
    ] {
        let middle = [
            centre[0] + placement.offset[0],
            centre[1] + placement.offset[1],
        ];
        let uv = placement
            .source_at(middle, centre, size)
            .expect("the centre is on the image");
        assert!(
            (uv[0] - 0.5).abs() < 1.0e-3 && (uv[1] - 0.5).abs() < 1.0e-3,
            "{placement:?} put its centre at {uv:?}"
        );

        let half = [
            size[0] * placement.scale * 0.5,
            size[1] * placement.scale * 0.5,
        ];
        let (sine, cosine) = placement.rotation.sin_cos();
        for (corner, expected) in [
            ([-half[0], -half[1]], [0.0_f32, 0.0]),
            ([half[0], -half[1]], [1.0, 0.0]),
            ([half[0], half[1]], [1.0, 1.0]),
            ([-half[0], half[1]], [0.0, 1.0]),
        ] {
            let screen = [
                middle[0] + corner[0] * cosine - corner[1] * sine,
                middle[1] + corner[0] * sine + corner[1] * cosine,
            ];
            let uv = placement
                .source_at(screen, centre, size)
                .unwrap_or_else(|| panic!("{placement:?} lost its corner {expected:?}"));
            assert!(
                (uv[0] - expected[0]).abs() < 1.0e-2 && (uv[1] - expected[1]).abs() < 1.0e-2,
                "{placement:?} corner {expected:?} read back {uv:?}"
            );
        }

        assert!(
            placement
                .source_at([middle[0] + 10_000.0, middle[1]], centre, size)
                .is_none()
        );
    }
}

#[test]
fn the_stencil_zooms_about_the_pointer() {
    let centre = [400.0_f32, 300.0];
    let size = [200.0_f32, 100.0];
    let pointer = [460.0_f32, 320.0];
    let placement = StencilPlacement::default();
    let before = placement
        .source_at(pointer, centre, size)
        .expect("on the image");
    let after = placement
        .zoomed(2.0, pointer, centre)
        .source_at(pointer, centre, size)
        .expect("still on the image");
    assert!(
        (before[0] - after[0]).abs() < 1.0e-3 && (before[1] - after[1]).abs() < 1.0e-3,
        "the pixel under the cursor moved: {before:?} -> {after:?}"
    );
}

#[test]
fn textures_export_beside_vam_own_folders_and_not_under_a_sex_directory() {
    let project = TextureProject::default();
    let root = PathBuf::from("V:/VaM");
    for (sex, expected) in [
        (FigureSex::Female, "FemaleBase"),
        (FigureSex::Male, "MaleBase"),
    ] {
        let directory = project
            .default_export_directory(Some(&root), sex)
            .expect("a root gives a directory");
        assert_eq!(
            directory,
            root.join("Custom")
                .join("Atom")
                .join("Person")
                .join("Textures")
                .join(expected)
        );
        assert!(
            !directory
                .components()
                .any(|part| part.as_os_str() == "Female" || part.as_os_str() == "Male"),
            "no sex directory: {}",
            directory.display()
        );
    }
}

#[test]
fn a_colour_map_with_alpha_is_named_a_decal_and_an_opaque_one_a_diffuse() {
    let decal = texture_export_filename("winter", TextureChannel::Diffuse, false);
    let diffuse = texture_export_filename("winter", TextureChannel::Diffuse, true);
    assert_eq!(decal, "winter_decal.png");
    assert_eq!(diffuse, "winter_diffuse.jpg");

    for channel in TextureChannel::ALL {
        if channel == TextureChannel::Diffuse {
            continue;
        }
        assert_eq!(
            channel.suffix_for(true),
            channel.suffix_for(false),
            "{channel:?} should not change its name over transparency"
        );
        assert_eq!(
            texture_export_filename("winter", channel, false),
            texture_export_filename("winter", channel, true)
        );
    }

    for opaque in [false, true] {
        let name = texture_export_filename("winter", TextureChannel::Diffuse, opaque);
        assert_eq!(name.contains("_decal"), name.ends_with(".png"));
    }

    assert!(is_opaque(&[1, 2, 3, 255, 4, 5, 6, 255]));
    assert!(!is_opaque(&[1, 2, 3, 255, 4, 5, 6, 254]));
}

#[test]
fn the_user_names_the_texture_and_the_channel_adds_its_suffix() {
    let diffuse = texture_export_filename("winter", TextureChannel::Diffuse, true);
    let normal = texture_export_filename("winter", TextureChannel::Normal, true);
    assert!(diffuse.starts_with("winter"), "{diffuse}");
    assert!(normal.starts_with("winter"), "{normal}");
    assert_ne!(diffuse, normal, "the map type has to survive the name");
    assert!(diffuse.ends_with(".jpg"), "{diffuse}");
    assert!(normal.ends_with(".png"), "{normal}");

    assert_ne!(
        texture_export_filename("summer", TextureChannel::Diffuse, true),
        diffuse
    );
}

#[test]
fn the_linear_to_srgb_table_answers_exactly_what_the_curve_does() {
    const SAMPLES: u32 = 200_000;
    let mut worst = 0_i32;
    let mut worst_at = 0.0_f32;
    for sample in 0..=SAMPLES {
        let linear = sample as f32 / SAMPLES as f32;
        let difference =
            i32::from(linear_to_srgb(linear)) - i32::from(exact_linear_to_srgb(linear));
        if difference.abs() > worst.abs() {
            worst = difference;
            worst_at = linear;
        }
    }
    assert_eq!(
        worst, 0,
        "table and curve disagree by {worst} at linear {worst_at}"
    );

    assert_eq!(linear_to_srgb(0.0), 0);
    assert_eq!(linear_to_srgb(1.0), 255);

    for value in 0..=255_u8 {
        assert_eq!(linear_to_srgb(srgb_to_linear(value)), value, "byte {value}");
    }
}

#[test]
fn project_uses_an_accordion_and_stable_layer_selection() {
    let mut project = TextureProject::default();
    let first =
        project.add_image_layer(PathBuf::from("first.png"), TextureSourceMode::LandmarkPins);
    let second =
        project.add_image_layer(PathBuf::from("second.png"), TextureSourceMode::LandmarkPins);

    assert_eq!(project.selected_layer_id, Some(second));
    assert_eq!(
        project.selected_layer().unwrap().name,
        format!("Layer {second}")
    );
    assert!(project.remove_layer(second));
    assert_eq!(project.selected_layer_id, Some(first));
}

#[test]
fn texture_stroke_transaction_undoes_as_one_edit() {
    let mut project = TextureProject::default();
    let layer_id =
        project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    project.begin_undo_transaction();
    project.add_mask_dab(layer_id, [0.25, 0.35], Some([0.25, 0.65]), false);
    project.add_mask_dab(layer_id, [0.30, 0.40], Some([0.30, 0.60]), false);
    project.end_undo_transaction();
    assert!(project.selected_layer().unwrap().mask.is_some());

    assert!(project.undo());
    assert!(project.selected_layer().unwrap().mask.is_none());
    assert!(project.dirty);
}

#[test]
fn mask_preview_tracks_hidden_alpha_without_rebaking_the_source_image() {
    let mut project = TextureProject::default();
    let layer_id =
        project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    let raster_revision = project.selected_layer().unwrap().raster_revision;
    project.mask_brush_falloff = SculptFalloff::Sharp;
    project.mask_brush_opacity = 1.0;
    project.add_mask_dab(layer_id, [0.5, 0.5], Some([0.5, 0.5]), true);

    let layer = project.selected_layer().unwrap();
    let preview = layer.mask_preview.as_deref().unwrap();
    let center =
        ((preview.height as usize / 2) * preview.width as usize + preview.width as usize / 2) * 4;
    assert_eq!(&preview.rgba8[center..center + 3], &[255, 0, 0]);
    assert!(preview.rgba8[center + 3] > 100);
    assert!(preview.rgba8[center + 3] <= MASK_PREVIEW_MAX_ALPHA);
    assert!(layer.image.is_none());
    let mask = layer.mask.as_ref().unwrap();
    let mask_center = mask.height as usize / 2 * mask.width as usize + mask.width as usize / 2;
    assert!(mask.alpha8[mask_center] < 32);
    assert_eq!(layer.raster_revision, raster_revision);

    let hidden_alpha = preview.rgba8[center + 3];
    project.add_mask_dab(layer_id, [0.5, 0.5], Some([0.5, 0.5]), false);
    let restored = project
        .selected_layer()
        .unwrap()
        .mask_preview
        .as_deref()
        .unwrap();
    assert!(restored.rgba8[center + 3] < hidden_alpha);
}

#[test]
fn a_mask_painted_at_a_higher_resolution_still_takes_dabs_after_the_project_shrinks() {
    let mut project = TextureProject {
        resolution: 4096,
        ..Default::default()
    };
    let layer_id =
        project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    project.mask_brush_falloff = SculptFalloff::Sharp;
    project.mask_brush_opacity = 1.0;
    project.add_mask_dab(layer_id, [0.5, 0.9], None, true);
    project.end_undo_transaction();

    project.resolution = 2048;
    project.add_mask_dab(layer_id, [0.5, 0.5], None, true);

    let mask = project.layers[0]
        .mask
        .as_ref()
        .expect("the mask keeps the dimensions it was painted at");
    assert_eq!((mask.width, mask.height), (4096, 4096));
    let center = mask.height as usize / 2 * mask.width as usize + mask.width as usize / 2;
    assert!(
        mask.alpha8[center] < 32,
        "a dab below the shrunken ledger's reach must still move the mask, got alpha {}",
        mask.alpha8[center]
    );
}

#[test]
fn mask_changes_preserve_the_expensive_target_color_raster_cache() {
    let mut layer = TextureLayer::image(
        1,
        PathBuf::from("face.png"),
        TextureSourceMode::LandmarkPins,
    );
    let cached = CachedTextureLayerRaster {
        mirror: FaceMirror::Off,
        raster_revision: layer.raster_revision,
        resolution: 2048,
        boundary_feather_pixels: 16,
        image: Arc::new(SkinImage::solid(1, [20, 40, 60, 255])),
    };
    apply_layer_mask_dab(
        &mut layer,
        2048,
        TextureMaskDab {
            uv: [0.5, 0.5],
            radius: 0.05,
            falloff: SculptFalloff::Smooth,
            opacity: 1.0,
            add: false,
            source: Some([0.5, 0.5]),
        },
        &mut one_stroke(1, TextureTool::MaskBrush, 2048),
    );
    let masked = TextureLayerBakeInput::from(&layer);
    assert!(layer_raster_cache_matches(&masked, &cached, 2048, 16));

    layer.invalidate_raster();
    let changed_source = TextureLayerBakeInput::from(&layer);
    assert!(!layer_raster_cache_matches(
        &changed_source,
        &cached,
        2048,
        16
    ));
}

fn preview_raster(id: u64) -> CachedTextureLayerRaster {
    CachedTextureLayerRaster {
        mirror: FaceMirror::Off,
        raster_revision: 0,
        resolution: PREVIEW_BAKE_RESOLUTION,
        boundary_feather_pixels: 16,
        image: Arc::new(SkinImage::solid(id, [20, 40, 60, 255])),
    }
}

#[test]
fn a_new_head_keeps_the_image_raster_and_drops_the_scan_projection() {
    let mut project = TextureProject::default();
    let image = project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    let scan = project
        .ensure_scan_layer("scan".to_owned())
        .expect("a scan layer");
    project.absorb_layer_rasters(&BTreeMap::from([
        (image, preview_raster(image)),
        (scan, preview_raster(scan)),
    ]));

    project.forget_geometry_bound_rasters();
    let kept = project.cached_layer_rasters(PREVIEW_BAKE_RESOLUTION);
    assert!(
        kept.contains_key(&image),
        "an image layer is warped into G2 UV space, not onto the head that changed"
    );
    assert!(
        !kept.contains_key(&scan),
        "the scan projection is baked against the head, so it must go"
    );
}

#[test]
fn a_layer_hidden_for_one_bake_keeps_the_raster_it_already_paid_for() {
    let mut project = TextureProject::default();
    let kept = project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    let hidden =
        project.add_image_layer(PathBuf::from("brow.png"), TextureSourceMode::LandmarkPins);
    project.absorb_layer_rasters(&BTreeMap::from([
        (kept, preview_raster(kept)),
        (hidden, preview_raster(hidden)),
    ]));

    project.absorb_layer_rasters(&BTreeMap::from([(kept, preview_raster(kept))]));
    let cache = project.cached_layer_rasters(PREVIEW_BAKE_RESOLUTION);
    assert!(cache.contains_key(&hidden), "hiding a layer is not an edit");

    project.remove_layer(hidden);
    project.absorb_layer_rasters(&BTreeMap::from([(kept, preview_raster(kept))]));
    assert!(
        !project
            .cached_layer_rasters(PREVIEW_BAKE_RESOLUTION)
            .contains_key(&hidden),
        "a deleted layer must not hold its raster forever"
    );
}

#[test]
fn an_export_bake_leaves_the_preview_raster_the_viewport_edits_against() {
    let mut project = TextureProject::default();
    let layer = project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    project.absorb_layer_rasters(&BTreeMap::from([(layer, preview_raster(layer))]));

    let mut export = preview_raster(layer);
    export.resolution = project.resolution;
    project.absorb_layer_rasters(&BTreeMap::from([(layer, export)]));

    assert!(
        project
            .cached_layer_rasters(PREVIEW_BAKE_RESOLUTION)
            .contains_key(&layer),
        "a full-resolution export must not evict the preview raster"
    );
    assert!(
        project
            .cached_layer_rasters(project.resolution)
            .contains_key(&layer),
        "a second export in a row should still find its raster"
    );

    project.absorb_layer_rasters(&BTreeMap::from([(layer, preview_raster(layer))]));
    assert!(
        project.cached_layer_rasters(project.resolution).is_empty(),
        "the export raster is dropped once the viewport bakes again"
    );
}

#[test]
fn pin_sides_fill_the_first_incomplete_pair() {
    let mut project = TextureProject::default();
    project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    project.add_source_pin([0.2, 0.3]);
    project.add_target_pin(TextureTargetPin {
        triangle_index: 7,
        barycentric: [0.2, 0.3, 0.5],
        uv: [0.4, 0.6],
    });
    let pins = &project.selected_layer().unwrap().pins;
    assert_eq!(pins.len(), 1);
    assert!(pins[0].source.is_some() && pins[0].target.is_some());
    assert!(!project.selected_layer().unwrap().pin_pair_invalid(0));
}

fn skin_field(rgb: [u8; 3], intruder: Option<[u8; 3]>) -> SkinImage {
    let (width, height) = (32_u32, 32_u32);
    let mut rgba8 = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..height {
        for x in 0..width {
            let colour = match intruder {
                Some(other) if x < width / 2 => other,
                _ => rgb,
            };
            rgba8.extend_from_slice(&[colour[0], colour[1], colour[2], 255]);
        }
    }
    SkinImage::new(1, width, height, rgba8).expect("valid test image")
}

#[test]
fn tone_matching_lands_the_source_on_the_target() {
    let source = skin_field([150, 112, 96], None);
    let target = skin_field([196, 150, 124], None);
    let solved = solve_tone_match(&source, &target, true).expect("both fields are skin");

    let mut matched = source.rgba8.as_ref().clone();
    vkit_core::texture_bake::apply_color_adjustments(
        &mut matched,
        TextureColorAdjustments {
            exposure: solved.exposure,
            saturation: solved.saturation,
            temperature: solved.temperature,
            ..TextureColorAdjustments::default()
        },
    );
    for (channel, (&got, &want)) in matched.iter().zip(target.rgba8.iter()).take(3).enumerate() {
        let (got, want) = (i32::from(got), i32::from(want));
        assert!(
            (got - want).abs() <= 6,
            "channel {channel} landed on {got}, wanted {want}"
        );
    }
}

#[test]
fn tone_matching_ignores_what_is_not_skin() {
    let clean = solve_tone_match(
        &skin_field([150, 112, 96], None),
        &skin_field([196, 150, 124], None),
        true,
    )
    .expect("skin present");
    let littered = solve_tone_match(
        &skin_field([150, 112, 96], Some([20, 40, 200])),
        &skin_field([196, 150, 124], None),
        true,
    )
    .expect("skin still present in the other half");
    assert!((clean.exposure - littered.exposure).abs() < 0.05);
    assert!((clean.saturation - littered.saturation).abs() < 0.05);
    assert!((clean.temperature - littered.temperature).abs() < 0.05);
}

fn scan_atlas(project: &TextureProject, revision: u64) -> BTreeMap<u64, CachedTextureLayerRaster> {
    let scan = project
        .layers
        .iter()
        .find(|layer| layer.source_mode == TextureSourceMode::ScanMesh)
        .expect("a scan layer to hand the atlas to");
    BTreeMap::from([(
        scan.id,
        CachedTextureLayerRaster {
            mirror: FaceMirror::Off,
            raster_revision: scan.raster_revision,
            resolution: 2,
            boundary_feather_pixels: 0,
            image: Arc::new(SkinImage::new(revision, 2, 2, vec![7; 16]).unwrap()),
        },
    )])
}

fn scan_projection(project: &TextureProject, revision: u64) -> BTreeMap<u64, Arc<SkinImage>> {
    let scan = project
        .layers
        .iter()
        .find(|layer| layer.source_mode == TextureSourceMode::ScanMesh)
        .expect("a scan layer to hand the atlas to");
    BTreeMap::from([(
        scan.id,
        Arc::new(SkinImage::new(revision, 2, 2, vec![7; 16]).unwrap()),
    )])
}

#[test]
fn the_scan_layer_adopts_the_projection_rather_than_its_mirrored_copy() {
    let mut project = TextureProject::default();
    project.ensure_scan_layer("Scan".to_owned()).unwrap();

    project.adopt_scan_atlases(&scan_atlas(&project, 11), &scan_projection(&project, 41));

    assert_eq!(
        project.layers[0].image.as_ref().map(|image| image.revision),
        Some(41),
        "the 2D view edits the projection; the bake mirrors on top of it"
    );
}

#[test]
fn the_scan_layer_goes_under_whatever_is_already_there() {
    let mut project = TextureProject::default();
    let painted = project.add_image_layer(
        PathBuf::from("freckles.png"),
        TextureSourceMode::LandmarkPins,
    );
    let scan = project.ensure_scan_layer("Scan".to_owned()).unwrap();
    assert_eq!(
        project
            .layers
            .iter()
            .map(|layer| layer.id)
            .collect::<Vec<_>>(),
        vec![scan, painted]
    );

    assert_eq!(project.ensure_scan_layer("Scan".to_owned()), Some(scan));
    assert_eq!(project.layers.len(), 2);
}

#[test]
fn a_bake_hands_the_projected_atlas_back_to_the_scan_layer() {
    let mut project = TextureProject::default();
    project.ensure_scan_layer("Scan".to_owned()).unwrap();
    assert!(project.layers[0].image.is_none());

    project.adopt_scan_atlases(&scan_atlas(&project, 11), &BTreeMap::new());
    assert_eq!(
        project.layers[0].image.as_ref().map(|image| image.revision),
        Some(11),
        "the 2D view has nothing to show without this"
    );
}

#[test]
fn moving_the_scan_drops_the_atlas_it_was_projected_from() {
    let mut project = TextureProject::default();
    project.ensure_scan_layer("Scan".to_owned()).unwrap();
    project.adopt_scan_atlases(&scan_atlas(&project, 11), &BTreeMap::new());
    let before = project.layers[0].raster_revision;

    project.invalidate_scan_projection();
    assert!(
        project.layers[0].image.is_none(),
        "a stale atlas is a picture of the previous placement"
    );
    assert!(project.layers[0].raster_revision > before);

    project.adopt_scan_atlases(&scan_atlas(&project, 12), &BTreeMap::new());
    assert_eq!(
        project.layers[0].image.as_ref().map(|image| image.revision),
        Some(12)
    );
}

#[test]
fn an_atlas_projected_before_the_scan_moved_is_never_adopted_after_it() {
    let mut project = TextureProject::default();
    project.ensure_scan_layer("Scan".to_owned()).unwrap();
    let stale_rasters = scan_atlas(&project, 11);
    let stale_projection = scan_projection(&project, 41);

    project.invalidate_scan_projection();
    project.adopt_scan_atlases(&stale_rasters, &stale_projection);
    assert!(
        project.layers[0].image.is_none(),
        "a bake in flight across the move pictures the previous placement; once adopted, the \
         rebake's correct atlas finds the slot taken and can never replace it"
    );

    project.adopt_scan_atlases(&scan_atlas(&project, 12), &BTreeMap::new());
    assert_eq!(
        project.layers[0].image.as_ref().map(|image| image.revision),
        Some(12),
        "the rebake that pictured the new placement still lands"
    );
}

#[test]
fn a_retouched_scan_atlas_is_baked_back_instead_of_being_re_projected() {
    let mut project = TextureProject::default();
    project.ensure_scan_layer("Scan".to_owned()).unwrap();
    project.layers[0].image = Some(Arc::new(
        SkinImage::new(1, 2, 2, [10, 20, 30, 255].repeat(4)).unwrap(),
    ));
    project.layers[0].edited_image = Some(Arc::new(
        SkinImage::new(2, 2, 2, [200, 100, 50, 255].repeat(4)).unwrap(),
    ));
    let request = TextureBakeRequest {
        request_id: 1,
        project_revision: 0,
        layers: Vec::new(),
        target: Arc::new(OrderedObjMesh {
            vertices: Vec::new(),
            faces: Vec::new(),
        }),
        mapping: Arc::new(G2UvMapping {
            source_path: PathBuf::new(),
            coordinate_rms_cm: 0.0,
            coordinate_max_cm: 0.0,
            uncovered_triangles: 0,
            faces: Vec::new(),
            triangles: Vec::new(),
        }),
        face_mirror: None,
        scan: None,
        base_preview: None,
        bake_base: TextureBakeBase::Transparent,
        hide_skin_preview: false,
        neutral_base_rgb: [0, 0, 0],
        resolution: 2,
        boundary_feather_pixels: 0,
        cached_layer_rasters: BTreeMap::new(),
        cached_base_face: None,
        base_face_source: None,
        base_surface_sources: BTreeMap::new(),
    };
    let options = TextureBakeOptions {
        width: 2,
        height: 2,
        boundary_feather_pixels: 0,
    };

    let input = TextureLayerBakeInput::from(&project.layers[0]);
    assert!(input.retouched);
    let baked = rasterize_scan_layer(&request, &input, options)
        .expect("the retouched atlas needs no scan source");
    assert_eq!(
        baked.rgba8,
        [200, 100, 50, 255].repeat(4),
        "the strokes live only in the edited copy, so the bake must read that copy"
    );

    let upsized = rasterize_scan_layer(
        &request,
        &input,
        TextureBakeOptions {
            width: 4,
            height: 4,
            boundary_feather_pixels: 0,
        },
    )
    .expect("another resolution resizes the atlas");
    assert_eq!(
        upsized.rgba8,
        [200, 100, 50, 255].repeat(16),
        "an export at another resolution resizes the strokes rather than dropping them"
    );

    project.layers[0].edited_image = None;
    let pristine = TextureLayerBakeInput::from(&project.layers[0]);
    assert!(!pristine.retouched);
    assert!(
        rasterize_scan_layer(&request, &pristine, options).is_err(),
        "an unedited scan layer still re-projects at the bake's own resolution, and this \
         harness offers no scan to project from"
    );
}

#[test]
fn the_size_the_viewport_bakes_at_is_one_the_baker_accepts() {
    assert!(
        is_bakeable_resolution(PREVIEW_BAKE_RESOLUTION),
        "the viewport asks for {PREVIEW_BAKE_RESOLUTION} on every edit; refusing it leaves the \
         face unpainted"
    );
    for edge in TEXTURE_RESOLUTIONS {
        assert!(is_bakeable_resolution(edge), "{edge}");
    }
    assert!(!is_bakeable_resolution(1023));
    assert!(
        !is_texture_resolution(PREVIEW_BAKE_RESOLUTION),
        "the preview size stays out of the export menu"
    );
}

#[test]
fn a_refused_bake_is_not_asked_for_again_until_the_project_changes() {
    let mut project = TextureProject::default();
    project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    project.finish_bake(Err("refused".to_owned()), true);
    assert!(project.bake_refused_current_edit());

    project.add_image_layer(PathBuf::from("other.png"), TextureSourceMode::LandmarkPins);
    assert!(
        !project.bake_refused_current_edit(),
        "an edit is a new question, and deserves a fresh answer"
    );
}

#[test]
fn resetting_a_layer_returns_it_to_its_freshly_loaded_state() {
    let mut project = TextureProject::default();
    let id = project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    project.add_source_pin([0.2, 0.3]);
    project.add_target_pin(TextureTargetPin {
        triangle_index: 7,
        barycentric: [0.2, 0.3, 0.5],
        uv: [0.4, 0.6],
    });
    {
        let layer = project.selected_layer_mut().unwrap();
        layer.opacity = 0.25;
        layer.blend_mode = TextureBlendMode::Multiply;
        layer.adjustments.exposure = 1.5;
    }

    project.reset_layer(id);

    let layer = project.selected_layer().unwrap();
    assert!(
        layer.pins.is_empty(),
        "the projection and its fit are cleared"
    );
    assert!(layer.mask.is_none() && layer.edited_image.is_none());
    assert!(
        layer.painted.is_none(),
        "the paint atlas is part of a reset"
    );
    assert_eq!(layer.adjustments, TextureColorAdjustments::default());
    assert_eq!(layer.opacity, 1.0);
    assert_eq!(layer.blend_mode, TextureBlendMode::Normal);

    assert!(layer.source_path.is_some());
}

#[test]
fn broadcasting_pins_copies_them_to_every_other_layer() {
    let mut project = TextureProject::default();
    let first =
        project.add_image_layer(PathBuf::from("first.png"), TextureSourceMode::LandmarkPins);
    let second =
        project.add_image_layer(PathBuf::from("second.png"), TextureSourceMode::LandmarkPins);
    let third =
        project.add_image_layer(PathBuf::from("third.png"), TextureSourceMode::LandmarkPins);

    project.selected_layer_id = Some(first);
    project.add_source_pin([0.2, 0.3]);
    project.add_target_pin(TextureTargetPin {
        triangle_index: 7,
        barycentric: [0.2, 0.3, 0.5],
        uv: [0.4, 0.6],
    });
    let copied = project.broadcast_pins_from_selected();
    assert_eq!(copied, 2, "every other layer receives the pins");

    let pins_of = |id: u64| {
        project
            .layers
            .iter()
            .find(|layer| layer.id == id)
            .unwrap()
            .pins
            .clone()
    };
    assert_eq!(pins_of(first).len(), 1, "the source layer is unchanged");
    let received = pins_of(second);
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].source, Some([0.2, 0.3]));
    assert!(received[0].target.is_some());
    assert_eq!(pins_of(third).len(), 1, "every other layer receives them");
}

#[test]
fn incomplete_and_duplicate_texture_pin_pairs_are_invalid() {
    let mut layer = TextureLayer::image(
        1,
        PathBuf::from("face.png"),
        TextureSourceMode::LandmarkPins,
    );
    layer.pins = vec![
        TexturePinPair {
            source: Some([0.2, 0.3]),
            target: Some(TextureTargetPin {
                triangle_index: 7,
                barycentric: [0.2, 0.3, 0.5],
                uv: [0.4, 0.6],
            }),
        },
        TexturePinPair {
            source: Some([0.8, 0.3]),
            target: None,
        },
    ];
    assert!(!layer.pin_pair_invalid(0));
    assert!(layer.pin_pair_invalid(1));
    assert!(!layer.landmark_warp_ready());
    layer.pins[1].target = layer.pins[0].target;
    assert!(layer.pin_pair_invalid(0));
    assert!(layer.pin_pair_invalid(1));
    assert!(!layer.landmark_warp_ready());
}

#[test]
fn landmark_warp_requires_three_non_collinear_complete_pairs() {
    let mut layer = TextureLayer::image(
        1,
        PathBuf::from("face.png"),
        TextureSourceMode::LandmarkPins,
    );
    layer.pins = [[0.2, 0.2], [0.5, 0.5], [0.8, 0.8]]
        .into_iter()
        .enumerate()
        .map(|(index, uv)| TexturePinPair {
            source: Some(uv),
            target: Some(TextureTargetPin {
                triangle_index: index as u32,
                barycentric: [1.0, 0.0, 0.0],
                uv,
            }),
        })
        .collect();
    assert!(!layer.landmark_warp_ready());
    layer.pins[2].target.as_mut().unwrap().uv = [0.8, 0.3];
    layer.pins[2].source = Some([0.8, 0.3]);
    assert!(layer.landmark_warp_ready());
}

pub(crate) fn ready_pins() -> Vec<TexturePinPair> {
    [[0.2, 0.2], [0.5, 0.5], [0.8, 0.3]]
        .into_iter()
        .enumerate()
        .map(|(index, uv)| TexturePinPair {
            source: Some(uv),
            target: Some(TextureTargetPin {
                triangle_index: index as u32,
                barycentric: [1.0, 0.0, 0.0],
                uv,
            }),
        })
        .collect()
}

#[test]
fn a_landmark_layer_locks_the_brushes_until_its_pins_make_a_warp() {
    let mut project = TextureProject::default();
    project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);

    for tool in TextureTool::ALL {
        assert_eq!(
            project.tool_usable(tool),
            !tool.needs_warp(),
            "{tool:?} before the pins"
        );
    }
    assert_eq!(
        project.usable_tools(),
        vec![TextureTool::Projection, TextureTool::PinPair]
    );

    project.selected_layer_mut().unwrap().pins = ready_pins();
    for tool in TextureTool::ALL {
        assert!(project.tool_usable(tool), "{tool:?} after the pins");
    }
}

#[test]
fn adding_an_image_takes_the_reader_off_a_brush_that_cannot_reach_the_face() {
    let mut project = TextureProject::default();
    project.set_active_tool(TextureTool::MaskBrush);

    project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    assert_ne!(project.active_tool, TextureTool::MaskBrush);
    assert!(project.tool_usable(project.active_tool));

    project.set_active_tool(TextureTool::MaskBrush);
    assert_ne!(project.active_tool, TextureTool::MaskBrush);

    project.selected_layer_mut().unwrap().pins = ready_pins();
    project.set_active_tool(TextureTool::MaskBrush);
    assert_eq!(project.active_tool, TextureTool::MaskBrush);
}

#[test]
fn the_undo_history_is_bounded_by_memory_rather_than_by_a_step_count() {
    let mut project = TextureProject::default();
    let heavy = |bytes: usize| TextureUndoSnapshot {
        layers: vec![{
            let mut layer =
                TextureLayer::image(1, PathBuf::from("f.png"), TextureSourceMode::LandmarkPins);
            layer.mask = Some(TextureLayerMask {
                revision: 1,
                width: 1,
                height: 1,
                alpha8: std::sync::Arc::new(vec![0; bytes]),
            });
            layer
        }],
        selected_layer_id: None,
        resolution: 2048,
        boundary_feather_pixels: 0,
        bake_base: TextureBakeBase::default(),
        source_revision: 0,
    };

    for _ in 0..64 {
        project.push_undo(heavy(1024));
    }
    assert_eq!(project.history_position().0, 64);

    project.push_undo(heavy(TEXTURE_UNDO_BYTES + 1));
    assert_eq!(project.history_position().0, 1);
}

#[test]
fn the_history_walks_both_ways_and_a_new_edit_closes_the_way_forward() {
    let mut project = TextureProject::default();
    let steps = || TextureUndoSnapshot {
        layers: Vec::new(),
        selected_layer_id: None,
        resolution: 2048,
        boundary_feather_pixels: 0,
        bake_base: TextureBakeBase::default(),
        source_revision: 0,
    };
    for _ in 0..3 {
        project.push_undo(steps());
    }
    assert_eq!(project.history_position(), (3, 0));

    assert!(project.undo() && project.undo());
    assert_eq!(project.history_position(), (1, 2));
    assert_ne!(project.history_position().1, 0);

    assert!(project.redo());
    assert_eq!(project.history_position(), (2, 1));

    project.push_undo(steps());
    assert_eq!(project.history_position(), (3, 0));
    assert_eq!(project.history_position().1, 0);
}

#[test]
fn layer_drop_uses_visual_insertion_indices() {
    let mut project = TextureProject::default();
    let first =
        project.add_image_layer(PathBuf::from("first.png"), TextureSourceMode::LandmarkPins);
    let second =
        project.add_image_layer(PathBuf::from("second.png"), TextureSourceMode::LandmarkPins);
    let third =
        project.add_image_layer(PathBuf::from("third.png"), TextureSourceMode::LandmarkPins);
    assert_eq!(
        project
            .layers
            .iter()
            .map(|layer| layer.id)
            .collect::<Vec<_>>(),
        [third, second, first]
    );

    project.move_layer_to(first, 0);
    assert_eq!(
        project
            .layers
            .iter()
            .map(|layer| layer.id)
            .collect::<Vec<_>>(),
        [first, third, second]
    );
    project.move_layer_to(first, 3);
    assert_eq!(
        project
            .layers
            .iter()
            .map(|layer| layer.id)
            .collect::<Vec<_>>(),
        [third, second, first]
    );
}

#[test]
fn baked_preview_composites_transparent_diffuse_over_the_skin() {
    let mapping = G2UvMapping {
        source_path: PathBuf::new(),
        coordinate_rms_cm: 0.0,
        coordinate_max_cm: 0.0,
        uncovered_triangles: 0,
        faces: Vec::new(),
        triangles: vec![vkit_core::vam::G2UvTriangle {
            canonical_face_index: 0,
            canonical_triangle_index: 0,
            material_region: UvMaterialRegion::Face,
            on_head: true,
            position_indices: [0, 1, 2],
            uvs: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        }],
    };
    let mut images = BTreeMap::new();
    images.insert(
        TextureChannel::Diffuse,
        Arc::new(SkinImage::new(2, 1, 1, vec![255, 0, 0, 128]).unwrap()),
    );

    let neutral = [0xe8, 0xb2, 0x78];
    let preview = build_baked_preview(3, &mapping, None, None, neutral, &images).unwrap();
    assert_eq!(preview.face.rgba8[3], 255);
    assert!(preview.face.rgba8[0] > u32::from(neutral[0]) as u8);
    assert!(preview.face.rgba8[1] < neutral[1]);
    assert_ne!(preview.face.rgba8.as_slice(), &[255, 0, 0, 128]);

    let dark = build_baked_preview(4, &mapping, None, None, [20, 30, 40], &images).unwrap();
    assert!(
        dark.face.rgba8[2] < preview.face.rgba8[2],
        "the neutral base ignored the solid colour"
    );
}

#[test]
fn source_view_rejects_invalid_values_and_clamps_navigation_state() {
    let mut project = TextureProject::default();
    let layer = project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
    project.set_source_view(layer, 8.0, [0.2, 0.8]);
    let selected = project.selected_layer().unwrap();
    assert_eq!(selected.source_view_zoom, 8.0);
    assert_eq!(selected.source_view_center, [0.2, 0.8]);

    project.set_source_view(layer, f32::NAN, [0.5, 0.5]);
    let selected = project.selected_layer().unwrap();
    assert_eq!(selected.source_view_zoom, 8.0);
    assert_eq!(selected.source_view_center, [0.2, 0.8]);

    project.set_source_view(layer, 99.0, [-2.0, 4.0]);
    let selected = project.selected_layer().unwrap();
    assert_eq!(selected.source_view_zoom, 32.0);
    assert_eq!(selected.source_view_center, [0.0, 1.0]);
}

#[test]
fn textures_go_where_vam_keeps_them() {
    let root = PathBuf::from("V:/VaM");
    let project = TextureProject::default();
    let female = project
        .default_export_directory(Some(&root), FigureSex::Female)
        .expect("a root gives a directory");
    assert!(
        female.ends_with("Custom/Atom/Person/Textures/FemaleBase")
            || female.ends_with(r"Custom\Atom\Person\Textures\FemaleBase"),
        "{}",
        female.display()
    );
    assert!(
        !female.components().any(|part| part.as_os_str() == "Female"),
        "no bare `Female` level: {}",
        female.display()
    );
    let male = project
        .default_export_directory(Some(&root), FigureSex::Male)
        .expect("a root gives a directory");
    assert!(
        male.ends_with("MaleBase") || male.ends_with(r"MaleBase"),
        "{}",
        male.display()
    );

    let named = TextureProject {
        export_subfolder: "MyLook".to_owned(),
        ..TextureProject::default()
    };
    let custom = named
        .default_export_directory(Some(&root), FigureSex::Female)
        .expect("a root gives a directory");
    assert!(custom.ends_with("MyLook"), "{}", custom.display());
    assert_eq!(custom.parent(), female.parent());
}

#[test]
fn export_names_are_safe_and_channel_specific() {
    assert_eq!(
        texture_export_filename("My:Face", TextureChannel::Diffuse, true),
        "My_Face_diffuse.jpg"
    );

    assert_eq!(
        texture_export_filename("", TextureChannel::Normal, true),
        "texture_normal.png"
    );
}

#[test]
fn layer_alpha_mask_subtracts_and_alt_reverse_restores_visibility() {
    let mut layer = TextureLayer::image(
        1,
        PathBuf::from("face.png"),
        TextureSourceMode::LandmarkPins,
    );
    assert!(layer.mask_stroke_subtracts(false));
    assert!(!layer.mask_stroke_subtracts(true));
    layer.mask_base = 0;
    assert!(!layer.mask_stroke_subtracts(false));
    assert!(layer.mask_stroke_subtracts(true));

    layer.mask_base = 255;
    let center_uv = [1024.0 / 2047.0, 1.0 - 1024.0 / 2047.0];
    let subtract = TextureMaskDab {
        uv: center_uv,
        radius: 0.25,
        falloff: SculptFalloff::Sharp,
        opacity: 1.0,
        add: false,
        source: None,
    };

    apply_layer_mask_dab(
        &mut layer,
        2048,
        subtract,
        &mut one_stroke(1, TextureTool::MaskBrush, 2048),
    );
    let center = 1024 * 2048 + 1024;
    assert_eq!(layer.mask.as_ref().unwrap().alpha8[center], 0);

    apply_layer_mask_dab(
        &mut layer,
        2048,
        TextureMaskDab {
            add: true,
            ..subtract
        },
        &mut one_stroke(1, TextureTool::MaskBrush, 2048),
    );
    assert_eq!(layer.mask.as_ref().unwrap().alpha8[center], 255);
}

#[test]
fn a_stroke_does_not_compound_against_itself() {
    let size = RasterSize {
        width: 16,
        height: 16,
    };
    let dab = BrushDab {
        radius: 0.25,
        falloff: SculptFalloff::Smooth,
        opacity: 0.5,
    };
    let stroke = RetouchStroke {
        tool: TextureTool::Sponge,
        point: [0.5, 0.5],
        clone_offset: None,
        reverse: true,
    };
    let mut once = [200, 120, 60, 255].repeat(256);
    let mut coverage = one_stroke(1, TextureTool::Sponge, size);
    apply_retouch_pixels(&mut once, size, stroke, dab, &mut coverage);
    let mut repeated = [200, 120, 60, 255].repeat(256);
    let mut coverage = one_stroke(1, TextureTool::Sponge, size);
    for _ in 0..8 {
        apply_retouch_pixels(&mut repeated, size, stroke, dab, &mut coverage);
    }
    assert_eq!(once, repeated);

    let mut coverage = one_stroke(1, TextureTool::Sponge, size);
    apply_retouch_pixels(&mut repeated, size, stroke, dab, &mut coverage);
    let centre = (8 * 16 + 8) * 4;
    assert!(repeated[centre] < once[centre], "a second stroke goes on");
}

#[test]
fn a_clone_stroke_carries_its_source_along_with_it() {
    let size = RasterSize {
        width: 32,
        height: 8,
    };

    let mut rgba8 = vec![0_u8; 32 * 8 * 4];
    for y in 0..8 {
        for x in 0..16 {
            let offset = (y * 32 + x) * 4;
            let value = if x == 4 { 255 } else { 40 };
            rgba8[offset..offset + 4].copy_from_slice(&[value, value, value, 255]);
        }
    }
    let dab = BrushDab {
        radius: 0.12,
        falloff: SculptFalloff::Linear,
        opacity: 1.0,
    };

    let offset = [16.0 / 31.0, 0.0];
    let mut coverage = one_stroke(1, TextureTool::CloneStamp, size);
    for column in [20_usize, 21, 22] {
        apply_retouch_pixels(
            &mut rgba8,
            size,
            RetouchStroke {
                tool: TextureTool::CloneStamp,
                point: [column as f32 / 31.0, 0.5],
                clone_offset: Some(offset),
                reverse: false,
            },
            dab,
            &mut coverage,
        );
    }
    let row = 4 * 32;

    assert!(rgba8[(row + 20) * 4] > 200, "the bright column was copied");
    assert!(rgba8[(row + 21) * 4] < 120, "and it did not smear along");
    assert!(rgba8[(row + 22) * 4] < 120);
}

#[test]
fn a_clone_never_reads_colour_that_carries_no_alpha() {
    let size = RasterSize {
        width: 16,
        height: 16,
    };
    let mut rgba8 = vec![0_u8; 16 * 16 * 4];

    for y in 0..16 {
        for x in 8..16 {
            let offset = (y * 16 + x) * 4;
            rgba8[offset..offset + 4].copy_from_slice(&[128, 128, 128, 255]);
        }
    }
    let before = rgba8.clone();

    let mut coverage = one_stroke(1, TextureTool::CloneStamp, size);
    apply_retouch_pixels(
        &mut rgba8,
        size,
        RetouchStroke {
            tool: TextureTool::CloneStamp,
            point: [12.0 / 15.0, 0.5],
            clone_offset: Some([8.0 / 15.0, 0.0]),
            reverse: false,
        },
        BrushDab {
            radius: 0.2,
            falloff: SculptFalloff::Linear,
            opacity: 1.0,
        },
        &mut coverage,
    );
    assert_eq!(rgba8, before, "a transparent source contributes nothing");
}

#[test]
fn retouch_dodge_changes_only_the_working_pixels() {
    let mut rgba8 = [80, 90, 100, 255].repeat(64);
    apply_retouch_pixels(
        &mut rgba8,
        RasterSize {
            width: 8,
            height: 8,
        },
        RetouchStroke {
            tool: TextureTool::DodgeBurn,
            point: [0.5, 0.5],
            clone_offset: None,
            reverse: false,
        },
        BrushDab {
            radius: 0.25,
            falloff: SculptFalloff::Sharp,
            opacity: 1.0,
        },
        &mut one_stroke(
            1,
            TextureTool::DodgeBurn,
            RasterSize {
                width: 8,
                height: 8,
            },
        ),
    );
    let center = (4 * 8 + 4) * 4;
    assert!(rgba8[center] > 80);
    assert_eq!(rgba8[3], 255);
}
