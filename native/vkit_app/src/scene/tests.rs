use super::*;
use serde_json::json;
use vkit_core::formats::ObjFace;

fn plane_mesh() -> SurfaceMesh {
    SurfaceMesh::new(
        Mesh::new(
            vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            vec![[0, 1, 2], [0, 2, 3]],
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn bvh_picks_nearest_triangle_with_stable_barycentrics() {
    let mesh = plane_mesh();
    let ray = Ray3::from_dvec(DVec3::new(0.75, 0.0, 2.0), -DVec3::Z).unwrap();
    let first = mesh
        .pick_visible_surface(ray, ModelTransform::default())
        .unwrap();
    let second = mesh
        .pick_visible_surface(ray, ModelTransform::default())
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.triangle, 0);
    assert!((first.barycentric.iter().sum::<f64>() - 1.0).abs() < 1.0e-12);
    assert!(first.local_point.z.abs() < 1.0e-12);
}

#[test]
fn world_transform_is_inverted_for_picking() {
    let mesh = plane_mesh();
    let transform = ModelTransform::new(2.0, [3.0, -1.0, 5.0]);
    let ray = Ray3::from_dvec(DVec3::new(3.0, -1.0, 10.0), -DVec3::Z).unwrap();
    let hit = mesh.pick_visible_surface(ray, transform).unwrap();
    let world = transform.point_to_world(hit.local_point);
    assert!((world - DVec3::new(3.0, -1.0, 5.0)).length() < 1.0e-10);
}

#[test]
fn rotated_transform_keeps_a_nonzero_facial_pivot_fixed_and_roundtrips_rays() {
    let transform = ModelTransform::from_components_with_pivot(
        1.75,
        [3.0, -4.0, 5.0],
        [17.0, -31.0, 83.0],
        [10.0, 20.0, 30.0],
    );
    let pivot = DVec3::new(10.0, 20.0, 30.0);
    assert!(
        transform
            .point_to_world(pivot)
            .abs_diff_eq(pivot + transform.translation, 1.0e-12)
    );

    let local = DVec3::new(13.0, 18.0, 34.0);
    let world = transform.point_to_world(local);
    assert!(transform.point_to_local(world).abs_diff_eq(local, 1.0e-12));
    let matrix_world = transform.matrix().transform_point3(local.as_vec3());
    assert!(matrix_world.abs_diff_eq(world.as_vec3(), 2.0e-5));

    let local_ray_origin = DVec3::new(11.0, 22.0, 40.0);
    let local_ray_target = DVec3::new(9.0, 19.0, 28.0);
    let world_ray = Ray3::from_dvec(
        transform.point_to_world(local_ray_origin),
        transform.point_to_world(local_ray_target) - transform.point_to_world(local_ray_origin),
    )
    .unwrap();
    let roundtrip = transform.ray_to_local(world_ray).unwrap();
    assert!(roundtrip.origin.abs_diff_eq(local_ray_origin, 1.0e-12));
    assert!(
        roundtrip
            .direction
            .dot((local_ray_target - local_ray_origin).normalize())
            > 0.999_999
    );
}

#[test]
fn nonuniform_transform_roundtrips_points_matrices_and_rays_about_a_pivot() {
    let transform = ModelTransform::from_components_xyz_with_pivot(
        [2.25, 0.6, 1.4],
        [-3.0, 4.5, 2.0],
        [27.0, -41.0, 73.0],
        [10.0, 20.0, 30.0],
    );
    let pivot = DVec3::new(10.0, 20.0, 30.0);
    assert!(
        transform
            .point_to_world(pivot)
            .abs_diff_eq(pivot + transform.translation, 1.0e-12)
    );

    let local = DVec3::new(13.0, 18.0, 34.0);
    let world = transform.point_to_world(local);
    assert!(transform.point_to_local(world).abs_diff_eq(local, 1.0e-12));
    assert!(
        transform
            .matrix()
            .transform_point3(local.as_vec3())
            .abs_diff_eq(world.as_vec3(), 3.0e-5)
    );

    let local_ray_origin = DVec3::new(11.0, 22.0, 40.0);
    let local_ray_target = DVec3::new(9.0, 19.0, 28.0);
    let world_ray = Ray3::from_dvec(
        transform.point_to_world(local_ray_origin),
        transform.point_to_world(local_ray_target) - transform.point_to_world(local_ray_origin),
    )
    .unwrap();
    let local_ray = transform.ray_to_local(world_ray).unwrap();
    assert!(local_ray.origin.abs_diff_eq(local_ray_origin, 1.0e-12));
    assert!(
        local_ray
            .direction
            .dot((local_ray_target - local_ray_origin).normalize())
            > 0.999_999
    );
}

#[test]
fn nonuniform_constructor_sanitizes_each_invalid_axis_independently() {
    let transform = ModelTransform::from_components_xyz([2.0, f64::NAN, -4.0], [0.0; 3], [0.0; 3]);
    assert_eq!(transform.scale_xyz, DVec3::new(2.0, 1.0, 1.0));
}

#[test]
fn pin_pairs_keep_numeric_slots_when_one_side_is_deleted() {
    let endpoint = SurfaceEndpoint {
        triangle: 0,
        barycentric: [1.0, 0.0, 0.0],
    };
    let mut pins = PinSet::default();
    assert_eq!(pins.add(MeshSide::Scan, endpoint), 0);
    assert_eq!(pins.add(MeshSide::Scan, endpoint), 1);
    assert_eq!(pins.add(MeshSide::Template, endpoint), 0);
    assert_eq!(pins.complete_count(), 1);
    assert_eq!(pins.mismatch_start(), Some(1));
    assert!(pins.delete(MeshSide::Template, 0));
    assert_eq!(pins.pairs().len(), 2);
    assert_eq!(pins.mismatch_start(), Some(0));
    assert_eq!(pins.add(MeshSide::Template, endpoint), 0);
    assert_eq!(pins.complete_count(), 1);
}

#[test]
fn a_drag_is_one_undo_step() {
    let start = SurfaceEndpoint {
        triangle: 0,
        barycentric: [1.0, 0.0, 0.0],
    };
    let moved = SurfaceEndpoint {
        triangle: 1,
        barycentric: [0.0, 1.0, 0.0],
    };
    let mut pins = PinSet::default();
    pins.add(MeshSide::Scan, start);
    assert!(pins.begin_drag(MeshSide::Scan, 0));
    assert!(pins.move_without_history(MeshSide::Scan, 0, moved));
    assert!(pins.move_without_history(MeshSide::Scan, 0, moved));
    assert!(pins.undo());
    assert_eq!(pins.pairs()[0].scan, Some(start));
}

#[test]
fn normals_and_wire_edges_are_deterministic() {
    let mesh = plane_mesh();
    assert_eq!(mesh.normals.len(), 4);
    assert_eq!(mesh.wire_indices.len(), 12);
    assert_eq!(mesh.normals[0], [0.0, 0.0, 1.0]);
}

#[test]
fn result_preview_reuses_static_render_topology_without_stale_picking() {
    let base = OrderedObjMesh {
        vertices: vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]],
        faces: vec![ObjFace {
            vertex_indices: vec![0, 1, 2],
            group: Some("Face".into()),
            material: Some("Face".into()),
        }],
    };
    let mut workspace = WorkspaceScene::default();
    workspace.install_result(Arc::new(base.clone())).unwrap();
    let before = Arc::clone(workspace.result.as_ref().unwrap());
    let mut deformed = base.clone();
    deformed.vertices[2][2] = 0.25;
    workspace.update_result_preview(deformed.clone()).unwrap();
    let after = workspace.result.as_ref().unwrap();

    assert!(Arc::ptr_eq(&before.wire_indices, &after.wire_indices));
    assert!(Arc::ptr_eq(
        &before.visible_triangle_ids,
        &after.visible_triangle_ids
    ));
    assert_eq!(after.mesh.vertices, deformed.vertices);
    assert_ne!(after.normals, before.normals);
    let ray = Ray3::from_dvec(DVec3::new(0.0, 0.0, 2.0), -DVec3::Z).unwrap();
    assert!(
        after
            .pick_visible_surface(ray, ModelTransform::default())
            .is_none(),
        "render-only result surfaces must not retain a stale BVH"
    );
}

