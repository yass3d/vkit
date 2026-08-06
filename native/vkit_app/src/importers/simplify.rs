use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, HashSet},
};

use vkit_core::formats::{ObjAppearance, ObjDocument, ObjFace, OrderedObjMesh};

const MAX_INPUT_TRIANGLES: usize = 5_000_000;
const MAX_CANONICAL_VERTICES: usize = 12_000_000;
const AREA_EPSILON_SQUARED: f64 = 1.0e-28;

const WEDGE_UV_MERGE_DISTANCE: f64 = 0.02;

const NO_UV: u32 = u32::MAX;

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct FaceLabels {
    pub group: Option<String>,
    pub material: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Corner {
    pub source_vertex: u64,
    pub position: [f64; 3],
    pub uv: Option<[f64; 2]>,
}

#[derive(Clone, Copy, Debug)]
struct InputTriangle {
    vertices: [u32; 3],
    corner_uvs: [u32; 3],
    partition: u32,
}

#[derive(Debug)]
pub(crate) struct AttributeMesh {
    positions: Vec<[f64; 3]>,
    texcoords: Vec<[f64; 2]>,
    triangles: Vec<InputTriangle>,
    labels: Vec<FaceLabels>,
    pub material_libraries: Vec<std::path::PathBuf>,
    pub material_names: Vec<String>,

    pub welded_coincident_vertices: usize,
}

impl AttributeMesh {
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }
}

#[derive(Debug, Default)]
pub(crate) struct AttributeMeshBuilder {
    positions: Vec<[f64; 3]>,
    texcoords: Vec<[f64; 2]>,
    triangles: Vec<InputTriangle>,
    labels: Vec<FaceLabels>,
    label_lookup: HashMap<FaceLabels, u32>,
    vertex_lookup: HashMap<u64, u32>,
    texcoord_lookup: HashMap<[u64; 2], u32>,
    material_libraries: Vec<std::path::PathBuf>,
    material_names: Vec<String>,
}

impl AttributeMeshBuilder {
    pub fn with_appearance(
        material_libraries: Vec<std::path::PathBuf>,
        material_names: Vec<String>,
    ) -> Self {
        Self {
            material_libraries,
            material_names,
            ..Self::default()
        }
    }

    pub fn add_triangle(&mut self, labels: FaceLabels, corners: [Corner; 3]) -> Result<(), String> {
        if self.triangles.len() >= MAX_INPUT_TRIANGLES {
            return Err(format!(
                "mesh exceeds the native importer limit of {MAX_INPUT_TRIANGLES} triangles"
            ));
        }
        let partition = if let Some(&index) = self.label_lookup.get(&labels) {
            index
        } else {
            let index = u32::try_from(self.labels.len())
                .map_err(|_| "mesh has too many material/group partitions".to_owned())?;
            self.labels.push(labels.clone());
            self.label_lookup.insert(labels, index);
            index
        };
        let mut vertices = [0_u32; 3];
        let mut corner_uvs = [NO_UV; 3];
        for (slot, corner) in corners.into_iter().enumerate() {
            if !corner.position.iter().all(|value| value.is_finite())
                || corner
                    .uv
                    .is_some_and(|uv| !uv.iter().all(|value| value.is_finite()))
            {
                return Err("mesh contains a non-finite position or UV".to_owned());
            }
            vertices[slot] = if let Some(&index) = self.vertex_lookup.get(&corner.source_vertex) {
                index
            } else {
                if self.positions.len() >= MAX_CANONICAL_VERTICES {
                    return Err(format!(
                        "mesh exceeds the native importer limit of {MAX_CANONICAL_VERTICES} canonical vertices"
                    ));
                }
                let index = u32::try_from(self.positions.len())
                    .map_err(|_| "mesh exceeds the u32 vertex limit".to_owned())?;
                self.positions.push(corner.position);
                self.vertex_lookup.insert(corner.source_vertex, index);
                index
            };
            if let Some(uv) = corner.uv {
                corner_uvs[slot] =
                    intern_texcoord(uv, &mut self.texcoords, &mut self.texcoord_lookup)
                        .ok_or_else(|| {
                            "mesh exceeds the u32 texture-coordinate limit".to_owned()
                        })?;
            }
        }
        self.triangles.push(InputTriangle {
            vertices,
            corner_uvs,
            partition,
        });
        Ok(())
    }

    pub fn finish(mut self) -> Result<AttributeMesh, String> {
        if self.positions.is_empty() || self.triangles.is_empty() {
            return Err("mesh contains no triangle geometry".to_owned());
        }
        for material in self
            .labels
            .iter()
            .filter_map(|labels| labels.material.as_ref())
        {
            if !self.material_names.iter().any(|name| name == material) {
                self.material_names.push(material.clone());
            }
        }
        Ok(AttributeMesh {
            positions: self.positions,
            texcoords: self.texcoords,
            triangles: self.triangles,
            labels: self.labels,
            material_libraries: self.material_libraries,
            material_names: self.material_names,
            welded_coincident_vertices: 0,
        })
    }
}

