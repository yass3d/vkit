use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use base64::{
    Engine as _, alphabet,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};
use serde_json::{Map, Value};
use vkit_core::formats::{MeshoptFilter, MeshoptMode, decode_meshopt_buffer_view};

const BASE64_FORGIVING: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

const BASE64_URL_FORGIVING: GeneralPurpose = GeneralPurpose::new(
    &alphabet::URL_SAFE,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

const MAX_EXTERNAL_PAYLOAD_BYTES: u64 = 1_500_000_000;

const MESHOPT_EXTENSION: &str = "EXT_meshopt_compression";
const DRACO_EXTENSION: &str = "KHR_draco_mesh_compression";
const INSTANCING_EXTENSION: &str = "EXT_mesh_gpu_instancing";

const MAX_EXPANDED_INSTANCES: usize = 100_000;

const HANDLED_REQUIRED_EXTENSIONS: [&str; 5] = [
    "KHR_mesh_quantization",
    "KHR_texture_transform",
    MESHOPT_EXTENSION,
    DRACO_EXTENSION,
    INSTANCING_EXTENSION,
];

const IGNORABLE_REQUIRED_EXTENSIONS: [&str; 14] = [
    "KHR_texture_basisu",
    "EXT_texture_webp",
    "EXT_texture_avif",
    "MSFT_texture_dds",
    "MSFT_packing_normalRoughnessMetallic",
    "MSFT_packing_occlusionRoughnessMetallic",
    "ADOBE_materials_thin_transparency",
    "KHR_lights_punctual",
    "EXT_lights_image_based",
    "EXT_lights_ies",
    "KHR_animation_pointer",
    "KHR_xmp",
    "KHR_xmp_json_ld",
    "KHR_emitter_audio",
];

fn required_extension_is_readable(extension: &str) -> bool {
    HANDLED_REQUIRED_EXTENSIONS.contains(&extension)
        || IGNORABLE_REQUIRED_EXTENSIONS.contains(&extension)
        || extension.starts_with("KHR_materials_")
}

const TEXTURE_SOURCE_EXTENSIONS: [&str; 4] = [
    "EXT_texture_webp",
    "KHR_texture_basisu",
    "EXT_texture_avif",
    "MSFT_texture_dds",
];

pub(super) struct PreparedContainer {
    pub(super) gltf: gltf::Gltf,
    pub(super) buffers: Vec<Vec<u8>>,
    pub(super) budget: PayloadBudget,
}

pub(super) struct PayloadBudget {
    remaining: u64,
}

impl PayloadBudget {
    fn new() -> Self {
        Self {
            remaining: MAX_EXTERNAL_PAYLOAD_BYTES,
        }
    }

    pub(super) fn charge(&mut self, bytes: u64, what: &str) -> Result<(), String> {
        if bytes > self.remaining {
            return Err(format!(
                "glTF {what} needs {bytes} bytes but only {} of the {MAX_EXTERNAL_PAYLOAD_BYTES} byte importer budget is left",
                self.remaining
            ));
        }
        self.remaining -= bytes;
        Ok(())
    }
}

pub(super) fn prepare(path: &Path) -> Result<PreparedContainer, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("failed to read the glTF container: {error}"))?;
    let (json_bytes, blob) = split_container(&bytes)?;
    let mut root = serde_json::from_slice::<Value>(json_bytes)
        .map_err(|error| format!("glTF JSON is invalid: {error}"))?;
    let mut budget = PayloadBudget::new();
    let buffers = {
        let object = root
            .as_object_mut()
            .ok_or_else(|| "glTF JSON root is not an object".to_owned())?;
        reject_unsupported_features(object)?;
        drop_animations(object);
        hoist_texture_sources(object);
        validate_index_references(object)?;
        let mut buffers = resolve_buffers(object, path, blob, &mut budget)?;
        decompress_meshopt_buffer_views(object, &mut buffers, &mut budget)?;
        decode_draco_primitives(object, &mut buffers, &mut budget)?;
        expand_gpu_instancing(object, &buffers);
        normalize_for_the_crate(object);
        object.remove("extensionsRequired");
        buffers
    };
    let json = serde_json::to_vec(&root)
        .map_err(|error| format!("glTF JSON could not be rewritten: {error}"))?;
    let gltf = gltf::Gltf::from_slice(&json)
        .map_err(|error| format!("glTF document is invalid: {error}"))?;
    Ok(PreparedContainer {
        gltf,
        buffers,
        budget,
    })
}

fn split_container(bytes: &[u8]) -> Result<(&[u8], Option<&[u8]>), String> {
    let bytes = bytes
        .strip_prefix(b"\xef\xbb\xbf".as_slice())
        .unwrap_or(bytes);
    if !bytes.starts_with(b"glTF") {
        return Ok((bytes, None));
    }
    let version = read_u32(bytes, 4).ok_or_else(|| "GLB header is truncated".to_owned())?;
    if version != 2 {
        return Err(format!(
            "GLB container is version {version}; only version 2 is supported"
        ));
    }
    let declared = read_u32(bytes, 8).ok_or_else(|| "GLB header is truncated".to_owned())? as usize;
    if declared < 12 || declared > bytes.len() {
        return Err(format!(
            "GLB declares {declared} bytes but the file holds {}",
            bytes.len()
        ));
    }
    let body = bytes
        .get(12..declared)
        .ok_or_else(|| "GLB body is truncated".to_owned())?;

    let mut json = None;
    let mut bin = None;
    let mut cursor = 0_usize;
    while cursor + 8 <= body.len() {
        let length =
            read_u32(body, cursor).ok_or_else(|| "GLB chunk is truncated".to_owned())? as usize;
        let kind = body
            .get(cursor + 4..cursor + 8)
            .ok_or_else(|| "GLB chunk is truncated".to_owned())?;
        let start = cursor + 8;
        let end = start
            .checked_add(length)
            .ok_or_else(|| "GLB chunk length overflows".to_owned())?;
        let data = body
            .get(start..end)
            .ok_or_else(|| "GLB chunk runs past the end of the file".to_owned())?;
        match kind {
            b"JSON" if json.is_none() => json = Some(data),
            b"BIN\0" if bin.is_none() => bin = Some(data),
            _ => {}
        }
        cursor = end.checked_next_multiple_of(4).unwrap_or(body.len());
    }
    let json = json.ok_or_else(|| "GLB has no JSON chunk".to_owned())?;
    Ok((json, bin))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    let slice = bytes.get(at..end)?;
    Some(u32::from_le_bytes(le_bytes(slice)))
}

