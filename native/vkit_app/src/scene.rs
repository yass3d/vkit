use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

use glam::{DQuat, DVec3, EulerRot, Mat4, Quat, Vec3};
use rayon::prelude::*;
use tempfile::TempDir;
use thiserror::Error;
use vkit_core::{
    formats::{
        DazGeometry, HEAD_SKIN_MATERIALS, HEAD_VISUAL_MATERIALS, Mesh, MorphTarget, MtlDocument,
        ObjDocument, OrderedObjMesh, load_mtl, load_obj_document,
    },
    surface_smoothing::{SurfaceSmoothingError, SurfaceSmoothingTopology},
    symmetry::{SymmetryMode, SymmetryOptions, symmetrize_mesh_x},
};

use crate::camera::TurntableCamera;
use crate::importers::{MeshImportProgress, prepare_mesh_import_with_progress};

const BVH_LEAF_TRIANGLES: usize = 8;
const MAX_PIN_HISTORY: usize = 64;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SculptSurfaceGroup {
    HeadSkin,
    TearLacrimal,
    Eyelashes,
    Eyes,
    Lips,
    InnerMouth,
    TeethTongue,
}

impl SculptSurfaceGroup {
    pub const ALL: [Self; 7] = [
        Self::HeadSkin,
        Self::TearLacrimal,
        Self::Eyelashes,
        Self::Eyes,
        Self::Lips,
        Self::TeethTongue,
        Self::InnerMouth,
    ];

