use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    fs::{self, File},
    io::{BufReader, Read},
    path::{Component, Path, PathBuf},
};

use fbx_dom::{Document, ImportSettings, OwnedObject, Property};
use fbxcel::{low::v7400::AttributeValue, tree::any::AnyTree};
use fbxscii::{ElementAttribute, Parser, Tokenizer};

use vkit_core::{
    G2F_POLYGON_COUNT, G2F_VERTEX_COUNT,
    formats::{ObjFace, OrderedObjMesh},
};

use super::simplify::{AttributeMesh, AttributeMeshBuilder, Corner, FaceLabels};

type Matrix = [[f64; 4]; 4];

const BINARY_MAGIC: &[u8] = b"Kaydara FBX Binary  \0\x1a\0";
const MAX_HIERARCHY_DEPTH: usize = 256;
const MAX_TEXTURE_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_MATERIAL: &str = "FBX_Default";
const TEMPLATE_ANATOMY_GROUPS: &[&str] = &["lEye", "rEye", "upperJaw", "lowerJaw", "tongue"];
const TEMPLATE_EYE_MATERIALS: &[&str] = &["Pupils", "Irises", "Cornea", "Sclera", "EyeReflection"];
const TEMPLATE_JAW_MATERIALS: &[&str] = &["Teeth", "Gums", "InnerMouth"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Encoding {
    Ascii,
    Binary,
}

#[derive(Clone, Debug)]
struct Connection {
    kind: ConnectionKind,
    source: u64,
    destination: u64,
    property: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConnectionKind {
    Object,
    ObjectProperty,
}

#[derive(Debug)]
struct RawGeometry {
    id: u64,
    name: String,
    control_points: Vec<[f64; 3]>,
    polygons: Vec<Polygon>,
}

#[derive(Debug)]
struct Polygon {
    control_points: Vec<usize>,
    uvs: Vec<Option<[f64; 2]>>,
    material: Option<i32>,
}

#[derive(Debug)]
struct Appearance {
    material_names: Vec<String>,
    names_by_id: HashMap<u64, String>,
}

#[derive(Debug)]
struct MaterialInfo {
    id: u64,
    name: String,
    diffuse: [f64; 3],
    opacity: f64,
}

#[derive(Clone, Copy, Debug)]
struct ResolvedTransform {
    matrix: Matrix,
    rotation: Matrix,
    scale: Matrix,
}

#[derive(Clone, Copy, Debug)]
struct LocalTransform {
    matrix: Matrix,
    rotation: Matrix,
    scale: Matrix,
    inheritance: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AxisUnits {
    up_axis: usize,
    up_sign: f64,
    front_axis: usize,
    front_sign: f64,
    coord_axis: usize,
    coord_sign: f64,
    unit_scale_to_cm: f64,
}

pub(crate) fn load_fbx(
    path: &Path,
    appearance_root: &Path,
    mut progress: impl FnMut(f32),
) -> Result<AttributeMesh, String> {
    progress(0.03);
    let encoding = detect_encoding(path)?;
    let source_to_y_up_cm =
        load_axis_units(path, encoding)?.map_or_else(identity, AxisUnits::source_to_y_up_cm);
    let document = load_document(path, encoding)?;
    let mut objects = Vec::new();
    for object in document.objects() {
        let object = object.map_err(|error| {
            format!("FBX object could not be materialized (missing or invalid template): {error:?}")
        })?;
        objects.push(OwnedObject::from(object));
    }
    objects.sort_by_key(|object| object.object_index);
    let connections = load_connections(path, encoding)?;
    progress(0.22);

    reject_unsupported_features(&objects)?;
    let objects_by_id = objects
        .iter()
        .map(|object| (object.object_index, object))
        .collect::<HashMap<_, _>>();
    let model_ids = objects
        .iter()
        .filter(|object| object.type_name.eq_ignore_ascii_case("Model"))
        .map(|object| object.object_index)
        .collect::<HashSet<_>>();
    let geometry_ids = objects
        .iter()
        .filter(|object| {
            object.type_name.eq_ignore_ascii_case("Geometry")
                && object.class_name.eq_ignore_ascii_case("Mesh")
        })
        .map(|object| object.object_index)
        .collect::<HashSet<_>>();

    let mut geometries = HashMap::new();
    for object in objects
        .iter()
        .filter(|object| geometry_ids.contains(&object.object_index))
    {
        geometries.insert(object.object_index, parse_geometry(object)?);
    }
    if geometries.is_empty() {
        return Err("FBX contains no static polygon mesh geometry".to_owned());
    }
    progress(0.42);

    let appearance = write_material_library(
        path.parent().unwrap_or_else(|| Path::new(".")),
        appearance_root,
        &objects,
        &objects_by_id,
        &connections,
    )?;
    let mut builder = AttributeMeshBuilder::with_appearance(
        vec![PathBuf::from("vkit-import.mtl")],
        appearance.material_names.clone(),
    );
    progress(0.52);

    let parents = model_parents(&connections, &model_ids)?;
    let mut worlds = HashMap::new();
    let mut visiting = HashSet::new();
    let mut ordered_models = model_ids.iter().copied().collect::<Vec<_>>();
    ordered_models.sort_unstable();
    for model_id in ordered_models {
        resolve_world_matrix(
            model_id,
            &objects_by_id,
            &parents,
            &mut worlds,
            &mut visiting,
            0,
        )?;
    }

    let mut instances = Vec::new();
    let mut seen_instances = HashSet::new();
    for connection in &connections {
        if connection.kind == ConnectionKind::Object
            && geometry_ids.contains(&connection.source)
            && model_ids.contains(&connection.destination)
            && seen_instances.insert((connection.source, connection.destination))
        {
            instances.push((connection.source, Some(connection.destination)));
        }
    }
    let connected_geometries = instances
        .iter()
        .map(|(geometry, _)| *geometry)
        .collect::<HashSet<_>>();
    let mut unconnected = geometry_ids
        .iter()
        .filter(|geometry| !connected_geometries.contains(geometry))
        .copied()
        .collect::<Vec<_>>();
    unconnected.sort_unstable();
    instances.extend(unconnected.into_iter().map(|geometry| (geometry, None)));

    let mut source_vertex_base = 0_u64;
    let instance_total = instances.len().max(1);
    for (instance_index, (geometry_id, model_id)) in instances.into_iter().enumerate() {
        let geometry = geometries
            .get(&geometry_id)
            .ok_or_else(|| format!("FBX geometry {geometry_id} disappeared during import"))?;
        let (transform, group, material_slots) = if let Some(model_id) = model_id {
            let model = objects_by_id
                .get(&model_id)
                .copied()
                .ok_or_else(|| format!("FBX model {model_id} is missing"))?;
            let world = worlds
                .get(&model_id)
                .ok_or_else(|| format!("FBX model {model_id} has no world transform"))?
                .matrix;
            (
                multiply(
                    source_to_y_up_cm,
                    multiply(world, geometric_transform(model)?),
                ),
                format!(
                    "{}_{}",
                    safe_label(strip_namespace(&model.name), "FBX_Model"),
                    model_id
                ),
                material_slots(model_id, &connections, &appearance.names_by_id),
            )
        } else {
            (
                source_to_y_up_cm,
                format!(
                    "{}_{}",
                    safe_label(strip_namespace(&geometry.name), "FBX_Geometry"),
                    geometry.id
                ),
                Vec::new(),
            )
        };
        emit_geometry(
            geometry,
            transform,
            &group,
            &material_slots,
            &mut source_vertex_base,
            &mut builder,
        )?;
        progress(0.52 + 0.46 * ((instance_index + 1) as f32 / instance_total as f32));
    }

    let mesh = builder.finish()?;
    progress(1.0);
    Ok(mesh)
}

pub(crate) fn load_ordered_template_fbx(
    path: &Path,
) -> Result<(OrderedObjMesh, Vec<String>), String> {
    let encoding = detect_encoding(path)?;
    let source_to_y_up_cm =
        load_axis_units(path, encoding)?.map_or_else(identity, AxisUnits::source_to_y_up_cm);
    let document = load_document(path, encoding)?;
    let mut objects = Vec::new();
    for object in document.objects() {
        let object = object.map_err(|error| {
            format!("FBX object could not be materialized (missing or invalid template): {error:?}")
        })?;
        objects.push(OwnedObject::from(object));
    }
    objects.sort_by_key(|object| object.object_index);
    let connections = load_connections(path, encoding)?;
    reject_unsupported_features(&objects)?;

    let objects_by_id = objects
        .iter()
        .map(|object| (object.object_index, object))
        .collect::<HashMap<_, _>>();
    let model_ids = objects
        .iter()
        .filter(|object| object.type_name.eq_ignore_ascii_case("Model"))
        .map(|object| object.object_index)
        .collect::<HashSet<_>>();
    let geometry_ids = objects
        .iter()
        .filter(|object| {
            object.type_name.eq_ignore_ascii_case("Geometry")
                && object.class_name.eq_ignore_ascii_case("Mesh")
        })
        .map(|object| object.object_index)
        .collect::<HashSet<_>>();
    if geometry_ids.len() != 1 {
        return Err(format!(
            "ordered FBX template requires exactly one static mesh geometry; found {}",
            geometry_ids.len()
        ));
    }
    let mut instances = connections
        .iter()
        .filter(|connection| {
            connection.kind == ConnectionKind::Object
                && geometry_ids.contains(&connection.source)
                && model_ids.contains(&connection.destination)
        })
        .map(|connection| (connection.source, connection.destination))
        .collect::<Vec<_>>();
    instances.sort_unstable();
    instances.dedup();
    let [(geometry_id, model_id)] = instances.as_slice() else {
        return Err(format!(
            "ordered FBX template requires exactly one mesh-to-model connection; found {}",
            instances.len()
        ));
    };
    let geometry = parse_geometry(
        objects_by_id
            .get(geometry_id)
            .copied()
            .ok_or_else(|| format!("FBX geometry {geometry_id} is missing"))?,
    )?;
    let model = objects_by_id
        .get(model_id)
        .copied()
        .ok_or_else(|| format!("FBX model {model_id} is missing"))?;

    let parents = model_parents(&connections, &model_ids)?;
    let mut worlds = HashMap::new();
    let mut visiting = HashSet::new();
    let world = resolve_world_matrix(
        *model_id,
        &objects_by_id,
        &parents,
        &mut worlds,
        &mut visiting,
        0,
    )?
    .matrix;
    let transform = multiply(
        source_to_y_up_cm,
        multiply(world, geometric_transform(model)?),
    );
    let vertices = geometry
        .control_points
        .iter()
        .map(|&point| transform_point(transform, point))
        .collect::<Vec<_>>();
    if !vertices.iter().flatten().all(|value| value.is_finite()) {
        return Err(format!(
            "FBX geometry {} produces non-finite transformed positions",
            geometry.id
        ));
    }

    let material_names = objects
        .iter()
        .filter(|object| object.type_name.eq_ignore_ascii_case("Material"))
        .map(|object| {
            (
                object.object_index,
                strip_namespace(&object.name).to_owned(),
            )
        })
        .collect::<HashMap<_, _>>();
    let material_slots = material_slots(*model_id, &connections, &material_names);
    let group = strip_namespace(&geometry.name).to_owned();
    let faces = geometry
        .polygons
        .iter()
        .enumerate()
        .map(|(polygon_index, polygon)| {
            let material = match polygon.material {
                Some(index) if index >= 0 => material_slots.get(index as usize).cloned().ok_or_else(|| {
                    format!(
                        "FBX geometry {} polygon {polygon_index} references material slot {index}, but its model has {} slots",
                        geometry.id,
                        material_slots.len()
                    )
                })?,
                _ if material_slots.len() == 1 => material_slots[0].clone(),
                _ => DEFAULT_MATERIAL.to_owned(),
            };
            Ok(ObjFace {
                vertex_indices: polygon
                    .control_points
                    .iter()
                    .map(|&index| u32::try_from(index).map_err(|_| "FBX control-point index exceeds u32".to_owned()))
                    .collect::<Result<Vec<_>, _>>()?,
                group: Some(group.clone()),
                material: Some(material),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut ordered = OrderedObjMesh { vertices, faces };
    ordered.validate().map_err(|error| error.to_string())?;
    if ordered.vertices.len() == G2F_VERTEX_COUNT
        && ordered.faces.len() == G2F_POLYGON_COUNT
        && ordered
            .faces
            .iter()
            .all(|face| matches!(face.vertex_indices.len(), 3 | 4))
        && !has_required_template_anatomy_groups(&ordered.faces)
    {
        reconstruct_template_anatomy_groups(&mut ordered)?;
        ordered.validate().map_err(|error| error.to_string())?;
    }
    Ok((ordered, vec![geometry.name, model.name.clone()]))
}

fn has_required_template_anatomy_groups(faces: &[ObjFace]) -> bool {
    TEMPLATE_ANATOMY_GROUPS.iter().all(|required| {
        faces.iter().any(|face| {
            face.group
                .as_deref()
                .is_some_and(|group| group.eq_ignore_ascii_case(required))
        })
    })
}

fn reconstruct_template_anatomy_groups(mesh: &mut OrderedObjMesh) -> Result<(), String> {
    mesh.validate().map_err(|error| error.to_string())?;
    let centroids = mesh
        .faces
        .iter()
        .enumerate()
        .map(|(face_index, face)| face_centroid(mesh, face_index, face))
        .collect::<Result<Vec<_>, _>>()?;
    let (minimum, maximum) = vertex_bounds(&mesh.vertices)?;
    let width = maximum[0] - minimum[0];
    if !width.is_finite() || width <= f64::EPSILON {
        return Err(
            "FBX anatomy reconstruction cannot classify lEye/rEye because the mesh has no usable X extent"
                .to_owned(),
        );
    }

    let eye_faces = matching_material_faces(&mesh.faces, TEMPLATE_EYE_MATERIALS);
    if eye_faces.is_empty() {
        return Err(format!(
            "FBX anatomy reconstruction requires eye faces using DAZ materials {:?}",
            TEMPLATE_EYE_MATERIALS
        ));
    }
    let jaw_faces = matching_material_faces(&mesh.faces, TEMPLATE_JAW_MATERIALS);
    if jaw_faces.is_empty() {
        return Err(format!(
            "FBX anatomy reconstruction requires jaw faces using DAZ materials {:?}",
            TEMPLATE_JAW_MATERIALS
        ));
    }
    if !mesh.faces.iter().any(|face| {
        face.material
            .as_deref()
            .is_some_and(|material| material.eq_ignore_ascii_case("Tongue"))
    }) {
        return Err(
            "FBX anatomy reconstruction requires faces using the DAZ material \"Tongue\""
                .to_owned(),
        );
    }

    let jaw_minimum_y = jaw_faces
        .iter()
        .map(|&face_index| centroids[face_index][1])
        .min_by(f64::total_cmp)
        .expect("jaw faces are non-empty");
    let jaw_maximum_y = jaw_faces
        .iter()
        .map(|&face_index| centroids[face_index][1])
        .max_by(f64::total_cmp)
        .expect("jaw faces are non-empty");
    let jaw_height = jaw_maximum_y - jaw_minimum_y;
    if !jaw_height.is_finite() || jaw_height <= f64::EPSILON {
        return Err(
            "FBX anatomy reconstruction cannot separate upperJaw/lowerJaw because DAZ teeth, gums, and inner-mouth face centroids have no vertical span"
                .to_owned(),
        );
    }

    let mut assignments = Vec::with_capacity(mesh.faces.len());
    for (face_index, face) in mesh.faces.iter().enumerate() {
        if let Some(authored) = face.group.as_deref().filter(|group| {
            TEMPLATE_ANATOMY_GROUPS
                .iter()
                .any(|required| group.eq_ignore_ascii_case(required))
        }) {
            assignments.push(authored.to_owned());
            continue;
        }
        let material = face.material.as_deref().ok_or_else(|| {
            format!("FBX anatomy reconstruction found no material on polygon {face_index}")
        })?;
        let group = if material.eq_ignore_ascii_case("Tongue") {
            "tongue"
        } else if label_in(material, TEMPLATE_EYE_MATERIALS) {
            let normalized_x = (centroids[face_index][0] - minimum[0]) / width;
            if normalized_x >= 0.5 { "lEye" } else { "rEye" }
        } else if label_in(material, TEMPLATE_JAW_MATERIALS) {
            let normalized_y = (centroids[face_index][1] - jaw_minimum_y) / jaw_height;
            if normalized_y >= 0.5 {
                "upperJaw"
            } else {
                "lowerJaw"
            }
        } else {
            "head"
        };
        assignments.push(group.to_owned());
    }

    for required in TEMPLATE_ANATOMY_GROUPS {
        if !assignments
            .iter()
            .any(|group| group.eq_ignore_ascii_case(required))
        {
            return Err(format!(
                "FBX anatomy reconstruction produced no faces for required polygon group {required:?}"
            ));
        }
    }
    for (face, group) in mesh.faces.iter_mut().zip(assignments) {
        face.group = Some(group);
    }
    Ok(())
}

fn face_centroid(
    mesh: &OrderedObjMesh,
    face_index: usize,
    face: &ObjFace,
) -> Result<[f64; 3], String> {
    if face.vertex_indices.is_empty() {
        return Err(format!(
            "FBX anatomy reconstruction found an empty polygon at index {face_index}"
        ));
    }
    let mut centroid = [0.0; 3];
    for &vertex_index in &face.vertex_indices {
        let point = mesh.vertices.get(vertex_index as usize).ok_or_else(|| {
            format!(
                "FBX anatomy reconstruction polygon {face_index} references missing vertex {vertex_index}"
            )
        })?;
        for axis in 0..3 {
            centroid[axis] += point[axis];
        }
    }
    for coordinate in &mut centroid {
        *coordinate /= face.vertex_indices.len() as f64;
    }
    Ok(centroid)
}

fn vertex_bounds(vertices: &[[f64; 3]]) -> Result<([f64; 3], [f64; 3]), String> {
    let Some(first) = vertices.first().copied() else {
        return Err("FBX anatomy reconstruction requires at least one vertex".to_owned());
    };
    let mut minimum = first;
    let mut maximum = first;
    for point in &vertices[1..] {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(point[axis]);
            maximum[axis] = maximum[axis].max(point[axis]);
        }
    }
    Ok((minimum, maximum))
}

fn matching_material_faces(faces: &[ObjFace], materials: &[&str]) -> Vec<usize> {
    faces
        .iter()
        .enumerate()
        .filter_map(|(face_index, face)| {
            face.material
                .as_deref()
                .filter(|material| label_in(material, materials))
                .map(|_| face_index)
        })
        .collect()
}

fn label_in(label: &str, labels: &[&str]) -> bool {
    labels
        .iter()
        .any(|candidate| label.eq_ignore_ascii_case(candidate))
}

fn detect_encoding(path: &Path) -> Result<Encoding, String> {
    let mut file = File::open(path).map_err(|error| format!("failed to open FBX: {error}"))?;
    let mut magic = [0_u8; BINARY_MAGIC.len()];
    let read = file
        .read(&mut magic)
        .map_err(|error| format!("failed to inspect FBX header: {error}"))?;
    if read == BINARY_MAGIC.len() && magic == BINARY_MAGIC {
        Ok(Encoding::Binary)
    } else {
        Ok(Encoding::Ascii)
    }
}

fn load_document(path: &Path, encoding: Encoding) -> Result<Document, String> {
    let file = File::open(path).map_err(|error| format!("failed to open FBX: {error}"))?;
    match encoding {
        Encoding::Binary => {
            Document::from_binary_reader(BufReader::new(file), ImportSettings { strict: false })
                .map_err(|error| format!("invalid binary FBX: {error:?}"))
        }
        Encoding::Ascii => {
            let tokenizer = Tokenizer::new(BufReader::new(file));
            Document::from_parser(Parser::new(tokenizer), ImportSettings { strict: false })
                .map_err(|error| format!("invalid ASCII FBX: {error:?}"))
        }
    }
}

fn load_connections(path: &Path, encoding: Encoding) -> Result<Vec<Connection>, String> {
    match encoding {
        Encoding::Ascii => load_ascii_connections(path),
        Encoding::Binary => load_binary_connections(path),
    }
}

fn load_axis_units(path: &Path, encoding: Encoding) -> Result<Option<AxisUnits>, String> {
    match encoding {
        Encoding::Ascii => load_ascii_axis_units(path),
        Encoding::Binary => load_binary_axis_units(path),
    }
}

fn load_ascii_axis_units(path: &Path) -> Result<Option<AxisUnits>, String> {
    let file = File::open(path).map_err(|error| format!("failed to reopen FBX: {error}"))?;
    let arena = Parser::new(Tokenizer::new(BufReader::new(file)))
        .load()
        .map_err(|error| format!("failed to read ASCII FBX GlobalSettings: {error:?}"))?;
    let Some(global) = arena.get_handle_by_key("GlobalSettings") else {
        return Ok(None);
    };
    let properties = global
        .children()
        .find(|child| child.key() == "Properties70")
        .ok_or_else(|| "FBX GlobalSettings has no Properties70 axis/unit metadata".to_owned())?;
    let mut values = HashMap::new();
    for property in properties.children().filter(|child| child.key() == "P") {
        let tokens = property.tokens();
        let Some(name) = tokens.first() else {
            continue;
        };
        if !is_axis_unit_property(name) {
            continue;
        }
        let value = tokens
            .last()
            .ok_or_else(|| format!("FBX GlobalSettings property {name} has no value"))?
            .parse::<f64>()
            .map_err(|error| {
                format!("FBX GlobalSettings property {name} has an invalid value: {error}")
            })?;
        insert_axis_unit_property(&mut values, name, value)?;
    }
    axis_units_from_properties(&values).map(Some)
}

fn load_binary_axis_units(path: &Path) -> Result<Option<AxisUnits>, String> {
    let file = File::open(path).map_err(|error| format!("failed to reopen FBX: {error}"))?;
    let tree = AnyTree::from_seekable_reader(BufReader::new(file))
        .map_err(|error| format!("failed to read binary FBX GlobalSettings: {error}"))?;
    let AnyTree::V7400(_, tree, _) = tree else {
        return Err("binary FBX version is unsupported by the v7400 parser".to_owned());
    };
    let Some(global) = tree.root().first_child_by_name("GlobalSettings") else {
        return Ok(None);
    };
    let Some(properties) = global.first_child_by_name("Properties70") else {
        return Err("FBX GlobalSettings has no Properties70 axis/unit metadata".to_owned());
    };
    let mut values = HashMap::new();
    for property in properties.children_by_name("P") {
        let attributes = property.attributes();
        let Some(name) = attributes.first().and_then(binary_string) else {
            continue;
        };
        if !is_axis_unit_property(name) {
            continue;
        }
        let value = attributes
            .last()
            .and_then(binary_number)
            .ok_or_else(|| format!("FBX GlobalSettings property {name} has an invalid value"))?;
        insert_axis_unit_property(&mut values, name, value)?;
    }
    axis_units_from_properties(&values).map(Some)
}

fn is_axis_unit_property(name: &str) -> bool {
    matches!(
        name,
        "UpAxis"
            | "UpAxisSign"
            | "FrontAxis"
            | "FrontAxisSign"
            | "CoordAxis"
            | "CoordAxisSign"
            | "UnitScaleFactor"
    )
}

fn insert_axis_unit_property(
    values: &mut HashMap<String, f64>,
    name: &str,
    value: f64,
) -> Result<(), String> {
    if let Some(previous) = values.insert(name.to_owned(), value)
        && previous != value
    {
        return Err(format!(
            "FBX GlobalSettings property {name} has conflicting values {previous} and {value}"
        ));
    }
    Ok(())
}

fn axis_units_from_properties(values: &HashMap<String, f64>) -> Result<AxisUnits, String> {
    let required = [
        "UpAxis",
        "UpAxisSign",
        "FrontAxis",
        "FrontAxisSign",
        "CoordAxis",
        "CoordAxisSign",
        "UnitScaleFactor",
    ];
    let missing = required
        .iter()
        .filter(|name| !values.contains_key(**name))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "FBX GlobalSettings axis/unit metadata is incomplete; missing {}",
            missing.join(", ")
        ));
    }
    let up_axis = axis_index(values["UpAxis"], "UpAxis")?;
    let front_axis = axis_index(values["FrontAxis"], "FrontAxis")?;
    let coord_axis = axis_index(values["CoordAxis"], "CoordAxis")?;
    if [up_axis, front_axis, coord_axis]
        .iter()
        .copied()
        .collect::<HashSet<_>>()
        .len()
        != 3
    {
        return Err("FBX GlobalSettings axes are not an orthogonal permutation".to_owned());
    }
    let unit_scale_to_cm = values["UnitScaleFactor"];
    if !unit_scale_to_cm.is_finite() || unit_scale_to_cm <= 0.0 {
        return Err("FBX UnitScaleFactor must be positive and finite".to_owned());
    }
    Ok(AxisUnits {
        up_axis,
        up_sign: axis_sign(values["UpAxisSign"], "UpAxisSign")?,
        front_axis,
        front_sign: axis_sign(values["FrontAxisSign"], "FrontAxisSign")?,
        coord_axis,
        coord_sign: axis_sign(values["CoordAxisSign"], "CoordAxisSign")?,
        unit_scale_to_cm,
    })
}

fn axis_index(value: f64, name: &str) -> Result<usize, String> {
    if value == 0.0 || value == 1.0 || value == 2.0 {
        Ok(value as usize)
    } else {
        Err(format!("FBX {name} must be an integer axis in 0..=2"))
    }
}

fn axis_sign(value: f64, name: &str) -> Result<f64, String> {
    if value == -1.0 || value == 1.0 {
        Ok(value)
    } else {
        Err(format!("FBX {name} must be -1 or 1"))
    }
}

impl AxisUnits {
    fn source_to_y_up_cm(self) -> Matrix {
        let mut result = [[0.0; 4]; 4];
        result[0][self.coord_axis] = self.coord_sign * self.unit_scale_to_cm;
        result[1][self.up_axis] = self.up_sign * self.unit_scale_to_cm;

        result[2][self.front_axis] = -self.front_sign * self.unit_scale_to_cm;
        result[3][3] = 1.0;
        result
    }
}

fn load_ascii_connections(path: &Path) -> Result<Vec<Connection>, String> {
    let file = File::open(path).map_err(|error| format!("failed to reopen FBX: {error}"))?;
    let arena = Parser::new(Tokenizer::new(BufReader::new(file)))
        .load()
        .map_err(|error| format!("failed to read ordered ASCII FBX connections: {error:?}"))?;
    let Some(connections) = arena.get_handle_by_key("Connections") else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for connection in connections.children().filter(|child| child.key() == "C") {
        let tokens = connection.tokens();
        let Some(kind) = tokens.first().and_then(|value| connection_kind(value)) else {
            continue;
        };
        let source = tokens
            .get(1)
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| "ASCII FBX connection has an invalid source id".to_owned())?;
        let destination = tokens
            .get(2)
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| "ASCII FBX connection has an invalid destination id".to_owned())?;
        result.push(Connection {
            kind,
            source,
            destination,
            property: tokens.get(3).cloned(),
        });
    }
    Ok(result)
}

fn load_binary_connections(path: &Path) -> Result<Vec<Connection>, String> {
    let file = File::open(path).map_err(|error| format!("failed to reopen FBX: {error}"))?;
    let tree = AnyTree::from_seekable_reader(BufReader::new(file))
        .map_err(|error| format!("failed to read ordered binary FBX connections: {error}"))?;
    let AnyTree::V7400(_, tree, _) = tree else {
        return Err("binary FBX version is unsupported by the v7400 parser".to_owned());
    };
    let Some(connections) = tree.root().first_child_by_name("Connections") else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for connection in connections.children_by_name("C") {
        let attributes = connection.attributes();
        let Some(kind) = attributes
            .first()
            .and_then(binary_string)
            .and_then(connection_kind)
        else {
            continue;
        };
        let source = attributes
            .get(1)
            .and_then(binary_u64)
            .ok_or_else(|| "binary FBX connection has an invalid source id".to_owned())?;
        let destination = attributes
            .get(2)
            .and_then(binary_u64)
            .ok_or_else(|| "binary FBX connection has an invalid destination id".to_owned())?;
        result.push(Connection {
            kind,
            source,
            destination,
            property: attributes.get(3).and_then(binary_string).map(str::to_owned),
        });
    }
    Ok(result)
}

fn connection_kind(value: &str) -> Option<ConnectionKind> {
    match value {
        "OO" => Some(ConnectionKind::Object),
        "OP" => Some(ConnectionKind::ObjectProperty),
        _ => None,
    }
}

fn binary_string(value: &AttributeValue) -> Option<&str> {
    match value {
        AttributeValue::String(value) => Some(value),
        _ => None,
    }
}

fn binary_u64(value: &AttributeValue) -> Option<u64> {
    match value {
        AttributeValue::I16(value) => (*value).try_into().ok(),
        AttributeValue::I32(value) => (*value).try_into().ok(),
        AttributeValue::I64(value) => (*value).try_into().ok(),
        _ => None,
    }
}

fn binary_number(value: &AttributeValue) -> Option<f64> {
    match value {
        AttributeValue::I16(value) => Some(f64::from(*value)),
        AttributeValue::I32(value) => Some(f64::from(*value)),
        AttributeValue::I64(value) => Some(*value as f64),
        AttributeValue::F32(value) => Some(f64::from(*value)),
        AttributeValue::F64(value) => Some(*value),
        _ => None,
    }
}

fn reject_unsupported_features(objects: &[OwnedObject]) -> Result<(), String> {
    for object in objects {
        let object_type = object.type_name.as_str();
        let class = object.class_name.as_str();
        if object_type.starts_with("Animation") {
            return Err(format!(
                "animated FBX data ({object_type}) is unsupported; export the evaluated static mesh"
            ));
        }
        if object_type.eq_ignore_ascii_case("Deformer")
            || object_type.eq_ignore_ascii_case("SubDeformer")
            || class.eq_ignore_ascii_case("Skin")
            || class.eq_ignore_ascii_case("Cluster")
            || class.eq_ignore_ascii_case("BlendShape")
            || class.eq_ignore_ascii_case("BlendShapeChannel")
        {
            return Err(format!(
                "FBX skin/deformer data ({object_type}/{class}) is unsupported; apply it before export"
            ));
        }
        if object_type.eq_ignore_ascii_case("Geometry") && class.eq_ignore_ascii_case("Shape") {
            return Err(
                "FBX blend-shape geometry is unsupported; apply the blend shape before export"
                    .to_owned(),
            );
        }
    }
    Ok(())
}

fn parse_geometry(object: &OwnedObject) -> Result<RawGeometry, String> {
    let vertices = object_attribute(object, "Vertices")
        .ok_or_else(|| format!("FBX mesh {} has no Vertices array", object.object_index))?;
    let vertex_values = parse_f64_array(vertices, "Vertices")?;
    if vertex_values.is_empty() || vertex_values.len() % 3 != 0 {
        return Err(format!(
            "FBX mesh {} Vertices length is not a nonzero multiple of three",
            object.object_index
        ));
    }
    let control_points = vertex_values
        .chunks_exact(3)
        .map(|value| [value[0], value[1], value[2]])
        .collect::<Vec<_>>();
    if !control_points
        .iter()
        .flatten()
        .all(|value| value.is_finite())
    {
        return Err(format!(
            "FBX mesh {} contains a non-finite control point",
            object.object_index
        ));
    }

    let polygon_indices = object_attribute(object, "PolygonVertexIndex").ok_or_else(|| {
        format!(
            "FBX mesh {} has no PolygonVertexIndex array",
            object.object_index
        )
    })?;
    let polygon_indices = parse_i64_array(polygon_indices, "PolygonVertexIndex")?;
    let mut polygon_control_points = Vec::<Vec<usize>>::new();
    let mut current = Vec::new();
    for value in polygon_indices {
        let end = value < 0;
        let decoded = if end {
            value
                .checked_neg()
                .and_then(|value| value.checked_sub(1))
                .ok_or_else(|| "FBX polygon index underflow".to_owned())?
        } else {
            value
        };
        let index = usize::try_from(decoded).map_err(|_| {
            format!(
                "FBX mesh {} has a negative polygon index",
                object.object_index
            )
        })?;
        if index >= control_points.len() {
            return Err(format!(
                "FBX mesh {} polygon index {index} exceeds {} control points",
                object.object_index,
                control_points.len()
            ));
        }
        current.push(index);
        if end {
            if current.len() < 3 {
                return Err(format!(
                    "FBX mesh {} contains a polygon with fewer than three corners",
                    object.object_index
                ));
            }
            polygon_control_points.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        return Err(format!(
            "FBX mesh {} has an unterminated polygon index list",
            object.object_index
        ));
    }
    if polygon_control_points.is_empty() {
        return Err(format!(
            "FBX mesh {} contains no polygons",
            object.object_index
        ));
    }

    let corner_control_points = polygon_control_points
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    let corner_uvs = parse_uvs(object, &corner_control_points, control_points.len())?;
    let polygon_materials = parse_polygon_materials(object, polygon_control_points.len())?;
    let mut corner_offset = 0_usize;
    let polygons = polygon_control_points
        .into_iter()
        .enumerate()
        .map(|(polygon_index, control_points)| {
            let end = corner_offset + control_points.len();
            let uvs = corner_uvs[corner_offset..end].to_vec();
            corner_offset = end;
            Polygon {
                control_points,
                uvs,
                material: polygon_materials[polygon_index],
            }
        })
        .collect();

    Ok(RawGeometry {
        id: object.object_index,
        name: object.name.clone(),
        control_points,
        polygons,
    })
}

fn parse_uvs(
    object: &OwnedObject,
    corner_control_points: &[usize],
    control_point_count: usize,
) -> Result<Vec<Option<[f64; 2]>>, String> {
    let Some(layer) = object_attribute(object, "LayerElementUV") else {
        return Ok(vec![None; corner_control_points.len()]);
    };
    let children = layer.get_children_distinct();
    let mapping = child_token(&children, "MappingInformationType")
        .ok_or_else(|| "FBX UV layer has no MappingInformationType".to_owned())?;
    let reference = child_token(&children, "ReferenceInformationType")
        .ok_or_else(|| "FBX UV layer has no ReferenceInformationType".to_owned())?;
    let uv_attribute = child_attribute(&children, "UV")
        .ok_or_else(|| "FBX UV layer has no UV array".to_owned())?;
    let uv_values = parse_f64_array(uv_attribute, "UV")?;
    if uv_values.len() % 2 != 0 {
        return Err("FBX UV array length is not divisible by two".to_owned());
    }
    let direct = uv_values
        .chunks_exact(2)
        .map(|value| [value[0], value[1]])
        .collect::<Vec<_>>();
    let indices = if reference.eq_ignore_ascii_case("IndexToDirect") {
        let index_attribute = child_attribute(&children, "UVIndex")
            .ok_or_else(|| "FBX IndexToDirect UV layer has no UVIndex array".to_owned())?;
        Some(parse_i64_array(index_attribute, "UVIndex")?)
    } else if reference.eq_ignore_ascii_case("Direct") {
        None
    } else {
        return Err(format!(
            "FBX UV reference mode {reference:?} is unsupported; use Direct or IndexToDirect"
        ));
    };

    let by_control_point = mapping.eq_ignore_ascii_case("ByControlPoint")
        || mapping.eq_ignore_ascii_case("ByVertice")
        || mapping.eq_ignore_ascii_case("ByVertex");
    let by_polygon_vertex = mapping.eq_ignore_ascii_case("ByPolygonVertex");
    if !by_control_point && !by_polygon_vertex {
        return Err(format!(
            "FBX UV mapping mode {mapping:?} is unsupported; use ByControlPoint or ByPolygonVertex"
        ));
    }
    if by_control_point && control_point_count == 0 {
        return Err("FBX ByControlPoint UV layer has no control points".to_owned());
    }

    corner_control_points
        .iter()
        .enumerate()
        .map(|(corner_index, &control_point)| {
            let mapped = if by_control_point {
                control_point
            } else {
                corner_index
            };
            let direct_index = if let Some(indices) = &indices {
                let index = *indices.get(mapped).ok_or_else(|| {
                    format!("FBX UV index array has no entry for mapped index {mapped}")
                })?;
                usize::try_from(index).map_err(|_| format!("FBX UV index {index} is negative"))?
            } else {
                mapped
            };
            direct
                .get(direct_index)
                .copied()
                .map(Some)
                .ok_or_else(|| format!("FBX UV direct index {direct_index} is out of range"))
        })
        .collect()
}

fn parse_polygon_materials(
    object: &OwnedObject,
    polygon_count: usize,
) -> Result<Vec<Option<i32>>, String> {
    let Some(layer) = object_attribute(object, "LayerElementMaterial") else {
        return Ok(vec![None; polygon_count]);
    };
    let children = layer.get_children_distinct();
    let mapping = child_token(&children, "MappingInformationType")
        .ok_or_else(|| "FBX material layer has no MappingInformationType".to_owned())?;
    let reference = child_token(&children, "ReferenceInformationType")
        .ok_or_else(|| "FBX material layer has no ReferenceInformationType".to_owned())?;
    if !reference.eq_ignore_ascii_case("Direct") && !reference.eq_ignore_ascii_case("IndexToDirect")
    {
        return Err(format!(
            "FBX material reference mode {reference:?} is unsupported"
        ));
    }
    let materials = child_attribute(&children, "Materials")
        .ok_or_else(|| "FBX material layer has no Materials array".to_owned())?;
    let materials = parse_i64_array(materials, "Materials")?
        .into_iter()
        .map(|value| {
            i32::try_from(value).map_err(|_| format!("FBX material index {value} exceeds i32"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if mapping.eq_ignore_ascii_case("AllSame") {
        let material = materials.first().copied().unwrap_or(-1);
        Ok(vec![Some(material); polygon_count])
    } else if mapping.eq_ignore_ascii_case("ByPolygon") {
        if materials.len() != polygon_count {
            return Err(format!(
                "FBX ByPolygon material layer declares {} indices for {polygon_count} polygons",
                materials.len()
            ));
        }
        Ok(materials.into_iter().map(Some).collect())
    } else {
        Err(format!(
            "FBX material mapping mode {mapping:?} is unsupported; use AllSame or ByPolygon"
        ))
    }
}

fn object_attribute<'a>(object: &'a OwnedObject, name: &str) -> Option<&'a ElementAttribute> {
    object
        .attributes
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value)
}

fn child_attribute<'a>(
    children: &'a HashMap<String, ElementAttribute>,
    name: &str,
) -> Option<&'a ElementAttribute> {
    children
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value)
}

