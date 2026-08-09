use std::{collections::BTreeMap, path::PathBuf};

use crate::formats::{DazGeometry, ObjDocument, load_obj_document};
use crate::{G2F_POLYGON_COUNT, G2F_VERTEX_COUNT};

use super::{Result, VaMError};
use super::{catalog::VaMRoot, skin::SkinSex};

const HEAD_MORPH_MATERIALS: &[&str] = &[
    "face",
    "head",
    "neck",
    "ears",
    "lips",
    "nostrils",
    "sclera",
    "irises",
    "pupils",
    "cornea",
    "eyereflection",
    "innermouth",
    "teeth",
    "gums",
    "tongue",
    "lacrimals",
    "tear",
    "eyelashes",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UvMaterialRegion {
    Face,
    Torso,

    Limbs,

    Genitals,
    Sclera,
    Iris,
    Pupil,
    Cornea,
    EyeReflection,
    Lacrimal,
    Tear,
    InnerMouth,
    Teeth,
    Gums,
    Tongue,
    Eyelashes,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UvCorner {
    pub position_index: u32,
    pub uv: [f32; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub struct G2UvFace {
    pub canonical_face_index: u32,
    pub material_region: UvMaterialRegion,
    pub corners: Vec<UvCorner>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct G2UvTriangle {
    pub canonical_face_index: u32,
    pub canonical_triangle_index: u32,
    pub material_region: UvMaterialRegion,

    pub on_head: bool,
    pub position_indices: [u32; 3],
    pub uvs: [[f32; 2]; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct G2UvMapping {
    pub source_path: PathBuf,
    pub coordinate_rms_cm: f64,
    pub coordinate_max_cm: f64,
    pub faces: Vec<G2UvFace>,
    pub triangles: Vec<G2UvTriangle>,

    pub uncovered_triangles: usize,
}

pub fn canonical_head_vertex_mask(geometry: &DazGeometry) -> crate::formats::Result<Vec<bool>> {
    geometry.validate()?;
    let face_mask = canonical_head_face_mask(geometry)?;
    let mut mask = vec![false; geometry.vertices.len()];
    for (face, is_head) in geometry.faces.iter().zip(face_mask) {
        if !is_head {
            continue;
        }
        for &vertex in face {
            mask[vertex as usize] = true;
        }
    }
    Ok(mask)
}

pub fn canonical_head_face_mask(geometry: &DazGeometry) -> crate::formats::Result<Vec<bool>> {
    geometry.validate()?;
    Ok(geometry
        .material_group_indices
        .iter()
        .map(|index| {
            geometry
                .material_groups
                .get(*index as usize)
                .map(|name| name.to_ascii_lowercase())
                .as_deref()
                .is_some_and(|name| HEAD_MORPH_MATERIALS.contains(&name))
        })
        .collect())
}

pub fn load_g2_uv_mapping(root: &VaMRoot, geometry: &DazGeometry) -> Result<G2UvMapping> {
    load_g2_uv_mapping_for_sex(root, geometry, SkinSex::Female)
}

pub fn load_g2_uv_mapping_for_sex(
    root: &VaMRoot,
    geometry: &DazGeometry,
    sex: SkinSex,
) -> Result<G2UvMapping> {
    // The figure's own bundle carries this mapping and ships with every
    // installation. The loose OBJ below does not: it is written out later by
    // features not everyone uses, so requiring it turned a stock install into
    // "UV unavailable". It stays as a fallback for anyone whose bundle this
    // decoder cannot read.
    match mapping_from_bundle(root, geometry, sex) {
        Ok(mapping) => return Ok(mapping),
        Err(error) => {
            let _ = error;
        }
    }
    let (source_path, validate_coordinates) = match sex {
        SkinSex::Female => (find_female_custom_obj(root)?, true),
        SkinSex::Male => (find_male_custom_obj(root)?, false),
        SkinSex::Unknown => {
            return Err(VaMError::InvalidUv(
                "figure sex must be Female or Male".to_owned(),
            ));
        }
    };
    let document = load_obj_document(&source_path)?;
    build_mapping(source_path, &document, geometry, true, validate_coordinates)
}

/// Builds the mapping straight from the figure bundle.
///
/// No correspondence to guess and no coordinates to validate: the bundle's
/// polygon list is the canonical one, and its UV polygon list runs beside it
/// corner for corner, so face i's texture coordinates are simply face i's.
fn mapping_from_bundle(
    root: &VaMRoot,
    geometry: &DazGeometry,
    sex: SkinSex,
) -> Result<G2UvMapping> {
    let bundle_sex = match sex {
        SkinSex::Female => super::geometry::GeometrySex::Female,
        SkinSex::Male => super::geometry::GeometrySex::Male,
        SkinSex::Unknown => {
            return Err(VaMError::InvalidUv(
                "figure sex must be Female or Male".to_owned(),
            ));
        }
    };
    geometry.validate()?;
    let bundle_path = root.neutral_base_bundle_path(bundle_sex);
    let bundle = std::fs::read(&bundle_path)
        .map_err(|error| VaMError::InvalidUv(format!("{}: {error}", bundle_path.display())))?;
    let figure = super::unity_base::extract_figure_uv(&bundle)?;
    // The material list is the cheapest proof that this bundle holds the same
    // figure the canonical geometry was taken from — a mismatch here means the
    // face-for-face assumption below is not safe to make.
    for (face_index, material_index) in figure.material_indices.iter().enumerate() {
        let Some(bundle_material) = figure.material_names.get(*material_index as usize) else {
            return Err(VaMError::InvalidUv(format!(
                "{} face {face_index} names material {material_index}, which it does not list",
                bundle_path.display()
            )));
        };
        let canonical = canonical_material_name(geometry, face_index)?;
        if normalized_material_name(bundle_material) != canonical {
            return Err(VaMError::InvalidUv(format!(
                "{} calls face {face_index} {bundle_material:?} where this figure calls it                  {canonical:?}",
                bundle_path.display()
            )));
        }
    }
    if figure.base_polygons.len() != geometry.faces.len()
        || figure.uv_polygons.len() != geometry.faces.len()
    {
        return Err(VaMError::InvalidUv(format!(
            "{} holds {} polygons but this figure has {}",
            bundle_path.display(),
            figure.base_polygons.len(),
            geometry.faces.len()
        )));
    }

    let mut faces = Vec::new();
    let mut triangles = Vec::new();
    let mut uncovered = 0_usize;
    let mut triangle_cursor = 0_u32;
    for (face_index, positions) in geometry.faces.iter().enumerate() {
        let face_triangle_start = triangle_cursor;
        triangle_cursor = triangle_cursor.saturating_add(positions.len().saturating_sub(2) as u32);
        let Some(material_region) = material_region(geometry, face_index) else {
            continue;
        };
        let bundle_positions = &figure.base_polygons[face_index];
        let bundle_uv_corners = &figure.uv_polygons[face_index];
        if bundle_positions.len() != positions.len() || bundle_uv_corners.len() != positions.len() {
            uncovered += positions.len().saturating_sub(2);
            continue;
        }
        // The bundle lists the same corners in the same order; anything else
        // means this is not the figure the canonical geometry came from.
        if bundle_positions != positions {
            return Err(VaMError::InvalidUv(format!(
                "{} disagrees with the canonical geometry at face {face_index}",
                bundle_path.display()
            )));
        }
        let corner_uv = |corner: usize| -> Result<[f32; 2]> {
            let uv_index = bundle_uv_corners[corner] as usize;
            figure.uvs.get(uv_index).copied().ok_or_else(|| {
                VaMError::InvalidUv(format!(
                    "{} face {face_index} corner {corner} references missing UV {uv_index}",
                    bundle_path.display()
                ))
            })
        };
        let mut corners = Vec::with_capacity(positions.len());
        for (corner, &position_index) in positions.iter().enumerate() {
            corners.push(UvCorner {
                position_index,
                uv: corner_uv(corner)?,
            });
        }
        let on_head = face_is_on_head(geometry, face_index);
        for corner in 1..positions.len() - 1 {
            triangles.push(G2UvTriangle {
                canonical_face_index: face_index as u32,
                canonical_triangle_index: face_triangle_start + (corner - 1) as u32,
                material_region,
                on_head,
                position_indices: [positions[0], positions[corner], positions[corner + 1]],
                uvs: [corner_uv(0)?, corner_uv(corner)?, corner_uv(corner + 1)?],
            });
        }
        faces.push(G2UvFace {
            canonical_face_index: face_index as u32,
            material_region,
            corners,
        });
    }

    Ok(G2UvMapping {
        source_path: bundle_path,
        // The bundle is the geometry's own source, so there is no second set of
        // coordinates to have drifted from.
        coordinate_rms_cm: 0.0,
        coordinate_max_cm: 0.0,
        faces,
        triangles,
        uncovered_triangles: uncovered,
    })
}

fn find_female_custom_obj(root: &VaMRoot) -> Result<PathBuf> {
    let candidates = [
        root.path().join("femalecustom.obj"),
        root.path().join("female_custom.obj"),
        root.person_assets_path()
            .join("Geometry")
            .join("femalecustom.obj"),
    ];
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            VaMError::InvalidUv(format!(
                "femalecustom.obj was not found below {}",
                root.path().display()
            ))
        })
}

fn find_male_custom_obj(root: &VaMRoot) -> Result<PathBuf> {
    super::catalog::geometry_base_candidate_names(super::geometry::GeometrySex::Male)
        .iter()
        .map(|name| root.path().join(name))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            VaMError::InvalidUv(format!(
                "a VaM male custom reference OBJ was not found below {}",
                root.path().display()
            ))
        })
}

struct PartialFace;

const MINIMUM_TRIANGLE_COVERAGE: f64 = 0.5;

const DIFFERENT_VERTEX_CM: f64 = 1.0;

const MEDIAN_LIMIT_CM: f64 = 0.25;

const SHARE_APART_LIMIT: f64 = 0.05;

fn canonical_order_refusal(sorted_errors: &[f64]) -> Option<String> {
    if sorted_errors.is_empty() {
        return None;
    }
    let median = sorted_errors[sorted_errors.len() / 2];
    if !median.is_finite() || median > MEDIAN_LIMIT_CM {
        return Some(format!("median disagreement {median:.4} cm"));
    }
    #[expect(clippy::cast_precision_loss, reason = "a vertex count")]
    let share = sorted_errors
        .iter()
        .filter(|error| !error.is_finite() || **error > DIFFERENT_VERTEX_CM)
        .count() as f64
        / sorted_errors.len() as f64;
    (share > SHARE_APART_LIMIT).then(|| {
        format!(
            "{:.1}% of vertices more than {DIFFERENT_VERTEX_CM} cm apart",
            share * 100.0
        )
    })
}

fn build_mapping(
    source_path: PathBuf,
    document: &ObjDocument,
    geometry: &DazGeometry,
    require_canonical_counts: bool,
    validate_coordinates: bool,
) -> Result<G2UvMapping> {
    document.validate()?;
    geometry.validate()?;
    if require_canonical_counts
        && (geometry.vertices.len() != G2F_VERTEX_COUNT
            || geometry.faces.len() != G2F_POLYGON_COUNT)
    {
        return Err(VaMError::InvalidUv(format!(
            "canonical G2F requires {G2F_VERTEX_COUNT} vertices and {G2F_POLYGON_COUNT} polygons; got {} and {}",
            geometry.vertices.len(),
            geometry.faces.len()
        )));
    }
    let vertex_count = geometry.vertices.len();
    if document.geometry.vertices.len() < vertex_count
        || document.appearance.texcoords.len() < vertex_count
    {
        return Err(VaMError::InvalidUv(format!(
            "{} has {} positions and {} UVs; canonical geometry requires {vertex_count}",
            source_path.display(),
            document.geometry.vertices.len(),
            document.appearance.texcoords.len()
        )));
    }

    let mut squared_error = 0.0;
    let mut maximum_error = 0.0_f64;
    let mut errors = Vec::with_capacity(vertex_count);
    for (canonical, vam) in geometry
        .vertices
        .iter()
        .zip(document.geometry.vertices.iter())
        .take(vertex_count)
    {
        let converted = [vam[0] * 100.0, vam[1] * 100.0, vam[2] * 100.0];
        let error_squared = (0..3)
            .map(|axis| {
                let difference = canonical[axis] - converted[axis];
                difference * difference
            })
            .sum::<f64>();
        squared_error += error_squared;
        let error = error_squared.sqrt();
        maximum_error = maximum_error.max(error);
        errors.push(error);
    }
    let coordinate_rms_cm = (squared_error / vertex_count as f64).sqrt();
    errors.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let refusal = if require_canonical_counts {
        canonical_order_refusal(&errors)
    } else {
        (!coordinate_rms_cm.is_finite()
            || !maximum_error.is_finite()
            || coordinate_rms_cm > 1.0e-6
            || maximum_error > 1.0e-5)
            .then(|| format!("RMS {coordinate_rms_cm:.6} cm, max {maximum_error:.6} cm"))
    };
    if validate_coordinates && let Some(reason) = refusal {
        return Err(VaMError::InvalidUv(format!(
            "{} does not list canonical G2F vertices in canonical order: {reason}              (RMS {coordinate_rms_cm:.4} cm, max {maximum_error:.4} cm)",
            source_path.display()
        )));
    }

    let source_to_canonical = source_position_mapping(document, vertex_count);
    let mut triangle_specs = Vec::new();
    let mut triangle_lookup = BTreeMap::<(String, [u32; 3]), usize>::new();
    let mut face_corner_uvs = vec![None; geometry.faces.len()];
    let mut canonical_triangle_cursor = 0_u32;
    for (face_index, positions) in geometry.faces.iter().enumerate() {
        let face_triangle_start = canonical_triangle_cursor;
        canonical_triangle_cursor =
            canonical_triangle_cursor.saturating_add(positions.len().saturating_sub(2) as u32);
        let Some(material_region) = material_region(geometry, face_index) else {
            continue;
        };
        let material = canonical_material_name(geometry, face_index)?;
        face_corner_uvs[face_index] = Some(vec![None; positions.len()]);
        for corner in 1..positions.len() - 1 {
            let position_indices = [positions[0], positions[corner], positions[corner + 1]];
            let mut sorted = position_indices;
            sorted.sort_unstable();
            let spec_index = triangle_specs.len();
            let previous = triangle_lookup.insert((material.clone(), sorted), spec_index);
            if previous.is_some() {
                return Err(VaMError::InvalidUv(format!(
                    "canonical material {material:?} has an ambiguous triangle {sorted:?}"
                )));
            }
            triangle_specs.push((
                face_index,
                face_triangle_start + (corner - 1) as u32,
                material_region,
                position_indices,
                [0, corner, corner + 1],
            ));
        }
    }
    let mut recovered_triangle_uvs = vec![None; triangle_specs.len()];
    for (source_face_index, (source_face, source_uv_indices)) in document
        .geometry
        .faces
        .iter()
        .zip(&document.appearance.face_texcoord_indices)
        .enumerate()
    {
        let Some(source_material) = source_face.material.as_deref() else {
            continue;
        };
        let material = normalized_material_name(source_material);
        if material_region_from_name(&material).is_none() {
            continue;
        }
        for corner in 1..source_face.vertex_indices.len() - 1 {
            let source_positions = [
                source_face.vertex_indices[0],
                source_face.vertex_indices[corner],
                source_face.vertex_indices[corner + 1],
            ];
            let source_uvs = [
                source_uv_indices[0],
                source_uv_indices[corner],
                source_uv_indices[corner + 1],
            ];

            let mut canonical_positions = [0_u32; 3];
            let mut off_the_figure = false;
            for (slot, source_position) in source_positions.into_iter().enumerate() {
                match source_to_canonical
                    .get(source_position as usize)
                    .copied()
                    .flatten()
                {
                    Some(canonical) => canonical_positions[slot] = canonical,
                    None => {
                        off_the_figure = true;
                        break;
                    }
                }
            }
            if off_the_figure {
                continue;
            }
            let mut sorted = canonical_positions;
            sorted.sort_unstable();

            let Some(&spec_index) = triangle_lookup.get(&(material.clone(), sorted)) else {
                continue;
            };
            if recovered_triangle_uvs[spec_index].is_some() {
                return Err(VaMError::InvalidUv(format!(
                    "{} maps more than one source triangle to canonical triangle {spec_index}",
                    source_path.display()
                )));
            }
            let (canonical_face_index, _, _, oriented_positions, face_corners) =
                &triangle_specs[spec_index];
            let mut oriented_uvs = [[0.0_f32; 2]; 3];
            for oriented_corner in 0..3 {
                let source_corner = canonical_positions
                    .iter()
                    .position(|position| *position == oriented_positions[oriented_corner])
                    .ok_or_else(|| {
                        VaMError::InvalidUv(format!(
                            "source triangle {source_face_index} cannot be oriented to canonical triangle {spec_index}"
                        ))
                    })?;
                let uv_index = source_uvs[source_corner].ok_or_else(|| {
                    VaMError::InvalidUv(format!(
                        "{} omits a UV at head triangle {source_face_index} corner {source_corner}",
                        source_path.display()
                    ))
                })?;
                let uv = document
                    .appearance
                    .texcoords
                    .get(uv_index as usize)
                    .ok_or_else(|| {
                        VaMError::InvalidUv(format!(
                            "{} head triangle {source_face_index} references missing UV {uv_index}",
                            source_path.display()
                        ))
                    })?;
                let uv = [uv[0] as f32, uv[1] as f32];
                oriented_uvs[oriented_corner] = uv;
                let recovered_face = face_corner_uvs[*canonical_face_index]
                    .as_mut()
                    .expect("routed canonical face has a UV accumulator");
                let face_corner = face_corners[oriented_corner];
                if let Some(existing) = recovered_face[face_corner] {
                    if !uv_nearly_equal(existing, uv) {
                        return Err(VaMError::InvalidUv(format!(
                            "{} gives conflicting UVs for canonical face {} corner {face_corner}: {existing:?} and {uv:?}",
                            source_path.display(),
                            canonical_face_index
                        )));
                    }
                } else {
                    recovered_face[face_corner] = Some(uv);
                }
            }
            recovered_triangle_uvs[spec_index] = Some(oriented_uvs);
        }
    }

    let expected_triangles = triangle_specs.len();
    let mut uncovered = 0_usize;
    let mut triangles = Vec::with_capacity(expected_triangles);
    for (spec_index, (face_index, triangle_index, material_region, position_indices, _)) in
        triangle_specs.into_iter().enumerate()
    {
        let Some(uvs) = recovered_triangle_uvs[spec_index] else {
            uncovered += 1;
            continue;
        };
        triangles.push(G2UvTriangle {
            canonical_face_index: face_index as u32,
            canonical_triangle_index: triangle_index,
            material_region,
            on_head: face_is_on_head(geometry, face_index),
            position_indices,
            uvs,
        });
    }
    #[expect(clippy::cast_precision_loss, reason = "a triangle count")]
    let coverage = if expected_triangles == 0 {
        1.0
    } else {
        triangles.len() as f64 / expected_triangles as f64
    };
    if coverage < MINIMUM_TRIANGLE_COVERAGE {
        return Err(VaMError::InvalidUv(format!(
            "{} carries UVs for only {:.0}% of this figure's triangles              ({} of {expected_triangles}), so it is not this figure's UV source",
            source_path.display(),
            coverage * 100.0,
            triangles.len()
        )));
    }

    let mut faces = Vec::new();
    for (face_index, recovered) in face_corner_uvs.into_iter().enumerate() {
        let Some(recovered) = recovered else {
            continue;
        };
        let positions = &geometry.faces[face_index];
        let corners = positions
            .iter()
            .zip(recovered)
            .map(|(&position_index, uv)| {
                Ok(UvCorner {
                    position_index,
                    uv: uv.ok_or(PartialFace)?,
                })
            })
            .collect::<std::result::Result<Vec<_>, PartialFace>>();

        let Ok(corners) = corners else {
            continue;
        };
        faces.push(G2UvFace {
            canonical_face_index: face_index as u32,
            material_region: material_region(geometry, face_index)
                .expect("only routed faces have UV accumulators"),
            corners,
        });
    }
    Ok(G2UvMapping {
        source_path,
        coordinate_rms_cm,
        coordinate_max_cm: maximum_error,
        faces,
        triangles,
        uncovered_triangles: uncovered,
    })
}

fn source_position_mapping(document: &ObjDocument, canonical_count: usize) -> Vec<Option<u32>> {
    let mut reverse = BTreeMap::<[u64; 3], Vec<u32>>::new();
    for (index, position) in document
        .geometry
        .vertices
        .iter()
        .take(canonical_count)
        .enumerate()
    {
        reverse
            .entry(coordinate_key(*position))
            .or_default()
            .push(index as u32);
    }
    let mut mapping = vec![None; document.geometry.vertices.len()];
    for (index, target) in mapping.iter_mut().take(canonical_count).enumerate() {
        *target = Some(index as u32);
    }
    for (position, target) in document
        .geometry
        .vertices
        .iter()
        .zip(mapping.iter_mut())
        .skip(canonical_count)
    {
        let Some(candidates) = reverse.get(&coordinate_key(*position)) else {
            continue;
        };
        if candidates.len() == 1 {
            *target = Some(candidates[0]);
        }
    }
    mapping
}

fn coordinate_key(position: [f64; 3]) -> [u64; 3] {
    position.map(|value| if value == 0.0 { 0.0 } else { value }.to_bits())
}

fn canonical_material_name(geometry: &DazGeometry, face_index: usize) -> Result<String> {
    geometry
        .material_group_indices
        .get(face_index)
        .and_then(|index| geometry.material_groups.get(*index as usize))
        .map(|name| normalized_material_name(name))
        .ok_or_else(|| {
            VaMError::InvalidUv(format!("canonical face {face_index} has no material group"))
        })
}

fn normalized_material_name(name: &str) -> String {
    let lowered = name.trim().to_ascii_lowercase();
    let without_instance = lowered
        .rsplit_once('-')
        .filter(|(_, suffix)| {
            !suffix.is_empty() && suffix.chars().all(|value| value.is_ascii_digit())
        })
        .map_or(lowered.as_str(), |(base, _)| base);
    without_instance
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect()
}

fn uv_nearly_equal(left: [f32; 2], right: [f32; 2]) -> bool {
    left.into_iter()
        .zip(right)
        .all(|(left, right)| (left - right).abs() <= 1.0e-6)
}

fn material_region(geometry: &DazGeometry, face_index: usize) -> Option<UvMaterialRegion> {
    let material = geometry
        .material_group_indices
        .get(face_index)
        .and_then(|index| geometry.material_groups.get(*index as usize))
        .map(|name| normalized_material_name(name))?;
    material_region_from_name(&material)
}

fn face_is_on_head(geometry: &DazGeometry, face_index: usize) -> bool {
    geometry
        .material_group_indices
        .get(face_index)
        .and_then(|index| geometry.material_groups.get(*index as usize))
        .is_some_and(|name| {
            crate::formats::HEAD_VISUAL_MATERIALS
                .iter()
                .any(|head| name.eq_ignore_ascii_case(head))
        })
}

fn material_region_from_name(material: &str) -> Option<UvMaterialRegion> {
    match material {
        "face" | "lips" | "nostrils" => Some(UvMaterialRegion::Face),

        "head" | "neck" | "ears" | "torso" | "nipples" | "hips" => Some(UvMaterialRegion::Torso),

        "shoulders" | "forearms" | "hands" | "fingernails" | "legs" | "feet" | "toenails" => {
            Some(UvMaterialRegion::Limbs)
        }
        "genitalia" | "genitals" | "anus" | "defaultmat" => Some(UvMaterialRegion::Genitals),
        "sclera" => Some(UvMaterialRegion::Sclera),
        "irises" | "iris" => Some(UvMaterialRegion::Iris),
        "pupils" | "pupil" => Some(UvMaterialRegion::Pupil),
        "cornea" => Some(UvMaterialRegion::Cornea),
        "eyereflection" => Some(UvMaterialRegion::EyeReflection),
        "lacrimals" | "lacrimal" => Some(UvMaterialRegion::Lacrimal),
        "tear" | "tears" => Some(UvMaterialRegion::Tear),
        "innermouth" => Some(UvMaterialRegion::InnerMouth),
        "teeth" => Some(UvMaterialRegion::Teeth),
        "gums" => Some(UvMaterialRegion::Gums),
        "tongue" => Some(UvMaterialRegion::Tongue),
        "eyelashes" | "eyelash" => Some(UvMaterialRegion::Eyelashes),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use serde_json::json;

    use crate::formats::{DazGeometry, parse_obj_document};

    use super::*;

    fn triangle_geometry(material: &str) -> DazGeometry {
        DazGeometry::new(
            "g2".to_owned(),
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![vec![0, 1, 2]],
            crate::formats::GroupTable {
                indices: vec![0],
                names: vec!["head".to_owned()],
            },
            crate::formats::GroupTable {
                indices: vec![0],
                names: vec![material.to_owned()],
            },
            json!({}),
        )
        .unwrap()
    }

    fn quad_geometry(material: &str) -> DazGeometry {
        DazGeometry::new(
            "g2".to_owned(),
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            vec![vec![0, 1, 2, 3]],
            crate::formats::GroupTable {
                indices: vec![0],
                names: vec!["head".to_owned()],
            },
            crate::formats::GroupTable {
                indices: vec![0],
                names: vec![material.to_owned()],
            },
            json!({}),
        )
        .unwrap()
    }

    #[test]
    fn a_source_missing_a_patch_still_maps_what_it_covers() {
        let document = parse_obj_document(Cursor::new(
            b"v 0 0 0\nv 0.01 0 0\nv 0.01 0.01 0\nv 0 0.01 0\nvt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nusemtl Face\nf 1/1 2/2 3/3\n",
        ))
        .unwrap();
        let mapping = build_mapping(
            PathBuf::from("partial.obj"),
            &document,
            &quad_geometry("Face"),
            false,
            true,
        )
        .expect("a partial source still maps");
        assert_eq!(mapping.triangles.len(), 1, "the covered triangle is kept");
        assert_eq!(mapping.uncovered_triangles, 1, "and the gap is reported");

        assert!(mapping.faces.is_empty());
    }

    #[test]
    fn a_localized_difference_in_shape_is_not_a_wrong_vertex_order() {
        let mut errors = vec![0.0_f64; 21_556];
        for (offset, error) in errors.iter_mut().rev().take(282).enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "a small index")]
            let ramp = offset as f64;
            *error = 3.03 - ramp * 0.007;
        }
        assert_eq!(
            canonical_order_refusal(&errors),
            None,
            "{:?}",
            &errors[21_270..]
        );
    }

    #[test]
    fn a_shuffled_vertex_order_is_refused() {
        let errors = vec![14.0_f64; 21_556];
        let refusal = canonical_order_refusal(&errors).expect("a shuffle must be refused");
        assert!(refusal.contains("median"), "{refusal}");
    }

    #[test]
    fn a_region_of_misplaced_vertices_is_refused() {
        let mut errors = vec![0.0_f64; 21_556];
        for error in errors.iter_mut().take(2_500) {
            *error = 9.0;
        }
        errors.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let refusal = canonical_order_refusal(&errors).expect("a misplaced region must be refused");
        assert!(refusal.contains("% of vertices"), "{refusal}");
    }

    #[test]
    fn vertices_that_did_not_compare_count_as_apart() {
        let mut errors = vec![0.0_f64; 21_556];
        for error in errors.iter_mut().take(2_500) {
            *error = f64::NAN;
        }
        assert!(canonical_order_refusal(&errors).is_some());
    }

    #[test]
    fn projects_identity_vt_stream_onto_canonical_positions() {
        let document = parse_obj_document(Cursor::new(
            b"v 0 0 0\nv 0.01 0 0\nv 0 0.01 0\nvt 0 0\nvt 1 0\nvt 0 1\nusemtl Face\nf 1/1 2/2 3/3\n",
        ))
        .unwrap();
        let mapping = build_mapping(
            PathBuf::from("femalecustom.obj"),
            &document,
            &triangle_geometry("Face"),
            false,
            true,
        )
        .unwrap();
        assert_eq!(mapping.triangles.len(), 1);
        assert_eq!(mapping.triangles[0].position_indices, [0, 1, 2]);
        assert_eq!(mapping.triangles[0].material_region, UvMaterialRegion::Face);
    }

    #[test]
    fn accepts_non_identity_per_corner_uv_mapping() {
        let document = parse_obj_document(Cursor::new(
            b"v 0 0 0\nv 0.01 0 0\nv 0 0.01 0\nvt 0 0\nvt 1 0\nvt 0 1\nusemtl Face\nf 1/2 2/1 3/3\n",
        ))
        .unwrap();
        let mapping = build_mapping(
            PathBuf::from("per-corner.obj"),
            &document,
            &triangle_geometry("Face"),
            false,
            true,
        )
        .unwrap();
        assert_eq!(
            mapping.triangles[0].uvs,
            [[1.0, 0.0], [0.0, 0.0], [0.0, 1.0]]
        );
    }

    #[test]
    fn recovers_uv_seams_from_appended_duplicate_positions() {
        let document = parse_obj_document(Cursor::new(
            b"v 0 0 0\nv 0.01 0 0\nv 0.01 0.01 0\nv 0 0.01 0\n\
              v 0 0 0\nv 0.01 0.01 0\n\
              vt 0.9 0.9\nvt 1 0\nvt 0.8 0.8\nvt 0 1\nvt 0.1 0.2\nvt 0.7 0.6\n\
              usemtl Face-1\nf 5/5 2/2 6/6\nf 5/5 6/6 4/4\n",
        ))
        .unwrap();
        let mapping = build_mapping(
            PathBuf::from("seamed.obj"),
            &document,
            &quad_geometry("Face"),
            false,
            true,
        )
        .unwrap();
        assert_eq!(mapping.triangles.len(), 2);
        assert_eq!(mapping.faces[0].corners[0].uv, [0.1, 0.2]);
        assert_eq!(mapping.faces[0].corners[2].uv, [0.7, 0.6]);
        assert_eq!(mapping.triangles[0].uvs[0], [0.1, 0.2]);
        assert_eq!(mapping.triangles[1].uvs[1], [0.7, 0.6]);
    }

    #[test]
    fn head_mask_includes_eye_and_skin_materials() {
        let geometry = triangle_geometry("Sclera");
        assert_eq!(
            canonical_head_vertex_mask(&geometry).unwrap(),
            [true, true, true]
        );
    }

    #[test]
    fn every_g2_body_material_routes_to_a_body_tile() {
        let expected = [
            ("Torso", UvMaterialRegion::Torso),
            ("Hips", UvMaterialRegion::Torso),
            ("Nipples", UvMaterialRegion::Torso),
            ("Head", UvMaterialRegion::Torso),
            ("Neck", UvMaterialRegion::Torso),
            ("Ears", UvMaterialRegion::Torso),
            ("Shoulders", UvMaterialRegion::Limbs),
            ("Forearms", UvMaterialRegion::Limbs),
            ("Hands", UvMaterialRegion::Limbs),
            ("Fingernails", UvMaterialRegion::Limbs),
            ("Legs", UvMaterialRegion::Limbs),
            ("Feet", UvMaterialRegion::Limbs),
            ("Toenails", UvMaterialRegion::Limbs),
            ("defaultMat", UvMaterialRegion::Genitals),
            ("Genitalia", UvMaterialRegion::Genitals),
        ];
        for (material, region) in expected {
            assert_eq!(
                material_region_from_name(&normalized_material_name(material)),
                Some(region),
                "{material}"
            );
        }
    }

    #[test]
    fn no_g2_material_is_left_without_a_route() {
        let every_material = [
            "Face",
            "Head",
            "Neck",
            "Ears",
            "Lips",
            "Nostrils",
            "Torso",
            "Hips",
            "Nipples",
            "Shoulders",
            "Forearms",
            "Hands",
            "Fingernails",
            "Legs",
            "Feet",
            "Toenails",
            "Sclera",
            "Irises",
            "Pupils",
            "Cornea",
            "EyeReflection",
            "Lacrimals",
            "Tear",
            "InnerMouth",
            "Teeth",
            "Gums",
            "Tongue",
            "Eyelashes",
        ];
        assert_eq!(
            every_material.len(),
            28,
            "the base figure carries 28 groups"
        );
        let stranded: Vec<&str> = every_material
            .into_iter()
            .filter(|material| {
                material_region_from_name(&normalized_material_name(material)).is_none()
            })
            .collect();
        assert!(stranded.is_empty(), "no route for {stranded:?}");
    }

    #[test]
    fn every_visible_g2_head_material_has_an_explicit_preview_route() {
        let expected = [
            ("Face", UvMaterialRegion::Face),
            ("Head", UvMaterialRegion::Torso),
            ("Neck", UvMaterialRegion::Torso),
            ("Ears", UvMaterialRegion::Torso),
            ("Lips", UvMaterialRegion::Face),
            ("Nostrils", UvMaterialRegion::Face),
            ("Sclera", UvMaterialRegion::Sclera),
            ("Irises", UvMaterialRegion::Iris),
            ("Pupils", UvMaterialRegion::Pupil),
            ("Cornea", UvMaterialRegion::Cornea),
            ("EyeReflection", UvMaterialRegion::EyeReflection),
            ("InnerMouth", UvMaterialRegion::InnerMouth),
            ("Teeth", UvMaterialRegion::Teeth),
            ("Gums", UvMaterialRegion::Gums),
            ("Tongue", UvMaterialRegion::Tongue),
            ("Lacrimals", UvMaterialRegion::Lacrimal),
            ("Tear", UvMaterialRegion::Tear),
            ("Eyelashes", UvMaterialRegion::Eyelashes),
        ];
        for (material, region) in expected {
            let geometry = triangle_geometry(material);
            assert_eq!(material_region(&geometry, 0), Some(region), "{material}");
        }
    }
}

#[cfg(test)]
mod head_membership {
    use super::*;

    #[test]
    fn the_torso_tile_carries_head_materials_and_body_materials_alike() {
        let head = ["Face", "Head", "Neck", "Ears", "Lips", "Nostrils"];
        let body = ["Torso", "Hips", "Nipples"];
        for name in head {
            assert!(
                crate::formats::HEAD_VISUAL_MATERIALS
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(name)),
                "{name} should be part of the head"
            );
        }
        for name in body {
            assert!(
                !crate::formats::HEAD_VISUAL_MATERIALS
                    .iter()
                    .any(|known| known.eq_ignore_ascii_case(name)),
                "{name} is below the neck"
            );

            assert_eq!(
                material_region_from_name(&normalized_material_name(name)),
                Some(UvMaterialRegion::Torso)
            );
        }
    }

    /// Holds the bundle UV path to the file path it replaced.
    ///
    /// The loose `femalecustom.obj` is not part of a stock installation — it is
    /// written out later by features not everyone uses — so requiring it left a
    /// clean install with no UV mapping at all. The bundle ships with every
    /// copy and carries the same coordinates; this proves "the same" rather
    /// than assuming it, and it names the regions whose orientation took the
    /// longest to get right, because those are the ones a changed source would
    /// break most quietly.
    #[test]
    #[ignore = "requires the user's VaM installation"]
    fn the_bundle_carries_the_same_uv_islands_as_the_loose_obj() {
        use std::collections::HashMap;
        let Some(root) =
            std::env::var_os("VKIT_VAM_ROOT").and_then(|path| VaMRoot::open(path).ok())
        else {
            eprintln!("set VKIT_VAM_ROOT to run this");
            return;
        };
        let Ok(obj_path) = std::env::var("VKIT_UV_REFERENCE_OBJ") else {
            eprintln!("set VKIT_UV_REFERENCE_OBJ to that installation's femalecustom.obj");
            return;
        };
        let bundle =
            std::fs::read(root.neutral_base_bundle_path(crate::vam::geometry::GeometrySex::Female))
                .expect("read the female figure bundle");
        let figure = crate::vam::unity_base::extract_figure_uv(&bundle).expect("figure uv");

        let mut bundle_islands: HashMap<String, [f64; 4]> = HashMap::new();
        for (face, uv_corners) in figure.uv_polygons.iter().enumerate() {
            let Some(material) = figure
                .material_indices
                .get(face)
                .and_then(|index| figure.material_names.get(*index as usize))
            else {
                continue;
            };
            let island = bundle_islands.entry(material.clone()).or_insert([
                f64::MAX,
                f64::MIN,
                f64::MAX,
                f64::MIN,
            ]);
            for uv_index in uv_corners {
                let uv = figure.uvs[*uv_index as usize];
                island[0] = island[0].min(f64::from(uv[0]));
                island[1] = island[1].max(f64::from(uv[0]));
                island[2] = island[2].min(f64::from(uv[1]));
                island[3] = island[3].max(f64::from(uv[1]));
            }
        }

        let document = crate::formats::load_obj_document(std::path::Path::new(&obj_path))
            .expect("read the reference obj");
        let mut obj_islands: HashMap<String, [f64; 4]> = HashMap::new();
        for (face, uv_indices) in document
            .geometry
            .faces
            .iter()
            .zip(&document.appearance.face_texcoord_indices)
        {
            let Some(material) = face.material.as_deref() else {
                continue;
            };
            // The exported OBJ suffixes every material with its figure number.
            let material = material.rsplit_once('-').map_or(material, |(head, _)| head);
            let island = obj_islands.entry(material.to_owned()).or_insert([
                f64::MAX,
                f64::MIN,
                f64::MAX,
                f64::MIN,
            ]);
            for uv_index in uv_indices.iter().flatten() {
                let uv = document.appearance.texcoords[*uv_index as usize];
                island[0] = island[0].min(uv[0]);
                island[1] = island[1].max(uv[0]);
                island[2] = island[2].min(uv[1]);
                island[3] = island[3].max(uv[1]);
            }
        }

        for material in [
            "Sclera",
            "Irises",
            "Pupils",
            "Cornea",
            "Lacrimals",
            "Tear",
            "Eyelashes",
            "Teeth",
            "Gums",
            "Tongue",
            "InnerMouth",
            "Face",
            "Lips",
            "Head",
            "Neck",
            "Ears",
        ] {
            let from_bundle = bundle_islands
                .get(material)
                .unwrap_or_else(|| panic!("the bundle carries no {material} island"));
            let from_obj = obj_islands
                .get(material)
                .unwrap_or_else(|| panic!("the reference obj carries no {material} island"));
            for corner in 0..4 {
                let delta = (from_bundle[corner] - from_obj[corner]).abs();
                assert!(
                    delta < 1.0e-6,
                    "{material} island corner {corner} differs by {delta}:                      bundle {from_bundle:?} against obj {from_obj:?}"
                );
            }
        }
    }
}
