//! Measures the Draco decoder against an uncompressed answer key.
//!
//! The corpus is a set of glTF files exported from one source mesh by a real Blender install:
//! `01_plain.glb` carries the geometry uncompressed, `04_draco.glb` carries the same geometry
//! Draco-compressed at the exporter's defaults, and `06_draco_max.glb` at its maximum
//! compression level. That makes the plain file a literal answer key, and it is the only way
//! to tell a decoder that is right from one that is nearly right — a face count matching
//! proves the connectivity survived and says nothing at all about where the vertices went.
//!
//! Point `VKIT_GLTF_CORPUS` at the directory to run it; the files are large enough that they
//! live outside the repository, so the test skips silently when the variable is unset.

use std::path::{Path, PathBuf};

use serde_json::Value;
use vkit_core::formats::{DracoError, decode_draco_mesh};

fn corpus_dir() -> Option<PathBuf> {
    std::env::var_os("VKIT_GLTF_CORPUS").map(PathBuf::from)
}

struct Glb {
    json: Value,
    binary: Vec<u8>,
}

fn read_glb(path: &Path) -> Glb {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert_eq!(&bytes[..4], b"glTF", "{} is not a GLB", path.display());

    let mut json = None;
    let mut binary = Vec::new();
    let mut cursor = 12usize;
    while cursor + 8 <= bytes.len() {
        let length = u32::from_le_bytes([
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
        ]) as usize;
        let kind = &bytes[cursor + 4..cursor + 8];
        let start = cursor + 8;
        let end = start + length;
        if kind == b"JSON" {
            json = Some(serde_json::from_slice(&bytes[start..end]).expect("GLB JSON chunk"));
        } else if kind == b"BIN\0" {
            binary = bytes[start..end].to_vec();
        }
        cursor = end + (4 - end % 4) % 4;
    }

    Glb {
        json: json.expect("GLB has a JSON chunk"),
        binary,
    }
}

fn view_bytes(glb: &Glb, view_index: usize) -> &[u8] {
    let view = &glb.json["bufferViews"][view_index];
    let offset = view["byteOffset"].as_u64().unwrap_or(0) as usize;
    let length = view["byteLength"].as_u64().expect("byteLength") as usize;
    &glb.binary[offset..offset + length]
}

fn plain_positions(glb: &Glb) -> Vec<[f32; 3]> {
    let accessor_index = glb.json["meshes"][0]["primitives"][0]["attributes"]["POSITION"]
        .as_u64()
        .expect("POSITION accessor") as usize;
    let accessor = &glb.json["accessors"][accessor_index];
    let count = accessor["count"].as_u64().expect("count") as usize;
    let view = accessor["bufferView"].as_u64().expect("bufferView") as usize;
    let bytes = view_bytes(glb, view);

    (0..count)
        .map(|element| {
            let base = element * 12;
            let read = |lane: usize| {
                let at = base + lane * 4;
                f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
            };
            [read(0), read(1), read(2)]
        })
        .collect()
}

fn draco_payload(glb: &Glb) -> &[u8] {
    let view = glb.json["meshes"][0]["primitives"][0]["extensions"]["KHR_draco_mesh_compression"]
        ["bufferView"]
        .as_u64()
        .expect("Draco bufferView") as usize;
    view_bytes(glb, view)
}

fn bounding_box(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for position in positions {
        for lane in 0..3 {
            min[lane] = min[lane].min(position[lane]);
            max[lane] = max[lane].max(position[lane]);
        }
    }
    (min, max)
}

/// Draco quantizes positions on a uniform grid across the whole bounding box, so one grid step
/// is the largest error a *correct* decode can show on any axis. The reference numbers below
/// are stated against that step rather than against an absolute epsilon, because the step is
/// the only figure that scales with the model.
fn quantization_step(positions: &[[f32; 3]], bits: u32) -> f32 {
    let (min, max) = bounding_box(positions);
    let extent = (0..3).fold(0.0f32, |widest, lane| widest.max(max[lane] - min[lane]));
    extent / ((1u32 << bits) - 1) as f32
}

