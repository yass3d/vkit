use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};

use glam::DVec3;
use thiserror::Error;

use crate::formats::OrderedObjMesh;

pub const MIN_BRUSH_RADIUS: f64 = 1.0e-6;
pub const POSITION_EPSILON_SQUARED: f64 = 1.0e-24;

const MIN_NECK_BOUNDARY_LOOP_VERTICES: usize = 4;

const NECK_BOUNDARY_LOWEST_FRACTION: f64 = 0.25;

const MIN_FRONT_SURFACE_NORMAL_DOT: f64 = -0.5;

const MAX_FRONT_FACE_INCOMING_DOT: f64 = -1.0e-6;

const CREASE_PIN_NORMAL_DOT: f64 = -0.5;

pub const SMOOTH_JACOBI_PASSES: usize = 4;

pub const SMOOTH_ANTISHRINK: f64 = 0.8;
pub const MAX_SMOOTH_EDGE_FRACTION: f64 = 0.50;
const MIN_VALID_TRIANGLE_DOUBLE_AREA: f64 = 1.0e-12;

pub const MIN_SMOOTH_STROKE_AREA_RATIO: f64 = 0.35;
pub const MIN_SMOOTH_TRIANGLE_AREA_RATIO: f64 = 0.25;
const SMOOTH_LOCAL_BACKTRACK_STEPS: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SculptTarget {
    HeadSkin = 1 << 0,
    Tear = 1 << 1,
    Eyelashes = 1 << 2,
    TeethTongue = 1 << 3,
    Eyes = 1 << 4,
    InnerMouth = 1 << 5,
    Lips = 1 << 6,
}

