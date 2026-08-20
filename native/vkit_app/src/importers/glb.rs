use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use gltf::{
    Semantic,
    accessor::{DataType, Dimensions},
    buffer, image,
    mesh::Mode,
};

use super::gltf_container::{self, PayloadBudget};
use super::simplify::{AttributeMesh, AttributeMeshBuilder, Corner, FaceLabels};

type Matrix = [[f64; 4]; 4];

const MAX_PRIMITIVE_VERTICES: usize = 12_000_000;
const MAX_PRIMITIVE_TRIANGLES: usize = 5_000_000;

#[derive(Debug, Default)]
struct SkipTally {
    surfaceless_primitives: usize,
    unreadable_primitives: usize,
    out_of_range_triangles: usize,
    non_finite_triangles: usize,
    dropped_index_tail: usize,
    dropped_uv_streams: usize,
    revisited_nodes: usize,
    bind_pose_meshes: usize,
    unreferenced_roots: usize,
    deep_subtrees: usize,
    non_finite_nodes: usize,
    normal_corrected_primitives: usize,
}

impl SkipTally {
    fn is_empty(&self) -> bool {
        let SkipTally {
            surfaceless_primitives,
            unreadable_primitives,
            out_of_range_triangles,
            non_finite_triangles,
            dropped_index_tail,
            dropped_uv_streams,
            revisited_nodes,
            bind_pose_meshes,
            unreferenced_roots,
            deep_subtrees,
            non_finite_nodes,
            normal_corrected_primitives,
        } = self;
        *surfaceless_primitives == 0
            && *unreadable_primitives == 0
            && *out_of_range_triangles == 0
            && *non_finite_triangles == 0
            && *dropped_index_tail == 0
            && *dropped_uv_streams == 0
            && *revisited_nodes == 0
            && *bind_pose_meshes == 0
            && *unreferenced_roots == 0
            && *deep_subtrees == 0
            && *non_finite_nodes == 0
            && *normal_corrected_primitives == 0
    }

    fn report(&self) {
        if self.is_empty() {
            return;
        }
        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Info,
            "importers",
            "gltf_partial_import",
            &format!(
                "surfaceless_primitives={}; unreadable_primitives={}; out_of_range_triangles={}; \
                 non_finite_triangles={}; dropped_index_tail={}; dropped_uv_streams={}; \
                 revisited_nodes={}; bind_pose_meshes={}; unreferenced_roots={}; \
                 deep_subtrees={}; non_finite_nodes={}; normal_corrected_primitives={}",
                self.surfaceless_primitives,
                self.unreadable_primitives,
                self.out_of_range_triangles,
                self.non_finite_triangles,
                self.dropped_index_tail,
                self.dropped_uv_streams,
                self.revisited_nodes,
                self.bind_pose_meshes,
                self.unreferenced_roots,
                self.deep_subtrees,
                self.non_finite_nodes,
                self.normal_corrected_primitives
            ),
        );
    }
}

#[derive(Debug, Default)]
struct PositionWelder {
    lookup: HashMap<[u64; 3], u64>,
    welded_duplicates: usize,
}

impl PositionWelder {
    fn canonical_id(&mut self, position: [f64; 3]) -> u64 {
        let key = position.map(f64::to_bits);
        let next = self.lookup.len() as u64;
        match self.lookup.entry(key) {
            Entry::Occupied(entry) => {
                self.welded_duplicates += 1;
                *entry.get()
            }
            Entry::Vacant(entry) => *entry.insert(next),
        }
    }
}

pub(crate) fn load_glb(
    path: &Path,
    appearance_root: &Path,
    mut progress: impl FnMut(f32),
) -> Result<AttributeMesh, String> {
    progress(0.05);
    let gltf_container::PreparedContainer {
        gltf,
        buffers,
        mut budget,
    } = gltf_container::prepare(path)?;
    progress(0.16);

    let material_names = gltf
        .materials()
        .map(|material| material_name(material.index(), material.name()))
        .chain(std::iter::once(material_name(None, None)))
        .collect::<Vec<_>>();
    let mut builder = AttributeMeshBuilder::with_appearance(
        vec![PathBuf::from("vkit-import.mtl")],
        material_names,
    );

    let mut welder = PositionWelder::default();
    let mut visited_nodes = 0_usize;
    let mut emitted = HashSet::new();
    let mut tally = SkipTally::default();
    let node_total = gltf.nodes().len().max(1);
    let (roots, unreferenced_roots) = walk_roots(&gltf);
    tally.unreferenced_roots = unreferenced_roots;
    for node in roots {
        visit_node(
            node,
            identity(),
            0,
            &mut SceneWalk {
                buffers: &buffers,
                welder: &mut welder,
                builder: &mut builder,
                visited_nodes: &mut visited_nodes,
                emitted: &mut emitted,
                tally: &mut tally,
            },
            &mut |visited| {
                progress(0.16 + 0.74 * (visited as f32 / node_total as f32).min(1.0));
            },
        )?;
    }
    tally.report();
    let mut mesh = builder.finish()?;
    mesh.welded_coincident_vertices = welder.welded_duplicates;
    progress(0.95);

    write_material_library(
        &gltf,
        &buffers,
        path.parent().unwrap_or_else(|| Path::new(".")),
        appearance_root,
        &mut budget,
    );
    progress(1.0);
    Ok(mesh)
}

