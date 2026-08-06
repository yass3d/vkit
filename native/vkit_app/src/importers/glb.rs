use std::{
    collections::{HashMap, hash_map::Entry},
    fmt::Write as _,
    fs,
    path::{Component, Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use gltf::{buffer, image, mesh::Mode};

use super::simplify::{AttributeMesh, AttributeMeshBuilder, Corner, FaceLabels};

type Matrix = [[f64; 4]; 4];
const MAX_EXTERNAL_PAYLOAD_BYTES: u64 = 1_500_000_000;

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
    let gltf = gltf::Gltf::open(path).map_err(|error| format!("invalid GLB: {error}"))?;
    reject_unsupported_features(&gltf)?;
    let buffers = load_buffers(path, &gltf)?;
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
    write_material_library(
        &gltf,
        &buffers,
        path.parent().unwrap_or_else(|| Path::new(".")),
        appearance_root,
    )?;

    let scene = gltf
        .default_scene()
        .or_else(|| gltf.scenes().next())
        .ok_or_else(|| "GLB contains no scene".to_owned())?;
    let mut welder = PositionWelder::default();
    let mut visited_nodes = 0_usize;
    let node_total = gltf.nodes().len().max(1);
    for node in scene.nodes() {
        visit_node(
            node,
            identity(),
            0,
            &mut SceneWalk {
                buffers: &buffers,
                welder: &mut welder,
                builder: &mut builder,
                visited_nodes: &mut visited_nodes,
            },
            &mut |visited| {
                progress(0.16 + 0.74 * (visited as f32 / node_total as f32).min(1.0));
            },
        )?;
    }
    let mut mesh = builder.finish()?;
    mesh.welded_coincident_vertices = welder.welded_duplicates;
    progress(1.0);
    Ok(mesh)
}

fn reject_unsupported_features(gltf: &gltf::Gltf) -> Result<(), String> {
    if gltf.animations().next().is_some() {
        return Err("animated GLB input is unsupported; export a static mesh".to_owned());
    }
    if gltf.skins().next().is_some() {
        return Err(
            "skinned GLB input is unsupported; apply the armature before export".to_owned(),
        );
    }
    for extension in gltf.extensions_used() {
        if matches!(
            extension,
            "KHR_draco_mesh_compression" | "EXT_meshopt_compression" | "KHR_texture_transform"
        ) {
            return Err(format!(
                "GLB extension {extension} is unsupported; export uncompressed geometry with baked UVs"
            ));
        }
    }
    let supported_required = ["KHR_materials_unlit"];
    if let Some(extension) = gltf
        .extensions_required()
        .find(|extension| !supported_required.contains(extension))
    {
        return Err(format!(
            "required GLB extension {extension} is unsupported by the native importer"
        ));
    }
    Ok(())
}

fn load_buffers(path: &Path, gltf: &gltf::Gltf) -> Result<Vec<Vec<u8>>, String> {
    let root = path.parent().unwrap_or_else(|| Path::new("."));
    let mut buffers = Vec::with_capacity(gltf.buffers().len());
    let mut total_bytes = 0_u64;
    for buffer in gltf.buffers() {
        let data = match buffer.source() {
            buffer::Source::Bin => gltf
                .blob
                .as_ref()
                .ok_or_else(|| "GLB declares a binary buffer but has no BIN payload".to_owned())?
                .clone(),
            buffer::Source::Uri(uri) => read_uri(root, uri, "buffer")?.0,
        };
        if data.len() < buffer.length() {
            return Err(format!(
                "GLB buffer {} has {} bytes but declares {}",
                buffer.index(),
                data.len(),
                buffer.length()
            ));
        }
        total_bytes = total_bytes
            .checked_add(data.len() as u64)
            .ok_or_else(|| "GLB buffer byte count overflow".to_owned())?;
        if total_bytes > MAX_EXTERNAL_PAYLOAD_BYTES {
            return Err(format!(
                "GLB buffers total {total_bytes} bytes; the bounded native importer limit is {MAX_EXTERNAL_PAYLOAD_BYTES} bytes"
            ));
        }
        buffers.push(data);
    }
    Ok(buffers)
}

struct SceneWalk<'a> {
    buffers: &'a [Vec<u8>],
    welder: &'a mut PositionWelder,
    builder: &'a mut AttributeMeshBuilder,
    visited_nodes: &'a mut usize,
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
    } = &mut *walk;
    let (welder, builder, visited_nodes) = (&mut **welder, &mut **builder, &mut **visited_nodes);
    let buffers: &[Vec<u8>] = buffers;
    if depth > 256 {
        return Err("GLB scene hierarchy exceeds 256 levels".to_owned());
    }
    if node.skin().is_some() {
        return Err(format!(
            "GLB node {} references a skin; apply the armature before export",
            node.index()
        ));
    }
    if node.weights().is_some() {
        return Err(format!(
            "GLB node {} has morph weights; apply morph targets before export",
            node.index()
        ));
    }
    let local = node
        .transform()
        .matrix()
        .map(|column| column.map(f64::from));
    let world = multiply(parent, local);
    if !world.iter().flatten().all(|value| value.is_finite()) {
        return Err(format!(
            "GLB node {} has a non-finite transform",
            node.index()
        ));
    }
    if let Some(mesh) = node.mesh() {
        let group = Some(safe_label(
            node.name().or(mesh.name()).unwrap_or("Mesh"),
            &format!("GLB_Node_{}", node.index()),
        ));
        for primitive in mesh.primitives() {
            if primitive.mode() != Mode::Triangles {
                return Err(format!(
                    "GLB mesh {} primitive {} uses {:?}; only triangle primitives are supported",
                    mesh.index(),
                    primitive.index(),
                    primitive.mode()
                ));
            }
            if primitive.morph_targets().next().is_some() {
                return Err(format!(
                    "GLB mesh {} primitive {} has morph targets; apply them before export",
                    mesh.index(),
                    primitive.index()
                ));
            }
            let reader = primitive.reader(|buffer| buffers.get(buffer.index()).map(Vec::as_slice));
            let positions = reader
                .read_positions()
                .ok_or_else(|| format!("GLB mesh {} has no POSITION stream", mesh.index()))?
                .map(|position| transform_point(world, position.map(f64::from)))
                .collect::<Vec<_>>();
            if positions.is_empty() || !positions.iter().flatten().all(|value| value.is_finite()) {
                return Err(format!(
                    "GLB mesh {} contains an empty or non-finite POSITION stream",
                    mesh.index()
                ));
            }
            let material = primitive.material();
            let pbr = material.pbr_metallic_roughness();
            if pbr
                .base_color_texture()
                .is_some_and(|texture| texture.tex_coord() != 0)
            {
                return Err(format!(
                    "GLB material {:?} uses a nonzero base-color UV set, which is unsupported",
                    material.index()
                ));
            }
            let uvs = reader.read_tex_coords(0).map(|stream| {
                stream
                    .into_f32()
                    .map(|uv| uv.map(f64::from))
                    .collect::<Vec<_>>()
            });
            if uvs.as_ref().is_some_and(|uvs| uvs.len() != positions.len()) {
                return Err(format!(
                    "GLB mesh {} has a mismatched TEXCOORD_0 stream",
                    mesh.index()
                ));
            }
            if pbr.base_color_texture().is_some() && uvs.is_none() {
                return Err(format!(
                    "GLB material {:?} has a base-color texture but the mesh has no TEXCOORD_0 stream",
                    material.index()
                ));
            }
            let indices = reader
                .read_indices()
                .map(|indices| indices.into_u32().collect::<Vec<_>>())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());
            if indices.len() % 3 != 0 {
                return Err(format!(
                    "GLB mesh {} triangle index count is not divisible by three",
                    mesh.index()
                ));
            }
            let labels = FaceLabels {
                group: group.clone(),
                material: Some(material_name(material.index(), material.name())),
            };

            let canonical_ids = positions
                .iter()
                .map(|&position| welder.canonical_id(position))
                .collect::<Vec<_>>();
            let reverse_winding = linear_determinant(world) < 0.0;
            for indices in indices.chunks_exact(3) {
                let mut triangle = [indices[0], indices[1], indices[2]];
                if reverse_winding {
                    triangle.swap(1, 2);
                }
                let corners = triangle.map(|index| {
                    let index_usize = index as usize;
                    positions.get(index_usize).map(|&position| Corner {
                        source_vertex: canonical_ids[index_usize],
                        position,
                        uv: uvs.as_ref().map(|uvs| uvs[index_usize]),
                    })
                });
                let [Some(a), Some(b), Some(c)] = corners else {
                    return Err(format!(
                        "GLB mesh {} has an out-of-range index",
                        mesh.index()
                    ));
                };
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
            },
            progress,
        )?;
    }
    Ok(())
}