fn child_token(children: &HashMap<String, ElementAttribute>, name: &str) -> Option<String> {
    child_attribute(children, name)
        .and_then(|attribute| attribute.get_tokens().first())
        .map(|token| token.trim().trim_matches(['"', '\'']).to_owned())
}

fn payload_tokens(attribute: &ElementAttribute) -> Vec<String> {
    let children = attribute.get_children_distinct();
    child_attribute(&children, "a")
        .unwrap_or(attribute)
        .get_tokens()
        .to_vec()
}

fn parse_f64_array(attribute: &ElementAttribute, name: &str) -> Result<Vec<f64>, String> {
    payload_tokens(attribute)
        .iter()
        .flat_map(|token| token.split(','))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| {
            token
                .parse::<f64>()
                .map_err(|error| format!("FBX {name} contains invalid float {token:?}: {error}"))
        })
        .collect()
}

fn parse_i64_array(attribute: &ElementAttribute, name: &str) -> Result<Vec<i64>, String> {
    payload_tokens(attribute)
        .iter()
        .flat_map(|token| token.split(','))
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| {
            token
                .parse::<i64>()
                .map_err(|error| format!("FBX {name} contains invalid integer {token:?}: {error}"))
        })
        .collect()
}

fn write_material_library(
    source_root: &Path,
    destination_root: &Path,
    objects: &[OwnedObject],
    objects_by_id: &HashMap<u64, &OwnedObject>,
    connections: &[Connection],
) -> Result<Appearance, String> {
    let mut materials = objects
        .iter()
        .filter(|object| object.type_name.eq_ignore_ascii_case("Material"))
        .map(material_info)
        .collect::<Vec<_>>();
    materials.sort_by_key(|material| material.id);
    let names_by_id = materials
        .iter()
        .map(|material| (material.id, material.name.clone()))
        .collect::<HashMap<_, _>>();
    let mut material_names = materials
        .iter()
        .map(|material| material.name.clone())
        .collect::<Vec<_>>();
    material_names.push(DEFAULT_MATERIAL.to_owned());

    fs::create_dir_all(destination_root.join("textures"))
        .map_err(|error| format!("failed to create FBX appearance workspace: {error}"))?;
    let canonical_source_root = source_root.canonicalize().map_err(|error| {
        format!(
            "failed to resolve the FBX appearance directory {}: {error}",
            source_root.display()
        )
    })?;
    let mut copied = HashMap::<PathBuf, PathBuf>::new();
    let mut mtl = String::from("# Vkit native FBX material bridge\n");
    for material in &materials {
        writeln!(mtl, "newmtl {}", material.name).unwrap();
        writeln!(
            mtl,
            "Kd {} {} {}\nd {}",
            material.diffuse[0], material.diffuse[1], material.diffuse[2], material.opacity
        )
        .unwrap();
        if let Some(texture) = diffuse_texture(material.id, objects_by_id, connections)
            && let Some(source) = resolve_texture_path(texture, &canonical_source_root)
        {
            let bytes = source
                .metadata()
                .map_err(|error| {
                    format!(
                        "failed to inspect FBX texture {}: {error}",
                        source.display()
                    )
                })?
                .len();
            if bytes > MAX_TEXTURE_BYTES {
                return Err(format!(
                    "FBX texture {} is {bytes} bytes; the native texture limit is {MAX_TEXTURE_BYTES} bytes",
                    source.display()
                ));
            }
            let relative = if let Some(relative) = copied.get(&source) {
                relative.clone()
            } else {
                let extension = source
                    .extension()
                    .and_then(|value| value.to_str())
                    .filter(|value| value.chars().all(|ch| ch.is_ascii_alphanumeric()))
                    .unwrap_or("bin");
                let relative = PathBuf::from(format!(
                    "textures/fbx_{}_{}.{}",
                    material.id,
                    safe_label(
                        source
                            .file_stem()
                            .and_then(|value| value.to_str())
                            .unwrap_or("texture"),
                        "texture"
                    ),
                    extension.to_ascii_lowercase()
                ));
                fs::copy(&source, destination_root.join(&relative)).map_err(|error| {
                    format!(
                        "failed to preserve FBX texture {}: {error}",
                        source.display()
                    )
                })?;
                copied.insert(source, relative.clone());
                relative
            };
            writeln!(
                mtl,
                "map_Kd {}",
                relative.to_string_lossy().replace('\\', "/")
            )
            .unwrap();
        }
    }
    writeln!(mtl, "newmtl {DEFAULT_MATERIAL}\nKd 1 1 1\nd 1").unwrap();
    fs::write(destination_root.join("vkit-import.mtl"), mtl)
        .map_err(|error| format!("failed to write FBX material library: {error}"))?;

    Ok(Appearance {
        material_names,
        names_by_id,
    })
}