fn walk_roots<'a>(gltf: &'a gltf::Gltf) -> (Vec<gltf::Node<'a>>, usize) {
    let mut roots = Vec::new();
    let mut claimed = HashSet::new();
    if let Some(scene) = gltf.default_scene().or_else(|| gltf.scenes().next()) {
        for node in scene.nodes() {
            if claimed.insert(node.index()) {
                roots.push(node);
            }
        }
    }
    let mut parented = vec![false; gltf.nodes().len()];
    for node in gltf.nodes() {
        for child in node.children() {
            if let Some(slot) = parented.get_mut(child.index()) {
                *slot = true;
            }
        }
    }
    let mut unreferenced = 0_usize;
    for node in gltf.nodes() {
        if parented.get(node.index()).copied().unwrap_or(false) {
            continue;
        }
        if claimed.insert(node.index()) {
            unreferenced += 1;
            roots.push(node);
        }
    }
    (roots, unreferenced)
}

struct SceneWalk<'a> {
    buffers: &'a [Vec<u8>],
    welder: &'a mut PositionWelder,
    builder: &'a mut AttributeMeshBuilder,
    visited_nodes: &'a mut usize,
    emitted: &'a mut HashSet<usize>,
    tally: &'a mut SkipTally,
}

fn visit_node(
    node: gltf::Node<'_>,
    parent: Matrix,
    depth: usize,
    walk: &mut SceneWalk<'_>,
    progress: &mut impl FnMut(usize),
) -> Result<(), String> {
    let SceneWalk {
        buffers,
        welder,
        builder,
        visited_nodes,
        emitted,
        tally,
    } = &mut *walk;
    let (welder, builder, visited_nodes) = (&mut **welder, &mut **builder, &mut **visited_nodes);
    let (emitted, tally) = (&mut **emitted, &mut **tally);
    let buffers: &[Vec<u8>] = buffers;
    if depth > 256 {
        tally.deep_subtrees += 1;
        return Ok(());
    }
    if !emitted.insert(node.index()) {
        tally.revisited_nodes += 1;
        return Ok(());
    }
    let local = node
        .transform()
        .matrix()
        .map(|column| column.map(f64::from));
    let world = multiply(parent, local);
    if !world.iter().flatten().all(|value| value.is_finite()) {
        tally.non_finite_nodes += 1;
        return Ok(());
    }
    if let Some(mesh) = node.mesh() {
        let group = Some(safe_label(
            node.name().or(mesh.name()).unwrap_or("Mesh"),
            &format!("GLB_Node_{}", node.index()),
        ));
        let skinned = node.skin().is_some();
        if skinned {
            tally.bind_pose_meshes += 1;
        }
        let placement = mesh_placement(skinned, world);
        for primitive in mesh.primitives() {
            let Some(topology) = Topology::of(primitive.mode()) else {
                tally.surfaceless_primitives += 1;
                continue;
            };
            let local_positions = match read_positions(&primitive, buffers) {
                Ok(positions) => positions,
                Err(reason) => {
                    tally.unreadable_primitives += 1;
                    let _ = crate::diagnostics::record(
                        crate::diagnostics::Severity::Warning,
                        "importers",
                        "gltf_primitive_skipped",
                        &format!(
                            "mesh={} primitive={}; {reason}",
                            mesh.index(),
                            primitive.index()
                        ),
                    );
                    continue;
                }
            };
            if local_positions.is_empty() {
                tally.unreadable_primitives += 1;
                continue;
            }
            let material = primitive.material();
            let pbr = material.pbr_metallic_roughness();
            let base_color = pbr.base_color_texture();
            let uv_set = base_color
                .as_ref()
                .map(|info| {
                    info.texture_transform()
                        .and_then(|transform| transform.tex_coord())
                        .unwrap_or_else(|| info.tex_coord())
                })
                .or_else(|| material_uv_sets(&material).first().map(|(set, _)| *set))
                .unwrap_or(0);
            let transform = material_uv_sets(&material)
                .into_iter()
                .find(|(set, transform)| *set == uv_set && transform.is_some())
                .and_then(|(_, transform)| transform);
            let mut uvs = read_tex_coords(
                &primitive,
                buffers,
                uv_set,
                local_positions.len(),
                transform.is_some(),
            );
            if uvs.is_none() && primitive.get(&Semantic::TexCoords(uv_set)).is_some() {
                tally.dropped_uv_streams += 1;
            }
            if let (Some(uvs), Some(transform)) = (uvs.as_mut(), transform.as_ref()) {
                apply_texture_transform(transform, uvs);
            }
            let source_indices = read_indices(&primitive, buffers, local_positions.len());
            if matches!(topology, Topology::Triangles) && !source_indices.len().is_multiple_of(3) {
                tally.dropped_index_tail += 1;
            }
            let Some(indices) = topology.expand(source_indices) else {
                tally.unreadable_primitives += 1;
                continue;
            };
            let labels = FaceLabels {
                group: group.clone(),
                material: Some(material_name(material.index(), material.name())),
            };

            let normals_disagree = read_normals(&primitive, buffers, local_positions.len())
                .is_some_and(|normals| {
                    winding_disagrees_with_normals(&local_positions, &normals, &indices)
                });
            if normals_disagree {
                tally.normal_corrected_primitives += 1;
            }
            let positions = local_positions
                .into_iter()
                .map(|position| transform_point(placement, position))
                .collect::<Vec<_>>();
            let canonical_ids = positions
                .iter()
                .map(|&position| welder.canonical_id(position))
                .collect::<Vec<_>>();
            let reverse_winding = (linear_determinant(placement) < 0.0) != normals_disagree;
            for indices in indices.chunks_exact(3) {
                let mut triangle = [indices[0], indices[1], indices[2]];
                if reverse_winding {
                    triangle.swap(1, 2);
                }
                let corners = triangle.map(|index| {
                    let index = index as usize;
                    positions.get(index).copied().map(|position| Corner {
                        source_vertex: canonical_ids.get(index).copied().unwrap_or_default(),
                        position,
                        uv: uvs.as_ref().and_then(|uvs| uvs.get(index).copied()),
                    })
                });
                let [Some(a), Some(b), Some(c)] = corners else {
                    tally.out_of_range_triangles += 1;
                    continue;
                };
                if ![a.position, b.position, c.position]
                    .iter()
                    .flatten()
                    .all(|value| value.is_finite())
                {
                    tally.non_finite_triangles += 1;
                    continue;
                }
                builder.add_triangle(labels.clone(), [a, b, c])?;
            }
        }
    }
    *visited_nodes += 1;
    progress(*visited_nodes);
    for child in node.children() {
        visit_node(
            child,
            world,
            depth + 1,
            &mut SceneWalk {
                buffers,
                welder,
                builder,
                visited_nodes,
                emitted,
                tally,
            },
            progress,
        )?;
    }
    Ok(())
}

