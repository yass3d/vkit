use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::unity_base::{
    TreeValue, decode_record_until, find_script_by_class_name, merged_figure_parts,
    mono_behaviour_script_reference, parse_serialized_asset, pointer_path_id,
};
use super::unity_morph_bank::decode_unity_bundle;
use super::{GeometrySex, Result, VaMError, io_error};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriaxWeight {
    pub vertex: u32,

    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl TriaxWeight {
    #[must_use]
    pub fn linear(self) -> f32 {
        self.x.max(self.y).max(self.z).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoneBinding {
    pub id: String,

    pub triax: Vec<TriaxWeight>,

    pub general: Vec<(u32, f32)>,

    pub fully_weighted: Vec<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkinBinding {
    pub bones: Vec<BoneBinding>,

    pub declared_bone_count: u32,
    pub uses_general_weights: bool,
}

impl SkinBinding {
    #[must_use]
    pub fn to_linear_skin(&self) -> LinearSkin {
        LinearSkin {
            bone_ids: self.bones.iter().map(|bone| bone.id.clone()).collect(),
            per_vertex: self.linear(),
        }
    }

    #[must_use]
    pub fn linear(&self) -> BTreeMap<u32, Vec<(u32, f32)>> {
        let mut per_vertex: BTreeMap<u32, Vec<(u32, f32)>> = BTreeMap::new();
        for (index, bone) in self.bones.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            let mut add = |vertex: u32, weight: f32| {
                if weight > 0.0 && weight.is_finite() {
                    per_vertex.entry(vertex).or_default().push((index, weight));
                }
            };

            if self.uses_general_weights {
                for (vertex, weight) in &bone.general {
                    add(*vertex, *weight);
                }
            } else {
                for weight in &bone.triax {
                    add(weight.vertex, weight.linear());
                }
            }
        }

        let mut primary: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for (index, bone) in self.bones.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            for vertex in &bone.fully_weighted {
                primary.entry(*vertex).or_default().push(index);
            }
        }
        for (vertex, bones) in primary {
            per_vertex.entry(vertex).or_insert_with(|| {
                #[expect(clippy::cast_precision_loss, reason = "a handful of bones")]
                let share = 1.0 / bones.len() as f32;
                bones.into_iter().map(|bone| (bone, share)).collect()
            });
        }

        for weights in per_vertex.values_mut() {
            weights.sort_unstable_by_key(|(bone, _)| *bone);
            weights.dedup_by(|later, kept| {
                if later.0 == kept.0 {
                    kept.1 = kept.1.max(later.1);
                    true
                } else {
                    false
                }
            });
            let total: f32 = weights.iter().map(|(_, weight)| *weight).sum();
            if total > 0.0 {
                for (_, weight) in weights.iter_mut() {
                    *weight /= total;
                }
            }
        }
        per_vertex
    }

    #[must_use]
    pub fn highest_vertex(&self) -> Option<u32> {
        self.bones
            .iter()
            .flat_map(|bone| {
                bone.triax
                    .iter()
                    .map(|weight| weight.vertex)
                    .chain(bone.general.iter().map(|(vertex, _)| *vertex))
                    .chain(bone.fully_weighted.iter().copied())
            })
            .max()
    }

    #[must_use]
    pub fn restricted_to(&self, vertex_count: u32) -> Self {
        Self {
            declared_bone_count: self.declared_bone_count,
            uses_general_weights: self.uses_general_weights,
            bones: self
                .bones
                .iter()
                .map(|bone| BoneBinding {
                    id: bone.id.clone(),
                    triax: bone
                        .triax
                        .iter()
                        .copied()
                        .filter(|weight| weight.vertex < vertex_count)
                        .collect(),
                    general: bone
                        .general
                        .iter()
                        .copied()
                        .filter(|(vertex, _)| *vertex < vertex_count)
                        .collect(),
                    fully_weighted: bone
                        .fully_weighted
                        .iter()
                        .copied()
                        .filter(|vertex| *vertex < vertex_count)
                        .collect(),
                })
                .collect(),
        }
    }

    #[must_use]
    pub fn bone(&self, id: &str) -> Option<&BoneBinding> {
        self.bones.iter().find(|bone| bone.id == id)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinearSkin {
    pub bone_ids: Vec<String>,

    pub per_vertex: BTreeMap<u32, Vec<(u32, f32)>>,
}

impl LinearSkin {
    #[must_use]
    pub fn bound_vertex_count(&self) -> usize {
        self.per_vertex.len()
    }
}

pub fn extract_skin_binding(bundle_path: &Path) -> Result<SkinBinding> {
    let bundle = fs::read(bundle_path).map_err(|error| io_error(bundle_path, error))?;
    extract_skin_binding_from_bundle(&bundle)
}

pub fn extract_skin_binding_from_bundle(bundle: &[u8]) -> Result<SkinBinding> {
    let (data_area, nodes) = decode_unity_bundle(bundle)?;
    let mut best: Option<SkinBinding> = None;

    for node in &nodes {
        if node.path.ends_with(".resS") || node.path.ends_with(".resource") {
            continue;
        }
        let end = node
            .offset
            .checked_add(node.size)
            .ok_or_else(|| binding_error("UnityFS node byte range overflow"))?;
        let cab = data_area
            .get(node.offset..end)
            .ok_or_else(|| binding_error("UnityFS node exceeds decompressed data"))?;
        let asset = parse_serialized_asset(cab)?;
        let Some(script_path_id) = find_script_by_class_name(cab, &asset, "DAZSkinV2")? else {
            continue;
        };

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
            let record = TreeValue::Record(fields);
            let binding = binding_from_record(&record);
            if binding.bones.is_empty() {
                continue;
            }
            if best
                .as_ref()
                .is_none_or(|kept| kept.bones.len() < binding.bones.len())
            {
                best = Some(binding);
            }
        }
    }

    best.ok_or_else(|| binding_error("the figure bundle holds no readable DAZSkinV2 binding"))
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoundSkin {
    pub skin_id: String,

    pub mesh_id: String,

    pub mesh_offset: usize,

    pub mesh_vertices: usize,

    pub binding: SkinBinding,
}

pub fn extract_skin_bindings(bundle_path: &Path, sex: GeometrySex) -> Result<Vec<BoundSkin>> {
    let bundle = fs::read(bundle_path).map_err(|error| io_error(bundle_path, error))?;
    extract_skin_bindings_from_bundle(&bundle, sex)
}

pub fn extract_skin_bindings_from_bundle(
    bundle: &[u8],
    sex: GeometrySex,
) -> Result<Vec<BoundSkin>> {
    let parts = merged_figure_parts(bundle, sex)?;
    let (data_area, nodes) = decode_unity_bundle(bundle)?;
    let mut skins = Vec::new();

    for node in &nodes {
        if node.path.ends_with(".resS") || node.path.ends_with(".resource") {
            continue;
        }
        let end = node
            .offset
            .checked_add(node.size)
            .ok_or_else(|| binding_error("UnityFS node byte range overflow"))?;
        let cab = data_area
            .get(node.offset..end)
            .ok_or_else(|| binding_error("UnityFS node exceeds decompressed data"))?;
        let asset = parse_serialized_asset(cab)?;
        let Some(script_path_id) = find_script_by_class_name(cab, &asset, "DAZSkinV2")? else {
            continue;
        };

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
            let fields = decode_record_until(cab, object, tree, asset.endian, PLACED_FIELDS)?;
            let record = TreeValue::Record(fields);
            let binding = binding_from_record(&record);
            if binding.bones.is_empty() {
                continue;
            }

            let mesh = pointer_path_id(record.field("dazMesh"));
            let Some((_, mesh_id, offset, vertices)) =
                parts.iter().find(|(path_id, ..)| Some(*path_id) == mesh)
            else {
                continue;
            };
            skins.push(BoundSkin {
                skin_id: record
                    .field("skinId")
                    .and_then(TreeValue::as_text)
                    .unwrap_or_default()
                    .to_owned(),
                mesh_id: mesh_id.clone(),
                mesh_offset: *offset,
                mesh_vertices: *vertices,
                binding,
            });
        }
    }

    if skins.len() != parts.len() {
        return Err(binding_error(
            "the figure's parts and its skins do not correspond one to one",
        ));
    }
    skins.sort_by_key(|skin| skin.mesh_offset);
    Ok(skins)
}

const REQUIRED_FIELDS: &[&str] = &["_hasGeneralWeights", "_numBones", "nodes"];

const PLACED_FIELDS: &[&str] = &[
    "_hasGeneralWeights",
    "skinId",
    "dazMesh",
    "_numBones",
    "nodes",
];

fn binding_from_record(record: &TreeValue) -> SkinBinding {
    let nodes = match record.field("nodes") {
        Some(TreeValue::List(entries)) => entries.as_slice(),
        _ => &[],
    };
    SkinBinding {
        declared_bone_count: record
            .field("_numBones")
            .and_then(TreeValue::as_signed)
            .and_then(|count| u32::try_from(count).ok())
            .unwrap_or_default(),
        uses_general_weights: record
            .field("_hasGeneralWeights")
            .and_then(TreeValue::as_unsigned)
            .is_some_and(|value| value != 0),
        bones: nodes.iter().map(bone_from_node).collect(),
    }
}

fn bone_from_node(node: &TreeValue) -> BoneBinding {
    let id = node
        .field("id")
        .and_then(TreeValue::as_text)
        .unwrap_or_default()
        .to_owned();

    let triax = list_of(node, "weights")
        .iter()
        .filter_map(|entry| {
            Some(TriaxWeight {
                vertex: index_of(entry, "vertex")?,
                x: float_of(entry, "xweight")?,
                y: float_of(entry, "yweight")?,
                z: float_of(entry, "zweight")?,
            })
        })
        .collect();

    let general = list_of(node, "generalWeights")
        .iter()
        .filter_map(|entry| Some((index_of(entry, "vertex")?, float_of(entry, "weight")?)))
        .collect();

    let fully_weighted = list_of(node, "fullyWeightedVertices")
        .iter()
        .filter_map(|entry| {
            entry
                .as_signed()
                .and_then(|value| u32::try_from(value).ok())
        })
        .collect();

    BoneBinding {
        id,
        triax,
        general,
        fully_weighted,
    }
}

fn list_of<'value>(value: &'value TreeValue, name: &str) -> &'value [TreeValue] {
    match value.field(name) {
        Some(TreeValue::List(entries)) => entries.as_slice(),
        _ => &[],
    }
}

fn index_of(value: &TreeValue, name: &str) -> Option<u32> {
    value
        .field(name)
        .and_then(TreeValue::as_signed)
        .and_then(|index| u32::try_from(index).ok())
}

fn float_of(value: &TreeValue, name: &str) -> Option<f32> {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the bundle stores these as f32 and the decoder widens them"
    )]
    let component = value.field(name).and_then(TreeValue::as_float)? as f32;
    component.is_finite().then_some(component)
}

