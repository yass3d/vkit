use super::*;
use vkit_core::formats::ObjFace;

fn face(indices: &[u32], material: &str) -> ObjFace {
    ObjFace {
        vertex_indices: indices.to_vec(),
        group: None,
        material: Some(material.to_owned()),
    }
}

fn isolated_targets() -> OrderedObjMesh {
    OrderedObjMesh {
        vertices: vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, -1.0, 2.0],
            [1.0, -1.0, 2.0],
            [0.0, 1.0, 2.0],
            [-1.0, -1.0, 4.0],
            [1.0, -1.0, 4.0],
            [0.0, 1.0, 4.0],
        ],
        faces: vec![
            face(&[0, 1, 2], "Face"),
            face(&[3, 4, 5], "Sclera"),
            face(&[6, 7, 8], "Teeth"),
        ],
    }
}

fn nearby_independent_eye_shells() -> OrderedObjMesh {
    OrderedObjMesh {
        vertices: vec![
            [-3.0, -1.0, 0.0],
            [-1.0, -1.0, 0.0],
            [-2.0, 1.0, 0.0],
            [-3.0, -1.0, 0.1],
            [-1.0, -1.0, 0.1],
            [-2.0, 1.0, 0.1],
            [1.0, -1.0, 0.0],
            [3.0, -1.0, 0.0],
            [2.0, 1.0, 0.0],
            [1.0, -1.0, 0.1],
            [3.0, -1.0, 0.1],
            [2.0, 1.0, 0.1],
        ],
        faces: vec![
            face(&[0, 1, 2], "Tear"),
            face(&[3, 4, 5], "Eyelashes"),
            face(&[6, 7, 8], "Tear"),
            face(&[9, 10, 11], "Eyelashes"),
        ],
    }
}

fn layered_lash_components() -> OrderedObjMesh {
    OrderedObjMesh {
        vertices: vec![
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.0, 0.5, 0.0],
            [0.65, -0.2, 0.05],
            [0.95, -0.2, 0.05],
            [0.8, 0.2, 0.05],
            [0.8, 0.0, 0.08],
            [-1.0, -0.2, 0.08],
            [-0.7, -0.2, 0.08],
            [-0.85, 0.1, 0.08],
        ],
        faces: vec![
            face(&[0, 1, 2], "Tear"),
            face(&[3, 4, 5], "Eyelashes"),
            face(&[4, 3, 6], "Eyelashes"),
            face(&[8, 7, 9], "Eyelashes"),
        ],
    }
}

fn triangle_area(vertices: &[[f64; 3]], triangle: [usize; 3]) -> f64 {
    let [a, b, c] = triangle.map(|index| DVec3::from_array(vertices[index]));
    (b - a).cross(c - a).length() * 0.5
}

fn signed_volume(vertices: &[[f64; 3]], triangles: &[[u32; 3]]) -> f64 {
    triangles
        .iter()
        .map(|&triangle| {
            let [a, b, c] = triangle.map(|index| DVec3::from_array(vertices[index as usize]));
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

fn connected_fold() -> OrderedObjMesh {
    OrderedObjMesh {
        vertices: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.02],
        ],

        faces: vec![face(&[0, 1, 2], "Face"), face(&[1, 0, 3], "Face")],
    }
}

fn hairpin_fold() -> OrderedObjMesh {
    OrderedObjMesh {
        vertices: vec![
            [-0.5, -0.5, 0.0],
            [0.5, -0.5, 0.0],
            [0.5, 0.5, 0.0],
            [-0.5, 0.5, 0.0],
            [3.0, -0.5, 0.0],
            [3.0, 0.5, 0.0],
            [0.5, -0.5, 0.1],
            [0.5, 0.5, 0.1],
            [-0.5, -0.5, 0.1],
            [-0.5, 0.5, 0.1],
        ],
        faces: vec![
            face(&[0, 1, 2, 3], "Face"),
            face(&[1, 4, 5, 2], "Face"),
            face(&[4, 6, 7, 5], "Face"),
            face(&[6, 8, 9, 7], "Face"),
        ],
    }
}

fn noisy_grid() -> OrderedObjMesh {
    OrderedObjMesh {
        vertices: vec![
            [-1.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 0.0, 0.8],
            [1.0, 0.0, 0.0],
            [-1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
        faces: vec![
            face(&[0, 1, 4, 3], "Face"),
            face(&[1, 2, 5, 4], "Face"),
            face(&[3, 4, 7, 6], "Face"),
            face(&[4, 5, 8, 7], "Face"),
        ],
    }
}

fn frequency_grid(size: usize, high_frequency: bool) -> OrderedObjMesh {
    assert!(size >= 5);
    let extent = (size - 1) as f64;
    let mut vertices = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let normalized_x = x as f64 / extent;
            let normalized_y = y as f64 / extent;
            let envelope = (std::f64::consts::PI * normalized_x).sin()
                * (std::f64::consts::PI * normalized_y).sin();
            let carrier = if high_frequency && (x + y) % 2 == 1 {
                -1.0
            } else {
                1.0
            };
            let amplitude = if high_frequency { 0.08 } else { 0.20 };
            vertices.push([
                x as f64 - extent * 0.5,
                y as f64 - extent * 0.5,
                amplitude * envelope * carrier,
            ]);
        }
    }
    let mut faces = Vec::with_capacity((size - 1) * (size - 1) * 2);
    for y in 0..size - 1 {
        for x in 0..size - 1 {
            let a = (y * size + x) as u32;
            let b = a + 1;
            let d = ((y + 1) * size + x) as u32;
            let c = d + 1;
            faces.push(face(&[a, b, c], "Face"));
            faces.push(face(&[a, c, d], "Face"));
        }
    }
    OrderedObjMesh { vertices, faces }
}

fn full_strength_head_influence(mesh: &OrderedObjMesh) -> Vec<BrushInfluence> {
    mesh.vertices
        .iter()
        .enumerate()
        .map(|(vertex, _)| BrushInfluence {
            vertex,
            falloff: 1.0,
            radial_falloff: 1.0,
            brush_strength: 1.0,
            sheet_normal: None,
            surface_bits: SculptTarget::HeadSkin as u8,
            restrict_to_front_sheet: false,
            incoming_direction: None,
        })
        .collect()
}

#[test]
fn canonical_x_mirror_map_preserves_center_and_pairs_both_sides() {
    let basis = [[-1.0, 0.25, 0.5], [1.0, 0.25, 0.5], [0.0, 1.0, -0.5]];

    assert_eq!(build_x_mirror_vertex_map(&basis), vec![1, 0, 2]);
}

#[test]
fn topology_mirror_remaps_vertices_and_mirrors_directional_data() {
    let influence = [BrushInfluence {
        vertex: 0,
        falloff: 0.75,
        radial_falloff: 0.5,
        brush_strength: 0.8,
        sheet_normal: Some(DVec3::new(1.0, 2.0, 3.0)),
        surface_bits: SculptTarget::HeadSkin as u8,
        restrict_to_front_sheet: true,
        incoming_direction: Some(DVec3::new(-4.0, 5.0, 6.0)),
    }];

    let mirrored = mirror_influence_by_topology(&influence, &[1, 0]);

    assert_eq!(mirrored.len(), 1);
    assert_eq!(mirrored[0].vertex, 1);
    assert_eq!(mirrored[0].falloff, 0.75);
    assert_eq!(mirrored[0].sheet_normal, Some(DVec3::new(-1.0, 2.0, 3.0)));
    assert_eq!(
        mirrored[0].incoming_direction,
        Some(DVec3::new(4.0, 5.0, 6.0))
    );
}

fn legacy_taubin_smooth_proposals(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    influence: &[BrushInfluence],
) -> Vec<(usize, [f64; 3])> {
    const LEGACY_LAMBDA: f64 = 0.60;
    const LEGACY_MU: f64 = -0.62;
    let targets = SculptTargets::HEAD_SKIN;
    let mut first_pass = Vec::with_capacity(influence.len());
    for entry in influence {
        let point = DVec3::from_array(vertices[entry.vertex]);
        if smooth_vertex_touches_feature(entry, vertices, topology) {
            first_pass.push((entry.vertex, point));
            continue;
        }
        let Some((average, _)) = smooth_neighbor_average(entry, vertices, topology, targets, &[])
        else {
            first_pass.push((entry.vertex, point));
            continue;
        };
        first_pass.push((entry.vertex, point.lerp(average, LEGACY_LAMBDA)));
    }
    let mut proposals = Vec::with_capacity(influence.len());
    for (entry, &(_, first)) in influence.iter().zip(&first_pass) {
        if smooth_vertex_touches_feature(entry, vertices, topology) {
            continue;
        }
        let Some((average, mean_edge_length)) =
            smooth_neighbor_average(entry, vertices, topology, targets, &first_pass)
        else {
            continue;
        };
        let original = DVec3::from_array(vertices[entry.vertex]);
        let filtered = first + (average - first) * LEGACY_MU;
        let unrestricted = original.lerp(filtered, entry.falloff);
        let displacement = unrestricted - original;
        let maximum_displacement = mean_edge_length * MAX_SMOOTH_EDGE_FRACTION;
        let next = if displacement.length() > maximum_displacement {
            original + displacement.normalize_or_zero() * maximum_displacement
        } else {
            unrestricted
        };
        if next.is_finite() && next.distance_squared(original) > POSITION_EPSILON_SQUARED {
            proposals.push((entry.vertex, next.to_array()));
        }
    }
    backtrack_smooth_proposals(vertices, topology, proposals, &BTreeMap::new())
}

fn apply_proposals(vertices: &[[f64; 3]], proposals: &[(usize, [f64; 3])]) -> Vec<[f64; 3]> {
    let mut result = vertices.to_vec();
    for &(vertex, position) in proposals {
        result[vertex] = position;
    }
    result
}

fn rms_height(vertices: &[[f64; 3]]) -> f64 {
    (vertices
        .iter()
        .map(|vertex| vertex[2] * vertex[2])
        .sum::<f64>()
        / vertices.len() as f64)
        .sqrt()
}

fn symmetric_head_triangles() -> OrderedObjMesh {
    OrderedObjMesh {
        vertices: vec![
            [-3.0, -1.0, 0.0],
            [-1.0, -1.0, 0.0],
            [-2.0, 1.0, 0.0],
            [3.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [2.0, 1.0, 0.0],
        ],

        faces: vec![face(&[0, 1, 2], "Face"), face(&[3, 5, 4], "Face")],
    }
}

fn symmetric_noisy_patches() -> OrderedObjMesh {
    let mut left = noisy_grid();
    for vertex in &mut left.vertices {
        vertex[0] -= 3.0;
    }
    let left_count = left.vertices.len() as u32;
    let mut vertices = left.vertices.clone();
    vertices.extend(
        left.vertices
            .iter()
            .map(|vertex| [-vertex[0], vertex[1], vertex[2]]),
    );
    let mut faces = left.faces.clone();
    faces.extend(left.faces.iter().map(|source| {
        ObjFace {
            vertex_indices: source
                .vertex_indices
                .iter()
                .rev()
                .map(|index| index + left_count)
                .collect(),
            group: source.group.clone(),
            material: source.material.clone(),
        }
    }));
    OrderedObjMesh { vertices, faces }
}

fn center_seam() -> OrderedObjMesh {
    OrderedObjMesh {
        vertices: vec![
            [0.0, 0.0, 0.0],
            [-1.0, -1.0, 0.0],
            [-1.0, 1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.0],
        ],
        faces: vec![face(&[1, 0, 2], "Face"), face(&[3, 4, 0], "Face")],
    }
}

fn flat_irregular_grid() -> OrderedObjMesh {
    let mut mesh = noisy_grid();
    mesh.vertices[4] = [0.35, 0.20, 0.0];
    mesh
}

fn neck_and_feature_boundaries() -> OrderedObjMesh {
    let vertices = vec![
        [0.0, -2.5, 0.0],
        [2.0, -3.0, 0.0],
        [1.4, -3.0, 1.4],
        [0.0, -3.0, 2.0],
        [-1.4, -3.0, 1.4],
        [-2.0, -3.0, 0.0],
        [-1.4, -3.0, -1.4],
        [0.0, -3.0, -2.0],
        [1.4, -3.0, -1.4],
        [0.0, 1.0, 0.0],
        [0.6, 1.0, 0.0],
        [0.0, 1.0, 0.6],
        [-0.6, 1.0, 0.0],
        [0.0, 1.0, -0.6],
    ];
    let mut faces = (0..8)
        .map(|index| face(&[0, 1 + index, 1 + (index + 1) % 8], "Face"))
        .collect::<Vec<_>>();
    faces.extend((0..4).map(|index| face(&[9, 10 + index, 10 + (index + 1) % 4], "Face")));
    OrderedObjMesh { vertices, faces }
}

fn closed_tetrahedron() -> OrderedObjMesh {
    OrderedObjMesh {
        vertices: vec![
            [1.0, 1.0, 1.0],
            [-1.0, -1.0, 1.0],
            [-1.0, 1.0, -1.0],
            [1.0, -1.0, -1.0],
        ],
        faces: vec![
            face(&[0, 2, 1], "Face"),
            face(&[0, 1, 3], "Face"),
            face(&[0, 3, 2], "Face"),
            face(&[1, 2, 3], "Face"),
        ],
    }
}

#[test]
fn falloff_presets_are_bounded_monotonic_and_have_exact_endpoints() {
    for preset in [
        SculptFalloff::Smooth,
        SculptFalloff::Smoother,
        SculptFalloff::Sharp,
        SculptFalloff::Linear,
    ] {
        assert_eq!(preset.weight(-1.0), 1.0);
        assert_eq!(preset.weight(0.0), 1.0);
        assert_eq!(preset.weight(1.0), 0.0);
        assert_eq!(preset.weight(2.0), 0.0);
        assert_eq!(preset.weight(f64::NAN), 0.0);
        let mut previous = 1.0;
        for step in 0..=1_000 {
            let weight = preset.weight(f64::from(step) / 1_000.0);
            assert!((0.0..=1.0).contains(&weight));
            assert!(weight <= previous + 1.0e-15);
            previous = weight;
        }
    }
    let sample = 0.25;
    assert_ne!(
        SculptFalloff::Smooth.weight(sample),
        SculptFalloff::Smoother.weight(sample)
    );
    assert_ne!(
        SculptFalloff::Sharp.weight(sample),
        SculptFalloff::Linear.weight(sample)
    );
}

#[test]
fn brush_preferences_persist_across_geometry_reloads_but_clear_resets_them() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    assert!(!session.backface_masking(), "masking is off by default");
    assert_eq!(session.falloff_preset(), SculptFalloff::Smooth);
    assert!(session.x_symmetry());
    assert!(!session.connected_topology_only());
    session.set_backface_masking(true);
    session.set_falloff_preset(SculptFalloff::Sharp);
    session.set_x_symmetry(false);
    session.set_connected_topology_only(true);
    session.begin(&source).unwrap();
    assert!(session.backface_masking());
    assert_eq!(session.falloff_preset(), SculptFalloff::Sharp);
    assert!(!session.x_symmetry());
    assert!(session.connected_topology_only());
    session.load_applied(&source).unwrap();
    assert!(session.backface_masking());
    assert_eq!(session.falloff_preset(), SculptFalloff::Sharp);
    assert!(!session.x_symmetry());
    assert!(session.connected_topology_only());
    session.clear();
    assert!(!session.backface_masking());
    assert_eq!(session.falloff_preset(), SculptFalloff::Smooth);
    assert!(session.x_symmetry());
    assert!(!session.connected_topology_only());
}

#[test]
fn visible_and_editable_target_masks_are_independent() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.set_x_symmetry(false);
    session.begin(&source).unwrap();
    assert_eq!(session.editable_targets(), SculptTargets::FACE_SURFACE);
    assert_eq!(session.visible_targets(), SculptTargets::ALL);

    session.set_visible_targets(SculptTargets::HEAD_SKIN);
    assert!(
        !session
            .visible_targets()
            .contains(SculptTarget::TeethTongue)
    );
    assert!(session.editable_targets().contains(SculptTarget::HeadSkin));
    assert!(
        !session
            .editable_targets()
            .contains(SculptTarget::TeethTongue)
    );
    assert!(session.toggle_editable_target(SculptTarget::TeethTongue));
    assert!(
        session
            .editable_targets()
            .contains(SculptTarget::TeethTongue)
    );

    let skin = session.raycast([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]).unwrap();
    assert_eq!(skin.triangle_index, 0);
    session.set_visible_target_enabled(SculptTarget::TeethTongue, true);
    let teeth = session.raycast([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]).unwrap();
    assert_eq!(teeth.triangle_index, 2);
}