#[test]
fn bound_preview_rejects_bad_vertices_transactionally() {
    let base = OrderedObjMesh {
        vertices: vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]],
        faces: vec![ObjFace {
            vertex_indices: vec![0, 1, 2],
            group: Some("Face".into()),
            material: Some("Face".into()),
        }],
    };
    let mut workspace = WorkspaceScene::default();
    workspace.install_result(Arc::new(base.clone())).unwrap();
    let output_before = Arc::clone(workspace.result_output.as_ref().unwrap());
    let surface_before = Arc::clone(workspace.result.as_ref().unwrap());

    let count_error = workspace
        .update_result_preview_vertices(base.vertices[..2].to_vec())
        .unwrap_err();
    assert!(matches!(
        count_error,
        SceneLoadError::ResultVertexCountMismatch {
            expected: 3,
            actual: 2
        }
    ));
    assert!(Arc::ptr_eq(
        &output_before,
        workspace.result_output.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        &surface_before,
        workspace.result.as_ref().unwrap()
    ));

    let mut nonfinite = base.vertices.clone();
    nonfinite[1][2] = f64::NAN;
    let nonfinite_error = workspace
        .update_result_preview_vertices(nonfinite)
        .unwrap_err();
    assert!(matches!(nonfinite_error, SceneLoadError::Format(_)));
    assert!(Arc::ptr_eq(
        &output_before,
        workspace.result_output.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        &surface_before,
        workspace.result.as_ref().unwrap()
    ));
    assert_eq!(output_before.as_ref(), &base);
}