/// The tolerance a correct decode must meet: one quantization step, but never finer than the
/// float format itself can hold.
///
/// Past roughly 21 bits over this model the grid step drops below one `f32` ULP, so the grid
/// stops being the limiting error and rounding in the dequantization multiply takes over — the
/// answer key is `f32` too, and neither side can represent the difference. Holding the assert
/// at the grid step there would fail a decode that is as exact as the format permits.
fn position_tolerance(positions: &[[f32; 3]], bits: u32) -> f32 {
    let (min, max) = bounding_box(positions);
    let magnitude = (0..3).fold(0.0f32, |widest, lane| {
        widest.max(min[lane].abs()).max(max[lane].abs())
    });
    quantization_step(positions, bits).max(8.0 * f32::EPSILON * magnitude)
}

/// Largest per-axis deviation, matching each decoded point to its nearest answer-key point.
///
/// The match has to be by proximity: Draco reorders vertices as a consequence of its traversal,
/// so decoded index `i` is not source index `i` and comparing them positionally would report a
/// correct decode as catastrophically wrong.
fn worst_axis_deviation(decoded: &[[f32; 3]], answer: &[[f32; 3]]) -> (f32, usize) {
    let mut worst = 0.0f32;
    let mut worst_point = 0usize;
    for (point, value) in decoded.iter().enumerate() {
        let mut best = f32::INFINITY;
        for reference in answer {
            let axis = (0..3).fold(0.0f32, |widest, lane| {
                widest.max((value[lane] - reference[lane]).abs())
            });
            best = best.min(axis);
        }
        if best > worst {
            worst = best;
            worst_point = point;
        }
    }
    (worst, worst_point)
}

fn decoded_positions(payload: &[u8]) -> (Vec<[f32; 3]>, usize) {
    let mesh = decode_draco_mesh(payload).expect("Draco payload must decode");
    let positions = mesh.positions().expect("decoded mesh has positions");
    assert_eq!(positions.components, 3, "POSITION must be VEC3");

    let values: Vec<[f32; 3]> = (0..mesh.num_points)
        .map(|point| {
            let value = positions.value(point).expect("every point has a position");
            [value[0], value[1], value[2]]
        })
        .collect();
    (values, mesh.indices.len() / 3)
}

#[test]
fn draco_defaults_reproduce_the_uncompressed_answer_key() {
    let Some(dir) = corpus_dir() else {
        return;
    };

    let plain = read_glb(&dir.join("01_plain.glb"));
    let answer = plain_positions(&plain);

    let compressed = read_glb(&dir.join("04_draco.glb"));
    let (decoded, faces) = decoded_positions(draco_payload(&compressed));

    assert_eq!(decoded.len(), answer.len(), "vertex count");
    assert_eq!(faces, 2976, "face count");

    let step = quantization_step(&answer, 14);
    let (worst, point) = worst_axis_deviation(&decoded, &answer);
    println!(
        "04_draco.glb: {} vertices, {faces} faces, 14-bit step {step:.3e}, \
         worst per-axis deviation {worst:.3e} at point {point} ({:.2} steps)",
        decoded.len(),
        worst / step
    );

    assert!(
        worst <= step,
        "worst per-axis deviation {worst:.3e} exceeds one 14-bit quantization step {step:.3e}"
    );
}

fn plain_texcoords(glb: &Glb) -> Vec<[f32; 2]> {
    let accessor_index = glb.json["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"]
        .as_u64()
        .expect("TEXCOORD_0 accessor") as usize;
    let accessor = &glb.json["accessors"][accessor_index];
    let count = accessor["count"].as_u64().expect("count") as usize;
    let view = accessor["bufferView"].as_u64().expect("bufferView") as usize;
    let bytes = view_bytes(glb, view);

    (0..count)
        .map(|element| {
            let base = element * 8;
            let read = |lane: usize| {
                let at = base + lane * 4;
                f32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
            };
            [read(0), read(1)]
        })
        .collect()
}