#[test]
fn a_solo_view_returns_the_arrangement_it_replaced() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_visible_target_enabled(SculptTarget::Eyes, false);
    let arranged = session.visible_targets();
    assert!(!arranged.contains(SculptTarget::Eyes));

    session.toggle_solo_target(SculptTarget::HeadSkin);
    assert_eq!(session.soloed_target(), Some(SculptTarget::HeadSkin));
    assert_eq!(session.visible_targets(), SculptTargets::HEAD_SKIN);

    session.toggle_solo_target(SculptTarget::Lips);
    assert_eq!(session.soloed_target(), Some(SculptTarget::Lips));
    assert_eq!(session.visible_targets(), SculptTargets::LIPS);

    session.toggle_solo_target(SculptTarget::Lips);
    assert_eq!(session.soloed_target(), None);
    assert_eq!(session.visible_targets(), arranged);
}

#[test]
fn arranging_visibility_by_hand_ends_the_solo_view() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.toggle_solo_target(SculptTarget::HeadSkin);
    session.set_visible_target_enabled(SculptTarget::Eyes, true);
    assert_eq!(session.soloed_target(), None);

    let arranged = session.visible_targets();
    assert!(arranged.contains(SculptTarget::HeadSkin));
    assert!(arranged.contains(SculptTarget::Eyes));
    session.toggle_solo_target(SculptTarget::Eyes);
    session.toggle_solo_target(SculptTarget::Eyes);
    assert_eq!(session.visible_targets(), arranged);
}

#[test]
fn default_grab_moves_only_head_skin_and_preserves_topology() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.set_x_symmetry(false);
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();
    let changed = session
        .dab(SculptDab {
            center_local: [0.0, 0.0, 0.0],
            radius_local: 3.0,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.5, 0.0, 0.0],
            },
        })
        .unwrap();
    assert_eq!(changed, 3);
    session.end_stroke().unwrap();
    let working = session.working_mesh().unwrap();
    assert_eq!(working.faces, source.faces);
    assert_ne!(&working.vertices[..3], &source.vertices[..3]);
    assert_eq!(&working.vertices[3..], &source.vertices[3..]);
}

#[test]
fn a_grab_lands_the_anchor_vertex_exactly_where_it_was_dragged() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.set_x_symmetry(false);
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();

    session
        .dab(SculptDab {
            center_local: [-0.9, -0.95, 0.0],
            radius_local: 3.0,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.0, 0.0, 0.5],
            },
        })
        .unwrap();
    session.end_stroke().unwrap();

    let moved = session.working_mesh().unwrap().vertices[0];
    let expected = [
        source.vertices[0][0],
        source.vertices[0][1],
        source.vertices[0][2] + 0.5,
    ];
    for axis in 0..3 {
        assert!(
            (moved[axis] - expected[axis]).abs() < 1.0e-9,
            "axis {axis}: {moved:?} should be {expected:?}"
        );
    }

    for neighbour in 1..3 {
        let delta =
            session.working_mesh().unwrap().vertices[neighbour][2] - source.vertices[neighbour][2];
        assert!(
            delta > 0.0 && delta < 0.5,
            "vertex {neighbour} moved {delta}"
        );
    }
}