fn mesh_placement(skinned: bool, world: Matrix) -> Matrix {
    if skinned { identity() } else { world }
}

fn material_uv_sets<'a>(
    material: &gltf::Material<'a>,
) -> Vec<(u32, Option<gltf::texture::TextureTransform<'a>>)> {
    let pbr = material.pbr_metallic_roughness();
    [
        pbr.base_color_texture(),
        pbr.metallic_roughness_texture(),
        material.emissive_texture(),
    ]
    .into_iter()
    .flatten()
    .map(|info| {
        let transform = info.texture_transform();
        let set = transform
            .as_ref()
            .and_then(gltf::texture::TextureTransform::tex_coord)
            .unwrap_or_else(|| info.tex_coord());
        (set, transform)
    })
    .collect()
}

fn apply_texture_transform(transform: &gltf::texture::TextureTransform<'_>, uvs: &mut [[f64; 2]]) {
    let [offset_u, offset_v] = transform.offset().map(f64::from);
    let [scale_u, scale_v] = transform.scale().map(f64::from);
    let (sine, cosine) = f64::from(transform.rotation()).sin_cos();
    for uv in uvs {
        let (u, v) = (uv[0] * scale_u, uv[1] * scale_v);
        *uv = [
            offset_u + u * cosine - v * sine,
            offset_v + u * sine + v * cosine,
        ];
    }
}

#[derive(Clone, Copy, Debug)]
enum Topology {
    Triangles,
    Strip,
    Fan,
}

impl Topology {
    fn of(mode: Mode) -> Option<Self> {
        match mode {
            Mode::Triangles => Some(Self::Triangles),
            Mode::TriangleStrip => Some(Self::Strip),
            Mode::TriangleFan => Some(Self::Fan),
            Mode::Points | Mode::Lines | Mode::LineLoop | Mode::LineStrip => None,
        }
    }

    fn expand(self, indices: Vec<u32>) -> Option<Vec<u32>> {
        let triangles = match self {
            Self::Triangles => indices.len() / 3,
            Self::Strip | Self::Fan => indices.len().saturating_sub(2),
        };
        if triangles > MAX_PRIMITIVE_TRIANGLES {
            return None;
        }
        match self {
            Self::Triangles => Some(indices),
            Self::Strip => {
                let mut expanded = Vec::with_capacity(triangles * 3);
                for (step, window) in indices.windows(3).enumerate() {
                    let [first, second, third] = [window[0], window[1], window[2]];
                    if step.is_multiple_of(2) {
                        expanded.extend_from_slice(&[first, second, third]);
                    } else {
                        expanded.extend_from_slice(&[second, first, third]);
                    }
                }
                Some(expanded)
            }
            Self::Fan => {
                let mut expanded = Vec::with_capacity(triangles * 3);
                let hub = *indices.first()?;
                for window in indices.windows(2).skip(1) {
                    expanded.extend_from_slice(&[hub, window[0], window[1]]);
                }
                Some(expanded)
            }
        }
    }
}

fn read_indices(primitive: &gltf::Primitive<'_>, buffers: &[Vec<u8>], vertices: usize) -> Vec<u32> {
    let readable = primitive
        .indices()
        .filter(|accessor| validate_readable_accessor(accessor, buffers, "indices").is_ok());
    if readable.is_some() {
        let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
        if let Some(indices) = reader.read_indices() {
            return indices.into_u32().collect();
        }
    }
    (0..vertices as u32).collect()
}