fn le_bytes<const N: usize>(bytes: &[u8]) -> [u8; N] {
    let mut value = [0_u8; N];
    let length = bytes.len().min(N);
    value[..length].copy_from_slice(&bytes[..length]);
    value
}

fn reject_unsupported_features(root: &Map<String, Value>) -> Result<(), String> {
    let unsupported = string_array(root, "extensionsRequired")
        .filter(|extension| !required_extension_is_readable(extension))
        .collect::<Vec<_>>();
    match unsupported.as_slice() {
        [] => Ok(()),
        [only] => Err(format!(
            "this file requires the glTF extension {only}, which the native importer cannot read; \
             re-export without it. Vkit does read gltfpack output (KHR_mesh_quantization, {MESHOPT_EXTENSION})"
        )),
        many => Err(format!(
            "this file requires the glTF extensions {}, which the native importer cannot read; \
             re-export without them. Vkit does read gltfpack output (KHR_mesh_quantization, {MESHOPT_EXTENSION})",
            many.join(", ")
        )),
    }
}

fn drop_animations(root: &mut Map<String, Value>) {
    root.remove("animations");
}

fn hoist_texture_sources(root: &mut Map<String, Value>) {
    let Some(textures) = root.get_mut("textures").and_then(Value::as_array_mut) else {
        return;
    };
    for texture in textures {
        if texture.get("source").and_then(Value::as_u64).is_some() {
            continue;
        }
        let hoisted = texture.get("extensions").and_then(|extensions| {
            TEXTURE_SOURCE_EXTENSIONS.iter().find_map(|name| {
                extensions
                    .get(name)
                    .and_then(|entry| entry.get("source"))
                    .and_then(Value::as_u64)
            })
        });
        if let (Some(source), Some(object)) = (hoisted, texture.as_object_mut()) {
            object.insert("source".to_owned(), Value::from(source));
        }
    }
}

fn validate_index_references(root: &Map<String, Value>) -> Result<(), String> {
    let bound =
        |key: &str| -> usize { root.get(key).and_then(Value::as_array).map_or(0, Vec::len) };
    let (nodes, meshes, accessors) = (bound("nodes"), bound("meshes"), bound("accessors"));
    let (views, buffers) = (bound("bufferViews"), bound("buffers"));
    let (materials, textures) = (bound("materials"), bound("textures"));
    let (images, samplers) = (bound("images"), bound("samplers"));
    let (skins, cameras, scenes) = (bound("skins"), bound("cameras"), bound("scenes"));

    check_index(root.get("scene"), scenes, "scene")?;
    for (index, scene) in entries(root, "scenes") {
        check_index_array(scene.get("nodes"), nodes, &format!("scenes[{index}].nodes"))?;
    }
    for (index, node) in entries(root, "nodes") {
        check_index(node.get("mesh"), meshes, &format!("nodes[{index}].mesh"))?;
        check_index(node.get("skin"), skins, &format!("nodes[{index}].skin"))?;
        check_index(
            node.get("camera"),
            cameras,
            &format!("nodes[{index}].camera"),
        )?;
        check_index_array(
            node.get("children"),
            nodes,
            &format!("nodes[{index}].children"),
        )?;
    }
    for (index, skin) in entries(root, "skins") {
        check_index(
            skin.get("inverseBindMatrices"),
            accessors,
            &format!("skins[{index}].inverseBindMatrices"),
        )?;
        check_index(
            skin.get("skeleton"),
            nodes,
            &format!("skins[{index}].skeleton"),
        )?;
        check_index_array(skin.get("joints"), nodes, &format!("skins[{index}].joints"))?;
    }
    for (index, mesh) in entries(root, "meshes") {
        for (at, primitive) in indexed(mesh.get("primitives")) {
            let where_ = format!("meshes[{index}].primitives[{at}]");
            check_index(
                primitive.get("indices"),
                accessors,
                &format!("{where_}.indices"),
            )?;
            check_index(
                primitive.get("material"),
                materials,
                &format!("{where_}.material"),
            )?;
            check_attribute_map(primitive.get("attributes"), accessors, &where_)?;
            for (target, attributes) in indexed(primitive.get("targets")) {
                check_attribute_map(
                    Some(attributes),
                    accessors,
                    &format!("{where_}.targets[{target}]"),
                )?;
            }
        }
    }
    for (index, accessor) in entries(root, "accessors") {
        check_index(
            accessor.get("bufferView"),
            views,
            &format!("accessors[{index}].bufferView"),
        )?;
        if let Some(sparse) = accessor.get("sparse") {
            for part in ["indices", "values"] {
                check_index(
                    sparse.get(part).and_then(|part| part.get("bufferView")),
                    views,
                    &format!("accessors[{index}].sparse.{part}.bufferView"),
                )?;
            }
        }
    }
    for (index, view) in entries(root, "bufferViews") {
        check_index(
            view.get("buffer"),
            buffers,
            &format!("bufferViews[{index}].buffer"),
        )?;
    }
    for (index, material) in entries(root, "materials") {
        for (slot, info) in material_texture_slots(material) {
            check_index(
                info.get("index"),
                textures,
                &format!("materials[{index}].{slot}.index"),
            )?;
        }
    }
    for (index, texture) in entries(root, "textures") {
        check_index(
            texture.get("source"),
            images,
            &format!("textures[{index}].source"),
        )?;
        check_index(
            texture.get("sampler"),
            samplers,
            &format!("textures[{index}].sampler"),
        )?;
    }
    for (index, image) in entries(root, "images") {
        check_index(
            image.get("bufferView"),
            views,
            &format!("images[{index}].bufferView"),
        )?;
    }
    Ok(())
}

fn material_texture_slots(material: &Value) -> Vec<(String, &Value)> {
    let mut slots = Vec::new();
    let pbr = material.get("pbrMetallicRoughness");
    for name in ["baseColorTexture", "metallicRoughnessTexture"] {
        if let Some(info) = pbr.and_then(|pbr| pbr.get(name)) {
            slots.push((format!("pbrMetallicRoughness.{name}"), info));
        }
    }
    for name in ["normalTexture", "occlusionTexture", "emissiveTexture"] {
        if let Some(info) = material.get(name) {
            slots.push((name.to_owned(), info));
        }
    }
    slots
}