fn write_material_library(
    gltf: &gltf::Gltf,
    buffers: &[Vec<u8>],
    source_root: &Path,
    destination_root: &Path,
) -> Result<(), String> {
    fs::create_dir_all(destination_root.join("textures"))
        .map_err(|error| format!("failed to create GLB appearance workspace: {error}"))?;
    let mut mtl = String::from("# Vkit native GLB material bridge\n");
    let mut extracted_images = HashMap::<usize, PathBuf>::new();
    for material in gltf.materials() {
        write_material(
            &mut mtl,
            &material,
            buffers,
            source_root,
            destination_root,
            &mut extracted_images,
        )?;
    }
    writeln!(mtl, "newmtl {}", material_name(None, None)).unwrap();
    writeln!(mtl, "Kd 1 1 1\nd 1").unwrap();
    fs::write(destination_root.join("vkit-import.mtl"), mtl)
        .map_err(|error| format!("failed to write native GLB material library: {error}"))
}

fn write_material(
    mtl: &mut String,
    material: &gltf::Material<'_>,
    buffers: &[Vec<u8>],
    source_root: &Path,
    destination_root: &Path,
    extracted_images: &mut HashMap<usize, PathBuf>,
) -> Result<(), String> {
    let pbr = material.pbr_metallic_roughness();
    let [red, green, blue, alpha] = pbr.base_color_factor();
    writeln!(
        mtl,
        "newmtl {}",
        material_name(material.index(), material.name())
    )
    .unwrap();
    writeln!(mtl, "Kd {red} {green} {blue}\nd {alpha}").unwrap();
    if let Some(texture) = pbr.base_color_texture() {
        if texture.tex_coord() != 0 {
            return Err("GLB base-color textures must use TEXCOORD_0".to_owned());
        }
        let image = texture.texture().source();
        let path = if let Some(path) = extracted_images.get(&image.index()) {
            path.clone()
        } else {
            let (bytes, extension) = image_bytes(&image, buffers, source_root)?;
            let relative = PathBuf::from(format!("textures/image_{}.{}", image.index(), extension));
            fs::write(destination_root.join(&relative), bytes).map_err(|error| {
                format!("failed to extract GLB image {}: {error}", image.index())
            })?;
            extracted_images.insert(image.index(), relative.clone());
            relative
        };
        writeln!(mtl, "map_Kd {}", path.to_string_lossy().replace('\\', "/")).unwrap();
    }
    Ok(())
}

