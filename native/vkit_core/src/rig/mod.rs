pub mod assemble;
pub use assemble::{AssembleError, assemble, from_installation, rotations_for};

use std::collections::BTreeMap;

use glam::{Mat4, Quat, Vec3};

pub const MAX_INFLUENCES: usize = 4;

#[derive(Clone, Debug)]
pub struct Bone {
    pub name: String,

    pub rest: Mat4,

    pub parent: Option<u16>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Influences {
    pub bones: [u16; MAX_INFLUENCES],
    pub weights: [f32; MAX_INFLUENCES],
}

impl Influences {
    #[must_use]
    pub fn from_claims(claims: &[(u16, f32)]) -> Self {
        let mut ranked: Vec<(u16, f32)> = claims
            .iter()
            .copied()
            .filter(|(_, weight)| *weight > 0.0 && weight.is_finite())
            .collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(MAX_INFLUENCES);
        let total: f32 = ranked.iter().map(|(_, weight)| *weight).sum();
        let mut influences = Self::default();
        if total <= 0.0 {
            return influences;
        }
        for (slot, (bone, weight)) in ranked.into_iter().enumerate() {
            influences.bones[slot] = bone;
            influences.weights[slot] = weight / total;
        }
        influences
    }

    #[must_use]
    pub fn dominant(&self) -> Option<u16> {
        let (slot, weight) = self
            .weights
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))?;
        (*weight > 0.0).then(|| self.bones[slot])
    }
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum RigError {
    #[error("the figure has {vertices} vertices and the weights cover {influences}")]
    VertexCountMismatch { vertices: usize, influences: usize },
    #[error("vertex {vertex} is weighted to bone {bone}, and there are {bones} bones")]
    BoneOutOfRange {
        vertex: usize,
        bone: u16,
        bones: usize,
    },
    #[error("bone {bone} names parent {parent}, and there are {bones} bones")]
    ParentOutOfRange {
        bone: usize,
        parent: u16,
        bones: usize,
    },
    #[error("bone {bone} is its own ancestor")]
    CyclicHierarchy { bone: usize },
    #[error("vertex {vertex} has weights summing to {total}, not one")]
    UnnormalizedWeights { vertex: usize, total: f32 },
    #[error("{count} vertices are moved by no bone at all")]
    UnweightedVertices { count: usize },
    #[error("a figure needs vertices and bones; it has {vertices} and {bones}")]
    Empty { vertices: usize, bones: usize },
    #[error("the parts do not tile the figure: expected a part at {expected}, found {found}")]
    PartsDoNotTile { expected: usize, found: usize },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub name: String,

    pub offset: usize,

    pub vertices: usize,
}

#[derive(Clone, Debug)]
pub struct SkinnedFigure {
    vertices: Vec<[f32; 3]>,
    bones: Vec<Bone>,
    influences: Vec<Influences>,

    parts: Vec<Part>,

    order: Vec<u16>,
    by_name: BTreeMap<String, u16>,
}

impl SkinnedFigure {
    pub fn new(
        vertices: Vec<[f32; 3]>,
        bones: Vec<Bone>,
        influences: Vec<Influences>,
    ) -> Result<Self, RigError> {
        if vertices.is_empty() || bones.is_empty() {
            return Err(RigError::Empty {
                vertices: vertices.len(),
                bones: bones.len(),
            });
        }
        if vertices.len() != influences.len() {
            return Err(RigError::VertexCountMismatch {
                vertices: vertices.len(),
                influences: influences.len(),
            });
        }
        let bone_count = bones.len();
        for (index, bone) in bones.iter().enumerate() {
            if let Some(parent) = bone.parent
                && usize::from(parent) >= bone_count
            {
                return Err(RigError::ParentOutOfRange {
                    bone: index,
                    parent,
                    bones: bone_count,
                });
            }
        }
        let mut unweighted = 0_usize;
        for (vertex, influence) in influences.iter().enumerate() {
            let mut total = 0.0_f32;
            for slot in 0..MAX_INFLUENCES {
                let weight = influence.weights[slot];
                if weight <= 0.0 {
                    continue;
                }
                let bone = influence.bones[slot];
                if usize::from(bone) >= bone_count {
                    return Err(RigError::BoneOutOfRange {
                        vertex,
                        bone,
                        bones: bone_count,
                    });
                }
                total += weight;
            }
            if total <= 0.0 {
                unweighted += 1;
            } else if (total - 1.0).abs() > 1.0e-3 {
                return Err(RigError::UnnormalizedWeights { vertex, total });
            }
        }
        if unweighted > 0 {
            return Err(RigError::UnweightedVertices { count: unweighted });
        }
        let order = hierarchy_order(&bones)?;
        let by_name = bones
            .iter()
            .enumerate()
            .map(|(index, bone)| (bone.name.clone(), u16::try_from(index).unwrap_or(u16::MAX)))
            .collect();

        let parts = vec![Part {
            name: String::new(),
            offset: 0,
            vertices: vertices.len(),
        }];
        Ok(Self {
            vertices,
            bones,
            influences,
            parts,
            order,
            by_name,
        })
    }