fn check_index(value: Option<&Value>, bound: usize, where_: &str) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.is_null() {
        return Ok(());
    }
    let index = value
        .as_u64()
        .ok_or_else(|| format!("glTF {where_} is not a whole number"))?;
    if index >= bound as u64 {
        return Err(format!(
            "glTF {where_} names entry {index}, but the document holds {bound}; the file is \
             truncated or was edited after export"
        ));
    }
    Ok(())
}

fn check_index_array(value: Option<&Value>, bound: usize, where_: &str) -> Result<(), String> {
    for (at, entry) in indexed(value) {
        check_index(Some(entry), bound, &format!("{where_}[{at}]"))?;
    }
    Ok(())
}

fn check_attribute_map(value: Option<&Value>, bound: usize, where_: &str) -> Result<(), String> {
    let Some(attributes) = value.and_then(Value::as_object) else {
        return Ok(());
    };
    for (semantic, index) in attributes {
        check_index(
            Some(index),
            bound,
            &format!("{where_}.attributes.{semantic}"),
        )?;
    }
    Ok(())
}

fn entries<'a>(root: &'a Map<String, Value>, key: &str) -> Vec<(usize, &'a Value)> {
    indexed(root.get(key))
}

fn indexed(value: Option<&Value>) -> Vec<(usize, &Value)> {
    value
        .and_then(Value::as_array)
        .map(|entries| entries.iter().enumerate().collect())
        .unwrap_or_default()
}

struct InstancePlan {
    node: usize,
    mesh: u64,
    skin: Option<u64>,
    instances: Vec<InstanceTransform>,
}

#[derive(Default)]
struct InstanceTransform {
    translation: Option<[f64; 3]>,
    rotation: Option<[f64; 4]>,
    scale: Option<[f64; 3]>,
}

fn expand_gpu_instancing(root: &mut Map<String, Value>, buffers: &[Vec<u8>]) {
    let plans = collect_instance_plans(root, buffers);
    if plans.is_empty() {
        return;
    }
    let Some(nodes) = root.get_mut("nodes").and_then(Value::as_array_mut) else {
        return;
    };
    let mut next_index = nodes.len();
    let mut appended = Vec::new();
    for plan in plans {
        let mut children = Vec::new();
        for instance in &plan.instances {
            let mut child = Map::new();
            child.insert("mesh".to_owned(), Value::from(plan.mesh));
            if let Some(skin) = plan.skin {
                child.insert("skin".to_owned(), Value::from(skin));
            }
            if let Some(translation) = instance.translation {
                child.insert("translation".to_owned(), Value::from(translation.to_vec()));
            }
            if let Some(rotation) = instance.rotation {
                child.insert("rotation".to_owned(), Value::from(rotation.to_vec()));
            }
            if let Some(scale) = instance.scale {
                child.insert("scale".to_owned(), Value::from(scale.to_vec()));
            }
            appended.push(Value::Object(child));
            children.push(Value::from(next_index as u64));
            next_index += 1;
        }
        let Some(node) = nodes.get_mut(plan.node).and_then(Value::as_object_mut) else {
            continue;
        };
        node.remove("mesh");
        node.remove("skin");
        if let Some(extensions) = node.get_mut("extensions").and_then(Value::as_object_mut) {
            extensions.remove(INSTANCING_EXTENSION);
        }
        match node.get_mut("children").and_then(Value::as_array_mut) {
            Some(existing) => existing.extend(children),
            None => {
                node.insert("children".to_owned(), Value::Array(children));
            }
        }
    }
    nodes.extend(appended);
}

fn collect_instance_plans(root: &Map<String, Value>, buffers: &[Vec<u8>]) -> Vec<InstancePlan> {
    let mut plans = Vec::new();
    let mut spent = 0_usize;
    for (index, node) in entries(root, "nodes") {
        let Some(mesh) = node.get("mesh").and_then(Value::as_u64) else {
            continue;
        };
        let Some(attributes) = node
            .get("extensions")
            .and_then(|extensions| extensions.get(INSTANCING_EXTENSION))
            .and_then(|instancing| instancing.get("attributes"))
            .and_then(Value::as_object)
        else {
            continue;
        };
        let translation = attributes.get("TRANSLATION").and_then(Value::as_u64);
        let rotation = attributes.get("ROTATION").and_then(Value::as_u64);
        let scale = attributes.get("SCALE").and_then(Value::as_u64);
        let streams =
            [(translation, 3_usize), (rotation, 4), (scale, 3)].map(|(accessor, components)| {
                accessor.and_then(|accessor| {
                    json_accessor_floats(root, buffers, accessor as usize, components)
                })
            });
        let [translation, rotation, scale] = streams;
        let count = [
            translation.as_ref().map(Vec::len),
            rotation.as_ref().map(Vec::len),
            scale.as_ref().map(Vec::len),
        ]
        .into_iter()
        .flatten()
        .min();
        let Some(count) = count.filter(|count| *count > 0) else {
            continue;
        };
        if spent.saturating_add(count) > MAX_EXPANDED_INSTANCES {
            continue;
        }
        spent += count;
        let mut instances = Vec::with_capacity(count);
        for step in 0..count {
            instances.push(InstanceTransform {
                translation: translation
                    .as_ref()
                    .and_then(|values| values.get(step))
                    .map(|value| [value[0], value[1], value[2]]),
                rotation: rotation
                    .as_ref()
                    .and_then(|values| values.get(step))
                    .map(|value| [value[0], value[1], value[2], value[3]]),
                scale: scale
                    .as_ref()
                    .and_then(|values| values.get(step))
                    .map(|value| [value[0], value[1], value[2]]),
            });
        }
        plans.push(InstancePlan {
            node: index,
            mesh,
            skin: node.get("skin").and_then(Value::as_u64),
            instances,
        });
    }
    plans
}