#[test]
fn grab_pins_only_the_lowest_outer_headskin_boundary_loop() {
    let source = neck_and_feature_boundaries();
    let topology = SculptTopology::build(&source).unwrap();
    assert!(
        topology.grab_protected_vertices[1..9]
            .iter()
            .all(|&protected| protected)
    );
    assert!(
        topology.grab_protected_vertices[10..14]
            .iter()
            .all(|&protected| !protected)
    );

    let mut session = SculptSession::default();
    session.set_x_symmetry(false);
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();
    assert!(
        session
            .dab(SculptDab {
                center_local: [0.0, -2.75, 0.0],
                radius_local: 8.0,
                strength: 1.0,
                operation: SculptOperation::Grab {
                    translation_local: [0.25, 0.20, 0.0],
                },
            })
            .unwrap()
            > 0
    );
    session.end_stroke().unwrap();

    let working = session.working_mesh().unwrap();
    assert_eq!(&working.vertices[1..9], &source.vertices[1..9]);
    assert!(
        working.vertices[10..14]
            .iter()
            .zip(&source.vertices[10..14])
            .all(|(after, before)| after != before),
        "unrelated upper boundary vertices must remain editable"
    );
}

#[test]
fn symmetric_grab_cannot_move_the_lower_neck_weld_loop() {
    let source = neck_and_feature_boundaries();
    let mut session = SculptSession::default();
    session.set_x_symmetry(true);
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();
    assert!(
        session
            .dab(SculptDab {
                center_local: [1.0, -2.75, 0.0],
                radius_local: 8.0,
                strength: 1.0,
                operation: SculptOperation::Grab {
                    translation_local: [0.25, 0.20, 0.0],
                },
            })
            .unwrap()
            > 0
    );
    session.end_stroke().unwrap();

    let working = session.working_mesh().unwrap();
    assert_eq!(&working.vertices[1..9], &source.vertices[1..9]);
    assert!(working.vertices[9] != source.vertices[9]);
}

#[test]
fn explicit_eye_target_does_not_leak_into_skin_or_teeth() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_target_enabled(SculptTarget::HeadSkin, false);
    session.set_target_enabled(SculptTarget::Eyes, true);
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: [0.0, 0.0, 2.0],
            radius_local: 2.5,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.0, 0.5, 0.0],
            },
        })
        .unwrap();
    session.end_stroke().unwrap();
    let working = session.working_mesh().unwrap();
    assert_eq!(&working.vertices[..3], &source.vertices[..3]);
    assert_ne!(&working.vertices[3..6], &source.vertices[3..6]);
    assert_eq!(&working.vertices[6..], &source.vertices[6..]);
}

#[test]
fn inflate_uses_signed_surface_normal_and_undo_is_exact() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: [0.0, 0.0, 0.0],
            radius_local: 3.0,
            strength: 1.0,
            operation: SculptOperation::Inflate { distance: -0.25 },
        })
        .unwrap();
    session.end_stroke().unwrap();
    assert!(session.working_mesh().unwrap().vertices[0][2] < 0.0);
    assert!(session.undo().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);
}

#[test]
fn smooth_is_target_limited_reset_restores_stage_baseline() {
    let mut source = noisy_grid();
    let protected_start = source.vertices.len();
    source
        .vertices
        .extend([[-1.0, -1.0, 2.0], [1.0, -1.0, 2.0], [0.0, 1.0, 2.5]]);
    source.faces.push(face(
        &[
            protected_start as u32,
            protected_start as u32 + 1,
            protected_start as u32 + 2,
        ],
        "Sclera",
    ));
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: [0.0, 0.0, 0.8],
            radius_local: 4.0,
            strength: 0.5,
            operation: SculptOperation::Smooth,
        })
        .unwrap();
    session.end_stroke().unwrap();
    let working = session.working_mesh().unwrap();
    assert_ne!(working.vertices[4], source.vertices[4]);
    assert_eq!(
        &working.vertices[protected_start..],
        &source.vertices[protected_start..]
    );
    assert!(session.reset().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);
    assert_eq!(session.history_len(), 2);
}

#[test]
fn reset_is_one_undoable_history_step_with_full_session_state() {
    let source = noisy_grid();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: [0.0, 0.0, 0.8],
            radius_local: 4.0,
            strength: 0.5,
            operation: SculptOperation::Smooth,
        })
        .unwrap();
    session.end_stroke().unwrap();
    let deformed = session.working_mesh().unwrap().clone();
    let references = session.smooth_reference_areas.clone();
    session.last_hit_triangle.set(Some(2));
    session.last_hit_view_direction.set(Some([0.0, 0.0, -1.0]));
    session.last_hit_anchor.set(Some(StrokeAnchor {
        point: DVec3::new(0.0, 0.0, 0.8),
        normal: DVec3::Z,
        seed_triangle: 2,
    }));
    session.mark_applied();

    assert_eq!(session.history_len(), 1);
    assert!(session.reset().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);
    assert_eq!(session.history_len(), 2);
    assert!(!session.is_applied());
    assert!(!session.has_changes());
    assert!(session.smooth_reference_areas.is_empty());

    assert!(session.undo().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &deformed);
    assert_eq!(session.smooth_reference_areas, references);
    assert_eq!(session.last_hit_triangle.get(), Some(2));
    assert_eq!(
        session.last_hit_view_direction.get(),
        Some([0.0, 0.0, -1.0])
    );
    assert_eq!(
        session
            .last_hit_anchor
            .get()
            .map(|anchor| anchor.seed_triangle),
        Some(2)
    );
    assert!(session.is_applied());
    assert!(session.has_changes());
    assert_eq!(session.history_len(), 1);

    assert!(session.undo().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);
}

#[test]
fn stroke_history_is_bounded() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.set_x_symmetry(false);
    session.begin(&source).unwrap();
    for _ in 0..MAX_SCULPT_HISTORY + 7 {
        session.begin_stroke().unwrap();
        session
            .dab(SculptDab {
                center_local: [0.0, 0.0, 0.0],
                radius_local: 3.0,
                strength: 1.0,
                operation: SculptOperation::Grab {
                    translation_local: [0.001, 0.0, 0.0],
                },
            })
            .unwrap();
        session.end_stroke().unwrap();
    }
    assert_eq!(session.history_len(), MAX_SCULPT_HISTORY);
}

#[test]
fn raycast_rejects_disabled_front_surface_instead_of_tunneling() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();

    assert!(session.raycast([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]).is_none());
    session.set_target_enabled(SculptTarget::TeethTongue, true);
    let teeth = session.raycast([0.0, 0.0, 5.0], [0.0, 0.0, -1.0]).unwrap();
    assert_eq!(teeth.triangle_index, 2);
}

#[test]
fn raycast_switches_between_front_only_and_two_sided_picking() {
    let source = isolated_targets();
    let mut session = SculptSession::default();

    session.set_backface_masking(true);
    session.begin(&source).unwrap();

    assert!(session.raycast([0.0, 0.0, -1.0], [0.0, 0.0, 1.0]).is_none());
    let skin = session.raycast([0.0, 0.0, 1.0], [0.0, 0.0, -1.0]).unwrap();
    assert_eq!(skin.triangle_index, 0);

    session.set_backface_masking(false);
    let skin_back = session.raycast([0.0, 0.0, -1.0], [0.0, 0.0, 1.0]).unwrap();
    assert_eq!(skin_back.triangle_index, 0);
}

#[test]
fn grab_uses_morphed_presentation_coordinates_but_writes_authored_delta() {
    let source = isolated_targets();
    let mut presentation = source.vertices.clone();
    for vertex in &mut presentation {
        vertex[0] += 10.0;
    }
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_x_symmetry(false);
    session.set_presentation_vertices(&presentation).unwrap();

    assert!(
        session
            .raycast([10.0, 0.0, 1.0], [0.0, 0.0, -1.0])
            .is_some()
    );
    assert!(session.raycast([0.0, 0.0, 1.0], [0.0, 0.0, -1.0]).is_none());
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: [10.0, 0.0, 0.0],
            radius_local: 2.0,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.5, 0.0, 0.0],
            },
        })
        .unwrap();

    let working = &session.working_mesh().unwrap().vertices;
    let visible = session.presentation_vertices.as_ref().unwrap();
    assert_ne!(&working[..3], &source.vertices[..3]);
    for index in 0..3 {
        let authored_delta =
            DVec3::from_array(working[index]) - DVec3::from_array(source.vertices[index]);
        let visible_delta =
            DVec3::from_array(visible[index]) - DVec3::from_array(presentation[index]);
        assert!(authored_delta.distance(visible_delta) < 1.0e-12);
    }

    session.undo().unwrap();
    assert_eq!(session.working_mesh().unwrap().vertices, source.vertices);
    assert_eq!(
        session.presentation_vertices.as_ref().unwrap(),
        &presentation
    );
}

#[test]
fn raycast_uses_the_stroke_captured_backface_setting() {
    let source = isolated_targets();
    let mut session = SculptSession::default();

    session.set_backface_masking(true);
    session.begin(&source).unwrap();

    session.raycast([0.0, 0.0, 1.0], [0.0, 0.0, -1.0]).unwrap();
    session.begin_stroke().unwrap();
    session.set_backface_masking(false);
    assert!(session.raycast([0.0, 0.0, -1.0], [0.0, 0.0, 1.0]).is_none());
    assert!(!session.end_stroke().unwrap());

    session.set_backface_masking(false);
    session.raycast([0.0, 0.0, -1.0], [0.0, 0.0, 1.0]).unwrap();
    session.begin_stroke().unwrap();
    session.set_backface_masking(true);
    assert!(session.raycast([0.0, 0.0, -1.0], [0.0, 0.0, 1.0]).is_some());
    assert!(!session.end_stroke().unwrap());
}