/// A UV seam puts two source vertices at the same position with different texture coordinates,
/// so a texcoord check cannot pair vertices by position first and then compare UVs — on a
/// sphere it would compare against the wrong twin half the time. Pairing on both at once, and
/// requiring some single source vertex to match both, is the check that means something.
#[test]
fn draco_defaults_preserve_texture_coordinates_on_the_same_vertex() {
    let Some(dir) = corpus_dir() else {
        return;
    };

    let plain = read_glb(&dir.join("01_plain.glb"));
    let answer_positions = plain_positions(&plain);
    let answer_texcoords = plain_texcoords(&plain);

    let compressed = read_glb(&dir.join("04_draco.glb"));
    let mesh = decode_draco_mesh(draco_payload(&compressed)).expect("04_draco.glb must decode");
    let positions = mesh.positions().expect("positions");
    let texcoords = mesh.texcoords().expect("texcoords");
    assert_eq!(texcoords.components, 2, "TEXCOORD_0 must be VEC2");
    assert!(mesh.normals().is_some(), "NORMAL must survive the decode");

    let position_tolerance = position_tolerance(&answer_positions, 14);
    let texcoord_tolerance = 1.0f32 / ((1u32 << 12) - 1) as f32;

    let mut worst_pair = 0.0f32;
    let mut worst_texcoord = 0.0f32;
    for point in 0..mesh.num_points {
        let position = positions.value(point).expect("position");
        let texcoord = texcoords.value(point).expect("texcoord");

        let mut best = f32::INFINITY;
        let mut best_texcoord = f32::INFINITY;
        for (candidate, reference) in answer_positions.iter().enumerate() {
            let spatial = (0..3).fold(0.0f32, |widest, lane| {
                widest.max((position[lane] - reference[lane]).abs())
            });
            let uv = (0..2).fold(0.0f32, |widest, lane| {
                widest.max((texcoord[lane] - answer_texcoords[candidate][lane]).abs())
            });
            let combined = (spatial / position_tolerance).max(uv / texcoord_tolerance);
            if combined < best {
                best = combined;
                best_texcoord = uv;
            }
        }
        worst_pair = worst_pair.max(best);
        worst_texcoord = worst_texcoord.max(best_texcoord);
    }

    println!(
        "04_draco.glb texcoords: 12-bit tolerance {texcoord_tolerance:.3e}, worst UV deviation \
         on the matched vertex {worst_texcoord:.3e}, worst combined score {worst_pair:.3}"
    );
    assert!(
        worst_pair <= 1.0,
        "no source vertex matches both the decoded position and its texture coordinate \
         (worst combined score {worst_pair:.3})"
    );
}

#[test]
fn draco_separate_gltf_carries_the_same_geometry_as_the_glb() {
    let Some(dir) = corpus_dir() else {
        return;
    };

    let plain = read_glb(&dir.join("01_plain.glb"));
    let answer = plain_positions(&plain);

    let json: Value = serde_json::from_slice(
        &std::fs::read(dir.join("05_draco_separate.gltf")).expect("05_draco_separate.gltf"),
    )
    .expect("glTF JSON");
    let binary = std::fs::read(dir.join("05_draco_separate.bin")).expect("05_draco_separate.bin");
    let glb = Glb { json, binary };

    let (decoded, faces) = decoded_positions(draco_payload(&glb));
    assert_eq!(decoded.len(), answer.len(), "vertex count");
    assert_eq!(faces, 2976, "face count");

    let step = quantization_step(&answer, 14);
    let (worst, _) = worst_axis_deviation(&decoded, &answer);
    println!(
        "05_draco_separate.gltf: worst per-axis deviation {worst:.3e} ({:.2} steps)",
        worst / step
    );
    assert!(worst <= step);
}