fn material_info(object: &OwnedObject) -> MaterialInfo {
    let mut diffuse = vec3_property(object, "BaseColor")
        .or_else(|| vec3_property(object, "DiffuseColor"))
        .unwrap_or([1.0, 1.0, 1.0]);
    let factor = float_property(object, "DiffuseFactor").unwrap_or(1.0);
    for channel in &mut diffuse {
        *channel = (*channel * factor).clamp(0.0, 1.0);
    }
    let opacity = float_property(object, "Opacity")
        .or_else(|| float_property(object, "TransparencyFactor").map(|value| 1.0 - value))
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    MaterialInfo {
        id: object.object_index,
        name: format!(
            "FBX_{}_{}",
            object.object_index,
            safe_label(strip_namespace(&object.name), "Material")
        ),
        diffuse,
        opacity,
    }
}

fn diffuse_texture<'a>(
    material_id: u64,
    objects_by_id: &HashMap<u64, &'a OwnedObject>,
    connections: &[Connection],
) -> Option<&'a OwnedObject> {
    connections
        .iter()
        .filter(|connection| {
            connection.kind == ConnectionKind::ObjectProperty
                && connection.destination == material_id
                && connection.property.as_deref().is_some_and(|property| {
                    matches_ignore_ascii_case(
                        property,
                        &["DiffuseColor", "BaseColor", "Diffuse", "BaseColorMap"],
                    )
                })
        })
        .find_map(|connection| {
            objects_by_id
                .get(&connection.source)
                .copied()
                .filter(|object| object.type_name.eq_ignore_ascii_case("Texture"))
        })
}