#[test]
fn two_sided_inactive_visible_surface_still_blocks_selection_through() {
    let source = OrderedObjMesh {
        vertices: vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, -1.0, 0.1],
            [1.0, -1.0, 0.1],
            [0.0, 1.0, 0.1],
        ],
        faces: vec![face(&[0, 1, 2], "Tear"), face(&[3, 4, 5], "Face")],
    };
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_backface_masking(false);

    assert!(
        session
            .raycast_visible([0.0, 0.0, -1.0], [0.0, 0.0, 1.0], SculptTargets::ALL)
            .is_none()
    );
    let skin = session
        .raycast_visible([0.0, 0.0, -1.0], [0.0, 0.0, 1.0], SculptTargets::HEAD_SKIN)
        .unwrap();
    assert_eq!(skin.triangle_index, 1);
}

#[test]
fn geodesic_brush_does_not_bleed_to_close_disconnected_fold() {
    let source = OrderedObjMesh {
        vertices: vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, -1.0, 0.05],
            [1.0, -1.0, 0.05],
            [0.0, 1.0, 0.05],
        ],
        faces: vec![face(&[0, 1, 2], "Face"), face(&[3, 4, 5], "Face")],
    };
    for backface_masking in [true, false] {
        let mut session = SculptSession::default();
        session.set_x_symmetry(false);
        session.begin(&source).unwrap();
        session.set_backface_masking(backface_masking);

        let hit = session.raycast([0.0, 0.0, 0.02], [0.0, 0.0, -1.0]).unwrap();
        assert_eq!(hit.triangle_index, 0);
        session.begin_stroke().unwrap();
        session
            .dab(SculptDab {
                center_local: hit.point_local,
                radius_local: 3.0,
                strength: 1.0,
                operation: SculptOperation::Grab {
                    translation_local: [0.25, 0.0, 0.0],
                },
            })
            .unwrap();
        session.end_stroke().unwrap();
        let working = session.working_mesh().unwrap();
        assert_ne!(&working.vertices[..3], &source.vertices[..3]);
        assert_ne!(&working.vertices[3..], &source.vertices[3..]);
    }
}

#[test]
fn masking_off_reaches_spatially_close_hidden_hairpin_on_same_component() {
    let source = hairpin_fold();
    let sculpt = |backface_masking| {
        let mut session = SculptSession::default();
        session.begin(&source).unwrap();
        session.set_backface_masking(backface_masking);
        session
            .begin_stroke_with_directions(Some([0.0, 0.0, -1.0]), None)
            .unwrap();
        session
            .dab(SculptDab {
                center_local: [0.0, 0.0, 0.0],
                radius_local: 0.8,
                strength: 1.0,
                operation: SculptOperation::Grab {
                    translation_local: [0.0, 0.0, 0.25],
                },
            })
            .unwrap();
        session.working_mesh().unwrap().vertices.clone()
    };

    let masked = sculpt(true);
    let unmasked = sculpt(false);
    assert_eq!(&masked[6..10], &source.vertices[6..10]);
    assert_ne!(&unmasked[6..10], &source.vertices[6..10]);
    assert_eq!(&unmasked[4..6], &source.vertices[4..6]);
}

#[test]
fn nostrils_follow_head_skin_while_inner_mouth_requires_internal_target() {
    assert_eq!(
        target_bits_for_label("Nostrils"),
        SculptTarget::HeadSkin as u8
    );
    assert_eq!(
        target_bits_for_label("InnerMouth"),
        SculptTarget::InnerMouth as u8
    );
}

#[test]
fn apply_commit_is_explicit_and_preserves_history_until_install_succeeds() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.set_x_symmetry(false);
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: [0.0, 0.0, 0.0],
            radius_local: 3.0,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.1, 0.0, 0.0],
            },
        })
        .unwrap();
    let _candidate = session.prepare_apply().unwrap();
    assert!(!session.is_applied());
    assert!(session.has_changes());
    assert_eq!(session.history_len(), 0);

    session.mark_applied();
    assert!(session.is_applied());
    assert_eq!(session.history_len(), 1);

    assert!(session.undo().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);
    assert!(!session.has_changes());
    assert!(!session.is_applied());
}

#[test]
fn changing_target_does_not_make_nearer_disabled_surface_pickable() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_target_enabled(SculptTarget::HeadSkin, false);
    session.set_target_enabled(SculptTarget::Eyes, true);

    let eye = session.raycast([0.0, 0.0, 3.0], [0.0, 0.0, -1.0]).unwrap();
    assert_eq!(eye.triangle_index, 1);
}

#[test]
fn hidden_optional_shell_does_not_occlude_visible_head_skin() {
    let source = OrderedObjMesh {
        vertices: vec![
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, -1.0, 0.1],
            [1.0, -1.0, 0.1],
            [0.0, 1.0, 0.1],
        ],
        faces: vec![face(&[0, 1, 2], "Face"), face(&[3, 4, 5], "Tear")],
    };
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();

    assert!(
        session
            .raycast_visible([0.0, 0.0, 1.0], [0.0, 0.0, -1.0], SculptTargets::ALL,)
            .is_none()
    );

    let skin = session
        .raycast_visible([0.0, 0.0, 1.0], [0.0, 0.0, -1.0], SculptTargets::HEAD_SKIN)
        .unwrap();
    assert_eq!(skin.triangle_index, 0);
}

#[test]
fn brush_radius_press_selects_actual_near_ray_lash_deterministically() {
    let source = OrderedObjMesh {
        vertices: vec![
            [0.18, -0.1, 0.0],
            [0.38, -0.1, 0.0],
            [0.18, 0.1, 0.0],
            [-0.18, -0.1, 0.0],
            [-0.18, 0.1, 0.0],
            [-0.38, -0.1, 0.0],
        ],
        faces: vec![face(&[0, 1, 2], "Eyelashes"), face(&[3, 4, 5], "Eyelashes")],
    };
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_target_enabled(SculptTarget::HeadSkin, false);
    session.set_target_enabled(SculptTarget::Eyelashes, true);

    assert!(
        session
            .raycast_visible([0.0, 0.0, 1.0], [0.0, 0.0, -1.0], SculptTargets::ALL)
            .is_none()
    );
    assert!(
        session
            .raycast_visible_with_brush_radius(
                [0.0, 0.0, 1.0],
                [0.0, 0.0, -1.0],
                SculptTargets::ALL,
                0.17,
            )
            .is_none()
    );
    let hit = session
        .raycast_visible_with_brush_radius(
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            SculptTargets::ALL,
            0.2,
        )
        .unwrap();
    assert_eq!(hit.triangle_index, 0, "triangle id breaks an exact tie");
    assert!((hit.point_local[0] - 0.18).abs() < 1.0e-12);
    assert!(hit.point_local[1].abs() < 1.0e-12);
    assert!(hit.point_local[2].abs() < 1.0e-12);
    assert!((hit.distance - 1.0).abs() < 1.0e-12);
    assert_eq!(hit.normal_local, [0.0, 0.0, 1.0]);

    let anchor = session.last_hit_anchor.get().unwrap();
    assert_eq!(anchor.point.to_array(), hit.point_local);
    assert_eq!(anchor.normal.to_array(), hit.normal_local);
    assert_eq!(anchor.seed_triangle, hit.triangle_index);
    session.begin_stroke().unwrap();
    assert_eq!(
        session.active_stroke.as_ref().unwrap().seed_triangle,
        Some(hit.triangle_index)
    );
}

#[test]
fn brush_radius_press_never_overrides_an_exact_visible_hit() {
    let source = OrderedObjMesh {
        vertices: vec![
            [0.18, -0.1, 0.5],
            [0.38, -0.1, 0.5],
            [0.18, 0.1, 0.5],
            [-0.1, -0.1, 0.0],
            [0.1, -0.1, 0.0],
            [0.0, 0.1, 0.0],
        ],
        faces: vec![face(&[0, 1, 2], "Eyelashes"), face(&[3, 4, 5], "Face")],
    };
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_target_enabled(SculptTarget::Eyelashes, true);
    let hit = session
        .raycast_visible_with_brush_radius(
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            SculptTargets::ALL,
            1.0,
        )
        .unwrap();
    assert_eq!(hit.triangle_index, 1);
    assert!((hit.distance - 1.0).abs() < 1.0e-12);
}

#[test]
fn grab_admits_a_layered_lash_component_without_dropping_its_back_root() {
    let source = layered_lash_components();
    let mut session = SculptSession::default();

    session.set_backface_masking(true);
    session.set_x_symmetry(false);
    session.set_falloff_preset(SculptFalloff::Linear);
    session.begin(&source).unwrap();
    session.set_target_enabled(SculptTarget::HeadSkin, false);
    session.set_target_enabled(SculptTarget::Tear, true);
    session.set_target_enabled(SculptTarget::Eyelashes, true);
    let hit = session
        .raycast_visible([0.0, 0.0, 1.0], [0.0, 0.0, -1.0], SculptTargets::TEAR)
        .unwrap();
    session.begin_stroke().unwrap();
    let radius = 1.5;
    let strength = 0.5;
    session
        .dab(SculptDab {
            center_local: hit.point_local,
            radius_local: radius,
            strength,
            operation: SculptOperation::Grab {
                translation_local: [0.0, 1.0, 0.0],
            },
        })
        .unwrap();

    let working = &session.working_mesh().unwrap().vertices;
    for (moved, original) in working[3..=6].iter().zip(&source.vertices[3..=6]) {
        assert_ne!(moved, original);
    }

    let anchor = DVec3::from_array(source.vertices[2]);
    let root_distance = DVec3::from_array(source.vertices[6]).distance(anchor);
    let expected_root_delta = SculptFalloff::Linear.weight(root_distance / radius) * strength;
    let actual_root_delta = working[6][1] - source.vertices[6][1];
    assert!(
        (actual_root_delta - expected_root_delta).abs() < 1.0e-12,
        "{actual_root_delta} should be {expected_root_delta}"
    );
    assert_eq!(
        &working[7..10],
        &source.vertices[7..10],
        "a wholly back-facing disconnected shell stays protected"
    );
}

