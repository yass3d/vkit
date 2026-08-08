mod fbx;
mod glb;
mod gltf_container;
mod simplify;

use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use tempfile::TempDir;
use thiserror::Error;
use vkit_core::formats::{
    OrderedObjMesh, load_mtl, load_obj_document, load_ordered_obj, write_obj_document_path,
};

#[cfg(test)]
pub use simplify::SimplificationReceipt;

pub const MAX_SCAN_TRIANGLES: usize = 100_000;
const MAX_NATIVE_SOURCE_BYTES: u64 = 1_500_000_000;

#[derive(Debug)]
pub struct PreparedMeshImport {
    pub ordered: OrderedObjMesh,
    pub appearance_path: PathBuf,

    #[cfg(test)]
    pub appearance_root: PathBuf,
    pub source_triangles: Option<usize>,
    pub final_triangles: usize,

    #[cfg(test)]
    pub simplification: SimplificationReceipt,
    pub workspace: Option<TempDir>,
}

#[derive(Debug)]
pub struct OrderedTemplateMesh {
    pub ordered: OrderedObjMesh,

    pub structural_labels: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeshImportPhase {
    MeshLoading,
    Simplification,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshImportProgress {
    pub phase: MeshImportPhase,

    pub progress: f32,

    pub source_triangles: Option<usize>,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("unsupported mesh input extension; use OBJ, GLB, glTF, or FBX")]
    UnsupportedExtension,
    #[error("failed to create a temporary import workspace: {0}")]
    TemporaryWorkspace(#[from] std::io::Error),
    #[error("mesh could not be parsed: {0}")]
    ConvertedMesh(String),
    #[error("native mesh import failed: {0}")]
    NativeMesh(String),
    #[error("failed to preserve imported appearance assets: {0}")]
    AppearanceAssets(String),
    #[error("failed to inspect OBJ polygon count: {0}")]
    PolygonInspection(String),
    #[error("native simplification produced {actual} triangles; the Vkit safety cap is {maximum}")]
    DecimationLimit { actual: usize, maximum: usize },
}

pub fn is_supported_mesh_path(path: &Path) -> bool {
    extension(path)
        .as_deref()
        .is_some_and(|value| matches!(value, "obj" | "glb" | "gltf" | "fbx"))
}

pub fn load_ordered_template_mesh(path: &Path) -> Result<OrderedTemplateMesh, ImportError> {
    match extension(path).as_deref() {
        Some("obj") => Ok(OrderedTemplateMesh {
            ordered: load_ordered_obj(path)
                .map_err(|error| ImportError::ConvertedMesh(error.to_string()))?,
            structural_labels: Vec::new(),
        }),
        Some("fbx") => {
            validate_native_source_size(path)?;
            let (ordered, structural_labels) =
                fbx::load_ordered_template_fbx(path).map_err(ImportError::NativeMesh)?;
            Ok(OrderedTemplateMesh {
                ordered,
                structural_labels,
            })
        }
        Some("glb" | "gltf") => Err(ImportError::NativeMesh(
            "glTF template ingestion is rejected because triangle primitives cannot prove the canonical ordered quad stream".to_owned(),
        )),
        _ => Err(ImportError::UnsupportedExtension),
    }
}

pub fn prepare_mesh_import_with_progress(
    path: &Path,
    mut callback: impl FnMut(MeshImportProgress),
) -> Result<PreparedMeshImport, ImportError> {
    let mut progress = ProgressEmitter::new(&mut callback);
    progress.emit(MeshImportPhase::MeshLoading, 0.0);
    match extension(path).as_deref() {
        Some("obj") => {
            let source_triangles = inspect_obj_triangles(path, Some(MAX_SCAN_TRIANGLES + 1))?;
            progress.note_source_triangles(source_triangles);
            if source_triangles <= MAX_SCAN_TRIANGLES {
                let mut ordered = load_ordered_obj(path)
                    .map_err(|error| ImportError::ConvertedMesh(error.to_string()))?;
                weld_scan_surface(&mut ordered);
                let final_triangles = ordered_triangle_count(&ordered);
                progress.emit(MeshImportPhase::MeshLoading, 0.72);
                progress.emit(MeshImportPhase::Simplification, 1.0);
                return Ok(PreparedMeshImport {
                    final_triangles,
                    ordered,
                    appearance_path: path.to_path_buf(),
                    #[cfg(test)]
                    appearance_root: path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .to_path_buf(),
                    source_triangles: Some(source_triangles),
                    #[cfg(test)]
                    simplification: SimplificationReceipt {
                        attempted: false,
                        source_triangles,
                        final_triangles,
                        ..SimplificationReceipt::default()
                    },
                    workspace: None,
                });
            }
            validate_native_source_size(path)?;
            let document = load_obj_document(path)
                .map_err(|error| ImportError::NativeMesh(error.to_string()))?;
            let source_triangles = ordered_triangle_count(&document.geometry);
            progress.note_source_triangles(source_triangles);
            progress.emit(MeshImportPhase::MeshLoading, 0.65);
            let workspace = TempDir::new()?;
            copy_obj_appearance_assets(path, &document, workspace.path())?;
            let mesh = simplify::from_obj_document(&document).map_err(ImportError::NativeMesh)?;
            finish_native_import(mesh, source_triangles, workspace, &mut progress)
        }
        Some("glb" | "gltf") => {
            validate_native_source_size(path)?;
            let workspace = TempDir::new()?;
            let mesh = glb::load_glb(path, workspace.path(), |fraction| {
                progress.emit(MeshImportPhase::MeshLoading, fraction * 0.65)
            })
            .map_err(ImportError::NativeMesh)?;
            let source_triangles = mesh.triangle_count();
            progress.note_source_triangles(source_triangles);
            finish_native_import(mesh, source_triangles, workspace, &mut progress)
        }
        Some("fbx") => {
            validate_native_source_size(path)?;
            let workspace = TempDir::new()?;
            let mesh = fbx::load_fbx(path, workspace.path(), |fraction| {
                progress.emit(MeshImportPhase::MeshLoading, fraction * 0.65)
            })
            .map_err(ImportError::NativeMesh)?;
            let source_triangles = mesh.triangle_count();
            progress.note_source_triangles(source_triangles);
            finish_native_import(mesh, source_triangles, workspace, &mut progress)
        }
        _ => Err(ImportError::UnsupportedExtension),
    }
}

/// Bounds the container file itself. A `.gltf` container is only the JSON, so for that input
/// this gate says nothing about the payload; what bounds a `.gltf` is the importer's payload
/// budget, which every external buffer, image, and decompressed buffer view draws from.
fn validate_native_source_size(path: &Path) -> Result<(), ImportError> {
    let bytes = path
        .metadata()
        .map_err(|error| ImportError::NativeMesh(error.to_string()))?
        .len();
    if bytes > MAX_NATIVE_SOURCE_BYTES {
        return Err(ImportError::NativeMesh(format!(
            "source is {bytes} bytes; the bounded native importer limit is {MAX_NATIVE_SOURCE_BYTES} bytes"
        )));
    }
    Ok(())
}

struct ProgressEmitter<'a, F> {
    callback: &'a mut F,
    last: f32,
    source_triangles: Option<usize>,
}

impl<'a, F: FnMut(MeshImportProgress)> ProgressEmitter<'a, F> {
    fn new(callback: &'a mut F) -> Self {
        Self {
            callback,
            last: 0.0,
            source_triangles: None,
        }
    }

    fn emit(&mut self, phase: MeshImportPhase, progress: f32) {
        let progress = progress.clamp(self.last, 1.0);
        self.last = progress;
        (self.callback)(MeshImportProgress {
            phase,
            progress,
            source_triangles: self.source_triangles,
        });
    }

    fn note_source_triangles(&mut self, triangles: usize) {
        self.source_triangles = Some(triangles);
    }
}

fn weld_scan_surface(ordered: &mut vkit_core::formats::OrderedObjMesh) {
    let receipt = vkit_core::formats::weld_ordered_obj_vertices(ordered);
    if !receipt.changed() {
        return;
    }
    let _ = crate::diagnostics::record(
        crate::diagnostics::Severity::Info,
        "importers",
        "scan_surface_welded",
        &format!(
            "merged={}; dropped_faces={}",
            receipt.merged_vertices, receipt.dropped_faces
        ),
    );
}

fn finish_native_import<F: FnMut(MeshImportProgress)>(
    mesh: simplify::AttributeMesh,
    source_triangles: usize,
    workspace: TempDir,
    progress: &mut ProgressEmitter<'_, F>,
) -> Result<PreparedMeshImport, ImportError> {
    progress.emit(MeshImportPhase::Simplification, 0.65);
    let simplified = simplify::simplify_to_limit(mesh, MAX_SCAN_TRIANGLES, |fraction| {
        progress.emit(
            MeshImportPhase::Simplification,
            0.65 + fraction.clamp(0.0, 1.0) * 0.35,
        )
    })
    .map_err(ImportError::NativeMesh)?;
    if simplified.receipt.final_triangles > MAX_SCAN_TRIANGLES {
        return Err(ImportError::DecimationLimit {
            actual: simplified.receipt.final_triangles,
            maximum: MAX_SCAN_TRIANGLES,
        });
    }
    let appearance_path = workspace.path().join("vkit-native-import.obj");
    write_obj_document_path(&appearance_path, &simplified.document)
        .map_err(|error| ImportError::NativeMesh(error.to_string()))?;
    let mut ordered = simplified.document.geometry;
    weld_scan_surface(&mut ordered);
    let final_triangles = ordered_triangle_count(&ordered);
    if ordered.vertices.is_empty() || final_triangles == 0 || final_triangles > MAX_SCAN_TRIANGLES {
        return Err(ImportError::NativeMesh(
            "native import produced an empty mesh or exceeded the triangle cap".to_owned(),
        ));
    }
    progress.emit(MeshImportPhase::Simplification, 1.0);
    Ok(PreparedMeshImport {
        ordered,
        appearance_path,
        #[cfg(test)]
        appearance_root: workspace.path().to_path_buf(),
        source_triangles: Some(source_triangles),
        final_triangles,
        #[cfg(test)]
        simplification: simplified.receipt,
        workspace: Some(workspace),
    })
}

fn copy_obj_appearance_assets(
    source_obj: &Path,
    document: &vkit_core::formats::ObjDocument,
    destination_root: &Path,
) -> Result<(), ImportError> {
    let source_root = source_obj.parent().unwrap_or_else(|| Path::new("."));
    for library in &document.appearance.material_libraries {
        let source_library = source_root.join(library);
        if !source_library.is_file() {
            continue;
        }
        let destination_library = destination_root.join(library);
        if let Some(parent) = destination_library.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| ImportError::AppearanceAssets(error.to_string()))?;
        }
        fs::copy(&source_library, &destination_library)
            .map_err(|error| ImportError::AppearanceAssets(error.to_string()))?;
        let Ok(materials) = load_mtl(&source_library) else {
            continue;
        };
        let source_material_root = source_library.parent().unwrap_or(source_root);
        let destination_material_root = destination_library.parent().unwrap_or(destination_root);
        for diffuse_map in materials
            .materials
            .iter()
            .filter_map(|material| material.diffuse_map.as_ref())
        {
            let source_texture = source_material_root.join(diffuse_map);
            if !source_texture.is_file() {
                continue;
            }
            let destination_texture = destination_material_root.join(diffuse_map);
            if let Some(parent) = destination_texture.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| ImportError::AppearanceAssets(error.to_string()))?;
            }
            fs::copy(source_texture, destination_texture)
                .map_err(|error| ImportError::AppearanceAssets(error.to_string()))?;
        }
    }
    Ok(())
}