#[test]
fn bound_preview_cow_preserves_old_arc_and_static_faces() {
    let base = OrderedObjMesh {
        vertices: vec![[-1.0, -1.0, 0.0], [1.0, -1.0, 0.0], [0.0, 1.0, 0.0]],
        faces: vec![ObjFace {
            vertex_indices: vec![0, 1, 2],
            group: Some("Face".into()),
            material: Some("Face".into()),
        }],
    };
    let mut workspace = WorkspaceScene::default();
    workspace.install_result(Arc::new(base.clone())).unwrap();

    let mut first = base.vertices.clone();
    first[2][2] = 0.1;
    workspace.update_result_preview_vertices(first).unwrap();
    let stable_face_ptr = workspace.result_output.as_ref().unwrap().faces.as_ptr();
    let stable_faces = workspace.result_output.as_ref().unwrap().faces.clone();

    let held_old = Arc::clone(workspace.result_output.as_ref().unwrap());
    let mut second = base.vertices.clone();
    second[2][2] = 0.25;
    workspace
        .update_result_preview_vertices(second.clone())
        .unwrap();
    let current = workspace.result_output.as_ref().unwrap();
    assert!(!Arc::ptr_eq(&held_old, current));
    assert_eq!(held_old.vertices[2][2], 0.1);
    assert_eq!(current.vertices, second);
    assert_eq!(current.faces, stable_faces);
    assert_ne!(current.faces.as_ptr(), stable_face_ptr);

    drop(held_old);
    let current_face_ptr = current.faces.as_ptr();
    let _ = current;
    let mut third = base.vertices.clone();
    third[2][2] = 0.5;
    workspace.update_result_preview_vertices(third).unwrap();
    assert_eq!(
        workspace.result_output.as_ref().unwrap().faces.as_ptr(),
        current_face_ptr
    );
    assert_eq!(
        workspace.result_output.as_ref().unwrap().faces,
        stable_faces
    );
}