fn resolve_texture_path(texture: &OwnedObject, source_root: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(relative) = attribute_token(texture, "RelativeFilename") {
        candidates.push(relative);
    }
    if let Some(file_name) = attribute_token(texture, "FileName") {
        candidates.push(file_name);
    }
    for raw in &candidates {
        let path = Path::new(raw);
        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else if path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        {
            source_root.join(path)
        } else {
            continue;
        };
        if let Ok(candidate) = candidate.canonicalize()
            && candidate.is_file()
            && candidate.starts_with(source_root)
        {
            return Some(candidate);
        }
    }
    candidates.iter().find_map(|raw| {
        let file_name = Path::new(raw).file_name()?;
        let candidate = source_root.join(file_name).canonicalize().ok()?;
        (candidate.is_file() && candidate.starts_with(source_root)).then_some(candidate)
    })
}

fn attribute_token(object: &OwnedObject, name: &str) -> Option<String> {
    object_attribute(object, name)
        .and_then(|attribute| attribute.get_tokens().first())
        .map(|value| value.trim().trim_matches(['"', '\'']).to_owned())
        .filter(|value| !value.is_empty())
}

fn matches_ignore_ascii_case(value: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

fn material_slots(
    model_id: u64,
    connections: &[Connection],
    names_by_id: &HashMap<u64, String>,
) -> Vec<String> {
    connections
        .iter()
        .filter(|connection| {
            connection.kind == ConnectionKind::Object
                && connection.destination == model_id
                && names_by_id.contains_key(&connection.source)
        })
        .filter_map(|connection| names_by_id.get(&connection.source).cloned())
        .collect()
}

fn model_parents(
    connections: &[Connection],
    model_ids: &HashSet<u64>,
) -> Result<HashMap<u64, u64>, String> {
    let mut parents = HashMap::new();
    for connection in connections {
        if connection.kind == ConnectionKind::Object
            && model_ids.contains(&connection.source)
            && model_ids.contains(&connection.destination)
            && let Some(existing) = parents.insert(connection.source, connection.destination)
            && existing != connection.destination
        {
            return Err(format!(
                "FBX model {} has multiple hierarchy parents",
                connection.source
            ));
        }
    }
    Ok(parents)
}

fn resolve_world_matrix(
    model_id: u64,
    objects: &HashMap<u64, &OwnedObject>,
    parents: &HashMap<u64, u64>,
    worlds: &mut HashMap<u64, ResolvedTransform>,
    visiting: &mut HashSet<u64>,
    depth: usize,
) -> Result<ResolvedTransform, String> {
    if let Some(matrix) = worlds.get(&model_id) {
        return Ok(*matrix);
    }
    if depth > MAX_HIERARCHY_DEPTH {
        return Err(format!(
            "FBX model hierarchy exceeds {MAX_HIERARCHY_DEPTH} levels"
        ));
    }
    if !visiting.insert(model_id) {
        return Err(format!(
            "FBX model hierarchy contains a cycle at {model_id}"
        ));
    }
    let model = objects
        .get(&model_id)
        .copied()
        .ok_or_else(|| format!("FBX hierarchy references missing model {model_id}"))?;
    let local = local_transform(model)?;
    let world = if let Some(parent) = parents.get(&model_id) {
        let parent = resolve_world_matrix(*parent, objects, parents, worlds, visiting, depth + 1)?;
        inherit_transform(parent, local)?
    } else {
        ResolvedTransform {
            matrix: local.matrix,
            rotation: local.rotation,
            scale: local.scale,
        }
    };
    visiting.remove(&model_id);
    if !world.matrix.iter().flatten().all(|value| value.is_finite()) {
        return Err(format!("FBX model {model_id} has a non-finite transform"));
    }
    worlds.insert(model_id, world);
    Ok(world)
}

fn local_transform(model: &OwnedObject) -> Result<LocalTransform, String> {
    let inheritance = int_property(model, "InheritType").unwrap_or(0);
    if !(0..=2).contains(&inheritance) {
        return Err(format!(
            "FBX model {} uses invalid transform inheritance type {inheritance}",
            model.object_index
        ));
    }
    let local_rotation = vec3_property(model, "Lcl Rotation").unwrap_or([0.0, 0.0, 0.0]);
    let local_scaling = vec3_property(model, "Lcl Scaling").unwrap_or([1.0, 1.0, 1.0]);
    let pre_rotation = vec3_property(model, "PreRotation").unwrap_or([0.0, 0.0, 0.0]);
    let post_rotation = vec3_property(model, "PostRotation").unwrap_or([0.0, 0.0, 0.0]);
    let rotation_order = int_property(model, "RotationOrder").unwrap_or(0);
    let rotation = multiply(
        multiply(
            euler_rotation(pre_rotation, rotation_order)?,
            euler_rotation(local_rotation, rotation_order)?,
        ),
        transpose_rotation(euler_rotation(post_rotation, rotation_order)?),
    );
    let scale = scaling(local_scaling);
    let matrix = transform_stack(
        FbxTransform {
            local_translation: vec3_property(model, "Lcl Translation").unwrap_or([0.0, 0.0, 0.0]),
            local_rotation,
            local_scaling,
            rotation_offset: vec3_property(model, "RotationOffset").unwrap_or([0.0, 0.0, 0.0]),
            rotation_pivot: vec3_property(model, "RotationPivot").unwrap_or([0.0, 0.0, 0.0]),
            scaling_offset: vec3_property(model, "ScalingOffset").unwrap_or([0.0, 0.0, 0.0]),
            scaling_pivot: vec3_property(model, "ScalingPivot").unwrap_or([0.0, 0.0, 0.0]),
            pre_rotation,
            post_rotation,
        },
        rotation_order,
    )?;
    Ok(LocalTransform {
        matrix,
        rotation,
        scale,
        inheritance,
    })
}

fn inherit_transform(
    parent: ResolvedTransform,
    local: LocalTransform,
) -> Result<ResolvedTransform, String> {
    let rotation = multiply(parent.rotation, local.rotation);
    let scale = match local.inheritance {
        0 | 1 => multiply(parent.scale, local.scale),
        2 => local.scale,
        _ => {
            return Err(format!(
                "FBX transform inheritance {} is invalid",
                local.inheritance
            ));
        }
    };
    let linear = match local.inheritance {
        0 => multiply(rotation, scale),
        1 => multiply(
            multiply(multiply(parent.rotation, parent.scale), local.rotation),
            local.scale,
        ),
        2 => multiply(rotation, local.scale),
        _ => unreachable!(),
    };
    let local_origin = [local.matrix[0][3], local.matrix[1][3], local.matrix[2][3]];
    let translation = transform_point(parent.matrix, local_origin);
    let mut matrix = linear;
    matrix[0][3] = translation[0];
    matrix[1][3] = translation[1];
    matrix[2][3] = translation[2];
    matrix[3] = [0.0, 0.0, 0.0, 1.0];
    Ok(ResolvedTransform {
        matrix,
        rotation,
        scale,
    })
}

fn geometric_transform(model: &OwnedObject) -> Result<Matrix, String> {
    let order = int_property(model, "RotationOrder").unwrap_or(0);
    Ok(multiply(
        multiply(
            translation(vec3_property(model, "GeometricTranslation").unwrap_or([0.0; 3])),
            euler_rotation(
                vec3_property(model, "GeometricRotation").unwrap_or([0.0; 3]),
                order,
            )?,
        ),
        scaling(vec3_property(model, "GeometricScaling").unwrap_or([1.0; 3])),
    ))
}

#[derive(Clone, Copy, Debug, Default)]
struct FbxTransform {
    local_translation: [f64; 3],
    local_rotation: [f64; 3],
    local_scaling: [f64; 3],
    rotation_offset: [f64; 3],
    rotation_pivot: [f64; 3],
    scaling_offset: [f64; 3],
    scaling_pivot: [f64; 3],
    pre_rotation: [f64; 3],
    post_rotation: [f64; 3],
}

fn transform_stack(stack: FbxTransform, rotation_order: i32) -> Result<Matrix, String> {
    let FbxTransform {
        local_translation,
        local_rotation,
        local_scaling,
        rotation_offset,
        rotation_pivot,
        scaling_offset,
        scaling_pivot,
        pre_rotation,
        post_rotation,
    } = stack;
    let matrices = [
        translation(local_translation),
        translation(rotation_offset),
        translation(rotation_pivot),
        euler_rotation(pre_rotation, rotation_order)?,
        euler_rotation(local_rotation, rotation_order)?,
        transpose_rotation(euler_rotation(post_rotation, rotation_order)?),
        translation(negate(rotation_pivot)),
        translation(scaling_offset),
        translation(scaling_pivot),
        scaling(local_scaling),
        translation(negate(scaling_pivot)),
    ];
    Ok(matrices.into_iter().fold(identity(), multiply))
}

fn vec3_property(object: &OwnedObject, name: &str) -> Option<[f64; 3]> {
    property(object, name).and_then(|property| match property {
        Property::Vec3(value) => Some(value.map(f64::from)),
        _ => None,
    })
}

fn float_property(object: &OwnedObject, name: &str) -> Option<f64> {
    property(object, name).and_then(|property| match property {
        Property::Float(value) => Some(f64::from(*value)),
        _ => None,
    })
}

fn int_property(object: &OwnedObject, name: &str) -> Option<i32> {
    property(object, name).and_then(|property| match property {
        Property::Int(value) => Some(*value),
        _ => None,
    })
}

fn property<'a>(object: &'a OwnedObject, name: &str) -> Option<&'a Property> {
    object
        .properties
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value)
}