#[test]
fn connected_only_is_captured_at_press_for_disconnected_grab() {
    let source = nearby_independent_eye_shells();
    let sculpt = |connected_at_press: bool, toggle_after_press: bool| {
        let mut session = SculptSession::default();
        session.set_x_symmetry(false);
        session.set_connected_topology_only(connected_at_press);
        session.begin(&source).unwrap();
        session.set_target_enabled(SculptTarget::HeadSkin, false);
        session.set_target_enabled(SculptTarget::Tear, true);
        session.set_target_enabled(SculptTarget::Eyelashes, true);
        let hit = session
            .raycast_visible([-2.0, 0.0, 1.0], [0.0, 0.0, -1.0], SculptTargets::TEAR)
            .unwrap();
        session.begin_stroke().unwrap();
        session.set_connected_topology_only(toggle_after_press);
        session
            .dab(SculptDab {
                center_local: hit.point_local,
                radius_local: 2.5,
                strength: 1.0,
                operation: SculptOperation::Grab {
                    translation_local: [0.0, 0.25, 0.0],
                },
            })
            .unwrap();
        session.working_mesh().unwrap().vertices.clone()
    };

    let captured_off = sculpt(false, true);
    assert_ne!(&captured_off[0..3], &source.vertices[0..3]);
    assert_ne!(&captured_off[3..6], &source.vertices[3..6]);
    let captured_on = sculpt(true, false);
    assert_ne!(&captured_on[0..3], &source.vertices[0..3]);
    assert_eq!(&captured_on[3..6], &source.vertices[3..6]);
}

#[test]
fn symmetric_grab_admits_matching_disconnected_eye_components_proportionately() {
    let source = nearby_independent_eye_shells();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_target_enabled(SculptTarget::HeadSkin, false);
    session.set_target_enabled(SculptTarget::Tear, true);
    session.set_target_enabled(SculptTarget::Eyelashes, true);
    session
        .begin_stroke_with_directions(Some([0.0, 0.0, -1.0]), None)
        .unwrap();
    session
        .dab(SculptDab {
            center_local: [-2.0, 0.0, 0.0],
            radius_local: 2.5,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.2, 0.1, 0.0],
            },
        })
        .unwrap();

    let working = &session.working_mesh().unwrap().vertices;
    for (left, right) in [(0, 7), (1, 6), (2, 8), (3, 10), (4, 9), (5, 11)] {
        let left_delta =
            DVec3::from_array(working[left]) - DVec3::from_array(source.vertices[left]);
        let right_delta =
            DVec3::from_array(working[right]) - DVec3::from_array(source.vertices[right]);
        assert!((left_delta.x + right_delta.x).abs() < 1.0e-12);
        assert!((left_delta.y - right_delta.y).abs() < 1.0e-12);
        assert!((left_delta.z - right_delta.z).abs() < 1.0e-12);
    }
    assert_ne!(&working[3..6], &source.vertices[3..6]);
    assert_ne!(&working[9..12], &source.vertices[9..12]);
}

#[test]
fn grab_keeps_the_first_dab_selection_and_weights_for_the_whole_stroke() {
    let source = OrderedObjMesh {
        vertices: vec![
            [0.0, 0.0, 0.0],
            [0.9, 0.0, 0.0],
            [0.0, 0.9, 0.0],
            [3.0, 0.0, 0.0],
        ],
        faces: vec![face(&[0, 1, 2], "Face"), face(&[1, 3, 2], "Face")],
    };
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_falloff_preset(SculptFalloff::Sharp);
    session.begin_stroke().unwrap();
    let dab = SculptDab {
        center_local: [0.0, 0.0, 0.0],
        radius_local: 1.0,
        strength: 1.0,
        operation: SculptOperation::Grab {
            translation_local: [1.0, 0.0, 0.0],
        },
    };
    session.dab(dab).unwrap();
    let after_first = session.working_mesh().unwrap().vertices.clone();
    session.set_falloff_preset(SculptFalloff::Linear);

    session
        .dab(SculptDab {
            center_local: [100.0, 100.0, 100.0],
            radius_local: 200.0,
            strength: 0.1,
            operation: dab.operation,
        })
        .unwrap();
    let after_second = &session.working_mesh().unwrap().vertices;

    for index in 0..source.vertices.len() {
        let first_delta = after_first[index][0] - source.vertices[index][0];
        let second_delta = after_second[index][0] - after_first[index][0];
        assert!(
            (first_delta - second_delta).abs() < 1.0e-12,
            "grab weight changed for vertex {index}: {first_delta} vs {second_delta}"
        );
    }
    assert_eq!(after_second[3], source.vertices[3]);
}

#[test]
fn grab_moves_nearby_unlocked_tear_and_eyelashes_together() {
    let source = nearby_independent_eye_shells();
    let mut session = SculptSession::default();
    session.set_x_symmetry(false);
    session.begin(&source).unwrap();
    session.set_target_enabled(SculptTarget::HeadSkin, false);
    session.set_target_enabled(SculptTarget::Tear, true);
    session.set_target_enabled(SculptTarget::Eyelashes, true);
    session.begin_stroke().unwrap();
    let changed = session
        .dab(SculptDab {
            center_local: [-2.0, 0.0, 0.0],
            radius_local: 2.5,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.0, 0.25, 0.0],
            },
        })
        .unwrap();
    session.end_stroke().unwrap();

    assert_eq!(changed, 6);
    let working = &session.working_mesh().unwrap().vertices;
    assert_ne!(&working[0..3], &source.vertices[0..3]);
    assert_ne!(&working[3..6], &source.vertices[3..6]);
    assert_eq!(&working[6..], &source.vertices[6..]);
}

#[test]
fn grab_keeps_the_first_independent_group_membership_for_the_whole_stroke() {
    let source = nearby_independent_eye_shells();
    let mut session = SculptSession::default();
    session.set_x_symmetry(false);
    session.begin(&source).unwrap();
    session.set_target_enabled(SculptTarget::HeadSkin, false);
    session.set_target_enabled(SculptTarget::Tear, true);
    session.set_target_enabled(SculptTarget::Eyelashes, true);
    session.begin_stroke().unwrap();
    let operation = SculptOperation::Grab {
        translation_local: [0.0, 0.1, 0.0],
    };
    session
        .dab(SculptDab {
            center_local: [-2.0, 0.0, 0.0],
            radius_local: 2.5,
            strength: 1.0,
            operation,
        })
        .unwrap();
    let after_first = session.working_mesh().unwrap().vertices.clone();
    assert_ne!(&after_first[3..6], &source.vertices[3..6]);
    assert_eq!(&after_first[6..], &source.vertices[6..]);
    session
        .dab(SculptDab {
            center_local: [2.0, 0.0, 0.0],
            radius_local: 100.0,
            strength: 0.1,
            operation,
        })
        .unwrap();

    let after_second = &session.working_mesh().unwrap().vertices;
    assert_eq!(&after_second[6..], &source.vertices[6..]);
    for index in 0..6 {
        let first_delta =
            DVec3::from_array(after_first[index]) - DVec3::from_array(source.vertices[index]);
        let second_delta =
            DVec3::from_array(after_second[index]) - DVec3::from_array(after_first[index]);
        assert!(first_delta.distance(second_delta) < 1.0e-12);
    }
}

#[test]
fn locked_target_stays_protected_from_nearby_grab() {
    let source = nearby_independent_eye_shells();
    let mut session = SculptSession::default();
    session.set_x_symmetry(false);
    session.begin(&source).unwrap();
    session.set_target_enabled(SculptTarget::HeadSkin, false);
    session.set_target_enabled(SculptTarget::Tear, true);
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: [-2.0, 0.0, 0.0],
            radius_local: 2.5,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.0, 0.25, 0.0],
            },
        })
        .unwrap();

    let working = &session.working_mesh().unwrap().vertices;
    assert_ne!(&working[0..3], &source.vertices[0..3]);
    assert_eq!(&working[3..6], &source.vertices[3..6]);
    assert_eq!(&working[6..], &source.vertices[6..]);
}

#[test]
fn large_grab_reaches_a_disconnected_enabled_group() {
    let source = OrderedObjMesh {
        vertices: vec![
            [-3.0, -1.0, 0.0],
            [-1.0, -1.0, 0.0],
            [-2.0, 1.0, 0.0],
            [1.0, -1.0, 0.1],
            [3.0, -1.0, 0.1],
            [2.0, 1.0, 0.1],
        ],
        faces: vec![face(&[0, 1, 2], "Tear"), face(&[3, 4, 5], "Eyelashes")],
    };
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_target_enabled(SculptTarget::HeadSkin, false);
    session.set_target_enabled(SculptTarget::Tear, true);
    session.set_target_enabled(SculptTarget::Eyelashes, true);
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: [-2.0, 0.0, 0.0],

            radius_local: 1_000.0,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.0, 0.25, 0.0],
            },
        })
        .unwrap();

    let working = &session.working_mesh().unwrap().vertices;
    assert_ne!(&working[..3], &source.vertices[..3]);
    assert_ne!(&working[3..], &source.vertices[3..]);
}