fn read_tex_coords(
    primitive: &gltf::Primitive<'_>,
    buffers: &[Vec<u8>],
    set: u32,
    vertices: usize,
    dequantized_by_transform: bool,
) -> Option<Vec<[f64; 2]>> {
    let mut wanted = vec![set];
    if set != 0 {
        wanted.push(0);
    }
    for set in wanted {
        let Some(accessor) = primitive.get(&Semantic::TexCoords(set)) else {
            continue;
        };
        if validate_readable_accessor(&accessor, buffers, "TEXCOORD").is_err() {
            continue;
        }
        if !dequantized_by_transform && quantized_without_scale(&accessor) {
            continue;
        }
        let uvs = if tex_coords_read_by_the_crate(&accessor) {
            let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
            let Some(stream) = reader.read_tex_coords(set) else {
                continue;
            };
            stream
                .into_f32()
                .map(|uv| uv.map(f64::from))
                .collect::<Vec<_>>()
        } else {
            let Some(uvs) = decode_tex_coords(&accessor, buffers, vertices) else {
                continue;
            };
            uvs
        };
        if uvs.len() == vertices {
            return Some(uvs);
        }
    }
    None
}

fn read_positions(
    primitive: &gltf::Primitive<'_>,
    buffers: &[Vec<u8>],
) -> Result<Vec<[f64; 3]>, String> {
    read_vec3_attribute(primitive, buffers, &Semantic::Positions, "POSITION")
}

fn read_normals(
    primitive: &gltf::Primitive<'_>,
    buffers: &[Vec<u8>],
    vertices: usize,
) -> Option<Vec<[f64; 3]>> {
    let normals = read_vec3_attribute(primitive, buffers, &Semantic::Normals, "NORMAL").ok()?;
    (normals.len() == vertices).then_some(normals)
}

fn read_vec3_attribute(
    primitive: &gltf::Primitive<'_>,
    buffers: &[Vec<u8>],
    semantic: &Semantic,
    label: &str,
) -> Result<Vec<[f64; 3]>, String> {
    let accessor = primitive
        .get(semantic)
        .ok_or_else(|| format!("no {label} stream"))?;
    if accessor.dimensions() != Dimensions::Vec3 {
        return Err(format!(
            "{label} is {:?}; only VEC3 is supported",
            accessor.dimensions()
        ));
    }
    validate_readable_accessor(&accessor, buffers, label)?;
    let component = position_component(accessor.data_type(), accessor.normalized())?;
    let count = accessor.count();
    if count > MAX_PRIMITIVE_VERTICES {
        return Err(format!(
            "{label} declares {count} vertices, past the {MAX_PRIMITIVE_VERTICES} this importer \
             accepts in one primitive"
        ));
    }
    let element = component.size * 3;
    let mut values = vec![[0.0_f64; 3]; count];
    if let Some(view) = accessor.view() {
        let data = buffer_view_bytes(&view, buffers)?;
        let stride = view.stride().unwrap_or(element);
        for (index, value) in values.iter_mut().enumerate() {
            let base = stride
                .checked_mul(index)
                .and_then(|span| span.checked_add(accessor.offset()))
                .ok_or_else(|| format!("{label} range overflows"))?;
            *value = read_vec3(data, base, &component)?;
        }
    }
    if let Some(sparse) = accessor.sparse() {
        apply_sparse_vec3(&sparse, buffers, &component, label, &mut values)?;
    }
    Ok(values)
}

const WINDING_VOTE_MAJORITY: usize = 4;
const WINDING_VOTE_QUORUM: usize = 16;

fn winding_disagrees_with_normals(
    positions: &[[f64; 3]],
    normals: &[[f64; 3]],
    indices: &[u32],
) -> bool {
    let mut with = 0_usize;
    let mut against = 0_usize;
    for triangle in indices.chunks_exact(3) {
        let corners = [triangle[0], triangle[1], triangle[2]]
            .map(|index| positions.get(index as usize).copied());
        let [Some(a), Some(b), Some(c)] = corners else {
            continue;
        };
        let edge = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let other = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let face = [
            edge[1] * other[2] - edge[2] * other[1],
            edge[2] * other[0] - edge[0] * other[2],
            edge[0] * other[1] - edge[1] * other[0],
        ];
        let mut shipped = [0.0_f64; 3];
        for index in triangle {
            let Some(normal) = normals.get(*index as usize) else {
                continue;
            };
            for (axis, value) in shipped.iter_mut().enumerate() {
                *value += normal[axis];
            }
        }
        let vote = face[0] * shipped[0] + face[1] * shipped[1] + face[2] * shipped[2];
        if !vote.is_finite() || vote == 0.0 {
            continue;
        }
        if vote > 0.0 {
            with += 1;
        } else {
            against += 1;
        }
    }
    against >= WINDING_VOTE_QUORUM && against >= with.saturating_mul(WINDING_VOTE_MAJORITY)
}

