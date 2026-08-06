use std::collections::BTreeMap;

use super::skeleton::{RestBone, RestSkeleton, RotationOrder};
use super::skin_binding::SkinBinding;
use super::unity_morph_bank::{Formula, FormulaTarget};

const fn swing_axis(target: &FormulaTarget) -> Option<usize> {
    match target {
        FormulaTarget::RotationX | FormulaTarget::OrientationX => Some(0),
        FormulaTarget::RotationY | FormulaTarget::OrientationY => Some(1),
        FormulaTarget::RotationZ | FormulaTarget::OrientationZ => Some(2),
        _ => None,
    }
}

fn rest_basis(bone: &RestBone) -> [[f64; 3]; 3] {
    let [x, y, z] = bone.orientation;
    let rx = axis_rotation(0, f64::from(x).to_radians());
    let ry = axis_rotation(1, f64::from(y).to_radians());
    let rz = axis_rotation(2, f64::from(z).to_radians());

    match bone.rotation_order {
        RotationOrder::Xyz => multiply(rx, multiply(ry, rz)),
        RotationOrder::Yzx => multiply(ry, multiply(rz, rx)),
        RotationOrder::Zyx => multiply(rz, multiply(ry, rx)),
        RotationOrder::Zxy => multiply(rz, multiply(rx, ry)),
        RotationOrder::Xzy => multiply(rx, multiply(rz, ry)),
        RotationOrder::Yxz => multiply(ry, multiply(rx, rz)),
    }
}

fn axis_rotation(axis: usize, angle: f64) -> [[f64; 3]; 3] {
    let (sin, cos) = angle.sin_cos();
    match axis {
        0 => [[1.0, 0.0, 0.0], [0.0, cos, -sin], [0.0, sin, cos]],
        1 => [[cos, 0.0, sin], [0.0, 1.0, 0.0], [-sin, 0.0, cos]],
        _ => [[cos, -sin, 0.0], [sin, cos, 0.0], [0.0, 0.0, 1.0]],
    }
}

fn multiply(left: [[f64; 3]; 3], right: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0_f64; 3]; 3];
    for (row, slot) in out.iter_mut().enumerate() {
        for (column, cell) in slot.iter_mut().enumerate() {
            *cell = (0..3).map(|k| left[row][k] * right[k][column]).sum();
        }
    }
    out
}

fn rotate_about(point: [f64; 3], pivot: [f64; 3], axis: [f64; 3], angle: f64) -> [f64; 3] {
    let length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    if length < f64::EPSILON {
        return point;
    }
    let unit = [axis[0] / length, axis[1] / length, axis[2] / length];
    let relative = [
        point[0] - pivot[0],
        point[1] - pivot[1],
        point[2] - pivot[2],
    ];
    let (sin, cos) = angle.sin_cos();
    let dot = unit[0] * relative[0] + unit[1] * relative[1] + unit[2] * relative[2];
    let cross = [
        unit[1] * relative[2] - unit[2] * relative[1],
        unit[2] * relative[0] - unit[0] * relative[2],
        unit[0] * relative[1] - unit[1] * relative[0],
    ];
    let mut out = [0.0_f64; 3];
    for index in 0..3 {
        out[index] = pivot[index]
            + relative[index] * cos
            + cross[index] * sin
            + unit[index] * dot * (1.0 - cos);
    }
    out
}

fn bone_weights(binding: &SkinBinding, bone_id: &str) -> BTreeMap<u32, f64> {
    let mut weights: BTreeMap<u32, f64> = BTreeMap::new();
    let Some(bone) = binding.bone(bone_id) else {
        return weights;
    };
    for weight in &bone.triax {
        weights.insert(weight.vertex, f64::from(weight.linear()));
    }
    for (vertex, weight) in &bone.general {
        weights.insert(*vertex, f64::from(*weight));
    }
    for vertex in &bone.fully_weighted {
        weights.insert(*vertex, 1.0);
    }
    weights.retain(|_, weight| *weight > 1.0e-4);
    weights
}

fn swung_with(skeleton: &RestSkeleton, root: &str) -> Vec<String> {
    let mut carried = vec![root.to_owned()];
    let mut index = 0;
    while index < carried.len() {
        let parent = carried[index].clone();
        for bone in skeleton.bones.values() {
            if bone.parent.as_deref() == Some(parent.as_str()) && !carried.contains(&bone.id) {
                carried.push(bone.id.clone());
            }
        }
        index += 1;
    }
    carried
}

#[must_use]
pub fn rig_delta(
    skeleton: &RestSkeleton,
    binding: &SkinBinding,
    formulas: &[Formula],
    rest: &[[f64; 3]],
) -> Vec<(u32, [f64; 3])> {
    let mut moved: BTreeMap<u32, [f64; 3]> = BTreeMap::new();
    for formula in formulas {
        let Some(axis_index) = swing_axis(&formula.target_type) else {
            continue;
        };
        if formula.multiplier.abs() < 1.0e-6 {
            continue;
        }
        let Some(bone) = skeleton.bone(&formula.target) else {
            continue;
        };
        let basis = rest_basis(bone);

        let axis = [
            basis[0][axis_index],
            basis[1][axis_index],
            basis[2][axis_index],
        ];
        let pivot = [
            f64::from(bone.position[0]),
            f64::from(bone.position[1]),
            f64::from(bone.position[2]),
        ];
        let angle = formula.multiplier.to_radians();
        for carried in swung_with(skeleton, &formula.target) {
            for (vertex, weight) in bone_weights(binding, &carried) {
                let Some(point) = rest.get(vertex as usize) else {
                    continue;
                };
                let turned = rotate_about(*point, pivot, axis, angle);
                let slot = moved.entry(vertex).or_insert([0.0; 3]);
                for index in 0..3 {
                    slot[index] += (turned[index] - point[index]) * weight;
                }
            }
        }
    }

    moved
        .into_iter()
        .filter(|(_, delta)| {
            delta[0].abs() > 1.0e-6 || delta[1].abs() > 1.0e-6 || delta[2].abs() > 1.0e-6
        })
        .collect()
}