fn json_accessor_floats(
    root: &Map<String, Value>,
    buffers: &[Vec<u8>],
    index: usize,
    components: usize,
) -> Option<Vec<Vec<f64>>> {
    let accessor = root.get("accessors")?.as_array()?.get(index)?;
    if accessor.get("type").and_then(Value::as_str)? != accessor_type(components)? {
        return None;
    }
    let normalized = accessor
        .get("normalized")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (size, read): (usize, fn(&[u8]) -> f64) = match (
        accessor.get("componentType").and_then(Value::as_u64)?,
        normalized,
    ) {
        (5126, _) => (4, |bytes| f64::from(f32::from_le_bytes(le_bytes(bytes)))),
        (5120, true) => (1, |bytes| {
            (f64::from(i8::from_le_bytes(le_bytes(bytes))) / 127.0).max(-1.0)
        }),
        (5121, true) => (1, |bytes| {
            f64::from(u8::from_le_bytes(le_bytes(bytes))) / 255.0
        }),
        (5122, true) => (2, |bytes| {
            (f64::from(i16::from_le_bytes(le_bytes(bytes))) / 32767.0).max(-1.0)
        }),
        (5123, true) => (2, |bytes| {
            f64::from(u16::from_le_bytes(le_bytes(bytes))) / 65535.0
        }),
        _ => return None,
    };
    let count = usize::try_from(accessor.get("count").and_then(Value::as_u64)?).ok()?;
    if count > MAX_EXPANDED_INSTANCES {
        return None;
    }
    let view = root
        .get("bufferViews")?
        .as_array()?
        .get(usize::try_from(accessor.get("bufferView").and_then(Value::as_u64)?).ok()?)?;
    let buffer = buffers.get(usize::try_from(view.get("buffer").and_then(Value::as_u64)?).ok()?)?;
    let view_offset =
        usize::try_from(view.get("byteOffset").and_then(Value::as_u64).unwrap_or(0)).ok()?;
    let view_length = usize::try_from(view.get("byteLength").and_then(Value::as_u64)?).ok()?;
    let data = buffer.get(view_offset..view_offset.checked_add(view_length)?)?;
    let element = size.checked_mul(components)?;
    let stride = usize::try_from(view.get("byteStride").and_then(Value::as_u64).unwrap_or(0))
        .ok()
        .filter(|stride| *stride >= element)
        .unwrap_or(element);
    let base = usize::try_from(
        accessor
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    )
    .ok()?;
    let mut values = Vec::with_capacity(count.min(MAX_EXPANDED_INSTANCES));
    for step in 0..count {
        let at = stride.checked_mul(step)?.checked_add(base)?;
        let mut element = Vec::with_capacity(components);
        for axis in 0..components {
            let start = at.checked_add(size.checked_mul(axis)?)?;
            let bytes = data.get(start..start.checked_add(size)?)?;
            let value = read(bytes);
            if !value.is_finite() {
                return None;
            }
            element.push(value);
        }
        values.push(element);
    }
    Some(values)
}

fn normalize_for_the_crate(root: &mut Map<String, Value>) {
    normalize_appearance_enums(root);
    if let Some(views) = root.get_mut("bufferViews").and_then(Value::as_array_mut) {
        for view in views {
            let zero_stride = view
                .get("byteStride")
                .and_then(Value::as_u64)
                .is_some_and(|stride| stride == 0);
            if zero_stride && let Some(object) = view.as_object_mut() {
                object.remove("byteStride");
            }
        }
    }
    let mut position_accessors = Vec::new();
    for (_, mesh) in entries(root, "meshes") {
        for (_, primitive) in indexed(mesh.get("primitives")) {
            if let Some(index) = primitive
                .get("attributes")
                .and_then(|attributes| attributes.get("POSITION"))
                .and_then(Value::as_u64)
            {
                position_accessors.push(index as usize);
            }
        }
    }
    let Some(accessors) = root.get_mut("accessors").and_then(Value::as_array_mut) else {
        return;
    };
    for index in position_accessors {
        let Some(accessor) = accessors.get_mut(index).and_then(Value::as_object_mut) else {
            continue;
        };
        for bound in ["min", "max"] {
            let usable = accessor
                .get(bound)
                .and_then(Value::as_array)
                .is_some_and(|values| {
                    values.len() == 3 && values.iter().all(|value| value.as_f64().is_some())
                });
            if !usable {
                accessor.insert(bound.to_owned(), Value::from(vec![0.0_f64; 3]));
            }
        }
    }
}

fn normalize_appearance_enums(root: &mut Map<String, Value>) {
    if let Some(materials) = root.get_mut("materials").and_then(Value::as_array_mut) {
        for material in materials {
            let known = material
                .get("alphaMode")
                .and_then(Value::as_str)
                .is_some_and(|mode| matches!(mode, "OPAQUE" | "MASK" | "BLEND"));
            if !known && let Some(object) = material.as_object_mut() {
                object.remove("alphaMode");
            }
        }
    }
    let Some(samplers) = root.get_mut("samplers").and_then(Value::as_array_mut) else {
        return;
    };
    for sampler in samplers {
        const MAG_FILTERS: &[u64] = &[9728, 9729];
        const MIN_FILTERS: &[u64] = &[9728, 9729, 9984, 9985, 9986, 9987];
        const WRAP_MODES: &[u64] = &[33071, 33648, 10497];
        for (field, allowed) in [
            ("magFilter", MAG_FILTERS),
            ("minFilter", MIN_FILTERS),
            ("wrapS", WRAP_MODES),
            ("wrapT", WRAP_MODES),
        ] {
            let present = sampler.get(field).is_some();
            let known = sampler
                .get(field)
                .and_then(Value::as_u64)
                .is_some_and(|value| allowed.contains(&value));
            if present
                && !known
                && let Some(object) = sampler.as_object_mut()
            {
                object.remove(field);
            }
        }
    }
}

fn string_array<'a>(root: &'a Map<String, Value>, key: &str) -> impl Iterator<Item = &'a str> + 'a {
    root.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
}

fn resolve_buffers(
    root: &Map<String, Value>,
    path: &Path,
    blob: Option<&[u8]>,
    budget: &mut PayloadBudget,
) -> Result<Vec<Vec<u8>>, String> {
    let source_root = path.parent().unwrap_or_else(|| Path::new("."));
    let Some(entries) = root.get("buffers").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut buffers = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let declared = entry
            .get("byteLength")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let fallback = is_meshopt_fallback(entry);
        let data = match entry.get("uri").and_then(Value::as_str) {
            Some(uri) => read_uri(source_root, uri, "buffer", budget)?.0,
            None if fallback => Vec::new(),
            None if index == 0 => {
                let blob = blob.ok_or_else(|| {
                    "glTF declares a binary buffer but the container has no BIN payload".to_owned()
                })?;
                budget.charge(blob.len() as u64, "BIN payload")?;
                blob.to_vec()
            }
            None => {
                return Err(format!(
                    "glTF buffer {index} has no URI and is not the container's BIN payload"
                ));
            }
        };
        let data = pad_to_four(data);
        if !fallback && (data.len() as u64) < declared {
            return Err(format!(
                "glTF buffer {index} has {} bytes but declares {declared}",
                data.len()
            ));
        }
        buffers.push(data);
    }
    Ok(buffers)
}

