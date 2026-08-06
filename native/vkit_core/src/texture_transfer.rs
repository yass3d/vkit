use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::formats::{
    DiffuseMapReport, FormatError, G2F_TOPOLOGY_SHA256, MtlDocument, MtlMaterial, ObjAppearance,
    ObjDocument, ObjFace, OrderedObjMesh, Result as FormatResult, matches_canonical_g2f_topology,
};
use crate::spatial::projector_for_mesh;

pub const G2_TEXTURE_TRANSFER_MATERIALS: &[&str] =
    &["Face", "Head", "Neck", "Ears", "Lips", "Nostrils"];

const APPEARANCE_DIGEST_PREFIX: &[u8] = b"vkit.texture-transfer.appearance.v1\0";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureTransferOptions {
    pub symmetry_applied: bool,
    pub output_material_library: PathBuf,
    pub output_diffuse_map: Option<PathBuf>,
}

impl Default for TextureTransferOptions {
    fn default() -> Self {
        Self {
            symmetry_applied: false,
            output_material_library: PathBuf::from("Vkit_Texture.mtl"),
            output_diffuse_map: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureTransferSkipReason {
    SymmetryApplied,
    InvalidTargetGeometry {
        detail: String,
    },
    NonCanonicalG2Target,
    InvalidScanDocument {
        detail: String,
    },
    EmptyScanSurface,
    MissingMaterialLibrary,
    MultipleMaterialLibraries {
        count: usize,
    },
    UnsafeMaterialLibraryPath {
        path: PathBuf,
    },
    NoDiffuseMap,
    MultipleDiffuseMaps {
        count: usize,
    },
    UnsafeDiffuseMapPath {
        path: PathBuf,
    },
    UnsafeOutputMaterialLibraryPath {
        path: PathBuf,
    },
    UnsafeOutputDiffuseMapPath {
        path: PathBuf,
    },
    OutputAtlasExtensionMismatch {
        source: Option<String>,
        output: Option<String>,
    },
    IncompleteScanUv {
        triangle: usize,
        corner: usize,
    },
    MissingScanMaterial {
        triangle: usize,
    },
    ScanMaterialWithoutAtlas {
        triangle: usize,
        material: String,
    },
    NoEligibleTargetFaces,
    ProjectionFailed {
        target_face: usize,
        target_corner: usize,
        detail: String,
    },
    IndexLimitExceeded,
}

impl TextureTransferSkipReason {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::SymmetryApplied => "symmetry_applied",
            Self::InvalidTargetGeometry { .. } => "invalid_target_geometry",
            Self::NonCanonicalG2Target => "noncanonical_g2_target",
            Self::InvalidScanDocument { .. } => "invalid_scan_document",
            Self::EmptyScanSurface => "empty_scan_surface",
            Self::MissingMaterialLibrary => "missing_material_library",
            Self::MultipleMaterialLibraries { .. } => "multiple_material_libraries",
            Self::UnsafeMaterialLibraryPath { .. } => "unsafe_material_library_path",
            Self::NoDiffuseMap => "no_diffuse_map",
            Self::MultipleDiffuseMaps { .. } => "multiple_diffuse_maps",
            Self::UnsafeDiffuseMapPath { .. } => "unsafe_diffuse_map_path",
            Self::UnsafeOutputMaterialLibraryPath { .. } => "unsafe_output_material_library_path",
            Self::UnsafeOutputDiffuseMapPath { .. } => "unsafe_output_diffuse_map_path",
            Self::OutputAtlasExtensionMismatch { .. } => "output_atlas_extension_mismatch",
            Self::IncompleteScanUv { .. } => "incomplete_scan_uv",
            Self::MissingScanMaterial { .. } => "missing_scan_material",
            Self::ScanMaterialWithoutAtlas { .. } => "scan_material_without_atlas",
            Self::NoEligibleTargetFaces => "no_eligible_target_faces",
            Self::ProjectionFailed { .. } => "projection_failed",
            Self::IndexLimitExceeded => "index_limit_exceeded",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureTransferSkipReceipt {
    pub reason: TextureTransferSkipReason,
    pub scan_vertex_count: usize,
    pub scan_polygon_count: usize,
    pub target_vertex_count: usize,
    pub target_polygon_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureAtlasCopyPlan {
    pub source_material_library: PathBuf,
    pub source_diffuse_map: PathBuf,
    pub source_path_from_obj: PathBuf,
    pub output_material_library: PathBuf,
    pub output_diffuse_map: PathBuf,
    pub output_path_from_obj: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextureTransferReceipt {
    pub canonical_target_verified: bool,
    pub source_vertex_count: usize,
    pub source_triangle_count: usize,
    pub source_texcoord_count: usize,
    pub target_vertex_count: usize,
    pub target_polygon_count: usize,
    pub projected_face_count: usize,
    pub projected_corner_count: usize,
    pub unique_projected_vertex_count: usize,
    pub maximum_projection_distance: f64,
    pub target_geometry_sha256: [u8; 32],
    pub projected_appearance_sha256: [u8; 32],
    pub target_topology_sha256: Option<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectedTextureTransfer {
    pub document: ObjDocument,
    pub materials: MtlDocument,
    pub atlas_copy: TextureAtlasCopyPlan,
    pub receipt: TextureTransferReceipt,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TextureTransferOutcome {
    Transferred(Box<ProjectedTextureTransfer>),
    Skipped(TextureTransferSkipReceipt),
}

impl TextureTransferOutcome {
    #[must_use]
    pub fn transferred(&self) -> Option<&ProjectedTextureTransfer> {
        match self {
            Self::Transferred(value) => Some(value),
            Self::Skipped(_) => None,
        }
    }

    #[must_use]
    pub fn skipped(&self) -> Option<&TextureTransferSkipReceipt> {
        match self {
            Self::Transferred(_) => None,
            Self::Skipped(value) => Some(value),
        }
    }
}

#[must_use]
pub fn transfer_texture_to_g2(
    scan: &ObjDocument,
    scan_materials: &MtlDocument,
    fitted_g2: &OrderedObjMesh,
    options: &TextureTransferOptions,
) -> TextureTransferOutcome {
    transfer_texture_impl(scan, scan_materials, fitted_g2, options, true)
}

pub fn write_projected_texture_obj(
    mut writer: impl Write,
    transfer: &ProjectedTextureTransfer,
) -> FormatResult<()> {
    transfer.document.validate()?;
    writeln!(writer, "# Vkit projected texture companion")?;
    for library in &transfer.document.appearance.material_libraries {
        writeln!(writer, "mtllib {}", portable_path(library)?)?;
    }
    for [x, y, z] in &transfer.document.geometry.vertices {
        writeln!(writer, "v {x} {y} {z}")?;
    }
    for [u, v] in &transfer.document.appearance.texcoords {
        writeln!(writer, "vt {u} {v}")?;
    }

    let mut current_group: Option<&str> = None;
    let mut current_material: Option<&str> = None;
    for (face, texcoords) in transfer
        .document
        .geometry
        .faces
        .iter()
        .zip(&transfer.document.appearance.face_texcoord_indices)
    {
        let next_group = face.group.as_deref();
        if next_group != current_group {
            write_label_switch(&mut writer, "g", next_group)?;
            current_group = next_group;
        }
        let next_material = face.material.as_deref();
        if next_material != current_material {
            write_label_switch(&mut writer, "usemtl", next_material)?;
            current_material = next_material;
        }
        write!(writer, "f")?;
        for (&vertex, texcoord) in face.vertex_indices.iter().zip(texcoords) {
            let vertex = u64::from(vertex) + 1;
            if let Some(texcoord) = texcoord {
                write!(writer, " {vertex}/{}", u64::from(*texcoord) + 1)?;
            } else {
                write!(writer, " {vertex}")?;
            }
        }
        writeln!(writer)?;
    }
    Ok(())
}

pub fn write_projected_texture_mtl(
    mut writer: impl Write,
    transfer: &ProjectedTextureTransfer,
) -> FormatResult<()> {
    writeln!(writer, "# Vkit projected texture material")?;
    for material in &transfer.materials.materials {
        validate_label(&material.name)?;
        writeln!(writer, "newmtl {}", material.name)?;
        if let Some([red, green, blue]) = material.diffuse_color {
            writeln!(writer, "Kd {red} {green} {blue}")?;
        }
        if let Some(opacity) = material.opacity {
            writeln!(writer, "d {opacity}")?;
        }
        if let Some(path) = &material.diffuse_map {
            writeln!(writer, "map_Kd {}", portable_path(path)?)?;
        }
        writeln!(writer)?;
    }
    Ok(())
}

fn transfer_texture_impl(
    scan: &ObjDocument,
    scan_materials: &MtlDocument,
    fitted_g2: &OrderedObjMesh,
    options: &TextureTransferOptions,
    require_canonical_target: bool,
) -> TextureTransferOutcome {
    if options.symmetry_applied {
        return skipped(
            scan,
            fitted_g2.vertices.len(),
            fitted_g2.faces.len(),
            TextureTransferSkipReason::SymmetryApplied,
        );
    }
    if let Err(error) = fitted_g2.validate() {
        return skipped(
            scan,
            fitted_g2.vertices.len(),
            fitted_g2.faces.len(),
            TextureTransferSkipReason::InvalidTargetGeometry {
                detail: error.to_string(),
            },
        );
    }
    if require_canonical_target {
        let faces = fitted_g2
            .faces
            .iter()
            .map(|face| face.vertex_indices.clone())
            .collect::<Vec<_>>();
        match matches_canonical_g2f_topology(fitted_g2.vertices.len(), &faces) {
            Ok(true) => {}
            Ok(false) => {
                return skipped(
                    scan,
                    fitted_g2.vertices.len(),
                    fitted_g2.faces.len(),
                    TextureTransferSkipReason::NonCanonicalG2Target,
                );
            }
            Err(error) => {
                return skipped(
                    scan,
                    fitted_g2.vertices.len(),
                    fitted_g2.faces.len(),
                    TextureTransferSkipReason::InvalidTargetGeometry {
                        detail: error.to_string(),
                    },
                );
            }
        }
    }
    if let Err(error) = scan.validate() {
        return skipped(
            scan,
            fitted_g2.vertices.len(),
            fitted_g2.faces.len(),
            TextureTransferSkipReason::InvalidScanDocument {
                detail: error.to_string(),
            },
        );
    }
    let triangulated = match scan.triangulated_appearance() {
        Ok(value) => value,
        Err(error) => {
            return skipped(
                scan,
                fitted_g2.vertices.len(),
                fitted_g2.faces.len(),
                TextureTransferSkipReason::InvalidScanDocument {
                    detail: error.to_string(),
                },
            );
        }
    };
    if triangulated.mesh.triangles.is_empty() {
        return skipped(
            scan,
            fitted_g2.vertices.len(),
            fitted_g2.faces.len(),
            TextureTransferSkipReason::EmptyScanSurface,
        );
    }

    let source_material_library = match scan.appearance.material_libraries.as_slice() {
        [] => {
            return skipped(
                scan,
                fitted_g2.vertices.len(),
                fitted_g2.faces.len(),
                TextureTransferSkipReason::MissingMaterialLibrary,
            );
        }
        [path] => path.clone(),
        paths => {
            return skipped(
                scan,
                fitted_g2.vertices.len(),
                fitted_g2.faces.len(),
                TextureTransferSkipReason::MultipleMaterialLibraries { count: paths.len() },
            );
        }
    };
    if !is_safe_material_library_path(&source_material_library) {
        return skipped(
            scan,
            fitted_g2.vertices.len(),
            fitted_g2.faces.len(),
            TextureTransferSkipReason::UnsafeMaterialLibraryPath {
                path: source_material_library,
            },
        );
    }

    let (source_diffuse_map, atlas_materials, diffuse_map_count) =
        match scan_materials.diffuse_map_report() {
            DiffuseMapReport::NoMap => {
                return skipped(
                    scan,
                    fitted_g2.vertices.len(),
                    fitted_g2.faces.len(),
                    TextureTransferSkipReason::NoDiffuseMap,
                );
            }
            DiffuseMapReport::SingleMap { path, materials } => (path, materials, 1),
            DiffuseMapReport::MultipleMaps { maps } => {
                return skipped(
                    scan,
                    fitted_g2.vertices.len(),
                    fitted_g2.faces.len(),
                    TextureTransferSkipReason::MultipleDiffuseMaps { count: maps.len() },
                );
            }
        };
    debug_assert_eq!(diffuse_map_count, 1);
    if !is_safe_local_path(&source_diffuse_map) {
        return skipped(
            scan,
            fitted_g2.vertices.len(),
            fitted_g2.faces.len(),
            TextureTransferSkipReason::UnsafeDiffuseMapPath {
                path: source_diffuse_map,
            },
        );
    }
    if !is_safe_material_library_path(&options.output_material_library) {
        return skipped(
            scan,
            fitted_g2.vertices.len(),
            fitted_g2.faces.len(),
            TextureTransferSkipReason::UnsafeOutputMaterialLibraryPath {
                path: options.output_material_library.clone(),
            },
        );
    }

    let output_diffuse_map = options.output_diffuse_map.clone().unwrap_or_else(|| {
        source_diffuse_map
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| source_diffuse_map.clone())
    });
    if !is_safe_local_path(&output_diffuse_map) {
        return skipped(
            scan,
            fitted_g2.vertices.len(),
            fitted_g2.faces.len(),
            TextureTransferSkipReason::UnsafeOutputDiffuseMapPath {
                path: output_diffuse_map,
            },
        );
    }
    let source_extension = normalized_extension(&source_diffuse_map);
    let output_extension = normalized_extension(&output_diffuse_map);
    if source_extension != output_extension {
        return skipped(
            scan,
            fitted_g2.vertices.len(),
            fitted_g2.faces.len(),
            TextureTransferSkipReason::OutputAtlasExtensionMismatch {
                source: source_extension,
                output: output_extension,
            },
        );
    }

    let mut source_triangle_uvs = Vec::with_capacity(triangulated.mesh.triangles.len());
    let atlas_materials = atlas_materials.into_iter().collect::<HashSet<_>>();
    for triangle in 0..triangulated.mesh.triangles.len() {
        let mut resolved = [0_u32; 3];
        for (corner, candidate) in triangulated.triangle_texcoord_indices[triangle]
            .iter()
            .enumerate()
        {
            let Some(index) = candidate else {
                return skipped(
                    scan,
                    fitted_g2.vertices.len(),
                    fitted_g2.faces.len(),
                    TextureTransferSkipReason::IncompleteScanUv { triangle, corner },
                );
            };
            resolved[corner] = *index;
        }
        let Some(material) = triangulated.triangle_materials[triangle].as_ref() else {
            return skipped(
                scan,
                fitted_g2.vertices.len(),
                fitted_g2.faces.len(),
                TextureTransferSkipReason::MissingScanMaterial { triangle },
            );
        };
        if !atlas_materials.contains(material) {
            return skipped(
                scan,
                fitted_g2.vertices.len(),
                fitted_g2.faces.len(),
                TextureTransferSkipReason::ScanMaterialWithoutAtlas {
                    triangle,
                    material: material.clone(),
                },
            );
        }
        source_triangle_uvs.push(resolved);
    }

    let selected_faces = fitted_g2
        .faces
        .iter()
        .map(|face| is_transfer_material(face.material.as_deref()))
        .collect::<Vec<_>>();
    let projected_face_count = selected_faces.iter().filter(|selected| **selected).count();
    if projected_face_count == 0 {
        return skipped(
            scan,
            fitted_g2.vertices.len(),
            fitted_g2.faces.len(),
            TextureTransferSkipReason::NoEligibleTargetFaces,
        );
    }

    let projector = match projector_for_mesh(&triangulated.mesh) {
        Ok(value) => value,
        Err(error) => {
            return skipped(
                scan,
                fitted_g2.vertices.len(),
                fitted_g2.faces.len(),
                TextureTransferSkipReason::InvalidScanDocument {
                    detail: error.to_string(),
                },
            );
        }
    };

    #[derive(Clone, Copy)]
    struct ProjectedUv {
        value: [f64; 2],
        distance_squared: f64,
    }

    let mut vertex_projection = vec![None::<ProjectedUv>; fitted_g2.vertices.len()];
    let mut texcoords = Vec::new();
    let mut face_texcoord_indices = fitted_g2
        .faces
        .iter()
        .map(|face| vec![None; face.vertex_indices.len()])
        .collect::<Vec<_>>();
    let mut unique_projected_vertex_count = 0_usize;
    let mut projected_corner_count = 0_usize;
    let mut maximum_projection_distance_squared = 0.0_f64;

    for (face_id, face) in fitted_g2.faces.iter().enumerate() {
        if !selected_faces[face_id] {
            continue;
        }
        for (corner, &vertex_index) in face.vertex_indices.iter().enumerate() {
            let vertex_index_usize = vertex_index as usize;
            let projected = if let Some(projected) = vertex_projection[vertex_index_usize] {
                projected
            } else {
                let hit = match projector.project(fitted_g2.vertices[vertex_index_usize]) {
                    Ok(value) => value,
                    Err(error) => {
                        return skipped(
                            scan,
                            fitted_g2.vertices.len(),
                            fitted_g2.faces.len(),
                            TextureTransferSkipReason::ProjectionFailed {
                                target_face: face_id,
                                target_corner: corner,
                                detail: error.to_string(),
                            },
                        );
                    }
                };
                let triangle = hit.primitive_id as usize;
                let Some(indices) = source_triangle_uvs.get(triangle).copied() else {
                    return skipped(
                        scan,
                        fitted_g2.vertices.len(),
                        fitted_g2.faces.len(),
                        TextureTransferSkipReason::ProjectionFailed {
                            target_face: face_id,
                            target_corner: corner,
                            detail: "surface projector returned an unknown primitive".to_owned(),
                        },
                    );
                };
                let mut uv = [0.0_f64; 2];
                for (&source_index, &weight) in indices.iter().zip(&hit.barycentric) {
                    let source_uv = scan.appearance.texcoords[source_index as usize];
                    uv[0] += weight * source_uv[0];
                    uv[1] += weight * source_uv[1];
                }
                let projected = ProjectedUv {
                    value: uv,
                    distance_squared: hit.distance_squared,
                };
                vertex_projection[vertex_index_usize] = Some(projected);
                unique_projected_vertex_count += 1;
                projected
            };
            let Ok(texcoord_index) = u32::try_from(texcoords.len()) else {
                return skipped(
                    scan,
                    fitted_g2.vertices.len(),
                    fitted_g2.faces.len(),
                    TextureTransferSkipReason::IndexLimitExceeded,
                );
            };
            texcoords.push(projected.value);
            face_texcoord_indices[face_id][corner] = Some(texcoord_index);
            projected_corner_count += 1;
            maximum_projection_distance_squared =
                maximum_projection_distance_squared.max(projected.distance_squared);
        }
    }

    let material_names = first_seen_material_names(fitted_g2.faces.iter());
    let appearance = ObjAppearance {
        texcoords,
        face_texcoord_indices,
        material_libraries: vec![options.output_material_library.clone()],
        material_names,
    };
    let document = ObjDocument {
        geometry: fitted_g2.clone(),
        appearance,
    };
    if let Err(error) = document.validate() {
        return skipped(
            scan,
            fitted_g2.vertices.len(),
            fitted_g2.faces.len(),
            TextureTransferSkipReason::InvalidTargetGeometry {
                detail: error.to_string(),
            },
        );
    }

    let projected_material_names = first_seen_material_names(
        fitted_g2
            .faces
            .iter()
            .enumerate()
            .filter_map(|(index, face)| selected_faces[index].then_some(face)),
    );
    let materials = MtlDocument {
        materials: projected_material_names
            .into_iter()
            .map(|name| MtlMaterial {
                name,
                diffuse_color: Some([1.0, 1.0, 1.0]),
                opacity: None,
                opacity_source: None,
                diffuse_map: Some(output_diffuse_map.clone()),
            })
            .collect(),
    };
    let source_path_from_obj =
        join_below_material_library(&source_material_library, &source_diffuse_map);
    let output_path_from_obj =
        join_below_material_library(&options.output_material_library, &output_diffuse_map);
    let atlas_copy = TextureAtlasCopyPlan {
        source_material_library,
        source_diffuse_map,
        source_path_from_obj,
        output_material_library: options.output_material_library.clone(),
        output_diffuse_map,
        output_path_from_obj,
    };
    let target_geometry_sha256 = match fitted_g2
        .triangulated()
        .and_then(|mesh| mesh.canonical_hash())
    {
        Ok(value) => value,
        Err(error) => {
            return skipped(
                scan,
                fitted_g2.vertices.len(),
                fitted_g2.faces.len(),
                TextureTransferSkipReason::InvalidTargetGeometry {
                    detail: error.to_string(),
                },
            );
        }
    };
    let projected_appearance_sha256 = appearance_digest(&document);
    TextureTransferOutcome::Transferred(Box::new(ProjectedTextureTransfer {
        receipt: TextureTransferReceipt {
            canonical_target_verified: require_canonical_target,
            source_vertex_count: triangulated.mesh.vertices.len(),
            source_triangle_count: triangulated.mesh.triangles.len(),
            source_texcoord_count: scan.appearance.texcoords.len(),
            target_vertex_count: fitted_g2.vertices.len(),
            target_polygon_count: fitted_g2.faces.len(),
            projected_face_count,
            projected_corner_count,
            unique_projected_vertex_count,
            maximum_projection_distance: maximum_projection_distance_squared.sqrt(),
            target_geometry_sha256,
            projected_appearance_sha256,
            target_topology_sha256: require_canonical_target.then_some(G2F_TOPOLOGY_SHA256),
        },
        document,
        materials,
        atlas_copy,
    }))
}

fn skipped(
    scan: &ObjDocument,
    target_vertex_count: usize,
    target_polygon_count: usize,
    reason: TextureTransferSkipReason,
) -> TextureTransferOutcome {
    TextureTransferOutcome::Skipped(TextureTransferSkipReceipt {
        reason,
        scan_vertex_count: scan.geometry.vertices.len(),
        scan_polygon_count: scan.geometry.faces.len(),
        target_vertex_count,
        target_polygon_count,
    })
}

fn is_transfer_material(material: Option<&str>) -> bool {
    material.is_some_and(|material| {
        G2_TEXTURE_TRANSFER_MATERIALS
            .iter()
            .any(|candidate| material.eq_ignore_ascii_case(candidate))
    })
}

fn first_seen_material_names<'a>(faces: impl IntoIterator<Item = &'a ObjFace>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for face in faces {
        if let Some(name) = &face.material
            && seen.insert(name.clone())
        {
            result.push(name.clone());
        }
    }
    result
}

fn join_below_material_library(library: &Path, diffuse_map: &Path) -> PathBuf {
    library
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map_or_else(
            || diffuse_map.to_path_buf(),
            |parent| parent.join(diffuse_map),
        )
}

fn normalized_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

fn is_safe_local_path(path: &Path) -> bool {
    let Some(raw) = path.to_str() else {
        return false;
    };
    if raw.is_empty()
        || raw.trim() != raw
        || raw.chars().any(char::is_control)
        || raw.contains(':')
        || raw.contains('#')
    {
        return false;
    }
    let normalized = raw.replace('\\', "/");
    if normalized.starts_with('/') {
        return false;
    }
    let mut component_count = 0_usize;
    for component in normalized.split('/') {
        if component.is_empty() || component == ".." {
            return false;
        }
        if component == "." {
            continue;
        }
        if component.ends_with('.') || component.ends_with(' ') || is_windows_device_name(component)
        {
            return false;
        }
        component_count += 1;
    }
    component_count > 0
}

fn is_safe_material_library_path(path: &Path) -> bool {
    is_safe_local_path(path)
        && path
            .to_str()
            .is_some_and(|value| !value.chars().any(char::is_whitespace))
}

fn is_windows_device_name(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'))
}

fn appearance_digest(document: &ObjDocument) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(APPEARANCE_DIGEST_PREFIX);
    digest.update((document.appearance.texcoords.len() as u64).to_le_bytes());
    digest.update((document.appearance.face_texcoord_indices.len() as u64).to_le_bytes());
    for uv in &document.appearance.texcoords {
        for &coordinate in uv {
            let normalized = if coordinate == 0.0 { 0.0 } else { coordinate };
            digest.update(normalized.to_bits().to_le_bytes());
        }
    }
    for corners in &document.appearance.face_texcoord_indices {
        digest.update((corners.len() as u64).to_le_bytes());
        for corner in corners {
            match corner {
                Some(index) => {
                    digest.update([1]);
                    digest.update(index.to_le_bytes());
                }
                None => digest.update([0]),
            }
        }
    }
    digest.finalize().into()
}

fn portable_path(path: &Path) -> FormatResult<String> {
    if !is_safe_local_path(path) {
        return Err(FormatError::InvalidObj {
            line: 0,
            message: format!("unsafe local asset path {path:?}"),
        });
    }
    Ok(path
        .to_str()
        .expect("safe local paths are valid Unicode")
        .replace('\\', "/"))
}

fn validate_label(label: &str) -> FormatResult<()> {
    if label.is_empty() || label.contains('#') || label.chars().any(char::is_control) {
        return Err(FormatError::InvalidObj {
            line: 0,
            message: "OBJ/MTL label is empty or contains a control character".to_owned(),
        });
    }
    Ok(())
}

fn write_label_switch(
    writer: &mut impl Write,
    keyword: &str,
    value: Option<&str>,
) -> FormatResult<()> {
    match value {
        Some(value) => {
            validate_label(value)?;
            writeln!(writer, "{keyword} {value}")?;
        }
        None => writeln!(writer, "{keyword} off")?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    #[test]
    fn the_digest_domains_keep_their_original_spelling() {
        assert_eq!(
            APPEARANCE_DIGEST_PREFIX,
            b"vkit.texture-transfer.appearance.v1\x00"
        );
    }

    use approx::assert_abs_diff_eq;

    use super::*;
    use crate::formats::{parse_mtl, parse_obj_document};

    fn textured_scan() -> (ObjDocument, MtlDocument) {
        let obj = parse_obj_document(Cursor::new(concat!(
            "mtllib scan.mtl\n",
            "v 0 0 0\n",
            "v 1 0 0\n",
            "v 1 1 0\n",
            "v 0 1 0\n",
            "vt 0 0\n",
            "vt 1 0\n",
            "vt 1 1\n",
            "vt 0 1\n",
            "vt 10 10\n",
            "vt 20 10\n",
            "vt 20 20\n",
            "usemtl ScanSkin\n",
            "f 1/1 2/2 3/3\n",
            "f 1/5 3/6 4/7\n",
        )))
        .unwrap();
        let mtl = parse_mtl(Cursor::new("newmtl ScanSkin\nmap_Kd textures/scan.png\n")).unwrap();
        (obj, mtl)
    }

    fn target() -> OrderedObjMesh {
        OrderedObjMesh {
            vertices: vec![
                [0.25, 0.25, 0.2],
                [0.75, 0.25, 0.2],
                [0.75, 0.75, 0.2],
                [0.25, 0.75, 0.2],
            ],
            faces: vec![
                ObjFace {
                    vertex_indices: vec![0, 1, 2],
                    group: Some("head".into()),
                    material: Some("Face".into()),
                },
                ObjFace {
                    vertex_indices: vec![0, 2, 3],
                    group: Some("eyes".into()),
                    material: Some("Sclera".into()),
                },
            ],
        }
    }

    fn transfer_fixture(
        scan: &ObjDocument,
        materials: &MtlDocument,
        target: &OrderedObjMesh,
        options: &TextureTransferOptions,
    ) -> TextureTransferOutcome {
        transfer_texture_impl(scan, materials, target, options, false)
    }

    #[test]
    fn barycentric_uv_projection_and_material_mask_are_exact() {
        let (scan, materials) = textured_scan();
        let target = target();
        let outcome = transfer_fixture(&scan, &materials, &target, &Default::default());
        let transferred = outcome.transferred().unwrap();

        assert_eq!(transferred.document.geometry, target);
        assert_eq!(transferred.receipt.projected_face_count, 1);
        assert_eq!(transferred.receipt.projected_corner_count, 3);
        assert_eq!(transferred.receipt.unique_projected_vertex_count, 3);
        assert!(
            transferred.document.appearance.face_texcoord_indices[0]
                .iter()
                .all(Option::is_some)
        );
        assert!(
            transferred.document.appearance.face_texcoord_indices[1]
                .iter()
                .all(Option::is_none)
        );
        assert_abs_diff_eq!(
            transferred.document.appearance.texcoords[0][0],
            0.25,
            epsilon = 1e-12
        );
        assert_abs_diff_eq!(
            transferred.document.appearance.texcoords[0][1],
            0.25,
            epsilon = 1e-12
        );
        assert_eq!(
            transferred
                .materials
                .materials
                .iter()
                .map(|material| material.name.as_str())
                .collect::<Vec<_>>(),
            ["Face"]
        );
    }

    #[test]
    fn source_seams_and_target_corner_streams_remain_independent() {
        let (scan, materials) = textured_scan();
        let mut target = target();
        target.faces[1].material = Some("Lips".into());
        let outcome = transfer_fixture(&scan, &materials, &target, &Default::default());
        let transferred = outcome.transferred().unwrap();

        let first_face = &transferred.document.appearance.face_texcoord_indices[0];
        let second_face = &transferred.document.appearance.face_texcoord_indices[1];
        assert_ne!(first_face[0], second_face[0]);
        assert_ne!(first_face[2], second_face[1]);

        let uv_for_last_corner =
            transferred.document.appearance.texcoords[second_face[2].unwrap() as usize];
        assert!(uv_for_last_corner[0] >= 10.0);
        assert!(uv_for_last_corner[1] >= 10.0);
    }

    #[test]
    fn receipts_and_export_text_are_deterministic() {
        let (scan, materials) = textured_scan();
        let target = target();
        let first = transfer_fixture(&scan, &materials, &target, &Default::default());
        let second = transfer_fixture(&scan, &materials, &target, &Default::default());
        assert_eq!(first, second);

        let transfer = first.transferred().unwrap();
        assert_eq!(
            transfer.atlas_copy.source_path_from_obj,
            PathBuf::from("textures").join("scan.png")
        );
        assert_eq!(
            transfer.atlas_copy.output_path_from_obj,
            PathBuf::from("scan.png")
        );
        let mut obj = Vec::new();
        let mut mtl = Vec::new();
        write_projected_texture_obj(&mut obj, transfer).unwrap();
        write_projected_texture_mtl(&mut mtl, transfer).unwrap();
        let obj = String::from_utf8(obj).unwrap();
        let mtl = String::from_utf8(mtl).unwrap();
        let reparsed_obj = parse_obj_document(Cursor::new(obj.as_bytes())).unwrap();
        let reparsed_mtl = parse_mtl(Cursor::new(mtl.as_bytes())).unwrap();
        assert_eq!(reparsed_obj, transfer.document);
        assert_eq!(reparsed_mtl, transfer.materials);
        assert!(obj.contains("mtllib Vkit_Texture.mtl"));
        assert!(obj.contains("f 1/1 2/2 3/3"));
        assert!(obj.contains("usemtl Sclera"));
        assert!(mtl.contains("newmtl Face"));
        assert!(mtl.contains("map_Kd scan.png"));
        assert!(!mtl.contains("newmtl Sclera"));
    }

    #[test]
    fn unsupported_inputs_skip_in_a_stable_order() {
        let (scan, materials) = textured_scan();
        let target = target();

        let symmetry = transfer_fixture(
            &scan,
            &materials,
            &target,
            &TextureTransferOptions {
                symmetry_applied: true,
                ..Default::default()
            },
        );
        assert_eq!(
            symmetry.skipped().unwrap().reason,
            TextureTransferSkipReason::SymmetryApplied
        );

        let mut no_library = scan.clone();
        no_library.appearance.material_libraries.clear();
        assert_eq!(
            transfer_fixture(&no_library, &materials, &target, &Default::default())
                .skipped()
                .unwrap()
                .reason,
            TextureTransferSkipReason::MissingMaterialLibrary
        );

        let no_map = parse_mtl(Cursor::new("newmtl ScanSkin\n")).unwrap();
        assert_eq!(
            transfer_fixture(&scan, &no_map, &target, &Default::default())
                .skipped()
                .unwrap()
                .reason,
            TextureTransferSkipReason::NoDiffuseMap
        );

        let multiple = parse_mtl(Cursor::new(concat!(
            "newmtl ScanSkin\nmap_Kd a.png\n",
            "newmtl Other\nmap_Kd b.png\n",
        )))
        .unwrap();
        assert_eq!(
            transfer_fixture(&scan, &multiple, &target, &Default::default())
                .skipped()
                .unwrap()
                .reason,
            TextureTransferSkipReason::MultipleDiffuseMaps { count: 2 }
        );
    }

    #[test]
    fn incomplete_uv_unsafe_paths_and_unbound_materials_are_nonfatal_skips() {
        let (scan, materials) = textured_scan();
        let target = target();

        let mut incomplete = scan.clone();
        incomplete.appearance.face_texcoord_indices[0][1] = None;
        assert_eq!(
            transfer_fixture(&incomplete, &materials, &target, &Default::default())
                .skipped()
                .unwrap()
                .reason,
            TextureTransferSkipReason::IncompleteScanUv {
                triangle: 0,
                corner: 1,
            }
        );

        let unsafe_options = TextureTransferOptions {
            output_material_library: PathBuf::from("../outside.mtl"),
            ..Default::default()
        };
        assert!(matches!(
            transfer_fixture(&scan, &materials, &target, &unsafe_options)
                .skipped()
                .unwrap()
                .reason,
            TextureTransferSkipReason::UnsafeOutputMaterialLibraryPath { .. }
        ));

        let unbound = parse_mtl(Cursor::new(concat!(
            "newmtl ScanSkin\n",
            "newmtl Other\nmap_Kd scan.png\n",
        )))
        .unwrap();
        assert!(matches!(
            transfer_fixture(&scan, &unbound, &target, &Default::default())
                .skipped()
                .unwrap()
                .reason,
            TextureTransferSkipReason::ScanMaterialWithoutAtlas { triangle: 0, .. }
        ));
    }

    #[test]
    fn public_entry_point_refuses_noncanonical_targets_without_erroring() {
        let (scan, materials) = textured_scan();
        let outcome = transfer_texture_to_g2(
            &scan,
            &materials,
            &target(),
            &TextureTransferOptions::default(),
        );
        assert_eq!(
            outcome.skipped().unwrap().reason,
            TextureTransferSkipReason::NonCanonicalG2Target
        );
    }
}
