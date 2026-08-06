use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::geometry::GeometrySex;
use super::unity_base::{
    decode_record_until, find_script_by_class_name, mono_behaviour_script_reference,
    parse_serialized_asset,
};
use super::unity_morph_bank::decode_unity_bundle;
use super::{Result, VaMError, io_error};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum RotationOrder {
    #[default]
    Xyz,
    Yzx,
    Zyx,
    Zxy,
    Xzy,
    Yxz,
}

impl RotationOrder {
    #[must_use]
    pub const fn from_daz(value: i64) -> Self {
        match value {
            1 => Self::Yzx,
            2 => Self::Zyx,
            3 => Self::Zxy,
            4 => Self::Xzy,
            5 => Self::Yxz,
            _ => Self::Xyz,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RestBone {
    pub id: String,

    pub position: [f32; 3],

    pub orientation: [f32; 3],

    pub rotation_order: RotationOrder,

    pub parent: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RestSkeleton {
    pub bones: BTreeMap<String, RestBone>,
}

impl RestSkeleton {
    #[must_use]
    pub fn to_canonical_space(&self, scale: f32) -> Self {
        Self {
            bones: self
                .bones
                .iter()
                .map(|(id, bone)| {
                    (
                        id.clone(),
                        RestBone {
                            position: [
                                -bone.position[0] * scale,
                                bone.position[1] * scale,
                                bone.position[2] * scale,
                            ],

                            orientation: [
                                bone.orientation[0],
                                -bone.orientation[1],
                                -bone.orientation[2],
                            ],
                            ..bone.clone()
                        },
                    )
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn scaled(&self, factor: f32) -> Self {
        Self {
            bones: self
                .bones
                .iter()
                .map(|(id, bone)| {
                    (
                        id.clone(),
                        RestBone {
                            position: bone.position.map(|axis| axis * factor),
                            ..bone.clone()
                        },
                    )
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn bone(&self, id: &str) -> Option<&RestBone> {
        self.bones.get(id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.bones.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bones.is_empty()
    }
}

pub fn extract_rest_skeleton(bundle_path: &Path, sex: GeometrySex) -> Result<RestSkeleton> {
    let bundle = fs::read(bundle_path).map_err(|error| io_error(bundle_path, error))?;
    extract_rest_skeleton_from_bundle(&bundle, sex)
}

pub fn extract_rest_skeleton_from_bundle(bundle: &[u8], sex: GeometrySex) -> Result<RestSkeleton> {
    let (data_area, nodes) = decode_unity_bundle(bundle)?;
    let mut skeleton = RestSkeleton::default();

    for node in &nodes {
        if node.path.ends_with(".resS") || node.path.ends_with(".resource") {
            continue;
        }
        let end = node
            .offset
            .checked_add(node.size)
            .ok_or_else(|| skeleton_error("UnityFS node byte range overflow"))?;
        let cab = data_area
            .get(node.offset..end)
            .ok_or_else(|| skeleton_error("UnityFS node exceeds decompressed data"))?;
        let asset = parse_serialized_asset(cab)?;
        let Some(script_path_id) = find_script_by_class_name(cab, &asset, "DAZBone")? else {
            continue;
        };

        let mut transform_owner: BTreeMap<i64, i64> = BTreeMap::new();
        let mut transform_parent: BTreeMap<i64, i64> = BTreeMap::new();
        for object in &asset.objects {
            let serialized_type = &asset.types[object.type_index];
            if serialized_type.class_id != 4 {
                continue;
            }
            let Some(tree) = serialized_type.tree.as_ref() else {
                continue;
            };
            let Ok(fields) = decode_record_until(
                cab,
                object,
                tree,
                asset.endian,
                &["m_GameObject", "m_Father"],
            ) else {
                continue;
            };
            let record = super::unity_base::TreeValue::Record(fields);
            let pointer = |name: &str| {
                record
                    .field(name)
                    .and_then(|value| value.field("m_PathID"))
                    .and_then(super::unity_base::TreeValue::as_signed)
                    .filter(|path_id| *path_id != 0)
            };
            if let Some(owner) = pointer("m_GameObject") {
                transform_owner.insert(object.path_id, owner);
            }
            if let Some(father) = pointer("m_Father") {
                transform_parent.insert(object.path_id, father);
            }
        }

        let mut bone_by_game_object: BTreeMap<i64, String> = BTreeMap::new();
        let mut game_object_of_bone: BTreeMap<String, i64> = BTreeMap::new();

        for object in &asset.objects {
            let serialized_type = &asset.types[object.type_index];
            if serialized_type.class_id != 114 {
                continue;
            }
            let Some(tree) = serialized_type.tree.as_ref() else {
                continue;
            };

            if mono_behaviour_script_reference(cab, object, asset.endian)? != (0, script_path_id) {
                continue;
            }
            let fields = decode_record_until(cab, object, tree, asset.endian, REQUIRED_FIELDS)?;
            let record = super::unity_base::TreeValue::Record(fields);

            let Some(id) = record
                .field("_id")
                .and_then(super::unity_base::TreeValue::as_text)
                .filter(|id| !id.trim().is_empty())
            else {
                continue;
            };
            let (position_field, orientation_field) = match sex {
                GeometrySex::Female => ("_worldPosition", "_worldOrientation"),
                GeometrySex::Male => ("_maleWorldPosition", "_maleWorldOrientation"),
            };
            let Some(position) = read_vector(&record, position_field) else {
                continue;
            };
            let orientation = read_vector(&record, orientation_field).unwrap_or([0.0; 3]);

            let owner = record
                .field("m_GameObject")
                .and_then(|pointer| pointer.field("m_PathID"))
                .and_then(super::unity_base::TreeValue::as_signed);
            if let Some(owner) = owner {
                bone_by_game_object.insert(owner, id.to_owned());
            }

            if skeleton.bones.contains_key(id) {
                continue;
            }
            if let Some(owner) = owner {
                game_object_of_bone.insert(id.to_owned(), owner);
            }
            let rotation_order = RotationOrder::from_daz(
                record
                    .field(match sex {
                        GeometrySex::Female => "_rotationOrder",
                        GeometrySex::Male => "_maleRotationOrder",
                    })
                    .and_then(super::unity_base::TreeValue::as_signed)
                    .unwrap_or_default(),
            );
            skeleton.bones.insert(
                id.to_owned(),
                RestBone {
                    id: id.to_owned(),
                    position,
                    orientation,
                    rotation_order,
                    parent: None,
                },
            );
        }

        let transform_of: BTreeMap<i64, i64> = transform_owner
            .iter()
            .map(|(transform, owner)| (*owner, *transform))
            .collect();
        for (id, game_object) in &game_object_of_bone {
            let mut transform = transform_of.get(game_object).copied();
            let mut climbed = 0;
            let parent = loop {
                let Some(current) = transform else { break None };
                let Some(father) = transform_parent.get(&current).copied() else {
                    break None;
                };
                climbed += 1;
                if climbed > MAXIMUM_TRANSFORM_CLIMB {
                    break None;
                }
                if let Some(found) = transform_owner
                    .get(&father)
                    .and_then(|owner| bone_by_game_object.get(owner))
                {
                    break Some(found.clone());
                }
                transform = Some(father);
            };
            if let (Some(parent), Some(bone)) = (parent, skeleton.bones.get_mut(id))
                && parent != *id
            {
                bone.parent = Some(parent);
            }
        }
    }

    if skeleton.bones.is_empty() {
        return Err(skeleton_error(
            "the Person atom bundle holds no readable DAZBone joints",
        ));
    }
    Ok(skeleton)
}

const MAXIMUM_TRANSFORM_CLIMB: usize = 24;

const REQUIRED_FIELDS: &[&str] = &[
    "_id",
    "_worldPosition",
    "_maleWorldPosition",
    "_worldOrientation",
    "_maleWorldOrientation",
    "_maleRotationOrder",
    "_rotationOrder",
    "parentBone",
    "useUnityEulerOrientation",
];

fn read_vector(record: &super::unity_base::TreeValue, name: &str) -> Option<[f32; 3]> {
    let value = record.field(name)?;
    let axis = |axis: &str| -> Option<f32> {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the prefab stores these as f32 and the decoder widens them"
        )]
        let component = value
            .field(axis)
            .and_then(super::unity_base::TreeValue::as_float)? as f32;
        component.is_finite().then_some(component)
    };
    Some([axis("x")?, axis("y")?, axis("z")?])
}

fn skeleton_error(detail: impl Into<String>) -> VaMError {
    VaMError::InvalidBaseBundle(detail.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_that_are_not_a_bundle_are_an_error_and_not_an_empty_skeleton() {
        assert!(extract_rest_skeleton_from_bundle(b"not a bundle", GeometrySex::Female).is_err());
        assert!(extract_rest_skeleton_from_bundle(&[], GeometrySex::Female).is_err());
    }

    #[test]
    fn a_missing_bundle_says_which_one() {
        let error = extract_rest_skeleton(Path::new("no/such/a_per"), GeometrySex::Female)
            .expect_err("a missing bundle cannot produce a skeleton");
        assert!(format!("{error}").contains("a_per"), "{error}");
    }

    #[test]
    fn a_skeleton_is_looked_up_by_the_name_a_pose_carries() {
        let mut skeleton = RestSkeleton::default();
        skeleton.bones.insert(
            "rThigh".to_owned(),
            RestBone {
                id: "rThigh".to_owned(),
                position: [0.1, 0.9, 0.0],
                orientation: [0.0; 3],
                rotation_order: RotationOrder::Xyz,
                parent: Some("pelvis".to_owned()),
            },
        );
        assert_eq!(
            skeleton.bone("rThigh").map(|bone| bone.position[1]),
            Some(0.9)
        );
        assert!(skeleton.bone("lThigh").is_none());
        assert_eq!(skeleton.len(), 1);
        assert!(!skeleton.is_empty());
    }

    #[test]
    #[ignore = "reads the user's VaM install"]
    fn the_real_person_bundle_yields_both_skeletons() {
        let Some(path) = std::env::var_os("VKIT_VAM_PERSON_BUNDLE").map(std::path::PathBuf::from)
        else {
            eprintln!("set VKIT_VAM_PERSON_BUNDLE to the a_per bundle to run this");
            return;
        };
        let female = extract_rest_skeleton(&path, GeometrySex::Female).expect("female skeleton");
        let male = extract_rest_skeleton(&path, GeometrySex::Male).expect("male skeleton");
        println!("{} female joints, {} male", female.len(), male.len());
        let mut names: Vec<&str> = female.bones.keys().map(String::as_str).collect();
        names.sort_unstable();
        println!("joints: {}", names.join(" "));

        for joint in ["hip", "pelvis", "chest", "head", "neck", "rThigh", "lFoot"] {
            assert!(female.bone(joint).is_some(), "missing joint {joint}");
        }
        assert!(
            female.len() > 30,
            "a G2 skeleton is larger than {}",
            female.len()
        );
        assert_eq!(
            female.bones.keys().collect::<Vec<_>>(),
            male.bones.keys().collect::<Vec<_>>()
        );

        let moved = female
            .bones
            .iter()
            .filter(|(id, bone)| {
                male.bone(id)
                    .is_some_and(|other| other.position != bone.position)
            })
            .count();
        println!("{moved} joints sit differently on the male figure");
        assert!(moved > 0, "the male skeleton must not be the female one");

        let mut orders: std::collections::BTreeMap<RotationOrder, usize> =
            std::collections::BTreeMap::new();
        for bone in female.bones.values() {
            *orders.entry(bone.rotation_order).or_default() += 1;
        }
        println!("rotation orders in use: {orders:?}");
        assert!(
            orders.len() > 1,
            "a skeleton that used one order everywhere would not need this read"
        );
        let commonest = orders.values().copied().max().unwrap_or_default();
        assert!(
            commonest < female.len(),
            "no single order covers the skeleton"
        );

        assert!(
            female.bone("hip").is_some(),
            "the root joint must be present"
        );

        assert!(female.bone("lEye").is_some() && female.bone("rEye").is_some());

        let parented = female
            .bones
            .values()
            .filter(|bone| bone.parent.is_some())
            .count();
        println!("{parented} of {} joints have a parent", female.len());
        assert_eq!(
            female.bone("hip").and_then(|bone| bone.parent.as_deref()),
            None,
            "hip is the root and hangs from nothing"
        );
        for (child, parent) in [
            ("pelvis", "hip"),
            ("abdomen", "hip"),
            ("rShin", "rThigh"),
            ("rFoot", "rShin"),
            ("head", "neck"),
            ("lForeArm", "lShldr"),
        ] {
            assert_eq!(
                female.bone(child).and_then(|bone| bone.parent.as_deref()),
                Some(parent),
                "{child} should hang from {parent}, got {:?}",
                female.bone(child).and_then(|bone| bone.parent.as_deref())
            );
        }

        for bone in female.bones.values() {
            let mut walk = bone;
            let mut steps = 0;
            while let Some(parent) = walk.parent.as_deref() {
                walk = female.bone(parent).expect("a parent must be a known joint");
                steps += 1;
                assert!(steps < 32, "{} does not reach the root", bone.id);
            }
            assert_eq!(
                walk.id, "hip",
                "{} reaches {} and not hip",
                bone.id, walk.id
            );
        }
    }
}