fn apply_sparse_vec3(
    sparse: &gltf::accessor::sparse::Sparse<'_>,
    buffers: &[Vec<u8>],
    component: &PositionComponent,
    label: &str,
    positions: &mut [[f64; 3]],
) -> Result<(), String> {
    let indices = sparse.indices();
    let index_view = indices.view();
    let index_data = buffer_view_bytes(&index_view, buffers)?;
    let index_size = indices.index_type().size();
    let index_stride = index_view.stride().unwrap_or(index_size);

    let values = sparse.values();
    let value_view = values.view();
    let value_data = buffer_view_bytes(&value_view, buffers)?;
    let element = component.size * 3;
    let value_stride = value_view.stride().unwrap_or(element);

    for step in 0..sparse.count() {
        let index_at = index_stride
            .checked_mul(step)
            .and_then(|span| span.checked_add(indices.offset()))
            .and_then(|span| span.checked_add(index_size).map(|end| (span, end)))
            .ok_or_else(|| format!("{label} sparse index range overflows"))?;
        let raw = index_data
            .get(index_at.0..index_at.1)
            .ok_or_else(|| format!("{label} sparse indices read past their bufferView"))?;
        let target = match index_size {
            1 => u64::from(u8::from_le_bytes(le_bytes(raw))),
            2 => u64::from(u16::from_le_bytes(le_bytes(raw))),
            _ => u64::from(u32::from_le_bytes(le_bytes(raw))),
        };
        let value_at = value_stride
            .checked_mul(step)
            .and_then(|span| span.checked_add(values.offset()))
            .ok_or_else(|| format!("{label} sparse value range overflows"))?;
        let value = read_vec3(value_data, value_at, component)?;
        if let Ok(target) = usize::try_from(target)
            && let Some(slot) = positions.get_mut(target)
        {
            *slot = value;
        }
    }
    Ok(())
}

fn read_vec3(data: &[u8], base: usize, component: &PositionComponent) -> Result<[f64; 3], String> {
    let mut position = [0.0_f64; 3];
    for (axis, value) in position.iter_mut().enumerate() {
        let at = base
            .checked_add(component.size * axis)
            .ok_or_else(|| "POSITION range overflows".to_owned())?;
        let end = at
            .checked_add(component.size)
            .ok_or_else(|| "POSITION range overflows".to_owned())?;
        let bytes = data
            .get(at..end)
            .ok_or_else(|| "POSITION reads past its bufferView".to_owned())?;
        *value = (component.read)(bytes);
    }
    Ok(position)
}

struct PositionComponent {
    size: usize,
    read: fn(&[u8]) -> f64,
}

fn position_component(data_type: DataType, normalized: bool) -> Result<PositionComponent, String> {
    let component = match (data_type, normalized) {
        (DataType::F32, _) => PositionComponent {
            size: 4,
            read: |bytes| f64::from(f32::from_le_bytes(le_bytes(bytes))),
        },
        (DataType::I8, false) => PositionComponent {
            size: 1,
            read: |bytes| f64::from(i8::from_le_bytes(le_bytes(bytes))),
        },
        (DataType::I8, true) => PositionComponent {
            size: 1,
            read: |bytes| (f64::from(i8::from_le_bytes(le_bytes(bytes))) / 127.0).max(-1.0),
        },
        (DataType::U8, false) => PositionComponent {
            size: 1,
            read: |bytes| f64::from(u8::from_le_bytes(le_bytes(bytes))),
        },
        (DataType::U8, true) => PositionComponent {
            size: 1,
            read: |bytes| f64::from(u8::from_le_bytes(le_bytes(bytes))) / 255.0,
        },
        (DataType::I16, false) => PositionComponent {
            size: 2,
            read: |bytes| f64::from(i16::from_le_bytes(le_bytes(bytes))),
        },
        (DataType::I16, true) => PositionComponent {
            size: 2,
            read: |bytes| (f64::from(i16::from_le_bytes(le_bytes(bytes))) / 32767.0).max(-1.0),
        },
        (DataType::U16, false) => PositionComponent {
            size: 2,
            read: |bytes| f64::from(u16::from_le_bytes(le_bytes(bytes))),
        },
        (DataType::U16, true) => PositionComponent {
            size: 2,
            read: |bytes| f64::from(u16::from_le_bytes(le_bytes(bytes))) / 65535.0,
        },
        (DataType::U32, _) => {
            return Err(
                "POSITION uses UNSIGNED_INT, which glTF does not permit for vertex positions"
                    .to_owned(),
            );
        }
    };
    Ok(component)
}

fn tex_coords_read_by_the_crate(accessor: &gltf::Accessor<'_>) -> bool {
    match accessor.data_type() {
        DataType::F32 => true,
        DataType::U8 | DataType::U16 => accessor.normalized(),
        DataType::I8 | DataType::I16 | DataType::U32 => false,
    }
}

fn quantized_without_scale(accessor: &gltf::Accessor<'_>) -> bool {
    !accessor.normalized()
        && matches!(
            accessor.data_type(),
            DataType::U8 | DataType::U16 | DataType::I8 | DataType::I16
        )
}

fn decode_tex_coords(
    accessor: &gltf::Accessor<'_>,
    buffers: &[Vec<u8>],
    vertices: usize,
) -> Option<Vec<[f64; 2]>> {
    if accessor.count() != vertices
        || accessor.dimensions() != Dimensions::Vec2
        || accessor.sparse().is_some()
    {
        return None;
    }
    let component = position_component(accessor.data_type(), accessor.normalized()).ok()?;
    let view = accessor.view()?;
    let data = buffer_view_bytes(&view, buffers).ok()?;
    let element = component.size * 2;
    let stride = view.stride().unwrap_or(element);
    let mut uvs = Vec::with_capacity(accessor.count());
    for index in 0..accessor.count() {
        let base = stride
            .checked_mul(index)
            .and_then(|span| span.checked_add(accessor.offset()))?;
        let mut uv = [0.0_f64; 2];
        for (axis, value) in uv.iter_mut().enumerate() {
            let at = base.checked_add(component.size * axis)?;
            let end = at.checked_add(component.size)?;
            *value = (component.read)(data.get(at..end)?);
        }
        uvs.push(uv);
    }
    Some(uvs)
}