fn pad_to_four(mut data: Vec<u8>) -> Vec<u8> {
    while !data.len().is_multiple_of(4) {
        data.push(0);
    }
    data
}

fn is_meshopt_fallback(entry: &Value) -> bool {
    entry
        .get("extensions")
        .and_then(|extensions| extensions.get(MESHOPT_EXTENSION))
        .and_then(|extension| extension.get("fallback"))
        .and_then(Value::as_bool)
        .unwrap_or_default()
}

struct MeshoptRequest {
    buffer: usize,
    offset: usize,
    length: usize,
    stride: usize,
    count: usize,
    mode: MeshoptMode,
    filter: MeshoptFilter,
}

fn decompress_meshopt_buffer_views(
    root: &mut Map<String, Value>,
    buffers: &mut Vec<Vec<u8>>,
    budget: &mut PayloadBudget,
) -> Result<(), String> {
    let decoded_buffer = buffers.len();
    let mut decoded = Vec::<u8>::new();
    let Some(views) = root.get_mut("bufferViews").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for (index, view) in views.iter_mut().enumerate() {
        let Some(request) = meshopt_request(view, index)? else {
            continue;
        };
        let source = buffers.get(request.buffer).ok_or_else(|| {
            format!(
                "glTF bufferView {index} is compressed into missing buffer {}",
                request.buffer
            )
        })?;
        let end = request
            .offset
            .checked_add(request.length)
            .ok_or_else(|| format!("glTF bufferView {index} compressed range overflows"))?;
        let encoded = source.get(request.offset..end).ok_or_else(|| {
            format!(
                "glTF bufferView {index} compressed range is outside buffer {}",
                request.buffer
            )
        })?;
        let bytes = decode_meshopt_buffer_view(
            encoded,
            request.mode,
            request.filter,
            request.count,
            request.stride,
        )
        .map_err(|error| format!("glTF bufferView {index} could not be decompressed: {error}"))?;
        budget.charge(bytes.len() as u64, "decompressed buffer view")?;
        while !decoded.len().is_multiple_of(4) {
            decoded.push(0);
        }
        let offset = decoded.len();
        let length = bytes.len();
        decoded.extend_from_slice(&bytes);
        rewrite_view(view, decoded_buffer, offset, length);
    }
    if decoded.is_empty() {
        return Ok(());
    }
    let length = decoded.len();
    buffers.push(decoded);
    let mut entry = Map::new();
    entry.insert("byteLength".to_owned(), Value::from(length as u64));
    root.entry("buffers")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "glTF buffers is not an array".to_owned())?
        .push(Value::Object(entry));
    Ok(())
}

struct DracoRequest {
    mesh: usize,
    primitive: usize,
    view: usize,
    attributes: Vec<(String, usize, u32)>,
    indices: Option<usize>,
}

fn decode_draco_primitives(
    root: &mut Map<String, Value>,
    buffers: &mut Vec<Vec<u8>>,
    budget: &mut PayloadBudget,
) -> Result<(), String> {
    ensure_draco_indices(root);
    let requests = collect_draco_requests(root);
    if requests.is_empty() {
        return Ok(());
    }
    let decoded_buffer = buffers.len();
    let mut decoded = Vec::<u8>::new();
    let mut views = Vec::<Value>::new();
    let mut rewrites = Vec::<(usize, usize, String)>::new();
    let base_view = root
        .get("bufferViews")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);

    for request in requests {
        let encoded = draco_payload(root, buffers, request.view)?;
        let mesh = vkit_core::formats::decode_draco_mesh(&encoded).map_err(|error| {
            format!(
                "glTF mesh {} primitive {} could not be Draco-decoded: {error}",
                request.mesh, request.primitive
            )
        })?;
        if let Some(accessor) = request.indices {
            let offset = decoded.len();
            for index in &mesh.indices {
                decoded.extend_from_slice(&index.to_le_bytes());
            }
            let length = decoded.len() - offset;
            budget.charge(length as u64, "Draco indices")?;
            views.push(json_view(decoded_buffer, offset, length));
            rewrites.push((
                accessor,
                base_view + views.len() - 1,
                format!(
                    r#"{{"componentType":5125,"count":{},"type":"SCALAR"}}"#,
                    mesh.indices.len()
                ),
            ));
        }
        for (semantic, accessor, unique_id) in &request.attributes {
            let Some(attribute) = mesh.attribute_by_unique_id(*unique_id) else {
                continue;
            };
            let Some(kind) = accessor_type(attribute.components) else {
                continue;
            };
            let offset = decoded.len();
            for value in &attribute.values {
                decoded.extend_from_slice(&value.to_le_bytes());
            }
            let length = decoded.len() - offset;
            budget.charge(length as u64, "Draco attribute")?;
            views.push(json_view(decoded_buffer, offset, length));
            let bounds = if semantic == "POSITION" {
                component_bounds(&attribute.values, attribute.components)
            } else {
                String::new()
            };
            rewrites.push((
                *accessor,
                base_view + views.len() - 1,
                format!(
                    r#"{{"componentType":5126,"count":{},"type":"{kind}"{bounds}}}"#,
                    mesh.num_points
                ),
            ));
        }
    }
    if decoded.is_empty() {
        return Ok(());
    }
    let length = decoded.len();
    buffers.push(decoded);
    let mut buffer_entry = Map::new();
    buffer_entry.insert("byteLength".to_owned(), Value::from(length as u64));
    root.entry("buffers")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "glTF buffers is not an array".to_owned())?
        .push(Value::Object(buffer_entry));
    root.entry("bufferViews")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| "glTF bufferViews is not an array".to_owned())?
        .extend(views);
    let accessors = root
        .get_mut("accessors")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "glTF accessors is not an array".to_owned())?;
    for (index, view, shape) in rewrites {
        let Some(slot) = accessors.get_mut(index).and_then(Value::as_object_mut) else {
            continue;
        };
        let shape = serde_json::from_str::<Value>(&shape)
            .map_err(|error| format!("glTF accessor {index} could not be rewritten: {error}"))?;
        let Some(shape) = shape.as_object() else {
            continue;
        };
        slot.remove("sparse");
        slot.remove("normalized");
        slot.insert("bufferView".to_owned(), Value::from(view as u64));
        slot.insert("byteOffset".to_owned(), Value::from(0_u64));
        for (key, value) in shape {
            slot.insert(key.clone(), value.clone());
        }
    }
    strip_draco_extension(root);
    Ok(())
}