fn intern_texcoord(
    uv: [f64; 2],
    texcoords: &mut Vec<[f64; 2]>,
    lookup: &mut HashMap<[u64; 2], u32>,
) -> Option<u32> {
    let key = [uv[0].to_bits(), uv[1].to_bits()];
    if let Some(&index) = lookup.get(&key) {
        return Some(index);
    }
    let index = u32::try_from(texcoords.len()).ok()?;
    if index == NO_UV {
        return None;
    }
    texcoords.push(uv);
    lookup.insert(key, index);
    Some(index)
}

pub(crate) fn from_obj_document(document: &ObjDocument) -> Result<AttributeMesh, String> {
    document.validate().map_err(|error| error.to_string())?;
    let mut builder = AttributeMeshBuilder::with_appearance(
        document.appearance.material_libraries.clone(),
        document.appearance.material_names.clone(),
    );
    for (face, face_uvs) in document
        .geometry
        .faces
        .iter()
        .zip(&document.appearance.face_texcoord_indices)
    {
        let labels = FaceLabels {
            group: face.group.clone(),
            material: face.material.clone(),
        };
        for corner in 1..face.vertex_indices.len() - 1 {
            let indices = [0, corner, corner + 1];
            let corners = indices.map(|face_corner| {
                let position_index = face.vertex_indices[face_corner] as usize;
                Corner {
                    source_vertex: position_index as u64,
                    position: document.geometry.vertices[position_index],
                    uv: face_uvs[face_corner]
                        .map(|index| document.appearance.texcoords[index as usize]),
                }
            });
            builder.add_triangle(labels.clone(), corners)?;
        }
    }
    builder.finish()
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SimplificationReceipt {
    pub attempted: bool,
    pub source_triangles: usize,
    pub final_triangles: usize,
    pub collapsed_edges: usize,
    pub removed_degenerate_triangles: usize,

    pub budget_trimmed_triangles: usize,

    pub welded_coincident_vertices: usize,

    pub open_boundary_edges: usize,
}

pub(crate) struct SimplifiedMesh {
    pub document: ObjDocument,
    pub receipt: SimplificationReceipt,
}

#[derive(Clone, Copy, Debug, Default)]
struct Quadric([f64; 10]);

impl Quadric {
    fn from_plane(plane: [f64; 4], weight: f64) -> Self {
        let [a, b, c, d] = plane;
        Self([
            a * a * weight,
            a * b * weight,
            a * c * weight,
            a * d * weight,
            b * b * weight,
            b * c * weight,
            b * d * weight,
            c * c * weight,
            c * d * weight,
            d * d * weight,
        ])
    }

    fn add_assign(&mut self, rhs: Self) {
        for (left, right) in self.0.iter_mut().zip(rhs.0) {
            *left += right;
        }
    }

    fn sum(self, rhs: Self) -> Self {
        let mut result = self;
        result.add_assign(rhs);
        result
    }

    fn evaluate(self, [x, y, z]: [f64; 3]) -> f64 {
        let q = self.0;
        q[0] * x * x
            + 2.0 * q[1] * x * y
            + 2.0 * q[2] * x * z
            + 2.0 * q[3] * x
            + q[4] * y * y
            + 2.0 * q[5] * y * z
            + 2.0 * q[6] * y
            + q[7] * z * z
            + 2.0 * q[8] * z
            + q[9]
    }

    fn stationary_point(self) -> Option<[f64; 3]> {
        let q = self.0;
        solve_symmetric_3x3(
            [[q[0], q[1], q[2]], [q[1], q[4], q[5]], [q[2], q[5], q[7]]],
            [-q[3], -q[6], -q[8]],
        )
    }
}

#[derive(Clone, Copy, Debug)]
struct WorkingVertex {
    position: [f64; 3],
    quadric: Quadric,
    active: bool,
    version: u32,
}

#[derive(Clone, Copy, Debug)]
struct WorkingTriangle {
    vertices: [u32; 3],
    corner_uvs: [u32; 3],
    partition: u32,
    active: bool,
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    cost: f64,
    keep: u32,
    remove: u32,
    keep_version: u32,
    remove_version: u32,
    position: [f64; 3],
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cost.to_bits() == other.cost.to_bits()
            && self.keep == other.keep
            && self.remove == other.remove
            && self.keep_version == other.keep_version
            && self.remove_version == other.remove_version
    }
}

impl Eq for Candidate {}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.keep.cmp(&self.keep))
            .then_with(|| other.remove.cmp(&self.remove))
            .then_with(|| other.keep_version.cmp(&self.keep_version))
            .then_with(|| other.remove_version.cmp(&self.remove_version))
    }
}

#[derive(Clone, Copy)]
struct EdgeInfo {
    faces: u8,
    normal: [f64; 3],
}

