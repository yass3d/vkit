use std::collections::BTreeMap;

use glam::{Mat4, Quat, Vec3};

use super::{Bone, Influences, Part, RigError, SkinnedFigure};
use crate::vam::{
    BoundSkin, GeometrySex, PoseDocument, RestSkeleton, RotationOrder, VaMError, VaMRoot,
};

#[derive(Debug, thiserror::Error)]
pub enum AssembleError {
    #[error("reading the installation: {0}")]
    Install(#[from] VaMError),
    #[error("reading the base mesh: {0}")]
    Mesh(#[from] crate::formats::FormatError),
    #[error("no base mesh in the installation")]
    NoBaseMesh,
    #[error("the parts do not agree: {0}")]
    Rig(#[from] RigError),
    #[error(
        "the weights do not address this mesh: a bone sits {percent:.0}% of \r
         the figure's height from the vertices it moves"
    )]
    WeightsAddressAnotherMesh { percent: f32 },
    #[error("the figure has no parts to assemble")]
    NoParts,
    #[error(
        "part {part} ends at vertex {end} and the merged mesh has {vertices}; \r
         these parts are not this figure's"
    )]
    PartOutrunsMesh {
        part: String,
        end: usize,
        vertices: usize,
    },
}

const fn euler_order(order: RotationOrder) -> glam::EulerRot {
    match order {
        RotationOrder::Xyz => glam::EulerRot::XYZ,
        RotationOrder::Yzx => glam::EulerRot::YZX,
        RotationOrder::Zyx => glam::EulerRot::ZYX,
        RotationOrder::Zxy => glam::EulerRot::ZXY,
        RotationOrder::Xzy => glam::EulerRot::XZY,
        RotationOrder::Yxz => glam::EulerRot::YXZ,
    }
}

pub fn from_installation(root: &VaMRoot, sex: GeometrySex) -> Result<SkinnedFigure, AssembleError> {
    let bundle_path = root.neutral_base_bundle_path(sex);
    let bundle = std::fs::read(&bundle_path)
        .map_err(|error| AssembleError::Mesh(crate::formats::FormatError::Io(error)))?;
    let vertices = crate::vam::extract_merged_mesh_from_bundle(&bundle, sex)?;
    let skeleton = crate::vam::extract_rest_skeleton(&root.person_atom_bundle_path(), sex)?;
    let skins = crate::vam::extract_skin_bindings_from_bundle(&bundle, sex)?;
    assemble(&vertices, &skeleton, &skins)
}

const ADDRESSES_ANOTHER_MESH: f32 = 0.10;

pub fn assemble(
    vertices: &[[f64; 3]],
    skeleton: &RestSkeleton,
    skins: &[BoundSkin],
) -> Result<SkinnedFigure, AssembleError> {
    if skins.is_empty() {
        return Err(AssembleError::NoParts);
    }
    for skin in skins {
        let end = skin.mesh_offset + skin.mesh_vertices;
        if end > vertices.len() {
            return Err(AssembleError::PartOutrunsMesh {
                part: skin.mesh_id.clone(),
                end,
                vertices: vertices.len(),
            });
        }
    }

    let mut bones: Vec<Bone> = Vec::new();
    let mut index_of: BTreeMap<&str, u16> = BTreeMap::new();
    let mut per_skin: Vec<Vec<u16>> = Vec::with_capacity(skins.len());
    for skin in skins {
        let mut local = Vec::with_capacity(skin.binding.bones.len());
        for bound in &skin.binding.bones {
            let index = match index_of.get(bound.id.as_str()) {
                Some(index) => *index,
                None => {
                    let Ok(index) = u16::try_from(bones.len()) else {
                        continue;
                    };
                    bones.push(Bone {
                        name: bound.id.clone(),
                        rest: Mat4::IDENTITY,
                        parent: None,
                    });
                    index_of.insert(bound.id.as_str(), index);
                    index
                }
            };
            local.push(index);
        }
        per_skin.push(local);
    }

    for bone in &mut bones {
        if let Some(joint) = skeleton.bone(&bone.name) {
            bone.rest = Mat4::from_rotation_translation(
                Quat::from_euler(
                    euler_order(joint.rotation_order),
                    joint.orientation[0].to_radians(),
                    joint.orientation[1].to_radians(),
                    joint.orientation[2].to_radians(),
                ),
                Vec3::from_array(joint.position),
            );
            bone.parent = joint
                .parent
                .as_deref()
                .and_then(|name| index_of.get(name).copied());
        }
    }

    let mut claims: Vec<Vec<(u16, f32)>> = vec![Vec::new(); vertices.len()];
    for (skin, local) in skins.iter().zip(&per_skin) {
        let mut record = |vertex: u32, bone: u16, weight: f32| {
            if (vertex as usize) < skin.mesh_vertices
                && weight > 0.0
                && weight.is_finite()
                && let Some(entry) = claims.get_mut(skin.mesh_offset + vertex as usize)
            {
                entry.push((bone, weight));
            }
        };
        for (bound, bone) in skin.binding.bones.iter().zip(local) {
            if skin.binding.uses_general_weights {
                for (vertex, weight) in &bound.general {
                    record(*vertex, *bone, *weight);
                }
            } else {
                for weight in &bound.triax {
                    record(weight.vertex, *bone, weight.linear());
                }
            }
        }

        for (bound, bone) in skin.binding.bones.iter().zip(local) {
            for vertex in &bound.fully_weighted {
                if (*vertex as usize) < skin.mesh_vertices
                    && let Some(entry) = claims.get_mut(skin.mesh_offset + *vertex as usize)
                    && entry.is_empty()
                {
                    entry.push((*bone, 1.0));
                }
            }
        }
    }

    let influences: Vec<Influences> = claims
        .iter()
        .map(|claim| Influences::from_claims(claim))
        .collect();
    #[expect(clippy::cast_possible_truncation, reason = "a mesh that fits f32")]
    let positions: Vec<[f32; 3]> = vertices
        .iter()
        .map(|vertex| [vertex[0] as f32, vertex[1] as f32, vertex[2] as f32])
        .collect();
    let figure = SkinnedFigure::new(positions, bones, influences)?.with_parts(
        skins
            .iter()
            .map(|skin| Part {
                name: skin.mesh_id.clone(),
                offset: skin.mesh_offset,
                vertices: skin.mesh_vertices,
            })
            .collect(),
    )?;

    if let Some(spread) = figure.bone_placement_error()
        && spread > ADDRESSES_ANOTHER_MESH
    {
        return Err(AssembleError::WeightsAddressAnotherMesh {
            percent: spread * 100.0,
        });
    }
    Ok(figure)
}