fn validate_readable_accessor(
    accessor: &gltf::Accessor<'_>,
    buffers: &[Vec<u8>],
    label: &str,
) -> Result<(), String> {
    let count = accessor.count();
    if count == 0 {
        return Err(format!("{label} accessor is empty"));
    }
    let element = accessor.size();
    if element == 0 {
        return Err(format!("{label} accessor has a zero-sized element"));
    }
    match accessor.view() {
        Some(view) => {
            validate_strided_range(&view, buffers, accessor.offset(), element, count, label)?
        }
        None if accessor.sparse().is_none() => {
            return Err(format!("{label} accessor has no bufferView"));
        }
        None => {}
    }
    if let Some(sparse) = accessor.sparse() {
        let overrides = sparse.count();
        if overrides == 0 {
            return Err(format!("{label} accessor declares no sparse overrides"));
        }
        let indices = sparse.indices();
        validate_strided_range(
            &indices.view(),
            buffers,
            indices.offset(),
            indices.index_type().size(),
            overrides,
            &format!("{label} sparse indices"),
        )?;
        let values = sparse.values();
        validate_strided_range(
            &values.view(),
            buffers,
            values.offset(),
            element,
            overrides,
            &format!("{label} sparse values"),
        )?;
    }
    Ok(())
}

fn validate_strided_range(
    view: &buffer::View<'_>,
    buffers: &[Vec<u8>],
    offset: usize,
    element: usize,
    count: usize,
    label: &str,
) -> Result<(), String> {
    let data = buffer_view_bytes(view, buffers)?;
    let stride = view.stride().unwrap_or(element);
    if stride < element {
        return Err(format!(
            "{label} accessor has a {stride} byte stride for {element} byte elements"
        ));
    }
    let end = stride
        .checked_mul(count - 1)
        .and_then(|span| span.checked_add(element))
        .and_then(|span| span.checked_add(offset))
        .ok_or_else(|| format!("{label} accessor range overflows"))?;
    if end > data.len() {
        return Err(format!(
            "{label} accessor reads {end} bytes from a {} byte bufferView",
            data.len()
        ));
    }
    Ok(())
}

fn buffer_view_bytes<'a>(
    view: &buffer::View<'_>,
    buffers: &'a [Vec<u8>],
) -> Result<&'a [u8], String> {
    let buffer = buffers
        .get(view.buffer().index())
        .ok_or_else(|| format!("glTF bufferView {} names a missing buffer", view.index()))?;
    let end = view
        .offset()
        .checked_add(view.length())
        .ok_or_else(|| format!("glTF bufferView {} range overflows", view.index()))?;
    buffer
        .get(view.offset()..end)
        .ok_or_else(|| format!("glTF bufferView {} lies outside its buffer", view.index()))
}

fn le_bytes<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut value = [0_u8; N];
    let length = bytes.len().min(N);
    value[..length].copy_from_slice(&bytes[..length]);
    value
}

fn write_material_library(
    gltf: &gltf::Gltf,
    buffers: &[Vec<u8>],
    source_root: &Path,
    destination_root: &Path,
    budget: &mut PayloadBudget,
) {
    let mut dropped = Vec::new();
    if let Err(error) = fs::create_dir_all(destination_root.join("textures")) {
        dropped.push(format!("no textures folder could be created: {error}"));
    }
    let mut mtl = String::from("# Vkit native glTF material bridge\n");
    let mut extracted_images = HashMap::<usize, PathBuf>::new();
    for material in gltf.materials() {
        write_material(
            &mut mtl,
            &material,
            &mut MaterialSink {
                gltf,
                buffers,
                source_root,
                destination_root,
                extracted_images: &mut extracted_images,
                budget,
                dropped: &mut dropped,
            },
        );
    }
    writeln!(mtl, "newmtl {}", material_name(None, None)).unwrap();
    writeln!(mtl, "Kd 1 1 1\nd 1").unwrap();
    if let Err(error) = fs::write(destination_root.join("vkit-import.mtl"), mtl) {
        dropped.push(format!(
            "the material library could not be written: {error}"
        ));
    }
    if dropped.is_empty() {
        return;
    }
    let _ = crate::diagnostics::record(
        crate::diagnostics::Severity::Warning,
        "importers",
        "gltf_appearance_dropped",
        &format!("dropped={}; {}", dropped.len(), dropped.join("; ")),
    );
}

struct MaterialSink<'a> {
    gltf: &'a gltf::Gltf,
    buffers: &'a [Vec<u8>],
    source_root: &'a Path,
    destination_root: &'a Path,
    extracted_images: &'a mut HashMap<usize, PathBuf>,
    budget: &'a mut PayloadBudget,
    dropped: &'a mut Vec<String>,
}