pub(crate) fn simplify_to_limit(
    mesh: AttributeMesh,
    target: usize,
    mut progress: impl FnMut(f32),
) -> Result<SimplifiedMesh, String> {
    let source_triangles = mesh.triangles.len();
    if target == 0 {
        return Err("triangle target must be nonzero".to_owned());
    }
    let mut vertices = mesh
        .positions
        .iter()
        .map(|&position| WorkingVertex {
            position,
            quadric: Quadric::default(),
            active: true,
            version: 0,
        })
        .collect::<Vec<_>>();
    let mut triangles = mesh
        .triangles
        .iter()
        .map(|triangle| WorkingTriangle {
            vertices: triangle.vertices,
            corner_uvs: triangle.corner_uvs,
            partition: triangle.partition,
            active: true,
        })
        .collect::<Vec<_>>();

    let mut texcoords = mesh.texcoords.clone();
    let mut texcoord_lookup = HashMap::with_capacity(texcoords.len());
    for (index, uv) in texcoords.iter().enumerate() {
        texcoord_lookup
            .entry([uv[0].to_bits(), uv[1].to_bits()])
            .or_insert(index as u32);
    }
    let mut adjacency = vec![Vec::<usize>::new(); vertices.len()];
    let mut edges = HashMap::<(u32, u32), EdgeInfo>::with_capacity(source_triangles * 2);
    let mut active_triangles = source_triangles;
    let mut removed_degenerate = 0_usize;

    for (face_index, triangle) in triangles.iter_mut().enumerate() {
        let [a, b, c] = triangle
            .vertices
            .map(|index| vertices[index as usize].position);
        let Some(normal) = triangle_normal(a, b, c) else {
            triangle.active = false;
            active_triangles -= 1;
            removed_degenerate += 1;
            continue;
        };
        let plane = [normal[0], normal[1], normal[2], -dot(normal, a)];
        let quadric = Quadric::from_plane(plane, 1.0);
        for index in triangle.vertices {
            vertices[index as usize].quadric.add_assign(quadric);
            adjacency[index as usize].push(face_index);
        }
        for edge in triangle_edges(triangle.vertices) {
            edges
                .entry(edge)
                .and_modify(|info| info.faces = info.faces.saturating_add(1))
                .or_insert(EdgeInfo { faces: 1, normal });
        }
    }
    if active_triangles == 0 {
        return Err("mesh contains only degenerate triangles".to_owned());
    }

    let mut sorted_edges = edges.into_iter().collect::<Vec<_>>();
    sorted_edges.sort_unstable_by_key(|(edge, _)| *edge);

    let mut open_boundary_edges = 0_usize;
    for &((a, b), info) in &sorted_edges {
        if info.faces != 1 {
            continue;
        }
        open_boundary_edges += 1;
        let pa = vertices[a as usize].position;
        let pb = vertices[b as usize].position;
        let edge_direction = normalize(sub(pb, pa)).unwrap_or([1.0, 0.0, 0.0]);
        let boundary_normal = normalize(cross(edge_direction, info.normal)).unwrap_or(info.normal);
        let plane = [
            boundary_normal[0],
            boundary_normal[1],
            boundary_normal[2],
            -dot(boundary_normal, pa),
        ];
        let constraint = Quadric::from_plane(plane, 10.0);
        vertices[a as usize].quadric.add_assign(constraint);
        vertices[b as usize].quadric.add_assign(constraint);
    }

    let mut heap = BinaryHeap::with_capacity(sorted_edges.len());
    for &((a, b), _) in &sorted_edges {
        if let Some(candidate) = make_candidate(a, b, &vertices) {
            heap.push(candidate);
        }
    }
    drop(sorted_edges);

    let goal_removals = active_triangles.saturating_sub(target).max(1);
    let mut collapsed_edges = 0_usize;
    while active_triangles > target {
        let Some(candidate) = heap.pop() else {
            break;
        };
        let keep = candidate.keep as usize;
        let remove = candidate.remove as usize;
        if !vertices[keep].active
            || !vertices[remove].active
            || vertices[keep].version != candidate.keep_version
            || vertices[remove].version != candidate.remove_version
            || !edge_exists(candidate.keep, candidate.remove, &triangles, &adjacency)
        {
            continue;
        }
        if !collapse_is_valid(
            candidate.keep,
            candidate.remove,
            candidate.position,
            &vertices,
            &triangles,
            &adjacency,
        ) {
            continue;
        }

        let affected = unique_affected_faces(candidate.keep, candidate.remove, &adjacency);
        vertices[keep].position = candidate.position;
        vertices[keep].quadric = vertices[keep].quadric.sum(vertices[remove].quadric);
        vertices[keep].version = vertices[keep].version.wrapping_add(1);
        vertices[remove].active = false;
        vertices[remove].version = vertices[remove].version.wrapping_add(1);

        let mut retained_faces = Vec::with_capacity(affected.len());
        for face_index in affected {
            let triangle = &mut triangles[face_index];
            if !triangle.active {
                continue;
            }
            for index in &mut triangle.vertices {
                if *index == candidate.remove {
                    *index = candidate.keep;
                }
            }
            let [a, b, c] = triangle.vertices;
            if a == b
                || b == c
                || c == a
                || triangle_normal(
                    vertices[a as usize].position,
                    vertices[b as usize].position,
                    vertices[c as usize].position,
                )
                .is_none()
            {
                triangle.active = false;
                active_triangles -= 1;
                removed_degenerate += 1;
            } else {
                retained_faces.push(face_index);
            }
        }
        adjacency[keep] = retained_faces;
        adjacency[remove].clear();
        merge_vertex_wedges(
            candidate.keep,
            &adjacency[keep],
            &mut triangles,
            &mut texcoords,
            &mut texcoord_lookup,
        );
        collapsed_edges += 1;

        let mut neighbors = HashSet::new();
        for &face_index in &adjacency[keep] {
            for neighbor in triangles[face_index].vertices {
                if neighbor != candidate.keep {
                    neighbors.insert(neighbor);
                }
            }
        }
        for neighbor in neighbors {
            if let Some(next) = make_candidate(candidate.keep, neighbor, &vertices) {
                heap.push(next);
            }
        }
        if collapsed_edges & 0x3ff == 0 {
            let completed = source_triangles.saturating_sub(active_triangles);
            progress((completed as f32 / goal_removals as f32).clamp(0.0, 0.995));
        }
    }

    let mut budget_trimmed = 0_usize;
    if active_triangles > target {
        let original_active = active_triangles;
        let mut seen = 0_usize;
        let mut kept = 0_usize;
        for triangle in &mut triangles {
            if !triangle.active {
                continue;
            }
            let before = (seen as u128 * target as u128) / original_active as u128;
            seen += 1;
            let after = (seen as u128 * target as u128) / original_active as u128;
            if after > before && kept < target {
                kept += 1;
            } else {
                triangle.active = false;
                budget_trimmed += 1;
            }
        }
        active_triangles = kept;
    }
    if active_triangles == 0 || active_triangles > target {
        return Err(
            "native simplification did not produce a usable mesh within the triangle cap"
                .to_owned(),
        );
    }

    let document = compact_document(vertices, triangles, &texcoords, &mesh)?;
    document.validate().map_err(|error| error.to_string())?;
    progress(1.0);
    Ok(SimplifiedMesh {
        document,
        receipt: SimplificationReceipt {
            attempted: source_triangles > target,
            source_triangles,
            final_triangles: active_triangles,
            collapsed_edges,
            removed_degenerate_triangles: removed_degenerate,
            budget_trimmed_triangles: budget_trimmed,
            welded_coincident_vertices: mesh.welded_coincident_vertices,
            open_boundary_edges,
        },
    })
}