fn ensure_draco_indices(root: &mut Map<String, Value>) {
    let mut next = root
        .get("accessors")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let mut claimed = 0_usize;
    if let Some(meshes) = root.get_mut("meshes").and_then(Value::as_array_mut) {
        for mesh in meshes {
            let Some(primitives) = mesh.get_mut("primitives").and_then(Value::as_array_mut) else {
                continue;
            };
            for primitive in primitives {
                let compressed = primitive
                    .get("extensions")
                    .is_some_and(|extensions| extensions.get(DRACO_EXTENSION).is_some());
                let Some(object) = primitive.as_object_mut() else {
                    continue;
                };
                if !compressed || object.contains_key("indices") {
                    continue;
                }
                object.insert("indices".to_owned(), Value::from(next as u64));
                next += 1;
                claimed += 1;
            }
        }
    }
    if claimed == 0 {
        return;
    }
    let accessors = root
        .entry("accessors")
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(accessors) = accessors.as_array_mut() {
        for _ in 0..claimed {
            accessors.push(
                serde_json::from_str(r#"{"componentType":5125,"count":1,"type":"SCALAR"}"#)
                    .unwrap_or_else(|_| Value::Object(Map::new())),
            );
        }
    }
}

fn collect_draco_requests(root: &Map<String, Value>) -> Vec<DracoRequest> {
    let mut requests = Vec::new();
    for (mesh, entry) in entries(root, "meshes") {
        for (primitive, body) in indexed(entry.get("primitives")) {
            let Some(extension) = body
                .get("extensions")
                .and_then(|extensions| extensions.get(DRACO_EXTENSION))
            else {
                continue;
            };
            let Some(view) = extension
                .get("bufferView")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
            else {
                continue;
            };
            let declared = body.get("attributes").and_then(Value::as_object);
            let compressed = extension.get("attributes").and_then(Value::as_object);
            let mut attributes = Vec::new();
            if let (Some(declared), Some(compressed)) = (declared, compressed) {
                for (semantic, unique_id) in compressed {
                    let accessor = declared
                        .get(semantic)
                        .and_then(Value::as_u64)
                        .and_then(|value| usize::try_from(value).ok());
                    let unique_id = unique_id
                        .as_u64()
                        .and_then(|value| u32::try_from(value).ok());
                    if let (Some(accessor), Some(unique_id)) = (accessor, unique_id) {
                        attributes.push((semantic.clone(), accessor, unique_id));
                    }
                }
            }
            requests.push(DracoRequest {
                mesh,
                primitive,
                view,
                attributes,
                indices: body
                    .get("indices")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok()),
            });
        }
    }
    requests
}

fn draco_payload(
    root: &Map<String, Value>,
    buffers: &[Vec<u8>],
    view: usize,
) -> Result<Vec<u8>, String> {
    let entry = root
        .get("bufferViews")
        .and_then(Value::as_array)
        .and_then(|views| views.get(view))
        .ok_or_else(|| format!("glTF bufferView {view} named by a Draco primitive is missing"))?;
    let buffer = entry
        .get("buffer")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("glTF bufferView {view} names no buffer"))?;
    let offset = entry
        .get("byteOffset")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_default();
    let length = entry
        .get("byteLength")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("glTF bufferView {view} declares no byteLength"))?;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| format!("glTF bufferView {view} range overflows"))?;
    buffers
        .get(buffer)
        .and_then(|bytes| bytes.get(offset..end))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| format!("glTF bufferView {view} lies outside buffer {buffer}"))
}

fn json_view(buffer: usize, offset: usize, length: usize) -> Value {
    let mut entry = Map::new();
    entry.insert("buffer".to_owned(), Value::from(buffer as u64));
    entry.insert("byteOffset".to_owned(), Value::from(offset as u64));
    entry.insert("byteLength".to_owned(), Value::from(length as u64));
    Value::Object(entry)
}

fn accessor_type(components: usize) -> Option<&'static str> {
    match components {
        1 => Some("SCALAR"),
        2 => Some("VEC2"),
        3 => Some("VEC3"),
        4 => Some("VEC4"),
        _ => None,
    }
}

fn component_bounds(values: &[f32], components: usize) -> String {
    if components == 0 || values.is_empty() {
        return String::new();
    }
    let mut minimum = vec![f64::INFINITY; components];
    let mut maximum = vec![f64::NEG_INFINITY; components];
    for point in values.chunks_exact(components) {
        for (axis, value) in point.iter().enumerate() {
            let value = f64::from(*value);
            if let Some(slot) = minimum.get_mut(axis) {
                *slot = slot.min(value);
            }
            if let Some(slot) = maximum.get_mut(axis) {
                *slot = slot.max(value);
            }
        }
    }
    if minimum.iter().chain(maximum.iter()).any(|v| !v.is_finite()) {
        return String::new();
    }
    let list = |values: &[f64]| {
        values
            .iter()
            .map(|value| format!("{value}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(r#","min":[{}],"max":[{}]"#, list(&minimum), list(&maximum))
}

fn strip_draco_extension(root: &mut Map<String, Value>) {
    let Some(meshes) = root.get_mut("meshes").and_then(Value::as_array_mut) else {
        return;
    };
    for mesh in meshes {
        let Some(primitives) = mesh.get_mut("primitives").and_then(Value::as_array_mut) else {
            continue;
        };
        for primitive in primitives {
            let Some(object) = primitive.as_object_mut() else {
                continue;
            };
            if let Some(extensions) = object.get_mut("extensions").and_then(Value::as_object_mut) {
                extensions.remove(DRACO_EXTENSION);
                if extensions.is_empty() {
                    object.remove("extensions");
                }
            }
        }
    }
}

fn meshopt_request(view: &Value, index: usize) -> Result<Option<MeshoptRequest>, String> {
    let Some(extension) = view
        .get("extensions")
        .and_then(|extensions| extensions.get(MESHOPT_EXTENSION))
    else {
        return Ok(None);
    };
    let count = |name: &str| -> Result<usize, String> {
        extension
            .get(name)
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| {
                format!("glTF bufferView {index} {MESHOPT_EXTENSION} has no usable {name}")
            })
    };
    let offset = match extension.get("byteOffset") {
        None => 0,
        Some(_) => count("byteOffset")?,
    };
    let mode = name_field(extension, "mode", index)?
        .ok_or_else(|| format!("glTF bufferView {index} {MESHOPT_EXTENSION} has no mode"))?;
    let mode = MeshoptMode::from_extension_name(mode)
        .map_err(|error| format!("glTF bufferView {index}: {error}"))?;
    let filter = match name_field(extension, "filter", index)? {
        None => MeshoptFilter::None,
        Some(name) => MeshoptFilter::from_extension_name(name)
            .map_err(|error| format!("glTF bufferView {index}: {error}"))?,
    };
    Ok(Some(MeshoptRequest {
        buffer: count("buffer")?,
        offset,
        length: count("byteLength")?,
        stride: count("byteStride")?,
        count: count("count")?,
        mode,
        filter,
    }))
}

fn name_field<'a>(
    extension: &'a Value,
    key: &str,
    index: usize,
) -> Result<Option<&'a str>, String> {
    match extension.get(key) {
        None => Ok(None),
        Some(value) => value.as_str().map(Some).ok_or_else(|| {
            format!("glTF bufferView {index} {MESHOPT_EXTENSION} {key} is not a string")
        }),
    }
}