impl SculptTarget {
    pub const ALL: [Self; 7] = [
        Self::HeadSkin,
        Self::Tear,
        Self::Eyelashes,
        Self::Eyes,
        Self::Lips,
        Self::TeethTongue,
        Self::InnerMouth,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SculptTargets(u8);

impl SculptTargets {
    pub const NONE: Self = Self(0);
    pub const HEAD_SKIN: Self = Self(SculptTarget::HeadSkin as u8);
    pub const TEAR: Self = Self(SculptTarget::Tear as u8);
    pub const EYELASHES: Self = Self(SculptTarget::Eyelashes as u8);
    pub const TEETH_TONGUE: Self = Self(SculptTarget::TeethTongue as u8);
    pub const EYES: Self = Self(SculptTarget::Eyes as u8);
    pub const INNER_MOUTH: Self = Self(SculptTarget::InnerMouth as u8);
    pub const LIPS: Self = Self(SculptTarget::Lips as u8);
    pub const FACE_SURFACE: Self = Self(
        SculptTarget::HeadSkin as u8
            | SculptTarget::Tear as u8
            | SculptTarget::Eyelashes as u8
            | SculptTarget::Lips as u8,
    );
    pub const ALL: Self = Self(
        SculptTarget::HeadSkin as u8
            | SculptTarget::Tear as u8
            | SculptTarget::Eyelashes as u8
            | SculptTarget::TeethTongue as u8
            | SculptTarget::Eyes as u8
            | SculptTarget::InnerMouth as u8
            | SculptTarget::Lips as u8,
    );

    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    pub const fn contains(self, target: SculptTarget) -> bool {
        self.0 & target as u8 != 0
    }

    pub const fn with(self, target: SculptTarget) -> Self {
        Self(self.0 | target as u8)
    }

    pub const fn without(self, target: SculptTarget) -> Self {
        Self(self.0 & !(target as u8))
    }

    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn bits(self) -> u8 {
        self.0
    }

    pub const fn intersects_bits(self, bits: u8) -> bool {
        self.0 & bits != 0
    }
}

impl Default for SculptTargets {
    fn default() -> Self {
        Self::FACE_SURFACE
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SculptFalloff {
    #[default]
    Smooth,
    Smoother,
    Sharp,
    Linear,
}

impl SculptFalloff {
    pub fn weight(self, normalized_distance: f64) -> f64 {
        if !normalized_distance.is_finite() {
            return 0.0;
        }
        let inside = (1.0 - normalized_distance).clamp(0.0, 1.0);
        match self {
            Self::Smooth => inside * inside * (3.0 - 2.0 * inside),
            Self::Smoother => inside * inside * inside * (inside * (inside * 6.0 - 15.0) + 10.0),
            Self::Sharp => inside * inside,
            Self::Linear => inside,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SculptOperation {
    Grab { translation_local: [f64; 3] },

    Smooth,

    Inflate { distance: f64 },

    Restore,

    RestoreFit,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SculptHit {
    pub point_local: [f64; 3],

    pub normal_local: [f64; 3],
    pub triangle_index: u32,
    pub barycentric: [f64; 3],
    pub distance: f64,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SculptError {
    #[error("sculpt requires a valid result mesh: {0}")]
    InvalidMesh(String),
    #[error("the sculpt stage has not been initialized")]
    NotInitialized,
    #[error("a sculpt stroke is already active")]
    StrokeAlreadyActive,
    #[error("no sculpt stroke is active")]
    NoActiveStroke,
    #[error("sculpt dab contains a non-finite or invalid parameter")]
    InvalidDab,
}

#[derive(Clone, Copy, Debug)]
pub struct StrokeAnchor {
    pub point: DVec3,
    pub normal: DVec3,
    pub seed_triangle: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SculptOperationKind {
    Grab,
    Smooth,
    Inflate,
    Restore,
}

impl SculptOperationKind {
    pub const fn of(operation: SculptOperation) -> Self {
        match operation {
            SculptOperation::Grab { .. } => Self::Grab,
            SculptOperation::Smooth => Self::Smooth,
            SculptOperation::Inflate { .. } => Self::Inflate,
            SculptOperation::Restore | SculptOperation::RestoreFit => Self::Restore,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BrushInfluence {
    pub vertex: usize,

    pub falloff: f64,

    pub radial_falloff: f64,
    pub brush_strength: f64,

    pub sheet_normal: Option<DVec3>,

    pub surface_bits: u8,

    pub restrict_to_front_sheet: bool,
    pub incoming_direction: Option<DVec3>,
}

#[derive(Clone, Copy, Debug)]
pub struct InfluenceSettings {
    pub radius: f64,
    pub strength: f64,
    pub targets: SculptTargets,
    pub falloff: SculptFalloff,
    pub backface_masking: bool,
    pub incoming_direction: Option<DVec3>,

    pub include_nearby_components: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct TopologyNeighbor {
    pub vertex: u32,
    pub target_bits: u8,
}

#[derive(Clone, Copy, Debug)]
struct DistanceQueueEntry {
    distance: f64,
    vertex: u32,
}

impl PartialEq for DistanceQueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.vertex == other.vertex && self.distance.to_bits() == other.distance.to_bits()
    }
}

impl Eq for DistanceQueueEntry {}

impl PartialOrd for DistanceQueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DistanceQueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .distance
            .total_cmp(&self.distance)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

#[derive(Debug, Default)]
pub struct InfluenceScratch {
    stamp: u64,
    stamps: Vec<u64>,
    distances: Vec<f64>,
    touched: Vec<u32>,
    queue: BinaryHeap<DistanceQueueEntry>,
}

impl InfluenceScratch {
    fn begin(&mut self, vertex_count: usize) {
        if self.stamps.len() != vertex_count {
            self.stamps = vec![0; vertex_count];
            self.distances = vec![f64::INFINITY; vertex_count];
            self.stamp = 1;
        } else {
            self.stamp = self.stamp.wrapping_add(1);
            if self.stamp == 0 {
                self.stamps.fill(0);
                self.stamp = 1;
            }
        }
        self.touched.clear();
        self.queue.clear();
    }

    #[inline]
    fn distance(&self, vertex: usize) -> f64 {
        if self.stamps[vertex] == self.stamp {
            self.distances[vertex]
        } else {
            f64::INFINITY
        }
    }

    #[inline]
    fn set_distance(&mut self, vertex: usize, distance: f64) {
        if self.stamps[vertex] != self.stamp {
            self.stamps[vertex] = self.stamp;
            self.touched.push(vertex as u32);
        }
        self.distances[vertex] = distance;
    }
}

#[derive(Debug)]
pub struct SculptTopology {
    pub triangles: Vec<[u32; 3]>,
    pub triangle_targets: Vec<u8>,
    pub vertex_targets: Vec<u8>,
    pub adjacency: Vec<Vec<TopologyNeighbor>>,
    pub incident_triangles: Vec<Vec<u32>>,

    pub boundary_vertex_targets: Vec<u8>,

    pub grab_protected_vertices: Vec<bool>,
}

impl SculptTopology {
    pub fn build(mesh: &OrderedObjMesh) -> Result<Self, SculptError> {
        mesh.validate()
            .map_err(|error| SculptError::InvalidMesh(error.to_string()))?;
        let mut triangles = Vec::new();
        let mut triangle_targets = Vec::new();
        let mut vertex_targets = vec![0_u8; mesh.vertices.len()];
        let mut adjacency = vec![BTreeMap::<u32, u8>::new(); mesh.vertices.len()];
        let mut edge_target_face_counts =
            BTreeMap::<(u32, u32), [u16; SculptTarget::ALL.len()]>::new();
        for face in &mesh.faces {
            let targets = face_target_bits(face.material.as_deref(), face.group.as_deref());
            for &vertex in &face.vertex_indices {
                vertex_targets[vertex as usize] |= targets;
            }

            for edge in face.vertex_indices.windows(2) {
                add_edge(&mut adjacency, edge[0], edge[1], targets);
                count_edge_target_face(&mut edge_target_face_counts, edge[0], edge[1], targets);
            }
            let last = *face.vertex_indices.last().expect("validated face");
            let first = face.vertex_indices[0];
            add_edge(&mut adjacency, last, first, targets);
            count_edge_target_face(&mut edge_target_face_counts, last, first, targets);
            for corner in 1..face.vertex_indices.len() - 1 {
                triangles.push([
                    first,
                    face.vertex_indices[corner],
                    face.vertex_indices[corner + 1],
                ]);
                triangle_targets.push(targets);
            }
        }

        let mut incident_triangles = vec![Vec::new(); mesh.vertices.len()];
        for (triangle_index, &[a, b, c]) in triangles.iter().enumerate() {
            for vertex in [a, b, c] {
                incident_triangles[vertex as usize].push(triangle_index as u32);
            }
        }
        let mut boundary_vertex_targets = vec![0_u8; mesh.vertices.len()];
        let mut head_skin_boundary = BTreeMap::<u32, BTreeSet<u32>>::new();
        for ((a, b), counts) in edge_target_face_counts {
            for (target_index, count) in counts.into_iter().enumerate() {
                if count == 0 || count == 2 {
                    continue;
                }
                let target_bit = SculptTarget::ALL[target_index] as u8;
                boundary_vertex_targets[a as usize] |= target_bit;
                boundary_vertex_targets[b as usize] |= target_bit;

                if SculptTarget::ALL[target_index] == SculptTarget::HeadSkin && count == 1 {
                    head_skin_boundary.entry(a).or_default().insert(b);
                    head_skin_boundary.entry(b).or_default().insert(a);
                }
            }
        }
        let grab_protected_vertices =
            lower_neck_weld_vertices(&mesh.vertices, &vertex_targets, &head_skin_boundary);
        let adjacency = adjacency
            .into_iter()
            .map(|neighbors| {
                neighbors
                    .into_iter()
                    .map(|(vertex, target_bits)| TopologyNeighbor {
                        vertex,
                        target_bits,
                    })
                    .collect()
            })
            .collect();
        Ok(Self {
            triangles,
            triangle_targets,
            vertex_targets,
            adjacency,
            incident_triangles,
            boundary_vertex_targets,
            grab_protected_vertices,
        })
    }
}

#[derive(Debug)]
struct NeckBoundaryCandidate {
    vertices: Vec<u32>,
    mean_y: f64,
    horizontal_perimeter: f64,
}

pub fn lower_neck_weld_vertices(
    vertices: &[[f64; 3]],
    vertex_targets: &[u8],
    boundary: &BTreeMap<u32, BTreeSet<u32>>,
) -> Vec<bool> {
    let mut protected = vec![false; vertices.len()];
    let head_skin = SculptTarget::HeadSkin as u8;
    let mut lowest_y = f64::INFINITY;
    let mut highest_y = f64::NEG_INFINITY;
    for (&point, &targets) in vertices.iter().zip(vertex_targets) {
        if targets & head_skin == 0 || !point.iter().all(|value| value.is_finite()) {
            continue;
        }
        lowest_y = lowest_y.min(point[1]);
        highest_y = highest_y.max(point[1]);
    }
    let vertical_extent = highest_y - lowest_y;
    if !vertical_extent.is_finite() || vertical_extent <= MIN_BRUSH_RADIUS {
        return protected;
    }
    let lower_band_high = lowest_y + vertical_extent * NECK_BOUNDARY_LOWEST_FRACTION;
    let mut seen = BTreeSet::new();
    let mut candidates = Vec::new();
    for &start in boundary.keys() {
        if !seen.insert(start) {
            continue;
        }
        let mut stack = vec![start];
        let mut component = Vec::new();
        while let Some(vertex) = stack.pop() {
            component.push(vertex);
            if let Some(neighbors) = boundary.get(&vertex) {
                for &neighbor in neighbors.iter().rev() {
                    if seen.insert(neighbor) {
                        stack.push(neighbor);
                    }
                }
            }
        }
        component.sort_unstable();
        if component.len() < MIN_NECK_BOUNDARY_LOOP_VERTICES
            || !component.iter().all(|&vertex| {
                boundary
                    .get(&vertex)
                    .is_some_and(|neighbors| neighbors.len() == 2)
            })
        {
            continue;
        }
        let points = component
            .iter()
            .filter_map(|&vertex| vertices.get(vertex as usize))
            .collect::<Vec<_>>();
        if points.len() != component.len()
            || !points
                .iter()
                .all(|point| point.iter().all(|value| value.is_finite()))
        {
            continue;
        }
        let mean_y = points.iter().map(|point| point[1]).sum::<f64>() / points.len() as f64;
        if !mean_y.is_finite() || mean_y > lower_band_high {
            continue;
        }
        let horizontal_perimeter = component
            .iter()
            .copied()
            .flat_map(|vertex| {
                boundary
                    .get(&vertex)
                    .into_iter()
                    .flat_map(move |neighbors| {
                        neighbors
                            .range((vertex + 1)..)
                            .copied()
                            .map(move |neighbor| (vertex, neighbor))
                    })
            })
            .map(|(first, second)| {
                let first = DVec3::from_array(vertices[first as usize]);
                let second = DVec3::from_array(vertices[second as usize]);
                DVec3::new(first.x - second.x, 0.0, first.z - second.z).length()
            })
            .sum::<f64>();
        if !horizontal_perimeter.is_finite() || horizontal_perimeter <= MIN_BRUSH_RADIUS {
            continue;
        }
        candidates.push(NeckBoundaryCandidate {
            vertices: component,
            mean_y,
            horizontal_perimeter,
        });
    }
    let Some(candidate) = candidates.into_iter().min_by(|left, right| {
        left.mean_y
            .total_cmp(&right.mean_y)
            .then_with(|| {
                right
                    .horizontal_perimeter
                    .total_cmp(&left.horizontal_perimeter)
            })
            .then_with(|| left.vertices.cmp(&right.vertices))
    }) else {
        return protected;
    };
    for vertex in candidate.vertices {
        protected[vertex as usize] = true;
    }
    protected
}

pub fn nearest_visible_ray_hit(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    origin: DVec3,
    direction: DVec3,
    visible_targets: SculptTargets,
    backface_masking: bool,
) -> Option<SculptHit> {
    let mut nearest: Option<SculptHit> = None;
    for (triangle_index, &triangle) in topology.triangles.iter().enumerate() {
        let target_bits = topology.triangle_targets[triangle_index];
        if target_bits != 0 && !visible_targets.intersects_bits(target_bits) {
            continue;
        }
        let [a, b, c] = triangle.map(|index| DVec3::from_array(vertices[index as usize]));
        let Some((distance, barycentric)) =
            ray_triangle(origin, direction, a, b, c, backface_masking)
        else {
            continue;
        };
        if nearest
            .as_ref()
            .is_some_and(|current| current.distance <= distance)
        {
            continue;
        }
        let normal = interpolated_hit_normal(triangle, barycentric, vertices, topology)
            .or_else(|| triangle_normal(triangle, vertices))
            .unwrap_or(-direction);
        nearest = Some(SculptHit {
            point_local: (origin + direction * distance).to_array(),
            normal_local: normal.to_array(),
            triangle_index: triangle_index as u32,
            barycentric,
            distance,
        });
    }
    nearest
}

#[derive(Clone, Copy, Debug)]
struct BrushSurfaceCandidate {
    hit: SculptHit,
    perpendicular_distance_squared: f64,
}

pub struct BrushHaloQuery {
    pub origin: DVec3,
    pub direction: DVec3,
    pub visible_targets: SculptTargets,
    pub editable_targets: SculptTargets,
    pub pick_radius: f64,
    pub backface_masking: bool,
}

pub fn nearest_brush_surface_hit(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    query: &BrushHaloQuery,
) -> Option<SculptHit> {
    let &BrushHaloQuery {
        origin,
        direction,
        visible_targets,
        editable_targets,
        pick_radius,
        backface_masking,
    } = query;
    let radius_squared = pick_radius * pick_radius;
    let mut nearest: Option<BrushSurfaceCandidate> = None;
    for (triangle_index, &triangle) in topology.triangles.iter().enumerate() {
        let target_bits = topology.triangle_targets[triangle_index];
        if !visible_targets.intersects_bits(target_bits)
            || !editable_targets.intersects_bits(target_bits)
        {
            continue;
        }
        let Some(triangle_normal) = triangle_normal(triangle, vertices) else {
            continue;
        };
        if backface_masking && triangle_normal.dot(direction) > MAX_FRONT_FACE_INCOMING_DOT {
            continue;
        }

        let [a, b, c] = triangle.map(|index| DVec3::from_array(vertices[index as usize]));
        let mut closest: Option<(DVec3, f64, f64, usize)> = None;
        for (edge_index, (start, end)) in [(a, b), (b, c), (c, a)].into_iter().enumerate() {
            let Some((point, depth, perpendicular_squared)) =
                closest_positive_point_on_segment_to_ray(origin, direction, start, end)
            else {
                continue;
            };
            let replaces = closest.as_ref().is_none_or(|current| {
                perpendicular_squared
                    .total_cmp(&current.2)
                    .then_with(|| depth.total_cmp(&current.1))
                    .then_with(|| edge_index.cmp(&current.3))
                    == Ordering::Less
            });
            if replaces {
                closest = Some((point, depth, perpendicular_squared, edge_index));
            }
        }
        let Some((point, depth, perpendicular_squared, _)) = closest else {
            continue;
        };
        if perpendicular_squared > radius_squared {
            continue;
        }
        let barycentric = barycentric_coordinates(point, a, b, c).unwrap_or([1.0 / 3.0; 3]);
        let normal = interpolated_hit_normal(triangle, barycentric, vertices, topology)
            .unwrap_or(triangle_normal);
        let candidate = BrushSurfaceCandidate {
            hit: SculptHit {
                point_local: point.to_array(),
                normal_local: normal.to_array(),
                triangle_index: triangle_index as u32,
                barycentric,
                distance: depth,
            },
            perpendicular_distance_squared: perpendicular_squared,
        };
        let replaces = nearest.as_ref().is_none_or(|current| {
            candidate
                .perpendicular_distance_squared
                .total_cmp(&current.perpendicular_distance_squared)
                .then_with(|| candidate.hit.distance.total_cmp(&current.hit.distance))
                .then_with(|| {
                    candidate
                        .hit
                        .triangle_index
                        .cmp(&current.hit.triangle_index)
                })
                == Ordering::Less
        });
        if replaces {
            nearest = Some(candidate);
        }
    }
    nearest.map(|candidate| candidate.hit)
}

fn closest_positive_point_on_segment_to_ray(
    origin: DVec3,
    direction: DVec3,
    start: DVec3,
    end: DVec3,
) -> Option<(DVec3, f64, f64)> {
    let segment = end - start;
    let from_origin = start - origin;
    let start_depth = from_origin.dot(direction);
    let depth_step = segment.dot(direction);
    let projected_start = from_origin - direction * start_depth;
    let projected_step = segment - direction * depth_step;
    let projected_length_squared = projected_step.length_squared();
    let closest_parameter = if projected_length_squared > POSITION_EPSILON_SQUARED {
        (-projected_start.dot(projected_step) / projected_length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let mut nearest: Option<(DVec3, f64, f64, usize)> = None;
    for (order, parameter) in [closest_parameter, 0.0, 1.0].into_iter().enumerate() {
        let point = start + segment * parameter;
        let from_origin = point - origin;
        let depth = from_origin.dot(direction);
        if !point.is_finite() || !depth.is_finite() || depth <= 1.0e-9 {
            continue;
        }
        let perpendicular = from_origin - direction * depth;
        let perpendicular_squared = perpendicular.length_squared();
        if !perpendicular_squared.is_finite() {
            continue;
        }
        let replaces = nearest.as_ref().is_none_or(|current| {
            perpendicular_squared
                .total_cmp(&current.2)
                .then_with(|| depth.total_cmp(&current.1))
                .then_with(|| order.cmp(&current.3))
                == Ordering::Less
        });
        if replaces {
            nearest = Some((point, depth, perpendicular_squared, order));
        }
    }
    nearest.map(|(point, depth, perpendicular_squared, _)| (point, depth, perpendicular_squared))
}

pub fn normalize_stroke_direction(
    direction_local: Option<[f64; 3]>,
) -> Result<Option<DVec3>, SculptError> {
    direction_local
        .map(|direction| {
            let direction = DVec3::from_array(direction);
            direction
                .is_finite()
                .then(|| direction.try_normalize())
                .flatten()
                .ok_or(SculptError::InvalidDab)
        })
        .transpose()
}

pub fn mirror_x_point(point: DVec3) -> DVec3 {
    DVec3::new(-point.x, point.y, point.z)
}

pub fn mirror_x_vector(vector: DVec3) -> DVec3 {
    DVec3::new(-vector.x, vector.y, vector.z)
}

pub fn mirror_x_operation(operation: SculptOperation) -> SculptOperation {
    match operation {
        SculptOperation::Grab { translation_local } => SculptOperation::Grab {
            translation_local: mirror_x_vector(DVec3::from_array(translation_local)).to_array(),
        },
        SculptOperation::Smooth => SculptOperation::Smooth,
        SculptOperation::Inflate { distance } => SculptOperation::Inflate { distance },
        SculptOperation::Restore => SculptOperation::Restore,
        SculptOperation::RestoreFit => SculptOperation::RestoreFit,
    }
}

#[derive(Clone, Copy)]
pub struct SculptBranch<'basis> {
    pub operation: SculptOperation,

    pub anchored_normal: Option<DVec3>,

    pub restore_basis: Option<&'basis [[f64; 3]]>,
}

pub fn sculpt_branch_proposals(
    branch: SculptBranch<'_>,
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    targets: SculptTargets,
    influence: &[BrushInfluence],
    reference_areas: &BTreeMap<u32, f64>,
) -> Vec<(usize, [f64; 3])> {
    let SculptBranch {
        operation,
        anchored_normal,
        restore_basis,
    } = branch;
    if matches!(operation, SculptOperation::Smooth) {
        return jacobi_smooth_proposals(vertices, topology, targets, influence, reference_areas);
    }
    if matches!(
        operation,
        SculptOperation::Restore | SculptOperation::RestoreFit
    ) {
        let Some(basis) = restore_basis else {
            return Vec::new();
        };
        return influence
            .iter()
            .filter_map(|entry| {
                let target = DVec3::from_array(*basis.get(entry.vertex)?);
                let point = DVec3::from_array(vertices[entry.vertex]);
                let next = point.lerp(target, entry.falloff.clamp(0.0, 1.0));
                (next.is_finite() && next.distance_squared(point) > POSITION_EPSILON_SQUARED)
                    .then_some((entry.vertex, next.to_array()))
            })
            .collect();
    }
    let displacement = match operation {
        SculptOperation::Grab { translation_local } => DVec3::from_array(translation_local),
        SculptOperation::Inflate { distance } => anchored_normal.unwrap_or(DVec3::ZERO) * distance,
        SculptOperation::Smooth | SculptOperation::Restore | SculptOperation::RestoreFit => {
            unreachable!("Smooth and Restore returned above")
        }
    };
    if !displacement.is_finite() || displacement.length_squared() <= POSITION_EPSILON_SQUARED {
        return Vec::new();
    }
    influence
        .iter()
        .filter_map(|entry| {
            if matches!(operation, SculptOperation::Grab { .. })
                && topology.grab_protected_vertices[entry.vertex]
            {
                return None;
            }
            let point = DVec3::from_array(vertices[entry.vertex]);
            let next = point + displacement * entry.falloff;
            (next.is_finite() && next.distance_squared(point) > POSITION_EPSILON_SQUARED)
                .then_some((entry.vertex, next.to_array()))
        })
        .collect()
}

pub fn merge_symmetric_proposals(
    vertices: &[[f64; 3]],
    primary: Vec<(usize, [f64; 3])>,
    mirrored: Vec<(usize, [f64; 3])>,
) -> Vec<(usize, [f64; 3])> {
    let mut accumulated = BTreeMap::<usize, (DVec3, u32)>::new();
    for (index, target) in primary.into_iter().chain(mirrored) {
        let delta = DVec3::from_array(target) - DVec3::from_array(vertices[index]);
        let entry = accumulated.entry(index).or_insert((DVec3::ZERO, 0));
        entry.0 += delta;
        entry.1 += 1;
    }
    accumulated
        .into_iter()
        .filter_map(|(index, (delta_sum, count))| {
            let original = DVec3::from_array(vertices[index]);
            let next = original + delta_sum / f64::from(count);
            (next.is_finite() && next.distance_squared(original) > POSITION_EPSILON_SQUARED)
                .then_some((index, next.to_array()))
        })
        .collect()
}

pub fn interpolated_hit_normal(
    triangle: [u32; 3],
    barycentric: [f64; 3],
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
) -> Option<DVec3> {
    let mut normal = DVec3::ZERO;
    for (&vertex, weight) in triangle.iter().zip(barycentric) {
        if weight.is_finite() && weight > 0.0 {
            normal += vertex_normal_at(vertex as usize, vertices, topology) * weight;
        }
    }
    normal.try_normalize()
}

pub fn stroke_anchor_from_seed(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    sample: DVec3,
    seed_triangle: u32,
) -> Option<StrokeAnchor> {
    let &triangle = topology.triangles.get(seed_triangle as usize)?;
    let [a, b, c] = triangle.map(|vertex| DVec3::from_array(vertices[vertex as usize]));
    let point = closest_point_on_triangle(sample, a, b, c);
    let barycentric = barycentric_coordinates(point, a, b, c).unwrap_or([1.0 / 3.0; 3]);
    let normal = interpolated_hit_normal(triangle, barycentric, vertices, topology)
        .or_else(|| triangle_normal(triangle, vertices))?;
    Some(StrokeAnchor {
        point,
        normal,
        seed_triangle,
    })
}

fn barycentric_coordinates(point: DVec3, a: DVec3, b: DVec3, c: DVec3) -> Option<[f64; 3]> {
    let ab = b - a;
    let ac = c - a;
    let ap = point - a;
    let d00 = ab.dot(ab);
    let d01 = ab.dot(ac);
    let d11 = ac.dot(ac);
    let d20 = ap.dot(ab);
    let d21 = ap.dot(ac);
    let denominator = d00 * d11 - d01 * d01;
    if !denominator.is_finite() || denominator.abs() <= 1.0e-24 {
        return None;
    }
    let v = (d11 * d20 - d01 * d21) / denominator;
    let w = (d00 * d21 - d01 * d20) / denominator;
    let u = 1.0 - v - w;
    [u, v, w]
        .into_iter()
        .all(f64::is_finite)
        .then_some([u, v, w])
}

fn add_edge(adjacency: &mut [BTreeMap<u32, u8>], a: u32, b: u32, target_bits: u8) {
    adjacency[a as usize]
        .entry(b)
        .and_modify(|bits| *bits |= target_bits)
        .or_insert(target_bits);
    adjacency[b as usize]
        .entry(a)
        .and_modify(|bits| *bits |= target_bits)
        .or_insert(target_bits);
}

fn count_edge_target_face(
    counts: &mut BTreeMap<(u32, u32), [u16; SculptTarget::ALL.len()]>,
    a: u32,
    b: u32,
    target_bits: u8,
) {
    let edge = if a <= b { (a, b) } else { (b, a) };
    let entry = counts.entry(edge).or_default();
    for (index, target) in SculptTarget::ALL.into_iter().enumerate() {
        if target_bits & target as u8 != 0 {
            entry[index] = entry[index].saturating_add(1);
        }
    }
}

pub fn nearest_target_triangle(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    center: DVec3,
    targets: SculptTargets,
) -> Option<u32> {
    nearest_target_triangle_with_point(vertices, topology, center, targets)
        .map(|(triangle, _, _)| triangle)
}

pub fn nearest_target_triangle_with_point(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    center: DVec3,
    targets: SculptTargets,
) -> Option<(u32, DVec3, f64)> {
    topology
        .triangles
        .iter()
        .zip(&topology.triangle_targets)
        .enumerate()
        .filter(|(_, (_, bits))| targets.intersects_bits(**bits))
        .map(|(index, (&triangle, _))| {
            let [a, b, c] = triangle.map(|vertex| DVec3::from_array(vertices[vertex as usize]));
            let point = closest_point_on_triangle(center, a, b, c);
            let distance_squared = point.distance_squared(center);
            (index as u32, point, distance_squared)
        })
        .filter(|(_, point, distance)| point.is_finite() && distance.is_finite())
        .min_by(|left, right| {
            left.2
                .total_cmp(&right.2)
                .then_with(|| left.0.cmp(&right.0))
        })
}

#[must_use]
pub fn nearest_target_vertex_position(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    point: DVec3,
    seed_triangle: u32,
    targets: SculptTargets,
) -> Option<DVec3> {
    let triangle = topology.triangles.get(seed_triangle as usize)?;
    triangle
        .iter()
        .map(|&vertex| vertex as usize)
        .filter(|&vertex| {
            topology
                .vertex_targets
                .get(vertex)
                .is_some_and(|bits| targets.intersects_bits(*bits))
        })
        .filter_map(|vertex| {
            vertices
                .get(vertex)
                .map(|position| DVec3::from_array(*position))
        })
        .filter(|position| position.is_finite())
        .min_by(|left, right| {
            left.distance_squared(point)
                .total_cmp(&right.distance_squared(point))
        })
}

fn geodesic_influence(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    center: DVec3,
    seed_triangle: u32,
    settings: InfluenceSettings,
    scratch: &mut InfluenceScratch,
) -> Vec<(usize, f64)> {
    let Some(&triangle) = topology.triangles.get(seed_triangle as usize) else {
        return Vec::new();
    };
    if !topology
        .triangle_targets
        .get(seed_triangle as usize)
        .is_some_and(|&bits| settings.targets.intersects_bits(bits))
    {
        return Vec::new();
    }
    let seed_normal = triangle_normal(triangle, vertices);
    if settings.backface_masking
        && settings.incoming_direction.is_some_and(|incoming| {
            !seed_normal.is_some_and(|normal| normal.dot(incoming) <= MAX_FRONT_FACE_INCOMING_DOT)
        })
    {
        return Vec::new();
    }

    let surface_targets = settings.targets;

    if !settings.backface_masking {
        return connected_spatial_influence(
            vertices,
            topology,
            center,
            triangle,
            surface_targets,
            settings.radius,
        );
    }

    scratch.begin(vertices.len());
    for vertex in triangle {
        let distance = DVec3::from_array(vertices[vertex as usize]).distance(center);
        if distance <= settings.radius && distance < scratch.distance(vertex as usize) {
            scratch.set_distance(vertex as usize, distance);
            scratch.queue.push(DistanceQueueEntry { distance, vertex });
        }
    }

    while let Some(DistanceQueueEntry { distance, vertex }) = scratch.queue.pop() {
        if distance > settings.radius || distance > scratch.distance(vertex as usize) {
            continue;
        }
        let from = DVec3::from_array(vertices[vertex as usize]);
        for neighbor in &topology.adjacency[vertex as usize] {
            let neighbor_index = neighbor.vertex as usize;
            if !surface_targets.intersects_bits(neighbor.target_bits)
                || !surface_targets.intersects_bits(topology.vertex_targets[neighbor_index])
                || (settings.backface_masking
                    && !edge_tracks_seed_surface(
                        vertex,
                        neighbor.vertex,
                        vertices,
                        topology,
                        surface_targets,
                        seed_normal,
                        settings.incoming_direction,
                    ))
            {
                continue;
            }
            let edge_length = from.distance(DVec3::from_array(vertices[neighbor_index]));
            if !edge_length.is_finite() {
                continue;
            }
            let next = distance + edge_length;
            if next <= settings.radius && next < scratch.distance(neighbor_index) {
                scratch.set_distance(neighbor_index, next);
                scratch.queue.push(DistanceQueueEntry {
                    distance: next,
                    vertex: neighbor.vertex,
                });
            }
        }
    }

    scratch.touched.sort_unstable();
    scratch
        .touched
        .iter()
        .map(|&vertex| (vertex as usize, scratch.distances[vertex as usize]))
        .filter(|(_, distance)| *distance <= settings.radius)
        .collect()
}

fn connected_spatial_influence(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    center: DVec3,
    seed_triangle: [u32; 3],
    surface_targets: SculptTargets,
    radius: f64,
) -> Vec<(usize, f64)> {
    let mut visited = vec![false; vertices.len()];
    let mut queue = VecDeque::new();
    for vertex in seed_triangle {
        let index = vertex as usize;
        if surface_targets.intersects_bits(topology.vertex_targets[index]) && !visited[index] {
            visited[index] = true;
            queue.push_back(vertex);
        }
    }

    while let Some(vertex) = queue.pop_front() {
        for neighbor in &topology.adjacency[vertex as usize] {
            let index = neighbor.vertex as usize;
            if visited[index]
                || !surface_targets.intersects_bits(neighbor.target_bits)
                || !surface_targets.intersects_bits(topology.vertex_targets[index])
            {
                continue;
            }
            visited[index] = true;
            queue.push_back(neighbor.vertex);
        }
    }

    visited
        .into_iter()
        .enumerate()
        .filter(|(_, connected)| *connected)
        .map(|(index, _)| (index, DVec3::from_array(vertices[index]).distance(center)))
        .filter(|(_, distance)| distance.is_finite() && *distance <= radius)
        .collect()
}

fn weighted_geodesic_influence(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    center: DVec3,
    seed_triangle: u32,
    settings: InfluenceSettings,
    scratch: &mut InfluenceScratch,
) -> Vec<BrushInfluence> {
    let surface_bits = settings.targets.bits();
    let sheet_normal = topology
        .triangles
        .get(seed_triangle as usize)
        .and_then(|&triangle| triangle_normal(triangle, vertices));
    geodesic_influence(vertices, topology, center, seed_triangle, settings, scratch)
        .into_iter()
        .filter_map(|(index, distance)| {
            let radial_falloff = settings.falloff.weight(distance / settings.radius);
            let falloff = radial_falloff * settings.strength;
            (falloff > 0.0).then_some(BrushInfluence {
                vertex: index,
                falloff,
                radial_falloff,
                brush_strength: settings.strength,
                sheet_normal,
                surface_bits,
                restrict_to_front_sheet: settings.backface_masking,
                incoming_direction: settings.incoming_direction,
            })
        })
        .collect()
}

pub fn weighted_sculpt_influence(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    center: DVec3,
    seed_triangle: u32,
    settings: InfluenceSettings,
) -> Vec<BrushInfluence> {
    let mut scratch = InfluenceScratch::default();
    weighted_sculpt_influence_with_scratch(
        vertices,
        topology,
        center,
        seed_triangle,
        settings,
        &mut scratch,
    )
}

pub fn weighted_sculpt_influence_with_scratch(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    center: DVec3,
    seed_triangle: u32,
    settings: InfluenceSettings,
    scratch: &mut InfluenceScratch,
) -> Vec<BrushInfluence> {
    let mut influence =
        weighted_geodesic_influence(vertices, topology, center, seed_triangle, settings, scratch);
    if !settings.include_nearby_components {
        return influence;
    }

    let mut visited = connected_component_vertices(topology, seed_triangle, settings.targets);
    for start in 0..topology.vertex_targets.len() {
        if visited[start]
            || !settings
                .targets
                .intersects_bits(topology.vertex_targets[start])
        {
            continue;
        }
        let component = collect_editable_component(topology, start, settings.targets, &mut visited);
        if !component_has_editable_surface_in_brush(
            &component, vertices, topology, center, settings,
        ) {
            continue;
        }
        let surface_bits = component.iter().fold(0_u8, |bits, &index| {
            bits | (topology.vertex_targets[index] & settings.targets.bits())
        });
        for index in component {
            let distance = DVec3::from_array(vertices[index]).distance(center);
            let radial_falloff = settings.falloff.weight(distance / settings.radius);
            let falloff = radial_falloff * settings.strength;
            if falloff > 0.0 {
                influence.push(BrushInfluence {
                    vertex: index,
                    falloff,
                    radial_falloff,
                    brush_strength: settings.strength,
                    sheet_normal: None,
                    surface_bits,
                    restrict_to_front_sheet: settings.backface_masking,
                    incoming_direction: settings.incoming_direction,
                });
            }
        }
    }
    influence.sort_unstable_by_key(|entry| entry.vertex);
    influence
}

fn connected_component_vertices(
    topology: &SculptTopology,
    seed_triangle: u32,
    targets: SculptTargets,
) -> Vec<bool> {
    let mut connected = vec![false; topology.vertex_targets.len()];
    let Some(&triangle) = topology.triangles.get(seed_triangle as usize) else {
        return connected;
    };
    let mut queue = VecDeque::new();
    for vertex in triangle {
        let index = vertex as usize;
        if targets.intersects_bits(topology.vertex_targets[index]) {
            connected[index] = true;
            queue.push_back(vertex);
        }
    }
    while let Some(vertex) = queue.pop_front() {
        for neighbor in &topology.adjacency[vertex as usize] {
            let index = neighbor.vertex as usize;
            if !connected[index]
                && targets.intersects_bits(neighbor.target_bits)
                && targets.intersects_bits(topology.vertex_targets[index])
            {
                connected[index] = true;
                queue.push_back(neighbor.vertex);
            }
        }
    }
    connected
}

fn collect_editable_component(
    topology: &SculptTopology,
    start: usize,
    targets: SculptTargets,
    visited: &mut [bool],
) -> Vec<usize> {
    let mut component = Vec::new();
    let mut queue = VecDeque::from([start as u32]);
    visited[start] = true;
    while let Some(vertex) = queue.pop_front() {
        component.push(vertex as usize);
        for neighbor in &topology.adjacency[vertex as usize] {
            let index = neighbor.vertex as usize;
            if visited[index]
                || !targets.intersects_bits(neighbor.target_bits)
                || !targets.intersects_bits(topology.vertex_targets[index])
            {
                continue;
            }
            visited[index] = true;
            queue.push_back(neighbor.vertex);
        }
    }
    component.sort_unstable();
    component
}

fn component_has_editable_surface_in_brush(
    component: &[usize],
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    center: DVec3,
    settings: InfluenceSettings,
) -> bool {
    let incident = component
        .iter()
        .flat_map(|&vertex| topology.incident_triangles[vertex].iter().copied())
        .collect::<BTreeSet<_>>();
    incident.into_iter().any(|triangle_index| {
        let triangle_index = triangle_index as usize;
        if !settings
            .targets
            .intersects_bits(topology.triangle_targets[triangle_index])
        {
            return false;
        }
        let triangle = topology.triangles[triangle_index];
        let Some(normal) = triangle_normal(triangle, vertices) else {
            return false;
        };
        if settings.backface_masking
            && settings
                .incoming_direction
                .is_some_and(|incoming| normal.dot(incoming) > MAX_FRONT_FACE_INCOMING_DOT)
        {
            return false;
        }
        let [a, b, c] = triangle.map(|vertex| DVec3::from_array(vertices[vertex as usize]));
        let point = closest_point_on_triangle(center, a, b, c);
        point.is_finite() && point.distance(center) <= settings.radius
    })
}

#[derive(Clone, Copy)]
enum SmoothNeighborSource {
    Slot(u32),
    Fixed(DVec3),
}

pub fn jacobi_smooth_proposals(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    targets: SculptTargets,
    influence: &[BrushInfluence],
    reference_areas: &BTreeMap<u32, f64>,
) -> Vec<(usize, [f64; 3])> {
    debug_assert!(
        influence
            .windows(2)
            .all(|pair| pair[0].vertex < pair[1].vertex)
    );

    let touches_feature = influence
        .iter()
        .map(|entry| smooth_vertex_touches_feature(entry, vertices, topology))
        .collect::<Vec<_>>();
    let mut neighbor_offsets = Vec::with_capacity(influence.len() + 1);
    let mut neighbor_sources = Vec::new();
    let mut mean_edge_lengths = Vec::with_capacity(influence.len());
    neighbor_offsets.push(0_u32);
    for entry in influence {
        let center = DVec3::from_array(vertices[entry.vertex]);
        let surface_targets = SculptTargets(entry.surface_bits & targets.bits());
        let mut total_edge_length = 0.0;
        let mut count = 0_u32;
        for neighbor in &topology.adjacency[entry.vertex] {
            let neighbor_index = neighbor.vertex as usize;
            if !surface_targets.intersects_bits(neighbor.target_bits)
                || !surface_targets.intersects_bits(topology.vertex_targets[neighbor_index])
                || (entry.restrict_to_front_sheet
                    && !edge_tracks_seed_surface(
                        entry.vertex as u32,
                        neighbor.vertex,
                        vertices,
                        topology,
                        surface_targets,
                        entry.sheet_normal,
                        entry.incoming_direction,
                    ))
            {
                continue;
            }
            let edge_length = center.distance(DVec3::from_array(vertices[neighbor_index]));
            if !edge_length.is_finite() || edge_length <= MIN_BRUSH_RADIUS {
                continue;
            }
            neighbor_sources.push(
                match influence.binary_search_by_key(&neighbor_index, |entry| entry.vertex) {
                    Ok(slot) => SmoothNeighborSource::Slot(slot as u32),
                    Err(_) => {
                        SmoothNeighborSource::Fixed(DVec3::from_array(vertices[neighbor_index]))
                    }
                },
            );
            total_edge_length += edge_length;
            count += 1;
        }
        neighbor_offsets.push(neighbor_sources.len() as u32);
        mean_edge_lengths.push((count > 0).then(|| total_edge_length / f64::from(count)));
    }

    let mut pass_positions = influence
        .iter()
        .map(|entry| DVec3::from_array(vertices[entry.vertex]))
        .collect::<Vec<_>>();
    let mut next_positions = vec![DVec3::ZERO; influence.len()];

    let mut displacement = vec![DVec3::ZERO; influence.len()];
    let mut displacement_average = vec![DVec3::ZERO; influence.len()];
    for pass_index in 0..SMOOTH_JACOBI_PASSES {
        for slot in 0..influence.len() {
            let neighbors = &neighbor_sources
                [neighbor_offsets[slot] as usize..neighbor_offsets[slot + 1] as usize];
            let mut sum = DVec3::ZERO;
            let mut count = 0.0_f64;
            for source in neighbors {
                sum += match *source {
                    SmoothNeighborSource::Slot(other) => pass_positions[other as usize],
                    SmoothNeighborSource::Fixed(position) => position,
                };
                count += 1.0;
            }
            let target = if count > 0.0 {
                sum / count
            } else {
                pass_positions[slot]
            };
            displacement[slot] = target - pass_positions[slot];
        }

        for slot in 0..influence.len() {
            let neighbors = &neighbor_sources
                [neighbor_offsets[slot] as usize..neighbor_offsets[slot + 1] as usize];
            let mut sum = DVec3::ZERO;
            let mut count = 0.0_f64;
            for source in neighbors {
                sum += match *source {
                    SmoothNeighborSource::Slot(other) => displacement[other as usize],
                    SmoothNeighborSource::Fixed(_) => displacement[slot],
                };
                count += 1.0;
            }
            displacement_average[slot] = if count > 0.0 {
                sum / count
            } else {
                displacement[slot]
            };
        }

        for (slot, entry) in influence.iter().enumerate() {
            let neighbors_empty = neighbor_offsets[slot] == neighbor_offsets[slot + 1];
            let relaxation =
                smooth_pass_relaxation(entry.brush_strength, entry.radial_falloff, pass_index);
            if relaxation <= 0.0 || touches_feature[slot] || neighbors_empty {
                next_positions[slot] = pass_positions[slot];
                continue;
            }
            let move_vector = displacement[slot] - displacement_average[slot] * SMOOTH_ANTISHRINK;
            next_positions[slot] = pass_positions[slot] + move_vector * relaxation;
        }
        std::mem::swap(&mut pass_positions, &mut next_positions);
    }

    let mut proposals = Vec::with_capacity(influence.len());
    for (slot, (entry, &filtered)) in influence.iter().zip(&pass_positions).enumerate() {
        if touches_feature[slot] {
            continue;
        }
        let Some(mean_edge_length) = mean_edge_lengths[slot] else {
            continue;
        };
        let original = DVec3::from_array(vertices[entry.vertex]);
        let displacement = filtered - original;
        let maximum_displacement = mean_edge_length * MAX_SMOOTH_EDGE_FRACTION;
        let next = if displacement.length() > maximum_displacement {
            original + displacement.normalize_or_zero() * maximum_displacement
        } else {
            filtered
        };
        if next.is_finite() && next.distance_squared(original) > POSITION_EPSILON_SQUARED {
            proposals.push((entry.vertex, next.to_array()));
        }
    }
    backtrack_smooth_proposals(vertices, topology, proposals, reference_areas)
}

pub fn smooth_pass_relaxation(strength: f64, radial_falloff: f64, pass_index: usize) -> f64 {
    let remaining_budget = SMOOTH_JACOBI_PASSES as f64 * strength * strength - pass_index as f64;
    remaining_budget.clamp(0.0, 1.0) * radial_falloff
}

pub fn smooth_vertex_touches_feature(
    entry: &BrushInfluence,
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
) -> bool {
    if topology.boundary_vertex_targets[entry.vertex] & entry.surface_bits != 0 {
        return true;
    }
    if topology.incident_triangles[entry.vertex]
        .iter()
        .copied()
        .filter(|&triangle| topology.triangle_targets[triangle as usize] & entry.surface_bits != 0)
        .any(|triangle| {
            triangle_double_area(topology.triangles[triangle as usize], vertices)
                <= MIN_VALID_TRIANGLE_DOUBLE_AREA
        })
    {
        return true;
    }
    if !entry.restrict_to_front_sheet {
        return false;
    }
    let Some(sheet_normal) = entry.sheet_normal else {
        return false;
    };
    topology.incident_triangles[entry.vertex]
        .iter()
        .copied()
        .filter(|&triangle| topology.triangle_targets[triangle as usize] & entry.surface_bits != 0)
        .filter_map(|triangle| triangle_normal(topology.triangles[triangle as usize], vertices))
        .any(|normal| normal.dot(sheet_normal) < CREASE_PIN_NORMAL_DOT)
}

pub fn smooth_reference_area_updates(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    influence: &[BrushInfluence],
) -> Vec<(u32, f64)> {
    influence
        .iter()
        .flat_map(|entry| topology.incident_triangles[entry.vertex].iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|triangle| {
            (
                triangle,
                triangle_double_area(topology.triangles[triangle as usize], vertices),
            )
        })
        .collect()
}

pub fn smooth_neighbor_average(
    entry: &BrushInfluence,
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    targets: SculptTargets,
    first_pass: &[(usize, DVec3)],
) -> Option<(DVec3, f64)> {
    let mut sum = DVec3::ZERO;
    let mut total_weight = 0.0;
    let mut total_edge_length = 0.0;
    let mut count = 0_u32;
    let center = DVec3::from_array(vertices[entry.vertex]);
    let surface_targets = SculptTargets(entry.surface_bits & targets.bits());
    for neighbor in &topology.adjacency[entry.vertex] {
        let neighbor_index = neighbor.vertex as usize;
        if !surface_targets.intersects_bits(neighbor.target_bits)
            || !surface_targets.intersects_bits(topology.vertex_targets[neighbor_index])
            || (entry.restrict_to_front_sheet
                && !edge_tracks_seed_surface(
                    entry.vertex as u32,
                    neighbor.vertex,
                    vertices,
                    topology,
                    surface_targets,
                    entry.sheet_normal,
                    entry.incoming_direction,
                ))
        {
            continue;
        }
        let point = first_pass
            .binary_search_by_key(&neighbor_index, |&(index, _)| index)
            .ok()
            .map(|position| first_pass[position].1)
            .unwrap_or_else(|| DVec3::from_array(vertices[neighbor_index]));
        let edge_length = center.distance(DVec3::from_array(vertices[neighbor_index]));
        if !edge_length.is_finite() || edge_length <= MIN_BRUSH_RADIUS {
            continue;
        }

        sum += point;
        total_weight += 1.0;
        total_edge_length += edge_length;
        count += 1;
    }
    (count > 0 && total_weight > 0.0)
        .then(|| (sum / total_weight, total_edge_length / f64::from(count)))
}

pub fn backtrack_smooth_proposals(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    proposals: Vec<(usize, [f64; 3])>,
    reference_areas: &BTreeMap<u32, f64>,
) -> Vec<(usize, [f64; 3])> {
    if proposals.is_empty() {
        return proposals;
    }
    debug_assert!(proposals.windows(2).all(|pair| pair[0].0 < pair[1].0));
    let affected_triangles = proposals
        .iter()
        .flat_map(|(vertex, _)| topology.incident_triangles[*vertex].iter().copied())
        .collect::<BTreeSet<_>>();
    let mut scales = vec![1.0; proposals.len()];
    for _ in 0..SMOOTH_LOCAL_BACKTRACK_STEPS {
        let candidate = proposals
            .iter()
            .zip(&scales)
            .map(|(&(index, target), &scale)| {
                let original = DVec3::from_array(vertices[index]);
                let target = DVec3::from_array(target);
                (index, original.lerp(target, scale).to_array())
            })
            .collect::<Vec<_>>();
        let invalid = smooth_invalid_triangles(
            vertices,
            topology,
            &candidate,
            &affected_triangles,
            reference_areas,
        );
        if invalid.is_empty() {
            return candidate
                .into_iter()
                .filter(|(index, point)| {
                    DVec3::from_array(*point).distance_squared(DVec3::from_array(vertices[*index]))
                        > POSITION_EPSILON_SQUARED
                })
                .collect();
        }
        let mut adjusted = false;
        for triangle_index in invalid {
            for vertex in topology.triangles[triangle_index as usize] {
                if let Ok(position) =
                    proposals.binary_search_by_key(&(vertex as usize), |&(index, _)| index)
                {
                    scales[position] *= 0.5;
                    adjusted = true;
                }
            }
        }
        if !adjusted {
            break;
        }
    }

    for _ in 0..4 {
        let candidate = proposals
            .iter()
            .zip(&scales)
            .map(|(&(index, target), &scale)| {
                let original = DVec3::from_array(vertices[index]);
                (
                    index,
                    original.lerp(DVec3::from_array(target), scale).to_array(),
                )
            })
            .collect::<Vec<_>>();
        let invalid = smooth_invalid_triangles(
            vertices,
            topology,
            &candidate,
            &affected_triangles,
            reference_areas,
        );
        if invalid.is_empty() {
            return candidate
                .into_iter()
                .filter(|(index, point)| {
                    DVec3::from_array(*point).distance_squared(DVec3::from_array(vertices[*index]))
                        > POSITION_EPSILON_SQUARED
                })
                .collect();
        }
        for triangle_index in invalid {
            for vertex in topology.triangles[triangle_index as usize] {
                if let Ok(position) =
                    proposals.binary_search_by_key(&(vertex as usize), |&(index, _)| index)
                {
                    scales[position] = 0.0;
                }
            }
        }
    }
    Vec::new()
}

fn smooth_invalid_triangles(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    proposals: &[(usize, [f64; 3])],
    affected_triangles: &BTreeSet<u32>,
    reference_areas: &BTreeMap<u32, f64>,
) -> Vec<u32> {
    affected_triangles
        .iter()
        .copied()
        .filter(|&triangle_index| {
            !smooth_triangle_is_valid(
                vertices,
                topology,
                proposals,
                triangle_index,
                reference_areas,
            )
        })
        .collect()
}

fn smooth_triangle_is_valid(
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    proposals: &[(usize, [f64; 3])],
    triangle_index: u32,
    reference_areas: &BTreeMap<u32, f64>,
) -> bool {
    let triangle = topology.triangles[triangle_index as usize];
    let old = triangle.map(|index| DVec3::from_array(vertices[index as usize]));
    let new = triangle.map(|index| {
        let index = index as usize;
        let position = proposals
            .binary_search_by_key(&index, |&(candidate, _)| candidate)
            .ok()
            .map(|position| proposals[position].1)
            .unwrap_or(vertices[index]);
        DVec3::from_array(position)
    });
    let old_cross = (old[1] - old[0]).cross(old[2] - old[0]);
    let new_cross = (new[1] - new[0]).cross(new[2] - new[0]);
    let old_area = old_cross.length();
    let new_area = new_cross.length();
    let reference_area = reference_areas
        .get(&triangle_index)
        .copied()
        .unwrap_or(old_area);
    old_area > MIN_VALID_TRIANGLE_DOUBLE_AREA
        && new_area.is_finite()
        && new_cross.dot(old_cross) > 0.0
        && new_area >= old_area * MIN_SMOOTH_TRIANGLE_AREA_RATIO
        && new_area >= reference_area * MIN_SMOOTH_STROKE_AREA_RATIO
}

pub fn triangle_double_area(triangle: [u32; 3], vertices: &[[f64; 3]]) -> f64 {
    let [a, b, c] = triangle.map(|index| DVec3::from_array(vertices[index as usize]));
    (b - a).cross(c - a).length()
}

pub fn triangle_normal(triangle: [u32; 3], vertices: &[[f64; 3]]) -> Option<DVec3> {
    let [a, b, c] = triangle.map(|index| DVec3::from_array(vertices[index as usize]));
    (b - a).cross(c - a).try_normalize()
}

fn edge_tracks_seed_surface(
    from: u32,
    to: u32,
    vertices: &[[f64; 3]],
    topology: &SculptTopology,
    targets: SculptTargets,
    seed_normal: Option<DVec3>,
    incoming_direction: Option<DVec3>,
) -> bool {
    let Some(seed_normal) = seed_normal else {
        return true;
    };
    topology.incident_triangles[from as usize]
        .iter()
        .copied()
        .filter(|triangle| {
            topology.incident_triangles[to as usize]
                .binary_search(triangle)
                .is_ok()
        })
        .any(|triangle_index| {
            targets.intersects_bits(topology.triangle_targets[triangle_index as usize])
                && triangle_normal(topology.triangles[triangle_index as usize], vertices)
                    .is_some_and(|normal| {
                        normal.dot(seed_normal) >= MIN_FRONT_SURFACE_NORMAL_DOT
                            && incoming_direction.is_none_or(|incoming| {
                                normal.dot(incoming) <= MAX_FRONT_FACE_INCOMING_DOT
                            })
                    })
        })
}

pub fn closest_point_on_triangle(point: DVec3, a: DVec3, b: DVec3, c: DVec3) -> DVec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = point - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = point - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = point - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denominator = (va + vb + vc).recip();
    a + ab * (vb * denominator) + ac * (vc * denominator)
}

pub fn face_target_bits(material: Option<&str>, group: Option<&str>) -> u8 {
    material
        .into_iter()
        .chain(group)
        .map(target_bits_for_label)
        .find(|&bits| bits != 0)
        .unwrap_or(0)
}

pub fn target_bits_for_label(label: &str) -> u8 {
    let key = label
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    match key.as_str() {
        "face" | "head" | "neck" | "ears" | "ear" | "nostril" | "nostrils" => {
            SculptTarget::HeadSkin as u8
        }
        "lips" | "lip" => SculptTarget::Lips as u8,
        "tear" | "tears" | "lacrimal" | "lacrimals" => SculptTarget::Tear as u8,
        "eyelash" | "eyelashes" | "lashes" => SculptTarget::Eyelashes as u8,
        "teeth" | "tooth" | "gums" | "gum" | "tongue" | "upperjaw" | "lowerjaw" => {
            SculptTarget::TeethTongue as u8
        }
        "innermouth" => SculptTarget::InnerMouth as u8,
        "sclera" | "scleras" | "iris" | "irises" | "pupil" | "pupils" | "cornea"
        | "eyereflection" | "leye" | "reye" | "eyes" | "eye" => SculptTarget::Eyes as u8,
        _ => 0,
    }
}

pub fn vertex_normal_at(vertex: usize, vertices: &[[f64; 3]], topology: &SculptTopology) -> DVec3 {
    let mut normal_sum = DVec3::ZERO;
    for &triangle_index in &topology.incident_triangles[vertex] {
        let triangle = topology.triangles[triangle_index as usize];
        let [a, b, c] = triangle.map(|index| DVec3::from_array(vertices[index as usize]));
        let normal = (b - a).cross(c - a);
        if !normal.is_finite() || normal.length_squared() <= 1.0e-24 {
            continue;
        }
        normal_sum += normal;
    }
    normal_sum.try_normalize().unwrap_or(DVec3::ZERO)
}

pub fn ray_triangle(
    origin: DVec3,
    direction: DVec3,
    a: DVec3,
    b: DVec3,
    c: DVec3,
    cull_backfaces: bool,
) -> Option<(f64, [f64; 3])> {
    let edge_ab = b - a;
    let edge_ac = c - a;
    let cross = direction.cross(edge_ac);
    let determinant = edge_ab.dot(cross);

    if (cull_backfaces && determinant <= 1.0e-12)
        || (!cull_backfaces && determinant.abs() <= 1.0e-12)
    {
        return None;
    }
    let inverse = determinant.recip();
    let from_a = origin - a;
    let u = from_a.dot(cross) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = from_a.cross(edge_ab);
    let v = direction.dot(q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = edge_ac.dot(q) * inverse;
    (distance > 1.0e-9).then_some((distance, [1.0 - u - v, u, v]))
}

#[cfg(test)]
mod default_target_tests {
    use super::*;

    #[test]
    fn the_face_someone_reshapes_is_the_skin_and_what_rides_on_it() {
        let default = SculptTargets::default();
        for wanted in [
            SculptTarget::HeadSkin,
            SculptTarget::Tear,
            SculptTarget::Eyelashes,
            SculptTarget::Lips,
        ] {
            assert!(
                default.contains(wanted),
                "{wanted:?} is part of the face surface and must start editable"
            );
        }
        for behind in [
            SculptTarget::Eyes,
            SculptTarget::TeethTongue,
            SculptTarget::InnerMouth,
        ] {
            assert!(
                !default.contains(behind),
                "{behind:?} sits behind the surface and must be asked for"
            );
        }
    }

    #[test]
    fn intersection_keeps_only_what_both_sides_hold() {
        let both = SculptTargets::FACE_SURFACE.intersection(SculptTargets::ALL);
        assert_eq!(both, SculptTargets::FACE_SURFACE);
        assert!(
            SculptTargets::HEAD_SKIN
                .intersection(SculptTargets::EYELASHES)
                .is_empty()
        );
    }
}