fn emit_geometry(
    geometry: &RawGeometry,
    transform: Matrix,
    group: &str,
    material_slots: &[String],
    source_vertex_base: &mut u64,
    builder: &mut AttributeMeshBuilder,
) -> Result<(), String> {
    let transformed = geometry
        .control_points
        .iter()
        .map(|&point| transform_point(transform, point))
        .collect::<Vec<_>>();
    if !transformed.iter().flatten().all(|value| value.is_finite()) {
        return Err(format!(
            "FBX geometry {} produces non-finite transformed positions",
            geometry.id
        ));
    }
    let mirrored = linear_determinant(transform) < 0.0;
    for (polygon_index, polygon) in geometry.polygons.iter().enumerate() {
        let points = polygon
            .control_points
            .iter()
            .map(|&index| transformed[index])
            .collect::<Vec<_>>();
        let triangles = triangulate_polygon(&points).map_err(|error| {
            format!(
                "FBX geometry {} polygon {polygon_index} could not be triangulated: {error}",
                geometry.id
            )
        })?;
        let material = match polygon.material {
            Some(index) if index >= 0 => material_slots
                .get(index as usize)
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "FBX geometry {} polygon {polygon_index} references material slot {index}, but its model has {} slots",
                        geometry.id,
                        material_slots.len()
                    )
                })?,
            _ if material_slots.len() == 1 => material_slots[0].clone(),
            _ => DEFAULT_MATERIAL.to_owned(),
        };
        let labels = FaceLabels {
            group: Some(group.to_owned()),
            material: Some(material),
        };
        for mut triangle in triangles {
            if mirrored {
                triangle.swap(1, 2);
            }
            let corners = triangle.map(|corner| {
                let control_point = polygon.control_points[corner];
                Corner {
                    source_vertex: *source_vertex_base + control_point as u64,
                    position: transformed[control_point],
                    uv: polygon.uvs[corner],
                }
            });
            builder.add_triangle(labels.clone(), corners)?;
        }
    }
    *source_vertex_base = source_vertex_base
        .checked_add(geometry.control_points.len() as u64)
        .ok_or_else(|| "FBX source vertex identity overflow".to_owned())?;
    Ok(())
}