#[test]
fn head_visual_mask_preserves_global_triangle_ids_and_visible_bounds() {
    let ordered = OrderedObjMesh {
        vertices: vec![
            [-20.0, -100.0, 0.0],
            [20.0, -100.0, 0.0],
            [0.0, -80.0, 0.0],
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        faces: vec![
            ObjFace {
                vertex_indices: vec![0, 1, 2],
                group: Some("Torso".into()),
                material: Some("Torso".into()),
            },
            ObjFace {
                vertex_indices: vec![3, 4, 5],
                group: Some("Face".into()),
                material: Some("Face".into()),
            },
        ],
    };
    let mesh = SurfaceMesh::from_ordered_head_visual(&ordered).unwrap();
    assert_eq!(mesh.mesh.triangles.len(), 2);
    assert_eq!(mesh.visible_triangle_ids.as_slice(), &[1]);
    assert_eq!(mesh.render_triangles.as_slice(), &[[3, 4, 5]]);
    assert_eq!(mesh.visible_bounds.min.y, -1.0);
    assert_eq!(mesh.visible_bounds.max.y, 1.0);
    assert_eq!(mesh.bounds.min.y, -100.0);

    let ray = Ray3::from_dvec(DVec3::new(0.0, 0.0, 2.0), -DVec3::Z).unwrap();
    let hit = mesh
        .pick_visible_surface(ray, ModelTransform::default())
        .unwrap();
    assert_eq!(
        hit.triangle, 1,
        "picking must return the global triangle id"
    );
    assert!(mesh.endpoint_local_point(hit.into()).is_some());
}

#[test]
fn result_eye_parts_are_separate_display_surfaces_over_one_full_mesh() {
    let ordered = OrderedObjMesh {
        vertices: vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, -1.0, 0.1],
            [1.0, -1.0, 0.1],
            [0.0, 1.0, 0.1],
            [-1.0, -1.0, 0.2],
            [1.0, -1.0, 0.2],
            [0.0, 1.0, 0.2],
            [-1.0, -1.0, 0.3],
            [1.0, -1.0, 0.3],
            [0.0, 1.0, 0.3],
        ],
        faces: [
            (vec![0, 1, 2], "Face"),
            (vec![3, 4, 5], "Tear"),
            (vec![6, 7, 8], "Lacrimals"),
            (vec![9, 10, 11], "Eyelashes"),
        ]
        .into_iter()
        .map(|(vertex_indices, material)| ObjFace {
            vertex_indices,
            group: Some(material.into()),
            material: Some(material.into()),
        })
        .collect(),
    };

    let surfaces = result_display_surfaces(&ordered).unwrap();
    assert_eq!(surfaces.head.visible_triangle_ids.as_slice(), &[0]);
    assert_eq!(
        surfaces
            .tear_lacrimals
            .as_ref()
            .unwrap()
            .visible_triangle_ids
            .as_slice(),
        &[1, 2]
    );
    assert_eq!(
        surfaces
            .eyelashes
            .as_ref()
            .unwrap()
            .visible_triangle_ids
            .as_slice(),
        &[3]
    );
    assert!(Arc::ptr_eq(
        &surfaces.head.mesh,
        &surfaces.tear_lacrimals.as_ref().unwrap().mesh
    ));
    assert!(Arc::ptr_eq(
        &surfaces.head.mesh,
        &surfaces.eyelashes.as_ref().unwrap().mesh
    ));

    let mut workspace = WorkspaceScene::default();
    workspace.install_result(Arc::new(ordered.clone())).unwrap();
    assert_eq!(workspace.result_output.as_ref().unwrap().faces.len(), 4);
    assert!(workspace.result_tear_lacrimals.is_some());
    assert!(workspace.result_eyelashes.is_some());
}

fn grouped_sculpt_result_fixture() -> OrderedObjMesh {
    let labels: &[(Option<&str>, Option<&str>)] = &[
        (Some("Face"), Some("head")),
        (Some("unmapped"), Some("Nostrils")),
        (Some("Tear"), None),
        (Some("Lacrimals"), None),
        (Some("Eyelashes"), None),
        (Some("Sclera"), None),
        (Some("Irises"), None),
        (Some("Pupils"), None),
        (Some("Cornea"), None),
        (Some("Eye Reflection"), None),
        (Some("Lips"), None),
        (Some("Inner Mouth"), None),
        (Some("Teeth"), None),
        (Some("Gums"), None),
        (Some("Tongue"), None),
        (Some("unmapped"), Some("upperJaw")),
        (Some("unmapped"), Some("lower_jaw")),
        (Some("Clothing"), Some("shirt")),
    ];
    let vertices = (0..labels.len())
        .flat_map(|index| {
            let x = index as f64 * 4.0;
            [[x, 0.0, 0.0], [x + 1.0, 0.0, 0.0], [x, 1.0, 0.0]]
        })
        .collect();
    let faces = labels
        .iter()
        .enumerate()
        .map(|(index, (material, group))| ObjFace {
            vertex_indices: vec![
                (index * 3) as u32,
                (index * 3 + 1) as u32,
                (index * 3 + 2) as u32,
            ],
            group: group.map(str::to_owned),
            material: material.map(str::to_owned),
        })
        .collect();
    OrderedObjMesh { vertices, faces }
}