fn make_candidate(first: u32, second: u32, vertices: &[WorkingVertex]) -> Option<Candidate> {
    let (keep, remove) = if first < second {
        (first, second)
    } else {
        (second, first)
    };
    let a = vertices.get(keep as usize)?;
    let b = vertices.get(remove as usize)?;
    if !a.active || !b.active {
        return None;
    }
    let quadric = a.quadric.sum(b.quadric);
    let midpoint = mul(add(a.position, b.position), 0.5);
    let mut positions = [a.position, b.position, midpoint, midpoint];
    if let Some(optimal) = quadric
        .stationary_point()
        .filter(|p| p.iter().all(|v| v.is_finite()))
    {
        positions[3] = optimal;
    }
    let (cost, position) = positions
        .into_iter()
        .map(|position| (quadric.evaluate(position).max(0.0), position))
        .filter(|(cost, _)| cost.is_finite())
        .min_by(|left, right| left.0.total_cmp(&right.0))?;
    Some(Candidate {
        cost,
        keep,
        remove,
        keep_version: a.version,
        remove_version: b.version,
        position,
    })
}

fn merge_vertex_wedges(
    vertex: u32,
    faces: &[usize],
    triangles: &mut [WorkingTriangle],
    texcoords: &mut Vec<[f64; 2]>,
    texcoord_lookup: &mut HashMap<[u64; 2], u32>,
) {
    let mut wedge_ids = Vec::new();
    for &face_index in faces {
        let triangle = &triangles[face_index];
        if !triangle.active {
            continue;
        }
        for (&corner_vertex, &corner_uv) in triangle.vertices.iter().zip(&triangle.corner_uvs) {
            if corner_vertex == vertex && corner_uv != NO_UV {
                wedge_ids.push(corner_uv);
            }
        }
    }
    wedge_ids.sort_unstable();
    wedge_ids.dedup();
    if wedge_ids.len() < 2 {
        return;
    }

    let threshold_squared = WEDGE_UV_MERGE_DISTANCE * WEDGE_UV_MERGE_DISTANCE;
    let mut clusters: Vec<Vec<u32>> = Vec::new();
    for &wedge in &wedge_ids {
        let uv = texcoords[wedge as usize];
        let mut best: Option<(usize, f64)> = None;
        for (cluster_index, cluster) in clusters.iter().enumerate() {
            let anchor = texcoords[cluster[0] as usize];
            let du = uv[0] - anchor[0];
            let dv = uv[1] - anchor[1];
            let distance_squared = du * du + dv * dv;
            if distance_squared <= threshold_squared
                && best.is_none_or(|(_, best_distance)| distance_squared < best_distance)
            {
                best = Some((cluster_index, distance_squared));
            }
        }
        match best {
            Some((cluster_index, _)) => clusters[cluster_index].push(wedge),
            None => clusters.push(vec![wedge]),
        }
    }

    let mut remap = HashMap::new();
    for cluster in clusters.iter().filter(|cluster| cluster.len() > 1) {
        let scale = (cluster.len() as f64).recip();
        let mut merged = [0.0_f64; 2];
        for &member in cluster {
            merged[0] += texcoords[member as usize][0] * scale;
            merged[1] += texcoords[member as usize][1] * scale;
        }

        let Some(merged_id) = intern_texcoord(merged, texcoords, texcoord_lookup) else {
            continue;
        };
        for &member in cluster {
            if member != merged_id {
                remap.insert(member, merged_id);
            }
        }
    }
    if remap.is_empty() {
        return;
    }
    for &face_index in faces {
        let triangle = &mut triangles[face_index];
        if !triangle.active {
            continue;
        }
        for (&corner_vertex, corner_uv) in triangle.vertices.iter().zip(&mut triangle.corner_uvs) {
            if corner_vertex == vertex
                && let Some(&merged_id) = remap.get(corner_uv)
            {
                *corner_uv = merged_id;
            }
        }
    }
}