#[must_use]
pub fn rotations_for(figure: &SkinnedFigure, pose: &PoseDocument) -> BTreeMap<u16, Quat> {
    let orders: BTreeMap<&str, RotationOrder> = BTreeMap::new();
    let _ = orders;
    pose.bones
        .iter()
        .filter_map(|bone| {
            let [x, y, z] = bone.rotation?;
            let index = figure.bone_index(&bone.id)?;

            let rotation = Quat::from_euler(
                glam::EulerRot::XYZ,
                x.to_radians(),
                y.to_radians(),
                z.to_radians(),
            );
            rotation.is_finite().then(|| (index, rotation.normalize()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vam::{BoneBinding, SkinBinding, TriaxWeight};

    fn skeleton() -> RestSkeleton {
        RestSkeleton {
            bones: [
                (
                    "hip".to_owned(),
                    crate::vam::RestBone {
                        id: "hip".to_owned(),
                        position: [0.0, 0.0, 0.0],
                        orientation: [0.0, 0.0, 0.0],
                        rotation_order: RotationOrder::Xyz,
                        parent: None,
                    },
                ),
                (
                    "chest".to_owned(),
                    crate::vam::RestBone {
                        id: "chest".to_owned(),
                        position: [0.0, 1.0, 0.0],
                        orientation: [0.0, 0.0, 0.0],
                        rotation_order: RotationOrder::Xyz,
                        parent: Some("hip".to_owned()),
                    },
                ),
            ]
            .into_iter()
            .collect(),
        }
    }

    fn claiming(id: &str, range: std::ops::Range<u32>) -> BoneBinding {
        BoneBinding {
            id: id.to_owned(),
            triax: range
                .map(|vertex| TriaxWeight {
                    vertex,
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                })
                .collect(),
            general: Vec::new(),
            fully_weighted: Vec::new(),
        }
    }

    fn parts() -> Vec<BoundSkin> {
        vec![
            BoundSkin {
                skin_id: "SkinBinding-1".to_owned(),
                mesh_id: "body".to_owned(),
                mesh_offset: 0,
                mesh_vertices: 20,
                binding: SkinBinding {
                    bones: vec![claiming("hip", 0..10), claiming("chest", 10..20)],
                    declared_bone_count: 2,
                    uses_general_weights: false,
                },
            },
            BoundSkin {
                skin_id: "SkinBinding-2".to_owned(),
                mesh_id: "graft".to_owned(),
                mesh_offset: 20,
                mesh_vertices: 4,
                binding: SkinBinding {
                    bones: vec![claiming("chest", 0..6)],
                    declared_bone_count: 1,
                    uses_general_weights: false,
                },
            },
        ]
    }

    fn vertices() -> Vec<[f64; 3]> {
        (0..24)
            .map(|step| {
                let angle = f64::from(step % 10) / 10.0 * std::f64::consts::TAU;
                let height = if step < 10 { 0.0 } else { 1.0 };
                [angle.cos() * 0.2, height, angle.sin() * 0.2]
            })
            .collect()
    }

    #[test]
    fn an_installations_parts_assemble_into_a_figure() {
        let figure = assemble(&vertices(), &skeleton(), &parts()).expect("the parts agree");
        assert_eq!(figure.vertices().len(), 24);
        assert_eq!(figure.bones().len(), 2);
        assert_eq!(figure.bone_index("chest"), Some(1));

        assert_eq!(figure.bones()[1].parent, Some(0));
        assert_eq!(figure.bones()[0].parent, None);

        let spread = figure.bone_placement_error().expect("something to measure");
        assert!(spread < 0.05, "{spread}");
    }

    #[test]
    fn a_graft_is_weighted_where_it_sits() {
        let figure = assemble(&vertices(), &skeleton(), &parts()).expect("the parts agree");
        let chest = figure.bone_index("chest").expect("a chest");
        for vertex in 20..24 {
            let influences = &figure.influences()[vertex];
            assert_eq!(influences.bones[0], chest, "graft vertex {vertex}");
            assert!((influences.weights[0] - 1.0).abs() < 1.0e-6);
        }

        let hip = figure.bone_index("hip").expect("a hip");
        assert_eq!(figure.influences()[0].bones[0], hip);
    }

    #[test]
    fn a_figure_knows_the_meshes_it_was_merged_from() {
        let figure = assemble(&vertices(), &skeleton(), &parts()).expect("the parts agree");
        let named: Vec<(&str, usize, usize)> = figure
            .parts()
            .iter()
            .map(|part| (part.name.as_str(), part.offset, part.vertices))
            .collect();
        assert_eq!(named, [("body", 0, 20), ("graft", 20, 4)]);
        assert_eq!(figure.part("graft").map(|part| part.offset), Some(20));
    }

    #[test]
    fn a_callers_own_copy_of_a_part_poses_the_same_way() {
        let figure = assemble(&vertices(), &skeleton(), &parts()).expect("a figure");
        let chest = figure.bone_index("chest").expect("a chest");
        let rotations = [(chest, Quat::from_rotation_z(0.4))].into_iter().collect();
        let whole = figure.pose(&rotations);
        let body: Vec<[f32; 3]> = figure.vertices()[..20].to_vec();
        let posed_body = figure.pose_part(&rotations, &body, 0);
        for (mine, theirs) in posed_body.iter().zip(&whole) {
            for axis in 0..3 {
                assert!(
                    (mine[axis] - theirs[axis]).abs() < 1.0e-6,
                    "{mine:?} {theirs:?}"
                );
            }
        }

        let graft: Vec<[f32; 3]> = figure.vertices()[20..].to_vec();
        let posed_graft = figure.pose_part(&rotations, &graft, 20);
        for (mine, theirs) in posed_graft.iter().zip(&whole[20..]) {
            for axis in 0..3 {
                assert!(
                    (mine[axis] - theirs[axis]).abs() < 1.0e-6,
                    "{mine:?} {theirs:?}"
                );
            }
        }

        let misplaced = figure.pose_part(&rotations, &graft, 0);
        assert!(
            misplaced
                .iter()
                .zip(&whole[20..])
                .any(|(mine, theirs)| (mine[0] - theirs[0]).abs() > 1.0e-3),
            "the graft posed at the wrong offset should not match"
        );
    }

    #[test]
    fn parts_that_outrun_the_mesh_are_refused() {
        let mut short = vertices();
        short.truncate(22);
        let error = assemble(&short, &skeleton(), &parts()).expect_err("this cannot be a rig");
        assert!(
            matches!(
                error,
                AssembleError::PartOutrunsMesh {
                    end: 24,
                    vertices: 22,
                    ..
                }
            ),
            "{error}"
        );
    }

    #[test]
    fn one_parts_weights_do_not_reach_into_the_next() {
        let mut parts = parts();

        parts[0].binding.bones[0]
            .triax
            .extend((20..26).map(|vertex| TriaxWeight {
                vertex,
                x: 1.0,
                y: 1.0,
                z: 1.0,
            }));
        let figure = assemble(&vertices(), &skeleton(), &parts).expect("the parts agree");
        let chest = figure.bone_index("chest").expect("a chest");
        for vertex in 20..24 {
            assert_eq!(
                figure.influences()[vertex].bones[0],
                chest,
                "vertex {vertex}"
            );
        }
    }

    #[test]
    fn a_pose_resolves_to_bone_indices() {
        let figure = assemble(&vertices(), &skeleton(), &parts()).expect("a figure");
        let pose = PoseDocument {
            root: crate::vam::PoseRoot::default(),
            bones: vec![
                crate::vam::PoseBone {
                    id: "chest".to_owned(),
                    position: None,
                    rotation: Some([0.0, 0.0, 90.0]),
                },
                crate::vam::PoseBone {
                    id: "tail".to_owned(),
                    position: None,
                    rotation: Some([0.0, 0.0, 90.0]),
                },
            ],
            morphs: Vec::new(),
        };
        let rotations = rotations_for(&figure, &pose);

        assert_eq!(rotations.len(), 1);
        assert!(rotations.contains_key(&1));

        let posed = figure.pose(&rotations);

        assert!((posed[0][0] - figure.vertices()[0][0]).abs() < 1.0e-5);
        assert!((posed[10][0] - figure.vertices()[10][0]).abs() > 1.0e-3);
    }
}

#[cfg(test)]
mod real_install {
    use super::*;

    #[test]
    #[ignore = "reads the user's VaM install"]
    fn the_installed_figure_assembles_and_every_pose_keeps_it_whole() {
        let Some(root) = std::env::var_os("VKIT_VAM_ROOT")
            .map(std::path::PathBuf::from)
            .and_then(|path| VaMRoot::open(&path).ok())
        else {
            eprintln!("set VKIT_VAM_ROOT to run this");
            return;
        };
        let sex = GeometrySex::Female;

        let skins = crate::vam::extract_skin_bindings(&root.neutral_base_bundle_path(sex), sex)
            .expect("the bundle carries a skin per part");
        for skin in &skins {
            let addresses = skin
                .binding
                .bones
                .iter()
                .flat_map(|bone| {
                    bone.general
                        .iter()
                        .map(|(vertex, _)| *vertex)
                        .chain(bone.triax.iter().map(|weight| weight.vertex))
                        .chain(bone.fully_weighted.iter().copied())
                })
                .max()
                .map_or(0, |highest| highest as usize + 1);
            println!(
                "{} moves {} at +{}: {} vertices, addressing {addresses}",
                skin.skin_id, skin.mesh_id, skin.mesh_offset, skin.mesh_vertices
            );
        }
        assert_eq!(skins.len(), 2, "a body and its graft");
        assert_eq!(skins[0].mesh_vertices, 21_556);
        assert_eq!(skins[1].mesh_offset, 21_556);
        assert_eq!(skins[1].mesh_vertices, 1_452);

        let figure = from_installation(&root, sex).expect("the installed figure assembles");
        let spread = figure
            .bone_placement_error()
            .expect("a figure with bones to measure");
        println!(
            "{} vertices, {} bones, bones sit {:.1}% of height from their own flesh",
            figure.vertices().len(),
            figure.bones().len(),
            spread * 100.0
        );
        assert_eq!(figure.vertices().len(), 23_008, "body plus graft");

        let Some(packages) = std::env::var_os("VKIT_ADDON_PACKAGES").map(std::path::PathBuf::from)
        else {
            eprintln!("set VKIT_ADDON_PACKAGES to pose this figure");
            return;
        };
        let index = crate::vam::PackageIndex::build(&packages, None, |_| {});
        let height = {
            let ys = figure.vertices().iter().map(|vertex| vertex[1]);
            let low = ys.clone().fold(f32::INFINITY, f32::min);
            let high = ys.fold(f32::NEG_INFINITY, f32::max);
            high - low
        };
        let mut posed = 0_usize;
        let mut worst = (0.0_f32, String::new());
        for asset in index
            .of_kind(crate::vam::PackageAssetKind::Pose)
            .take(POSES_SURVEYED * 2)
        {
            if !asset.entry.to_ascii_lowercase().ends_with(".vap") {
                continue;
            }
            let Some(package) = index.package_of(asset) else {
                continue;
            };
            let Ok(bytes) =
                crate::vam::read_var_entry_bytes(&package.path, &asset.entry, 8 * 1024 * 1024)
            else {
                continue;
            };
            let Ok(pose) = PoseDocument::parse(&bytes) else {
                continue;
            };
            let rotations = rotations_for(&figure, &pose);
            if rotations.len() < 10 {
                continue;
            }
            posed += 1;
            let moved = figure.pose(&rotations);
            for (after, before) in moved.iter().zip(figure.vertices()) {
                let travel = (Vec3::from_array(*after) - Vec3::from_array(*before)).length();
                if travel / height > worst.0 {
                    worst = (travel / height, asset.display_name().to_owned());
                }
            }
            if posed >= POSES_SURVEYED {
                break;
            }
        }
        println!(
            "posed {posed} presets; the farthest any vertex travelled was {:.2} of a height, in {}",
            worst.0, worst.1
        );
        assert!(posed > 0, "the install has poses to try");

        assert!(
            worst.0 < 1.5,
            "a vertex travelled {} of a height in {}",
            worst.0,
            worst.1
        );
    }

    const POSES_SURVEYED: usize = 200;
}