#[must_use]
pub fn merge_rig_delta(
    deltas: &[(u32, [f64; 3])],
    rig: &[(u32, [f64; 3])],
) -> Vec<(u32, [f64; 3])> {
    combine(deltas, rig, 1.0)
}

#[must_use]
pub fn strip_rig_delta(
    displayed: &[(u32, [f64; 3])],
    rig: &[(u32, [f64; 3])],
) -> Vec<(u32, [f64; 3])> {
    combine(displayed, rig, -1.0)
}

fn combine(base: &[(u32, [f64; 3])], other: &[(u32, [f64; 3])], sign: f64) -> Vec<(u32, [f64; 3])> {
    let mut merged: BTreeMap<u32, [f64; 3]> = base.iter().copied().collect();
    for (vertex, delta) in other {
        let slot = merged.entry(*vertex).or_insert([0.0; 3]);
        for index in 0..3 {
            slot[index] += delta[index] * sign;
        }
    }
    merged
        .into_iter()
        .filter(|(_, delta)| {
            delta[0].abs() > 1.0e-6 || delta[1].abs() > 1.0e-6 || delta[2].abs() > 1.0e-6
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skeleton_with_one_bone() -> RestSkeleton {
        let mut bones = std::collections::BTreeMap::new();
        bones.insert(
            "lowerJaw".to_owned(),
            RestBone {
                id: "lowerJaw".to_owned(),
                position: [0.0, 10.0, 0.0],
                orientation: [0.0, 0.0, 0.0],
                rotation_order: RotationOrder::Xyz,
                parent: None,
            },
        );
        RestSkeleton { bones }
    }

    fn binding_with_one_vertex() -> SkinBinding {
        SkinBinding {
            bones: vec![super::super::skin_binding::BoneBinding {
                id: "lowerJaw".to_owned(),
                triax: Vec::new(),
                general: Vec::new(),
                fully_weighted: vec![0],
            }],
            declared_bone_count: 1,
            uses_general_weights: false,
        }
    }

    #[test]
    fn a_right_angle_about_x_swings_the_point_forward_and_up() {
        let rig = rig_delta(
            &skeleton_with_one_bone(),
            &binding_with_one_vertex(),
            &[Formula {
                target_type: FormulaTarget::RotationX,
                target: "lowerJaw".to_owned(),
                multiplier: 90.0,
            }],
            &[[0.0, 9.0, 0.0]],
        );
        assert_eq!(rig.len(), 1);
        let (vertex, delta) = rig[0];
        assert_eq!(vertex, 0);
        assert!((delta[0]).abs() < 1.0e-9, "no sideways motion: {delta:?}");
        assert!(
            (delta[1] - 1.0).abs() < 1.0e-9,
            "back up to the pivot line: {delta:?}"
        );
        assert!(
            (delta[2] + 1.0).abs() < 1.0e-9,
            "and out in front: {delta:?}"
        );
    }

    #[test]
    fn a_partly_bound_vertex_takes_its_share() {
        let mut binding = binding_with_one_vertex();
        binding.bones[0].fully_weighted.clear();
        binding.bones[0].general = vec![(0, 0.25)];
        let rig = rig_delta(
            &skeleton_with_one_bone(),
            &binding,
            &[Formula {
                target_type: FormulaTarget::RotationX,
                target: "lowerJaw".to_owned(),
                multiplier: 90.0,
            }],
            &[[0.0, 9.0, 0.0]],
        );
        assert!((rig[0].1[1] - 0.25).abs() < 1.0e-9, "{:?}", rig[0].1);
    }

    #[test]
    fn merging_the_rig_share_and_stripping_it_again_is_the_identity() {
        let record = vec![(0_u32, [0.10, -0.20, 0.30]), (7, [0.01, 0.02, 0.03])];
        let rig = vec![(0_u32, [1.00, 2.00, -3.00]), (9, [0.50, 0.00, 0.00])];
        let displayed = merge_rig_delta(&record, &rig);
        let written = strip_rig_delta(&displayed, &rig);
        assert_eq!(written.len(), record.len());
        for ((left_vertex, left), (right_vertex, right)) in written.iter().zip(&record) {
            assert_eq!(left_vertex, right_vertex);
            for index in 0..3 {
                assert!(
                    (left[index] - right[index]).abs() < 1.0e-9,
                    "{left:?} != {right:?}"
                );
            }
        }
    }

    #[test]
    fn a_record_with_no_bone_formulas_produces_no_rig_delta() {
        let rig = rig_delta(
            &skeleton_with_one_bone(),
            &binding_with_one_vertex(),
            &[Formula {
                target_type: FormulaTarget::BoneCenterY,
                target: "lowerJaw".to_owned(),
                multiplier: 12.0,
            }],
            &[[0.0, 9.0, 0.0]],
        );
        assert!(rig.is_empty(), "bone centres translate, they do not swing");
    }
}