#[test]
fn maximum_compression_either_reproduces_the_answer_key_or_refuses_by_name() {
    let Some(dir) = corpus_dir() else {
        return;
    };

    let plain = read_glb(&dir.join("01_plain.glb"));
    let answer = plain_positions(&plain);
    let step = quantization_step(&answer, 14);

    let compressed = read_glb(&dir.join("06_draco_max.glb"));
    match decode_draco_mesh(draco_payload(&compressed)) {
        Ok(mesh) => {
            let positions = mesh.positions().expect("decoded mesh has positions");
            let decoded: Vec<[f32; 3]> = (0..mesh.num_points)
                .map(|point| {
                    let value = positions.value(point).expect("position");
                    [value[0], value[1], value[2]]
                })
                .collect();
            let (worst, _) = worst_axis_deviation(&decoded, &answer);
            println!(
                "06_draco_max.glb: decoded {} vertices, {} faces, worst per-axis deviation \
                 {worst:.3e} ({:.2} steps)",
                decoded.len(),
                mesh.indices.len() / 3,
                worst / step
            );
            assert_eq!(decoded.len(), answer.len(), "vertex count");
            assert_eq!(mesh.indices.len() / 3, 2976, "face count");
            assert!(
                worst <= step,
                "maximum compression decoded to geometry that is off by {worst:.3e}, more than \
                 one quantization step — a refusal would have been correct, corruption is not"
            );
        }
        Err(error) => {
            println!("06_draco_max.glb: refused with {error}");
            assert!(
                matches!(
                    error,
                    DracoError::Decode(_) | DracoError::BitstreamVersionUnsupported { .. }
                ),
                "a refusal must be a named decode failure, got {error:?}"
            );
        }
    }
}

#[test]
fn the_draco_matrix_never_corrupts_geometry_it_accepts() {
    let Some(dir) = corpus_dir() else {
        return;
    };
    let matrix = dir
        .parent()
        .map(|parent| parent.join("draco_matrix"))
        .filter(|path| path.is_dir());
    let Some(matrix) = matrix else {
        return;
    };

    let plain = read_glb(&matrix.join("01_plain.glb"));
    let answer = plain_positions(&plain);

    let mut accepted = 0usize;
    let mut refused = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&matrix)
        .expect("draco matrix directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "glb")
                && path.file_name().is_some_and(|name| name != "01_plain.glb")
        })
        .collect();
    entries.sort();

    for path in entries {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let glb = read_glb(&path);
        match decode_draco_mesh(draco_payload(&glb)) {
            Ok(mesh) => {
                accepted += 1;
                assert_eq!(mesh.indices.len() / 3, 2976, "{name} face count");
                assert_eq!(mesh.num_points, answer.len(), "{name} vertex count");

                let positions = mesh.positions().expect("positions");
                let decoded: Vec<[f32; 3]> = (0..mesh.num_points)
                    .map(|point| {
                        let value = positions.value(point).expect("position");
                        [value[0], value[1], value[2]]
                    })
                    .collect();

                let bits = name
                    .strip_prefix("d_pos")
                    .and_then(|rest| rest.strip_suffix(".glb"))
                    .and_then(|digits| digits.parse::<u32>().ok())
                    .unwrap_or(14);
                let step = quantization_step(&answer, bits);
                let tolerance = position_tolerance(&answer, bits);
                let (worst, _) = worst_axis_deviation(&decoded, &answer);
                println!(
                    "{name}: accepted, {bits}-bit step {step:.3e}, tolerance {tolerance:.3e}, \
                     worst {worst:.3e} ({:.2} tolerances)",
                    worst / tolerance
                );
                assert!(
                    worst <= tolerance,
                    "{name} decoded geometry is off by {worst:.3e}, over the {tolerance:.3e} a \
                     {bits}-bit decode may show"
                );
            }
            Err(error) => {
                println!("{name}: refused with {error}");
                refused.push(name);
            }
        }
    }

    println!(
        "draco matrix: {accepted} accepted, {} refused: {refused:?}",
        refused.len()
    );
    assert!(accepted > 0, "the matrix must not refuse everything");
}