    const fn index(self) -> usize {
        match self {
            Self::HeadSkin => 0,
            Self::TearLacrimal => 1,
            Self::Eyelashes => 2,
            Self::Eyes => 3,
            Self::Lips => 4,
            Self::TeethTongue => 5,
            Self::InnerMouth => 6,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SculptSurfaceDescriptor {
    pub group: SculptSurfaceGroup,
    pub mesh: Arc<SurfaceMesh>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds3 {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds3 {
    pub fn from_points(points: &[Vec3]) -> Self {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for &point in points {
            min = min.min(point);
            max = max.max(point);
        }
        if points.is_empty() {
            return Self {
                min: Vec3::splat(-0.5),
                max: Vec3::splat(0.5),
            };
        }
        Self { min, max }
    }

    fn from_vec3_points(points: impl IntoIterator<Item = Vec3>) -> Self {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let mut found = false;
        for point in points {
            min = min.min(point);
            max = max.max(point);
            found = true;
        }
        if !found {
            return Self {
                min: Vec3::splat(-0.5),
                max: Vec3::splat(0.5),
            };
        }
        Self { min, max }
    }

    fn from_f64_points(points: impl IntoIterator<Item = [f64; 3]>) -> Self {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let mut found = false;
        for point in points {
            let point = DVec3::from_array(point).as_vec3();
            min = min.min(point);
            max = max.max(point);
            found = true;
        }
        if !found {
            return Self {
                min: Vec3::splat(-0.5),
                max: Vec3::splat(0.5),
            };
        }
        Self { min, max }
    }

    pub fn center(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn radius(self) -> f32 {
        (self.max - self.min).length() * 0.5
    }

    fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    fn axis_extent(self, axis: usize) -> f32 {
        (self.max - self.min)[axis]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray3 {
    pub origin: DVec3,
    pub direction: DVec3,
}

impl Ray3 {
    pub fn new(origin: Vec3, direction: Vec3) -> Option<Self> {
        Self::from_dvec(origin.as_dvec3(), direction.as_dvec3())
    }

    pub fn from_dvec(origin: DVec3, direction: DVec3) -> Option<Self> {
        if !origin.is_finite() || !direction.is_finite() || direction.length_squared() <= 1.0e-24 {
            return None;
        }
        Some(Self {
            origin,
            direction: direction.normalize(),
        })
    }

    pub fn at(self, distance: f64) -> DVec3 {
        self.origin + self.direction * distance
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelTransform {
    pub scale_xyz: DVec3,
    pub translation: DVec3,
    pub rotation_degrees: [f64; 3],
    pub pivot_local: DVec3,
}

impl Default for ModelTransform {
    fn default() -> Self {
        Self {
            scale_xyz: DVec3::ONE,
            translation: DVec3::ZERO,
            rotation_degrees: [0.0; 3],
            pivot_local: DVec3::ZERO,
        }
    }
}

impl ModelTransform {
    #[allow(
        dead_code,
        reason = "compatibility constructor for fixed scene transforms"
    )]
    pub fn new(scale: f64, translation: [f64; 3]) -> Self {
        Self::from_components(scale, translation, [0.0; 3])
    }

    pub fn from_components(scale: f64, translation: [f64; 3], rotation_degrees: [f64; 3]) -> Self {
        Self::from_components_with_pivot(scale, translation, rotation_degrees, [0.0; 3])
    }

    pub fn from_components_with_pivot(
        scale: f64,
        translation: [f64; 3],
        rotation_degrees: [f64; 3],
        pivot_local: [f64; 3],
    ) -> Self {
        Self::from_components_xyz_with_pivot([scale; 3], translation, rotation_degrees, pivot_local)
    }

    #[allow(
        dead_code,
        reason = "public convenience constructor; current scan path supplies a facial pivot"
    )]
    pub fn from_components_xyz(
        scale_xyz: [f64; 3],
        translation: [f64; 3],
        rotation_degrees: [f64; 3],
    ) -> Self {
        Self::from_components_xyz_with_pivot(scale_xyz, translation, rotation_degrees, [0.0; 3])
    }

    pub fn from_components_xyz_with_pivot(
        scale_xyz: [f64; 3],
        translation: [f64; 3],
        rotation_degrees: [f64; 3],
        pivot_local: [f64; 3],
    ) -> Self {
        let scale_xyz = DVec3::from_array(scale_xyz.map(|scale| {
            if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            }
        }));
        let translation = DVec3::from_array(translation);
        let rotation_degrees = if rotation_degrees.into_iter().all(f64::is_finite) {
            rotation_degrees
        } else {
            [0.0; 3]
        };
        let pivot_local = DVec3::from_array(pivot_local);
        Self {
            scale_xyz,
            translation: if translation.is_finite() {
                translation
            } else {
                DVec3::ZERO
            },
            rotation_degrees,
            pivot_local: if pivot_local.is_finite() {
                pivot_local
            } else {
                DVec3::ZERO
            },
        }
    }

    pub fn matrix(self) -> Mat4 {
        let rotation = self.rotation_quat();
        Mat4::from_translation((self.pivot_local + self.translation).as_vec3())
            * Mat4::from_scale_rotation_translation(
                self.scale_xyz.as_vec3(),
                Quat::from_array(rotation.to_array().map(|value| value as f32)),
                Vec3::ZERO,
            )
            * Mat4::from_translation(-self.pivot_local.as_vec3())
    }

    pub fn point_to_world(self, local: DVec3) -> DVec3 {
        self.rotation_quat() * ((local - self.pivot_local) * self.scale_xyz)
            + self.pivot_local
            + self.translation
    }

    pub fn point_to_local(self, world: DVec3) -> DVec3 {
        (self.rotation_quat().inverse() * (world - self.pivot_local - self.translation))
            / self.scale_xyz
            + self.pivot_local
    }

    pub fn bounds_to_world(self, local: Bounds3) -> Bounds3 {
        let corners = [
            Vec3::new(local.min.x, local.min.y, local.min.z),
            Vec3::new(local.min.x, local.min.y, local.max.z),
            Vec3::new(local.min.x, local.max.y, local.min.z),
            Vec3::new(local.min.x, local.max.y, local.max.z),
            Vec3::new(local.max.x, local.min.y, local.min.z),
            Vec3::new(local.max.x, local.min.y, local.max.z),
            Vec3::new(local.max.x, local.max.y, local.min.z),
            Vec3::new(local.max.x, local.max.y, local.max.z),
        ]
        .map(|point| self.point_to_world(point.as_dvec3()).as_vec3());
        Bounds3::from_points(&corners)
    }

    pub fn ray_to_local(self, world: Ray3) -> Option<Ray3> {
        let inverse_rotation = self.rotation_quat().inverse();
        Ray3::from_dvec(
            self.point_to_local(world.origin),
            (inverse_rotation * world.direction) / self.scale_xyz,
        )
    }

    pub(crate) fn rotation_quat(self) -> DQuat {
        let [x, y, z] = self.rotation_degrees.map(f64::to_radians);
        DQuat::from_euler(EulerRot::XYZ, x, y, z)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceHit {
    pub triangle: u32,
    pub barycentric: [f64; 3],
    pub distance: f64,
    pub local_point: DVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceEndpoint {
    pub triangle: u32,
    pub barycentric: [f64; 3],
}

impl From<SurfaceHit> for SurfaceEndpoint {
    fn from(hit: SurfaceHit) -> Self {
        Self {
            triangle: hit.triangle,
            barycentric: hit.barycentric,
        }
    }
}

#[derive(Clone, Debug)]
struct BvhNode {
    bounds: Bounds3,
    kind: BvhNodeKind,
}

#[derive(Clone, Debug)]
enum BvhNodeKind {
    Leaf { start: usize, count: usize },
    Branch { left: usize, right: usize },
}

#[derive(Clone, Debug)]
struct TriangleBvh {
    nodes: Vec<BvhNode>,
    triangle_ids: Vec<u32>,
}

impl TriangleBvh {
    fn empty() -> Self {
        Self {
            nodes: Vec::new(),
            triangle_ids: Vec::new(),
        }
    }

    fn build(mesh: &Mesh, visible_triangle_ids: &[u32]) -> Self {
        let mut triangle_ids = visible_triangle_ids.to_vec();

        let acceleration: Vec<TriangleAcceleration> = (0..mesh.triangles.len() as u32)
            .into_par_iter()
            .map(|triangle_id| {
                let bounds = triangle_bounds(mesh, triangle_id);
                TriangleAcceleration {
                    bounds,
                    centroid: bounds.center(),
                }
            })
            .collect();
        let expected_nodes = triangle_ids
            .len()
            .div_ceil(BVH_LEAF_TRIANGLES)
            .saturating_mul(2)
            .saturating_add(1);
        let mut nodes = Vec::with_capacity(expected_nodes);
        if !triangle_ids.is_empty() {
            build_bvh_node(&acceleration, &mut triangle_ids, 0, &mut nodes);
        }
        Self {
            nodes,
            triangle_ids,
        }
    }

    fn intersect(&self, mesh: &Mesh, ray: Ray3) -> Option<SurfaceHit> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut nearest: Option<SurfaceHit> = None;
        let mut stack = Vec::with_capacity(64);
        stack.push(0usize);
        while let Some(node_id) = stack.pop() {
            let node = &self.nodes[node_id];
            let limit = nearest.map_or(f64::INFINITY, |hit| hit.distance);
            if ray_bounds_distance(ray, node.bounds).is_none_or(|distance| distance > limit) {
                continue;
            }
            match node.kind {
                BvhNodeKind::Leaf { start, count } => {
                    for &triangle_id in &self.triangle_ids[start..start + count] {
                        let Some(hit) = intersect_triangle(mesh, ray, triangle_id) else {
                            continue;
                        };
                        let replace = nearest.is_none_or(|current| {
                            hit.distance < current.distance - 1.0e-10
                                || ((hit.distance - current.distance).abs() <= 1.0e-10
                                    && hit.triangle < current.triangle)
                        });
                        if replace {
                            nearest = Some(hit);
                        }
                    }
                }
                BvhNodeKind::Branch { left, right } => {
                    let left_distance = ray_bounds_distance(ray, self.nodes[left].bounds);
                    let right_distance = ray_bounds_distance(ray, self.nodes[right].bounds);
                    match (left_distance, right_distance) {
                        (Some(left_t), Some(right_t)) if left_t <= right_t => {
                            stack.push(right);
                            stack.push(left);
                        }
                        (Some(_), Some(_)) => {
                            stack.push(left);
                            stack.push(right);
                        }
                        (Some(_), None) => stack.push(left),
                        (None, Some(_)) => stack.push(right),
                        (None, None) => {}
                    }
                }
            }
        }
        nearest
    }
}

#[derive(Clone, Copy, Debug)]
struct TriangleAcceleration {
    bounds: Bounds3,
    centroid: Vec3,
}

#[derive(Debug, Error)]
pub enum SceneLoadError {
    #[error("failed to read mesh data: {0}")]
    Format(#[from] vkit_core::formats::FormatError),
    #[error("failed to import mesh container: {0}")]
    Import(String),
    #[error("failed to prepare scan symmetry: {0}")]
    Symmetry(#[from] vkit_core::symmetry::SymmetryError),
    #[error("failed to prepare render-surface smoothing: {0}")]
    SurfaceSmoothing(#[from] SurfaceSmoothingError),
    #[error("OBJ contains no surface triangles")]
    NoSurface,
    #[error("visible triangle id {triangle} exceeds the {triangle_count}-triangle surface")]
    InvalidVisibleTriangle {
        triangle: u32,
        triangle_count: usize,
    },
    #[error("load a Genesis 2 Female DSF before the eye morph")]
    MissingTemplate,
    #[error("the closed-eye morph is not loaded")]
    MissingEyeMorph,
    #[error("result preview geometry is unavailable")]
    MissingResult,
    #[error("result preview has {actual} vertices; expected {expected}")]
    ResultVertexCountMismatch { expected: usize, actual: usize },
}

#[derive(Clone, Debug)]
pub struct SurfaceMesh {
    pub mesh: Arc<Mesh>,
    pub normals: Arc<Vec<[f32; 3]>>,

    pub visible_triangle_ids: Arc<Vec<u32>>,

    pub editable_triangle_ids: Arc<Vec<u32>>,

    pub render_triangles: Arc<Vec<[u32; 3]>>,
    pub wire_indices: Arc<Vec<u32>>,

    pub smoothing_topology: Arc<SurfaceSmoothingTopology>,

    #[allow(
        dead_code,
        reason = "retained for full-geometry diagnostics and regression tests"
    )]
    pub bounds: Bounds3,

    pub visible_bounds: Bounds3,
    /// Where the face is, for a scan that carries a body under it.
    ///
    /// Held rather than derived: the viewport asks for this about ten times a
    /// frame, and for a tall scan the answer costs a pass over every visible
    /// vertex. The mesh does not change under a `SurfaceMesh`, so once is
    /// enough — deriving it per call was a whole-mesh allocation per draw.
    facial_focus_bounds: Bounds3,

    pub revision: u64,

    pub topology_revision: u64,
    visible_bvh: Arc<TriangleBvh>,
    editable_bvh: Arc<TriangleBvh>,
}

struct SurfaceAttributes {
    bounds: Bounds3,
    visible_bounds: Bounds3,
    normals: Arc<Vec<[f32; 3]>>,
    render_triangles: Arc<Vec<[u32; 3]>>,
    wire_indices: Arc<Vec<u32>>,
    revision: u64,
}

impl SurfaceMesh {
    #[cfg(test)]
    pub fn from_ordered_head_visual(ordered: &OrderedObjMesh) -> Result<Self, SceneLoadError> {
        let mesh = ordered.triangulated()?;
        let visible = visible_triangle_ids_for_ordered(ordered);
        let smoothing_topology = Arc::new(SurfaceSmoothingTopology::from_polygons(
            ordered.vertices.len(),
            ordered.polygon_indices(),
        )?);
        Self::with_surface_masks_and_smoothing(mesh, visible.clone(), visible, smoothing_topology)
    }

    pub fn from_daz_head_visual(geometry: &DazGeometry) -> Result<Self, SceneLoadError> {
        let ordered = geometry.to_ordered_obj(None)?;
        let visible_face_mask =
            geometry.face_mask_for_materials(HEAD_VISUAL_MATERIALS.iter().copied());
        let editable_face_mask =
            geometry.face_mask_for_materials(HEAD_SKIN_MATERIALS.iter().copied());
        let visible = visible_triangle_ids_for_face_mask(&ordered, &visible_face_mask);
        let editable = triangle_ids_for_face_mask(&ordered, &editable_face_mask);
        let smoothing_topology = Arc::new(SurfaceSmoothingTopology::from_polygons(
            ordered.vertices.len(),
            ordered.polygon_indices(),
        )?);
        Self::with_surface_masks_and_smoothing(
            ordered.triangulated()?,
            visible,
            editable,
            smoothing_topology,
        )
    }

    pub fn new(mesh: Mesh) -> Result<Self, SceneLoadError> {
        let visible = (0..mesh.triangles.len() as u32).collect();
        Self::with_visible_triangles(mesh, visible)
    }

    pub fn with_visible_triangles(
        mesh: Mesh,
        visible_triangle_ids: Vec<u32>,
    ) -> Result<Self, SceneLoadError> {
        Self::with_surface_masks(mesh, visible_triangle_ids.clone(), visible_triangle_ids)
    }

    fn with_surface_masks(
        mesh: Mesh,
        visible_triangle_ids: Vec<u32>,
        editable_triangle_ids: Vec<u32>,
    ) -> Result<Self, SceneLoadError> {
        let smoothing_topology = Arc::new(SurfaceSmoothingTopology::from_triangles(
            mesh.vertices.len(),
            &mesh.triangles,
        )?);
        Self::with_surface_masks_and_smoothing(
            mesh,
            visible_triangle_ids,
            editable_triangle_ids,
            smoothing_topology,
        )
    }

    fn with_surface_masks_and_smoothing(
        mesh: Mesh,
        visible_triangle_ids: Vec<u32>,
        editable_triangle_ids: Vec<u32>,
        smoothing_topology: Arc<SurfaceSmoothingTopology>,
    ) -> Result<Self, SceneLoadError> {
        Self::with_shared_surface_masks_and_smoothing(
            Arc::new(mesh),
            visible_triangle_ids,
            editable_triangle_ids,
            smoothing_topology,
        )
    }

    fn with_shared_surface_masks_and_smoothing(
        mesh: Arc<Mesh>,
        visible_triangle_ids: Vec<u32>,
        editable_triangle_ids: Vec<u32>,
        smoothing_topology: Arc<SurfaceSmoothingTopology>,
    ) -> Result<Self, SceneLoadError> {
        Self::with_shared_surface_masks_and_picking(
            mesh,
            visible_triangle_ids,
            editable_triangle_ids,
            smoothing_topology,
            true,
        )
    }

    fn with_shared_render_mask(
        mesh: Arc<Mesh>,
        visible_triangle_ids: Vec<u32>,
        smoothing_topology: Arc<SurfaceSmoothingTopology>,
    ) -> Result<Self, SceneLoadError> {
        Self::with_shared_surface_masks_and_picking(
            mesh,
            visible_triangle_ids,
            Vec::new(),
            smoothing_topology,
            false,
        )
    }

    fn with_shared_surface_masks_and_picking(
        mesh: Arc<Mesh>,
        mut visible_triangle_ids: Vec<u32>,
        mut editable_triangle_ids: Vec<u32>,
        smoothing_topology: Arc<SurfaceSmoothingTopology>,
        build_visible_bvh: bool,
    ) -> Result<Self, SceneLoadError> {
        if mesh.triangles.is_empty() {
            return Err(SceneLoadError::NoSurface);
        }
        visible_triangle_ids.sort_unstable();
        visible_triangle_ids.dedup();
        editable_triangle_ids.sort_unstable();
        editable_triangle_ids.dedup();
        if visible_triangle_ids.is_empty() {
            return Err(SceneLoadError::NoSurface);
        }
        if let Some(&triangle) = visible_triangle_ids
            .iter()
            .chain(editable_triangle_ids.iter())
            .find(|&&triangle| triangle as usize >= mesh.triangles.len())
        {
            return Err(SceneLoadError::InvalidVisibleTriangle {
                triangle,
                triangle_count: mesh.triangles.len(),
            });
        }

        let (visible_bvh, attributes) = rayon::join(
            || {
                Arc::new(if build_visible_bvh {
                    TriangleBvh::build(&mesh, &visible_triangle_ids)
                } else {
                    TriangleBvh::empty()
                })
            },
            || {
                let points = mesh
                    .vertices
                    .iter()
                    .map(|point| DVec3::from_array(*point).as_vec3())
                    .collect::<Vec<Vec3>>();
                let visible_points = visible_vertex_points(&mesh, &visible_triangle_ids);
                SurfaceAttributes {
                    bounds: Bounds3::from_points(&points),
                    visible_bounds: Bounds3::from_points(&visible_points),
                    normals: Arc::new(vertex_normals(&mesh, &visible_triangle_ids)),
                    render_triangles: Arc::new(
                        visible_triangle_ids
                            .iter()
                            .map(|&triangle| mesh.triangles[triangle as usize])
                            .collect(),
                    ),
                    wire_indices: Arc::new(unique_wire_indices(&mesh, &visible_triangle_ids)),
                    revision: surface_revision(&mesh, &visible_triangle_ids),
                }
            },
        );
        let SurfaceAttributes {
            bounds,
            visible_bounds,
            normals,
            render_triangles,
            wire_indices,
            revision,
        } = attributes;
        let topology_revision = revision ^ smoothing_topology.revision().rotate_left(17);

        let editable_bvh = if editable_triangle_ids.is_empty() {
            Arc::new(TriangleBvh::empty())
        } else if build_visible_bvh && editable_triangle_ids == visible_triangle_ids {
            Arc::clone(&visible_bvh)
        } else {
            Arc::new(TriangleBvh::build(&mesh, &editable_triangle_ids))
        };
        let facial_focus_bounds = facial_focus_bounds(&mesh, &visible_triangle_ids, visible_bounds);
        Ok(Self {
            mesh,
            normals,
            visible_triangle_ids: Arc::new(visible_triangle_ids),
            editable_triangle_ids: Arc::new(editable_triangle_ids),
            render_triangles,
            wire_indices,
            smoothing_topology,
            bounds,
            visible_bounds,
            facial_focus_bounds,
            revision,
            topology_revision,
            visible_bvh,
            editable_bvh,
        })
    }

    fn with_deformed_render_vertices(&self, mesh: Arc<Mesh>, bounds: Bounds3) -> Self {
        let visible_bounds = visible_bounds(&mesh, &self.visible_triangle_ids);
        let facial_focus_bounds =
            facial_focus_bounds(&mesh, &self.visible_triangle_ids, visible_bounds);
        let normals = Arc::new(vertex_normals(&mesh, &self.visible_triangle_ids));
        let empty_bvh = Arc::new(TriangleBvh::empty());
        Self {
            mesh,
            normals,
            visible_triangle_ids: Arc::clone(&self.visible_triangle_ids),
            editable_triangle_ids: Arc::clone(&self.editable_triangle_ids),
            render_triangles: Arc::clone(&self.render_triangles),
            wire_indices: Arc::clone(&self.wire_indices),
            smoothing_topology: Arc::clone(&self.smoothing_topology),
            bounds,
            visible_bounds,
            facial_focus_bounds,
            revision: self.revision.wrapping_add(1).max(1),
            topology_revision: self.topology_revision,
            visible_bvh: Arc::clone(&empty_bvh),
            editable_bvh: empty_bvh,
        }
    }

    pub fn pick_visible_surface(
        &self,
        world_ray: Ray3,
        transform: ModelTransform,
    ) -> Option<SurfaceHit> {
        self.visible_bvh
            .intersect(&self.mesh, transform.ray_to_local(world_ray)?)
    }

    pub fn pick_editable_surface(
        &self,
        world_ray: Ray3,
        transform: ModelTransform,
    ) -> Option<SurfaceHit> {
        self.editable_bvh
            .intersect(&self.mesh, transform.ray_to_local(world_ray)?)
    }

    pub fn is_editable_triangle(&self, triangle: u32) -> bool {
        self.editable_triangle_ids.binary_search(&triangle).is_ok()
    }

    pub fn alignment_vertex_positions(&self) -> Vec<[f64; 3]> {
        let triangle_ids = if self.editable_triangle_ids.is_empty() {
            self.visible_triangle_ids.as_slice()
        } else {
            self.editable_triangle_ids.as_slice()
        };
        let mut referenced = vec![false; self.mesh.vertices.len()];
        for &triangle_id in triangle_ids {
            for &vertex in &self.mesh.triangles[triangle_id as usize] {
                referenced[vertex as usize] = true;
            }
        }
        self.mesh
            .vertices
            .iter()
            .zip(referenced)
            .filter_map(|(point, referenced)| referenced.then_some(*point))
            .collect()
    }

    pub fn facial_focus_bounds(&self) -> Bounds3 {
        self.facial_focus_bounds
    }

    pub fn endpoint_local_point(&self, endpoint: SurfaceEndpoint) -> Option<DVec3> {
        let triangle = self.mesh.triangles.get(endpoint.triangle as usize)?;
        let [a, b, c] = triangle.map(|index| DVec3::from_array(self.mesh.vertices[index as usize]));
        let [wa, wb, wc] = endpoint.barycentric;
        if ![wa, wb, wc].into_iter().all(f64::is_finite) {
            return None;
        }
        Some(a * wa + b * wb + c * wc)
    }

    pub fn endpoint_world_point(
        &self,
        endpoint: SurfaceEndpoint,
        transform: ModelTransform,
    ) -> Option<Vec3> {
        Some(
            transform
                .point_to_world(self.endpoint_local_point(endpoint)?)
                .as_vec3(),
        )
    }
}

fn visible_triangle_ids_for_ordered(ordered: &OrderedObjMesh) -> Vec<u32> {
    let face_mask = ordered_face_mask_for_materials(ordered, HEAD_VISUAL_MATERIALS);
    visible_triangle_ids_for_face_mask(ordered, &face_mask)
}

fn ordered_face_mask_for_materials(ordered: &OrderedObjMesh, materials: &[&str]) -> Vec<bool> {
    ordered
        .faces
        .iter()
        .map(|face| {
            face.material
                .as_deref()
                .into_iter()
                .chain(face.group.as_deref())
                .any(|name| {
                    materials
                        .iter()
                        .any(|candidate| name.eq_ignore_ascii_case(candidate))
                })
        })
        .collect()
}

#[derive(Clone, Debug, Default)]
struct SculptPreviewSurfaces {
    groups: [Option<Arc<SurfaceMesh>>; SculptSurfaceGroup::ALL.len()],
}

impl SculptPreviewSurfaces {
    fn from_triangle_ids(
        mesh: &Arc<Mesh>,
        triangle_ids: [Vec<u32>; SculptSurfaceGroup::ALL.len()],
        smoothing_topology: &Arc<SurfaceSmoothingTopology>,
    ) -> Result<Self, SceneLoadError> {
        let built = triangle_ids
            .into_par_iter()
            .map(|ids| optional_render_surface(mesh, ids, smoothing_topology))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            groups: std::array::from_fn(|index| built[index].clone()),
        })
    }

    fn surface(&self, group: SculptSurfaceGroup) -> Option<Arc<SurfaceMesh>> {
        self.groups[group.index()].clone()
    }

    fn with_deformed_render_vertices(&self, mesh: &Arc<Mesh>, bounds: Bounds3) -> Self {
        let rebuilt = self
            .groups
            .par_iter()
            .map(|group| {
                group.as_ref().map(|surface| {
                    Arc::new(surface.with_deformed_render_vertices(Arc::clone(mesh), bounds))
                })
            })
            .collect::<Vec<_>>();
        Self {
            groups: std::array::from_fn(|index| rebuilt[index].clone()),
        }
    }
}

#[derive(Clone, Debug)]
struct ResultDisplaySurfaces {
    head: Arc<SurfaceMesh>,

    figure: Arc<SurfaceMesh>,
    tear_lacrimals: Option<Arc<SurfaceMesh>>,
    eyelashes: Option<Arc<SurfaceMesh>>,
    sculpt: SculptPreviewSurfaces,
}

fn result_display_surfaces(
    ordered: &OrderedObjMesh,
) -> Result<ResultDisplaySurfaces, SceneLoadError> {
    let mesh = Arc::new(ordered.triangulated()?);
    let smoothing_topology = Arc::new(SurfaceSmoothingTopology::from_polygons(
        ordered.vertices.len(),
        ordered.polygon_indices(),
    )?);
    let (head_ids, sculpt_ids) = rayon::join(
        || visible_triangle_ids_for_ordered(ordered),
        || sculpt_surface_triangle_ids(ordered),
    );

    let (head, sculpt) = rayon::join(
        || {
            SurfaceMesh::with_shared_surface_masks_and_smoothing(
                Arc::clone(&mesh),
                head_ids.clone(),
                head_ids,
                Arc::clone(&smoothing_topology),
            )
        },
        || SculptPreviewSurfaces::from_triangle_ids(&mesh, sculpt_ids, &smoothing_topology),
    );
    let head = Arc::new(head?);
    let sculpt = sculpt?;
    let tear_lacrimals = sculpt.surface(SculptSurfaceGroup::TearLacrimal);
    let eyelashes = sculpt.surface(SculptSurfaceGroup::Eyelashes);

    let every_triangle = (0..mesh.triangles.len() as u32).collect::<Vec<_>>();
    let figure = Arc::new(SurfaceMesh::with_shared_surface_masks_and_smoothing(
        Arc::clone(&mesh),
        every_triangle,
        head.editable_triangle_ids.as_ref().clone(),
        Arc::clone(&smoothing_topology),
    )?);
    Ok(ResultDisplaySurfaces {
        head,
        figure,
        tear_lacrimals,
        eyelashes,
        sculpt,
    })
}

fn deformed_result_display_surfaces(
    head: &Arc<SurfaceMesh>,
    figure: &Arc<SurfaceMesh>,
    sculpt: &SculptPreviewSurfaces,
    vertices: Vec<[f64; 3]>,
) -> Result<ResultDisplaySurfaces, SceneLoadError> {
    let mesh = Arc::new(Mesh::new(vertices, head.mesh.triangles.clone())?);
    let bounds = Bounds3::from_f64_points(mesh.vertices.iter().copied());
    let sculpt = sculpt.with_deformed_render_vertices(&mesh, bounds);
    let tear_lacrimals = sculpt.surface(SculptSurfaceGroup::TearLacrimal);
    let eyelashes = sculpt.surface(SculptSurfaceGroup::Eyelashes);
    Ok(ResultDisplaySurfaces {
        head: Arc::new(head.with_deformed_render_vertices(Arc::clone(&mesh), bounds)),

        figure: Arc::new(figure.with_deformed_render_vertices(Arc::clone(&mesh), bounds)),
        tear_lacrimals,
        eyelashes,
        sculpt,
    })
}

fn optional_render_surface(
    mesh: &Arc<Mesh>,
    triangle_ids: Vec<u32>,
    smoothing_topology: &Arc<SurfaceSmoothingTopology>,
) -> Result<Option<Arc<SurfaceMesh>>, SceneLoadError> {
    if triangle_ids.is_empty() {
        return Ok(None);
    }
    Ok(Some(Arc::new(SurfaceMesh::with_shared_render_mask(
        Arc::clone(mesh),
        triangle_ids,
        Arc::clone(smoothing_topology),
    )?)))
}

type EyeSurfacePair = (Option<Arc<SurfaceMesh>>, Option<Arc<SurfaceMesh>>);

fn result_eye_surfaces(
    geometry: Option<&DazGeometry>,
    mesh: &Arc<Mesh>,
    smoothing_topology: &Arc<SurfaceSmoothingTopology>,
) -> Result<EyeSurfacePair, SceneLoadError> {
    let Some(geometry) = geometry else {
        return Ok((None, None));
    };
    Ok((
        optional_render_surface(
            mesh,
            polygon_group_triangle_ids(geometry, "lEye"),
            smoothing_topology,
        )?,
        optional_render_surface(
            mesh,
            polygon_group_triangle_ids(geometry, "rEye"),
            smoothing_topology,
        )?,
    ))
}

fn polygon_group_triangle_ids(geometry: &DazGeometry, name: &str) -> Vec<u32> {
    let Some(group_index) = geometry
        .polygon_groups
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(name))
        .map(|index| index as u32)
    else {
        return Vec::new();
    };
    let mut triangle_id = 0_u32;
    let mut result = Vec::new();
    for (face, &group) in geometry
        .faces
        .iter()
        .zip(geometry.polygon_group_indices.iter())
    {
        let triangle_count = face.len().saturating_sub(2) as u32;
        if group == group_index {
            result.extend((0..triangle_count).map(|offset| triangle_id + offset));
        }
        triangle_id = triangle_id.saturating_add(triangle_count);
    }
    result
}

fn sculpt_surface_triangle_ids(
    ordered: &OrderedObjMesh,
) -> [Vec<u32>; SculptSurfaceGroup::ALL.len()] {
    let mut groups: [Vec<u32>; SculptSurfaceGroup::ALL.len()] = Default::default();

    let mut classified: BTreeMap<&str, Option<SculptSurfaceGroup>> = BTreeMap::new();
    let mut triangle_id = 0_u32;
    for face in &ordered.faces {
        let triangle_count = face.vertex_indices.len().saturating_sub(2);

        let mut group = None;
        for label in face
            .material
            .as_deref()
            .into_iter()
            .chain(face.group.as_deref())
        {
            group = *classified
                .entry(label)
                .or_insert_with(|| sculpt_surface_group_for_label(label));
            if group.is_some() {
                break;
            }
        }
        if let Some(group) = group {
            groups[group.index()]
                .extend((0..triangle_count).map(|offset| triangle_id + offset as u32));
        }
        triangle_id = triangle_id.saturating_add(triangle_count as u32);
    }
    groups
}

fn sculpt_surface_group_for_label(label: &str) -> Option<SculptSurfaceGroup> {
    let key = label
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match key.as_str() {
        "face" | "head" | "neck" | "ears" | "ear" | "nostril" | "nostrils" => {
            Some(SculptSurfaceGroup::HeadSkin)
        }
        "lips" | "lip" => Some(SculptSurfaceGroup::Lips),
        "tear" | "tears" | "lacrimal" | "lacrimals" => Some(SculptSurfaceGroup::TearLacrimal),
        "eyelash" | "eyelashes" | "lashes" => Some(SculptSurfaceGroup::Eyelashes),
        "sclera" | "scleras" | "iris" | "irises" | "pupil" | "pupils" | "cornea"
        | "eyereflection" | "leye" | "reye" | "eyes" | "eye" => Some(SculptSurfaceGroup::Eyes),
        "teeth" | "tooth" | "gums" | "gum" | "tongue" | "upperjaw" | "lowerjaw" => {
            Some(SculptSurfaceGroup::TeethTongue)
        }
        "innermouth" => Some(SculptSurfaceGroup::InnerMouth),
        _ => None,
    }
}

fn visible_triangle_ids_for_face_mask(ordered: &OrderedObjMesh, face_mask: &[bool]) -> Vec<u32> {
    triangle_ids_for_face_mask(ordered, face_mask)
}

fn triangle_ids_for_face_mask(ordered: &OrderedObjMesh, face_mask: &[bool]) -> Vec<u32> {
    let mut visible = Vec::new();
    let mut triangle_id = 0_u32;
    for (face_id, face) in ordered.faces.iter().enumerate() {
        let triangle_count = face.vertex_indices.len().saturating_sub(2);
        if face_mask.get(face_id).copied().unwrap_or(false) {
            visible.extend((0..triangle_count).map(|offset| triangle_id + offset as u32));
        }
        triangle_id = triangle_id.saturating_add(triangle_count as u32);
    }
    visible
}

fn visible_vertex_points(mesh: &Mesh, visible_triangle_ids: &[u32]) -> Vec<Vec3> {
    let mut referenced = vec![false; mesh.vertices.len()];
    for &triangle_id in visible_triangle_ids {
        for &vertex in &mesh.triangles[triangle_id as usize] {
            referenced[vertex as usize] = true;
        }
    }
    mesh.vertices
        .iter()
        .zip(referenced)
        .filter(|(_, referenced)| *referenced)
        .map(|(point, _)| DVec3::from_array(*point).as_vec3())
        .collect()
}

fn mesh_mirror_plane_x(mesh: &Mesh) -> f64 {
    let (min_x, max_x) = mesh
        .vertices
        .iter()
        .map(|vertex| vertex[0])
        .filter(|x| x.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(min_x, max_x), x| {
            (min_x.min(x), max_x.max(x))
        });
    if min_x.is_finite() && max_x.is_finite() {
        (min_x + max_x) * 0.5
    } else {
        0.0
    }
}

fn visible_bounds(mesh: &Mesh, visible_triangle_ids: &[u32]) -> Bounds3 {
    Bounds3::from_f64_points(visible_triangle_ids.iter().flat_map(|&triangle_id| {
        mesh.triangles[triangle_id as usize]
            .into_iter()
            .map(|vertex_id| mesh.vertices[vertex_id as usize])
    }))
}

/// The bounds of the face alone on a scan that arrived with a body attached.
///
/// A head crop is its own answer and returns immediately. Anything much taller
/// than it is deep is a bust or a full body, and framing on all of it would put
/// the face in the top eighth of the view, so the top band is measured instead.
fn facial_focus_bounds(mesh: &Mesh, visible_triangle_ids: &[u32], visible: Bounds3) -> Bounds3 {
    let extent = visible.max - visible.min;
    if extent.y <= extent.z.max(1.0e-4) * 3.0 {
        return visible;
    }
    let lower_y = f64::from(visible.max.y - extent.y * 0.13);
    let mut counted = 0usize;
    let focused = Bounds3::from_f64_points(
        visible_triangle_ids
            .iter()
            .flat_map(|&triangle_id| {
                mesh.triangles[triangle_id as usize]
                    .into_iter()
                    .map(|vertex_id| mesh.vertices[vertex_id as usize])
            })
            .filter(|point| point[1] >= lower_y)
            .inspect(|_| counted += 1),
    );
    if counted < 3 { visible } else { focused }
}

fn surface_revision(mesh: &Mesh, visible_triangle_ids: &[u32]) -> u64 {
    let mut revision = mesh
        .canonical_hash()
        .map(|hash| u64::from_le_bytes(hash[..8].try_into().expect("eight-byte slice")))
        .unwrap_or(0);
    for &triangle in visible_triangle_ids {
        revision ^= u64::from(triangle).wrapping_add(0x9e37_79b9_7f4a_7c15);
        revision = revision.rotate_left(7).wrapping_mul(0x100_0000_01b3);
    }
    revision
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeshSide {
    Scan,
    Template,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PinPair {
    pub scan: Option<SurfaceEndpoint>,
    pub template: Option<SurfaceEndpoint>,
}

impl PinPair {
    pub const fn complete(&self) -> bool {
        self.scan.is_some() && self.template.is_some()
    }

    pub const fn empty(&self) -> bool {
        self.scan.is_none() && self.template.is_none()
    }

    pub const fn endpoint(&self, side: MeshSide) -> Option<SurfaceEndpoint> {
        match side {
            MeshSide::Scan => self.scan,
            MeshSide::Template => self.template,
        }
    }

    fn endpoint_mut(&mut self, side: MeshSide) -> &mut Option<SurfaceEndpoint> {
        match side {
            MeshSide::Scan => &mut self.scan,
            MeshSide::Template => &mut self.template,
        }
    }

    const fn counterpart(&self, side: MeshSide) -> Option<SurfaceEndpoint> {
        match side {
            MeshSide::Scan => self.template,
            MeshSide::Template => self.scan,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PinSet {
    pairs: Vec<PinPair>,
    review_required: BTreeSet<usize>,
    history: Vec<PinSnapshot>,
}

#[derive(Clone, Debug)]
struct PinSnapshot {
    pairs: Vec<PinPair>,
    review_required: BTreeSet<usize>,
}

impl PinSet {
    pub fn pairs(&self) -> &[PinPair] {
        &self.pairs
    }

    pub fn complete_count(&self) -> usize {
        self.pairs.iter().filter(|pair| pair.complete()).count()
    }

    pub fn mismatch_start(&self) -> Option<usize> {
        self.pairs.iter().position(|pair| !pair.complete())
    }

    pub fn requires_review(&self, index: usize) -> bool {
        self.review_required.contains(&index)
    }

    pub fn review_count(&self) -> usize {
        self.review_required.len()
    }

    pub fn mark_review_required(&mut self, indices: impl IntoIterator<Item = usize>) {
        self.review_required.extend(
            indices
                .into_iter()
                .filter(|&index| index < self.pairs.len()),
        );
    }

    pub fn add(&mut self, side: MeshSide, endpoint: SurfaceEndpoint) -> usize {
        self.push_history();
        self.add_without_history(side, endpoint)
    }

    pub fn add_many(
        &mut self,
        side: MeshSide,
        endpoints: impl IntoIterator<Item = SurfaceEndpoint>,
    ) -> Vec<usize> {
        let endpoints: Vec<_> = endpoints.into_iter().collect();
        if endpoints.is_empty() {
            return Vec::new();
        }
        self.push_history();
        endpoints
            .into_iter()
            .map(|endpoint| self.add_without_history(side, endpoint))
            .collect()
    }

    fn add_without_history(&mut self, side: MeshSide, endpoint: SurfaceEndpoint) -> usize {
        if let Some(index) = self
            .pairs
            .iter()
            .position(|pair| pair.endpoint(side).is_none() && pair.counterpart(side).is_some())
        {
            *self.pairs[index].endpoint_mut(side) = Some(endpoint);
            if side == MeshSide::Template {
                self.review_required.remove(&index);
            }
            return index;
        }
        if let Some(index) = self.pairs.iter().position(PinPair::empty) {
            *self.pairs[index].endpoint_mut(side) = Some(endpoint);
            if side == MeshSide::Template {
                self.review_required.remove(&index);
            }
            return index;
        }
        let mut pair = PinPair::default();
        *pair.endpoint_mut(side) = Some(endpoint);
        self.pairs.push(pair);
        self.pairs.len() - 1
    }

    pub fn delete(&mut self, side: MeshSide, index: usize) -> bool {
        if self
            .pairs
            .get(index)
            .and_then(|pair| pair.endpoint(side))
            .is_none()
        {
            return false;
        }
        self.push_history();
        *self.pairs[index].endpoint_mut(side) = None;
        self.trim_empty_tail();
        true
    }

    pub fn begin_drag(&mut self, side: MeshSide, index: usize) -> bool {
        if self
            .pairs
            .get(index)
            .and_then(|pair| pair.endpoint(side))
            .is_none()
        {
            return false;
        }
        self.push_history();
        true
    }

    pub fn move_without_history(
        &mut self,
        side: MeshSide,
        index: usize,
        endpoint: SurfaceEndpoint,
    ) -> bool {
        let Some(pair) = self.pairs.get_mut(index) else {
            return false;
        };
        let slot = pair.endpoint_mut(side);
        if slot.is_none() {
            return false;
        }
        *slot = Some(endpoint);
        if side == MeshSide::Template {
            self.review_required.remove(&index);
        }
        true
    }

    pub fn reset(&mut self) -> bool {
        if self.pairs.is_empty() {
            return false;
        }
        self.push_history();
        self.pairs.clear();
        self.review_required.clear();
        true
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.history.pop() else {
            return false;
        };
        self.pairs = previous.pairs;
        self.review_required = previous.review_required;
        true
    }

    pub fn clear_for_mesh_change(&mut self) {
        self.pairs.clear();
        self.review_required.clear();
        self.history.clear();
    }

    fn push_history(&mut self) {
        if self.history.len() == MAX_PIN_HISTORY {
            self.history.remove(0);
        }
        self.history.push(PinSnapshot {
            pairs: self.pairs.clone(),
            review_required: self.review_required.clone(),
        });
    }

    fn trim_empty_tail(&mut self) {
        while self.pairs.last().is_some_and(PinPair::empty) {
            self.pairs.pop();
        }
        self.review_required
            .retain(|&index| index < self.pairs.len());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivePinDrag {
    pub side: MeshSide,
    pub pair_index: usize,
}

#[derive(Debug)]
pub struct PreparedScan {
    mesh: Arc<SurfaceMesh>,
    appearance: Option<Arc<ObjDocument>>,
    materials: Option<Arc<MtlDocument>>,
    mirror_plane_x: f64,
    camera: TurntableCamera,
    source_triangles: Option<usize>,
    final_triangles: usize,
    workspace: Option<TempDir>,
}

impl PreparedScan {
    pub fn load_with_progress(
        path: impl AsRef<Path>,
        mut report_progress: impl FnMut(MeshImportProgress),
    ) -> Result<Self, SceneLoadError> {
        let imported = prepare_mesh_import_with_progress(path.as_ref(), |progress| {
            report_progress(progress);
        })
        .map_err(|error| SceneLoadError::Import(error.to_string()))?;
        let appearance = load_obj_document(&imported.appearance_path)
            .ok()
            .map(Arc::new);
        let materials = appearance.as_ref().and_then(|document| {
            let [library] = document.appearance.material_libraries.as_slice() else {
                return None;
            };
            let root = imported
                .appearance_path
                .parent()
                .unwrap_or_else(|| Path::new("."));
            load_mtl(root.join(library)).ok().map(Arc::new)
        });
        let mesh = Arc::new(SurfaceMesh::new(imported.ordered.triangulated()?)?);
        let mut camera = TurntableCamera::default();
        camera.frame(mesh.facial_focus_bounds());
        Ok(Self {
            mirror_plane_x: mesh_mirror_plane_x(&mesh.mesh),
            mesh,
            appearance,
            materials,
            camera,
            source_triangles: imported.source_triangles,
            final_triangles: imported.final_triangles,
            workspace: imported.workspace,
        })
    }

    pub const fn source_triangles(&self) -> Option<usize> {
        self.source_triangles
    }

    pub const fn final_triangles(&self) -> usize {
        self.final_triangles
    }
}

#[derive(Debug)]
struct TemplateResultSurfaces {
    ordered: Arc<OrderedObjMesh>,
    surfaces: ResultDisplaySurfaces,
    eyes: EyeSurfacePair,
}

#[derive(Debug, Default)]
pub struct WorkspaceScene {
    pub scan_source: Option<Arc<SurfaceMesh>>,
    pub scan: Option<Arc<SurfaceMesh>>,
    scan_mirror_plane_x: f64,

    pub scan_document: Option<Arc<ObjDocument>>,
    pub scan_materials: Option<Arc<MtlDocument>>,
    scan_import_workspace: Option<TempDir>,
    pub template: Option<Arc<SurfaceMesh>>,
    pub template_geometry: Option<Arc<DazGeometry>>,

    template_ordered: Option<Arc<OrderedObjMesh>>,
    /// Keep-the-base weights for the neck-and-ears restore, one per template
    /// vertex. Computed once per template — the Dijkstra pass is a few
    /// milliseconds nobody should pay on every toggle.
    neck_ear_weights: Option<Arc<Vec<f64>>>,

    template_surfaces: Option<TemplateResultSurfaces>,
    pub eye_morph: Option<Arc<MorphTarget>>,
    pub result: Option<Arc<SurfaceMesh>>,

    pub result_figure: Option<Arc<SurfaceMesh>>,

    pub result_tear_lacrimals: Option<Arc<SurfaceMesh>>,
    pub result_eyelashes: Option<Arc<SurfaceMesh>>,
    result_left_eye: Option<Arc<SurfaceMesh>>,
    result_right_eye: Option<Arc<SurfaceMesh>>,

    result_sculpt_surfaces: SculptPreviewSurfaces,

    pub fitted_result: Option<Arc<OrderedObjMesh>>,

    pub result_output: Option<Arc<OrderedObjMesh>>,
    result_display_preview_signature: Option<[f64; 2]>,
    pub scan_camera: TurntableCamera,
    pub template_camera: TurntableCamera,
    pub result_camera: TurntableCamera,

    pub figure_camera: TurntableCamera,
    pub pins: PinSet,
    pub active_drag: Option<ActivePinDrag>,
}

impl WorkspaceScene {
    pub fn clear_scan(&mut self) {
        self.scan_source = None;
        self.scan = None;
        self.scan_document = None;
        self.scan_materials = None;
        self.scan_import_workspace = None;
        self.pins.clear_for_mesh_change();
        self.active_drag = None;
    }

    pub fn install_prepared_scan(&mut self, prepared: PreparedScan) {
        self.scan_camera = prepared.camera;
        self.scan_mirror_plane_x = prepared.mirror_plane_x;
        self.scan_source = Some(Arc::clone(&prepared.mesh));
        self.scan = Some(prepared.mesh);
        self.scan_document = prepared.appearance;
        self.scan_materials = prepared.materials;
        self.scan_import_workspace = prepared.workspace;
        self.frame_linked_edit_cameras();
        self.pins.clear_for_mesh_change();
        self.active_drag = None;
    }

    pub fn template_ordered_obj(&self) -> Option<Arc<OrderedObjMesh>> {
        self.template_ordered.as_ref().map(Arc::clone)
    }

    /// The neck-and-ears keep weights for the current template, or None when
    /// the template does not carry the named materials to seed them from.
    pub fn neck_ear_restore_weights(&mut self) -> Option<Arc<Vec<f64>>> {
        if let Some(weights) = self.neck_ear_weights.as_ref() {
            return Some(Arc::clone(weights));
        }
        let template = self.template_ordered.as_deref()?;
        let weights = vkit_core::restore_region::neck_ear_restore_weights(template);
        if weights.is_empty() {
            return None;
        }
        let weights = Arc::new(weights);
        self.neck_ear_weights = Some(Arc::clone(&weights));
        Some(weights)
    }

    fn build_template_surfaces(&self) -> Option<TemplateResultSurfaces> {
        let ordered = self.template_ordered.as_ref()?;
        let surfaces = result_display_surfaces(ordered).ok()?;
        let eyes = result_eye_surfaces(
            self.template_geometry.as_deref(),
            &surfaces.head.mesh,
            &surfaces.head.smoothing_topology,
        )
        .ok()?;
        Some(TemplateResultSurfaces {
            ordered: Arc::clone(ordered),
            surfaces,
            eyes,
        })
    }

    pub fn load_template_geometry(&mut self, geometry: DazGeometry) -> Result<(), SceneLoadError> {
        let mesh = Arc::new(SurfaceMesh::from_daz_head_visual(&geometry)?);

        let first_template = self.template.is_none();
        if first_template {
            self.template_camera.frame(mesh.facial_focus_bounds());
        }
        self.template = Some(mesh);

        self.template_ordered = geometry.to_ordered_obj(None).ok().map(Arc::new);
        self.neck_ear_weights = None;
        self.template_geometry = Some(Arc::new(geometry));
        self.template_surfaces = self.build_template_surfaces();
        self.eye_morph = None;
        if first_template {
            self.frame_linked_edit_cameras();
        }
        self.pins.clear_for_mesh_change();
        self.active_drag = None;
        Ok(())
    }

    pub fn set_scan_symmetry_with_transform(
        &mut self,
        mode: SymmetryMode,
        transform: ModelTransform,
    ) -> Result<(), SceneLoadError> {
        let Some(source) = self.scan_source.as_ref() else {
            self.scan_mirror_plane_x = 0.0;
            return Ok(());
        };
        self.scan_mirror_plane_x = mesh_mirror_plane_x(&source.mesh);
        if mode == SymmetryMode::Off {
            self.scan = Some(Arc::clone(source));
            return Ok(());
        }
        let mut world_mesh = source.mesh.as_ref().clone();
        for vertex in &mut world_mesh.vertices {
            *vertex = transform
                .point_to_world(DVec3::from_array(*vertex))
                .to_array();
        }
        let center_x = mesh_mirror_plane_x(&world_mesh);

        let (min, max) = world_mesh.vertices.iter().fold(
            ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
            |(mut min, mut max), vertex| {
                for axis in 0..3 {
                    min[axis] = min[axis].min(vertex[axis]);
                    max[axis] = max[axis].max(vertex[axis]);
                }
                (min, max)
            },
        );
        let diagonal = (0..3)
            .map(|axis| (max[axis] - min[axis]).powi(2))
            .sum::<f64>()
            .sqrt()
            .max(f64::EPSILON);
        let prepared = symmetrize_mesh_x(
            &world_mesh,
            SymmetryOptions {
                mode,
                center_x,
                tolerance: Some(diagonal * 0.02),
                ..Default::default()
            },
        )?;
        let mut local_mesh = prepared.mesh;
        for vertex in &mut local_mesh.vertices {
            *vertex = transform
                .point_to_local(DVec3::from_array(*vertex))
                .to_array();
        }
        debug_assert_eq!(local_mesh.vertices.len(), source.mesh.vertices.len());
        debug_assert_eq!(local_mesh.triangles, source.mesh.triangles);
        self.scan = Some(Arc::new(SurfaceMesh::new(local_mesh)?));
        Ok(())
    }

    pub fn scan_mirror_plane_x(&self) -> f64 {
        self.scan_mirror_plane_x
    }

    pub fn install_result(&mut self, output: Arc<OrderedObjMesh>) -> Result<(), SceneLoadError> {
        let prebuilt = self
            .template_surfaces
            .as_ref()
            .filter(|cached| Arc::ptr_eq(&cached.ordered, &output))
            .map(|cached| (cached.surfaces.clone(), cached.eyes.clone()));
        let (surfaces, (left_eye, right_eye)) = match prebuilt {
            Some(ready) => ready,
            None => {
                let surfaces = result_display_surfaces(&output)?;
                let eyes = result_eye_surfaces(
                    self.template_geometry.as_deref(),
                    &surfaces.head.mesh,
                    &surfaces.head.smoothing_topology,
                )?;
                (surfaces, eyes)
            }
        };
        self.result_camera
            .frame(surfaces.head.facial_focus_bounds());
        self.figure_camera.frame(surfaces.figure.bounds);
        self.result = Some(surfaces.head);
        self.result_figure = Some(surfaces.figure);
        self.result_tear_lacrimals = surfaces.tear_lacrimals;
        self.result_eyelashes = surfaces.eyelashes;
        self.result_left_eye = left_eye;
        self.result_right_eye = right_eye;
        self.result_sculpt_surfaces = surfaces.sculpt;
        self.fitted_result = Some(Arc::clone(&output));
        self.result_output = Some(output);
        self.result_display_preview_signature = None;
        Ok(())
    }

    #[allow(
        dead_code,
        reason = "retained for callers that have not established the bound-topology preview contract"
    )]
    pub fn update_result_preview(&mut self, output: OrderedObjMesh) -> Result<(), SceneLoadError> {
        let reusable_topology = self.result_output.as_deref().is_some_and(|previous| {
            previous.vertices.len() == output.vertices.len() && previous.faces == output.faces
        });
        if reusable_topology {
            return self.update_result_preview_vertices(output.vertices);
        }
        let surfaces = result_display_surfaces(&output)?;
        let (left_eye, right_eye) = result_eye_surfaces(
            self.template_geometry.as_deref(),
            &surfaces.head.mesh,
            &surfaces.head.smoothing_topology,
        )?;
        self.result = Some(surfaces.head);
        self.result_figure = Some(surfaces.figure);
        self.result_tear_lacrimals = surfaces.tear_lacrimals;
        self.result_eyelashes = surfaces.eyelashes;
        self.result_left_eye = left_eye;
        self.result_right_eye = right_eye;
        self.result_sculpt_surfaces = surfaces.sculpt;
        self.result_output = Some(Arc::new(output));
        self.result_display_preview_signature = None;
        Ok(())
    }

    pub fn update_result_preview_vertices(
        &mut self,
        vertices: Vec<[f64; 3]>,
    ) -> Result<(), SceneLoadError> {
        let expected = self
            .result_output
            .as_deref()
            .ok_or(SceneLoadError::MissingResult)?
            .vertices
            .len();
        if vertices.len() != expected {
            return Err(SceneLoadError::ResultVertexCountMismatch {
                expected,
                actual: vertices.len(),
            });
        }
        let head = self.result.as_ref().ok_or(SceneLoadError::MissingResult)?;
        let figure = self.result_figure.as_ref().unwrap_or(head);
        let surfaces =
            deformed_result_display_surfaces(head, figure, &self.result_sculpt_surfaces, vertices)?;
        let bounds = surfaces.head.bounds;

        let (left_eye, right_eye) = rayon::join(
            || {
                self.result_left_eye.as_ref().map(|surface| {
                    Arc::new(
                        surface
                            .with_deformed_render_vertices(Arc::clone(&surfaces.head.mesh), bounds),
                    )
                })
            },
            || {
                self.result_right_eye.as_ref().map(|surface| {
                    Arc::new(
                        surface
                            .with_deformed_render_vertices(Arc::clone(&surfaces.head.mesh), bounds),
                    )
                })
            },
        );

        let output = Arc::make_mut(
            self.result_output
                .as_mut()
                .expect("result output checked before surface build"),
        );
        output.vertices.clone_from(&surfaces.head.mesh.vertices);
        self.result = Some(surfaces.head);
        self.result_figure = Some(surfaces.figure);
        self.result_tear_lacrimals = surfaces.tear_lacrimals;
        self.result_eyelashes = surfaces.eyelashes;
        self.result_left_eye = left_eye;
        self.result_right_eye = right_eye;
        self.result_sculpt_surfaces = surfaces.sculpt;
        self.result_display_preview_signature = None;
        Ok(())
    }

    pub fn result_display_preview_matches(&self, signature: [f64; 2]) -> bool {
        self.result_display_preview_signature == Some(signature)
    }

    pub fn set_result_display_preview_vertices(
        &mut self,
        vertices: Vec<[f64; 3]>,
        signature: [f64; 2],
    ) -> Result<(), SceneLoadError> {
        if self.result_display_preview_signature == Some(signature) {
            return Ok(());
        }
        let expected = self
            .result_output
            .as_deref()
            .ok_or(SceneLoadError::MissingResult)?
            .vertices
            .len();
        if vertices.len() != expected {
            return Err(SceneLoadError::ResultVertexCountMismatch {
                expected,
                actual: vertices.len(),
            });
        }
        let head = self.result.as_ref().ok_or(SceneLoadError::MissingResult)?;
        let figure = self.result_figure.as_ref().unwrap_or(head);
        let surfaces =
            deformed_result_display_surfaces(head, figure, &self.result_sculpt_surfaces, vertices)?;
        let bounds = surfaces.head.bounds;
        self.result_left_eye = self.result_left_eye.as_ref().map(|surface| {
            Arc::new(surface.with_deformed_render_vertices(Arc::clone(&surfaces.head.mesh), bounds))
        });
        self.result_right_eye = self.result_right_eye.as_ref().map(|surface| {
            Arc::new(surface.with_deformed_render_vertices(Arc::clone(&surfaces.head.mesh), bounds))
        });
        self.result = Some(surfaces.head);
        self.result_figure = Some(surfaces.figure);
        self.result_tear_lacrimals = surfaces.tear_lacrimals;
        self.result_eyelashes = surfaces.eyelashes;
        self.result_sculpt_surfaces = surfaces.sculpt;
        self.result_display_preview_signature = Some(signature);
        Ok(())
    }

    pub fn clear_result_display_preview(&mut self) -> Result<(), SceneLoadError> {
        if self.result_display_preview_signature.is_none() {
            return Ok(());
        }
        let vertices = self
            .result_output
            .as_deref()
            .ok_or(SceneLoadError::MissingResult)?
            .vertices
            .clone();
        self.set_result_display_preview_vertices(vertices, [f64::NAN; 2])?;
        self.result_display_preview_signature = None;
        Ok(())
    }

    pub fn set_template_eye_value(&mut self, value: f64) -> Result<(), SceneLoadError> {
        let geometry = self
            .template_geometry
            .as_deref()
            .ok_or(SceneLoadError::MissingTemplate)?;
        let displayed = if value == 0.0 && self.eye_morph.is_none() {
            geometry.clone()
        } else {
            self.eye_morph
                .as_deref()
                .ok_or(SceneLoadError::MissingEyeMorph)?
                .apply_to_daz_geometry(geometry, value)?
        };

        self.template = Some(Arc::new(SurfaceMesh::from_daz_head_visual(&displayed)?));
        Ok(())
    }

    pub fn install_loaded_result(&mut self, ordered: OrderedObjMesh) -> Result<(), SceneLoadError> {
        let surfaces = result_display_surfaces(&ordered)?;
        let (left_eye, right_eye) = result_eye_surfaces(
            self.template_geometry.as_deref(),
            &surfaces.head.mesh,
            &surfaces.head.smoothing_topology,
        )?;
        self.result_camera
            .frame(surfaces.head.facial_focus_bounds());
        self.figure_camera.frame(surfaces.figure.bounds);
        self.result = Some(surfaces.head);
        self.result_figure = Some(surfaces.figure);
        self.result_tear_lacrimals = surfaces.tear_lacrimals;
        self.result_eyelashes = surfaces.eyelashes;
        self.result_left_eye = left_eye;
        self.result_right_eye = right_eye;
        self.result_sculpt_surfaces = surfaces.sculpt;
        let ordered = Arc::new(ordered);
        self.fitted_result = Some(Arc::clone(&ordered));
        self.result_output = Some(ordered);
        self.result_display_preview_signature = None;
        Ok(())
    }

    pub fn result_sculpt_surface(&self, group: SculptSurfaceGroup) -> Option<Arc<SurfaceMesh>> {
        (self.result.is_some() && self.result_output.is_some())
            .then(|| self.result_sculpt_surfaces.surface(group))
            .flatten()
    }

    pub fn result_sculpt_surfaces(&self) -> impl Iterator<Item = SculptSurfaceDescriptor> + '_ {
        SculptSurfaceGroup::ALL.into_iter().filter_map(|group| {
            self.result_sculpt_surface(group)
                .map(|mesh| SculptSurfaceDescriptor { group, mesh })
        })
    }

    pub fn result_eye_surfaces(&self) -> Option<(Arc<SurfaceMesh>, Arc<SurfaceMesh>)> {
        self.result.as_ref()?;
        Some((
            self.result_left_eye.as_ref()?.clone(),
            self.result_right_eye.as_ref()?.clone(),
        ))
    }

    pub fn frame_linked_edit_cameras(&mut self) -> bool {
        let (mut camera, bounds) = if let Some(template) = self.template.as_deref() {
            (self.template_camera, template.facial_focus_bounds())
        } else if let Some(scan) = self.scan.as_deref() {
            (self.scan_camera, scan.facial_focus_bounds())
        } else {
            return false;
        };
        camera.frame(bounds);
        self.scan_camera = camera;
        self.template_camera = camera;
        true
    }

    pub fn reconcile_linked_edit_cameras(&mut self, authoritative: MeshSide) {
        match authoritative {
            MeshSide::Scan => self.template_camera = self.scan_camera,
            MeshSide::Template => self.scan_camera = self.template_camera,
        }
    }
}

fn build_bvh_node(
    acceleration: &[TriangleAcceleration],
    triangle_ids: &mut [u32],
    absolute_start: usize,
    nodes: &mut Vec<BvhNode>,
) -> usize {
    let node_id = nodes.len();
    let bounds = triangle_ids
        .iter()
        .map(|&id| acceleration[id as usize].bounds)
        .reduce(Bounds3::union)
        .unwrap_or(Bounds3 {
            min: Vec3::ZERO,
            max: Vec3::ZERO,
        });
    nodes.push(BvhNode {
        bounds,
        kind: BvhNodeKind::Leaf {
            start: absolute_start,
            count: triangle_ids.len(),
        },
    });
    if triangle_ids.len() <= BVH_LEAF_TRIANGLES {
        return node_id;
    }

    let centroid_bounds = Bounds3::from_vec3_points(
        triangle_ids
            .iter()
            .map(|&id| acceleration[id as usize].centroid),
    );
    let axis = (0..3)
        .max_by(|&left, &right| {
            centroid_bounds
                .axis_extent(left)
                .total_cmp(&centroid_bounds.axis_extent(right))
                .then_with(|| right.cmp(&left))
        })
        .unwrap_or(0);

    let midpoint = triangle_ids.len() / 2;
    triangle_ids.select_nth_unstable_by(midpoint, |&left, &right| {
        acceleration[left as usize].centroid[axis]
            .total_cmp(&acceleration[right as usize].centroid[axis])
            .then_with(|| left.cmp(&right))
    });
    let (left_ids, right_ids) = triangle_ids.split_at_mut(midpoint);
    let left = build_bvh_node(acceleration, left_ids, absolute_start, nodes);
    let right = build_bvh_node(acceleration, right_ids, absolute_start + midpoint, nodes);
    nodes[node_id].kind = BvhNodeKind::Branch { left, right };
    node_id
}

fn triangle_bounds(mesh: &Mesh, triangle_id: u32) -> Bounds3 {
    let triangle = mesh.triangles[triangle_id as usize];
    let points = triangle.map(|index| DVec3::from_array(mesh.vertices[index as usize]).as_vec3());
    Bounds3::from_points(&points)
}

fn ray_bounds_distance(ray: Ray3, bounds: Bounds3) -> Option<f64> {
    let min = bounds.min.as_dvec3();
    let max = bounds.max.as_dvec3();
    let mut near = 0.0_f64;
    let mut far = f64::INFINITY;
    for axis in 0..3 {
        let origin = ray.origin[axis];
        let direction = ray.direction[axis];
        if direction.abs() <= 1.0e-15 {
            if origin < min[axis] || origin > max[axis] {
                return None;
            }
            continue;
        }
        let inverse = direction.recip();
        let mut first = (min[axis] - origin) * inverse;
        let mut second = (max[axis] - origin) * inverse;
        if first > second {
            std::mem::swap(&mut first, &mut second);
        }
        near = near.max(first);
        far = far.min(second);
        if far < near {
            return None;
        }
    }
    Some(near)
}

fn intersect_triangle(mesh: &Mesh, ray: Ray3, triangle_id: u32) -> Option<SurfaceHit> {
    let [ia, ib, ic] = mesh.triangles[triangle_id as usize];
    let a = DVec3::from_array(mesh.vertices[ia as usize]);
    let b = DVec3::from_array(mesh.vertices[ib as usize]);
    let c = DVec3::from_array(mesh.vertices[ic as usize]);
    let edge_ab = b - a;
    let edge_ac = c - a;
    let perpendicular = ray.direction.cross(edge_ac);
    let determinant = edge_ab.dot(perpendicular);
    if determinant.abs() <= 1.0e-14 {
        return None;
    }
    let inverse = determinant.recip();
    let from_a = ray.origin - a;
    let weight_b = from_a.dot(perpendicular) * inverse;
    if !(-1.0e-10..=1.0 + 1.0e-10).contains(&weight_b) {
        return None;
    }
    let crossed = from_a.cross(edge_ab);
    let weight_c = ray.direction.dot(crossed) * inverse;
    if weight_c < -1.0e-10 || weight_b + weight_c > 1.0 + 1.0e-10 {
        return None;
    }
    let distance = edge_ac.dot(crossed) * inverse;
    if distance <= 1.0e-9 || !distance.is_finite() {
        return None;
    }
    let weight_a = 1.0 - weight_b - weight_c;
    Some(SurfaceHit {
        triangle: triangle_id,
        barycentric: [weight_a, weight_b, weight_c],
        distance,
        local_point: ray.at(distance),
    })
}

fn vertex_normals(mesh: &Mesh, visible_triangle_ids: &[u32]) -> Vec<[f32; 3]> {
    let mut accumulated = vec![DVec3::ZERO; mesh.vertices.len()];
    for &[ia, ib, ic] in visible_triangle_ids
        .iter()
        .map(|&triangle| &mesh.triangles[triangle as usize])
    {
        let a = DVec3::from_array(mesh.vertices[ia as usize]);
        let b = DVec3::from_array(mesh.vertices[ib as usize]);
        let c = DVec3::from_array(mesh.vertices[ic as usize]);
        let normal = (b - a).cross(c - a);
        if normal.is_finite() {
            accumulated[ia as usize] += normal;
            accumulated[ib as usize] += normal;
            accumulated[ic as usize] += normal;
        }
    }
    accumulated
        .into_iter()
        .map(|normal| {
            let normal = normal.normalize_or(DVec3::Y).as_vec3();
            normal.to_array()
        })
        .collect()
}

fn unique_wire_indices(mesh: &Mesh, visible_triangle_ids: &[u32]) -> Vec<u32> {
    let mut indices = Vec::with_capacity(visible_triangle_ids.len().saturating_mul(6));
    for &[a, b, c] in visible_triangle_ids
        .iter()
        .map(|&triangle| &mesh.triangles[triangle as usize])
    {
        indices.extend_from_slice(&[a, b, b, c, c, a]);
    }
    indices
}

#[cfg(test)]
mod tests;

impl WorkspaceScene {
    pub fn stage_camera(&self, tab: crate::state::Tab) -> TurntableCamera {
        use crate::state::Tab;
        match tab {
            Tab::Alignment | Tab::Edit => {
                if self.template.is_some() {
                    self.template_camera
                } else {
                    self.scan_camera
                }
            }
            Tab::Morph | Tab::Texture | Tab::Result => self.result_camera,
        }
    }

    pub fn stage_camera_mut(&mut self, tab: crate::state::Tab) -> Option<&mut TurntableCamera> {
        use crate::state::Tab;
        match tab {
            Tab::Alignment | Tab::Edit => None,
            Tab::Morph | Tab::Texture | Tab::Result => Some(&mut self.result_camera),
        }
    }
}
