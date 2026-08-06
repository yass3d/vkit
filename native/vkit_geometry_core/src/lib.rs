use std::cmp::Ordering;

const STATUS_INVALID_ARGUMENT: i32 = 2;
const STATUS_INDEX_OUT_OF_RANGE: i32 = 3;

#[derive(Clone, Copy, Debug)]
struct Vec3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vec3 {
    fn add(self, other: Self) -> Self {
        Self {
            x: self.x + other.x,
            y: self.y + other.y,
            z: self.z + other.z,
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }

    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    fn scale(self, scalar: f64) -> Self {
        Self {
            x: self.x * scalar,
            y: self.y * scalar,
            z: self.z * scalar,
        }
    }

    fn length_squared(self) -> f64 {
        self.dot(self)
    }

    fn component(self, axis: usize) -> f64 {
        match axis {
            0 => self.x,
            1 => self.y,
            _ => self.z,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Aabb {
    minimum: Vec3,
    maximum: Vec3,
}

impl Aabb {
    fn from_point(point: Vec3) -> Self {
        Self {
            minimum: point,
            maximum: point,
        }
    }

    fn include_point(mut self, point: Vec3) -> Self {
        self.minimum.x = self.minimum.x.min(point.x);
        self.minimum.y = self.minimum.y.min(point.y);
        self.minimum.z = self.minimum.z.min(point.z);
        self.maximum.x = self.maximum.x.max(point.x);
        self.maximum.y = self.maximum.y.max(point.y);
        self.maximum.z = self.maximum.z.max(point.z);
        self
    }

    fn include_box(self, other: Self) -> Self {
        self.include_point(other.minimum)
            .include_point(other.maximum)
    }

    fn extent(self, axis: usize) -> f64 {
        self.maximum.component(axis) - self.minimum.component(axis)
    }

    fn distance_squared(self, point: Vec3) -> f64 {
        let axis_distance = |value: f64, minimum: f64, maximum: f64| {
            if value < minimum {
                minimum - value
            } else if value > maximum {
                value - maximum
            } else {
                0.0
            }
        };
        let x = axis_distance(point.x, self.minimum.x, self.maximum.x);
        let y = axis_distance(point.y, self.minimum.y, self.maximum.y);
        let z = axis_distance(point.z, self.minimum.z, self.maximum.z);
        x * x + y * y + z * z
    }
}

#[derive(Clone, Copy)]
struct Triangle {
    points: [Vec3; 3],
    ids: [i32; 3],
    minimum: Vec3,
    maximum: Vec3,
}

impl Triangle {
    fn new(points: [Vec3; 3], ids: [i32; 3]) -> Self {
        let minimum = Vec3 {
            x: points[0].x.min(points[1].x).min(points[2].x),
            y: points[0].y.min(points[1].y).min(points[2].y),
            z: points[0].z.min(points[1].z).min(points[2].z),
        };
        let maximum = Vec3 {
            x: points[0].x.max(points[1].x).max(points[2].x),
            y: points[0].y.max(points[1].y).max(points[2].y),
            z: points[0].z.max(points[1].z).max(points[2].z),
        };
        Self {
            points,
            ids,
            minimum,
            maximum,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SurfaceTriangle {
    points: [Vec3; 3],
    primitive_id: i32,
    bounds: Aabb,
    centroid: Vec3,
    normal: Vec3,
}

impl SurfaceTriangle {
    fn new(points: [Vec3; 3], primitive_id: i32) -> Option<Self> {
        let first_edge = points[1].sub(points[0]);
        let second_edge = points[2].sub(points[0]);
        let raw_normal = first_edge.cross(second_edge);
        let normal_length_squared = raw_normal.length_squared();
        if !normal_length_squared.is_finite() || normal_length_squared <= 0.0 {
            return None;
        }
        let bounds = Aabb::from_point(points[0])
            .include_point(points[1])
            .include_point(points[2]);
        let centroid = points[0].add(points[1]).add(points[2]).scale(1.0 / 3.0);
        Some(Self {
            points,
            primitive_id,
            bounds,
            centroid,
            normal: raw_normal.scale(1.0 / normal_length_squared.sqrt()),
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum BvhNodeKind {
    Leaf { start: usize, end: usize },
    Branch { left: usize, right: usize },
}

#[derive(Clone, Copy, Debug)]
struct BvhNode {
    bounds: Aabb,
    kind: BvhNodeKind,
}

const INLINE_BVH_STACK_CAPACITY: usize = 64;

struct BvhTraversalStack {
    inline: [usize; INLINE_BVH_STACK_CAPACITY],
    inline_len: usize,
    overflow: Vec<usize>,
}

impl BvhTraversalStack {
    fn new() -> Self {
        Self {
            inline: [0; INLINE_BVH_STACK_CAPACITY],
            inline_len: 0,
            overflow: Vec::new(),
        }
    }

    fn clear(&mut self) {
        self.inline_len = 0;
        self.overflow.clear();
    }

    fn push(&mut self, node: usize) {
        if self.overflow.is_empty() && self.inline_len < INLINE_BVH_STACK_CAPACITY {
            self.inline[self.inline_len] = node;
            self.inline_len += 1;
        } else {
            self.overflow.push(node);
        }
    }

    fn pop(&mut self) -> Option<usize> {
        if let Some(node) = self.overflow.pop() {
            return Some(node);
        }
        if self.inline_len == 0 {
            return None;
        }
        self.inline_len -= 1;
        Some(self.inline[self.inline_len])
    }
}

#[derive(Clone, Copy, Debug)]
struct SurfaceHit {
    point: Vec3,
    distance_squared: f64,
    primitive_id: i32,
    barycentric: [f64; 3],
    normal: Vec3,
}

pub struct FmSurfaceProjector {
    triangles: Vec<SurfaceTriangle>,
    order: Vec<usize>,
    nodes: Vec<BvhNode>,
    root: usize,
}

impl FmSurfaceProjector {
    fn new(
        vertices: &[f64],
        triangle_indices: &[i32],
        candidate_ids: Option<&[i32]>,
        minimum_doubled_area: f64,
    ) -> Result<Self, i32> {
        if !vertices.len().is_multiple_of(3)
            || !triangle_indices.len().is_multiple_of(3)
            || !minimum_doubled_area.is_finite()
            || minimum_doubled_area < 0.0
            || vertices.iter().any(|value| !value.is_finite())
        {
            return Err(STATUS_INVALID_ARGUMENT);
        }
        let vertex_count = vertices.len() / 3;
        let triangle_count = triangle_indices.len() / 3;
        if vertex_count == 0 || triangle_count == 0 {
            return Err(STATUS_INVALID_ARGUMENT);
        }

        let mut selected_ids = match candidate_ids {
            Some(values) => {
                if values.is_empty() {
                    return Err(STATUS_INVALID_ARGUMENT);
                }
                let mut ids = values.to_vec();
                if ids
                    .iter()
                    .any(|id| *id < 0 || *id as usize >= triangle_count)
                {
                    return Err(STATUS_INDEX_OUT_OF_RANGE);
                }
                ids.sort_unstable();
                ids.dedup();
                ids
            }
            None => (0..triangle_count)
                .map(|index| i32::try_from(index).map_err(|_| STATUS_INVALID_ARGUMENT))
                .collect::<Result<Vec<_>, _>>()?,
        };

        let point = |index: i32| -> Result<Vec3, i32> {
            if index < 0 || index as usize >= vertex_count {
                return Err(STATUS_INDEX_OUT_OF_RANGE);
            }
            let offset = index as usize * 3;
            Ok(Vec3 {
                x: vertices[offset],
                y: vertices[offset + 1],
                z: vertices[offset + 2],
            })
        };

        let mut triangles = Vec::with_capacity(selected_ids.len());
        for primitive_id in selected_ids.drain(..) {
            let raw = &triangle_indices[primitive_id as usize * 3..primitive_id as usize * 3 + 3];
            let points = [point(raw[0])?, point(raw[1])?, point(raw[2])?];
            let doubled_area = points[1]
                .sub(points[0])
                .cross(points[2].sub(points[0]))
                .length_squared()
                .sqrt();
            if doubled_area <= minimum_doubled_area {
                continue;
            }
            if let Some(triangle) = SurfaceTriangle::new(points, primitive_id) {
                triangles.push(triangle);
            }
        }
        if triangles.is_empty() {
            return Err(STATUS_INVALID_ARGUMENT);
        }

        let mut order: Vec<usize> = (0..triangles.len()).collect();
        let mut nodes = Vec::with_capacity(triangles.len().saturating_mul(2));
        let order_len = order.len();
        let root = build_bvh_node(&triangles, &mut order, 0, order_len, &mut nodes);
        Ok(Self {
            triangles,
            order,
            nodes,
            root,
        })
    }

    fn project(&self, query: Vec3) -> SurfaceHit {
        let mut stack = BvhTraversalStack::new();
        self.project_with_stack(query, &mut stack)
    }

    fn project_with_stack(&self, query: Vec3, stack: &mut BvhTraversalStack) -> SurfaceHit {
        let mut best: Option<SurfaceHit> = None;
        stack.clear();
        stack.push(self.root);

        while let Some(node_id) = stack.pop() {
            let node = self.nodes[node_id];
            let best_distance = best
                .as_ref()
                .map_or(f64::INFINITY, |hit| hit.distance_squared);
            if node.bounds.distance_squared(query) > best_distance {
                continue;
            }
            match node.kind {
                BvhNodeKind::Leaf { start, end } => {
                    for ordered_id in &self.order[start..end] {
                        let triangle = self.triangles[*ordered_id];
                        let (point, barycentric) =
                            closest_point_on_triangle(query, triangle.points);
                        let distance_squared = point.sub(query).length_squared();
                        let replace = match best {
                            None => true,
                            Some(current) => {
                                distance_squared < current.distance_squared
                                    || (distance_squared == current.distance_squared
                                        && triangle.primitive_id < current.primitive_id)
                            }
                        };
                        if replace {
                            best = Some(SurfaceHit {
                                point,
                                distance_squared,
                                primitive_id: triangle.primitive_id,
                                barycentric,
                                normal: triangle.normal,
                            });
                        }
                    }
                }
                BvhNodeKind::Branch { left, right } => {
                    let left_distance = self.nodes[left].bounds.distance_squared(query);
                    let right_distance = self.nodes[right].bounds.distance_squared(query);

                    let (near, far) = if left_distance < right_distance
                        || (left_distance == right_distance && left < right)
                    {
                        (left, right)
                    } else {
                        (right, left)
                    };
                    stack.push(far);
                    stack.push(near);
                }
            }
        }

        best.expect("non-empty BVH must produce a surface hit")
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceProjection {
    pub point: [f64; 3],
    pub distance_squared: f64,
    pub primitive_id: u32,
    pub barycentric: [f64; 3],
    pub normal: [f64; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceProjectorError {
    InvalidArgument,
    IndexOutOfRange,
}

impl std::fmt::Display for SurfaceProjectorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArgument => formatter.write_str("invalid surface projector argument"),
            Self::IndexOutOfRange => formatter.write_str("surface projector index is out of range"),
        }
    }
}

impl std::error::Error for SurfaceProjectorError {}

pub struct SurfaceProjector {
    inner: FmSurfaceProjector,
}

impl SurfaceProjector {
    pub fn new(
        vertices: &[[f64; 3]],
        triangles: &[[u32; 3]],
        candidate_ids: Option<&[u32]>,
        minimum_doubled_area: f64,
    ) -> Result<Self, SurfaceProjectorError> {
        let mut flat_vertices = Vec::with_capacity(vertices.len().saturating_mul(3));
        for point in vertices {
            flat_vertices.extend_from_slice(point);
        }
        let mut flat_triangles = Vec::with_capacity(triangles.len().saturating_mul(3));
        for triangle in triangles {
            for &index in triangle {
                flat_triangles.push(
                    i32::try_from(index).map_err(|_| SurfaceProjectorError::IndexOutOfRange)?,
                );
            }
        }
        let converted_candidates = candidate_ids
            .map(|ids| {
                ids.iter()
                    .copied()
                    .map(|index| {
                        i32::try_from(index).map_err(|_| SurfaceProjectorError::IndexOutOfRange)
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        let inner = FmSurfaceProjector::new(
            &flat_vertices,
            &flat_triangles,
            converted_candidates.as_deref(),
            minimum_doubled_area,
        )
        .map_err(|status| match status {
            STATUS_INDEX_OUT_OF_RANGE => SurfaceProjectorError::IndexOutOfRange,
            _ => SurfaceProjectorError::InvalidArgument,
        })?;
        Ok(Self { inner })
    }

    pub fn project(&self, query: [f64; 3]) -> Result<SurfaceProjection, SurfaceProjectorError> {
        if query.iter().any(|value| !value.is_finite()) {
            return Err(SurfaceProjectorError::InvalidArgument);
        }
        let hit = self.inner.project(Vec3 {
            x: query[0],
            y: query[1],
            z: query[2],
        });
        Ok(SurfaceProjection {
            point: [hit.point.x, hit.point.y, hit.point.z],
            distance_squared: hit.distance_squared,
            primitive_id: u32::try_from(hit.primitive_id)
                .map_err(|_| SurfaceProjectorError::IndexOutOfRange)?,
            barycentric: hit.barycentric,
            normal: [hit.normal.x, hit.normal.y, hit.normal.z],
        })
    }
}

fn build_bvh_node(
    triangles: &[SurfaceTriangle],
    order: &mut [usize],
    absolute_start: usize,
    absolute_end: usize,
    nodes: &mut Vec<BvhNode>,
) -> usize {
    debug_assert!(!order.is_empty());
    let bounds = order[1..]
        .iter()
        .fold(triangles[order[0]].bounds, |current, triangle_id| {
            current.include_box(triangles[*triangle_id].bounds)
        });
    if order.len() <= 8 {
        let node_id = nodes.len();
        nodes.push(BvhNode {
            bounds,
            kind: BvhNodeKind::Leaf {
                start: absolute_start,
                end: absolute_end,
            },
        });
        return node_id;
    }

    let centroid_bounds = order[1..].iter().fold(
        Aabb::from_point(triangles[order[0]].centroid),
        |current, triangle_id| current.include_point(triangles[*triangle_id].centroid),
    );
    let axis = (1..3).fold(0, |best_axis, candidate_axis| {
        if centroid_bounds.extent(candidate_axis) > centroid_bounds.extent(best_axis) {
            candidate_axis
        } else {
            best_axis
        }
    });

    let midpoint = order.len() / 2;
    order.select_nth_unstable_by(midpoint, |left, right| {
        triangles[*left]
            .centroid
            .component(axis)
            .partial_cmp(&triangles[*right].centroid.component(axis))
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                triangles[*left]
                    .primitive_id
                    .cmp(&triangles[*right].primitive_id)
            })
    });
    let absolute_midpoint = absolute_start + midpoint;
    let (left_order, right_order) = order.split_at_mut(midpoint);
    let left = build_bvh_node(
        triangles,
        left_order,
        absolute_start,
        absolute_midpoint,
        nodes,
    );
    let right = build_bvh_node(
        triangles,
        right_order,
        absolute_midpoint,
        absolute_end,
        nodes,
    );
    let node_id = nodes.len();
    nodes.push(BvhNode {
        bounds,
        kind: BvhNodeKind::Branch { left, right },
    });
    node_id
}

fn closest_point_on_triangle(point: Vec3, triangle: [Vec3; 3]) -> (Vec3, [f64; 3]) {
    const REGION_TOLERANCE: f64 = 1e-13;
    let a = triangle[0];
    let b = triangle[1];
    let c = triangle[2];
    let ab = b.sub(a);
    let ac = c.sub(a);
    let ap = point.sub(a);
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 < REGION_TOLERANCE && d2 < REGION_TOLERANCE {
        return (a, [1.0, 0.0, 0.0]);
    }

    let bp = point.sub(b);
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 > -REGION_TOLERANCE && d4 <= d3 {
        return (b, [0.0, 1.0, 0.0]);
    }

    let vc = d1 * d4 - d3 * d2;
    if vc < REGION_TOLERANCE && d1 > -REGION_TOLERANCE && d3 < REGION_TOLERANCE {
        let v = d1 / (d1 - d3);
        return (a.add(ab.scale(v)), [1.0 - v, v, 0.0]);
    }

    let cp = point.sub(c);
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 > -REGION_TOLERANCE && d5 <= d6 {
        return (c, [0.0, 0.0, 1.0]);
    }

    let vb = d5 * d2 - d1 * d6;
    if vb < REGION_TOLERANCE && d2 > -REGION_TOLERANCE && d6 < REGION_TOLERANCE {
        let w = d2 / (d2 - d6);
        return (a.add(ac.scale(w)), [1.0 - w, 0.0, w]);
    }

    let va = d3 * d6 - d5 * d4;
    if va < REGION_TOLERANCE && d4 - d3 > -REGION_TOLERANCE && d5 - d6 > -REGION_TOLERANCE {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return (b.add(c.sub(b).scale(w)), [0.0, 1.0 - w, w]);
    }

    let denominator = 1.0 / (va + vb + vc);
    let v = vb * denominator;
    let w = vc * denominator;
    let barycentric = [1.0 - v - w, v, w];
    (a.add(ab.scale(v)).add(ac.scale(w)), barycentric)
}

fn segment_intersects_triangle(
    start: Vec3,
    end: Vec3,
    triangle: &[Vec3; 3],
    tolerance: f64,
) -> bool {
    let direction = end.sub(start);
    let first_edge = triangle[1].sub(triangle[0]);
    let second_edge = triangle[2].sub(triangle[0]);
    let cross = direction.cross(second_edge);
    let determinant = first_edge.dot(cross);
    if determinant.abs() <= tolerance {
        return false;
    }
    let offset = start.sub(triangle[0]);
    let u = offset.dot(cross) / determinant;
    if u < -tolerance || u > 1.0 + tolerance {
        return false;
    }
    let barycentric_cross = offset.cross(first_edge);
    let v = direction.dot(barycentric_cross) / determinant;
    if v < -tolerance || u + v > 1.0 + tolerance {
        return false;
    }
    let distance = second_edge.dot(barycentric_cross) / determinant;
    distance >= -tolerance && distance <= 1.0 + tolerance
}

fn triangles_intersect(first: &Triangle, second: &Triangle, tolerance: f64) -> bool {
    (0..3).any(|index| {
        segment_intersects_triangle(
            first.points[index],
            first.points[(index + 1) % 3],
            &second.points,
            tolerance,
        )
    }) || (0..3).any(|index| {
        segment_intersects_triangle(
            second.points[index],
            second.points[(index + 1) % 3],
            &first.points,
            tolerance,
        )
    })
}

fn shares_vertex(first: &[i32; 3], second: &[i32; 3]) -> bool {
    first
        .iter()
        .any(|left| second.iter().any(|right| left == right))
}

fn build_triangles(vertices: &[f64], indices: &[i32]) -> Result<Vec<Triangle>, i32> {
    let vertex_count = vertices.len() / 3;
    let mut triangles = Vec::with_capacity(indices.len() / 3);
    for raw in indices.chunks_exact(3) {
        let ids = [raw[0], raw[1], raw[2]];
        if ids
            .iter()
            .any(|index| *index < 0 || *index as usize >= vertex_count)
        {
            return Err(STATUS_INDEX_OUT_OF_RANGE);
        }
        let point = |index: i32| {
            let offset = index as usize * 3;
            Vec3 {
                x: vertices[offset],
                y: vertices[offset + 1],
                z: vertices[offset + 2],
            }
        };
        triangles.push(Triangle::new(
            [point(ids[0]), point(ids[1]), point(ids[2])],
            ids,
        ));
    }
    Ok(triangles)
}

fn intersection_count(
    vertices: &[f64],
    first_indices: &[i32],
    second_indices: &[i32],
    tolerance: f64,
) -> Result<u64, i32> {
    if !vertices.len().is_multiple_of(3)
        || !first_indices.len().is_multiple_of(3)
        || !second_indices.len().is_multiple_of(3)
        || !tolerance.is_finite()
        || tolerance < 0.0
        || vertices.iter().any(|value| !value.is_finite())
    {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    let first = build_triangles(vertices, first_indices)?;
    let second = build_triangles(vertices, second_indices)?;
    if first.is_empty() || second.is_empty() {
        return Ok(0);
    }

    let mut order: Vec<usize> = (0..second.len()).collect();
    order.sort_unstable_by(|left, right| {
        second[*left]
            .minimum
            .x
            .partial_cmp(&second[*right].minimum.x)
            .unwrap_or(Ordering::Equal)
    });
    let sorted_minimum_x: Vec<f64> = order.iter().map(|index| second[*index].minimum.x).collect();

    let mut count = 0_u64;
    for candidate in &first {
        let upper = candidate.maximum.x + tolerance;
        let stop = sorted_minimum_x.partition_point(|value| *value <= upper);
        for second_index in order[..stop].iter().copied() {
            let other = &second[second_index];
            let overlaps = other.maximum.x >= candidate.minimum.x - tolerance
                && other.minimum.y <= candidate.maximum.y + tolerance
                && other.maximum.y >= candidate.minimum.y - tolerance
                && other.minimum.z <= candidate.maximum.z + tolerance
                && other.maximum.z >= candidate.minimum.z - tolerance;
            if !overlaps || shares_vertex(&candidate.ids, &other.ids) {
                continue;
            }
            if triangles_intersect(candidate, other, tolerance) {
                count += 1;
            }
        }
    }
    Ok(count)
}

fn minimum_triangle_angle(points: [Vec3; 3]) -> f64 {
    let edge_a = points[1].sub(points[0]).length_squared().sqrt();
    let edge_b = points[2].sub(points[1]).length_squared().sqrt();
    let edge_c = points[0].sub(points[2]).length_squared().sqrt();
    let angle = |numerator: f64, denominator: f64| {
        (numerator / denominator.max(1e-20)).clamp(-1.0, 1.0).acos()
    };
    angle(
        edge_a * edge_a + edge_c * edge_c - edge_b * edge_b,
        2.0 * edge_a * edge_c,
    )
    .min(angle(
        edge_a * edge_a + edge_b * edge_b - edge_c * edge_c,
        2.0 * edge_a * edge_b,
    ))
    .min(angle(
        edge_b * edge_b + edge_c * edge_c - edge_a * edge_a,
        2.0 * edge_b * edge_c,
    ))
}

fn unsafe_triangle_mask_values(
    base_vertices: &[f64],
    candidate_vertices: &[f64],
    triangle_indices: &[i32],
    minimum_orientation_cosine: f64,
    minimum_area_ratio: f64,
    maximum_area_ratio: f64,
) -> Result<Vec<u8>, i32> {
    if !base_vertices.len().is_multiple_of(3)
        || candidate_vertices.len() != base_vertices.len()
        || !triangle_indices.len().is_multiple_of(3)
        || !minimum_orientation_cosine.is_finite()
        || !minimum_area_ratio.is_finite()
        || !maximum_area_ratio.is_finite()
        || minimum_area_ratio < 0.0
        || maximum_area_ratio < minimum_area_ratio
        || base_vertices.iter().any(|value| !value.is_finite())
        || candidate_vertices.iter().any(|value| !value.is_finite())
    {
        return Err(STATUS_INVALID_ARGUMENT);
    }
    let vertex_count = base_vertices.len() / 3;
    let read_point = |vertices: &[f64], index: i32| -> Result<Vec3, i32> {
        if index < 0 || index as usize >= vertex_count {
            return Err(STATUS_INDEX_OUT_OF_RANGE);
        }
        let offset = index as usize * 3;
        Ok(Vec3 {
            x: vertices[offset],
            y: vertices[offset + 1],
            z: vertices[offset + 2],
        })
    };

    let mut mask = Vec::with_capacity(triangle_indices.len() / 3);
    for raw in triangle_indices.chunks_exact(3) {
        let base = [
            read_point(base_vertices, raw[0])?,
            read_point(base_vertices, raw[1])?,
            read_point(base_vertices, raw[2])?,
        ];
        let candidate = [
            read_point(candidate_vertices, raw[0])?,
            read_point(candidate_vertices, raw[1])?,
            read_point(candidate_vertices, raw[2])?,
        ];
        let base_normal = base[1].sub(base[0]).cross(base[2].sub(base[0]));
        let candidate_normal = candidate[1]
            .sub(candidate[0])
            .cross(candidate[2].sub(candidate[0]));
        let base_area2 = base_normal.length_squared().sqrt();
        let candidate_area2 = candidate_normal.length_squared().sqrt();
        let signed = base_normal.dot(candidate_normal);
        let orientation_cosine = signed / (base_area2 * candidate_area2).max(1e-20);
        let area_ratio = candidate_area2 / base_area2.max(1e-20);
        let allowed_minimum_angle = (4.0_f64.to_radians()).min(minimum_triangle_angle(base) * 0.65);
        let candidate_minimum_angle = minimum_triangle_angle(candidate);
        let unsafe_triangle = orientation_cosine < minimum_orientation_cosine
            || area_ratio < minimum_area_ratio
            || area_ratio > maximum_area_ratio
            || candidate_minimum_angle < allowed_minimum_angle
            || candidate_area2 <= 1e-10;
        mask.push(u8::from(unsafe_triangle));
    }
    Ok(mask)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryKernelError {
    InvalidArgument,
    IndexOutOfRange,
}

impl std::fmt::Display for GeometryKernelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArgument => formatter.write_str("invalid geometry kernel argument"),
            Self::IndexOutOfRange => formatter.write_str("geometry kernel index is out of range"),
        }
    }
}

impl std::error::Error for GeometryKernelError {}

fn safe_kernel_status(status: i32) -> GeometryKernelError {
    if status == STATUS_INDEX_OUT_OF_RANGE {
        GeometryKernelError::IndexOutOfRange
    } else {
        GeometryKernelError::InvalidArgument
    }
}

fn flatten_vertices(vertices: &[[f64; 3]]) -> Vec<f64> {
    vertices
        .iter()
        .flat_map(|point| point.iter().copied())
        .collect()
}

fn flatten_triangles(triangles: &[[u32; 3]]) -> Result<Vec<i32>, GeometryKernelError> {
    triangles
        .iter()
        .flat_map(|triangle| triangle.iter().copied())
        .map(|index| i32::try_from(index).map_err(|_| GeometryKernelError::IndexOutOfRange))
        .collect()
}

pub fn deformation_safety_mask(
    canonical: &[[f64; 3]],
    candidate: &[[f64; 3]],
    triangles: &[[u32; 3]],
    minimum_orientation_cosine: f64,
    minimum_area_ratio: f64,
    maximum_area_ratio: f64,
) -> Result<Vec<bool>, GeometryKernelError> {
    let canonical = flatten_vertices(canonical);
    let candidate = flatten_vertices(candidate);
    let triangles = flatten_triangles(triangles)?;
    unsafe_triangle_mask_values(
        &canonical,
        &candidate,
        &triangles,
        minimum_orientation_cosine,
        minimum_area_ratio,
        maximum_area_ratio,
    )
    .map(|values| values.into_iter().map(|value| value != 0).collect())
    .map_err(safe_kernel_status)
}

pub fn triangle_intersection_count(
    vertices: &[[f64; 3]],
    first: &[[u32; 3]],
    second: &[[u32; 3]],
    tolerance: f64,
) -> Result<u64, GeometryKernelError> {
    let vertices = flatten_vertices(vertices);
    let first = flatten_triangles(first)?;
    let second = flatten_triangles(second)?;
    intersection_count(&vertices, &first, &second, tolerance).map_err(safe_kernel_status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flattened(points: &[[f64; 3]]) -> Vec<f64> {
        points.iter().flatten().copied().collect()
    }

    #[test]
    fn crossing_and_separated_triangles_match_contract() {
        let crossing = flattened(&[
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, 1.0],
            [0.0, 2.0, 0.0],
        ]);
        assert_eq!(
            intersection_count(&crossing, &[0, 1, 2], &[3, 4, 5], 1e-9),
            Ok(1)
        );
        let separated = flattened(&[
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [0.0, 1.0, 0.0],
            [3.0, 0.0, -1.0],
            [3.0, 0.0, 1.0],
            [3.0, 2.0, 0.0],
        ]);
        assert_eq!(
            intersection_count(&separated, &[0, 1, 2], &[3, 4, 5], 1e-9),
            Ok(0)
        );
    }

    #[test]
    fn canonical_shared_vertices_are_not_counted() {
        let points = flattened(&[
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ]);
        assert_eq!(
            intersection_count(&points, &[0, 1, 2], &[0, 1, 3], 1e-9),
            Ok(0)
        );
    }

    #[test]
    fn invalid_indices_fail_closed() {
        let points = flattened(&[[0.0, 0.0, 0.0]]);
        assert_eq!(
            intersection_count(&points, &[0, 1, 0], &[0, 0, 0], 1e-9),
            Err(STATUS_INDEX_OUT_OF_RANGE)
        );
    }

    #[test]
    fn unsafe_triangle_mask_covers_safe_flipped_and_collapsed_cases() {
        let base = flattened(&[
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
            [4.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            [4.0, 1.0, 0.0],
        ]);
        let candidate = flattened(&[
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
            [3.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [4.0 + 1e-12, 0.0, 0.0],
            [4.0, 1e-12, 0.0],
        ]);
        assert_eq!(
            unsafe_triangle_mask_values(
                &base,
                &candidate,
                &[0, 1, 2, 3, 4, 5, 6, 7, 8],
                0.03,
                0.08,
                4.0,
            ),
            Ok(vec![0, 1, 1])
        );
    }

    #[test]
    fn surface_projection_returns_full_barycentric_and_unit_normal() {
        let vertices = flattened(&[
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 2.0],
            [1.0, 0.0, 2.0],
            [0.0, 1.0, 2.0],
        ]);
        let triangles = [0, 1, 2, 3, 4, 5];
        let projector = FmSurfaceProjector::new(&vertices, &triangles, None, 0.0).unwrap();
        let hit = projector.project(Vec3 {
            x: 0.25,
            y: 0.25,
            z: 0.75,
        });
        assert_eq!(hit.primitive_id, 0);
        assert!((hit.point.x - 0.25).abs() < 1e-15);
        assert!((hit.point.y - 0.25).abs() < 1e-15);
        assert!(hit.point.z.abs() < 1e-15);
        assert_eq!(hit.barycentric, [0.5, 0.25, 0.25]);
        assert!((hit.distance_squared - 0.75 * 0.75).abs() < 1e-15);
        assert!((hit.normal.length_squared() - 1.0).abs() < 1e-15);
        assert!(hit.normal.z > 0.0);
    }

    #[test]
    fn surface_projection_ties_use_lowest_original_primitive_id() {
        let vertices = flattened(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        let triangles = [0, 1, 2, 0, 1, 2];
        let projector =
            FmSurfaceProjector::new(&vertices, &triangles, Some(&[1, 0, 1]), 0.0).unwrap();
        let hit = projector.project(Vec3 {
            x: 0.2,
            y: 0.2,
            z: 1.0,
        });
        assert_eq!(hit.primitive_id, 0);
    }

    #[test]
    fn bvh_traversal_stack_avoids_heap_storage_within_inline_bound() {
        let mut stack = BvhTraversalStack::new();
        for node in 0..INLINE_BVH_STACK_CAPACITY {
            stack.push(node);
        }
        assert_eq!(stack.overflow.capacity(), 0);
        for expected in (0..INLINE_BVH_STACK_CAPACITY).rev() {
            assert_eq!(stack.pop(), Some(expected));
        }
        assert_eq!(stack.pop(), None);
    }

    #[test]
    fn bvh_traversal_stack_overflow_preserves_exact_lifo_order() {
        let mut stack = BvhTraversalStack::new();
        let mut reference = Vec::new();
        let total = INLINE_BVH_STACK_CAPACITY + 17;
        for node in 0..total {
            stack.push(node);
            reference.push(node);
        }
        assert!(stack.overflow.capacity() >= 17);
        for _ in 0..20 {
            assert_eq!(stack.pop(), reference.pop());
        }
        for node in total..total + 29 {
            stack.push(node);
            reference.push(node);
        }
        while !reference.is_empty() {
            assert_eq!(stack.pop(), reference.pop());
        }
        assert_eq!(stack.pop(), None);
    }

    #[test]
    fn representative_bvh_query_keeps_overflow_unallocated() {
        let vertices = flattened(&[
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 2.0],
            [1.0, 0.0, 2.0],
            [0.0, 1.0, 2.0],
        ]);
        let triangles = [0, 1, 2, 3, 4, 5];
        let projector = FmSurfaceProjector::new(&vertices, &triangles, None, 0.0).unwrap();
        let mut stack = BvhTraversalStack::new();
        let hit = projector.project_with_stack(
            Vec3 {
                x: 0.25,
                y: 0.25,
                z: 0.75,
            },
            &mut stack,
        );
        assert_eq!(hit.primitive_id, 0);
        assert_eq!(stack.overflow.capacity(), 0);
    }

    #[test]
    fn bvh_projection_matches_exhaustive_search_on_triangle_soup() {
        fn random_signed(state: &mut u64) -> f64 {
            *state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let unit = ((*state >> 11) as f64) / ((1_u64 << 53) as f64);
            unit * 2.0 - 1.0
        }

        let mut state = 0xface_2026_0716_u64;
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        for triangle_id in 0..160_i32 {
            let origin = Vec3 {
                x: random_signed(&mut state) * 5.0,
                y: random_signed(&mut state) * 5.0,
                z: random_signed(&mut state) * 5.0,
            };
            let first_edge = Vec3 {
                x: random_signed(&mut state),
                y: random_signed(&mut state),
                z: random_signed(&mut state),
            };
            let mut second_edge = Vec3 {
                x: random_signed(&mut state),
                y: random_signed(&mut state),
                z: random_signed(&mut state),
            };
            if first_edge.cross(second_edge).length_squared() < 1e-8 {
                second_edge.z += 0.5;
            }
            for point in [origin, origin.add(first_edge), origin.add(second_edge)] {
                vertices.extend_from_slice(&[point.x, point.y, point.z]);
            }
            triangles.extend_from_slice(&[
                triangle_id * 3,
                triangle_id * 3 + 1,
                triangle_id * 3 + 2,
            ]);
        }
        let projector = FmSurfaceProjector::new(&vertices, &triangles, None, 0.0).unwrap();
        let mut traversal_stack = BvhTraversalStack::new();
        for _ in 0..80 {
            let query = Vec3 {
                x: random_signed(&mut state) * 6.0,
                y: random_signed(&mut state) * 6.0,
                z: random_signed(&mut state) * 6.0,
            };
            let actual = projector.project_with_stack(query, &mut traversal_stack);
            let expected = projector
                .triangles
                .iter()
                .map(|triangle| {
                    let (point, barycentric) = closest_point_on_triangle(query, triangle.points);
                    SurfaceHit {
                        point,
                        distance_squared: point.sub(query).length_squared(),
                        primitive_id: triangle.primitive_id,
                        barycentric,
                        normal: triangle.normal,
                    }
                })
                .min_by(|left, right| {
                    left.distance_squared
                        .partial_cmp(&right.distance_squared)
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| left.primitive_id.cmp(&right.primitive_id))
                })
                .unwrap();
            assert_eq!(actual.primitive_id, expected.primitive_id);
            assert!((actual.distance_squared - expected.distance_squared).abs() < 1e-12);
            assert!(actual.point.sub(expected.point).length_squared() < 1e-22);
        }
        assert_eq!(traversal_stack.overflow.capacity(), 0);
    }

    #[test]
    fn safe_surface_projector_returns_stable_attachment() {
        let projector = SurfaceProjector::new(
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[[0, 1, 2]],
            None,
            0.0,
        )
        .unwrap();
        let hit = projector.project([0.25, 0.25, 1.0]).unwrap();
        assert_eq!(hit.point, [0.25, 0.25, 0.0]);
        assert_eq!(hit.distance_squared, 1.0);
        assert_eq!(hit.primitive_id, 0);
        assert_eq!(hit.barycentric, [0.5, 0.25, 0.25]);
        assert_eq!(hit.normal, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn safe_geometry_kernels_match_native_contracts() {
        let canonical = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let flipped = [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]];
        let mask =
            deformation_safety_mask(&canonical, &flipped, &[[0, 1, 2]], 0.02, 0.02, 50.0).unwrap();
        assert_eq!(mask, vec![true]);

        let vertices = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.25, 0.25, -1.0],
            [0.25, 0.25, 1.0],
            [0.75, 0.25, 0.0],
        ];
        assert_eq!(
            triangle_intersection_count(&vertices, &[[0, 1, 2]], &[[3, 4, 5]], 1e-9).unwrap(),
            1
        );
    }
}