fn image_bytes(
    image: &gltf::Image<'_>,
    buffers: &[Vec<u8>],
    source_root: &Path,
) -> Result<(Vec<u8>, &'static str), String> {
    match image.source() {
        image::Source::View { view, mime_type } => {
            let extension = image_extension(mime_type)?;
            let buffer = buffers.get(view.buffer().index()).ok_or_else(|| {
                format!("GLB image {} references a missing buffer", image.index())
            })?;
            let end = view
                .offset()
                .checked_add(view.length())
                .ok_or_else(|| "GLB image buffer range overflow".to_owned())?;
            let bytes = buffer.get(view.offset()..end).ok_or_else(|| {
                format!("GLB image {} has an invalid buffer range", image.index())
            })?;
            Ok((bytes.to_vec(), extension))
        }
        image::Source::Uri { uri, mime_type } => {
            let (bytes, uri_mime) = read_uri(source_root, uri, "image")?;
            let inferred_mime = if uri.to_ascii_lowercase().ends_with(".png") {
                Some("image/png")
            } else if uri.to_ascii_lowercase().ends_with(".jpg")
                || uri.to_ascii_lowercase().ends_with(".jpeg")
            {
                Some("image/jpeg")
            } else {
                None
            };
            let mime = mime_type
                .or(uri_mime.as_deref())
                .or(inferred_mime)
                .ok_or_else(|| {
                    format!(
                        "GLB image {} has no supported MIME type or PNG/JPEG extension",
                        image.index()
                    )
                })?;
            let extension = image_extension(mime)?;
            Ok((bytes, extension))
        }
    }
}

fn image_extension(mime: &str) -> Result<&'static str, String> {
    match mime.to_ascii_lowercase().as_str() {
        "image/png" => Ok("png"),
        "image/jpeg" | "image/jpg" => Ok("jpg"),
        _ => Err(format!(
            "GLB image MIME type {mime:?} is unsupported; use PNG or JPEG"
        )),
    }
}

fn read_uri(root: &Path, uri: &str, label: &str) -> Result<(Vec<u8>, Option<String>), String> {
    if let Some(data) = uri.strip_prefix("data:") {
        let (metadata, encoded) = data
            .split_once(',')
            .ok_or_else(|| format!("GLB {label} data URI is malformed"))?;
        let mime = metadata
            .split(';')
            .next()
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if !metadata.split(';').any(|value| value == "base64") {
            return Err(format!("GLB {label} data URI must use base64 encoding"));
        }
        return BASE64
            .decode(encoded)
            .map(|bytes| (bytes, mime))
            .map_err(|error| format!("GLB {label} data URI is invalid: {error}"));
    }
    if uri.contains('%') || uri.contains(':') || uri.contains('\\') {
        return Err(format!("GLB {label} URI {uri:?} is not a safe local path"));
    }
    let relative = Path::new(uri);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "GLB {label} URI {uri:?} is not a safe relative path"
        ));
    }
    let source = root.join(relative);
    let bytes = source
        .metadata()
        .map_err(|error| format!("failed to inspect GLB {label} URI {uri:?}: {error}"))?
        .len();
    if bytes > MAX_EXTERNAL_PAYLOAD_BYTES {
        return Err(format!(
            "GLB {label} URI {uri:?} is {bytes} bytes; the bounded importer limit is {MAX_EXTERNAL_PAYLOAD_BYTES} bytes"
        ));
    }
    fs::read(source)
        .map(|bytes| (bytes, None))
        .map_err(|error| format!("failed to read GLB {label} URI {uri:?}: {error}"))
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
    fn unsafe_external_uri_is_rejected() {
        let error = read_uri(Path::new("."), "../outside.bin", "buffer").unwrap_err();
        assert!(error.contains("safe relative path"));
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
}