#[test]
fn repeated_smooth_preserves_independent_open_shells() {
    let mut source = nearby_independent_eye_shells();

    source.vertices.truncate(6);
    source.faces.truncate(2);
    for vertex in &mut source.vertices {
        vertex[0] += 2.0;
    }
    let initial_area = triangle_area(&source.vertices, [0, 1, 2]);

    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_target_enabled(SculptTarget::HeadSkin, false);
    session.set_target_enabled(SculptTarget::Tear, true);
    session.set_target_enabled(SculptTarget::Eyelashes, true);
    session.begin_stroke().unwrap();
    let mut changed = 0;
    for _ in 0..64 {
        changed += session
            .dab(SculptDab {
                center_local: [0.0, 0.0, 0.0],
                radius_local: 10.0,
                strength: 1.0,
                operation: SculptOperation::Smooth,
            })
            .unwrap();
    }

    let working = &session.working_mesh().unwrap().vertices;
    let smoothed_area = triangle_area(working, [0, 1, 2]);
    assert!(
        (smoothed_area - initial_area).abs() < 1.0e-12,
        "an open-shell boundary changed area: {smoothed_area} / {initial_area}"
    );
    assert_eq!(changed, 0, "a boundary-only triangle must remain pinned");
    assert_eq!(working, &source.vertices);
    for index in 0..3 {
        let gap = DVec3::from_array(working[index + 3]) - DVec3::from_array(working[index]);
        assert!(gap.distance(DVec3::new(0.0, 0.0, 0.1)) < 1.0e-12);
    }
}

#[test]
fn repeated_smooth_preserves_flat_open_boundary_and_projected_area() {
    let source = flat_irregular_grid();
    let topology = SculptTopology::build(&source).unwrap();
    let initial_area = topology
        .triangles
        .iter()
        .map(|&triangle| triangle_double_area(triangle, &source.vertices) * 0.5)
        .sum::<f64>();
    let initial_center_distance = DVec3::from_array(source.vertices[4]).length();

    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();
    for _ in 0..64 {
        session
            .dab(SculptDab {
                center_local: [0.0, 0.0, 0.0],
                radius_local: 10.0,
                strength: 1.0,
                operation: SculptOperation::Smooth,
            })
            .unwrap();
    }

    let working = &session.working_mesh().unwrap().vertices;
    for index in [0, 1, 2, 3, 5, 6, 7, 8] {
        assert_eq!(
            working[index], source.vertices[index],
            "open boundary vertex {index} drifted"
        );
    }
    let smoothed_area = topology
        .triangles
        .iter()
        .map(|&triangle| triangle_double_area(triangle, working) * 0.5)
        .sum::<f64>();
    assert!(
        (smoothed_area - initial_area).abs() < 1.0e-10,
        "flat open-shell area drifted: {smoothed_area} / {initial_area}"
    );
    assert!(
        DVec3::from_array(working[4]).length() < initial_center_distance * 0.75,
        "boundary protection also prevented useful interior relaxation"
    );
}

#[test]
fn separate_smooth_strokes_share_one_session_area_floor_and_still_reduce_roughness() {
    let source = closed_tetrahedron();
    let topology = SculptTopology::build(&source).unwrap();
    let initial_areas = topology
        .triangles
        .iter()
        .map(|&triangle| triangle_double_area(triangle, &source.vertices))
        .collect::<Vec<_>>();
    let initial_volume = signed_volume(&source.vertices, &topology.triangles).abs();

    let mut session = SculptSession::default();
    session.begin(&source).unwrap();

    session.set_backface_masking(false);
    for _ in 0..16 {
        session.begin_stroke().unwrap();
        for _ in 0..8 {
            session
                .dab(SculptDab {
                    center_local: [0.0, 0.0, 0.0],
                    radius_local: 10.0,
                    strength: 1.0,
                    operation: SculptOperation::Smooth,
                })
                .unwrap();
        }
        session.end_stroke().unwrap();
    }

    let working = &session.working_mesh().unwrap().vertices;
    assert_ne!(working, &source.vertices, "the safeguard blocked every dab");
    for (&triangle, initial_area) in topology.triangles.iter().zip(initial_areas) {
        let area = triangle_double_area(triangle, working);
        assert!(
            area + 1.0e-10 >= initial_area * MIN_SMOOTH_STROKE_AREA_RATIO,
            "fixed stroke area floor ratcheted down: {area} / {initial_area}"
        );
    }
    let volume = signed_volume(working, &topology.triangles).abs();
    assert!(
        volume + 1.0e-10 >= initial_volume * MIN_SMOOTH_STROKE_AREA_RATIO.powf(1.5),
        "separate strokes collapsed closed volume: {volume} / {initial_volume}"
    );

    let rough = noisy_grid();
    session.begin(&rough).unwrap();
    session.set_backface_masking(true);
    for _ in 0..4 {
        session.begin_stroke().unwrap();
        for _ in 0..2 {
            session
                .dab(SculptDab {
                    center_local: [0.0, 0.0, 0.8],
                    radius_local: 4.0,
                    strength: 0.5,
                    operation: SculptOperation::Smooth,
                })
                .unwrap();
        }
        session.end_stroke().unwrap();
    }
    let smoothed = &session.working_mesh().unwrap().vertices;
    assert!(
        smoothed[4][2] < rough.vertices[4][2] - 0.05,
        "persistent floor prevented useful roughness removal"
    );
    for index in [0, 1, 2, 3, 5, 6, 7, 8] {
        assert_eq!(smoothed[index], rough.vertices[index]);
    }
}

#[test]
fn non_smooth_rebase_is_local_and_undo_restores_the_reference_transaction() {
    let source = closed_tetrahedron();
    let topology = SculptTopology::build(&source).unwrap();
    let original_references = topology
        .triangles
        .iter()
        .enumerate()
        .map(|(index, &triangle)| {
            (
                index as u32,
                triangle_double_area(triangle, &source.vertices),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_backface_masking(false);

    session.smooth_reference_areas = original_references.clone();

    session.begin_stroke().unwrap();
    let changed = session
        .dab(SculptDab {
            center_local: source.vertices[0],
            radius_local: 0.1,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [-1.3, -1.3, -1.3],
            },
        })
        .unwrap();
    assert_eq!(changed, 1);
    session.end_stroke().unwrap();

    let deformed = session.working_mesh().unwrap().vertices.clone();
    for &triangle in &topology.incident_triangles[0] {
        let current_area = triangle_double_area(topology.triangles[triangle as usize], &deformed);
        assert_eq!(session.smooth_reference_areas[&triangle], current_area);
        assert!(
            current_area < original_references[&triangle] * MIN_SMOOTH_STROKE_AREA_RATIO,
            "test deformation did not cross the stale Smooth floor"
        );
    }

    let untouched = 3_u32;
    assert!(!topology.incident_triangles[0].contains(&untouched));
    assert_eq!(
        session.smooth_reference_areas[&untouched],
        original_references[&untouched]
    );

    let influence = weighted_sculpt_influence(
        &deformed,
        &topology,
        DVec3::ZERO,
        0,
        InfluenceSettings {
            radius: 10.0,
            strength: 1.0,
            targets: SculptTargets::HEAD_SKIN,
            falloff: SculptFalloff::Smooth,
            backface_masking: false,
            incoming_direction: None,
            include_nearby_components: false,
        },
    );
    let stale = jacobi_smooth_proposals(
        &deformed,
        &topology,
        SculptTargets::HEAD_SKIN,
        &influence,
        &original_references,
    );
    let rebased = jacobi_smooth_proposals(
        &deformed,
        &topology,
        SculptTargets::HEAD_SKIN,
        &influence,
        &session.smooth_reference_areas,
    );
    assert!(
        stale.is_empty(),
        "stale pre-Grab floors should reject Smooth"
    );
    assert!(
        !rebased.is_empty(),
        "rebased post-Grab floors should allow useful Smooth"
    );

    assert!(session.undo().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);
    assert_eq!(session.smooth_reference_areas, original_references);
}

#[test]
fn geodesic_brush_stays_on_seed_facing_sheet_across_a_sharp_fold() {
    let source = connected_fold();
    let mut session = SculptSession::default();

    session.set_backface_masking(true);
    session.begin(&source).unwrap();
    let hit = session.raycast([0.2, 0.2, 1.0], [0.0, 0.0, -1.0]).unwrap();
    assert_eq!(hit.triangle_index, 0);
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: hit.point_local,
            radius_local: 4.0,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.0, 0.0, 0.5],
            },
        })
        .unwrap();
    let working = session.working_mesh().unwrap();
    assert_ne!(working.vertices[2], source.vertices[2]);
    assert_eq!(working.vertices[3], source.vertices[3]);
}

#[test]
fn disabling_backface_masking_allows_a_connected_occluded_fold_to_participate() {
    let source = connected_fold();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_backface_masking(false);
    let hit = session.raycast([0.2, 0.2, 1.0], [0.0, 0.0, -1.0]).unwrap();
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: hit.point_local,
            radius_local: 4.0,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.0, 0.0, 0.5],
            },
        })
        .unwrap();
    assert_ne!(
        session.working_mesh().unwrap().vertices[3],
        source.vertices[3]
    );
}

#[test]
fn explicit_brush_direction_overrides_view_direction_for_facing() {
    let source = connected_fold();
    let mut session = SculptSession::default();

    session.set_backface_masking(true);
    session.begin(&source).unwrap();

    session
        .begin_stroke_with_directions(Some([0.0, 0.0, 1.0]), Some([0.0, 0.0, -1.0]))
        .unwrap();
    let changed = session
        .dab(SculptDab {
            center_local: [0.2, 0.2, 0.0],
            radius_local: 4.0,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.0, 0.0, 0.25],
            },
        })
        .unwrap();
    assert_eq!(changed, 3);
    assert_eq!(
        session.working_mesh().unwrap().vertices[3],
        source.vertices[3]
    );
}