#[test]
fn sculpt_preview_groups_are_disjoint_render_only_material_views() {
    let ordered = grouped_sculpt_result_fixture();
    let surfaces = result_display_surfaces(&ordered).unwrap();
    let expected = [
        (SculptSurfaceGroup::HeadSkin, &[0, 1][..]),
        (SculptSurfaceGroup::TearLacrimal, &[2, 3][..]),
        (SculptSurfaceGroup::Eyelashes, &[4][..]),
        (SculptSurfaceGroup::Eyes, &[5, 6, 7, 8, 9][..]),
        (SculptSurfaceGroup::Lips, &[10][..]),
        (SculptSurfaceGroup::TeethTongue, &[12, 13, 14, 15, 16][..]),
        (SculptSurfaceGroup::InnerMouth, &[11][..]),
    ];
    let mut claimed = BTreeSet::new();
    let ray = Ray3::from_dvec(DVec3::new(0.25, 0.25, 2.0), -DVec3::Z).unwrap();

    for (group, expected_ids) in expected {
        let surface = surfaces.sculpt.surface(group).unwrap();
        assert_eq!(surface.visible_triangle_ids.as_slice(), expected_ids);
        assert!(surface.editable_triangle_ids.is_empty());
        assert!(
            surface
                .pick_visible_surface(ray, ModelTransform::default())
                .is_none(),
            "sculpt color surfaces are render-only and must not duplicate BVHs"
        );
        assert!(Arc::ptr_eq(&surface.mesh, &surfaces.head.mesh));
        for &triangle in surface.visible_triangle_ids.iter() {
            assert!(
                claimed.insert(triangle),
                "triangle {triangle} was classified into more than one sculpt group"
            );
        }
    }

    assert_eq!(claimed, (0_u32..17).collect());
    assert!(
        !claimed.contains(&17),
        "unknown clothing must stay ungrouped"
    );
    assert!(Arc::ptr_eq(
        surfaces.tear_lacrimals.as_ref().unwrap(),
        &surfaces
            .sculpt
            .surface(SculptSurfaceGroup::TearLacrimal)
            .unwrap()
    ));
    assert!(Arc::ptr_eq(
        surfaces.eyelashes.as_ref().unwrap(),
        &surfaces
            .sculpt
            .surface(SculptSurfaceGroup::Eyelashes)
            .unwrap()
    ));
}

#[test]
fn sculpt_surface_api_reuses_topology_without_mutating_ordered_preview() {
    let original = grouped_sculpt_result_fixture();
    let mut workspace = WorkspaceScene::default();
    workspace
        .install_result(Arc::new(original.clone()))
        .unwrap();

    assert_eq!(workspace.result_output.as_deref(), Some(&original));
    let output_before = Arc::clone(workspace.result_output.as_ref().unwrap());
    let before: Vec<_> = workspace.result_sculpt_surfaces().collect();
    assert_eq!(
        before
            .iter()
            .map(|surface| surface.group)
            .collect::<Vec<_>>(),
        SculptSurfaceGroup::ALL
    );
    let result_mesh = &workspace.result.as_ref().unwrap().mesh;
    assert!(
        before
            .iter()
            .all(|surface| Arc::ptr_eq(&surface.mesh.mesh, result_mesh))
    );

    let mut deformed_vertices = original.vertices.clone();
    for vertex in &mut deformed_vertices {
        vertex[2] += 0.25;
    }
    workspace
        .update_result_preview_vertices(deformed_vertices.clone())
        .unwrap();

    assert_eq!(output_before.as_ref(), &original);
    assert_eq!(workspace.fitted_result.as_deref(), Some(&original));
    let output_after = workspace.result_output.as_ref().unwrap();
    assert_eq!(output_after.faces, original.faces);
    assert_eq!(output_after.vertices, deformed_vertices);
    assert_eq!(output_after.faces.len(), original.faces.len());

    let after: Vec<_> = workspace.result_sculpt_surfaces().collect();
    let deformed_mesh = &workspace.result.as_ref().unwrap().mesh;
    for current in &after {
        let previous = before
            .iter()
            .find(|surface| surface.group == current.group)
            .unwrap();
        assert!(Arc::ptr_eq(&current.mesh.mesh, deformed_mesh));
        assert!(Arc::ptr_eq(
            &current.mesh.visible_triangle_ids,
            &previous.mesh.visible_triangle_ids
        ));
        assert!(Arc::ptr_eq(
            &current.mesh.render_triangles,
            &previous.mesh.render_triangles
        ));
        assert!(Arc::ptr_eq(
            &current.mesh.wire_indices,
            &previous.mesh.wire_indices
        ));
    }

    workspace.result = None;
    assert_eq!(workspace.result_sculpt_surfaces().count(), 0);
    assert!(
        workspace
            .result_sculpt_surface(SculptSurfaceGroup::HeadSkin)
            .is_none(),
        "a cleared result must never reveal a stale cached group"
    );
}

#[test]
fn daz_head_mask_expands_polygon_faces_to_global_triangle_ids() {
    let geometry = DazGeometry::new(
        "masked-template".into(),
        vec![
            [-2.0, -2.0, 0.0],
            [2.0, -2.0, 0.0],
            [2.0, -1.0, 0.0],
            [-2.0, -1.0, 0.0],
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 2.0, 0.0],
        ],
        vec![vec![0, 1, 2, 3], vec![4, 5, 6]],
        vkit_core::formats::GroupTable {
            indices: vec![0, 1],
            names: vec!["hip".into(), "head".into()],
        },
        vkit_core::formats::GroupTable {
            indices: vec![0, 1],
            names: vec!["Torso".into(), "Face".into()],
        },
        json!({}),
    )
    .unwrap();
    let mesh = SurfaceMesh::from_daz_head_visual(&geometry).unwrap();
    assert_eq!(mesh.mesh.triangles.len(), 3);
    assert_eq!(mesh.visible_triangle_ids.as_slice(), &[2]);
    assert_eq!(mesh.render_triangles.as_slice(), &[[4, 5, 6]]);
    assert_eq!(mesh.editable_triangle_ids.as_slice(), &[2]);
}