fn rewrite_view(view: &mut Value, buffer: usize, offset: usize, length: usize) {
    let Some(object) = view.as_object_mut() else {
        return;
    };
    object.insert("buffer".to_owned(), Value::from(buffer as u64));
    object.insert("byteOffset".to_owned(), Value::from(offset as u64));
    object.insert("byteLength".to_owned(), Value::from(length as u64));
    if let Some(extensions) = object.get_mut("extensions").and_then(Value::as_object_mut) {
        extensions.remove(MESHOPT_EXTENSION);
        if extensions.is_empty() {
            object.remove("extensions");
        }
    }
}

pub(super) fn read_uri(
    root: &Path,
    uri: &str,
    label: &str,
    budget: &mut PayloadBudget,
) -> Result<(Vec<u8>, Option<String>), String> {
    if let Some(data) = uri.strip_prefix("data:") {
        let (metadata, encoded) = data
            .split_once(',')
            .ok_or_else(|| format!("glTF {label} data URI is malformed"))?;
        let mime = metadata
            .split(';')
            .next()
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if !metadata
            .split(';')
            .any(|value| value.eq_ignore_ascii_case("base64"))
        {
            return Err(format!("glTF {label} data URI must use base64 encoding"));
        }
        budget.charge(encoded.len() as u64, label)?;
        return decode_base64(encoded)
            .map(|bytes| (bytes, mime))
            .map_err(|error| format!("glTF {label} data URI is invalid: {error}"));
    }
    let source = resolve_uri_path(root, uri, label)?;
    let bytes = source
        .metadata()
        .map_err(|error| format!("failed to inspect glTF {label} URI {uri:?}: {error}"))?
        .len();
    budget.charge(bytes, label)?;
    fs::read(source)
        .map(|bytes| (bytes, None))
        .map_err(|error| format!("failed to read glTF {label} URI {uri:?}: {error}"))
}

fn resolve_uri_path(root: &Path, uri: &str, label: &str) -> Result<PathBuf, String> {
    let decoded = percent_decode(uri).replace('\\', "/");
    if decoded.contains(':') {
        return Err(format!(
            "glTF {label} URI {uri:?} names a drive or a remote scheme; only files beside the \
             glTF are read"
        ));
    }
    let relative = Path::new(&decoded);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err(format!(
            "glTF {label} URI {uri:?} is not a safe relative path"
        ));
    }
    let base = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let candidate = base.join(relative).canonicalize().map_err(|error| {
        format!("failed to locate glTF {label} URI {uri:?} beside the container: {error}")
    })?;
    if !candidate.starts_with(&base) {
        return Err(format!(
            "glTF {label} URI {uri:?} resolves outside the folder holding the glTF"
        ));
    }
    Ok(candidate)
}