fn write_material(mtl: &mut String, material: &gltf::Material<'_>, sink: &mut MaterialSink<'_>) {
    let pbr = material.pbr_metallic_roughness();
    let [red, green, blue, alpha] = pbr.base_color_factor();
    writeln!(
        mtl,
        "newmtl {}",
        material_name(material.index(), material.name())
    )
    .unwrap();
    writeln!(mtl, "Kd {red} {green} {blue}\nd {alpha}").unwrap();
    let Some(info) = pbr.base_color_texture() else {
        return;
    };
    let texture = info.texture();
    let Some(image) = texture.source() else {
        sink.dropped.push(format!(
            "texture {} names no image this build can reach; its only source is a compressed \
             texture extension with no PNG or JPEG fallback",
            texture.index()
        ));
        return;
    };
    let index = image.index();
    let path = match sink.extracted_images.get(&index) {
        Some(path) => path.clone(),
        None => match extract_image(&image, sink) {
            Ok(relative) => {
                sink.extracted_images.insert(index, relative.clone());
                relative
            }
            Err(reason) => {
                sink.dropped.push(format!("image {index}: {reason}"));
                return;
            }
        },
    };
    writeln!(mtl, "map_Kd {}", path.to_string_lossy().replace('\\', "/")).unwrap();
}

fn extract_image(image: &gltf::Image<'_>, sink: &mut MaterialSink<'_>) -> Result<PathBuf, String> {
    if !image_source_is_readable(sink.gltf, image.index()) {
        return Err(
            "the image declares neither a bufferView with a MIME type nor a URI, so the crate \
             would abort the process reading it"
                .to_owned(),
        );
    }
    let (bytes, extension) = image_bytes(image, sink.buffers, sink.source_root, sink.budget)?;
    let relative = PathBuf::from(format!("textures/image_{}.{}", image.index(), extension));
    fs::write(sink.destination_root.join(&relative), bytes)
        .map_err(|error| format!("could not be extracted: {error}"))?;
    Ok(relative)
}

fn image_source_is_readable(gltf: &gltf::Gltf, index: usize) -> bool {
    let Some(image) = gltf.as_json().images.get(index) else {
        return false;
    };
    if image.buffer_view.is_some() {
        return image.mime_type.is_some();
    }
    image.uri.is_some()
}

fn image_bytes(
    image: &gltf::Image<'_>,
    buffers: &[Vec<u8>],
    source_root: &Path,
    budget: &mut PayloadBudget,
) -> Result<(Vec<u8>, &'static str), String> {
    match image.source() {
        image::Source::View { view, mime_type } => {
            let extension = image_extension(mime_type, "")?;
            let bytes = buffer_view_bytes(&view, buffers)?;
            budget.charge(bytes.len() as u64, "image")?;
            Ok((bytes.to_vec(), extension))
        }
        image::Source::Uri { uri, mime_type } => {
            let (bytes, uri_mime) = gltf_container::read_uri(source_root, uri, "image", budget)?;
            let mime = mime_type.or(uri_mime.as_deref()).unwrap_or_default();
            let extension = image_extension(mime, uri)?;
            Ok((bytes, extension))
        }
    }
}

fn image_extension(mime: &str, uri: &str) -> Result<&'static str, String> {
    if !mime.is_empty() {
        return match mime.to_ascii_lowercase().as_str() {
            "image/png" => Ok("png"),
            "image/jpeg" | "image/jpg" => Ok("jpg"),
            "image/webp" => Ok("webp"),
            _ => Err(format!(
                "is a {mime:?} image, which this build has no decoder for; PNG, JPEG and WebP read"
            )),
        };
    }
    let lower = uri.to_ascii_lowercase();
    [
        (".png", "png"),
        (".jpeg", "jpg"),
        (".jpg", "jpg"),
        (".webp", "webp"),
    ]
    .into_iter()
    .find(|(suffix, _)| lower.ends_with(suffix))
    .map(|(_, extension)| extension)
    .ok_or_else(|| {
        "declares no MIME type and its URI ends in no extension this build decodes".to_owned()
    })
}

fn material_name(index: Option<usize>, name: Option<&str>) -> String {
    match index {
        Some(index) => safe_label(name.unwrap_or("Material"), &format!("GLB_Material_{index}")),
        None => "GLB_Default_Material".to_owned(),
    }
}