fn ordered_triangle_count(mesh: &OrderedObjMesh) -> usize {
    mesh.faces
        .iter()
        .map(|face| face.vertex_indices.len().saturating_sub(2))
        .sum()
}

fn inspect_obj_triangles(path: &Path, stop_after: Option<usize>) -> Result<usize, ImportError> {
    let file =
        File::open(path).map_err(|error| ImportError::PolygonInspection(error.to_string()))?;
    let mut triangles = 0_usize;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|error| ImportError::PolygonInspection(error.to_string()))?;
        let trimmed = line.trim_start();
        let Some(corners) = trimmed.strip_prefix("f ") else {
            continue;
        };
        triangles = triangles.saturating_add(corners.split_whitespace().count().saturating_sub(2));
        if stop_after.is_some_and(|limit| triangles >= limit) {
            break;
        }
    }
    Ok(triangles)
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
}

#[cfg(not(windows))]
fn configure_background_process(_: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn supported_mesh_extensions_are_case_insensitive() {
        assert!(is_supported_mesh_path(Path::new("head.OBJ")));
        assert!(is_supported_mesh_path(Path::new("head.glb")));
        assert!(is_supported_mesh_path(Path::new("head.glTF")));
        assert!(is_supported_mesh_path(Path::new("head.FbX")));
        assert!(!is_supported_mesh_path(Path::new("head.dsf")));
    }

    #[test]
    fn gltf_is_refused_for_template_ingestion_for_the_same_reason_glb_is() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = workspace.path().join("template.gltf");
        fs::write(&source, "{}").expect("fixture");
        let error = load_ordered_template_mesh(&source)
            .expect_err("triangles cannot prove the ordered quad stream");
        assert!(error.to_string().contains("ordered quad stream"));
    }

    #[test]
    fn obj_preflight_counts_fan_triangles_and_honors_the_early_limit() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = workspace.path().join("scan.obj");
        fs::write(
            &source,
            "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3\nf 1 2 3 4\n",
        )
        .expect("fixture");
        assert_eq!(inspect_obj_triangles(&source, None).unwrap(), 3);
        assert_eq!(inspect_obj_triangles(&source, Some(2)).unwrap(), 3);
    }

    #[test]
    fn under_cap_obj_keeps_ordered_geometry_and_reports_monotonic_progress() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = workspace.path().join("scan.obj");
        fs::write(
            &source,
            "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\ng scan\nusemtl skin\nf 1/1 2/2 3/3 4/4\n",
        )
        .expect("fixture");
        let mut events = Vec::new();
        let imported = prepare_mesh_import_with_progress(&source, |event| events.push(event))
            .expect("native OBJ import");
        assert_eq!(imported.appearance_path, source);
        assert_eq!(imported.ordered.vertices.len(), 4);
        assert_eq!(imported.ordered.faces[0].vertex_indices, [0, 1, 2, 3]);
        assert_eq!(imported.source_triangles, Some(2));
        assert_eq!(imported.final_triangles, 2);
        assert!(!imported.simplification.attempted);
        assert!(imported.workspace.is_none());
        assert!(
            events
                .windows(2)
                .all(|events| events[0].progress <= events[1].progress)
        );
        assert_eq!(events.last().map(|event| event.progress), Some(1.0));
        assert!(
            events
                .iter()
                .any(|event| event.phase == MeshImportPhase::Simplification)
        );
    }

    #[test]
    fn minimal_glb_is_loaded_natively_with_node_transform() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = workspace.path().join("triangle.glb");
        fs::write(&source, minimal_triangle_glb()).expect("GLB fixture");
        let imported =
            prepare_mesh_import_with_progress(&source, |_| {}).expect("native GLB import");
        assert_eq!(imported.source_triangles, Some(1));
        assert_eq!(imported.final_triangles, 1);
        assert_eq!(imported.ordered.vertices[0], [2.0, 0.0, 0.0]);
        assert_eq!(imported.ordered.vertices[1], [3.0, 0.0, 0.0]);
        assert_eq!(imported.ordered.vertices[2], [2.0, 1.0, 0.0]);
        assert!(imported.appearance_root.join("vkit-import.mtl").is_file());
        assert!(imported.workspace.is_some());
    }

    #[test]
    fn glb_seam_duplicated_positions_weld_into_canonical_vertices() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = workspace.path().join("seam.glb");

        fs::write(
            &source,
            build_test_glb(
                &[
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [1.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0],
                    [1.0, 1.0, 0.0],
                ],
                &[0, 1, 2, 3, 5, 4],
                [0.0, 0.0, 0.0],
            ),
        )
        .expect("GLB fixture");
        let imported =
            prepare_mesh_import_with_progress(&source, |_| {}).expect("native GLB import");
        assert_eq!(imported.final_triangles, 2);
        assert_eq!(
            imported.ordered.vertices.len(),
            4,
            "coincident seam duplicates must weld into canonical vertices"
        );
        assert_eq!(imported.simplification.welded_coincident_vertices, 2);
        assert_eq!(imported.simplification.open_boundary_edges, 4);
        let shared = imported.ordered.faces[0]
            .vertex_indices
            .iter()
            .filter(|&index| imported.ordered.faces[1].vertex_indices.contains(index))
            .count();
        assert_eq!(shared, 2, "the welded seam edge must be shared topology");
    }

    #[test]
    fn ascii_fbx_imports_instances_hierarchy_uvs_and_mirrored_transform() {
        let fixture = include_str!("importers/fixtures/static_instances_ascii.fbx");
        let source_root = tempfile::tempdir().expect("FBX fixture root");
        let source = source_root.path().join("static-instances.fbx");
        fs::write(&source, fixture).expect("FBX fixture");
        fs::write(
            source_root.path().join("generated-albedo.png"),
            [
                137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0,
                1, 8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 8, 215, 99, 248,
                207, 192, 240, 31, 0, 5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68,
                174, 66, 96, 130,
            ],
        )
        .expect("generated texture fixture");
        let imported =
            prepare_mesh_import_with_progress(&source, |_| {}).expect("native ASCII FBX import");
        assert_eq!(imported.source_triangles, Some(6));
        assert_eq!(imported.final_triangles, 6);
        assert!(imported.workspace.is_some());
        assert!(imported.appearance_root.join("vkit-import.mtl").is_file());
        assert!(
            imported
                .appearance_root
                .join("textures/fbx_400_generated-albedo.png")
                .is_file()
        );
        let document = load_obj_document(&imported.appearance_path).expect("appearance OBJ");
        assert_eq!(document.geometry.faces.len(), 6);
        assert_eq!(document.appearance.face_texcoord_indices.len(), 6);
        assert!(
            document
                .appearance
                .face_texcoord_indices
                .iter()
                .all(|indices| indices.len() == 3)
        );
        let max_y = imported
            .ordered
            .vertices
            .iter()
            .map(|vertex| vertex[1])
            .fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(max_y, 3.0);
    }

    #[test]
    fn a_gltf_container_reads_its_sibling_binary_payload() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let mut binary = Vec::new();
        for value in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            binary.extend_from_slice(&value.to_le_bytes());
        }
        for index in [0_u16, 1, 2] {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        fs::write(workspace.path().join("scan.bin"), &binary).expect("external buffer fixture");
        let source = workspace.path().join("scan.gltf");
        fs::write(
            &source,
            format!(
                concat!(
                    r#"{{"asset":{{"version":"2.0"}},"#,
                    r#""buffers":[{{"byteLength":{length},"uri":"scan.bin"}}],"#,
                    r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                    r#"{{"buffer":0,"byteOffset":36,"byteLength":6}}],"#,
                    r#""accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","#,
                    r#""min":[0,0,0],"max":[1,1,0]}},"#,
                    r#"{{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}],"#,
                    r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                    r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
                ),
                length = binary.len()
            ),
        )
        .expect("glTF fixture");
        let imported =
            prepare_mesh_import_with_progress(&source, |_| {}).expect("native glTF import");
        assert_eq!(imported.final_triangles, 1);
        assert_eq!(imported.ordered.vertices[0], [0.0, 0.0, 0.0]);
        assert_eq!(imported.ordered.vertices[1], [1.0, 0.0, 0.0]);
        assert_eq!(imported.ordered.vertices[2], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn quantized_positions_are_restored_by_the_node_transform() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = workspace.path().join("quantized.glb");
        let mut binary = Vec::new();
        for vertex in [[0_i16, 0, 0], [2, 0, 0], [0, 4, 0]] {
            for component in vertex {
                binary.extend_from_slice(&component.to_le_bytes());
            }
            binary.extend_from_slice(&[0, 0]);
        }
        for index in [0_u16, 1, 2] {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        let json = concat!(
            r#"{"asset":{"version":"2.0"},"#,
            r#""extensionsUsed":["KHR_mesh_quantization"],"#,
            r#""extensionsRequired":["KHR_mesh_quantization"],"#,
            r#""buffers":[{"byteLength":30}],"#,
            r#""bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":24,"byteStride":8},"#,
            r#"{"buffer":0,"byteOffset":24,"byteLength":6}],"#,
            r#""accessors":[{"bufferView":0,"componentType":5122,"count":3,"type":"VEC3","#,
            r#""min":[0,0,0],"max":[2,4,0]},"#,
            r#"{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],"#,
            r#""meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],"#,
            r#""nodes":[{"mesh":0,"scale":[0.5,0.5,0.5],"translation":[5,0,0]}],"#,
            r#""scenes":[{"nodes":[0]}],"scene":0}"#
        );
        fs::write(&source, wrap_glb(json, &binary)).expect("quantized GLB fixture");
        let imported =
            prepare_mesh_import_with_progress(&source, |_| {}).expect("quantized glTF import");
        assert_eq!(imported.ordered.vertices[0], [5.0, 0.0, 0.0]);
        assert_eq!(
            imported.ordered.vertices[1],
            [6.0, 0.0, 0.0],
            "un-normalized quantized positions are restored by the node scale, not by the reader"
        );
        assert_eq!(imported.ordered.vertices[2], [5.0, 2.0, 0.0]);
    }

    #[test]
    fn a_meshopt_compressed_position_view_is_decompressed_in_place() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = workspace.path().join("packed.glb");
        let encoded = encode_meshopt_vertices(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        let mut binary = encoded.clone();
        while !binary.len().is_multiple_of(4) {
            binary.push(0);
        }
        let index_offset = binary.len();
        for index in [0_u16, 1, 2] {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"#,
                r#""extensionsUsed":["EXT_meshopt_compression"],"#,
                r#""extensionsRequired":["EXT_meshopt_compression"],"#,
                r#""buffers":[{{"byteLength":{buffer_length}}},"#,
                r#"{{"byteLength":36,"extensions":{{"EXT_meshopt_compression":{{"fallback":true}}}}}}],"#,
                r#""bufferViews":[{{"buffer":1,"byteOffset":0,"byteLength":36,"byteStride":12,"#,
                r#""extensions":{{"EXT_meshopt_compression":{{"buffer":0,"byteOffset":0,"#,
                r#""byteLength":{encoded_length},"byteStride":12,"count":3,"mode":"ATTRIBUTES"}}}}}},"#,
                r#"{{"buffer":0,"byteOffset":{index_offset},"byteLength":6}}],"#,
                r#""accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","#,
                r#""min":[0,0,0],"max":[1,1,0]}},"#,
                r#"{{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0,"translation":[2,0,0]}}],"#,
                r#""scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            buffer_length = binary.len(),
            encoded_length = encoded.len(),
            index_offset = index_offset,
        );
        fs::write(&source, wrap_glb(&json, &binary)).expect("meshopt GLB fixture");
        let imported =
            prepare_mesh_import_with_progress(&source, |_| {}).expect("meshopt glTF import");
        assert_eq!(imported.final_triangles, 1);
        assert_eq!(imported.ordered.vertices[0], [2.0, 0.0, 0.0]);
        assert_eq!(imported.ordered.vertices[1], [3.0, 0.0, 0.0]);
        assert_eq!(imported.ordered.vertices[2], [2.0, 1.0, 0.0]);
    }

    #[test]
    fn a_corrupt_index_sequence_view_is_refused_with_its_own_reason() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = workspace.path().join("sequence.glb");
        let json = concat!(
            r#"{"asset":{"version":"2.0"},"#,
            r#""extensionsRequired":["EXT_meshopt_compression"],"#,
            r#""buffers":[{"byteLength":16}],"#,
            r#""bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":12,"#,
            r#""extensions":{"EXT_meshopt_compression":{"buffer":0,"byteOffset":0,"#,
            r#""byteLength":16,"byteStride":4,"count":3,"mode":"INDICES"}}}],"#,
            r#""accessors":[{"bufferView":0,"componentType":5125,"count":3,"type":"SCALAR"}],"#,
            r#""meshes":[{"primitives":[{"attributes":{}}]}],"#,
            r#""nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0}"#
        );
        fs::write(&source, wrap_glb(json, &[0_u8; 16])).expect("index sequence fixture");
        let error = prepare_mesh_import_with_progress(&source, |_| {})
            .expect_err("a stream that is not an index sequence must refuse rather than guess");
        let error = error.to_string();
        assert!(error.contains("bufferView 0"), "{error}");
        assert!(error.contains("decompressed"), "{error}");
    }

    fn triangle_binary() -> Vec<u8> {
        let mut binary = Vec::new();
        for position in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for value in position {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        for index in [0_u16, 1, 2] {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        while !binary.len().is_multiple_of(4) {
            binary.push(0);
        }
        binary
    }

    const TRIANGLE_ACCESSORS: &str = concat!(
        r#""accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","#,
        r#""min":[0,0,0],"max":[1,1,0]},"#,
        r#"{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}"#
    );

    fn write_glb(workspace: &Path, name: &str, json: &str, binary: &[u8]) -> PathBuf {
        let source = workspace.join(name);
        fs::write(&source, wrap_glb(json, binary)).expect("GLB fixture");
        source
    }

    #[test]
    fn a_glb_header_declaring_less_than_its_own_header_is_an_error_not_an_abort() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = workspace.path().join("truncated.glb");
        let mut truncated = b"glTF".to_vec();
        truncated.extend_from_slice(&2_u32.to_le_bytes());
        truncated.extend_from_slice(&0_u32.to_le_bytes());
        truncated.extend_from_slice(&[0_u8; 32]);
        fs::write(&source, truncated).expect("truncated fixture");
        let error = prepare_mesh_import_with_progress(&source, |_| {})
            .expect_err("a partially downloaded GLB must be refused, not allocated for")
            .to_string();
        assert!(error.contains("declares 0 bytes"), "{error}");
    }

    #[test]
    fn a_position_accessor_index_past_the_end_is_our_sentence_not_a_validator_panic() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":44}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}}],"#,
                r#"{accessors}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":7}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            accessors = TRIANGLE_ACCESSORS
        );
        let source = write_glb(workspace.path(), "oob.glb", &json, &triangle_binary());
        let error = prepare_mesh_import_with_progress(&source, |_| {})
            .expect_err("an index past the accessor array must be refused before it is followed")
            .to_string();
        assert!(error.contains("POSITION"), "{error}");
        assert!(error.contains("names entry 7"), "{error}");
    }

    #[test]
    fn a_texcoord_stride_smaller_than_its_element_drops_the_uvs_and_keeps_the_mesh() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let mut binary = triangle_binary();
        let uv_offset = binary.len();
        for uv in [[0.0_f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            for value in uv {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{length}}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}},"#,
                r#"{{"buffer":0,"byteOffset":{uv_offset},"byteLength":24,"byteStride":4}}],"#,
                r#"{accessors},"#,
                r#"{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":2}},"#,
                r#""indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            length = binary.len(),
            uv_offset = uv_offset,
            accessors = TRIANGLE_ACCESSORS
        );
        let source = write_glb(workspace.path(), "understrided.glb", &json, &binary);
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a four byte stride over eight byte UVs used to abort the process");
        assert_eq!(imported.final_triangles, 1);
    }

    #[test]
    fn a_short_texcoord_component_imports_without_uvs_instead_of_killing_the_process() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let mut binary = triangle_binary();
        let uv_offset = binary.len();
        for uv in [[0_i16, 0], [32767, 0], [0, 32767]] {
            for value in uv {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{length}}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}},"#,
                r#"{{"buffer":0,"byteOffset":{uv_offset},"byteLength":12}}],"#,
                r#"{accessors},"#,
                r#"{{"bufferView":2,"componentType":5122,"normalized":true,"count":3,"#,
                r#""type":"VEC2"}}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":2}},"#,
                r#""indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            length = binary.len(),
            uv_offset = uv_offset,
            accessors = TRIANGLE_ACCESSORS
        );
        let source = write_glb(workspace.path(), "short_uv.glb", &json, &binary);
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a SHORT TEXCOORD used to reach unreachable!() and abort the process");
        assert_eq!(imported.final_triangles, 1);
    }

    #[test]
    fn a_line_primitive_beside_a_triangle_one_keeps_the_triangle() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":44}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}}],"#,
                r#"{accessors}],"#,
                r#""meshes":[{{"primitives":["#,
                r#"{{"attributes":{{"POSITION":0}},"indices":1,"mode":1}},"#,
                r#"{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            accessors = TRIANGLE_ACCESSORS
        );
        let source = write_glb(
            workspace.path(),
            "mixed_modes.glb",
            &json,
            &triangle_binary(),
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a LINES primitive carries no surface and must not refuse the file");
        assert_eq!(imported.final_triangles, 1);
    }

    #[test]
    fn a_triangle_strip_primitive_expands_rather_than_refusing() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let mut binary = Vec::new();
        for position in [
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ] {
            for value in position {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        let index_offset = binary.len();
        for index in [0_u16, 1, 2, 3] {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{length}}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":48}},"#,
                r#"{{"buffer":0,"byteOffset":{index_offset},"byteLength":8}}],"#,
                r#""accessors":[{{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","#,
                r#""min":[0,0,0],"max":[1,1,0]}},"#,
                r#"{{"bufferView":1,"componentType":5123,"count":4,"type":"SCALAR"}}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1,"#,
                r#""mode":5}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            length = binary.len(),
            index_offset = index_offset
        );
        let source = write_glb(workspace.path(), "strip.glb", &json, &binary);
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a triangle strip carries exactly the surface this importer wants");
        assert_eq!(imported.final_triangles, 2);
    }

    #[test]
    fn morph_targets_no_longer_refuse_a_file_whose_base_positions_are_complete() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":44}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}}],"#,
                r#"{accessors}],"#,
                r#""meshes":[{{"weights":[0.5],"primitives":[{{"attributes":{{"POSITION":0}},"#,
                r#""indices":1,"targets":[{{"POSITION":0}}]}}]}}],"#,
                r#""nodes":[{{"mesh":0,"weights":[0.5]}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            accessors = TRIANGLE_ACCESSORS
        );
        let source = write_glb(workspace.path(), "morphed.glb", &json, &triangle_binary());
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("morph targets are deltas the reader never consults");
        assert_eq!(imported.final_triangles, 1);
        assert_eq!(imported.ordered.vertices[1], [1.0, 0.0, 0.0]);
    }

    #[test]
    fn a_sparse_position_accessor_overlays_its_base_stream() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let mut binary = triangle_binary();
        let sparse_index_offset = binary.len();
        binary.extend_from_slice(&2_u16.to_le_bytes());
        while !binary.len().is_multiple_of(4) {
            binary.push(0);
        }
        let sparse_value_offset = binary.len();
        for value in [0.0_f32, 2.0, 0.0] {
            binary.extend_from_slice(&value.to_le_bytes());
        }
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{length}}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}},"#,
                r#"{{"buffer":0,"byteOffset":{index_offset},"byteLength":2}},"#,
                r#"{{"buffer":0,"byteOffset":{value_offset},"byteLength":12}}],"#,
                r#""accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","#,
                r#""min":[0,0,0],"max":[1,2,0],"sparse":{{"count":1,"#,
                r#""indices":{{"bufferView":2,"componentType":5123}},"#,
                r#""values":{{"bufferView":3}}}}}},"#,
                r#"{{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            length = binary.len(),
            index_offset = sparse_index_offset,
            value_offset = sparse_value_offset
        );
        let source = write_glb(workspace.path(), "sparse.glb", &json, &binary);
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("sparse storage is how Blender and gltf-transform write a mostly-flat stream");
        assert_eq!(imported.final_triangles, 1);
        assert_eq!(
            imported.ordered.vertices[2],
            [0.0, 2.0, 0.0],
            "the sparse override replaces the base value for the vertex it names"
        );
    }

    #[test]
    fn one_non_finite_vertex_costs_its_triangle_and_not_the_document() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let mut binary = Vec::new();
        for position in [
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [f32::NAN, 1.0, 0.0],
        ] {
            for value in position {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        let index_offset = binary.len();
        for index in [0_u16, 1, 2, 1, 3, 2] {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        while !binary.len().is_multiple_of(4) {
            binary.push(0);
        }
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{length}}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":48}},"#,
                r#"{{"buffer":0,"byteOffset":{index_offset},"byteLength":12}}],"#,
                r#""accessors":[{{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","#,
                r#""min":[0,0,0],"max":[1,1,0]}},"#,
                r#"{{"bufferView":1,"componentType":5123,"count":6,"type":"SCALAR"}}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            length = binary.len(),
            index_offset = index_offset
        );
        let source = write_glb(workspace.path(), "nan.glb", &json, &binary);
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a photogrammetry scan with three bad verts out of 200k is still a scan");
        assert_eq!(imported.final_triangles, 1);
    }

    #[test]
    fn a_node_reached_twice_is_emitted_once_rather_than_walked_forever() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":44}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}}],"#,
                r#"{accessors}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"children":[1,1]}},{{"mesh":0,"children":[0]}}],"#,
                r#""scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            accessors = TRIANGLE_ACCESSORS
        );
        let source = write_glb(workspace.path(), "cyclic.glb", &json, &triangle_binary());
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a node graph that is not a forest must terminate, not multiply");
        assert_eq!(imported.final_triangles, 1);
    }

    #[test]
    fn a_percent_encoded_sibling_buffer_is_decoded_before_it_is_checked() {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::write(workspace.path().join("scan data.bin"), triangle_binary())
            .expect("sibling payload");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"#,
                r#""buffers":[{{"byteLength":44,"uri":"scan%20data.bin"}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}}],"#,
                r#"{accessors}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            accessors = TRIANGLE_ACCESSORS
        );
        let source = workspace.path().join("scan.gltf");
        fs::write(&source, json).expect("gltf fixture");
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("RFC 3986 escaping is mandatory in a glTF URI, not a reason to refuse");
        assert_eq!(imported.final_triangles, 1);
    }

    #[test]
    fn a_percent_encoded_traversal_is_still_refused_after_it_is_decoded() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let inside = workspace.path().join("assets");
        fs::create_dir_all(&inside).expect("nested folder");
        fs::write(workspace.path().join("outside.bin"), triangle_binary())
            .expect("outside payload");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"#,
                r#""buffers":[{{"byteLength":44,"uri":"%2e%2e%2foutside.bin"}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}}],"#,
                r#"{accessors}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            accessors = TRIANGLE_ACCESSORS
        );
        let source = inside.join("scan.gltf");
        fs::write(&source, json).expect("gltf fixture");
        let error = prepare_mesh_import_with_progress(&source, |_| {})
            .expect_err("decoding before checking must not turn a fix into a traversal")
            .to_string();
        assert!(error.contains("safe relative path"), "{error}");
    }

    #[test]
    fn a_bom_and_a_line_wrapped_data_uri_both_read() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let encoded = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(triangle_binary())
        };
        let wrapped = encoded
            .as_bytes()
            .chunks(24)
            .map(|line| String::from_utf8_lossy(line).into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"#,
                r#""buffers":[{{"byteLength":44,"uri":"data:application/octet-stream;base64,{payload}"}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}}],"#,
                r#"{accessors}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            payload = wrapped.replace('\n', "\\n"),
            accessors = TRIANGLE_ACCESSORS
        );
        let source = workspace.path().join("embedded.gltf");
        let mut bytes = b"\xef\xbb\xbf".to_vec();
        bytes.extend_from_slice(json.as_bytes());
        fs::write(&source, bytes).expect("gltf fixture");
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a Notepad BOM and a wrapped payload are both readable everywhere else");
        assert_eq!(imported.final_triangles, 1);
    }

    #[test]
    fn a_final_buffer_view_that_over_declares_by_the_alignment_pad_still_finds_position() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let mut binary = Vec::new();
        for position in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for value in position {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        for index in [0_u16, 1, 2] {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        fs::write(workspace.path().join("scan.bin"), &binary).expect("unpadded sibling payload");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"#,
                r#""buffers":[{{"byteLength":44,"uri":"scan.bin"}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":8}}],"#,
                r#"{accessors}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            accessors = TRIANGLE_ACCESSORS
        );
        let source = workspace.path().join("scan.gltf");
        fs::write(&source, json).expect("gltf fixture");
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a four-aligned final view is why the gltf crate's own importer pads");
        assert_eq!(imported.final_triangles, 1);
    }

    /// A one-triangle GLB carrying `POSITION`, `NORMAL` and indices, plus whatever node array the
    /// caller wants to hang it under. The scene-graph tests all differ only in that array, so it is
    /// the one thing this helper leaves to the caller.
    fn normal_triangle_glb(
        workspace: &Path,
        name: &str,
        nodes: &str,
        scene_roots: &str,
        extra_root: &str,
    ) -> PathBuf {
        let mut binary = Vec::new();
        for value in FLAT_TRIANGLE
            .iter()
            .flatten()
            .chain(UP_NORMALS.iter().flatten())
        {
            binary.extend_from_slice(&value.to_le_bytes());
        }
        for index in [0_u16, 1, 2] {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        while !binary.len().is_multiple_of(4) {
            binary.push(0);
        }
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{length}}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":72,"byteLength":6}}],"#,
                r#""accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","#,
                r#""min":[-1,-1,-1],"max":[1,1,1]}},"#,
                r#"{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},"#,
                r#"{{"bufferView":2,"componentType":5123,"count":3,"type":"SCALAR"}}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"#,
                r#""indices":2}}]}}],"#,
                r#"{extra_root}"nodes":{nodes},"scenes":[{{"nodes":{scene_roots}}}],"scene":0}}"#
            ),
            length = binary.len(),
            extra_root = extra_root,
            nodes = nodes,
            scene_roots = scene_roots
        );
        write_glb(workspace, name, &json, &binary)
    }

    /// The signed area of the first imported triangle projected on the XY plane, which is the `z`
    /// component of its geometric normal. Its sign is the winding, and winding is the only record
    /// of facing this importer keeps: no normal survives the import, so a flip that reaches the
    /// mesh is invisible everywhere except here.
    fn first_triangle_facing(imported: &PreparedMeshImport) -> f64 {
        let face = &imported.ordered.faces[0].vertex_indices;
        let corner = |at: usize| imported.ordered.vertices[face[at] as usize];
        let (a, b, c) = (corner(0), corner(1), corner(2));
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    }

    const FLAT_TRIANGLE: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    const UP_NORMALS: [[f32; 3]; 3] = [[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]];

    #[test]
    fn a_negative_determinant_parent_flips_the_winding_of_every_child_triangle() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let upright = normal_triangle_glb(
            workspace.path(),
            "upright.glb",
            r#"[{"mesh":0}]"#,
            "[0]",
            "",
        );
        let mirrored = normal_triangle_glb(
            workspace.path(),
            "mirrored.glb",
            r#"[{"mesh":0},{"children":[0],"scale":[-1,1,1]}]"#,
            "[1]",
            "",
        );
        let upright = prepare_mesh_import_with_progress(&upright, |_| {}).expect("upright imports");
        let mirrored =
            prepare_mesh_import_with_progress(&mirrored, |_| {}).expect("mirrored imports");
        let upright = first_triangle_facing(&upright);
        let mirrored = first_triangle_facing(&mirrored);
        assert!(upright > 0.0, "{upright}");
        assert!(
            mirrored > 0.0,
            "a parent with a negative determinant mirrors the vertices, so the index order has to \
             be swapped to keep the surface facing outward; without the swap every normal points \
             inward and the fit is wrong in a way that looks almost right: {mirrored}"
        );
    }

    #[test]
    fn a_non_uniform_scale_reaches_the_positions_and_leaves_the_winding_alone() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = normal_triangle_glb(
            workspace.path(),
            "stretched.glb",
            r#"[{"mesh":0,"scale":[2,4,8]}]"#,
            "[0]",
            "",
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {}).expect("imports");
        let extent = |axis: usize| {
            let values = imported
                .ordered
                .vertices
                .iter()
                .map(|vertex| vertex[axis])
                .collect::<Vec<_>>();
            values.iter().copied().fold(f64::MIN, f64::max)
                - values.iter().copied().fold(f64::MAX, f64::min)
        };
        assert!((extent(0) - 2.0).abs() < 1.0e-9, "{}", extent(0));
        assert!((extent(1) - 4.0).abs() < 1.0e-9, "{}", extent(1));
        assert!(
            first_triangle_facing(&imported) > 0.0,
            "a positive determinant never reverses winding, however uneven the scale"
        );
    }

    #[test]
    fn a_camera_and_a_light_beside_a_mesh_are_inert_rather_than_geometry() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = normal_triangle_glb(
            workspace.path(),
            "studio.glb",
            concat!(
                r#"[{"mesh":0},"#,
                r#"{"camera":0,"translation":[0,0,4]},"#,
                r#"{"extensions":{"KHR_lights_punctual":{"light":0}}},"#,
                r#"{"mesh":0,"translation":[10,0,0]}]"#
            ),
            "[0,1,2,3]",
            r#""cameras":[{"type":"perspective","perspective":{"yfov":1.0,"znear":0.1}}],"#,
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {}).expect("imports");
        assert_eq!(
            imported.final_triangles, 2,
            "a scan exported beside its camera and its light is two mesh nodes and two inert ones"
        );
    }

    #[test]
    fn a_hierarchy_past_the_recursion_cap_costs_its_subtree_not_the_file() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let mut nodes = vec![r#"{"mesh":0}"#.to_owned()];
        for step in 1..400 {
            nodes.push(format!(r#"{{"children":[{}]}}"#, step - 1));
        }
        let deep = nodes.len() - 1;
        nodes.push(r#"{"mesh":0,"translation":[5,0,0]}"#.to_owned());
        let shallow = nodes.len() - 1;
        let source = normal_triangle_glb(
            workspace.path(),
            "deep.glb",
            &format!("[{}]", nodes.join(",")),
            &format!("[{deep},{shallow}]"),
            "",
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a 400-level chain used to refuse the whole document");
        assert_eq!(
            imported.final_triangles, 1,
            "the deep chain's mesh sits past the recursion cap and is stepped over; the sibling \
             the user actually came for still imports"
        );
    }

    #[test]
    fn a_non_finite_composed_transform_costs_its_subtree_not_the_file() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let mut nodes = vec![r#"{"mesh":0}"#.to_owned()];
        for step in 1..=9 {
            nodes.push(format!(
                r#"{{"children":[{}],"scale":[1e38,1e38,1e38]}}"#,
                step - 1
            ));
        }
        let overflowing = nodes.len() - 1;
        nodes.push(r#"{"mesh":0,"translation":[5,0,0]}"#.to_owned());
        let sibling = nodes.len() - 1;
        let source = normal_triangle_glb(
            workspace.path(),
            "overflow.glb",
            &format!("[{}]", nodes.join(",")),
            &format!("[{overflowing},{sibling}]"),
            "",
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("an overflowing branch used to refuse the whole document");
        assert_eq!(
            imported.final_triangles, 1,
            "nine stacked 1e38 scales overflow f64 partway down the chain, so that branch is stepped over; the sibling mesh is untouched by it"
        );
    }

    fn wound_fan(reversed: bool) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u16>) {
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();
        for step in 0..24_u16 {
            let angle = f32::from(step) * 0.25;
            positions.extend_from_slice(&[
                [0.0, 0.0, 0.0],
                [angle.cos(), angle.sin(), 0.0],
                [(angle + 0.2).cos(), (angle + 0.2).sin(), 0.0],
            ]);
            normals.extend_from_slice(&[[0.0, 0.0, 1.0]; 3]);
            let base = step * 3;
            if reversed {
                indices.extend_from_slice(&[base, base + 2, base + 1]);
            } else {
                indices.extend_from_slice(&[base, base + 1, base + 2]);
            }
        }
        (positions, normals, indices)
    }

    #[test]
    fn an_index_buffer_reversed_without_its_normals_is_put_back_the_right_way_round() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let (positions, normals, indices) = wound_fan(true);
        let source = fan_glb(
            workspace.path(),
            "flipped.glb",
            &positions,
            &normals,
            &indices,
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a wound-backwards mesh still imports");
        assert!(
            first_triangle_facing(&imported) > 0.0,
            "aiProcess_FlipWindingOrder reverses the index buffer and leaves NORMAL alone; the \
             determinant is blind to that, so the shipped normals are the only vote there is"
        );
    }

    #[test]
    fn normals_that_agree_with_the_index_order_change_nothing() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let (positions, normals, indices) = wound_fan(false);
        let source = fan_glb(
            workspace.path(),
            "agreeing.glb",
            &positions,
            &normals,
            &indices,
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {}).expect("imports");
        assert!(
            first_triangle_facing(&imported) > 0.0,
            "the vote may only ever confirm what the author wrote"
        );
    }

    fn fan_glb(
        workspace: &Path,
        name: &str,
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        indices: &[u16],
    ) -> PathBuf {
        let mut binary = Vec::new();
        for value in positions.iter().flatten().chain(normals.iter().flatten()) {
            binary.extend_from_slice(&value.to_le_bytes());
        }
        let index_offset = binary.len();
        for index in indices {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        while !binary.len().is_multiple_of(4) {
            binary.push(0);
        }
        let stream = positions.len() * 12;
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{length}}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":{stream}}},"#,
                r#"{{"buffer":0,"byteOffset":{stream},"byteLength":{stream}}},"#,
                r#"{{"buffer":0,"byteOffset":{index_offset},"byteLength":{index_length}}}],"#,
                r#""accessors":[{{"bufferView":0,"componentType":5126,"count":{count},"#,
                r#""type":"VEC3","min":[-1,-1,-1],"max":[1,1,1]}},"#,
                r#"{{"bufferView":1,"componentType":5126,"count":{count},"type":"VEC3"}},"#,
                r#"{{"bufferView":2,"componentType":5123,"count":{indices},"type":"SCALAR"}}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"#,
                r#""indices":2}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            length = binary.len(),
            stream = stream,
            index_offset = index_offset,
            index_length = indices.len() * 2,
            count = positions.len(),
            indices = indices.len()
        );
        write_glb(workspace, name, &json, &binary)
    }

    #[test]
    fn gpu_instancing_emits_every_copy_rather_than_only_the_first() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let mut binary = triangle_binary();
        let instance_offset = binary.len();
        for translation in [[0.0_f32, 0.0, 0.0], [4.0, 0.0, 0.0], [8.0, 0.0, 0.0]] {
            for value in translation {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"#,
                r#""extensionsUsed":["EXT_mesh_gpu_instancing"],"#,
                r#""extensionsRequired":["EXT_mesh_gpu_instancing"],"#,
                r#""buffers":[{{"byteLength":{length}}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}},"#,
                r#"{{"buffer":0,"byteOffset":{instance_offset},"byteLength":36}}],"#,
                r#"{accessors},"#,
                r#"{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC3"}}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0,"extensions":{{"EXT_mesh_gpu_instancing":"#,
                r#"{{"attributes":{{"TRANSLATION":2}}}}}}}}],"#,
                r#""scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            length = binary.len(),
            instance_offset = instance_offset,
            accessors = TRIANGLE_ACCESSORS
        );
        let source = write_glb(workspace.path(), "instanced.glb", &json, &binary);
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("gltfpack -ki output used to be refused for requiring the extension");
        assert_eq!(
            imported.final_triangles, 3,
            "every instance is geometry; before this the extension was ignored and all copies but \
             one vanished with no error anywhere"
        );
        let spread = imported
            .ordered
            .vertices
            .iter()
            .map(|vertex| vertex[0])
            .fold(f64::MIN, f64::max);
        assert!(
            (spread - 9.0).abs() < 1.0e-9,
            "the instance translations have to reach the vertices: {spread}"
        );
    }

    fn gltf_corpus() -> Option<PathBuf> {
        std::env::var_os("VKIT_GLTF_CORPUS").map(PathBuf::from)
    }

    /// The largest distance from any answer-key vertex to the nearest vertex of `other`.
    ///
    /// Position order is not comparable across a Draco decode — the codec stores its own point
    /// order and the weld renumbers what survives — so agreement is measured as a nearest-vertex
    /// distance rather than index by index. It is the one-sided worst case, which is what a
    /// missing or displaced vertex would show up in.
    fn worst_nearest_vertex(key: &[[f64; 3]], other: &[[f64; 3]]) -> f64 {
        let mut worst = 0.0_f64;
        for want in key {
            let mut best = f64::MAX;
            for have in other {
                let delta = [have[0] - want[0], have[1] - want[1], have[2] - want[2]];
                let squared = delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2];
                if squared < best {
                    best = squared;
                }
            }
            worst = worst.max(best.sqrt());
        }
        worst
    }

    /// Six times the signed volume swept by a mesh's faces about its own centroid.
    ///
    /// Referring every corner to the centroid makes this translation invariant, which matters
    /// because it is compared across files whose nodes park the same mesh in different places.
    /// Under a linear map it scales by that map's determinant, and reversing the index order
    /// negates it — so its SIGN is exactly the question the winding flip exists to answer. A
    /// negative-determinant parent must not change it: the mirror flips the sign and the corner
    /// swap flips it back.
    fn signed_volume(imported: &PreparedMeshImport) -> f64 {
        let vertices = &imported.ordered.vertices;
        if vertices.is_empty() {
            return 0.0;
        }
        let mut centroid = [0.0_f64; 3];
        for vertex in vertices {
            for (axis, value) in centroid.iter_mut().enumerate() {
                *value += vertex[axis];
            }
        }
        for value in &mut centroid {
            *value /= vertices.len() as f64;
        }
        let mut total = 0.0_f64;
        for face in &imported.ordered.faces {
            let corners = &face.vertex_indices;
            for corner in 1..corners.len().saturating_sub(1) {
                let pick = |at: usize| {
                    let vertex = vertices[corners[at] as usize];
                    [
                        vertex[0] - centroid[0],
                        vertex[1] - centroid[1],
                        vertex[2] - centroid[2],
                    ]
                };
                let (a, b, c) = (pick(0), pick(corner), pick(corner + 1));
                let cross = [
                    b[1] * c[2] - b[2] * c[1],
                    b[2] * c[0] - b[0] * c[2],
                    b[0] * c[1] - b[1] * c[0],
                ];
                total += a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2];
            }
        }
        total
    }

    /// Replays every corpus file through the real importer and prints the table.
    ///
    /// This is the whole-corpus receipt rather than an assertion of one property, so it runs only
    /// when the corpus is present and prints under `--nocapture`. It still asserts: the three files
    /// that are literally the same mesh as the answer key must match it exactly, and the Draco pair
    /// must land inside one quantization step of it.
    #[test]
    fn the_whole_corpus_replays_through_the_real_importer() {
        let Some(corpus) = gltf_corpus() else {
            return;
        };
        let key_path = corpus.join("01_plain.glb");
        if !key_path.is_file() {
            return;
        }
        let key = prepare_mesh_import_with_progress(&key_path, |_| {}).expect("the answer key");
        let key_vertices = key.ordered.vertices.clone();
        let extent = (0..3)
            .map(|axis| {
                let values = key_vertices.iter().map(|vertex| vertex[axis]);
                let (low, high) = values.fold((f64::MAX, f64::MIN), |(low, high), value| {
                    (low.min(value), high.max(value))
                });
                high - low
            })
            .fold(0.0_f64, f64::max);
        let step = extent / f64::from((1_u32 << 14) - 1);

        println!("\nfile | imports | vertices | triangles | worst distance to 01_plain");
        let key_volume = signed_volume(&key);
        let mut volumes = Vec::new();
        let mut same_mesh = Vec::new();
        let mut draco = Vec::new();
        let mut matrix_read = 0_usize;
        let mut matrix_refused = Vec::new();
        let mut names = [
            "01_plain.glb",
            "02_separate.gltf",
            "03_embedded.gltf",
            "04_draco.glb",
            "05_draco_separate.gltf",
            "06_draco_max.glb",
            "07_no_uv.glb",
            "08_flat_shaded.glb",
            "09_mirrored_parent.glb",
            "10_multi_root.glb",
            "11_z_up.glb",
        ]
        .into_iter()
        .map(|name| (corpus.join(name), name.to_owned()))
        .collect::<Vec<_>>();
        let matrix = corpus
            .parent()
            .map(|parent| parent.join("draco_matrix"))
            .filter(|matrix| matrix.is_dir());
        if let Some(matrix) = matrix.as_ref() {
            let mut sample = fs::read_dir(matrix)
                .expect("the draco matrix directory")
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    extension(path).as_deref() == Some("glb")
                        && path
                            .file_name()
                            .is_some_and(|name| name.to_string_lossy().starts_with("d_"))
                })
                .collect::<Vec<_>>();
            sample.sort();
            for path in sample {
                let name = format!(
                    "draco_matrix/{}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                );
                names.push((path, name));
            }
        }
        for (path, name) in names {
            if !path.is_file() {
                continue;
            }
            let matrix_file = name.starts_with("draco_matrix/");
            match prepare_mesh_import_with_progress(&path, |_| {}) {
                Ok(imported) => {
                    let vertices = imported.ordered.vertices.len();
                    let triangles = imported.final_triangles;
                    let distance = worst_nearest_vertex(&key_vertices, &imported.ordered.vertices);
                    println!(
                        "{name} | yes | {vertices} | {triangles} | {distance:.3e} \
                         ({:.2} steps)",
                        distance / step
                    );
                    if matches!(
                        name.as_str(),
                        "02_separate.gltf" | "03_embedded.gltf" | "11_z_up.glb"
                    ) {
                        same_mesh.push((name.clone(), vertices, triangles));
                    }
                    if matches!(name.as_str(), "09_mirrored_parent.glb" | "11_z_up.glb") {
                        volumes.push((name.clone(), signed_volume(&imported)));
                    }
                    if matches!(name.as_str(), "04_draco.glb" | "05_draco_separate.gltf") {
                        draco.push((name.clone(), vertices, triangles, distance));
                    }
                    if matrix_file {
                        matrix_read += 1;
                    }
                }
                Err(error) => {
                    println!("{name} | NO | - | - | {error}");
                    if matrix_file {
                        matrix_refused.push(name.clone());
                    }
                    assert!(
                        name == "06_draco_max.glb"
                            || name.contains("level09")
                            || name.contains("level10"),
                        "{name} must import: {error}"
                    );
                }
            }
        }
        for (name, volume) in volumes {
            assert!(
                volume * key_volume > 0.0,
                "{name} carries the same mesh as the answer key under a node transform, so the surface has to end up facing the same way round; the mirrored parent's negative determinant negates the swept volume and only the corner swap puts it back: {volume:e} against {key_volume:e}"
            );
            assert!(
                (volume.abs() / key_volume.abs() - 1.0).abs() < 0.02,
                "{name} scales the swept volume by the magnitude of its determinant, which is one here: {volume:e} against {key_volume:e}"
            );
        }
        for (name, vertices, triangles) in same_mesh {
            assert_eq!(
                (vertices, triangles),
                (key_vertices.len(), key.final_triangles),
                "{name} is the same mesh as the answer key, only laid out differently"
            );
        }
        for (name, vertices, triangles, distance) in draco {
            assert_eq!(
                (vertices, triangles),
                (key_vertices.len(), key.final_triangles)
            );
            assert!(
                distance <= step,
                "{name} is {distance:e} from the answer key, past the {step:e} quantization step \
                 the file was packed at"
            );
        }
        if matrix.is_some() {
            assert!(
                matrix_read >= 21,
                "only {matrix_read} of the matrix decoded"
            );
            assert_eq!(
                matrix_refused.len(),
                2,
                "levels 9 and 10 are the two draco-core refuses: {matrix_refused:?}"
            );
        }
    }

    #[test]
    fn the_blender_corpus_imports_and_agrees_with_its_own_answer_key() {
        let Some(corpus) = gltf_corpus() else {
            return;
        };
        let mut counts = Vec::new();
        for name in [
            "01_plain.glb",
            "02_separate.gltf",
            "03_embedded.gltf",
            "07_no_uv.glb",
            "08_flat_shaded.glb",
            "09_mirrored_parent.glb",
            "10_multi_root.glb",
            "11_z_up.glb",
        ] {
            let source = corpus.join(name);
            if !source.is_file() {
                continue;
            }
            let imported = prepare_mesh_import_with_progress(&source, |_| {})
                .unwrap_or_else(|error| panic!("{name} must import: {error}"));
            assert!(
                imported.final_triangles > 0,
                "{name} imported no geometry at all"
            );
            counts.push((
                name,
                imported.final_triangles,
                imported.ordered.vertices.len(),
            ));
        }
        assert!(
            !counts.is_empty(),
            "the corpus directory held none of the files"
        );
        let plain = counts.iter().find(|(name, _, _)| *name == "01_plain.glb");
        if let Some(&(_, triangles, vertices)) = plain {
            for &(name, other_triangles, other_vertices) in &counts {
                if matches!(
                    name,
                    "02_separate.gltf" | "03_embedded.gltf" | "11_z_up.glb"
                ) {
                    assert_eq!(
                        (other_triangles, other_vertices),
                        (triangles, vertices),
                        "{name} is the same mesh as 01_plain.glb and must decode to the same counts"
                    );
                }
            }
        }
    }

    #[test]
    fn the_draco_corpus_pair_decodes_to_the_uncompressed_answer_key() {
        let Some(corpus) = gltf_corpus() else {
            return;
        };
        let key = corpus.join("01_plain.glb");
        if !key.is_file() {
            return;
        }
        let key = prepare_mesh_import_with_progress(&key, |_| {}).expect("the answer key imports");
        for name in ["04_draco.glb", "05_draco_separate.gltf"] {
            let source = corpus.join(name);
            if !source.is_file() {
                continue;
            }
            let imported = prepare_mesh_import_with_progress(&source, |_| {})
                .unwrap_or_else(|error| panic!("{name} must import: {error}"));
            assert_eq!(
                imported.final_triangles, key.final_triangles,
                "{name} is the same mesh as 01_plain.glb"
            );
            assert_eq!(imported.ordered.vertices.len(), key.ordered.vertices.len());
        }
        let matrix = corpus
            .parent()
            .map(|parent| parent.join("draco_matrix"))
            .filter(|matrix| matrix.is_dir());
        if let Some(matrix) = matrix {
            let mut read = 0_usize;
            let mut refused = Vec::new();
            let mut files = fs::read_dir(&matrix)
                .expect("the draco matrix directory")
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| extension(path).as_deref() == Some("glb"))
                .collect::<Vec<_>>();
            files.sort();
            for source in files {
                let name = source
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                match prepare_mesh_import_with_progress(&source, |_| {}) {
                    Ok(imported) => {
                        assert!(
                            imported.final_triangles <= key.final_triangles
                                && imported.final_triangles * 10 >= key.final_triangles * 9,
                            "{name} sweeps compression settings over one mesh, so it may only \
                             lose triangles to the quantization grid collapsing neighbours onto \
                             one point: {} against {}",
                            imported.final_triangles,
                            key.final_triangles
                        );
                        read += 1;
                    }
                    Err(error) => refused.push(format!("{name}: {error}")),
                }
            }
            assert!(
                read >= 21,
                "only {read} of the compression matrix decoded; refusals were {refused:?}"
            );
            for reason in &refused {
                assert!(
                    reason.contains("Draco-decoded"),
                    "a refusal must name the decode, not leak a crate message: {reason}"
                );
            }
        }
        let refused = corpus.join("06_draco_max.glb");
        if refused.is_file() {
            let error = prepare_mesh_import_with_progress(&refused, |_| {})
                .err()
                .map(|error| error.to_string());
            if let Some(error) = error {
                assert!(
                    error.contains("Draco-decoded"),
                    "a compression level this decoder cannot read must say so: {error}"
                );
            }
        }
    }

    #[test]
    fn an_unreadable_required_extension_is_named_in_our_own_words() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = workspace.path().join("unknown_extension.glb");
        let json = concat!(
            r#"{"asset":{"version":"2.0"},"#,
            r#""extensionsRequired":["EXT_unheard_of","EXT_invented_here"],"#,
            r#""scenes":[{"nodes":[]}],"scene":0}"#
        );
        fs::write(&source, wrap_glb(json, &[])).expect("unknown extension fixture");
        let error = prepare_mesh_import_with_progress(&source, |_| {})
            .expect_err("a required extension we cannot read must be refused")
            .to_string();
        assert!(error.contains("EXT_unheard_of"), "{error}");
        assert!(error.contains("EXT_invented_here"), "{error}");
        assert!(
            !error.contains("Unsupported extension"),
            "the gltf crate's verdict must never be what the user reads: {error}"
        );
    }

    /// A one-triangle payload plus a UV stream and eight bytes standing in for an image, laid out
    /// so the four bufferViews below always name the same ranges. Every appearance test shares it,
    /// so each one differs from the next only in the JSON under test.
    fn textured_binary() -> (Vec<u8>, usize, usize) {
        let mut binary = triangle_binary();
        let uv_offset = binary.len();
        for uv in [[0.0_f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            for value in uv {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        let image_offset = binary.len();
        binary.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
        (binary, uv_offset, image_offset)
    }

    /// Wraps the shared payload around whatever `images`, `textures` and `materials` a test wants
    /// to put in front of the material writer.
    fn textured_glb(appearance: &str) -> Vec<u8> {
        let (binary, uv_offset, image_offset) = textured_binary();
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{length}}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}},"#,
                r#"{{"buffer":0,"byteOffset":{uv_offset},"byteLength":24}},"#,
                r#"{{"buffer":0,"byteOffset":{image_offset},"byteLength":8}}],"#,
                r#"{accessors},"#,
                r#"{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}}],"#,
                r#"{appearance},"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":2}},"#,
                r#""indices":1,"material":0}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            length = binary.len(),
            uv_offset = uv_offset,
            image_offset = image_offset,
            accessors = TRIANGLE_ACCESSORS,
            appearance = appearance
        );
        wrap_glb(&json, &binary)
    }

    fn material_library(imported: &PreparedMeshImport) -> String {
        fs::read_to_string(imported.appearance_root.join("vkit-import.mtl"))
            .expect("the material library is written whatever happened to the textures")
    }

    #[test]
    fn an_image_format_this_build_cannot_decode_costs_the_map_and_not_the_scan() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = write_bytes(
            workspace.path(),
            "ktx2.glb",
            &textured_glb(concat!(
                r#""images":[{"bufferView":3,"mimeType":"image/ktx2"}],"#,
                r#""textures":[{"source":0}],"#,
                r#""materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}]"#
            )),
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a KTX2 texture is a texture Vkit cannot read, not a file it cannot open");
        assert_eq!(imported.final_triangles, 1);
        let mtl = material_library(&imported);
        assert!(mtl.contains("newmtl"), "{mtl}");
        assert!(
            !mtl.contains("map_Kd"),
            "the material keeps its colour factors and loses only the map: {mtl}"
        );
    }

    #[test]
    fn a_webp_texture_is_extracted_from_the_extension_that_declares_it() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = write_bytes(
            workspace.path(),
            "webp.glb",
            &textured_glb(concat!(
                r#""images":[{"bufferView":3,"mimeType":"image/webp"}],"#,
                r#""textures":[{"extensions":{"EXT_texture_webp":{"source":0}}}],"#,
                r#""materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}]"#
            )),
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {}).expect(
            "a texture whose only image sits under EXT_texture_webp used to fail JSON validation",
        );
        assert_eq!(imported.final_triangles, 1);
        assert!(
            material_library(&imported).contains("map_Kd textures/image_0.webp"),
            "the image crate decodes WebP in pure Rust, so the map is kept rather than dropped"
        );
        assert!(
            imported
                .appearance_root
                .join("textures/image_0.webp")
                .is_file()
        );
    }

    #[test]
    fn a_texture_with_no_image_anywhere_leaves_the_geometry_alone() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = write_bytes(
            workspace.path(),
            "sourceless.glb",
            &textured_glb(concat!(
                r#""images":[{"bufferView":3,"mimeType":"image/png"}],"#,
                r#""textures":[{"extensions":{"KHR_texture_basisu":{}}}],"#,
                r#""materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}]"#
            )),
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a texture object naming no image is a texture problem, not a file problem");
        assert_eq!(imported.final_triangles, 1);
        assert!(!material_library(&imported).contains("map_Kd"));
    }

    #[test]
    fn an_image_with_a_buffer_view_and_no_mime_type_no_longer_kills_the_process() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = write_bytes(
            workspace.path(),
            "mimeless.glb",
            &textured_glb(concat!(
                r#""images":[{"bufferView":3}],"#,
                r#""textures":[{"source":0}],"#,
                r#""materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}]"#
            )),
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {}).expect(
            "gltf::Image::source unwraps the MIME type of a bufferView image, and under \
             panic=abort that unwrap takes the whole app with it",
        );
        assert_eq!(imported.final_triangles, 1);
        assert!(!material_library(&imported).contains("map_Kd"));
    }

    #[test]
    fn a_data_uri_image_that_is_not_base64_costs_only_its_texture() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = write_bytes(
            workspace.path(),
            "plain_data_uri.glb",
            &textured_glb(concat!(
                r#""images":[{"uri":"data:image/png,not-encoded-at-all"}],"#,
                r#""textures":[{"source":0}],"#,
                r#""materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}]"#
            )),
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("an image encoding this reader will not guess at is not a geometry problem");
        assert_eq!(imported.final_triangles, 1);
        assert!(!material_library(&imported).contains("map_Kd"));
    }

    #[test]
    fn a_missing_sidecar_texture_imports_the_way_the_fbx_sibling_always_has() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let (binary, uv_offset, _) = textured_binary();
        fs::write(workspace.path().join("scan.bin"), &binary).expect("sibling payload");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"#,
                r#""buffers":[{{"byteLength":{length},"uri":"scan.bin"}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}},"#,
                r#"{{"buffer":0,"byteOffset":{uv_offset},"byteLength":24}}],"#,
                r#"{accessors},"#,
                r#"{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC2"}}],"#,
                r#""images":[{{"uri":"textures/Head_Base%20Color.png"}}],"#,
                r#""textures":[{{"source":0}}],"#,
                r#""materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":2}},"#,
                r#""indices":1,"material":0}}]}}],"#,
                r#""nodes":[{{"mesh":0}}],"scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            length = binary.len(),
            uv_offset = uv_offset,
            accessors = TRIANGLE_ACCESSORS
        );
        let source = workspace.path().join("scan.gltf");
        fs::write(&source, json).expect("gltf fixture");
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("the textures folder is the one thing users forget to copy");
        assert_eq!(imported.final_triangles, 1);
        assert!(!material_library(&imported).contains("map_Kd"));
    }

    #[test]
    fn a_rigged_head_imports_in_its_bind_pose_and_ignores_the_node_transform() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":44}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}}],"#,
                r#"{accessors}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""skins":[{{"joints":[1]}}],"#,
                r#""nodes":[{{"mesh":0,"skin":0,"translation":[100,0,0],"scale":[3,3,3]}},"#,
                r#"{{"name":"Hips"}}],"#,
                r#""scenes":[{{"nodes":[0,1]}}],"scene":0}}"#
            ),
            accessors = TRIANGLE_ACCESSORS
        );
        let source = write_glb(workspace.path(), "rigged.glb", &json, &triangle_binary());
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("every Ready Player Me and VRM avatar is skinned, and its bind pose is a head");
        assert_eq!(imported.final_triangles, 1);
        let mut vertices = imported.ordered.vertices.clone();
        vertices.sort_by(|left, right| left.partial_cmp(right).expect("finite vertices"));
        assert_eq!(
            vertices,
            vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
            "glTF says the world transform of a node referencing a skinned mesh MUST be ignored; \
             applying it would put this triangle at x=100 with a 3x scale and no error anywhere"
        );
    }

    #[test]
    fn an_animated_avatar_with_a_pointer_channel_still_imports() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"#,
                r#""extensionsUsed":["KHR_animation_pointer","KHR_materials_unlit"],"#,
                r#""extensionsRequired":["KHR_materials_unlit","KHR_texture_transform"],"#,
                r#""buffers":[{{"byteLength":44}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}}],"#,
                r#"{accessors}],"#,
                r#""animations":[{{"channels":[{{"sampler":0,"#,
                r#""target":{{"node":0,"path":"pointer"}}}}],"#,
                r#""samplers":[{{"input":0,"output":0,"interpolation":"LINEAR"}}]}}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1,"#,
                r#""targets":[{{"POSITION":0}}]}}]}}],"#,
                r#""nodes":[{{"mesh":0,"weights":[1.0]}}],"#,
                r#""scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            accessors = TRIANGLE_ACCESSORS
        );
        let source = write_glb(workspace.path(), "animated.glb", &json, &triangle_binary());
        let imported = prepare_mesh_import_with_progress(&source, |_| {}).expect(
            "keyframes, blend shapes and a required unlit material cannot move a base position",
        );
        assert_eq!(imported.final_triangles, 1);
    }

    #[test]
    fn a_texture_transform_lands_in_the_uvs_rather_than_refusing_the_file() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let source = write_bytes(
            workspace.path(),
            "transformed.glb",
            &textured_glb(concat!(
                r#""extensionsUsed":["KHR_texture_transform"],"#,
                r#""extensionsRequired":["KHR_texture_transform"],"#,
                r#""images":[{"bufferView":3,"mimeType":"image/png"}],"#,
                r#""textures":[{"source":0}],"#,
                r#""materials":[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0,"#,
                r#""extensions":{"KHR_texture_transform":{"offset":[0.25,0.5],"#,
                r#""rotation":1.5707963267948966,"scale":[2,4]}}}}}]"#
            )),
        );
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a UV offset and scale is the definition of an appearance-only extension");
        assert_eq!(imported.final_triangles, 1);
        let document = load_obj_document(&imported.appearance_path).expect("appearance OBJ");
        let texcoords = document.appearance.texcoords.clone();
        let expected = [[-3.75, 0.5], [0.25, 0.5], [0.25, 2.5]];
        assert_eq!(texcoords.len(), expected.len(), "{texcoords:?}");
        for wanted in expected {
            assert!(
                texcoords.iter().any(|found| {
                    (found[0] - wanted[0]).abs() < 1.0e-5 && (found[1] - wanted[1]).abs() < 1.0e-5
                }),
                "the source UVs are (0,0) (1,0) (0,1); scale 2x4, then a quarter turn \
                 counter-clockwise, then offset 0.25,0.5 puts one of them at {wanted:?}. A \
                 flipped rotation sign or a reversed composition order lands somewhere else. \
                 Found {texcoords:?}"
            );
        }
    }

    #[test]
    fn a_document_with_no_scenes_array_still_imports_its_meshes() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":44}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}}],"#,
                r#"{accessors}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0,"translation":[2,0,0]}}]}}"#
            ),
            accessors = TRIANGLE_ACCESSORS
        );
        let source = write_glb(workspace.path(), "sceneless.glb", &json, &triangle_binary());
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("the scenes array is optional in glTF 2.0, and a mesh library omits it");
        assert_eq!(imported.final_triangles, 1);
        assert_eq!(imported.ordered.vertices[0], [2.0, 0.0, 0.0]);
    }

    #[test]
    fn geometry_the_scene_never_names_is_imported_rather_than_lost() {
        let workspace = tempfile::tempdir().expect("tempdir");
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":44}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},"#,
                r#"{{"buffer":0,"byteOffset":36,"byteLength":6}}],"#,
                r#"{accessors}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0}},{{"mesh":0,"translation":[10,0,0]}}],"#,
                r#""scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            accessors = TRIANGLE_ACCESSORS
        );
        let source = write_glb(workspace.path(), "orphan.glb", &json, &triangle_binary());
        let imported = prepare_mesh_import_with_progress(&source, |_| {})
            .expect("a node no scene lists and no node parents still holds a surface");
        assert_eq!(
            imported.final_triangles, 2,
            "the scene names one of the two mesh nodes; the parentless one is picked up after"
        );
    }

    #[test]
    fn an_fbx_whose_texture_has_gone_missing_still_imports_its_mesh() {
        let fixture = include_str!("importers/fixtures/static_instances_ascii.fbx");
        let source_root = tempfile::tempdir().expect("FBX fixture root");
        let source = source_root.path().join("static-instances.fbx");
        fs::write(&source, fixture).expect("FBX fixture");
        let imported = prepare_mesh_import_with_progress(&source, |_| {}).expect(
            "the sibling generated-albedo.png is deliberately not written here, and an FBX \
             separated from its textures folder is the commonest thing a user drags in",
        );
        assert_eq!(imported.final_triangles, 6);
        let mtl = material_library(&imported);
        assert!(mtl.contains("newmtl"), "{mtl}");
        assert!(
            !mtl.contains("map_Kd"),
            "the material keeps its colour factors and loses only the map: {mtl}"
        );
    }

    fn write_bytes(workspace: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let source = workspace.join(name);
        fs::write(&source, bytes).expect("GLB fixture");
        source
    }

    /// Encodes a `EXT_meshopt_compression` ATTRIBUTES stream the long way: every byte group is
    /// written as sixteen literal zigzag deltas, and the tail is left zeroed so it also serves
    /// as the predictor seed. It is the simplest stream the decoder accepts, which is what makes
    /// it useful for proving the wiring rather than the codec.
    fn encode_meshopt_vertices(vertices: &[[f32; 3]]) -> Vec<u8> {
        const STRIDE: usize = 12;
        let raw = vertices
            .iter()
            .map(|vertex| {
                let mut bytes = [0_u8; STRIDE];
                for (axis, value) in vertex.iter().enumerate() {
                    bytes[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
                }
                bytes
            })
            .collect::<Vec<_>>();
        let mut encoded = vec![0xa0_u8];
        for plane in 0..STRIDE {
            encoded.push(0xff);
            let mut previous = 0_u8;
            let mut group = [0_u8; 16];
            for (slot, vertex) in raw.iter().enumerate() {
                let value = vertex[plane];
                let delta = value.wrapping_sub(previous);
                group[slot] = (delta << 1) ^ ((delta as i8) >> 7) as u8;
                previous = value;
            }
            encoded.extend_from_slice(&group);
        }
        encoded.extend(std::iter::repeat_n(0_u8, 32));
        encoded
    }

    fn wrap_glb(json: &str, binary: &[u8]) -> Vec<u8> {
        let mut json = json.as_bytes().to_vec();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut binary = binary.to_vec();
        while !binary.len().is_multiple_of(4) {
            binary.push(0);
        }
        let total_length = 12
            + 8
            + json.len()
            + if binary.is_empty() {
                0
            } else {
                8 + binary.len()
            };
        let mut glb = Vec::with_capacity(total_length);
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2_u32.to_le_bytes());
        glb.extend_from_slice(&(total_length as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4e4f_534a_u32.to_le_bytes());
        glb.extend_from_slice(&json);
        if !binary.is_empty() {
            glb.extend_from_slice(&(binary.len() as u32).to_le_bytes());
            glb.extend_from_slice(&0x004e_4942_u32.to_le_bytes());
            glb.extend_from_slice(&binary);
        }
        glb
    }

    fn minimal_triangle_glb() -> Vec<u8> {
        build_test_glb(
            &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            &[0, 1, 2],
            [2.0, 0.0, 0.0],
        )
    }

    fn build_test_glb(positions: &[[f32; 3]], indices: &[u16], translation: [f32; 3]) -> Vec<u8> {
        let mut binary = Vec::new();
        for position in positions {
            for value in position {
                binary.extend_from_slice(&value.to_le_bytes());
            }
        }
        let position_bytes = binary.len();
        for index in indices {
            binary.extend_from_slice(&index.to_le_bytes());
        }
        let index_bytes = binary.len() - position_bytes;
        while !binary.len().is_multiple_of(4) {
            binary.push(0);
        }
        let mut minimum = [f32::INFINITY; 3];
        let mut maximum = [f32::NEG_INFINITY; 3];
        for position in positions {
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min(position[axis]);
                maximum[axis] = maximum[axis].max(position[axis]);
            }
        }
        let json = format!(
            concat!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{buffer_length}}}],"#,
                r#""bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":{position_bytes}}},"#,
                r#"{{"buffer":0,"byteOffset":{position_bytes},"byteLength":{index_bytes}}}],"#,
                r#""accessors":[{{"bufferView":0,"componentType":5126,"count":{vertex_count},"#,
                r#""type":"VEC3","min":[{min_x},{min_y},{min_z}],"max":[{max_x},{max_y},{max_z}]}},"#,
                r#"{{"bufferView":1,"componentType":5123,"count":{index_count},"type":"SCALAR"}}],"#,
                r#""meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":1}}]}}],"#,
                r#""nodes":[{{"mesh":0,"translation":[{tx},{ty},{tz}]}}],"#,
                r#""scenes":[{{"nodes":[0]}}],"scene":0}}"#
            ),
            buffer_length = position_bytes + index_bytes,
            position_bytes = position_bytes,
            index_bytes = index_bytes,
            vertex_count = positions.len(),
            index_count = indices.len(),
            min_x = minimum[0],
            min_y = minimum[1],
            min_z = minimum[2],
            max_x = maximum[0],
            max_y = maximum[1],
            max_z = maximum[2],
            tx = translation[0],
            ty = translation[1],
            tz = translation[2],
        );
        wrap_glb(&json, &binary)
    }
}