fn triangulate_polygon(points: &[[f64; 3]]) -> Result<Vec<[usize; 3]>, String> {
    if points.len() < 3 {
        return Err("polygon has fewer than three corners".to_owned());
    }
    if points.len() == 3 {
        return Ok(vec![[0, 1, 2]]);
    }
    let mut normal = [0.0; 3];
    for index in 0..points.len() {
        let current = points[index];
        let next = points[(index + 1) % points.len()];
        normal[0] += (current[1] - next[1]) * (current[2] + next[2]);
        normal[1] += (current[2] - next[2]) * (current[0] + next[0]);
        normal[2] += (current[0] - next[0]) * (current[1] + next[1]);
    }
    let drop_axis = normal
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.abs().total_cmp(&right.abs()))
        .map(|(index, _)| index)
        .unwrap_or(2);
    if normal[drop_axis].abs() <= 1.0e-14 {
        return Err("polygon normal is degenerate".to_owned());
    }
    let projected = points
        .iter()
        .map(|point| match drop_axis {
            0 => [point[1], point[2]],
            1 => [point[0], point[2]],
            _ => [point[0], point[1]],
        })
        .collect::<Vec<_>>();
    let area = (0..projected.len())
        .map(|index| {
            let current = projected[index];
            let next = projected[(index + 1) % projected.len()];
            current[0] * next[1] - next[0] * current[1]
        })
        .sum::<f64>();
    let orientation = area.signum();
    if orientation == 0.0 {
        return Err("polygon projected area is zero".to_owned());
    }

    let mut remaining = (0..points.len()).collect::<Vec<_>>();
    let mut triangles = Vec::with_capacity(points.len() - 2);
    while remaining.len() > 3 {
        let mut ear = None;
        for position in 0..remaining.len() {
            let previous = remaining[(position + remaining.len() - 1) % remaining.len()];
            let current = remaining[position];
            let next = remaining[(position + 1) % remaining.len()];
            let cross = cross_2d(projected[previous], projected[current], projected[next]);
            if cross * orientation <= 1.0e-14 {
                continue;
            }
            if remaining.iter().copied().any(|candidate| {
                candidate != previous
                    && candidate != current
                    && candidate != next
                    && point_in_triangle(
                        projected[candidate],
                        projected[previous],
                        projected[current],
                        projected[next],
                        orientation,
                    )
            }) {
                continue;
            }
            ear = Some((position, [previous, current, next]));
            break;
        }
        let Some((position, triangle)) = ear else {
            return Err("polygon is self-intersecting or numerically degenerate".to_owned());
        };
        triangles.push(triangle);
        remaining.remove(position);
    }
    triangles.push([remaining[0], remaining[1], remaining[2]]);
    Ok(triangles)
}