fn edge_exists(a: u32, b: u32, triangles: &[WorkingTriangle], adjacency: &[Vec<usize>]) -> bool {
    adjacency[a as usize].iter().any(|&face_index| {
        let triangle = triangles[face_index];
        triangle.active && triangle.vertices.contains(&a) && triangle.vertices.contains(&b)
    })
}

fn unique_affected_faces(a: u32, b: u32, adjacency: &[Vec<usize>]) -> Vec<usize> {
    let mut faces = adjacency[a as usize]
        .iter()
        .chain(&adjacency[b as usize])
        .copied()
        .collect::<Vec<_>>();
    faces.sort_unstable();
    faces.dedup();
    faces
}

fn collapse_is_valid(
    keep: u32,
    remove: u32,
    position: [f64; 3],
    vertices: &[WorkingVertex],
    triangles: &[WorkingTriangle],
    adjacency: &[Vec<usize>],
) -> bool {
    for face_index in unique_affected_faces(keep, remove, adjacency) {
        let triangle = triangles[face_index];
        if !triangle.active
            || (triangle.vertices.contains(&keep) && triangle.vertices.contains(&remove))
        {
            continue;
        }
        let old = triangle
            .vertices
            .map(|index| vertices[index as usize].position);
        let Some(old_normal) = triangle_normal(old[0], old[1], old[2]) else {
            continue;
        };
        let new = triangle.vertices.map(|index| {
            if index == keep || index == remove {
                position
            } else {
                vertices[index as usize].position
            }
        });
        let Some(new_normal) = triangle_normal(new[0], new[1], new[2]) else {
            return false;
        };
        if dot(old_normal, new_normal) <= 0.05 {
            return false;
        }
    }
    true
}

fn compact_document(
    vertices: Vec<WorkingVertex>,
    triangles: Vec<WorkingTriangle>,
    texcoords: &[[f64; 2]],
    source: &AttributeMesh,
) -> Result<ObjDocument, String> {
    let mut used_vertices = vec![false; vertices.len()];
    let mut used_texcoords = vec![false; texcoords.len()];
    for triangle in triangles.iter().filter(|triangle| triangle.active) {
        for index in triangle.vertices {
            used_vertices[index as usize] = true;
        }
        for corner_uv in triangle.corner_uvs {
            if corner_uv != NO_UV {
                used_texcoords[corner_uv as usize] = true;
            }
        }
    }
    let mut vertex_remap = vec![u32::MAX; vertices.len()];
    let mut positions = Vec::new();
    for (old_index, vertex) in vertices.iter().enumerate() {
        if !used_vertices[old_index] {
            continue;
        }
        let new_index = u32::try_from(positions.len())
            .map_err(|_| "simplified mesh exceeds the u32 vertex limit".to_owned())?;
        vertex_remap[old_index] = new_index;
        positions.push(vertex.position);
    }
    let mut texcoord_remap = vec![u32::MAX; texcoords.len()];
    let mut emitted_texcoords = Vec::new();
    for (old_index, &uv) in texcoords.iter().enumerate() {
        if !used_texcoords[old_index] {
            continue;
        }
        let new_index = u32::try_from(emitted_texcoords.len())
            .map_err(|_| "simplified mesh exceeds the u32 UV limit".to_owned())?;
        texcoord_remap[old_index] = new_index;
        emitted_texcoords.push(uv);
    }

    let mut faces = Vec::new();
    let mut face_texcoord_indices = Vec::new();
    for triangle in triangles.into_iter().filter(|triangle| triangle.active) {
        let labels = source
            .labels
            .get(triangle.partition as usize)
            .ok_or_else(|| "triangle references a missing appearance partition".to_owned())?;
        faces.push(ObjFace {
            vertex_indices: triangle
                .vertices
                .map(|index| vertex_remap[index as usize])
                .to_vec(),
            group: labels.group.clone(),
            material: labels.material.clone(),
        });
        face_texcoord_indices.push(
            triangle
                .corner_uvs
                .map(|corner_uv| (corner_uv != NO_UV).then(|| texcoord_remap[corner_uv as usize]))
                .to_vec(),
        );
    }
    if faces.is_empty() || positions.is_empty() {
        return Err("simplification produced an empty mesh".to_owned());
    }
    Ok(ObjDocument {
        geometry: OrderedObjMesh {
            vertices: positions,
            faces,
        },
        appearance: ObjAppearance {
            texcoords: emitted_texcoords,
            face_texcoord_indices,
            material_libraries: source.material_libraries.clone(),
            material_names: source.material_names.clone(),
        },
    })
}