fn safe_label(name: &str, fallback: &str) -> String {
    let sanitized = name
        .chars()
        .map(|character| {
            if character.is_control() || matches!(character, '#' | '\r' | '\n') {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let sanitized = sanitized.trim();
    if sanitized.is_empty() {
        fallback.to_owned()
    } else {
        format!("{fallback}_{sanitized}")
    }
}

fn identity() -> Matrix {
    [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn multiply(left: Matrix, right: Matrix) -> Matrix {
    let mut result = [[0.0; 4]; 4];
    for column in 0..4 {
        for row in 0..4 {
            result[column][row] = (0..4)
                .map(|inner| left[inner][row] * right[column][inner])
                .sum();
        }
    }
    result
}

fn transform_point(matrix: Matrix, point: [f64; 3]) -> [f64; 3] {
    let [x, y, z] = point;
    let transformed = [
        matrix[0][0] * x + matrix[1][0] * y + matrix[2][0] * z + matrix[3][0],
        matrix[0][1] * x + matrix[1][1] * y + matrix[2][1] * z + matrix[3][1],
        matrix[0][2] * x + matrix[1][2] * y + matrix[2][2] * z + matrix[3][2],
        matrix[0][3] * x + matrix[1][3] * y + matrix[2][3] * z + matrix[3][3],
    ];
    if transformed[3] == 0.0 || transformed[3] == 1.0 {
        [transformed[0], transformed[1], transformed[2]]
    } else {
        let inverse_w = transformed[3].recip();
        [
            transformed[0] * inverse_w,
            transformed[1] * inverse_w,
            transformed[2] * inverse_w,
        ]
    }
}

fn linear_determinant(matrix: Matrix) -> f64 {
    let a = matrix[0][0];
    let b = matrix[1][0];
    let c = matrix[2][0];
    let d = matrix[0][1];
    let e = matrix[1][1];
    let f = matrix[2][1];
    let g = matrix[0][2];
    let h = matrix[1][2];
    let i = matrix[2][2];
    a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_child_transform_composition_and_winding_are_stable() {
        let parent = [
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [4.0, 0.0, 0.0, 1.0],
        ];
        let child = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [2.0, 0.0, 0.0, 1.0],
        ];
        let world = multiply(parent, child);
        assert_eq!(transform_point(world, [1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
        assert!(linear_determinant(world) < 0.0);
    }

    #[test]
    fn position_welder_is_exact_and_counts_duplicates() {
        let mut welder = PositionWelder::default();
        let first = welder.canonical_id([1.0, 2.0, 3.0]);
        let duplicate = welder.canonical_id([1.0, 2.0, 3.0]);
        let nudged = welder.canonical_id([1.0, 2.0, 3.0 + f64::EPSILON * 4.0]);
        let negative_zero = welder.canonical_id([-0.0, 0.0, 0.0]);
        let positive_zero = welder.canonical_id([0.0, 0.0, 0.0]);
        assert_eq!(first, duplicate);
        assert_ne!(first, nudged);
        assert_ne!(
            negative_zero, positive_zero,
            "the weld is exact by design; only bit-identical positions merge"
        );
        assert_eq!(welder.welded_duplicates, 1);
    }

    #[test]
    fn quantized_position_components_map_to_their_declared_range() {
        let normalized_short =
            position_component(DataType::I16, true).expect("normalized short positions");
        assert_eq!(normalized_short.size, 2);
        assert_eq!((normalized_short.read)(&32767_i16.to_le_bytes()), 1.0);
        assert_eq!((normalized_short.read)(&(-32768_i16).to_le_bytes()), -1.0);

        let raw_short = position_component(DataType::I16, false).expect("raw short positions");
        assert_eq!((raw_short.read)(&(-4096_i16).to_le_bytes()), -4096.0);

        let normalized_byte =
            position_component(DataType::U8, true).expect("normalized byte positions");
        assert_eq!((normalized_byte.read)(&[255]), 1.0);

        assert!(position_component(DataType::U32, false).is_err());
    }

    fn upward_normal(corners: [[f64; 3]; 4], triangle: &[u32]) -> f64 {
        let pick = |at: usize| corners[triangle[at] as usize];
        let (a, b, c) = (pick(0), pick(1), pick(2));
        let u = [b[0] - a[0], b[1] - a[1]];
        let v = [c[0] - a[0], c[1] - a[1]];
        u[0] * v[1] - u[1] * v[0]
    }

    #[test]
    fn strip_and_fan_expansion_wind_every_triangle_the_same_way() {
        let quad = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ];
        let strip = Topology::Strip.expand(vec![0, 1, 2, 3]).expect("strip");
        assert_eq!(strip.len(), 6, "four strip vertices are two triangles");
        for triangle in strip.chunks_exact(3) {
            assert!(
                upward_normal(quad, triangle) > 0.0,
                "the odd step of a strip swaps its first two corners; without that every other \
                 face imports inside out: {triangle:?}"
            );
        }
        let fan = Topology::Fan.expand(vec![0, 1, 3, 2]).expect("fan");
        assert_eq!(fan.len(), 6);
        for triangle in fan.chunks_exact(3) {
            assert!(upward_normal(quad, triangle) > 0.0, "{triangle:?}");
        }
    }

    #[test]
    fn a_surfaceless_primitive_mode_is_skipped_rather_than_refused() {
        assert!(Topology::of(Mode::Points).is_none());
        assert!(Topology::of(Mode::Lines).is_none());
        assert!(Topology::of(Mode::LineLoop).is_none());
        assert!(Topology::of(Mode::LineStrip).is_none());
        assert!(Topology::of(Mode::TriangleStrip).is_some());
        assert!(Topology::of(Mode::TriangleFan).is_some());
    }

    #[test]
    fn an_index_tail_is_dropped_rather_than_refusing_the_primitive() {
        let ragged = Topology::Triangles
            .expand(vec![0, 1, 2, 3, 4])
            .expect("a ragged index list still carries its whole triangles");
        assert_eq!(ragged.chunks_exact(3).count(), 1);
    }

    #[test]
    fn a_short_component_slice_reads_as_zero_rather_than_aborting() {
        let float = position_component(DataType::F32, false).expect("float positions");
        assert_eq!((float.read)(&[]), 0.0);
        assert_eq!((float.read)(&[0, 0]), 0.0);
    }
}