    pub fn with_parts(mut self, parts: Vec<Part>) -> Result<Self, RigError> {
        let mut next = 0_usize;
        for part in &parts {
            if part.offset != next {
                return Err(RigError::PartsDoNotTile {
                    expected: next,
                    found: part.offset,
                });
            }
            next += part.vertices;
        }
        if next != self.vertices.len() {
            return Err(RigError::PartsDoNotTile {
                expected: self.vertices.len(),
                found: next,
            });
        }
        self.parts = parts;
        Ok(self)
    }

    #[must_use]
    pub fn vertices(&self) -> &[[f32; 3]] {
        &self.vertices
    }

    #[must_use]
    pub fn parts(&self) -> &[Part] {
        &self.parts
    }

    #[must_use]
    pub fn part(&self, name: &str) -> Option<&Part> {
        self.parts.iter().find(|part| part.name == name)
    }

    #[must_use]
    pub fn bones(&self) -> &[Bone] {
        &self.bones
    }

    #[must_use]
    pub fn influences(&self) -> &[Influences] {
        &self.influences
    }

    #[must_use]
    pub fn bone_index(&self, name: &str) -> Option<u16> {
        self.by_name.get(name).copied()
    }

    #[must_use]
    pub fn pose(&self, rotations: &BTreeMap<u16, Quat>) -> Vec<[f32; 3]> {
        self.pose_part(rotations, &self.vertices, 0)
    }

    #[must_use]
    pub fn pose_part(
        &self,
        rotations: &BTreeMap<u16, Quat>,
        vertices: &[[f32; 3]],
        offset: usize,
    ) -> Vec<[f32; 3]> {
        let mut posed_frames = vec![Mat4::IDENTITY; self.bones.len()];
        for &index in &self.order {
            let slot = usize::from(index);
            let bone = &self.bones[slot];
            let turn = rotations
                .get(&index)
                .copied()
                .map_or(Mat4::IDENTITY, Mat4::from_quat);
            posed_frames[slot] = match bone.parent {
                Some(parent) => {
                    let parent_slot = usize::from(parent);
                    posed_frames[parent_slot]
                        * (self.bones[parent_slot].rest.inverse() * bone.rest)
                        * turn
                }

                None => bone.rest * turn,
            };
        }

        let deltas: Vec<Mat4> = self
            .bones
            .iter()
            .zip(&posed_frames)
            .map(|(bone, posed)| *posed * bone.rest.inverse())
            .collect();
        vertices
            .iter()
            .enumerate()
            .map(|(index, vertex)| {
                let rest = Vec3::from_array(*vertex);
                let Some(influence) = self.influences.get(offset + index) else {
                    return *vertex;
                };
                let mut moved = Vec3::ZERO;
                for slot in 0..MAX_INFLUENCES {
                    let weight = influence.weights[slot];
                    if weight <= 0.0 {
                        continue;
                    }
                    moved +=
                        deltas[usize::from(influence.bones[slot])].transform_point3(rest) * weight;
                }
                moved.to_array()
            })
            .collect()
    }