fn solve_symmetric_3x3(matrix: [[f64; 3]; 3], rhs: [f64; 3]) -> Option<[f64; 3]> {
    let [[a, b, c], [_, d, e], [_, _, f]] = matrix;
    let determinant = a * (d * f - e * e) - b * (b * f - c * e) + c * (b * e - c * d);
    let scale = matrix
        .iter()
        .flatten()
        .fold(0.0_f64, |largest, value| largest.max(value.abs()))
        .max(1.0);
    if !determinant.is_finite() || determinant.abs() <= f64::EPSILON * scale.powi(3) * 32.0 {
        return None;
    }
    let inverse = [
        [d * f - e * e, c * e - b * f, b * e - c * d],
        [c * e - b * f, a * f - c * c, b * c - a * e],
        [b * e - c * d, b * c - a * e, a * d - b * b],
    ];
    let result = inverse.map(|row| dot(row, rhs) / determinant);
    result
        .iter()
        .all(|value| value.is_finite())
        .then_some(result)
}

fn triangle_edges([a, b, c]: [u32; 3]) -> [(u32, u32); 3] {
    [ordered_edge(a, b), ordered_edge(b, c), ordered_edge(c, a)]
}

fn ordered_edge(a: u32, b: u32) -> (u32, u32) {
    if a < b { (a, b) } else { (b, a) }
}