fn percent_decode(uri: &str) -> String {
    let raw = uri.as_bytes();
    let mut decoded = Vec::with_capacity(raw.len());
    let mut at = 0;
    while at < raw.len() {
        let byte = raw[at];
        let escape = (byte == b'%')
            .then(|| raw.get(at + 1..at + 3))
            .flatten()
            .and_then(|digits| std::str::from_utf8(digits).ok())
            .and_then(|digits| u8::from_str_radix(digits, 16).ok());
        match escape {
            Some(value) => {
                decoded.push(value);
                at += 3;
            }
            None => {
                decoded.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| uri.to_owned())
}

fn decode_base64(encoded: &str) -> Result<Vec<u8>, base64::DecodeError> {
    let payload = if encoded.bytes().any(|byte| byte.is_ascii_whitespace()) {
        encoded
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>()
    } else {
        encoded.as_bytes().to_vec()
    };
    match BASE64_FORGIVING.decode(&payload) {
        Ok(bytes) => Ok(bytes),
        Err(error) => match BASE64_URL_FORGIVING.decode(&payload) {
            Ok(bytes) => Ok(bytes),
            Err(_) => Err(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json_root(text: &str) -> Map<String, Value> {
        match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(object)) => object,
            _ => Map::new(),
        }
    }

    #[test]
    fn unsafe_external_uri_is_rejected() {
        let mut budget = PayloadBudget::new();
        let error = read_uri(Path::new("."), "../outside.bin", "buffer", &mut budget)
            .expect_err("a parent-relative URI must not be read");
        assert!(error.contains("safe relative path"));
    }

    #[test]
    fn the_payload_budget_is_spent_across_reads_not_per_read() {
        let mut budget = PayloadBudget::new();
        assert!(
            budget
                .charge(MAX_EXTERNAL_PAYLOAD_BYTES - 1, "buffer")
                .is_ok()
        );
        let error = budget
            .charge(2, "buffer")
            .expect_err("a second read must see what the first one spent");
        assert!(error.contains("budget is left"));
    }

    #[test]
    fn required_extensions_are_named_before_the_crate_can_refuse_them() {
        let error = reject_unsupported_features(&json_root(
            r#"{"extensionsRequired":["EXT_invented_here","EXT_weird"]}"#,
        ))
        .expect_err("an unreadable required extension must be refused here");
        assert!(error.contains("EXT_invented_here"));
        assert!(error.contains("EXT_weird"));
        assert!(
            !error.contains("Unsupported extension"),
            "the crate's own verdict must never be what the user reads: {error}"
        );
    }

    #[test]
    fn the_geometry_extensions_this_importer_resolves_are_accepted() {
        assert!(
            reject_unsupported_features(&json_root(
                r#"{"extensionsRequired":["KHR_mesh_quantization","EXT_meshopt_compression",
                    "KHR_draco_mesh_compression","EXT_mesh_gpu_instancing"]}"#,
            ))
            .is_ok(),
            "each of these decides where a vertex is, and each is resolved before the crate sees the document, so requiring one is not grounds for refusing the file"
        );
    }

    #[test]
    fn an_appearance_only_required_extension_is_read_past_rather_than_refused() {
        for required in [
            "KHR_materials_unlit",
            "KHR_materials_variants",
            "KHR_materials_dispersion",
            "KHR_texture_transform",
            "KHR_texture_basisu",
            "EXT_texture_webp",
            "MSFT_texture_dds",
            "KHR_lights_punctual",
        ] {
            assert!(
                reject_unsupported_features(&json_root(&format!(
                    r#"{{"extensionsRequired":["{required}"]}}"#
                )))
                .is_ok(),
                "{required} decides how a surface is shaded, never where it is"
            );
        }
    }

    #[test]
    fn a_geometry_bearing_extension_is_still_refused_when_it_is_required() {
        for required in ["KHR_implicit_shapes", "EXT_invented_here"] {
            assert!(
                reject_unsupported_features(&json_root(&format!(
                    r#"{{"extensionsRequired":["{required}"]}}"#
                )))
                .is_err(),
                "{required} could move a vertex, so ignoring it would import silently wrong \
                 geometry rather than fail"
            );
        }
    }

    #[test]
    fn an_extension_named_only_in_extensions_used_is_never_grounds_for_anything() {
        assert!(
            reject_unsupported_features(&json_root(
                r#"{"extensionsUsed":["EXT_invented_here","KHR_texture_transform"]}"#,
            ))
            .is_ok(),
            "extensionsUsed says a producer wrote something somewhere, not that this mesh needs it"
        );
    }

    #[test]
    fn a_texture_whose_image_lives_under_an_extension_becomes_a_texture_again() {
        let mut root = json_root(
            r#"{"textures":[{"extensions":{"KHR_texture_basisu":{"source":1},
                "EXT_texture_webp":{"source":2}}},{"source":0,
                "extensions":{"EXT_texture_webp":{"source":3}}},{}]}"#,
        );
        hoist_texture_sources(&mut root);
        let textures = root["textures"].as_array().expect("textures");
        assert_eq!(
            textures[0]["source"].as_u64(),
            Some(2),
            "the decodable image wins when a texture offers several"
        );
        assert_eq!(
            textures[1]["source"].as_u64(),
            Some(0),
            "a texture that already names its own image keeps it"
        );
        assert!(
            textures[2].get("source").is_none(),
            "a texture with no image anywhere is left empty rather than pointed at image 0"
        );
    }

    #[test]
    fn a_shading_enumeration_the_crate_does_not_know_is_dropped_not_refused() {
        let mut root = json_root(
            r#"{"materials":[{"alphaMode":"OPAQUE"},{"alphaMode":"CUTOUT"}],
                "samplers":[{"wrapS":10497,"magFilter":9729},{"wrapS":0,"magFilter":33071}]}"#,
        );
        normalize_appearance_enums(&mut root);
        let materials = root["materials"].as_array().expect("materials");
        assert_eq!(materials[0]["alphaMode"].as_str(), Some("OPAQUE"));
        assert!(materials[1].get("alphaMode").is_none());
        let samplers = root["samplers"].as_array().expect("samplers");
        assert_eq!(samplers[0]["wrapS"].as_u64(), Some(10497));
        assert!(samplers[1].get("wrapS").is_none());
        assert!(
            samplers[1].get("magFilter").is_none(),
            "33071 is a wrap mode, not a filter, and the crate refuses the document over it"
        );
    }

    #[test]
    fn animations_are_dropped_rather_than_validated() {
        let mut root = json_root(
            r#"{"animations":[{"channels":[{"sampler":0,"target":{"node":0,"path":"pointer"}}],
                "samplers":[{"input":0,"output":1,"interpolation":"STEP"}]}]}"#,
        );
        drop_animations(&mut root);
        assert!(
            root.get("animations").is_none(),
            "a KHR_animation_pointer channel path is outside the crate's enumeration, so leaving \
             the array in place refuses a file over keyframes no vertex reads"
        );
    }

    #[test]
    fn a_percent_escape_is_decoded_before_the_path_is_judged() {
        assert_eq!(percent_decode("scan%20data.bin"), "scan data.bin");
        assert_eq!(percent_decode("%2e%2e%2foutside.bin"), "../outside.bin");
        assert_eq!(
            percent_decode("%ED%94%BC%EB%B6%80.png"),
            "\u{d53c}\u{bd80}.png"
        );
        assert_eq!(
            percent_decode("half%2.png"),
            "half%2.png",
            "a malformed escape is left alone rather than being an error of its own"
        );
    }

    #[test]
    fn base64_reads_what_browsers_read() {
        let canonical = "AAECAwQF";
        assert_eq!(
            decode_base64(canonical).expect("padded"),
            [0, 1, 2, 3, 4, 5]
        );
        assert_eq!(
            decode_base64("AAECAwQF\n").expect("line wrapped"),
            [0, 1, 2, 3, 4, 5]
        );
        assert_eq!(decode_base64("AAEC").expect("unpadded quantum"), [0, 1, 2]);
        assert_eq!(decode_base64("--8=").expect("url safe"), [251, 239]);
    }

    #[test]
    fn a_truncated_glb_header_is_an_error_not_an_abort() {
        let mut truncated = b"glTF".to_vec();
        truncated.extend_from_slice(&2_u32.to_le_bytes());
        truncated.extend_from_slice(&4_u32.to_le_bytes());
        let error =
            split_container(&truncated).expect_err("a short declared length must be an error");
        assert!(error.contains("declares 4 bytes"));
    }
}
