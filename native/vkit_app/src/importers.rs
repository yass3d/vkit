mod fbx;
mod glb;
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
    #[error("unsupported mesh input extension; use OBJ, GLB, or FBX")]
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
        .is_some_and(|value| matches!(value, "obj" | "glb" | "fbx"))
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
        Some("glb") => Err(ImportError::NativeMesh(
            "GLB template ingestion is rejected because triangle primitives cannot prove the canonical ordered quad stream".to_owned(),
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
        Some("glb") => {
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
        assert!(is_supported_mesh_path(Path::new("head.FbX")));
        assert!(!is_supported_mesh_path(Path::new("head.dsf")));
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
        while binary.len() % 4 != 0 {
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
        let mut json = format!(
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
        )
        .into_bytes();
        while json.len() % 4 != 0 {
            json.push(b' ');
        }
        let total_length = 12 + 8 + json.len() + 8 + binary.len();
        let mut glb = Vec::with_capacity(total_length);
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2_u32.to_le_bytes());
        glb.extend_from_slice(&(total_length as u32).to_le_bytes());
        glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x4e4f_534a_u32.to_le_bytes());
        glb.extend_from_slice(&json);
        glb.extend_from_slice(&(binary.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004e_4942_u32.to_le_bytes());
        glb.extend_from_slice(&binary);
        glb
    }
}
