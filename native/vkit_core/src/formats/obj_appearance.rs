use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::path::{Path, PathBuf};

use super::mtl::validate_local_asset_path;
use super::{FormatError, Mesh, OrderedObjMesh, Result, parse_ordered_obj};

#[derive(Clone, Debug, PartialEq)]
pub struct ObjAppearance {
    pub texcoords: Vec<[f64; 2]>,

    pub face_texcoord_indices: Vec<Vec<Option<u32>>>,

    pub material_libraries: Vec<PathBuf>,

    pub material_names: Vec<String>,
}

impl ObjAppearance {
    pub fn validate_against(&self, geometry: &OrderedObjMesh) -> Result<()> {
        geometry.validate()?;
        if self.face_texcoord_indices.len() != geometry.faces.len() {
            return Err(FormatError::InvalidObj {
                line: 0,
                message: format!(
                    "appearance has {} polygon streams, but geometry has {} polygons",
                    self.face_texcoord_indices.len(),
                    geometry.faces.len()
                ),
            });
        }
        for (polygon_index, (face, corner_texcoords)) in geometry
            .faces
            .iter()
            .zip(&self.face_texcoord_indices)
            .enumerate()
        {
            if corner_texcoords.len() != face.vertex_indices.len() {
                return Err(FormatError::InvalidObj {
                    line: 0,
                    message: format!(
                        "appearance polygon {polygon_index} has {} corners, but geometry has {}",
                        corner_texcoords.len(),
                        face.vertex_indices.len()
                    ),
                });
            }
            for &texcoord_index in corner_texcoords.iter().flatten() {
                if texcoord_index as usize >= self.texcoords.len() {
                    return Err(FormatError::InvalidObj {
                        line: 0,
                        message: format!(
                            "appearance polygon {polygon_index} references texture coordinate {texcoord_index}, but only {} exist",
                            self.texcoords.len()
                        ),
                    });
                }
            }
        }
        for (index, texcoord) in self.texcoords.iter().copied().enumerate() {
            if !texcoord.iter().all(|coordinate| coordinate.is_finite()) {
                return Err(FormatError::InvalidObj {
                    line: 0,
                    message: format!("texture coordinate {index} contains a non-finite value"),
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjDocument {
    pub geometry: OrderedObjMesh,
    pub appearance: ObjAppearance,
}

impl ObjDocument {
    pub fn validate(&self) -> Result<()> {
        self.appearance.validate_against(&self.geometry)
    }

    pub fn triangulated_appearance(&self) -> Result<TriangulatedObjAppearance> {
        self.validate()?;
        let mesh = self.geometry.triangulated()?;
        let triangle_count = mesh.triangles.len();
        let mut triangle_to_polygon = Vec::with_capacity(triangle_count);
        let mut triangle_texcoord_indices = Vec::with_capacity(triangle_count);
        let mut triangle_materials = Vec::with_capacity(triangle_count);

        for (polygon_index, (face, texcoords)) in self
            .geometry
            .faces
            .iter()
            .zip(&self.appearance.face_texcoord_indices)
            .enumerate()
        {
            let polygon_index =
                u32::try_from(polygon_index).map_err(|_| FormatError::InvalidObj {
                    line: 0,
                    message: "polygon count exceeds the supported u32 mapping range".to_owned(),
                })?;
            for corner in 1..face.vertex_indices.len() - 1 {
                triangle_to_polygon.push(polygon_index);
                triangle_texcoord_indices.push([
                    texcoords[0],
                    texcoords[corner],
                    texcoords[corner + 1],
                ]);
                triangle_materials.push(face.material.clone());
            }
        }

        debug_assert_eq!(triangle_to_polygon.len(), triangle_count);
        Ok(TriangulatedObjAppearance {
            mesh,
            triangle_to_polygon,
            triangle_texcoord_indices,
            triangle_materials,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TriangulatedObjAppearance {
    pub mesh: Mesh,
    pub triangle_to_polygon: Vec<u32>,
    pub triangle_texcoord_indices: Vec<[Option<u32>; 3]>,
    pub triangle_materials: Vec<Option<String>>,
}

pub fn load_obj_document(path: impl AsRef<Path>) -> Result<ObjDocument> {
    let stream = BufReader::new(File::open(path)?);
    parse_obj_document(stream)
}

pub fn write_obj_document(mut writer: impl Write, document: &ObjDocument) -> Result<()> {
    validate_document_for_write(document)?;
    write_validated_obj_document(&mut writer, document)
}

pub fn write_obj_document_path(path: impl AsRef<Path>, document: &ObjDocument) -> Result<()> {
    validate_document_for_write(document)?;
    write_validated_obj_document(File::create(path)?, document)
}

fn write_validated_obj_document(mut writer: impl Write, document: &ObjDocument) -> Result<()> {
    writeln!(writer, "# Vkit ordered OBJ document")?;

    for library in &document.appearance.material_libraries {
        writeln!(
            writer,
            "mtllib {}",
            material_library_text(library).expect("document was validated")
        )?;
    }
    for [x, y, z] in &document.geometry.vertices {
        writeln!(writer, "v {x} {y} {z}")?;
    }
    for [u, v] in &document.appearance.texcoords {
        writeln!(writer, "vt {u} {v}")?;
    }

    let face_material_names = face_material_names(document);
    if face_material_names != document.appearance.material_names {
        for material in &document.appearance.material_names {
            writeln!(writer, "usemtl {material}")?;
        }
        if !document.appearance.material_names.is_empty() {
            writeln!(writer, "usemtl off")?;
        }
    }

    let mut current_group: Option<&str> = None;
    let mut current_material: Option<&str> = None;
    for (face, texcoords) in document
        .geometry
        .faces
        .iter()
        .zip(&document.appearance.face_texcoord_indices)
    {
        let next_group = face.group.as_deref();
        if next_group != current_group {
            match next_group {
                Some(group) => writeln!(writer, "g {group}")?,
                None => writeln!(writer, "g off")?,
            }
            current_group = next_group;
        }

        let next_material = face.material.as_deref();
        if next_material != current_material {
            match next_material {
                Some(material) => writeln!(writer, "usemtl {material}")?,
                None => writeln!(writer, "usemtl off")?,
            }
            current_material = next_material;
        }

        write!(writer, "f")?;
        for (&vertex, &texcoord) in face.vertex_indices.iter().zip(texcoords) {
            let vertex = u64::from(vertex) + 1;
            match texcoord {
                Some(texcoord) => {
                    let texcoord = u64::from(texcoord) + 1;
                    write!(writer, " {vertex}/{texcoord}")?;
                }
                None => write!(writer, " {vertex}")?,
            }
        }
        writeln!(writer)?;
    }
    Ok(())
}

fn validate_document_for_write(document: &ObjDocument) -> Result<()> {
    document.validate()?;

    for library in &document.appearance.material_libraries {
        material_library_text(library)?;
    }

    for face in &document.geometry.faces {
        if let Some(group) = &face.group {
            validate_obj_name(group, "group")?;
        }
        if let Some(material) = &face.material {
            validate_obj_name(material, "material")?;
        }
    }

    let mut seen_materials: Vec<&str> = Vec::new();
    for material in &document.appearance.material_names {
        validate_obj_name(material, "material")?;
        if seen_materials.iter().any(|seen| *seen == material) {
            return Err(obj_error(
                0,
                format!("duplicate appearance material name {material:?}"),
            ));
        }
        seen_materials.push(material);
    }

    for material in face_material_names(document) {
        if !document
            .appearance
            .material_names
            .iter()
            .any(|declared| declared == &material)
        {
            return Err(obj_error(
                0,
                format!("face material {material:?} is missing from appearance material_names"),
            ));
        }
    }
    Ok(())
}

fn face_material_names(document: &ObjDocument) -> Vec<String> {
    let mut names = Vec::new();
    for material in document
        .geometry
        .faces
        .iter()
        .filter_map(|face| face.material.as_ref())
    {
        if !names.iter().any(|name| name == material) {
            names.push(material.clone());
        }
    }
    names
}

fn validate_obj_name(name: &str, label: &str) -> Result<()> {
    if name.is_empty() || name == "off" {
        return Err(obj_error(
            0,
            format!("{label} name must be non-empty and must not be `off`"),
        ));
    }
    if name.chars().any(char::is_control) || name.contains('#') {
        return Err(obj_error(
            0,
            format!("{label} name contains a control or comment character"),
        ));
    }
    if name.split_whitespace().collect::<Vec<_>>().join(" ") != name {
        return Err(obj_error(
            0,
            format!("{label} name contains non-canonical whitespace"),
        ));
    }
    Ok(())
}

fn material_library_text(path: &Path) -> Result<String> {
    let raw = path
        .to_str()
        .ok_or_else(|| obj_error(0, "material-library path is not valid Unicode"))?;
    if raw.chars().any(char::is_whitespace) || raw.contains('#') {
        return Err(obj_error(
            0,
            "material-library path contains whitespace or a comment character",
        ));
    }
    let normalized = validate_local_asset_path(raw).map_err(|message| obj_error(0, message))?;
    if normalized != path {
        return Err(obj_error(
            0,
            "material-library path is not in normalized relative form",
        ));
    }
    Ok(normalized.to_string_lossy().replace('\\', "/"))
}

pub fn parse_obj_document(mut reader: impl BufRead) -> Result<ObjDocument> {
    let mut source = Vec::new();
    reader.read_to_end(&mut source)?;

    let geometry = parse_ordered_obj(Cursor::new(source.as_slice()))?;
    let parsed = parse_appearance(Cursor::new(source.as_slice()))?;

    if parsed.face_vertex_indices.len() != geometry.faces.len() {
        return Err(obj_error(
            0,
            "appearance and ordered geometry polygon counts diverged",
        ));
    }
    for (polygon_index, (parsed_indices, face)) in parsed
        .face_vertex_indices
        .iter()
        .zip(&geometry.faces)
        .enumerate()
    {
        if parsed_indices != &face.vertex_indices {
            return Err(obj_error(
                0,
                format!(
                    "appearance and ordered geometry indices diverged at polygon {polygon_index}"
                ),
            ));
        }
    }

    let document = ObjDocument {
        geometry,
        appearance: ObjAppearance {
            texcoords: parsed.texcoords,
            face_texcoord_indices: parsed.face_texcoord_indices,
            material_libraries: parsed.material_libraries,
            material_names: parsed.material_names,
        },
    };
    document.validate()?;
    Ok(document)
}

struct ParsedAppearance {
    texcoords: Vec<[f64; 2]>,
    face_vertex_indices: Vec<Vec<u32>>,
    face_texcoord_indices: Vec<Vec<Option<u32>>>,
    material_libraries: Vec<PathBuf>,
    material_names: Vec<String>,
}

fn parse_appearance(reader: impl BufRead) -> Result<ParsedAppearance> {
    let mut vertex_count = 0_usize;
    let mut normal_count = 0_usize;
    let mut texcoords = Vec::new();
    let mut face_vertex_indices = Vec::new();
    let mut face_texcoord_indices = Vec::new();
    let mut material_libraries = Vec::new();
    let mut material_names = Vec::new();

    for (zero_based_line, line) in reader.lines().enumerate() {
        let line_number = zero_based_line + 1;
        let line = line?;
        let line = if line_number == 1 {
            line.trim_start_matches('\u{feff}')
        } else {
            line.as_str()
        };
        let mut fields = line.split_whitespace();
        let Some(kind) = fields.next() else {
            continue;
        };
        if kind.starts_with('#') {
            continue;
        }
        let fields = fields
            .take_while(|field| !field.starts_with('#'))
            .collect::<Vec<_>>();

        match kind {
            "v" => {
                parse_finite_values(&fields, 3, 4, line_number, "vertex")?;
                vertex_count = vertex_count
                    .checked_add(1)
                    .ok_or_else(|| obj_error(line_number, "OBJ vertex count overflow"))?;
                if vertex_count > u32::MAX as usize {
                    return Err(obj_error(line_number, "OBJ exceeds the u32 vertex limit"));
                }
            }
            "vt" => {
                let values = parse_finite_values(&fields, 2, 3, line_number, "texture coordinate")?;
                if texcoords.len() == u32::MAX as usize {
                    return Err(obj_error(
                        line_number,
                        "OBJ exceeds the u32 texture-coordinate limit",
                    ));
                }
                texcoords.push([values[0], values[1]]);
            }
            "vn" => {
                parse_finite_values(&fields, 3, 3, line_number, "normal")?;
                normal_count = normal_count
                    .checked_add(1)
                    .ok_or_else(|| obj_error(line_number, "OBJ normal count overflow"))?;
                if normal_count > u32::MAX as usize {
                    return Err(obj_error(line_number, "OBJ exceeds the u32 normal limit"));
                }
            }
            "f" => {
                if fields.len() < 3 {
                    return Err(obj_error(line_number, "face has fewer than three corners"));
                }
                let mut vertices = Vec::with_capacity(fields.len());
                let mut texture_coordinates = Vec::with_capacity(fields.len());
                for token in fields {
                    let corner = parse_face_corner(
                        token,
                        vertex_count,
                        texcoords.len(),
                        normal_count,
                        line_number,
                    )?;
                    vertices.push(corner.vertex);
                    texture_coordinates.push(corner.texcoord);
                }
                face_vertex_indices.push(vertices);
                face_texcoord_indices.push(texture_coordinates);
            }
            "mtllib" => {
                if fields.is_empty() {
                    return Err(obj_error(line_number, "mtllib has no local path"));
                }
                for raw in fields {
                    let path = validate_local_asset_path(raw)
                        .map_err(|message| obj_error(line_number, message))?;
                    material_libraries.push(path);
                }
            }
            "usemtl" => {
                let name = fields.join(" ");
                if !name.is_empty()
                    && name != "off"
                    && !material_names.iter().any(|existing| existing == &name)
                {
                    material_names.push(name);
                }
            }
            _ => {}
        }
    }

    Ok(ParsedAppearance {
        texcoords,
        face_vertex_indices,
        face_texcoord_indices,
        material_libraries,
        material_names,
    })
}

struct FaceCorner {
    vertex: u32,
    texcoord: Option<u32>,
}

fn parse_face_corner(
    token: &str,
    vertex_count: usize,
    texcoord_count: usize,
    normal_count: usize,
    line: usize,
) -> Result<FaceCorner> {
    let parts = token.split('/').collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 3 || parts[0].is_empty() {
        return Err(obj_error(line, format!("invalid face corner {token:?}")));
    }

    let vertex = resolve_index(parts[0], vertex_count, "vertex", line)?;
    let texcoord = match parts.len() {
        1 => None,
        2 => {
            if parts[1].is_empty() {
                return Err(obj_error(line, format!("invalid face corner {token:?}")));
            }
            Some(resolve_index(parts[1], texcoord_count, "texture", line)?)
        }
        3 => {
            if parts[1].is_empty() && parts[2].is_empty() {
                return Err(obj_error(line, format!("invalid face corner {token:?}")));
            }
            if parts[2].is_empty() {
                return Err(obj_error(line, format!("invalid face corner {token:?}")));
            }
            resolve_index(parts[2], normal_count, "normal", line)?;
            if parts[1].is_empty() {
                None
            } else {
                Some(resolve_index(parts[1], texcoord_count, "texture", line)?)
            }
        }
        _ => unreachable!(),
    };

    Ok(FaceCorner { vertex, texcoord })
}

fn resolve_index(raw: &str, count: usize, stream: &str, line: usize) -> Result<u32> {
    let value = raw
        .parse::<i64>()
        .map_err(|_| obj_error(line, format!("invalid {stream} index {raw:?}")))?;
    if value == 0 {
        return Err(obj_error(
            line,
            format!("OBJ {stream} index zero is invalid"),
        ));
    }
    let resolved = if value > 0 {
        value - 1
    } else {
        i64::try_from(count).map_err(|_| obj_error(line, format!("{stream} count exceeds i64")))?
            + value
    };
    if resolved < 0 || resolved as usize >= count {
        return Err(obj_error(
            line,
            format!("{stream} index {value} is outside {count} entries"),
        ));
    }
    u32::try_from(resolved)
        .map_err(|_| obj_error(line, format!("resolved {stream} index exceeds u32")))
}

fn parse_finite_values(
    fields: &[&str],
    minimum: usize,
    maximum: usize,
    line: usize,
    label: &str,
) -> Result<Vec<f64>> {
    if fields.len() < minimum || fields.len() > maximum {
        let expected = if minimum == maximum {
            minimum.to_string()
        } else {
            format!("{minimum} to {maximum}")
        };
        return Err(obj_error(
            line,
            format!("{label} requires {expected} finite values"),
        ));
    }
    fields
        .iter()
        .map(|raw| {
            let value = raw
                .parse::<f64>()
                .map_err(|_| obj_error(line, format!("invalid {label} value {raw:?}")))?;
            if !value.is_finite() {
                return Err(obj_error(
                    line,
                    format!("{label} contains a non-finite value"),
                ));
            }
            Ok(value)
        })
        .collect()
}

fn obj_error(line: usize, message: impl Into<String>) -> FormatError {
    FormatError::InvalidObj {
        line,
        message: message.into(),
    }
}