fn cross_2d(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn point_in_triangle(
    point: [f64; 2],
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
    orientation: f64,
) -> bool {
    cross_2d(a, b, point) * orientation >= -1.0e-14
        && cross_2d(b, c, point) * orientation >= -1.0e-14
        && cross_2d(c, a, point) * orientation >= -1.0e-14
}

fn strip_namespace(value: &str) -> &str {
    value.rsplit("::").next().unwrap_or(value)
}

fn safe_label(value: &str, fallback: &str) -> String {
    let result = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if result.is_empty() {
        fallback.to_owned()
    } else {
        result
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
    for row in 0..4 {
        for column in 0..4 {
            result[row][column] = (0..4)
                .map(|inner| left[row][inner] * right[inner][column])
                .sum();
        }
    }
    result
}

fn translation(value: [f64; 3]) -> Matrix {
    let mut result = identity();
    result[0][3] = value[0];
    result[1][3] = value[1];
    result[2][3] = value[2];
    result
}

fn scaling(value: [f64; 3]) -> Matrix {
    [
        [value[0], 0.0, 0.0, 0.0],
        [0.0, value[1], 0.0, 0.0],
        [0.0, 0.0, value[2], 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

fn euler_rotation(degrees: [f64; 3], order: i32) -> Result<Matrix, String> {
    let axes = match order {
        0 => [0, 1, 2],
        1 => [0, 2, 1],
        2 => [1, 2, 0],
        3 => [1, 0, 2],
        4 => [2, 0, 1],
        5 => [2, 1, 0],
        6 => return Err("FBX spherical Euler rotation is unsupported".to_owned()),
        _ => return Err(format!("FBX rotation order {order} is invalid")),
    };
    let radians = degrees.map(f64::to_radians);
    let mut result = identity();
    for axis in axes {
        let (sin, cos) = radians[axis].sin_cos();
        let rotation = match axis {
            0 => [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, cos, -sin, 0.0],
                [0.0, sin, cos, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            1 => [
                [cos, 0.0, sin, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [-sin, 0.0, cos, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            _ => [
                [cos, -sin, 0.0, 0.0],
                [sin, cos, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        };
        result = multiply(rotation, result);
    }
    Ok(result)
}

fn transpose_rotation(matrix: Matrix) -> Matrix {
    let mut result = identity();
    for row in 0..3 {
        for column in 0..3 {
            result[row][column] = matrix[column][row];
        }
    }
    result
}

fn negate(value: [f64; 3]) -> [f64; 3] {
    [-value[0], -value[1], -value[2]]
}

fn transform_point(matrix: Matrix, point: [f64; 3]) -> [f64; 3] {
    [
        matrix[0][0] * point[0] + matrix[0][1] * point[1] + matrix[0][2] * point[2] + matrix[0][3],
        matrix[1][0] * point[0] + matrix[1][1] * point[1] + matrix[1][2] * point[2] + matrix[1][3],
        matrix[2][0] * point[0] + matrix[2][1] * point[1] + matrix[2][2] * point[2] + matrix[2][3],
    ]
}

fn linear_determinant(matrix: Matrix) -> f64 {
    let a = matrix[0][0];
    let b = matrix[0][1];
    let c = matrix[0][2];
    let d = matrix[1][0];
    let e = matrix[1][1];
    let f = matrix[1][2];
    let g = matrix[2][0];
    let h = matrix[2][1];
    let i = matrix[2][2];
    a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g)
}

#[cfg(test)]
mod tests {
    use super::{
        axis_units_from_properties, euler_rotation, linear_determinant, load_ordered_template_fbx,
        multiply, reconstruct_template_anatomy_groups, scaling, transform_point, translation,
        triangulate_polygon,
    };
    use std::{collections::HashMap, fs};
    use vkit_core::formats::{ObjFace, OrderedObjMesh};

    fn axis_properties(values: [f64; 7]) -> HashMap<String, f64> {
        [
            "UpAxis",
            "UpAxisSign",
            "FrontAxis",
            "FrontAxisSign",
            "CoordAxis",
            "CoordAxisSign",
            "UnitScaleFactor",
        ]
        .into_iter()
        .zip(values)
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
    }

    #[test]
    fn transform_composition_and_mirror_are_stable() {
        let matrix = multiply(translation([3.0, 4.0, 5.0]), scaling([-2.0, 3.0, 4.0]));
        assert_eq!(transform_point(matrix, [1.0, 1.0, 1.0]), [1.0, 7.0, 9.0]);
        assert!(linear_determinant(matrix) < 0.0);
        assert!(euler_rotation([0.0, 90.0, 0.0], 0).is_ok());
    }

    #[test]
    fn concave_polygon_triangulation_is_deterministic() {
        let points = [
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [2.0, 2.0, 0.0],
            [0.0, 2.0, 0.0],
        ];
        let first = triangulate_polygon(&points).unwrap();
        let second = triangulate_polygon(&points).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 3);
    }

    #[test]
    fn global_settings_apply_z_up_millimetres_exactly_once() {
        let settings =
            axis_units_from_properties(&axis_properties([2.0, 1.0, 1.0, -1.0, 0.0, 1.0, 0.1]))
                .unwrap();
        assert_eq!(
            transform_point(settings.source_to_y_up_cm(), [10.0, 20.0, 30.0]),
            [1.0, 3.0, 2.0]
        );

        let standard =
            axis_units_from_properties(&axis_properties([1.0, 1.0, 2.0, -1.0, 0.0, 1.0, 1.0]))
                .unwrap();
        assert_eq!(
            transform_point(standard.source_to_y_up_cm(), [1.0, 2.0, 3.0]),
            [1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn incomplete_or_ambiguous_global_settings_are_rejected() {
        let mut incomplete = axis_properties([1.0, 1.0, 2.0, -1.0, 0.0, 1.0, 1.0]);
        incomplete.remove("UnitScaleFactor");
        assert!(axis_units_from_properties(&incomplete).is_err());

        let duplicate_axis = axis_properties([1.0, 1.0, 1.0, -1.0, 0.0, 1.0, 1.0]);
        assert!(axis_units_from_properties(&duplicate_axis).is_err());
    }

    #[test]
    fn ordered_template_fbx_keeps_a_control_point_quad_and_labels() {
        let workspace = tempfile::tempdir().expect("fixture workspace");
        let path = workspace.path().join("ordered-template.fbx");
        fs::write(
            &path,
            include_str!("fixtures/ordered_template_quad_ascii.fbx"),
        )
        .expect("fixture write");

        let (mesh, labels) = load_ordered_template_fbx(&path).expect("ordered template FBX");
        assert_eq!(
            mesh.vertices,
            [
                [0.0, 2.0, 0.0],
                [1.0, 2.0, 0.0],
                [1.0, 3.0, 0.0],
                [0.0, 3.0, 0.0]
            ]
        );
        assert_eq!(mesh.faces.len(), 1);
        assert_eq!(mesh.faces[0].vertex_indices, [0, 1, 2, 3]);
        assert_eq!(mesh.faces[0].group.as_deref(), Some("lEye"));
        assert_eq!(mesh.faces[0].material.as_deref(), Some("Face"));
        assert_eq!(labels, ["Geometry::lEye", "Model::Genesis2Male"]);
    }

    fn anatomy_fixture() -> OrderedObjMesh {
        let specifications = [
            ([2.0, 8.0, 0.0], "Sclera", "Geometry"),
            ([-2.0, 8.0, 0.0], "Cornea", "Geometry"),
            ([0.0, 3.0, 0.0], "Teeth", "Geometry"),
            ([0.0, 5.0, 0.0], "Gums", "Geometry"),
            ([0.0, 4.0, 0.0], "InnerMouth", "Geometry"),
            ([0.0, 4.0, 0.5], "Tongue", "Geometry"),
            ([0.0, 7.0, 0.0], "Face", "LOWERJAW"),
            ([0.0, 7.0, 0.5], "Face", "Geometry"),
        ];
        let mut vertices = Vec::new();
        let mut faces = Vec::new();
        for (center, material, group) in specifications {
            let start = vertices.len() as u32;
            vertices.extend([
                [center[0] - 0.1, center[1] - 0.1, center[2]],
                [center[0] + 0.1, center[1] - 0.1, center[2]],
                [center[0], center[1] + 0.2, center[2]],
            ]);
            faces.push(ObjFace {
                vertex_indices: vec![start, start + 1, start + 2],
                group: Some(group.to_owned()),
                material: Some(material.to_owned()),
            });
        }
        OrderedObjMesh { vertices, faces }
    }

    #[test]
    fn template_fbx_anatomy_reconstruction_is_complete_and_preserves_authored_groups() {
        let mut mesh = anatomy_fixture();
        reconstruct_template_anatomy_groups(&mut mesh).unwrap();
        let groups = mesh
            .faces
            .iter()
            .map(|face| face.group.as_deref().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(groups[0], "lEye");
        assert_eq!(groups[1], "rEye");
        assert_eq!(groups[2], "lowerJaw");
        assert_eq!(groups[3], "upperJaw");
        assert_eq!(groups[4], "upperJaw");
        assert_eq!(groups[5], "tongue");
        assert_eq!(groups[6], "LOWERJAW");
        assert_eq!(groups[7], "head");
    }

    #[test]
    fn template_fbx_anatomy_reconstruction_rejects_anunsafe_jaw_partition_atomically() {
        let mut mesh = anatomy_fixture();
        for face_index in [2, 3, 4] {
            for &vertex_index in &mesh.faces[face_index].vertex_indices {
                mesh.vertices[vertex_index as usize][1] = 4.0;
            }
        }
        let before = mesh.faces.clone();
        let error = reconstruct_template_anatomy_groups(&mut mesh).unwrap_err();
        assert!(error.contains("no vertical span"), "{error}");
        assert_eq!(mesh.faces, before);
    }
}