#[test]
fn template_renders_anatomy_but_editable_picking_is_skin_only() {
    let geometry = DazGeometry::new(
        "layered-template".into(),
        vec![
            [-1.0, -1.0, 0.5],
            [1.0, -1.0, 0.5],
            [0.0, 1.0, 0.5],
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [2.0, -1.0, 0.5],
            [4.0, -1.0, 0.5],
            [3.0, 1.0, 0.5],
        ],
        vec![vec![0, 1, 2], vec![3, 4, 5], vec![6, 7, 8]],
        vkit_core::formats::GroupTable {
            indices: vec![0, 0, 0],
            names: vec!["Head".into()],
        },
        vkit_core::formats::GroupTable {
            indices: vec![0, 1, 2],
            names: vec!["Sclera".into(), "Face".into(), "Nostrils".into()],
        },
        json!({}),
    )
    .unwrap();
    let mesh = SurfaceMesh::from_daz_head_visual(&geometry).unwrap();

    assert_eq!(mesh.visible_triangle_ids.as_slice(), &[0, 1, 2]);
    assert_eq!(mesh.editable_triangle_ids.as_slice(), &[1]);
    assert_eq!(mesh.render_triangles.len(), 3);

    let through_eye = Ray3::from_dvec(DVec3::new(0.0, 0.0, 2.0), -DVec3::Z).unwrap();
    assert_eq!(
        mesh.pick_visible_surface(through_eye, ModelTransform::default())
            .unwrap()
            .triangle,
        0,
        "the visible BVH must retain front anatomy for rendering/occlusion"
    );
    assert_eq!(
        mesh.pick_editable_surface(through_eye, ModelTransform::default())
            .unwrap()
            .triangle,
        1,
        "direct pin picking must pass over Sclera and attach to global Face triangle 1"
    );

    let nostril_only = Ray3::from_dvec(DVec3::new(3.0, 0.0, 2.0), -DVec3::Z).unwrap();
    assert_eq!(
        mesh.pick_visible_surface(nostril_only, ModelTransform::default())
            .unwrap()
            .triangle,
        2
    );
    assert!(
        mesh.pick_editable_surface(nostril_only, ModelTransform::default())
            .is_none(),
        "Nostrils remain visible but cannot receive a direct pin"
    );
}

#[test]
fn full_body_focus_uses_the_upper_head_region() {
    let mesh = SurfaceMesh::new(
        Mesh::new(
            vec![
                [-10.0, 0.0, -2.0],
                [10.0, 0.0, -2.0],
                [0.0, 100.0, 2.0],
                [-2.0, 84.0, -2.0],
                [2.0, 84.0, -2.0],
                [2.0, 100.0, 2.0],
                [-2.0, 100.0, 2.0],
            ],
            vec![[0, 1, 2], [3, 4, 5], [3, 5, 6]],
        )
        .unwrap(),
    )
    .unwrap();
    let focus = mesh.facial_focus_bounds();
    assert!(focus.min.y >= 81.0);
    assert!(focus.radius() < mesh.bounds.radius());
}