#[test]
fn jacobi_smooth_outperforms_legacy_filter_and_repeated_dabs_keep_orientation() {
    assert_eq!(smooth_pass_relaxation(0.5, 0.4, 0), 0.4);
    assert_eq!(smooth_pass_relaxation(0.5, 0.4, 1), 0.0);
    assert_eq!(smooth_pass_relaxation(1.0, 0.4, 3), 0.4);

    for high_frequency in [false, true] {
        let source = frequency_grid(9, high_frequency);
        let topology = SculptTopology::build(&source).unwrap();
        let influence = full_strength_head_influence(&source);
        let legacy = apply_proposals(
            &source.vertices,
            &legacy_taubin_smooth_proposals(&source.vertices, &topology, &influence),
        );
        let jacobi = apply_proposals(
            &source.vertices,
            &jacobi_smooth_proposals(
                &source.vertices,
                &topology,
                SculptTargets::HEAD_SKIN,
                &influence,
                &BTreeMap::new(),
            ),
        );
        let jacobi_rms = rms_height(&jacobi);
        let legacy_rms = rms_height(&legacy);
        let original_rms = rms_height(&source.vertices);

        assert!(
            jacobi_rms < legacy_rms,
            "{} roughness: jacobi {jacobi_rms} did not beat legacy {legacy_rms}",
            if high_frequency {
                "high-frequency"
            } else {
                "broad"
            }
        );
        if high_frequency {
            assert!(
                jacobi_rms < legacy_rms * 0.8,
                "high-frequency roughness barely damped: {jacobi_rms} vs {legacy_rms}"
            );
        } else {
            assert!(
                jacobi_rms > original_rms * 0.75,
                "broad form deflated too far: {jacobi_rms} vs original {original_rms}"
            );
        }
    }

    let source = frequency_grid(9, true);
    let topology = SculptTopology::build(&source).unwrap();
    let mut session = SculptSession::default();
    session.set_x_symmetry(false);
    session.set_backface_masking(false);
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();
    for _ in 0..24 {
        let before = session.working_mesh().unwrap().vertices.clone();
        session
            .dab(SculptDab {
                center_local: [0.0, 0.0, 0.0],
                radius_local: 20.0,
                strength: 1.0,
                operation: SculptOperation::Smooth,
            })
            .unwrap();
        let after = &session.working_mesh().unwrap().vertices;
        for &triangle in &topology.triangles {
            let [old_a, old_b, old_c] =
                triangle.map(|index| DVec3::from_array(before[index as usize]));
            let [new_a, new_b, new_c] =
                triangle.map(|index| DVec3::from_array(after[index as usize]));
            let old_cross = (old_b - old_a).cross(old_c - old_a);
            let new_cross = (new_b - new_a).cross(new_c - new_a);
            assert!(new_cross.is_finite());
            assert!(new_cross.dot(old_cross) > 0.0, "Smooth inverted a face");
        }
    }
}

#[test]
fn smooth_strength_is_effective_and_repeated_dabs_resist_footprint_shrinkage() {
    let source = noisy_grid();
    let smooth_once = |strength| {
        let mut session = SculptSession::default();
        session.begin(&source).unwrap();
        session.begin_stroke().unwrap();
        session
            .dab(SculptDab {
                center_local: [0.0, 0.0, 0.8],
                radius_local: 4.0,
                strength,
                operation: SculptOperation::Smooth,
            })
            .unwrap();
        session.working_mesh().unwrap().vertices[4][2]
    };
    let low = smooth_once(0.2);
    let high = smooth_once(1.0);
    assert!(
        high < low - 1.0e-3,
        "strength had no visible effect: {low} vs {high}"
    );
    assert!(
        source.vertices[4][2] - high > 0.20,
        "maximum Smooth is still only a subtle polish: {high}"
    );

    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();
    for _ in 0..8 {
        session
            .dab(SculptDab {
                center_local: [0.0, 0.0, 0.8],
                radius_local: 4.0,
                strength: 0.5,
                operation: SculptOperation::Smooth,
            })
            .unwrap();
    }
    let vertices = &session.working_mesh().unwrap().vertices;
    let (minimum_x, maximum_x) = vertices.iter().fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(minimum, maximum), vertex| (minimum.min(vertex[0]), maximum.max(vertex[0])),
    );
    assert!(vertices[4][2] < source.vertices[4][2] - 0.05);
    assert!(
        maximum_x - minimum_x > 1.4,
        "repeated smoothing collapsed the surface footprint: {}",
        maximum_x - minimum_x
    );
}

#[test]
fn one_degenerate_patch_does_not_weaken_safe_smoothing_elsewhere() {
    let source = OrderedObjMesh {
        vertices: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [3.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [3.0, 1.0, 0.0],
        ],
        faces: vec![face(&[0, 1, 2], "Face"), face(&[3, 4, 5], "Face")],
    };
    let topology = SculptTopology::build(&source).unwrap();
    let reference_areas = topology
        .triangles
        .iter()
        .enumerate()
        .map(|(index, &triangle)| {
            (
                index as u32,
                triangle_double_area(triangle, &source.vertices),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let proposals = backtrack_smooth_proposals(
        &source.vertices,
        &topology,
        vec![(2, [0.0, 0.0, 0.0]), (5, [3.0, 0.8, 0.0])],
        &reference_areas,
    );
    let first = proposals
        .iter()
        .find(|(index, _)| *index == 2)
        .map(|(_, point)| *point)
        .unwrap();
    let second = proposals
        .iter()
        .find(|(index, _)| *index == 5)
        .map(|(_, point)| *point)
        .unwrap();

    assert!(first[1] >= MIN_SMOOTH_STROKE_AREA_RATIO);
    assert_eq!(second, [3.0, 0.8, 0.0]);
}

#[test]
fn smooth_neighbor_average_never_pulls_toward_a_sharp_back_sheet() {
    let source = connected_fold();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    let hit = session.raycast([0.2, 0.2, 1.0], [0.0, 0.0, -1.0]).unwrap();
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: hit.point_local,
            radius_local: 4.0,
            strength: 1.0,
            operation: SculptOperation::Smooth,
        })
        .unwrap();

    let working = &session.working_mesh().unwrap().vertices;
    assert_eq!(working[3], source.vertices[3]);
    for vertex in &working[..3] {
        assert!(
            vertex[2].abs() < 1.0e-12,
            "front sheet was pulled toward the back fold: {vertex:?}"
        );
    }
}

#[test]
fn prepare_apply_does_not_finalize_an_active_stroke() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: [0.0, 0.0, 0.0],
            radius_local: 3.0,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.1, 0.0, 0.0],
            },
        })
        .unwrap();

    let _ = session.prepare_apply().unwrap();
    assert_eq!(session.history_len(), 0);
    assert!(session.undo().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);
}

#[test]
fn push_pull_anchors_first_hit_normal_membership_and_falloff_for_whole_stroke() {
    let source = symmetric_head_triangles();
    let mut session = SculptSession::default();
    session.set_x_symmetry(false);
    session.begin(&source).unwrap();
    let hit = session.raycast([-2.0, 0.0, 2.0], [0.0, 0.0, -1.0]).unwrap();
    assert!(DVec3::from_array(hit.normal_local).distance(DVec3::Z) < 1.0e-12);
    session.begin_stroke().unwrap();
    let first = session
        .dab(SculptDab {
            center_local: hit.point_local,
            radius_local: 3.0,
            strength: 1.0,
            operation: SculptOperation::Inflate { distance: 0.10 },
        })
        .unwrap();
    let second = session
        .dab(SculptDab {
            center_local: [2.0, 0.0, 0.0],
            radius_local: 100.0,
            strength: 0.1,
            operation: SculptOperation::Inflate { distance: 0.20 },
        })
        .unwrap();
    assert_eq!(first, 3);
    assert_eq!(second, 3);
    session.end_stroke().unwrap();
    let working = session.working_mesh().unwrap();
    for index in 0..3 {
        assert!(working.vertices[index][2] > source.vertices[index][2]);
        assert_eq!(working.vertices[index][0], source.vertices[index][0]);
        assert_eq!(working.vertices[index][1], source.vertices[index][1]);
    }
    assert_eq!(&working.vertices[3..], &source.vertices[3..]);
    assert_eq!(session.history_len(), 1);
    assert!(session.undo().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);
}

#[test]
fn x_symmetric_grab_uses_reflected_motion_and_press_time_setting() {
    let source = symmetric_head_triangles();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_x_symmetry(true);
    let hit = session.raycast([-2.0, 0.0, 2.0], [0.0, 0.0, -1.0]).unwrap();
    session.begin_stroke().unwrap();
    session.set_x_symmetry(false);
    let changed = session
        .dab(SculptDab {
            center_local: hit.point_local,
            radius_local: 3.0,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.4, 0.2, 0.1],
            },
        })
        .unwrap();
    assert_eq!(changed, 6);
    session.end_stroke().unwrap();
    let working = session.working_mesh().unwrap();
    for (left, right) in [(0, 3), (1, 4), (2, 5)] {
        let left_delta =
            DVec3::from_array(working.vertices[left]) - DVec3::from_array(source.vertices[left]);
        let right_delta =
            DVec3::from_array(working.vertices[right]) - DVec3::from_array(source.vertices[right]);
        assert!((right_delta.x + left_delta.x).abs() < 1.0e-12);
        assert!((right_delta.y - left_delta.y).abs() < 1.0e-12);
        assert!((right_delta.z - left_delta.z).abs() < 1.0e-12);
    }
    assert_eq!(session.history_len(), 1);
    assert!(session.undo().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);
}