    #[must_use]
    pub fn bone_placement_error(&self) -> Option<f32> {
        let (low, high) = self.vertices.iter().fold((f32::MAX, f32::MIN), |span, v| {
            (span.0.min(v[1]), span.1.max(v[1]))
        });
        let height = high - low;
        if height <= 0.0 || !height.is_finite() {
            return None;
        }
        let mut sums = vec![(Vec3::ZERO, 0_usize); self.bones.len()];
        for (vertex, influence) in self.vertices.iter().zip(&self.influences) {
            if let Some(bone) = influence.dominant() {
                let entry = &mut sums[usize::from(bone)];
                entry.0 += Vec3::from_array(*vertex);
                entry.1 += 1;
            }
        }
        let mut distances: Vec<f32> = Vec::new();
        for (bone, (sum, count)) in self.bones.iter().zip(&sums) {
            if *count < 8 {
                continue;
            }
            #[expect(clippy::cast_precision_loss, reason = "a vertex count")]
            let centroid = *sum / *count as f32;
            let joint = bone.rest.transform_point3(Vec3::ZERO);
            distances.push((centroid - joint).length() / height);
        }
        if distances.is_empty() {
            return None;
        }
        distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Some(distances[distances.len() / 2])
    }
}

fn hierarchy_order(bones: &[Bone]) -> Result<Vec<u16>, RigError> {
    let mut depth = vec![u32::MAX; bones.len()];
    for index in 0..bones.len() {
        let mut chain = Vec::new();
        let mut walk = Some(index);
        while let Some(current) = walk {
            if depth[current] != u32::MAX {
                break;
            }
            if chain.contains(&current) {
                return Err(RigError::CyclicHierarchy { bone: current });
            }
            chain.push(current);
            walk = bones[current].parent.map(usize::from);
        }
        let base = walk.map_or(0, |resolved| depth[resolved] + 1);
        for (level, entry) in (base..).zip(chain.into_iter().rev()) {
            depth[entry] = level;
        }
    }
    let mut order: Vec<u16> = (0..bones.len())
        .map(|index| u16::try_from(index).unwrap_or(u16::MAX))
        .collect();
    order.sort_by_key(|index| depth[usize::from(*index)]);
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn figure() -> (Vec<[f32; 3]>, Vec<Bone>, Vec<Influences>) {
        let bones = vec![
            Bone {
                name: "hip".to_owned(),
                rest: Mat4::from_translation(Vec3::new(0.0, 0.0, 0.0)),
                parent: None,
            },
            Bone {
                name: "chest".to_owned(),
                rest: Mat4::from_translation(Vec3::new(0.0, 10.0, 0.0)),
                parent: Some(0),
            },
        ];

        let ring = |height: f32| -> Vec<[f32; 3]> {
            (0..10)
                .map(|step| {
                    #[expect(clippy::cast_precision_loss, reason = "a small step")]
                    let angle = step as f32 / 10.0 * std::f32::consts::TAU;
                    [angle.cos(), height, angle.sin()]
                })
                .collect()
        };
        let mut vertices = ring(0.0);
        vertices.extend(ring(10.0));
        let hip = Influences::from_claims(&[(0, 1.0)]);
        let chest = Influences::from_claims(&[(1, 1.0)]);
        let influences = std::iter::repeat_n(hip, 10)
            .chain(std::iter::repeat_n(chest, 10))
            .collect();
        (vertices, bones, influences)
    }

    #[test]
    fn a_figure_that_agrees_with_itself_is_accepted() {
        let (vertices, bones, influences) = figure();
        let rig = SkinnedFigure::new(vertices, bones, influences).expect("the parts agree");
        assert_eq!(rig.vertices().len(), 20);
        assert_eq!(rig.bone_index("chest"), Some(1));
        assert_eq!(rig.bone_index("elbow"), None);
    }

    #[test]
    fn a_weight_table_for_a_different_mesh_is_refused() {
        let (mut vertices, bones, influences) = figure();
        vertices.push([0.0, 5.0, 0.0]);
        assert_eq!(
            SkinnedFigure::new(vertices, bones, influences).unwrap_err(),
            RigError::VertexCountMismatch {
                vertices: 21,
                influences: 20
            }
        );
    }

    #[test]
    fn a_weight_naming_a_bone_that_is_not_there_is_refused() {
        let (vertices, bones, mut influences) = figure();
        influences[0] = Influences::from_claims(&[(7, 1.0)]);
        assert!(matches!(
            SkinnedFigure::new(vertices, bones, influences),
            Err(RigError::BoneOutOfRange { bone: 7, .. })
        ));
    }

    #[test]
    fn a_vertex_no_bone_moves_is_refused() {
        let (vertices, bones, mut influences) = figure();
        influences[0] = Influences::default();
        assert_eq!(
            SkinnedFigure::new(vertices, bones, influences).unwrap_err(),
            RigError::UnweightedVertices { count: 1 }
        );
    }

    #[test]
    fn a_hierarchy_that_loops_is_refused() {
        let (vertices, mut bones, influences) = figure();
        bones[0].parent = Some(1);
        assert!(matches!(
            SkinnedFigure::new(vertices, bones, influences),
            Err(RigError::CyclicHierarchy { .. })
        ));
    }

    #[test]
    fn a_parent_that_is_not_there_is_refused() {
        let (vertices, mut bones, influences) = figure();
        bones[1].parent = Some(9);
        assert!(matches!(
            SkinnedFigure::new(vertices, bones, influences),
            Err(RigError::ParentOutOfRange { parent: 9, .. })
        ));
    }

    #[test]
    fn the_strongest_four_claims_are_kept_and_normalized() {
        let influences =
            Influences::from_claims(&[(0, 0.1), (1, 0.9), (2, 0.5), (3, 0.3), (4, 0.7)]);
        assert_eq!(influences.bones, [1, 4, 2, 3]);
        let total: f32 = influences.weights.iter().sum();
        assert!((total - 1.0).abs() < 1.0e-6, "{total}");
        assert_eq!(influences.dominant(), Some(1));

        assert!(!influences.bones.contains(&0));
    }

    #[test]
    fn the_hierarchy_carries_a_turn_downward_and_not_up() {
        let (vertices, bones, influences) = figure();
        let rig = SkinnedFigure::new(vertices, bones, influences).expect("a figure");
        let half = Quat::from_rotation_z(std::f32::consts::PI);
        let posed = rig.pose(&[(0_u16, half)].into_iter().collect());

        assert!((posed[0][0] - -1.0).abs() < 1.0e-4, "{:?}", posed[0]);

        assert!((posed[10][1] - -10.0).abs() < 1.0e-4, "{:?}", posed[10]);

        let posed = rig.pose(&[(1_u16, half)].into_iter().collect());

        assert!((posed[0][0] - 1.0).abs() < 1.0e-4, "{:?}", posed[0]);

        assert!((posed[10][1] - 10.0).abs() < 1.0e-4, "{:?}", posed[10]);
        assert!((posed[10][0] - -1.0).abs() < 1.0e-4, "{:?}", posed[10]);
    }

    #[test]
    fn a_pose_that_names_nothing_changes_nothing() {
        let (vertices, bones, influences) = figure();
        let rig = SkinnedFigure::new(vertices.clone(), bones, influences).expect("a figure");
        let posed = rig.pose(&BTreeMap::new());
        for (before, after) in vertices.iter().zip(posed.iter()) {
            for axis in 0..3 {
                assert!(
                    (before[axis] - after[axis]).abs() < 1.0e-5,
                    "{before:?} {after:?}"
                );
            }
        }
    }

    #[test]
    fn bones_sit_among_the_vertices_they_move() {
        let (vertices, bones, influences) = figure();
        let rig = SkinnedFigure::new(vertices, bones, influences).expect("a figure");
        let spread = rig.bone_placement_error().expect("something to measure");
        assert!(spread < 0.05, "{spread}");
    }

    #[test]
    fn weights_meant_for_another_mesh_show_up_as_misplaced_bones() {
        let (vertices, bones, mut influences) = figure();

        influences.rotate_left(10);
        let rig = SkinnedFigure::new(vertices, bones, influences).expect("structurally valid");
        let spread = rig.bone_placement_error().expect("something to measure");
        assert!(
            spread > 0.5,
            "swapped weights should read as misplaced: {spread}"
        );
    }
}