#[test]
fn dsf_geometry_is_rendered_with_the_same_triangulated_topology() {
    let geometry = DazGeometry::new(
        "synthetic-template".into(),
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        vec![vec![0, 1, 2, 3]],
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
    .unwrap();
    let mut workspace = WorkspaceScene::default();
    workspace.load_template_geometry(geometry).unwrap();
    assert_eq!(workspace.template.as_ref().unwrap().mesh.vertices.len(), 4);
    assert_eq!(workspace.template.as_ref().unwrap().mesh.triangles.len(), 2);
    assert_eq!(
        workspace.template_geometry.as_ref().unwrap().faces[0],
        vec![0, 1, 2, 3]
    );
    workspace.set_template_eye_value(0.0).unwrap();
    assert_eq!(
        workspace
            .template
            .as_ref()
            .unwrap()
            .visible_triangle_ids
            .as_slice(),
        &[0, 1],
        "eye refresh must retain the head-only visibility contract"
    );
}

fn synthetic_face_template() -> DazGeometry {
    DazGeometry::new(
        "synthetic-template".into(),
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        vec![vec![0, 1, 2, 3]],
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

#[test]
fn installing_the_template_unchanged_reuses_what_its_load_built() {
    let mut workspace = WorkspaceScene::default();
    workspace
        .load_template_geometry(synthetic_face_template())
        .unwrap();
    let prebuilt = Arc::clone(
        &workspace
            .template_surfaces
            .as_ref()
            .expect("the load prebuilds the template's own result")
            .surfaces
            .head,
    );

    let ordered = workspace.template_ordered_obj().unwrap();
    workspace.install_result(ordered).unwrap();
    assert!(
        Arc::ptr_eq(workspace.result.as_ref().unwrap(), &prebuilt),
        "the template's own install must reuse the prebuilt surfaces"
    );

    workspace
        .install_result(Arc::new(grouped_sculpt_result_fixture()))
        .unwrap();
    assert!(
        !Arc::ptr_eq(workspace.result.as_ref().unwrap(), &prebuilt),
        "a fitted result must never pick up the template's surfaces"
    );
}

#[test]
fn identical_pick_and_sculpt_masks_share_one_acceleration_tree() {
    let mut workspace = WorkspaceScene::default();
    workspace
        .install_result(Arc::new(grouped_sculpt_result_fixture()))
        .unwrap();
    let head = workspace.result.as_ref().expect("installed head surface");

    assert_eq!(head.visible_triangle_ids, head.editable_triangle_ids);
    assert!(
        Arc::ptr_eq(&head.visible_bvh, &head.editable_bvh),
        "identical masks must not build the tree twice"
    );
}

#[test]
fn template_load_caches_the_ordered_conversion_detail_entry_would_otherwise_repeat() {
    let geometry = synthetic_face_template();
    let expected = geometry.to_ordered_obj(None).unwrap();
    let mut workspace = WorkspaceScene::default();
    workspace.load_template_geometry(geometry).unwrap();

    let cached = workspace
        .template_ordered_obj()
        .expect("a template that converts is converted when it lands");
    assert_eq!(*cached, expected);

    let again = workspace.template_ordered_obj().unwrap();
    assert!(Arc::ptr_eq(&cached, &again));
}

#[test]
fn linked_edit_camera_frame_uses_one_canonical_head_reference() {
    let scan = Arc::new(
        SurfaceMesh::new(
            Mesh::new(
                vec![[-5.0, -8.0, 0.0], [5.0, -8.0, 0.0], [0.0, 8.0, 0.0]],
                vec![[0, 1, 2]],
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let template = Arc::new(plane_mesh());
    let mut workspace = WorkspaceScene {
        scan: Some(scan),
        template: Some(template),
        ..Default::default()
    };
    workspace.scan_camera.distance = 999.0;
    workspace.template_camera.distance = 123.0;
    assert!(workspace.frame_linked_edit_cameras());
    assert_eq!(workspace.scan_camera, workspace.template_camera);
    assert!(workspace.scan_camera.distance < 123.0);

    workspace.scan_camera.yaw = 0.75;
    workspace.reconcile_linked_edit_cameras(MeshSide::Scan);
    assert_eq!(workspace.scan_camera, workspace.template_camera);
}

#[test]
fn scan_symmetry_is_derived_without_destroying_the_loaded_source() {
    let source = SurfaceMesh::new(
        Mesh::new(
            vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
            vec![[0, 1, 2], [0, 2, 3]],
        )
        .unwrap(),
    )
    .unwrap();
    let source = Arc::new(source);
    let mut workspace = WorkspaceScene {
        scan_source: Some(Arc::clone(&source)),
        scan: Some(source),
        ..Default::default()
    };
    workspace.scan_camera.yaw = 0.71;
    workspace.scan_camera.pitch = -0.19;
    workspace.scan_camera.target = Vec3::new(3.0, 4.0, 5.0);
    workspace.scan_camera.distance = 42.0;
    workspace.scan_camera.orthographic_scale = 8.5;
    workspace.template_camera = workspace.scan_camera;
    let scan_camera = workspace.scan_camera;
    let template_camera = workspace.template_camera;

    workspace
        .set_scan_symmetry_with_transform(SymmetryMode::Off, ModelTransform::default())
        .unwrap();
    assert_eq!(
        workspace.scan_source.as_ref().unwrap().mesh.vertices,
        workspace.scan.as_ref().unwrap().mesh.vertices
    );
    assert_eq!(workspace.scan_camera, scan_camera);
    assert_eq!(workspace.template_camera, template_camera);

    workspace
        .set_scan_symmetry_with_transform(SymmetryMode::PositiveX, ModelTransform::default())
        .unwrap();
    assert_eq!(workspace.scan_camera, scan_camera);
    assert_eq!(workspace.template_camera, template_camera);
}

#[test]
fn reoriented_scan_symmetry_uses_world_x_and_preserves_stable_ids() {
    let source_mesh = Mesh::new(
        vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [-1.0, 1.0, 0.03],
            [1.0, 1.0, 0.02],
        ],
        vec![[0, 1, 2], [1, 3, 2], [2, 3, 4], [3, 5, 4]],
    )
    .unwrap();
    let source_vertices = source_mesh.vertices.clone();
    let source_triangles = source_mesh.triangles.clone();
    let source = Arc::new(SurfaceMesh::new(source_mesh).unwrap());
    let mut workspace = WorkspaceScene {
        scan_source: Some(Arc::clone(&source)),
        scan: Some(source),
        ..Default::default()
    };
    let transform = ModelTransform::from_components_xyz_with_pivot(
        [1.7, 0.8, 1.2],
        [4.0, -3.0, 2.0],
        [0.0, 0.0, 90.0],
        [0.0; 3],
    );

    workspace
        .set_scan_symmetry_with_transform(SymmetryMode::PositiveX, transform)
        .unwrap();

    let displayed = workspace.scan.as_ref().unwrap();
    assert_eq!(displayed.mesh.vertices.len(), source_vertices.len());
    assert_eq!(displayed.mesh.triangles, source_triangles);
    assert!(displayed.mesh.vertices[4][2].abs() < 1.0e-12);
    assert!(displayed.mesh.vertices[5][2].abs() < 1.0e-12);
    assert_eq!(
        workspace.scan_source.as_ref().unwrap().mesh.vertices,
        source_vertices
    );
}

#[test]
fn half_turn_symmetry_uses_the_displayed_positive_world_side() {
    let source = Arc::new(
        SurfaceMesh::new(
            Mesh::new(
                vec![
                    [-1.0, -1.0, 0.0],
                    [-1.0, 1.0, 0.0],
                    [0.0, -1.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [1.0, -1.0, 0.03],
                    [1.0, 1.0, 0.02],
                ],
                vec![[0, 2, 1], [1, 2, 3], [2, 4, 3], [3, 4, 5]],
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let mut workspace = WorkspaceScene {
        scan_source: Some(Arc::clone(&source)),
        scan: Some(source),
        ..Default::default()
    };

    workspace
        .set_scan_symmetry_with_transform(
            SymmetryMode::PositiveX,
            ModelTransform::from_components_xyz(
                [0.75, 1.5, 1.25],
                [2.0, 3.0, -4.0],
                [0.0, 180.0, 0.0],
            ),
        )
        .unwrap();

    let displayed = workspace.scan.as_ref().unwrap();
    assert!(displayed.mesh.vertices[4][2].abs() < 1.0e-12);
    assert!(displayed.mesh.vertices[5][2].abs() < 1.0e-12);
}

#[test]
fn the_figure_surface_shows_the_body_without_duplicating_or_unlocking_it() {
    let ordered = OrderedObjMesh {
        vertices: vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, -1.0, 0.5],
            [1.0, -1.0, 0.5],
            [0.0, 1.0, 0.5],
        ],
        faces: [(vec![0, 1, 2], "Face"), (vec![3, 4, 5], "Torso")]
            .into_iter()
            .map(|(vertex_indices, material)| ObjFace {
                vertex_indices,
                group: Some(material.into()),
                material: Some(material.into()),
            })
            .collect(),
    };

    let surfaces = result_display_surfaces(&ordered).unwrap();
    assert_eq!(
        surfaces.head.visible_triangle_ids.as_slice(),
        &[0],
        "the head surface still stops at the neck"
    );
    assert_eq!(
        surfaces.figure.visible_triangle_ids.as_slice(),
        &[0, 1],
        "the figure surface draws the torso too"
    );

    assert!(
        Arc::ptr_eq(&surfaces.head.mesh, &surfaces.figure.mesh),
        "the two surfaces must share one vertex buffer"
    );

    assert_eq!(
        surfaces.figure.editable_triangle_ids.as_slice(),
        surfaces.head.editable_triangle_ids.as_slice(),
        "the figure surface must not widen what can be sculpted"
    );
}