#[test]
fn x_symmetric_smooth_resolves_an_independent_mirrored_patch() {
    let source = symmetric_noisy_patches();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_x_symmetry(true);
    let hit = session.raycast([-3.0, 0.0, 3.0], [0.0, 0.0, -1.0]).unwrap();
    session.begin_stroke().unwrap();
    let changed = session
        .dab(SculptDab {
            center_local: hit.point_local,
            radius_local: 3.0,
            strength: 1.0,
            operation: SculptOperation::Smooth,
        })
        .unwrap();
    assert!(changed >= 2);
    session.end_stroke().unwrap();
    let working = session.working_mesh().unwrap();
    assert!(working.vertices[4][2] < source.vertices[4][2] - 0.20);
    assert!((working.vertices[4][2] - working.vertices[13][2]).abs() < 1.0e-12);
    assert_eq!(session.history_len(), 1);
    assert!(session.undo().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);
}

#[test]
fn x_symmetry_deduplicates_centerline_overlap_without_cancelling_shared_motion() {
    let source = center_seam();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_x_symmetry(true);
    let hit = session.raycast([-0.7, 0.0, 1.0], [0.0, 0.0, -1.0]).unwrap();
    session.begin_stroke().unwrap();
    let changed = session
        .dab(SculptDab {
            center_local: hit.point_local,
            radius_local: 1.5,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.4, 0.2, 0.0],
            },
        })
        .unwrap();
    assert_eq!(changed, 5, "the shared seam vertex must be counted once");
    session.end_stroke().unwrap();
    let center = session.working_mesh().unwrap().vertices[0];
    assert!(center[0].abs() < 1.0e-12, "opposed X deltas must cancel");
    assert!(center[1] > 0.0, "shared Y motion must remain");
    assert_eq!(session.history_len(), 1);
    assert!(session.undo().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);
}

#[test]
fn operation_kind_is_immutable_until_pointer_release() {
    let source = isolated_targets();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.begin_stroke().unwrap();
    session
        .dab(SculptDab {
            center_local: [0.0, 0.0, 0.0],
            radius_local: 3.0,
            strength: 1.0,
            operation: SculptOperation::Grab {
                translation_local: [0.1, 0.0, 0.0],
            },
        })
        .unwrap();
    assert_eq!(
        session.dab(SculptDab {
            center_local: [0.0, 0.0, 0.0],
            radius_local: 3.0,
            strength: 1.0,
            operation: SculptOperation::Smooth,
        }),
        Err(SculptError::InvalidDab)
    );
}

fn mirrored_face_pair() -> OrderedObjMesh {
    OrderedObjMesh {
        vertices: vec![
            [0.6, -0.5, 0.0],
            [1.6, -0.5, 0.0],
            [1.1, 0.5, 0.0],
            [-0.6, -0.5, 0.0],
            [-1.6, -0.5, 0.0],
            [-1.1, 0.5, 0.0],
        ],
        faces: vec![face(&[0, 1, 2], "Face"), face(&[4, 3, 5], "Face")],
    }
}

#[test]
fn restore_dabs_converge_to_the_basis_with_mirror_and_one_step_undo() {
    let basis = mirrored_face_pair();
    let mut source = basis.clone();
    for vertex in &mut source.vertices {
        vertex[2] += 0.4;
    }
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    session.set_x_symmetry(true);
    session
        .set_restore_basis(Some(basis.vertices.clone()))
        .unwrap();
    assert!(session.has_restore_basis());

    let mut began = false;
    let mut last_changed = usize::MAX;
    for _ in 0..48 {
        let hit = session.raycast([1.1, 0.0, 5.0], [0.0, 0.0, -1.0]).unwrap();
        if !began {
            session.begin_stroke().unwrap();
            began = true;
        }
        last_changed = session
            .dab(SculptDab {
                center_local: hit.point_local,
                radius_local: 3.0,
                strength: 1.0,
                operation: SculptOperation::Restore,
            })
            .unwrap();
    }
    session.end_stroke().unwrap();

    let working = session.working_mesh().unwrap();
    for (restored, basis_point) in working.vertices.iter().zip(&basis.vertices) {
        for axis in 0..3 {
            assert!(
                (restored[axis] - basis_point[axis]).abs() < 1.0e-3,
                "restore must converge to the basis on both mirror sides: {restored:?} vs {basis_point:?}"
            );
        }
    }
    assert_eq!(
        last_changed, 0,
        "a converged surface must stop reporting changed vertices"
    );

    assert_eq!(session.history_len(), 1);
    assert!(session.undo().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);
}

#[test]
fn restore_respects_every_falloff_preset() {
    for falloff in [
        SculptFalloff::Smooth,
        SculptFalloff::Smoother,
        SculptFalloff::Sharp,
        SculptFalloff::Linear,
    ] {
        let basis = mirrored_face_pair();
        let mut source = basis.clone();
        for vertex in &mut source.vertices {
            vertex[2] += 0.4;
        }
        let mut session = SculptSession::default();
        session.begin(&source).unwrap();
        session.set_x_symmetry(false);
        session.set_falloff_preset(falloff);
        session
            .set_restore_basis(Some(basis.vertices.clone()))
            .unwrap();
        let hit = session.raycast([1.1, 0.0, 5.0], [0.0, 0.0, -1.0]).unwrap();
        session.begin_stroke().unwrap();
        let changed = session
            .dab(SculptDab {
                center_local: hit.point_local,
                radius_local: 3.0,
                strength: 0.8,
                operation: SculptOperation::Restore,
            })
            .unwrap();
        session.end_stroke().unwrap();
        assert!(changed > 0, "{falloff:?} must produce a restore step");
        let working = session.working_mesh().unwrap();
        for (index, (moved, original)) in working
            .vertices
            .iter()
            .zip(&source.vertices)
            .take(3)
            .enumerate()
        {
            assert!(
                moved[2] < original[2],
                "vertex {index} must move toward the basis under {falloff:?}"
            );
            assert!(moved[2] > basis.vertices[index][2] - 1.0e-9);
        }

        assert_eq!(&working.vertices[3..], &source.vertices[3..]);
    }
}

#[test]
fn restore_without_a_basis_is_inert_and_a_mismatched_basis_is_rejected() {
    let source = mirrored_face_pair();
    let mut session = SculptSession::default();
    session.begin(&source).unwrap();
    assert!(!session.has_restore_basis());

    session
        .set_restore_basis(Some(source.vertices.clone()))
        .unwrap();
    session.begin(&source).unwrap();
    assert!(!session.has_restore_basis());

    session.begin_stroke().unwrap();
    assert_eq!(
        session
            .dab(SculptDab {
                center_local: [1.1, 0.0, 0.0],
                radius_local: 3.0,
                strength: 1.0,
                operation: SculptOperation::Restore,
            })
            .unwrap(),
        0
    );
    assert!(!session.end_stroke().unwrap());
    assert_eq!(session.working_mesh().unwrap(), &source);

    assert_eq!(
        session.set_restore_basis(Some(vec![[0.0; 3]; 2])),
        Err(SculptError::InvalidMesh(
            "restore basis does not match the sculpt topology".to_owned()
        ))
    );
    assert!(!session.has_restore_basis());
}

#[test]
fn restore_may_heal_the_grab_protected_neck_loop_toward_the_basis() {
    let source = neck_and_feature_boundaries();
    let mut basis_vertices = source.vertices.clone();
    for vertex in &mut basis_vertices {
        vertex[0] += 0.3;
    }
    let mut session = SculptSession::default();
    session.set_x_symmetry(false);
    session.begin(&source).unwrap();
    session
        .set_restore_basis(Some(basis_vertices.clone()))
        .unwrap();
    session.begin_stroke().unwrap();
    assert!(
        session
            .dab(SculptDab {
                center_local: [0.0, -2.75, 0.0],
                radius_local: 8.0,
                strength: 1.0,
                operation: SculptOperation::Restore,
            })
            .unwrap()
            > 0
    );
    session.end_stroke().unwrap();
    let working = session.working_mesh().unwrap();
    assert!(
        working.vertices[1..9]
            .iter()
            .zip(&source.vertices[1..9])
            .any(|(after, before)| after != before),
        "restore must be able to blend the weld loop toward its basis"
    );
}

#[test]
fn a_stage_reports_its_own_edit_as_a_displacement() {
    let source = isolated_targets();
    let mut session = SculptSession::default();

    assert!(session.displacement().is_none());

    session.begin(&source).unwrap();
    let untouched = session.displacement().expect("a begun stage has a delta");
    assert_eq!(untouched.len(), source.vertices.len());
    assert!(
        untouched.iter().flatten().all(|axis| *axis == 0.0),
        "an unsculpted stage must report no displacement at all"
    );
}

#[test]
fn grafting_an_edit_onto_a_different_base_reproduces_it_exactly() {
    let source = isolated_targets();

    let delta: Vec<[f64; 3]> = (0..source.vertices.len())
        .map(|index| {
            if index == 1 {
                [0.25, -0.5, 0.125]
            } else {
                [0.0; 3]
            }
        })
        .collect();

    let mut other = source.clone();
    for (index, vertex) in other.vertices.iter_mut().enumerate() {
        let shift = index as f64 * 0.01;
        vertex[0] += shift;
        vertex[1] -= shift;
    }

    let mut session = SculptSession::default();
    session.begin(&other).unwrap();
    assert!(session.graft_displacement(&delta).unwrap());

    assert_eq!(session.displacement().unwrap(), delta);

    let working = session.working_mesh().unwrap();
    for (index, (vertex, base)) in working.vertices.iter().zip(&other.vertices).enumerate() {
        for axis in 0..3 {
            let want = base[axis] + delta[index][axis];
            assert!(
                (vertex[axis] - want).abs() < 1.0e-12,
                "vertex {index} axis {axis}: {} wanted {want}",
                vertex[axis]
            );
        }
    }

    assert!(
        session
            .graft_displacement(&delta[..delta.len() - 1])
            .is_err()
    );
}
