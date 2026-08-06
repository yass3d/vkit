use std::fmt;

use crate::formats::{Mesh, SurfaceAttachment};
use crate::math::{SimilarityTransform, Vec3};

pub const SEMANTIC_RENDER_SIZE: usize = 256;

const CROP_HEIGHT_FACTOR: f64 = 0.82;
const CROP_CENTER_FROM_TOP: f64 = 0.52;
const BARYCENTRIC_EPSILON: f64 = 1.0e-8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticProjectionFrame {
    pub center_x: f64,
    pub center_y: f64,
    pub side_cm: f64,
}

impl SemanticProjectionFrame {
    #[must_use]
    pub fn normalized_to_world_xy(self, normalized_x: f64, normalized_y: f64) -> (f64, f64) {
        (
            self.center_x + (normalized_x - 0.5) * self.side_cm,
            self.center_y + (0.5 - normalized_y) * self.side_cm,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticFaceRender {
    pub rgb: Vec<f32>,
    pub width: usize,
    pub height: usize,
    pub frame: SemanticProjectionFrame,

    pub(crate) depths: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticSurfaceRejection {
    InvalidSurface,
    InvalidTriangleSelection,
    DegenerateProjectedBounds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemanticSurfaceError {
    pub reason: SemanticSurfaceRejection,
}

impl fmt::Display for SemanticSurfaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "semantic surface preparation rejected: {:?}",
            self.reason
        )
    }
}

impl std::error::Error for SemanticSurfaceError {}

pub fn render_semantic_face(
    mesh: &Mesh,
    triangle_ids: &[u32],
    transform: SimilarityTransform,
) -> Result<SemanticFaceRender, SemanticSurfaceError> {
    validate_selection(mesh, triangle_ids)?;
    let vertices = mesh
        .vertices
        .iter()
        .copied()
        .map(Vec3::from)
        .map(|point| transform.apply(point))
        .collect::<Vec<_>>();
    let frame = semantic_projection_frame(&vertices, &mesh.triangles, triangle_ids)?;
    let vertex_normals = smooth_camera_facing_normals(&vertices, &mesh.triangles, triangle_ids);
    let pixel_count = SEMANTIC_RENDER_SIZE * SEMANTIC_RENDER_SIZE;
    let mut rgb = vec![0.0_f32; pixel_count * 3];
    let mut depths = vec![f64::NEG_INFINITY; pixel_count];
    let light = normalize_or_default(Vec3::new(0.30, 0.42, 0.855));

    for &primitive in triangle_ids {
        let triangle = mesh.triangles[primitive as usize];
        let points = triangle.map(|index| vertices[index as usize]);
        let projected = points.map(|point| world_to_pixel(frame, point));
        let denominator = edge(projected[0], projected[1], projected[2]);
        if !denominator.is_finite() || denominator.abs() <= 1.0e-12 {
            continue;
        }
        let Some((minimum_x, maximum_x, minimum_y, maximum_y)) = clipped_pixel_bounds(projected)
        else {
            continue;
        };
        for pixel_y in minimum_y..=maximum_y {
            for pixel_x in minimum_x..=maximum_x {
                let sample = (pixel_x as f64 + 0.5, pixel_y as f64 + 0.5);
                let barycentric = barycentric_2d(projected, sample, denominator);
                if barycentric
                    .iter()
                    .any(|weight| *weight < -BARYCENTRIC_EPSILON)
                {
                    continue;
                }
                let depth = points[0].z * barycentric[0]
                    + points[1].z * barycentric[1]
                    + points[2].z * barycentric[2];
                let pixel_index = pixel_y * SEMANTIC_RENDER_SIZE + pixel_x;
                if depth <= depths[pixel_index] {
                    continue;
                }
                depths[pixel_index] = depth;
                let mut normal = vertex_normals[triangle[0] as usize] * barycentric[0]
                    + vertex_normals[triangle[1] as usize] * barycentric[1]
                    + vertex_normals[triangle[2] as usize] * barycentric[2];
                normal = normalize_or_default(normal);
                if normal.z < 0.0 {
                    normal = normal * -1.0;
                }
                let diffuse = normal.dot(light).max(0.0);

                let intensity = (0.56 + diffuse * 0.44).clamp(0.0, 1.0) as f32;
                let rgb_index = pixel_index * 3;
                rgb[rgb_index] = intensity;
                rgb[rgb_index + 1] = intensity;
                rgb[rgb_index + 2] = intensity;
            }
        }
    }

    Ok(SemanticFaceRender {
        rgb,
        width: SEMANTIC_RENDER_SIZE,
        height: SEMANTIC_RENDER_SIZE,
        frame,
        depths,
    })
}

#[must_use]
pub fn attach_semantic_points(
    mesh: &Mesh,
    triangle_ids: &[u32],
    transform: SimilarityTransform,
    frame: SemanticProjectionFrame,
    normalized_points: &[[f32; 3]],
) -> Vec<Option<SurfaceAttachment>> {
    if validate_selection(mesh, triangle_ids).is_err()
        || !frame.center_x.is_finite()
        || !frame.center_y.is_finite()
        || !frame.side_cm.is_finite()
        || frame.side_cm <= 0.0
    {
        return vec![None; normalized_points.len()];
    }
    let vertices = mesh
        .vertices
        .iter()
        .copied()
        .map(Vec3::from)
        .map(|point| transform.apply(point))
        .collect::<Vec<_>>();

    normalized_points
        .iter()
        .map(|point| {
            let normalized_x = f64::from(point[0]);
            let normalized_y = f64::from(point[1]);
            if !normalized_x.is_finite()
                || !normalized_y.is_finite()
                || !(-0.05..=1.05).contains(&normalized_x)
                || !(-0.05..=1.05).contains(&normalized_y)
            {
                return None;
            }
            let query = frame.normalized_to_world_xy(normalized_x, normalized_y);
            frontmost_attachment(mesh, triangle_ids, &vertices, query).map(|hit| hit.attachment)
        })
        .collect()
}

#[must_use]
pub fn attach_visible_semantic_points(
    mesh: &Mesh,
    triangle_ids: &[u32],
    transform: SimilarityTransform,
    render: &SemanticFaceRender,
    normalized_points: &[[f32; 3]],
) -> Vec<Option<SurfaceAttachment>> {
    if validate_selection(mesh, triangle_ids).is_err()
        || render.depths.len() != render.width * render.height
        || render.width != SEMANTIC_RENDER_SIZE
        || render.height != SEMANTIC_RENDER_SIZE
    {
        return vec![None; normalized_points.len()];
    }
    let vertices = mesh
        .vertices
        .iter()
        .copied()
        .map(Vec3::from)
        .map(|point| transform.apply(point))
        .collect::<Vec<_>>();
    let depth_tolerance = render.frame.side_cm * 0.025;
    normalized_points
        .iter()
        .map(|point| {
            let normalized_x = f64::from(point[0]);
            let normalized_y = f64::from(point[1]);
            if !normalized_x.is_finite()
                || !normalized_y.is_finite()
                || !(-0.05..=1.05).contains(&normalized_x)
                || !(-0.05..=1.05).contains(&normalized_y)
            {
                return None;
            }
            let visible_depth = visible_depth_near(render, normalized_x, normalized_y)?;
            let query = render
                .frame
                .normalized_to_world_xy(normalized_x, normalized_y);
            let hit = frontmost_attachment(mesh, triangle_ids, &vertices, query)?;
            (hit.depth + depth_tolerance >= visible_depth).then_some(hit.attachment)
        })
        .collect()
}

fn semantic_projection_frame(
    vertices: &[Vec3],
    triangles: &[[u32; 3]],
    triangle_ids: &[u32],
) -> Result<SemanticProjectionFrame, SemanticSurfaceError> {
    let mut minimum_x = f64::INFINITY;
    let mut maximum_x = f64::NEG_INFINITY;
    let mut minimum_y = f64::INFINITY;
    let mut maximum_y = f64::NEG_INFINITY;
    for &primitive in triangle_ids {
        for &index in &triangles[primitive as usize] {
            let point = vertices[index as usize];
            minimum_x = minimum_x.min(point.x);
            maximum_x = maximum_x.max(point.x);
            minimum_y = minimum_y.min(point.y);
            maximum_y = maximum_y.max(point.y);
        }
    }
    let height = maximum_y - minimum_y;
    if !height.is_finite() || height <= 1.0e-9 {
        return Err(SemanticSurfaceError {
            reason: SemanticSurfaceRejection::DegenerateProjectedBounds,
        });
    }
    let center_y = maximum_y - CROP_CENTER_FROM_TOP * height;
    let side_cm = CROP_HEIGHT_FACTOR * height;
    if !minimum_x.is_finite()
        || !maximum_x.is_finite()
        || !center_y.is_finite()
        || !side_cm.is_finite()
        || side_cm <= 1.0e-9
    {
        return Err(SemanticSurfaceError {
            reason: SemanticSurfaceRejection::DegenerateProjectedBounds,
        });
    }
    Ok(SemanticProjectionFrame {
        center_x: (minimum_x + maximum_x) * 0.5,
        center_y,
        side_cm,
    })
}

fn validate_selection(mesh: &Mesh, triangle_ids: &[u32]) -> Result<(), SemanticSurfaceError> {
    if mesh.require_surface().is_err() {
        return Err(SemanticSurfaceError {
            reason: SemanticSurfaceRejection::InvalidSurface,
        });
    }
    if triangle_ids.is_empty()
        || triangle_ids
            .iter()
            .any(|primitive| *primitive as usize >= mesh.triangles.len())
    {
        return Err(SemanticSurfaceError {
            reason: SemanticSurfaceRejection::InvalidTriangleSelection,
        });
    }
    Ok(())
}

fn smooth_camera_facing_normals(
    vertices: &[Vec3],
    triangles: &[[u32; 3]],
    triangle_ids: &[u32],
) -> Vec<Vec3> {
    let mut normals = vec![Vec3::ZERO; vertices.len()];
    for &primitive in triangle_ids {
        let triangle = triangles[primitive as usize];
        let points = triangle.map(|index| vertices[index as usize]);
        let mut normal = cross(points[1] - points[0], points[2] - points[0]);
        if normal.z < 0.0 {
            normal = normal * -1.0;
        }
        for index in triangle {
            normals[index as usize] += normal;
        }
    }
    normals
        .into_iter()
        .map(normalize_or_default)
        .collect::<Vec<_>>()
}

struct FrontmostSurfaceHit {
    depth: f64,
    attachment: SurfaceAttachment,
}

fn frontmost_attachment(
    mesh: &Mesh,
    triangle_ids: &[u32],
    vertices: &[Vec3],
    query: (f64, f64),
) -> Option<FrontmostSurfaceHit> {
    let mut best: Option<(f64, u32, [f64; 3])> = None;
    for &primitive in triangle_ids {
        let triangle = mesh.triangles[primitive as usize];
        let points = triangle.map(|index| vertices[index as usize]);
        let projected = points.map(|point| (point.x, point.y));
        let denominator = edge(projected[0], projected[1], projected[2]);
        if denominator.abs() <= 1.0e-12 {
            continue;
        }
        let barycentric = barycentric_2d(projected, query, denominator);
        if barycentric
            .iter()
            .any(|weight| *weight < -BARYCENTRIC_EPSILON)
        {
            continue;
        }
        let depth = points[0].z * barycentric[0]
            + points[1].z * barycentric[1]
            + points[2].z * barycentric[2];
        let replace = best.as_ref().is_none_or(|(best_depth, best_primitive, _)| {
            depth > *best_depth + 1.0e-10
                || ((depth - *best_depth).abs() <= 1.0e-10 && primitive < *best_primitive)
        });
        if replace {
            best = Some((depth, primitive, normalize_barycentric(barycentric)));
        }
    }
    best.map(|(depth, primitive, barycentric)| FrontmostSurfaceHit {
        depth,
        attachment: SurfaceAttachment {
            triangle_vertex_ids: mesh.triangles[primitive as usize],
            barycentric,
            primitive_id: Some(primitive),
        },
    })
}

fn visible_depth_near(
    render: &SemanticFaceRender,
    normalized_x: f64,
    normalized_y: f64,
) -> Option<f64> {
    let center_x = (normalized_x * render.width as f64 - 0.5).round() as isize;
    let center_y = (normalized_y * render.height as f64 - 0.5).round() as isize;
    let mut nearest = f64::NEG_INFINITY;
    for offset_y in -1..=1 {
        for offset_x in -1..=1 {
            let pixel_x = center_x + offset_x;
            let pixel_y = center_y + offset_y;
            if pixel_x < 0
                || pixel_y < 0
                || pixel_x >= render.width as isize
                || pixel_y >= render.height as isize
            {
                continue;
            }
            nearest =
                nearest.max(render.depths[pixel_y as usize * render.width + pixel_x as usize]);
        }
    }
    nearest.is_finite().then_some(nearest)
}

fn world_to_pixel(frame: SemanticProjectionFrame, point: Vec3) -> (f64, f64) {
    (
        ((point.x - frame.center_x) / frame.side_cm + 0.5) * SEMANTIC_RENDER_SIZE as f64,
        ((frame.center_y - point.y) / frame.side_cm + 0.5) * SEMANTIC_RENDER_SIZE as f64,
    )
}

fn clipped_pixel_bounds(projected: [(f64, f64); 3]) -> Option<(usize, usize, usize, usize)> {
    let minimum_x = projected
        .iter()
        .map(|point| point.0)
        .min_by(f64::total_cmp)?
        .floor()
        .max(0.0) as usize;
    let maximum_x = projected
        .iter()
        .map(|point| point.0)
        .max_by(f64::total_cmp)?
        .ceil()
        .min(SEMANTIC_RENDER_SIZE as f64 - 1.0) as usize;
    let minimum_y = projected
        .iter()
        .map(|point| point.1)
        .min_by(f64::total_cmp)?
        .floor()
        .max(0.0) as usize;
    let maximum_y = projected
        .iter()
        .map(|point| point.1)
        .max_by(f64::total_cmp)?
        .ceil()
        .min(SEMANTIC_RENDER_SIZE as f64 - 1.0) as usize;
    (minimum_x <= maximum_x && minimum_y <= maximum_y)
        .then_some((minimum_x, maximum_x, minimum_y, maximum_y))
}

fn barycentric_2d(triangle: [(f64, f64); 3], point: (f64, f64), denominator: f64) -> [f64; 3] {
    [
        edge(triangle[1], triangle[2], point) / denominator,
        edge(triangle[2], triangle[0], point) / denominator,
        edge(triangle[0], triangle[1], point) / denominator,
    ]
}

fn edge(first: (f64, f64), second: (f64, f64), point: (f64, f64)) -> f64 {
    (second.0 - first.0) * (point.1 - first.1) - (second.1 - first.1) * (point.0 - first.0)
}

fn normalize_barycentric(mut barycentric: [f64; 3]) -> [f64; 3] {
    for weight in &mut barycentric {
        if weight.abs() <= BARYCENTRIC_EPSILON {
            *weight = 0.0;
        }
    }
    let sum = barycentric.iter().sum::<f64>();
    if sum.abs() > 1.0e-12 {
        for weight in &mut barycentric {
            *weight /= sum;
        }
    }
    barycentric
}

fn cross(first: Vec3, second: Vec3) -> Vec3 {
    Vec3::new(
        first.y * second.z - first.z * second.y,
        first.z * second.x - first.x * second.z,
        first.x * second.y - first.y * second.x,
    )
}

fn normalize_or_default(vector: Vec3) -> Vec3 {
    let norm = vector.norm();
    if norm > 1.0e-12 && norm.is_finite() {
        vector / norm
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlapping_mesh() -> Mesh {
        Mesh::new(
            vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [0.0, 1.0, 0.0],
                [-1.0, -1.0, 1.0],
                [0.0, 1.0, 1.0],
                [1.0, -1.0, 1.0],
            ],
            vec![[0, 1, 2], [3, 4, 5]],
        )
        .unwrap()
    }

    #[test]
    fn projection_frame_uses_the_vetted_head_bounds_crop() {
        let mesh = overlapping_mesh();
        let render = render_semantic_face(&mesh, &[0, 1], SimilarityTransform::IDENTITY).unwrap();
        assert_eq!(render.width, SEMANTIC_RENDER_SIZE);
        assert_eq!(render.height, SEMANTIC_RENDER_SIZE);
        assert_eq!(render.rgb.len(), SEMANTIC_RENDER_SIZE.pow(2) * 3);
        assert!((render.frame.center_x - 0.0).abs() <= 1.0e-12);
        assert!((render.frame.center_y - (-0.04)).abs() <= 1.0e-12);
        assert!((render.frame.side_cm - 1.64).abs() <= 1.0e-12);
        assert!(render.rgb.iter().any(|value| *value > 0.5));
    }

    #[test]
    fn continuous_surface_recovery_keeps_the_frontmost_original_primitive() {
        let mesh = overlapping_mesh();
        let render = render_semantic_face(&mesh, &[0, 1], SimilarityTransform::IDENTITY).unwrap();
        let normalized_x = 0.5_f32;
        let normalized_y = ((render.frame.center_y - 0.0) / render.frame.side_cm + 0.5) as f32;
        let hits = attach_semantic_points(
            &mesh,
            &[0, 1],
            SimilarityTransform::IDENTITY,
            render.frame,
            &[[normalized_x, normalized_y, 0.0]],
        );
        let hit = hits[0].as_ref().unwrap();
        assert_eq!(hit.primitive_id, Some(1));
        assert_eq!(hit.triangle_vertex_ids, [3, 4, 5]);
        assert!((hit.barycentric.iter().sum::<f64>() - 1.0).abs() <= 1.0e-12);
    }

    #[test]
    fn visible_recovery_rejects_hidden_back_surface_through_an_opening() {
        let mesh = overlapping_mesh();
        let render = render_semantic_face(&mesh, &[0, 1], SimilarityTransform::IDENTITY).unwrap();
        let normalized_y = ((render.frame.center_y - 0.0) / render.frame.side_cm + 0.5) as f32;
        let hits = attach_visible_semantic_points(
            &mesh,
            &[0],
            SimilarityTransform::IDENTITY,
            &render,
            &[[0.5, normalized_y, 0.0]],
        );
        assert_eq!(hits, vec![None]);
    }

    #[test]
    fn render_and_attachment_are_deterministic_with_reversed_winding() {
        let mesh = overlapping_mesh();
        let first = render_semantic_face(&mesh, &[0, 1], SimilarityTransform::IDENTITY).unwrap();
        let second = render_semantic_face(&mesh, &[0, 1], SimilarityTransform::IDENTITY).unwrap();
        assert_eq!(first, second);

        let reversed = Mesh::new(mesh.vertices.clone(), vec![[2, 1, 0], [5, 4, 3]]).unwrap();
        let reversed_render =
            render_semantic_face(&reversed, &[0, 1], SimilarityTransform::IDENTITY).unwrap();
        assert_eq!(first.rgb, reversed_render.rgb);
    }

    #[test]
    fn invalid_selection_fails_closed_and_bad_points_do_not_attach() {
        let mesh = overlapping_mesh();
        assert_eq!(
            render_semantic_face(&mesh, &[], SimilarityTransform::IDENTITY)
                .unwrap_err()
                .reason,
            SemanticSurfaceRejection::InvalidTriangleSelection
        );
        let frame = SemanticProjectionFrame {
            center_x: 0.0,
            center_y: 0.0,
            side_cm: 2.0,
        };
        let hits = attach_semantic_points(
            &mesh,
            &[0, 1],
            SimilarityTransform::IDENTITY,
            frame,
            &[[f32::NAN, 0.5, 0.0], [2.0, 0.5, 0.0]],
        );
        assert_eq!(hits, vec![None, None]);
    }
}