fn triangle_normal(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> Option<[f64; 3]> {
    let normal = cross(sub(b, a), sub(c, a));
    let length_squared = dot(normal, normal);
    if !length_squared.is_finite() || length_squared <= AREA_EPSILON_SQUARED {
        None
    } else {
        Some(mul(normal, length_squared.sqrt().recip()))
    }
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn mul(vector: [f64; 3], scalar: f64) -> [f64; 3] {
    [vector[0] * scalar, vector[1] * scalar, vector[2] * scalar]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(vector: [f64; 3]) -> Option<[f64; 3]> {
    let length_squared = dot(vector, vector);
    (length_squared.is_finite() && length_squared > AREA_EPSILON_SQUARED)
        .then(|| mul(vector, length_squared.sqrt().recip()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid_with_uv_scale(size: usize, split_material: bool, uv_scale: f64) -> AttributeMesh {
        let mut builder = AttributeMeshBuilder::default();
        for y in 0..size {
            for x in 0..size {
                let index = |x: usize, y: usize| (y * (size + 1) + x) as u64;
                let corner = |x: usize, y: usize| Corner {
                    source_vertex: index(x, y),
                    position: [x as f64, y as f64, 0.0],
                    uv: Some([x as f64 * uv_scale, y as f64 * uv_scale]),
                };
                let labels = FaceLabels {
                    group: Some("scan".to_owned()),
                    material: Some(
                        if split_material && x >= size / 2 {
                            "right"
                        } else {
                            "left"
                        }
                        .to_owned(),
                    ),
                };
                builder
                    .add_triangle(
                        labels.clone(),
                        [corner(x, y), corner(x + 1, y), corner(x + 1, y + 1)],
                    )
                    .unwrap();
                builder
                    .add_triangle(
                        labels,
                        [corner(x, y), corner(x + 1, y + 1), corner(x, y + 1)],
                    )
                    .unwrap();
            }
        }
        builder.finish().unwrap()
    }

    fn grid(size: usize, split_material: bool) -> AttributeMesh {
        grid_with_uv_scale(size, split_material, 1.0 / size as f64)
    }

    fn charted_seam_grid(size: usize) -> AttributeMesh {
        let mid = size / 2;
        let mut builder = AttributeMeshBuilder::default();
        for y in 0..size {
            for x in 0..size {
                let left = x < mid;
                let corner = |x: usize, y: usize| {
                    let u = if left {
                        0.4 * x as f64 / mid as f64
                    } else {
                        0.6 + 0.4 * (x - mid) as f64 / (size - mid) as f64
                    };
                    Corner {
                        source_vertex: (y * (size + 1) + x) as u64,
                        position: [x as f64, y as f64, 0.0],
                        uv: Some([u, y as f64 / size as f64]),
                    }
                };
                let labels = FaceLabels {
                    group: Some("scan".to_owned()),
                    material: Some(if left { "left" } else { "right" }.to_owned()),
                };
                builder
                    .add_triangle(
                        labels.clone(),
                        [corner(x, y), corner(x + 1, y), corner(x + 1, y + 1)],
                    )
                    .unwrap();
                builder
                    .add_triangle(
                        labels,
                        [corner(x, y), corner(x + 1, y + 1), corner(x, y + 1)],
                    )
                    .unwrap();
            }
        }
        builder.finish().unwrap()
    }

    fn assert_positions_distinct(document: &ObjDocument) {
        let mut keys = document
            .geometry
            .vertices
            .iter()
            .map(|vertex| vertex.map(f64::to_bits))
            .collect::<Vec<_>>();
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(
            keys.len(),
            total,
            "welded output must not contain coincident duplicate positions"
        );
    }

    fn material_vertex_sets(document: &ObjDocument) -> (HashSet<u32>, HashSet<u32>) {
        let mut left = HashSet::new();
        let mut right = HashSet::new();
        for face in &document.geometry.faces {
            let target = match face.material.as_deref() {
                Some("left") => &mut left,
                Some("right") => &mut right,
                _ => continue,
            };
            target.extend(face.vertex_indices.iter().copied());
        }
        (left, right)
    }

    fn open_edge_count(document: &ObjDocument) -> usize {
        let mut edges = HashMap::<(u32, u32), usize>::new();
        for face in &document.geometry.faces {
            let indices = &face.vertex_indices;
            for corner in 0..indices.len() {
                let a = indices[corner];
                let b = indices[(corner + 1) % indices.len()];
                *edges.entry(ordered_edge(a, b)).or_insert(0) += 1;
            }
        }
        edges.values().filter(|&&count| count == 1).count()
    }

    #[test]
    fn qem_result_is_deterministic_and_respects_target() {
        let first = simplify_to_limit(grid(12, false), 40, |_| {}).unwrap();
        let second = simplify_to_limit(grid(12, false), 40, |_| {}).unwrap();
        assert_eq!(first.document, second.document);
        assert_eq!(first.receipt, second.receipt);
        assert!(first.receipt.final_triangles <= 40);
        assert!(first.receipt.collapsed_edges > 0);
    }

    #[test]
    fn material_partitions_share_welded_canonical_geometry() {
        let result = simplify_to_limit(grid(8, true), 24, |_| {}).unwrap();
        let (left, right) = material_vertex_sets(&result.document);
        assert!(
            !left.is_empty(),
            "left material must survive simplification"
        );
        assert!(
            !right.is_empty(),
            "right material must survive simplification"
        );
        assert!(
            left.intersection(&right).next().is_some(),
            "the material seam must remain shared canonical topology"
        );
        assert_positions_distinct(&result.document);
        assert!(result.receipt.final_triangles <= 24);
        assert!(result.receipt.collapsed_edges > 0);
    }

    #[test]
    fn uv_chart_seam_stays_welded_and_charts_never_blend() {
        let size = 8;
        let result = simplify_to_limit(charted_seam_grid(size), 40, |_| {}).unwrap();
        assert!(result.receipt.collapsed_edges > 0);
        assert_eq!(
            result.receipt.open_boundary_edges,
            4 * size,
            "only the outer perimeter is an open boundary"
        );

        assert_positions_distinct(&result.document);
        let (left, right) = material_vertex_sets(&result.document);
        assert!(
            left.intersection(&right).next().is_some(),
            "chart seam vertices must stay shared between both sides"
        );
        for &[u, _] in &result.document.appearance.texcoords {
            assert!(
                u <= 0.4 + 1e-9 || u >= 0.6 - 1e-9,
                "wedge UVs must never blend across charts (u = {u})"
            );
        }
        assert!(
            open_edge_count(&result.document) <= 4 * size,
            "simplification must not open new seam edges"
        );
    }

    #[test]
    fn single_chart_wedges_merge_within_threshold() {
        let result = simplify_to_limit(grid_with_uv_scale(6, false, 0.001), 30, |_| {}).unwrap();
        assert!(result.receipt.collapsed_edges > 0);
        let mut wedges = HashMap::<u32, HashSet<u32>>::new();
        for (face, corner_uvs) in result
            .document
            .geometry
            .faces
            .iter()
            .zip(&result.document.appearance.face_texcoord_indices)
        {
            for (&vertex, &uv) in face.vertex_indices.iter().zip(corner_uvs) {
                wedges
                    .entry(vertex)
                    .or_default()
                    .insert(uv.expect("every grid corner carries a UV"));
            }
        }
        assert!(
            wedges.values().all(|ids| ids.len() == 1),
            "single-chart wedges must merge to one UV per vertex"
        );
    }

    #[test]
    fn welded_obj_document_roundtrip_preserves_shared_topology() {
        let document = ObjDocument {
            geometry: OrderedObjMesh {
                vertices: vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [2.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [1.0, 1.0, 0.0],
                    [2.0, 1.0, 0.0],
                ],
                faces: vec![
                    ObjFace {
                        vertex_indices: vec![0, 1, 4, 3],
                        group: None,
                        material: Some("left".to_owned()),
                    },
                    ObjFace {
                        vertex_indices: vec![1, 2, 5, 4],
                        group: None,
                        material: Some("right".to_owned()),
                    },
                ],
            },
            appearance: ObjAppearance {
                texcoords: vec![
                    [0.0, 0.0],
                    [0.4, 0.0],
                    [0.4, 1.0],
                    [0.0, 1.0],
                    [0.6, 0.0],
                    [1.0, 0.0],
                    [1.0, 1.0],
                    [0.6, 1.0],
                ],
                face_texcoord_indices: vec![
                    vec![Some(0), Some(1), Some(2), Some(3)],
                    vec![Some(4), Some(5), Some(6), Some(7)],
                ],
                material_libraries: Vec::new(),
                material_names: vec!["left".to_owned(), "right".to_owned()],
            },
        };
        document.validate().unwrap();
        let mesh = from_obj_document(&document).unwrap();
        assert_eq!(mesh.triangle_count(), 4);
        let result = simplify_to_limit(mesh, 100, |_| {}).unwrap();
        assert!(!result.receipt.attempted);
        assert_eq!(result.receipt.collapsed_edges, 0);
        assert_eq!(result.receipt.open_boundary_edges, 6);
        assert_eq!(
            result.document.geometry.vertices.len(),
            6,
            "an author-welded OBJ must never re-split at UV or material seams"
        );
        let mut expected = document
            .geometry
            .vertices
            .iter()
            .map(|vertex| vertex.map(f64::to_bits))
            .collect::<Vec<_>>();
        let mut actual = result
            .document
            .geometry
            .vertices
            .iter()
            .map(|vertex| vertex.map(f64::to_bits))
            .collect::<Vec<_>>();
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(expected, actual);
        assert_eq!(result.document.appearance.texcoords.len(), 8);
        assert!(
            result
                .document
                .appearance
                .face_texcoord_indices
                .iter()
                .flatten()
                .all(Option::is_some)
        );
        let (left, right) = material_vertex_sets(&result.document);
        assert_eq!(
            left.intersection(&right).count(),
            2,
            "the two quads must keep sharing exactly their two seam vertices"
        );
    }

    #[test]
    fn true_open_boundaries_keep_their_extents() {
        let size = 10;
        let result = simplify_to_limit(grid(size, false), 30, |_| {}).unwrap();
        assert_eq!(result.receipt.open_boundary_edges, 4 * size);
        assert!(result.receipt.final_triangles <= 30);
        let mut minimum = [f64::INFINITY; 2];
        let mut maximum = [f64::NEG_INFINITY; 2];
        for vertex in &result.document.geometry.vertices {
            for axis in 0..2 {
                minimum[axis] = minimum[axis].min(vertex[axis]);
                maximum[axis] = maximum[axis].max(vertex[axis]);
            }
            assert!(vertex[2].abs() < 1e-9, "planar grid must stay planar");
        }
        let extent = size as f64;
        assert!(minimum[0].abs() < 1e-6 && minimum[1].abs() < 1e-6);
        assert!((maximum[0] - extent).abs() < 1e-6 && (maximum[1] - extent).abs() < 1e-6);
    }

    #[test]
    fn degenerate_input_is_removed_and_counted() {
        let mut builder = AttributeMeshBuilder::default();
        let labels = FaceLabels::default();
        let a = Corner {
            source_vertex: 0,
            position: [0.0, 0.0, 0.0],
            uv: None,
        };
        let b = Corner {
            source_vertex: 1,
            position: [1.0, 0.0, 0.0],
            uv: None,
        };
        let c = Corner {
            source_vertex: 2,
            position: [0.0, 1.0, 0.0],
            uv: None,
        };
        builder.add_triangle(labels.clone(), [a, b, c]).unwrap();
        builder.add_triangle(labels, [a, a, b]).unwrap();
        let result = simplify_to_limit(builder.finish().unwrap(), 10, |_| {}).unwrap();
        assert_eq!(result.receipt.removed_degenerate_triangles, 1);
        assert_eq!(result.receipt.final_triangles, 1);
    }
}
