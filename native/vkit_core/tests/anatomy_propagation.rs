use serde_json::json;
use vkit_core::anatomy::{
    discover_head_anatomy, material_vertices, polygon_group_vertices, propagate_head_anatomy,
    propagate_head_anatomy_auto,
};
use vkit_core::formats::{DazGeometry, GroupTable, load_dsf_path};
use vkit_core::math::{Mat3, Vec3};

fn add_component(
    vertices: &mut Vec<[f64; 3]>,
    faces: &mut Vec<Vec<u32>>,
    polygon_indices: &mut Vec<u32>,
    material_indices: &mut Vec<u32>,
    points: &[[f64; 3]],
    polygon: u32,
    material: u32,
) -> Vec<usize> {
    let start = vertices.len();
    vertices.extend_from_slice(points);
    let indices: Vec<_> = (start..vertices.len()).collect();
    for offset in 1..indices.len() - 1 {
        faces.push(vec![
            indices[0] as u32,
            indices[offset] as u32,
            indices[offset + 1] as u32,
        ]);
        polygon_indices.push(polygon);
        material_indices.push(material);
    }
    indices
}

fn synthetic_g2() -> DazGeometry {
    let polygon_groups: Vec<String> = ["head", "lEye", "rEye", "upperJaw", "lowerJaw", "tongue"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let materials: Vec<String> = [
        "Face",
        "Sclera",
        "Teeth",
        "Gums",
        "Tongue",
        "InnerMouth",
        "Nostrils",
        "Lacrimals",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let mut vertices = Vec::new();
    let mut faces = Vec::new();
    let mut polygon_indices = Vec::new();
    let mut material_indices = Vec::new();

    const ROWS: usize = 13;
    const COLUMNS: usize = 13;
    for row in 0..ROWS {
        let y = 157.0 + row as f64 * 1.4;
        for column in 0..COLUMNS {
            let x = -6.0 + column as f64;
            let z = 7.0 - 0.055 * x * x + 0.012 * (y - 165.0).powi(2);
            vertices.push([x, y, z]);
        }
    }
    for row in 0..ROWS - 1 {
        for column in 0..COLUMNS - 1 {
            let a = row * COLUMNS + column;
            let b = a + 1;
            let c = (row + 1) * COLUMNS + column;
            let d = c + 1;
            faces.extend([
                vec![a as u32, b as u32, d as u32],
                vec![a as u32, d as u32, c as u32],
            ]);
            polygon_indices.extend([0, 0]);
            material_indices.extend([0, 0]);
        }
    }

    let eye_shape = [
        [-0.7, 0.0, 0.0],
        [0.7, 0.0, 0.0],
        [0.0, -0.6, 0.0],
        [0.0, 0.6, 0.0],
        [0.0, 0.0, -0.8],
        [0.0, 0.0, 0.8],
    ];
    let translated = |center: [f64; 3]| -> Vec<[f64; 3]> {
        eye_shape
            .iter()
            .map(|point| {
                [
                    point[0] + center[0],
                    point[1] + center[1],
                    point[2] + center[2],
                ]
            })
            .collect()
    };
    add_component(
        &mut vertices,
        &mut faces,
        &mut polygon_indices,
        &mut material_indices,
        &translated([3.0, 168.0, 7.0]),
        1,
        1,
    );
    add_component(
        &mut vertices,
        &mut faces,
        &mut polygon_indices,
        &mut material_indices,
        &translated([-3.0, 168.0, 7.0]),
        2,
        1,
    );
    add_component(
        &mut vertices,
        &mut faces,
        &mut polygon_indices,
        &mut material_indices,
        &[[2.2, 167.8, 7.8], [2.5, 168.0, 8.0], [2.3, 168.2, 7.9]],
        0,
        7,
    );
    add_component(
        &mut vertices,
        &mut faces,
        &mut polygon_indices,
        &mut material_indices,
        &[[-2.2, 167.8, 7.8], [-2.5, 168.0, 8.0], [-2.3, 168.2, 7.9]],
        0,
        7,
    );
    add_component(
        &mut vertices,
        &mut faces,
        &mut polygon_indices,
        &mut material_indices,
        &[
            [-1.2, 162.0, 7.0],
            [1.2, 162.0, 7.0],
            [-1.0, 161.8, 6.0],
            [1.0, 161.8, 6.0],
        ],
        3,
        2,
    );
    add_component(
        &mut vertices,
        &mut faces,
        &mut polygon_indices,
        &mut material_indices,
        &[
            [-1.3, 162.4, 6.5],
            [1.3, 162.4, 6.5],
            [-0.9, 162.2, 5.5],
            [0.9, 162.2, 5.5],
        ],
        3,
        3,
    );
    add_component(
        &mut vertices,
        &mut faces,
        &mut polygon_indices,
        &mut material_indices,
        &[
            [-1.1, 161.4, 7.0],
            [1.1, 161.4, 7.0],
            [-0.9, 161.2, 6.0],
            [0.9, 161.2, 6.0],
        ],
        4,
        2,
    );
    add_component(
        &mut vertices,
        &mut faces,
        &mut polygon_indices,
        &mut material_indices,
        &[
            [-1.2, 160.9, 6.4],
            [1.2, 160.9, 6.4],
            [-0.8, 160.8, 5.4],
            [0.8, 160.8, 5.4],
        ],
        4,
        3,
    );
    add_component(
        &mut vertices,
        &mut faces,
        &mut polygon_indices,
        &mut material_indices,
        &[
            [-1.0, 160.8, 6.8],
            [1.0, 160.8, 6.8],
            [-0.8, 160.5, 4.5],
            [0.8, 160.5, 4.5],
            [0.0, 160.7, 7.2],
        ],
        5,
        4,
    );
    let inner_mouth = add_component(
        &mut vertices,
        &mut faces,
        &mut polygon_indices,
        &mut material_indices,
        &[
            [-1.5, 163.0, 4.0],
            [1.5, 163.0, 4.0],
            [-1.6, 161.0, 3.0],
            [1.6, 161.0, 3.0],
            [-1.0, 159.0, 1.0],
            [1.0, 159.0, 1.0],
            [0.0, 160.0, 0.0],
        ],
        0,
        5,
    );
    let left_nostril = add_component(
        &mut vertices,
        &mut faces,
        &mut polygon_indices,
        &mut material_indices,
        &[
            [0.2, 164.0, 9.0],
            [1.0, 164.0, 9.1],
            [0.7, 163.6, 9.4],
            [0.4, 164.3, 9.3],
        ],
        0,
        6,
    );
    let right_nostril = add_component(
        &mut vertices,
        &mut faces,
        &mut polygon_indices,
        &mut material_indices,
        &[
            [-0.2, 164.0, 9.0],
            [-1.0, 164.0, 9.1],
            [-0.7, 163.6, 9.4],
            [-0.4, 164.3, 9.3],
        ],
        0,
        6,
    );
    for (seam, a, b) in [
        (inner_mouth[0], 4 * COLUMNS + 5, 4 * COLUMNS + 6),
        (left_nostril[0], 5 * COLUMNS + 6, 5 * COLUMNS + 7),
        (right_nostril[0], 5 * COLUMNS + 5, 5 * COLUMNS + 6),
    ] {
        faces.push(vec![seam as u32, a as u32, b as u32]);
        polygon_indices.push(0);
        material_indices.push(0);
    }

    DazGeometry::new(
        "synthetic-g2".into(),
        vertices,
        faces,
        GroupTable {
            indices: polygon_indices,
            names: polygon_groups,
        },
        GroupTable {
            indices: material_indices,
            names: materials,
        },
        json!({}),
    )
    .unwrap()
}

#[test]
fn metadata_discovery_finds_complete_shared_assemblies() {
    let geometry = synthetic_g2();
    let components = discover_head_anatomy(&geometry).unwrap();
    assert!(components.skin.len() > 150);
    assert!(!components.left_eye.is_empty());
    assert!(!components.right_eye.is_empty());
    assert_eq!(components.left_eye_attached.len(), 3);
    assert_eq!(components.right_eye_attached.len(), 3);
    assert!(
        components
            .upper_jaw
            .iter()
            .all(|index| components.mouth_assembly.contains(index))
    );
    assert!(
        components
            .lower_jaw
            .iter()
            .all(|index| components.mouth_assembly.contains(index))
    );
    assert!(
        components
            .inner_mouth
            .iter()
            .all(|index| components.mouth_assembly.contains(index))
    );
}

#[test]
fn propagation_preserves_all_hard_anatomy_and_passes_strict_gate() {
    let geometry = synthetic_g2();
    let components = discover_head_anatomy(&geometry).unwrap();
    let base: Vec<_> = geometry.vertices.iter().copied().map(Vec3::from).collect();
    let mut fitted = base.clone();
    for &index in &components.skin {
        let point = base[index];
        fitted[index] += Vec3::new(
            0.04 * point.x.tanh(),
            0.015 * point.z.sin(),
            0.08 + 0.004 * point.x,
        );
    }
    let propagated = propagate_head_anatomy(&geometry, &fitted, &components.skin).unwrap();
    assert!(propagated.receipt.quality_gate.passed);
    assert_eq!(propagated.transforms.left_eye.scale, 1.0);
    assert_eq!(propagated.transforms.right_eye.scale, 1.0);
    assert_eq!(propagated.transforms.left_eye.rotation, Mat3::IDENTITY);
    assert_eq!(propagated.transforms.right_eye.rotation, Mat3::IDENTITY);
    assert_eq!(
        propagated.transforms.left_nostril.rotation,
        propagated.transforms.right_nostril.rotation
    );
    assert_eq!(
        propagated.transforms.upper_jaw,
        propagated.transforms.mouth_assembly
    );
    assert_eq!(
        propagated.transforms.lower_jaw,
        propagated.transforms.mouth_assembly
    );
    assert_eq!(
        propagated.transforms.tongue,
        propagated.transforms.mouth_assembly
    );
    assert_eq!(
        propagated.transforms.inner_mouth,
        propagated.transforms.mouth_assembly
    );
    assert!(propagated.receipt.mouth_assembly.spacing.all_preserved());
    assert!(propagated.receipt.skin_transition.topology_preserved);
    assert!(
        propagated
            .receipt
            .skin_transition
            .orientation_margin_preserved
    );
    assert!(
        !propagated
            .receipt
            .skin_transition
            .repair
            .protected_vertices_changed
    );

    for name in ["lEye", "rEye"] {
        let indices = polygon_group_vertices(&geometry, &[name]).unwrap();
        let transform = if name == "lEye" {
            propagated.transforms.left_eye
        } else {
            propagated.transforms.right_eye
        };
        for index in indices {
            assert!((propagated.vertices[index] - transform.apply(base[index])).norm() < 1.0e-10);
        }
    }
    let nostrils = material_vertices(&geometry, &["Nostrils"]).unwrap();
    assert_eq!(nostrils.len(), 8);
}

#[test]
fn canonical_g2_discovery_and_identity_smoke_when_present() {
    let Some(path) = std::env::var_os("VKIT_G2_DSF").map(std::path::PathBuf::from) else {
        return;
    };
    if !path.is_file() {
        return;
    }
    let geometry = load_dsf_path(&path, 0).unwrap();
    let components = discover_head_anatomy(&geometry).unwrap();
    assert!(components.skin.len() > 1_000);
    assert!(components.mouth_assembly.len() > components.inner_mouth.len());
    let base: Vec<_> = geometry.vertices.iter().copied().map(Vec3::from).collect();
    let propagated = propagate_head_anatomy_auto(&geometry, &base).unwrap();
    assert!(propagated.receipt.quality_gate.passed);
    assert!(propagated.receipt.skin_transition.topology_preserved);
    assert!(
        propagated
            .receipt
            .skin_transition
            .orientation_margin_preserved
    );
    let maximum_identity_drift = propagated
        .vertices
        .iter()
        .copied()
        .zip(base.iter().copied())
        .map(|(result, base)| (result - base).norm())
        .reduce(f64::max)
        .unwrap_or(0.0);
    assert!(
        maximum_identity_drift < 1.0e-10,
        "identity propagation drifted by {maximum_identity_drift} cm"
    );
}