fn binding_error(detail: impl Into<String>) -> VaMError {
    VaMError::InvalidBaseBundle(detail.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bone(id: &str, fully: &[u32], triax: &[(u32, f32, f32, f32)]) -> BoneBinding {
        BoneBinding {
            id: id.to_owned(),
            triax: triax
                .iter()
                .map(|(vertex, x, y, z)| TriaxWeight {
                    vertex: *vertex,
                    x: *x,
                    y: *y,
                    z: *z,
                })
                .collect(),
            general: Vec::new(),
            fully_weighted: fully.to_vec(),
        }
    }

    #[test]
    fn bytes_that_are_not_a_bundle_are_an_error() {
        assert!(extract_skin_binding_from_bundle(b"nope").is_err());
        assert!(extract_skin_binding_from_bundle(&[]).is_err());
    }

    #[test]
    fn a_vertex_that_follows_one_axis_completely_is_fully_attached() {
        let weight = TriaxWeight {
            vertex: 0,
            x: 1.0,
            y: 0.0,
            z: 0.0,
        };
        assert!((weight.linear() - 1.0).abs() < f32::EPSILON);

        let partial = TriaxWeight {
            vertex: 0,
            x: 0.25,
            y: 0.4,
            z: 0.1,
        };
        assert!((partial.linear() - 0.4).abs() < f32::EPSILON);
    }

    #[test]
    fn the_weights_of_a_vertex_add_up_to_one() {
        let binding = SkinBinding {
            bones: vec![
                bone("chest", &[10], &[(11, 0.6, 0.2, 0.1)]),
                bone("neck", &[], &[(11, 0.3, 0.9, 0.0), (12, 0.5, 0.5, 0.5)]),
            ],
            declared_bone_count: 2,
            uses_general_weights: false,
        };
        let linear = binding.linear();
        for (vertex, weights) in &linear {
            let total: f32 = weights.iter().map(|(_, weight)| *weight).sum();
            assert!(
                (total - 1.0).abs() < 1.0e-5,
                "vertex {vertex} sums to {total}"
            );
        }

        let shared = &linear[&11];
        assert_eq!(shared.len(), 2);
        assert!(shared[1].1 > shared[0].1, "{shared:?}");

        assert_eq!(linear[&10], vec![(0, 1.0)]);
    }

    #[test]
    fn a_weight_map_outranks_the_primary_bone_list() {
        let binding = SkinBinding {
            bones: vec![
                bone("hip", &[7], &[]),
                bone("pelvis", &[], &[(7, 1.0, 1.0, 1.0)]),
            ],
            declared_bone_count: 2,
            uses_general_weights: false,
        };
        let linear = binding.linear();
        assert_eq!(
            linear[&7],
            vec![(1, 1.0)],
            "the pelvis weighs it; the hip's primary claim is only a fallback"
        );
    }

    #[test]
    fn a_vertex_two_bones_own_is_split_between_them() {
        let binding = SkinBinding {
            bones: vec![bone("hip", &[7], &[]), bone("pelvis", &[7], &[])],
            declared_bone_count: 2,
            uses_general_weights: false,
        };
        let weights = binding.linear();
        let claims = &weights[&7];
        assert_eq!(claims.len(), 2);
        assert!((claims[0].1 - 0.5).abs() < 1.0e-5, "{claims:?}");
    }

    #[test]
    fn an_unclaimed_vertex_is_simply_not_there() {
        let binding = SkinBinding {
            bones: vec![bone("hip", &[], &[(3, 0.0, 0.0, 0.0)])],
            declared_bone_count: 1,
            uses_general_weights: false,
        };
        let linear = binding.linear();
        assert!(linear.is_empty(), "a zero weight is not an attachment");
    }

    #[test]
    fn a_bone_is_found_by_the_name_everything_else_uses() {
        let binding = SkinBinding {
            bones: vec![bone("rThigh", &[1], &[])],
            declared_bone_count: 1,
            uses_general_weights: false,
        };
        assert!(binding.bone("rThigh").is_some());
        assert!(binding.bone("lThigh").is_none());
    }

    #[test]
    #[ignore = "reads the user's VaM install"]
    fn the_real_figure_bundle_yields_a_usable_binding() {
        let Some(path) = std::env::var_os("VKIT_VAM_BUNDLE").map(std::path::PathBuf::from) else {
            eprintln!("set VKIT_VAM_BUNDLE to the f_1 bundle to run this");
            return;
        };
        let started = std::time::Instant::now();
        let binding = extract_skin_binding(&path).expect("binding");
        let read = started.elapsed();
        println!(
            "{} bones (declared {}), general weights: {}, read in {:.2}s",
            binding.bones.len(),
            binding.declared_bone_count,
            binding.uses_general_weights,
            read.as_secs_f64()
        );
        let triax: usize = binding.bones.iter().map(|bone| bone.triax.len()).sum();
        let general: usize = binding.bones.iter().map(|bone| bone.general.len()).sum();
        let full: usize = binding
            .bones
            .iter()
            .map(|bone| bone.fully_weighted.len())
            .sum();
        println!("{triax} triax weights, {general} general, {full} fully weighted");

        assert!(binding.bones.len() > 30, "a G2 skin binds many bones");
        for joint in ["hip", "chest", "head", "rThigh", "lForeArm"] {
            assert!(binding.bone(joint).is_some(), "no binding for {joint}");
        }

        let started = std::time::Instant::now();
        let linear = binding.linear();
        println!(
            "{} bound vertices, collapsed in {:.2}s",
            linear.len(),
            started.elapsed().as_secs_f64()
        );
        assert!(
            linear.len() > 10_000,
            "a G2 body has far more bound vertices than {}",
            linear.len()
        );
        let worst = linear
            .values()
            .map(|weights| {
                let total: f32 = weights.iter().map(|(_, weight)| *weight).sum();
                (total - 1.0).abs()
            })
            .fold(0.0_f32, f32::max);
        assert!(
            worst < 1.0e-4,
            "weights must normalize: worst error {worst}"
        );
        let widest = linear.values().map(Vec::len).max().unwrap_or_default();
        println!("at most {widest} bones move one vertex");

        #[expect(
            clippy::cast_possible_truncation,
            reason = "a vertex count that fits a u32 by construction"
        )]
        let base = crate::G2F_VERTEX_COUNT as u32;
        let below = linear.keys().filter(|vertex| **vertex < base).count();
        println!(
            "{below} of {base} base-range vertices claimed, {} above",
            linear.len() - below
        );
        assert_eq!(
            below,
            crate::G2F_VERTEX_COUNT,
            "the base figure must occupy the merged mesh's prefix, or restricting              to it binds the wrong vertices while every index still looks valid"
        );
        assert!(
            binding
                .highest_vertex()
                .is_some_and(|highest| highest >= base),
            "the bundle skin is expected to bind the merged mesh, not the base"
        );

        let restricted = binding.restricted_to(base).linear();
        assert_eq!(restricted.len(), crate::G2F_VERTEX_COUNT);
        assert!(restricted.keys().all(|vertex| *vertex < base));
        let worst = restricted
            .values()
            .map(|weights| {
                let total: f32 = weights.iter().map(|(_, weight)| *weight).sum();
                (total - 1.0).abs()
            })
            .fold(0.0_f32, f32::max);
        assert!(
            worst < 1.0e-4,
            "restricted weights must still normalize: {worst}"
        );
        println!("restricted to {} base vertices", restricted.len());
    }
}
