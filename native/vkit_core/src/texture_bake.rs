use std::collections::HashMap;

use rayon::prelude::*;
use thiserror::Error;

use crate::formats::ObjDocument;
use crate::{
    pixels::{RgbaView, resize_rgba_box},
    vam::{G2UvMapping, UvMaterialRegion},
};

const MAX_BAKE_EDGE: u32 = 4096;
const BARYCENTRIC_EPSILON: f32 = -1.0e-4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureBlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureColorAdjustments {
    pub exposure: f32,

    pub contrast: f32,

    pub saturation: f32,

    pub hue_degrees: f32,

    pub temperature: f32,
}

impl Default for TextureColorAdjustments {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 0.0,
            saturation: 0.0,
            hue_degrees: 0.0,
            temperature: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextureBakeOptions {
    pub width: u32,
    pub height: u32,

    pub boundary_feather_pixels: u16,
}

impl Default for TextureBakeOptions {
    fn default() -> Self {
        Self {
            width: 2048,
            height: 2048,
            boundary_feather_pixels: 16,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextureBakeImage {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

impl TextureBakeImage {
    pub fn from_rgba8(rgba8: Vec<u8>, width: u32, height: u32) -> Result<Self, TextureBakeError> {
        validate_dimensions(width, height)?;
        if rgba8.len() != width as usize * height as usize * 4 {
            return Err(TextureBakeError::InvalidSource(
                "pixel count does not match the dimensions given".to_owned(),
            ));
        }
        Ok(Self {
            width,
            height,
            rgba8,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AlphaMaskView<'a> {
    alpha8: &'a [u8],
    width: u32,
    height: u32,
}

impl<'a> AlphaMaskView<'a> {
    pub fn new(alpha8: &'a [u8], width: u32, height: u32) -> Result<Self, TextureBakeError> {
        validate_dimensions(width, height)?;
        if alpha8.len() != width as usize * height as usize {
            return Err(TextureBakeError::InvalidSource(
                "alpha mask byte count does not match its dimensions".to_owned(),
            ));
        }
        Ok(Self {
            alpha8,
            width,
            height,
        })
    }

    fn sample_scaled(self, index: usize, output_width: u32, output_height: u32) -> f32 {
        let x = index % output_width as usize;
        let y = index / output_width as usize;
        let source_x = ((x as f32 + 0.5) * self.width as f32 / output_width as f32 - 0.5)
            .clamp(0.0, self.width.saturating_sub(1) as f32);
        let source_y = ((y as f32 + 0.5) * self.height as f32 / output_height as f32 - 0.5)
            .clamp(0.0, self.height.saturating_sub(1) as f32);
        let x0 = source_x.floor() as usize;
        let y0 = source_y.floor() as usize;
        let x1 = (x0 + 1).min(self.width.saturating_sub(1) as usize);
        let y1 = (y0 + 1).min(self.height.saturating_sub(1) as usize);
        let tx = source_x - x0 as f32;
        let ty = source_y - y0 as f32;
        let width = self.width as usize;
        let top = f32::from(self.alpha8[y0 * width + x0])
            + (f32::from(self.alpha8[y0 * width + x1]) - f32::from(self.alpha8[y0 * width + x0]))
                * tx;
        let bottom = f32::from(self.alpha8[y1 * width + x0])
            + (f32::from(self.alpha8[y1 * width + x1]) - f32::from(self.alpha8[y1 * width + x0]))
                * tx;
        (top + (bottom - top) * ty) / 255.0
    }
}

impl TextureBakeImage {
    pub fn transparent(width: u32, height: u32) -> Result<Self, TextureBakeError> {
        validate_dimensions(width, height)?;
        Ok(Self {
            width,
            height,
            rgba8: vec![0; width as usize * height as usize * 4],
        })
    }

    pub fn view(&self) -> RgbaView<'_> {
        RgbaView {
            rgba8: &self.rgba8,
            width: self.width,
            height: self.height,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureWarpPin {
    pub source: [f32; 2],

    pub target_uv: [f32; 2],
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum TextureBakeError {
    #[error("texture bake dimensions {width}x{height} are invalid")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("projected G2 document is invalid: {0}")]
    InvalidProjectedDocument(String),
    #[error("texture bake source is invalid: {0}")]
    InvalidSource(String),
    #[error("G2 UV face {face} is missing from the projected document")]
    MissingFace { face: usize },
    #[error("G2 UV face {face} vertex {vertex} has no projected source UV")]
    MissingProjectedUv { face: usize, vertex: u32 },
    #[error("texture warp needs at least three complete, non-collinear pin pairs")]
    InsufficientWarpPins,
    #[error("texture warp contains a non-finite or out-of-range pin")]
    InvalidWarpPin,
    #[error("texture warp target pins contain duplicates")]
    DuplicateWarpTarget,
}

pub fn bake_projected_texture_to_g2(
    projected: &ObjDocument,
    mapping: &G2UvMapping,
    source: RgbaView<'_>,
    options: TextureBakeOptions,
) -> Result<TextureBakeImage, TextureBakeError> {
    validate_dimensions(options.width, options.height)?;
    RgbaView::new(source.rgba8, source.width, source.height)
        .map_err(TextureBakeError::InvalidSource)?;
    projected
        .validate()
        .map_err(|error| TextureBakeError::InvalidProjectedDocument(error.to_string()))?;

    let mut output = TextureBakeImage::transparent(options.width, options.height)?;
    for triangle in mapping
        .triangles
        .iter()
        .filter(|triangle| triangle.material_region == UvMaterialRegion::Face)
    {
        let face_index = triangle.canonical_face_index as usize;
        let face = projected
            .geometry
            .faces
            .get(face_index)
            .ok_or(TextureBakeError::MissingFace { face: face_index })?;
        let face_uvs = projected
            .appearance
            .face_texcoord_indices
            .get(face_index)
            .ok_or(TextureBakeError::MissingFace { face: face_index })?;
        let mut source_uvs = [[0.0_f32; 2]; 3];
        for (slot, &vertex) in triangle.position_indices.iter().enumerate() {
            let corner = face
                .vertex_indices
                .iter()
                .position(|candidate| *candidate == vertex)
                .ok_or(TextureBakeError::MissingProjectedUv {
                    face: face_index,
                    vertex,
                })?;
            let uv_index = face_uvs.get(corner).copied().flatten().ok_or(
                TextureBakeError::MissingProjectedUv {
                    face: face_index,
                    vertex,
                },
            )? as usize;
            let uv = projected.appearance.texcoords.get(uv_index).ok_or(
                TextureBakeError::MissingProjectedUv {
                    face: face_index,
                    vertex,
                },
            )?;
            source_uvs[slot] = [uv[0] as f32, uv[1] as f32];
        }
        raster_triangle(
            &mut output,
            triangle.uvs.map(obj_uv_to_top_origin),
            source,
            source_uvs.map(obj_uv_to_top_origin),
        );
    }
    feather_coverage_alpha(
        &mut output.rgba8,
        output.width,
        output.height,
        options.boundary_feather_pixels,
    );
    Ok(output)
}

pub fn warp_image_to_g2_by_pins(
    source: RgbaView<'_>,
    pins: &[TextureWarpPin],
    options: TextureBakeOptions,
) -> Result<TextureBakeImage, TextureBakeError> {
    warp_image_to_g2_by_pins_impl(source, pins, options, &[])
}

pub fn place_image_on_g2_uv_in_region(
    source: RgbaView<'_>,
    options: TextureBakeOptions,
    region_triangles: &[[[f32; 2]; 3]],
) -> Result<TextureBakeImage, TextureBakeError> {
    validate_dimensions(options.width, options.height)?;
    RgbaView::new(source.rgba8, source.width, source.height)
        .map_err(TextureBakeError::InvalidSource)?;
    let mut output = TextureBakeImage::transparent(options.width, options.height)?;
    for triangle in region_triangles {
        let top_origin = triangle.map(|point| [point[0], 1.0 - point[1]]);
        raster_triangle(&mut output, top_origin, source, top_origin);
    }

    Ok(output)
}

#[derive(Clone, Copy, Debug)]
pub struct ProjectedTriangle {
    pub screen: [[f32; 2]; 3],
    pub uv: [[f32; 2]; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct ProjectionBrush {
    pub centre: [f32; 2],
    pub radius: f32,

    pub falloff: crate::sculpt::SculptFalloff,
    pub opacity: f32,
}

pub fn stamp_projection_onto_g2(
    target_rgba8: &mut [u8],
    target_width: u32,
    target_height: u32,
    source: RgbaView<'_>,
    triangles: &[ProjectedTriangle],
    brush: ProjectionBrush,
    to_source: impl Fn([f32; 2]) -> Option<[f32; 2]>,
) -> usize {
    let expected = target_width as usize * target_height as usize * 4;
    if target_rgba8.len() != expected {
        return 0;
    }
    if !brush
        .centre
        .iter()
        .chain([&brush.radius, &brush.opacity])
        .all(|value| value.is_finite())
        || brush.radius <= 0.0
        || brush.opacity <= 0.0
    {
        return 0;
    }
    let scale = [
        target_width.saturating_sub(1) as f32,
        target_height.saturating_sub(1) as f32,
    ];
    let mut painted = 0;

    for triangle in triangles {
        if !triangle
            .screen
            .iter()
            .flatten()
            .chain(triangle.uv.iter().flatten())
            .all(|value| value.is_finite())
        {
            continue;
        }

        let (left, right, top, bottom) = bounds(&triangle.screen);
        if left - brush.radius > brush.centre[0]
            || right + brush.radius < brush.centre[0]
            || top - brush.radius > brush.centre[1]
            || bottom + brush.radius < brush.centre[1]
        {
            continue;
        }

        let pixels = triangle
            .uv
            .map(|point| [point[0] * scale[0], (1.0 - point[1]) * scale[1]]);
        let area = edge(pixels[0], pixels[1], pixels[2]);
        if area.abs() <= f32::EPSILON {
            continue;
        }
        let (min_x, max_x, min_y, max_y) = pixel_span(&pixels, scale);

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let point = [x as f32 + 0.5, y as f32 + 0.5];
                let weights = [
                    edge(pixels[1], pixels[2], point) / area,
                    edge(pixels[2], pixels[0], point) / area,
                    edge(pixels[0], pixels[1], point) / area,
                ];
                if weights.iter().any(|weight| *weight < BARYCENTRIC_EPSILON) {
                    continue;
                }

                let screen = [
                    weights[0] * triangle.screen[0][0]
                        + weights[1] * triangle.screen[1][0]
                        + weights[2] * triangle.screen[2][0],
                    weights[0] * triangle.screen[0][1]
                        + weights[1] * triangle.screen[1][1]
                        + weights[2] * triangle.screen[2][1],
                ];
                let distance = ((screen[0] - brush.centre[0]).powi(2)
                    + (screen[1] - brush.centre[1]).powi(2))
                .sqrt();
                if distance > brush.radius {
                    continue;
                }
                let weight = brush.falloff.weight(f64::from(distance / brush.radius)) as f32;
                if weight <= 0.0 {
                    continue;
                }
                let Some(uv) = to_source(screen) else {
                    continue;
                };
                let sample = bilinear_sample(source, uv);
                let coverage = weight * brush.opacity * f32::from(sample[3]) / 255.0;
                if coverage <= 0.0 {
                    continue;
                }
                blend_over(target_rgba8, target_width, x, y, sample, coverage);
                painted += 1;
            }
        }
    }
    painted
}

fn bounds(points: &[[f32; 2]; 3]) -> (f32, f32, f32, f32) {
    let xs = points.map(|point| point[0]);
    let ys = points.map(|point| point[1]);
    (
        xs.iter().copied().fold(f32::INFINITY, f32::min),
        xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        ys.iter().copied().fold(f32::INFINITY, f32::min),
        ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
    )
}

fn pixel_span(pixels: &[[f32; 2]; 3], scale: [f32; 2]) -> (u32, u32, u32, u32) {
    let (left, right, top, bottom) = bounds(pixels);
    (
        left.floor().clamp(0.0, scale[0]) as u32,
        right.ceil().clamp(0.0, scale[0]) as u32,
        top.floor().clamp(0.0, scale[1]) as u32,
        bottom.ceil().clamp(0.0, scale[1]) as u32,
    )
}

fn blend_over(target: &mut [u8], width: u32, x: u32, y: u32, source: [u8; 4], coverage: f32) {
    let index = (y as usize * width as usize + x as usize) * 4;
    let Some(slot) = target.get_mut(index..index + 4) else {
        return;
    };
    let coverage = coverage.clamp(0.0, 1.0);
    let existing_alpha = f32::from(slot[3]) / 255.0;
    let out_alpha = coverage + existing_alpha * (1.0 - coverage);
    if out_alpha <= 0.0 {
        return;
    }
    for channel in 0..3 {
        let existing = f32::from(slot[channel]);
        let incoming = f32::from(source[channel]);
        let blended =
            (incoming * coverage + existing * existing_alpha * (1.0 - coverage)) / out_alpha;
        slot[channel] = blended.round().clamp(0.0, 255.0) as u8;
    }
    slot[3] = (out_alpha * 255.0).round() as u8;
}

pub fn warp_image_to_g2_by_pins_in_region(
    source: RgbaView<'_>,
    pins: &[TextureWarpPin],
    options: TextureBakeOptions,
    region_triangles: &[[[f32; 2]; 3]],
) -> Result<TextureBakeImage, TextureBakeError> {
    warp_image_to_g2_by_pins_impl(source, pins, options, region_triangles)
}

fn warp_image_to_g2_by_pins_impl(
    source: RgbaView<'_>,
    pins: &[TextureWarpPin],
    options: TextureBakeOptions,
    region_triangles: &[[[f32; 2]; 3]],
) -> Result<TextureBakeImage, TextureBakeError> {
    validate_dimensions(options.width, options.height)?;
    RgbaView::new(source.rgba8, source.width, source.height)
        .map_err(TextureBakeError::InvalidSource)?;
    if pins.len() < 3 {
        return Err(TextureBakeError::InsufficientWarpPins);
    }
    let mut points = Vec::with_capacity(pins.len());
    for pin in pins {
        if !pin
            .source
            .into_iter()
            .chain(pin.target_uv)
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        {
            return Err(TextureBakeError::InvalidWarpPin);
        }
        points.push([pin.target_uv[0], 1.0 - pin.target_uv[1]]);
    }
    for left in 0..points.len() {
        if points[left + 1..]
            .iter()
            .any(|right| squared_distance(points[left], *right) <= 1.0e-12)
        {
            return Err(TextureBakeError::DuplicateWarpTarget);
        }
    }
    let triangles = delaunay_triangles(&points);
    if triangles.is_empty() {
        return Err(TextureBakeError::InsufficientWarpPins);
    }
    let mut output = TextureBakeImage::transparent(options.width, options.height)?;
    let pixel_count = options.width as usize * options.height as usize;
    let mut pin_coverage = (!region_triangles.is_empty()).then(|| vec![false; pixel_count]);
    for triangle in &triangles {
        let target = triangle.map(|index| points[index]);
        let source_uvs = triangle.map(|index| pins[index].source);
        raster_triangle(&mut output, target, source, source_uvs);
        if let Some(coverage) = pin_coverage.as_mut() {
            raster_triangle_mask(coverage, options.width, options.height, target);
        }
    }
    if !region_triangles.is_empty() {
        let mut region_coverage = vec![false; pixel_count];
        for triangle in region_triangles {
            let top_origin = triangle.map(|point| [point[0], 1.0 - point[1]]);
            raster_triangle_mask(
                &mut region_coverage,
                options.width,
                options.height,
                top_origin,
            );
        }
        extrapolate_warp_to_region(
            &mut output,
            source,
            pins,
            &points,
            &triangles,
            pin_coverage.as_deref().unwrap_or_default(),
            &region_coverage,
        );
    }
    feather_coverage_alpha(
        &mut output.rgba8,
        output.width,
        output.height,
        options.boundary_feather_pixels,
    );
    Ok(output)
}

#[derive(Clone, Copy)]
struct WarpBoundaryEdge {
    start: usize,
    end: usize,
    triangle: [usize; 3],
}

fn extrapolate_warp_to_region(
    output: &mut TextureBakeImage,
    source: RgbaView<'_>,
    pins: &[TextureWarpPin],
    target_points: &[[f32; 2]],
    triangles: &[[usize; 3]],
    pin_coverage: &[bool],
    region_coverage: &[bool],
) {
    let boundary_edges = triangulation_boundary_edges(triangles);
    if boundary_edges.is_empty() {
        return;
    }
    let scale = [
        output.width.saturating_sub(1) as f32,
        output.height.saturating_sub(1) as f32,
    ];
    for (index, (&inside_pins, &inside_region)) in
        pin_coverage.iter().zip(region_coverage).enumerate()
    {
        if inside_pins || !inside_region {
            continue;
        }
        let x = index % output.width as usize;
        let y = index / output.width as usize;
        let point = [
            (x as f32 + 0.5) / scale[0].max(1.0),
            (y as f32 + 0.5) / scale[1].max(1.0),
        ];
        let Some(boundary) = boundary_edges.iter().min_by(|left, right| {
            let distance_order = squared_distance_to_segment(
                point,
                target_points[left.start],
                target_points[left.end],
            )
            .total_cmp(&squared_distance_to_segment(
                point,
                target_points[right.start],
                target_points[right.end],
            ));
            distance_order
                .then_with(|| left.start.cmp(&right.start))
                .then_with(|| left.end.cmp(&right.end))
        }) else {
            continue;
        };
        let target = boundary.triangle.map(|vertex| target_points[vertex]);
        let area = edge(target[0], target[1], target[2]);
        if area.abs() <= f32::EPSILON {
            continue;
        }
        let barycentric = [
            edge(target[1], target[2], point) / area,
            edge(target[2], target[0], point) / area,
            edge(target[0], target[1], point) / area,
        ];
        let source_uvs = boundary.triangle.map(|vertex| pins[vertex].source);
        let source_point = [0, 1].map(|axis| {
            barycentric[0].mul_add(
                source_uvs[0][axis],
                barycentric[1].mul_add(source_uvs[1][axis], barycentric[2] * source_uvs[2][axis]),
            )
        });
        let sampled = bilinear_sample(source, source_point);
        output.rgba8[index * 4..index * 4 + 4].copy_from_slice(&sampled);
    }
}

fn triangulation_boundary_edges(triangles: &[[usize; 3]]) -> Vec<WarpBoundaryEdge> {
    let mut edges = HashMap::<(usize, usize), ([usize; 3], u8)>::new();
    for &triangle in triangles {
        for (start, end) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let key = if start <= end {
                (start, end)
            } else {
                (end, start)
            };
            edges
                .entry(key)
                .and_modify(|(_, count)| *count = count.saturating_add(1))
                .or_insert((triangle, 1));
        }
    }
    edges
        .into_iter()
        .filter_map(|((start, end), (triangle, count))| {
            (count == 1).then_some(WarpBoundaryEdge {
                start,
                end,
                triangle,
            })
        })
        .collect()
}

fn squared_distance_to_segment(point: [f32; 2], start: [f32; 2], end: [f32; 2]) -> f32 {
    let delta = [end[0] - start[0], end[1] - start[1]];
    let length_squared = delta[0].mul_add(delta[0], delta[1] * delta[1]);
    if length_squared <= f32::EPSILON {
        return squared_distance(point, start);
    }
    let projection = ((point[0] - start[0]).mul_add(delta[0], (point[1] - start[1]) * delta[1])
        / length_squared)
        .clamp(0.0, 1.0);
    squared_distance(
        point,
        [
            start[0] + delta[0] * projection,
            start[1] + delta[1] * projection,
        ],
    )
}

pub fn map_source_point_to_g2(
    pins: &[TextureWarpPin],
    source_point: [f32; 2],
) -> Result<Option<[f32; 2]>, TextureBakeError> {
    if pins.len() < 3 {
        return Err(TextureBakeError::InsufficientWarpPins);
    }
    if !source_point
        .into_iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
    {
        return Err(TextureBakeError::InvalidWarpPin);
    }
    let mut target_points = Vec::with_capacity(pins.len());
    for pin in pins {
        if !pin
            .source
            .into_iter()
            .chain(pin.target_uv)
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        {
            return Err(TextureBakeError::InvalidWarpPin);
        }
        target_points.push([pin.target_uv[0], 1.0 - pin.target_uv[1]]);
    }
    for left in 0..target_points.len() {
        if target_points[left + 1..]
            .iter()
            .any(|right| squared_distance(target_points[left], *right) <= 1.0e-12)
        {
            return Err(TextureBakeError::DuplicateWarpTarget);
        }
    }
    let triangles = delaunay_triangles(&target_points);
    if triangles.is_empty() {
        return Err(TextureBakeError::InsufficientWarpPins);
    }
    let map_triangle = |triangle: [usize; 3]| {
        let source = triangle.map(|index| pins[index].source);
        let area = edge(source[0], source[1], source[2]);
        if area.abs() <= f32::EPSILON {
            return None;
        }
        let barycentric = [
            edge(source[1], source[2], source_point) / area,
            edge(source[2], source[0], source_point) / area,
            edge(source[0], source[1], source_point) / area,
        ];
        let target = [0, 1].map(|axis| {
            barycentric[0].mul_add(
                target_points[triangle[0]][axis],
                barycentric[1].mul_add(
                    target_points[triangle[1]][axis],
                    barycentric[2] * target_points[triangle[2]][axis],
                ),
            )
        });
        Some((barycentric, target))
    };
    for &triangle in &triangles {
        let Some((barycentric, target)) = map_triangle(triangle) else {
            continue;
        };
        if barycentric
            .iter()
            .all(|weight| *weight >= BARYCENTRIC_EPSILON)
        {
            return Ok(Some([
                target[0].clamp(0.0, 1.0),
                (1.0 - target[1]).clamp(0.0, 1.0),
            ]));
        }
    }
    let boundary_edges = triangulation_boundary_edges(&triangles);
    let Some(boundary) = boundary_edges.iter().min_by(|left, right| {
        squared_distance_to_segment(source_point, pins[left.start].source, pins[left.end].source)
            .total_cmp(&squared_distance_to_segment(
                source_point,
                pins[right.start].source,
                pins[right.end].source,
            ))
            .then_with(|| left.start.cmp(&right.start))
            .then_with(|| left.end.cmp(&right.end))
    }) else {
        return Ok(None);
    };
    Ok(map_triangle(boundary.triangle)
        .map(|(_, target)| [target[0].clamp(0.0, 1.0), (1.0 - target[1]).clamp(0.0, 1.0)]))
}

pub fn map_g2_point_to_source(
    pins: &[TextureWarpPin],
    target_uv: [f32; 2],
) -> Result<Option<[f32; 2]>, TextureBakeError> {
    if pins.len() < 3 {
        return Err(TextureBakeError::InsufficientWarpPins);
    }
    if !target_uv
        .into_iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
    {
        return Err(TextureBakeError::InvalidWarpPin);
    }
    let mut target_points = Vec::with_capacity(pins.len());
    for pin in pins {
        if !pin
            .source
            .into_iter()
            .chain(pin.target_uv)
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        {
            return Err(TextureBakeError::InvalidWarpPin);
        }
        target_points.push([pin.target_uv[0], 1.0 - pin.target_uv[1]]);
    }
    for left in 0..target_points.len() {
        if target_points[left + 1..]
            .iter()
            .any(|right| squared_distance(target_points[left], *right) <= 1.0e-12)
        {
            return Err(TextureBakeError::DuplicateWarpTarget);
        }
    }
    let triangles = delaunay_triangles(&target_points);
    if triangles.is_empty() {
        return Err(TextureBakeError::InsufficientWarpPins);
    }
    let point = [target_uv[0], 1.0 - target_uv[1]];
    let map_triangle = |triangle: [usize; 3]| {
        let target = triangle.map(|index| target_points[index]);
        let area = edge(target[0], target[1], target[2]);
        if area.abs() <= f32::EPSILON {
            return None;
        }
        let barycentric = [
            edge(target[1], target[2], point) / area,
            edge(target[2], target[0], point) / area,
            edge(target[0], target[1], point) / area,
        ];
        let source = triangle.map(|index| pins[index].source);
        Some((
            barycentric,
            [0, 1].map(|axis| {
                barycentric[0].mul_add(
                    source[0][axis],
                    barycentric[1].mul_add(source[1][axis], barycentric[2] * source[2][axis]),
                )
            }),
        ))
    };
    for &triangle in &triangles {
        let Some((barycentric, source)) = map_triangle(triangle) else {
            continue;
        };
        if barycentric
            .iter()
            .all(|weight| *weight >= BARYCENTRIC_EPSILON)
        {
            return Ok(Some(source.map(|value| value.clamp(0.0, 1.0))));
        }
    }
    let boundary_edges = triangulation_boundary_edges(&triangles);
    let Some(boundary) = boundary_edges.iter().min_by(|left, right| {
        squared_distance_to_segment(point, target_points[left.start], target_points[left.end])
            .total_cmp(&squared_distance_to_segment(
                point,
                target_points[right.start],
                target_points[right.end],
            ))
            .then_with(|| left.start.cmp(&right.start))
            .then_with(|| left.end.cmp(&right.end))
    }) else {
        return Ok(None);
    };
    Ok(
        map_triangle(boundary.triangle)
            .map(|(_, source)| source.map(|value| value.clamp(0.0, 1.0))),
    )
}

pub fn resize_direct_uv(
    source: RgbaView<'_>,
    width: u32,
    height: u32,
) -> Result<TextureBakeImage, TextureBakeError> {
    validate_dimensions(width, height)?;
    RgbaView::new(source.rgba8, source.width, source.height)
        .map_err(TextureBakeError::InvalidSource)?;
    Ok(TextureBakeImage {
        width,
        height,
        rgba8: resize_rgba_box(source, width, height),
    })
}

pub fn apply_color_adjustments(rgba8: &mut [u8], adjustments: TextureColorAdjustments) {
    let exposure = if adjustments.exposure.is_finite() {
        2.0_f32.powf(adjustments.exposure.clamp(-5.0, 5.0))
    } else {
        1.0
    };
    let contrast = if adjustments.contrast.is_finite() {
        adjustments.contrast.clamp(-1.0, 1.0) + 1.0
    } else {
        1.0
    };
    let saturation = if adjustments.saturation.is_finite() {
        adjustments.saturation.clamp(-1.0, 1.0) + 1.0
    } else {
        1.0
    };
    let hue = if adjustments.hue_degrees.is_finite() {
        adjustments.hue_degrees.to_radians()
    } else {
        0.0
    };
    let temperature = if adjustments.temperature.is_finite() {
        adjustments.temperature.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let (hue_sin, hue_cos) = hue.sin_cos();
    for pixel in rgba8.chunks_exact_mut(4) {
        let mut rgb = [
            f32::from(pixel[0]) / 255.0,
            f32::from(pixel[1]) / 255.0,
            f32::from(pixel[2]) / 255.0,
        ];
        rgb[0] += temperature * 0.12;
        rgb[1] += temperature * 0.02;
        rgb[2] -= temperature * 0.12;
        let luminance = rgb[0].mul_add(0.2126, rgb[1].mul_add(0.7152, rgb[2] * 0.0722));
        for channel in &mut rgb {
            *channel = luminance + (*channel - luminance) * saturation;
        }
        if hue != 0.0 {
            let axis_dot = (rgb[0] + rgb[1] + rgb[2]) / 3.0;
            let cross = [
                (rgb[2] - rgb[1]) * 0.577_350_26,
                (rgb[0] - rgb[2]) * 0.577_350_26,
                (rgb[1] - rgb[0]) * 0.577_350_26,
            ];
            for channel in 0..3 {
                rgb[channel] =
                    rgb[channel] * hue_cos + cross[channel] * hue_sin + axis_dot * (1.0 - hue_cos);
            }
        }
        for (target, value) in pixel[..3].iter_mut().zip(rgb) {
            let adjusted = ((value * exposure - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
            *target = (adjusted * 255.0).round() as u8;
        }
    }
}

pub fn composite_rgba(
    base: &mut [u8],
    layer: &[u8],
    opacity: f32,
    mode: TextureBlendMode,
) -> Result<(), TextureBakeError> {
    if base.len() != layer.len() || !base.len().is_multiple_of(4) {
        return Err(TextureBakeError::InvalidSource(
            "composite layers must have equal RGBA dimensions".to_owned(),
        ));
    }
    composite_rgba_impl(base, layer, opacity, mode, |_| 1.0);
    Ok(())
}

pub fn composite_rgba_masked(
    base: &mut [u8],
    layer: &[u8],
    output_width: u32,
    output_height: u32,
    mask: AlphaMaskView<'_>,
    opacity: f32,
    mode: TextureBlendMode,
) -> Result<(), TextureBakeError> {
    let expected = output_width as usize * output_height as usize * 4;
    if base.len() != expected || layer.len() != expected {
        return Err(TextureBakeError::InvalidSource(
            "masked composite dimensions do not match its RGBA layers".to_owned(),
        ));
    }
    if mask.width == output_width && mask.height == output_height {
        composite_rgba_impl(base, layer, opacity, mode, |index| {
            f32::from(mask.alpha8[index]) / 255.0
        });
    } else {
        composite_rgba_impl(base, layer, opacity, mode, |index| {
            mask.sample_scaled(index, output_width, output_height)
        });
    }
    Ok(())
}

fn composite_rgba_impl(
    base: &mut [u8],
    layer: &[u8],
    opacity: f32,
    mode: TextureBlendMode,
    mask: impl Fn(usize) -> f32 + Sync,
) {
    let opacity = if opacity.is_finite() {
        opacity.clamp(0.0, 1.0)
    } else {
        1.0
    };
    if base.len() >= 256 * 256 * 4 {
        base.par_chunks_exact_mut(4)
            .zip(layer.par_chunks_exact(4))
            .enumerate()
            .for_each(|(index, (base, layer))| {
                composite_rgba_pixel(base, layer, opacity, mode, mask(index))
            });
    } else {
        base.chunks_exact_mut(4)
            .zip(layer.chunks_exact(4))
            .enumerate()
            .for_each(|(index, (base, layer))| {
                composite_rgba_pixel(base, layer, opacity, mode, mask(index))
            });
    }
}

fn composite_rgba_pixel(
    base: &mut [u8],
    layer: &[u8],
    opacity: f32,
    mode: TextureBlendMode,
    mask: f32,
) {
    let source_alpha = f32::from(layer[3]) / 255.0 * opacity * mask.clamp(0.0, 1.0);
    if source_alpha <= 0.0 {
        return;
    }
    let destination_alpha = f32::from(base[3]) / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    for channel in 0..3 {
        let backdrop = f32::from(base[channel]) / 255.0;
        let source = f32::from(layer[channel]) / 255.0;
        let blended = match mode {
            TextureBlendMode::Normal => source,
            TextureBlendMode::Multiply => backdrop * source,
            TextureBlendMode::Screen => 1.0 - (1.0 - backdrop) * (1.0 - source),
            TextureBlendMode::Overlay => {
                if backdrop <= 0.5 {
                    2.0 * backdrop * source
                } else {
                    1.0 - 2.0 * (1.0 - backdrop) * (1.0 - source)
                }
            }
        };
        let premultiplied =
            blended * source_alpha + backdrop * destination_alpha * (1.0 - source_alpha);
        base[channel] = if output_alpha > 0.0 {
            (premultiplied / output_alpha * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        } else {
            0
        };
    }
    base[3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), TextureBakeError> {
    if width == 0 || height == 0 || width > MAX_BAKE_EDGE || height > MAX_BAKE_EDGE {
        Err(TextureBakeError::InvalidDimensions { width, height })
    } else {
        Ok(())
    }
}

fn obj_uv_to_top_origin(uv: [f32; 2]) -> [f32; 2] {
    [uv[0], 1.0 - uv[1]]
}

pub(crate) fn raster_face_triangle(
    output: &mut TextureBakeImage,
    target_uvs: [[f32; 2]; 3],
    source: RgbaView<'_>,
    source_uvs: [[f32; 2]; 3],
) {
    raster_triangle(
        output,
        target_uvs.map(obj_uv_to_top_origin),
        source,
        source_uvs.map(obj_uv_to_top_origin),
    );
}

fn raster_triangle(
    output: &mut TextureBakeImage,
    target: [[f32; 2]; 3],
    source: RgbaView<'_>,
    source_uvs: [[f32; 2]; 3],
) {
    if !target
        .iter()
        .flatten()
        .chain(source_uvs.iter().flatten())
        .all(|value| value.is_finite())
    {
        return;
    }
    let scale = [
        output.width.saturating_sub(1) as f32,
        output.height.saturating_sub(1) as f32,
    ];
    let pixels = target.map(|point| [point[0] * scale[0], point[1] * scale[1]]);
    let area = edge(pixels[0], pixels[1], pixels[2]);
    if area.abs() <= f32::EPSILON {
        return;
    }
    let min_x = pixels
        .iter()
        .map(|point| point[0])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0.0, scale[0]) as u32;
    let max_x = pixels
        .iter()
        .map(|point| point[0])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0.0, scale[0]) as u32;
    let min_y = pixels
        .iter()
        .map(|point| point[1])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0.0, scale[1]) as u32;
    let max_y = pixels
        .iter()
        .map(|point| point[1])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0.0, scale[1]) as u32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let point = [x as f32 + 0.5, y as f32 + 0.5];
            let barycentric = [
                edge(pixels[1], pixels[2], point) / area,
                edge(pixels[2], pixels[0], point) / area,
                edge(pixels[0], pixels[1], point) / area,
            ];
            if barycentric
                .iter()
                .any(|weight| *weight < BARYCENTRIC_EPSILON)
            {
                continue;
            }
            let source_point = [0, 1].map(|axis| {
                barycentric[0].mul_add(
                    source_uvs[0][axis],
                    barycentric[1]
                        .mul_add(source_uvs[1][axis], barycentric[2] * source_uvs[2][axis]),
                )
            });
            let sampled = bilinear_sample(source, source_point);
            let offset = (y as usize * output.width as usize + x as usize) * 4;
            output.rgba8[offset..offset + 4].copy_from_slice(&sampled);
        }
    }
}

fn raster_triangle_mask(mask: &mut [bool], width: u32, height: u32, target: [[f32; 2]; 3]) {
    if mask.len() != width as usize * height as usize
        || !target.iter().flatten().all(|value| value.is_finite())
    {
        return;
    }
    let scale = [
        width.saturating_sub(1) as f32,
        height.saturating_sub(1) as f32,
    ];
    let pixels = target.map(|point| [point[0] * scale[0], point[1] * scale[1]]);
    let area = edge(pixels[0], pixels[1], pixels[2]);
    if area.abs() <= f32::EPSILON {
        return;
    }
    let min_x = pixels
        .iter()
        .map(|point| point[0])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0.0, scale[0]) as u32;
    let max_x = pixels
        .iter()
        .map(|point| point[0])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0.0, scale[0]) as u32;
    let min_y = pixels
        .iter()
        .map(|point| point[1])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0.0, scale[1]) as u32;
    let max_y = pixels
        .iter()
        .map(|point| point[1])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0.0, scale[1]) as u32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let point = [x as f32 + 0.5, y as f32 + 0.5];
            let barycentric = [
                edge(pixels[1], pixels[2], point) / area,
                edge(pixels[2], pixels[0], point) / area,
                edge(pixels[0], pixels[1], point) / area,
            ];
            if barycentric
                .iter()
                .all(|weight| *weight >= BARYCENTRIC_EPSILON)
            {
                mask[y as usize * width as usize + x as usize] = true;
            }
        }
    }
}

fn edge(a: [f32; 2], b: [f32; 2], point: [f32; 2]) -> f32 {
    (point[0] - a[0]).mul_add(b[1] - a[1], -(point[1] - a[1]) * (b[0] - a[0]))
}

fn bilinear_sample(source: RgbaView<'_>, uv: [f32; 2]) -> [u8; 4] {
    let x = uv[0].clamp(0.0, 1.0) * source.width.saturating_sub(1) as f32;
    let y = uv[1].clamp(0.0, 1.0) * source.height.saturating_sub(1) as f32;
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(source.width as usize - 1);
    let y1 = (y0 + 1).min(source.height as usize - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let pixel = |x: usize, y: usize| {
        let offset = (y * source.width as usize + x) * 4;
        [
            source.rgba8[offset],
            source.rgba8[offset + 1],
            source.rgba8[offset + 2],
            source.rgba8[offset + 3],
        ]
    };
    let p00 = pixel(x0, y0);
    let p10 = pixel(x1, y0);
    let p01 = pixel(x0, y1);
    let p11 = pixel(x1, y1);
    std::array::from_fn(|channel| {
        let top =
            f32::from(p00[channel]) + (f32::from(p10[channel]) - f32::from(p00[channel])) * fx;
        let bottom =
            f32::from(p01[channel]) + (f32::from(p11[channel]) - f32::from(p01[channel])) * fx;
        (top + (bottom - top) * fy).round().clamp(0.0, 255.0) as u8
    })
}

fn feather_coverage_alpha(rgba8: &mut [u8], width: u32, height: u32, radius: u16) {
    if radius == 0 || rgba8.len() != width as usize * height as usize * 4 {
        return;
    }
    let width = width as usize;
    let height = height as usize;
    let maximum = u16::MAX / 2;
    let mut distance = rgba8
        .chunks_exact(4)
        .map(|pixel| if pixel[3] == 0 { 0 } else { maximum })
        .collect::<Vec<_>>();
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if distance[index] == 0 {
                continue;
            }
            let mut best = if x == 0 || y == 0 { 1 } else { distance[index] };
            if x > 0 {
                best = best.min(distance[index - 1].saturating_add(1));
            }
            if y > 0 {
                best = best.min(distance[index - width].saturating_add(1));
            }
            distance[index] = best;
        }
    }
    for y in (0..height).rev() {
        for x in (0..width).rev() {
            let index = y * width + x;
            if distance[index] == 0 {
                continue;
            }
            let mut best = if x + 1 == width || y + 1 == height {
                distance[index].min(1)
            } else {
                distance[index]
            };
            if x + 1 < width {
                best = best.min(distance[index + 1].saturating_add(1));
            }
            if y + 1 < height {
                best = best.min(distance[index + width].saturating_add(1));
            }
            distance[index] = best;
        }
    }

    let radius = u32::from(radius);
    let span = radius.max(2) - 1;
    for (pixel, distance) in rgba8.chunks_exact_mut(4).zip(distance) {
        let depth = u32::from(distance);
        if depth == 0 || depth >= radius {
            continue;
        }
        pixel[3] = ((u32::from(pixel[3]) * (depth - 1)) / span) as u8;
    }
}

fn squared_distance(left: [f32; 2], right: [f32; 2]) -> f32 {
    (left[0] - right[0]).mul_add(
        left[0] - right[0],
        (left[1] - right[1]) * (left[1] - right[1]),
    )
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Triangle([usize; 3]);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Edge(usize, usize);

impl Edge {
    fn new(left: usize, right: usize) -> Self {
        if left < right {
            Self(left, right)
        } else {
            Self(right, left)
        }
    }
}

fn delaunay_triangles(points: &[[f32; 2]]) -> Vec<[usize; 3]> {
    let mut work = points.to_vec();
    let super_start = work.len();
    work.extend([[-8.0, -4.0], [9.0, -4.0], [0.5, 9.0]]);
    let mut triangles = vec![Triangle([super_start, super_start + 1, super_start + 2])];
    for point_index in 0..points.len() {
        let mut boundary = HashMap::<Edge, usize>::new();
        let mut retained = Vec::with_capacity(triangles.len());
        for triangle in triangles {
            if circumcircle_contains(
                work[triangle.0[0]],
                work[triangle.0[1]],
                work[triangle.0[2]],
                work[point_index],
            ) {
                for edge in [
                    Edge::new(triangle.0[0], triangle.0[1]),
                    Edge::new(triangle.0[1], triangle.0[2]),
                    Edge::new(triangle.0[2], triangle.0[0]),
                ] {
                    *boundary.entry(edge).or_default() += 1;
                }
            } else {
                retained.push(triangle);
            }
        }
        retained.extend(boundary.into_iter().filter_map(|(edge, count)| {
            (count == 1).then_some(Triangle([edge.0, edge.1, point_index]))
        }));
        triangles = retained;
    }
    let mut result = triangles
        .into_iter()
        .filter(|triangle| triangle.0.iter().all(|index| *index < super_start))
        .filter(|triangle| {
            edge(
                work[triangle.0[0]],
                work[triangle.0[1]],
                work[triangle.0[2]],
            )
            .abs()
                > 1.0e-8
        })
        .map(|triangle| triangle.0)
        .collect::<Vec<_>>();
    result.sort_unstable();
    result
}

fn circumcircle_contains(a: [f32; 2], b: [f32; 2], c: [f32; 2], point: [f32; 2]) -> bool {
    let ax = a[0] - point[0];
    let ay = a[1] - point[1];
    let bx = b[0] - point[0];
    let by = b[1] - point[1];
    let cx = c[0] - point[0];
    let cy = c[1] - point[1];
    let determinant = (ax * ax + ay * ay) * (bx * cy - by * cx)
        - (bx * bx + by * by) * (ax * cy - ay * cx)
        + (cx * cx + cy * cy) * (ax * by - ay * bx);

    let orientation = edge(a, b, c);
    if orientation < 0.0 {
        determinant > 1.0e-8
    } else {
        determinant < -1.0e-8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
        rgba.repeat(width as usize * height as usize)
    }

    #[test]
    fn the_feathered_edge_reaches_full_transparency() {
        let (width, height) = (16_u32, 16_u32);
        let mut rgba = solid(width, height, [255, 255, 255, 0]);
        for y in 4..12 {
            for x in 4..12 {
                rgba[(y * width as usize + x) * 4 + 3] = 255;
            }
        }

        feather_coverage_alpha(&mut rgba, width, height, 3);

        let alpha = |x: usize, y: usize| rgba[(y * width as usize + x) * 4 + 3];
        assert_eq!(alpha(4, 4), 0, "the outermost covered ring is fully clear");
        assert_eq!(alpha(4, 7), 0);
        assert!(
            alpha(5, 7) > 0 && alpha(5, 7) < 255,
            "the ramp climbs inward"
        );
        assert_eq!(alpha(7, 7), 255, "past the feather depth it is untouched");
        assert_eq!(alpha(0, 0), 0, "uncovered pixels stay uncovered");
    }

    #[test]
    fn direct_uv_resize_preserves_a_solid_image() {
        let source = solid(2, 2, [20, 40, 60, 255]);
        let resized = resize_direct_uv(RgbaView::new(&source, 2, 2).unwrap(), 4, 4).unwrap();
        assert_eq!(resized.rgba8, solid(4, 4, [20, 40, 60, 255]));
    }

    #[test]
    fn normal_composite_respects_source_alpha_and_opacity() {
        let mut base = vec![0, 0, 0, 255];
        composite_rgba(
            &mut base,
            &[200, 100, 50, 128],
            0.5,
            TextureBlendMode::Normal,
        )
        .unwrap();
        assert!((49..=51).contains(&base[0]));
        assert!((24..=26).contains(&base[1]));
        assert!((12..=13).contains(&base[2]));
        assert_eq!(base[3], 255);
    }

    #[test]
    fn masked_composite_samples_layer_owned_visibility_without_rewriting_color_alpha() {
        let mut base = solid(2, 1, [0, 0, 0, 255]);
        let layer = solid(2, 1, [200, 100, 50, 255]);
        let mask = AlphaMaskView::new(&[0, 255], 2, 1).unwrap();
        composite_rgba_masked(&mut base, &layer, 2, 1, mask, 1.0, TextureBlendMode::Normal)
            .unwrap();
        assert_eq!(&base[..4], &[0, 0, 0, 255]);
        assert_eq!(&base[4..], &[200, 100, 50, 255]);
        assert_eq!(layer, solid(2, 1, [200, 100, 50, 255]));
    }

    #[test]
    fn masked_composite_scales_an_existing_mask_without_reallocating_it() {
        let mut base = solid(2, 2, [0, 0, 0, 255]);
        let layer = solid(2, 2, [200, 100, 50, 255]);
        composite_rgba_masked(
            &mut base,
            &layer,
            2,
            2,
            AlphaMaskView::new(&[128], 1, 1).unwrap(),
            1.0,
            TextureBlendMode::Normal,
        )
        .unwrap();
        for pixel in base.chunks_exact(4) {
            assert!((99..=101).contains(&pixel[0]));
            assert!((49..=51).contains(&pixel[1]));
            assert!((24..=26).contains(&pixel[2]));
            assert_eq!(pixel[3], 255);
        }
    }

    #[test]
    fn three_pin_warp_covers_only_the_target_triangle() {
        let source = solid(2, 2, [220, 40, 20, 255]);
        let pins = [
            TextureWarpPin {
                source: [0.0, 0.0],
                target_uv: [0.0, 1.0],
            },
            TextureWarpPin {
                source: [1.0, 0.0],
                target_uv: [1.0, 1.0],
            },
            TextureWarpPin {
                source: [0.0, 1.0],
                target_uv: [0.0, 0.0],
            },
        ];
        let warped = warp_image_to_g2_by_pins(
            RgbaView::new(&source, 2, 2).unwrap(),
            &pins,
            TextureBakeOptions {
                width: 8,
                height: 8,
                boundary_feather_pixels: 0,
            },
        )
        .unwrap();
        assert!(warped.rgba8.chunks_exact(4).any(|pixel| pixel[3] == 255));
        assert!(warped.rgba8.chunks_exact(4).any(|pixel| pixel[3] == 0));
    }

    #[test]
    fn region_warp_extrapolates_beyond_pins_without_leaving_the_region() {
        let source = solid(2, 2, [220, 40, 20, 255]);
        let pins = [
            TextureWarpPin {
                source: [0.0, 0.0],
                target_uv: [0.35, 0.65],
            },
            TextureWarpPin {
                source: [1.0, 0.0],
                target_uv: [0.65, 0.65],
            },
            TextureWarpPin {
                source: [0.5, 1.0],
                target_uv: [0.5, 0.35],
            },
        ];
        let region = [[[0.1, 0.9], [0.9, 0.9], [0.5, 0.1]]];
        let warped = warp_image_to_g2_by_pins_in_region(
            RgbaView::new(&source, 2, 2).unwrap(),
            &pins,
            TextureBakeOptions {
                width: 32,
                height: 32,
                boundary_feather_pixels: 0,
            },
            &region,
        )
        .unwrap();
        let alpha_at = |x: usize, y: usize| warped.rgba8[(y * 32 + x) * 4 + 3];
        assert_eq!(alpha_at(16, 4), 255, "region above the pins is extended");
        assert_eq!(alpha_at(0, 0), 0, "pixels outside the region stay clear");
    }

    #[test]
    fn source_point_mapping_matches_warp_and_extrapolates_past_pin_hull() {
        let pins = [
            TextureWarpPin {
                source: [0.0, 0.0],
                target_uv: [0.2, 0.8],
            },
            TextureWarpPin {
                source: [1.0, 0.0],
                target_uv: [0.8, 0.8],
            },
            TextureWarpPin {
                source: [0.0, 1.0],
                target_uv: [0.2, 0.2],
            },
        ];
        let mapped = map_source_point_to_g2(&pins, [0.25, 0.25])
            .unwrap()
            .unwrap();
        assert!((mapped[0] - 0.35).abs() < 1.0e-5);
        assert!((mapped[1] - 0.65).abs() < 1.0e-5);
        let extrapolated = map_source_point_to_g2(&pins, [0.9, 0.9]).unwrap().unwrap();
        assert!((extrapolated[0] - 0.74).abs() < 1.0e-5);
        assert!((extrapolated[1] - 0.26).abs() < 1.0e-5);
    }

    #[test]
    fn g2_to_source_mapping_matches_warp_and_extrapolates_past_pin_hull() {
        let pins = [
            TextureWarpPin {
                source: [0.0, 0.0],
                target_uv: [0.2, 0.8],
            },
            TextureWarpPin {
                source: [1.0, 0.0],
                target_uv: [0.8, 0.8],
            },
            TextureWarpPin {
                source: [0.0, 1.0],
                target_uv: [0.2, 0.2],
            },
        ];
        let mapped = map_g2_point_to_source(&pins, [0.35, 0.65])
            .unwrap()
            .unwrap();
        assert!((mapped[0] - 0.25).abs() < 1.0e-5);
        assert!((mapped[1] - 0.25).abs() < 1.0e-5);

        let extrapolated = map_g2_point_to_source(&pins, [0.95, 0.05])
            .unwrap()
            .unwrap();
        assert_eq!(extrapolated, [1.0, 1.0]);
    }

    #[test]
    fn warp_rejects_duplicate_target_pins() {
        let source = solid(1, 1, [255; 4]);
        let pins = [TextureWarpPin {
            source: [0.0, 0.0],
            target_uv: [0.5, 0.5],
        }; 3];
        assert_eq!(
            warp_image_to_g2_by_pins(
                RgbaView::new(&source, 1, 1).unwrap(),
                &pins,
                TextureBakeOptions {
                    width: 16,
                    height: 16,
                    boundary_feather_pixels: 0,
                },
            ),
            Err(TextureBakeError::DuplicateWarpTarget)
        );
    }
}

#[cfg(test)]
mod projection_painting {
    use super::*;

    fn full_quad() -> [ProjectedTriangle; 2] {
        [
            ProjectedTriangle {
                screen: [[0.0, 0.0], [100.0, 0.0], [0.0, 100.0]],
                uv: [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0]],
            },
            ProjectedTriangle {
                screen: [[100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
                uv: [[1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            },
        ]
    }

    #[test]
    fn a_stamp_paints_under_the_brush_and_builds_up() {
        let pixels = [255_u8, 0, 0, 255].repeat(16);
        let source = RgbaView::new(&pixels, 4, 4).expect("a red square");
        let mut target = TextureBakeImage::transparent(64, 64).expect("an atlas");
        let brush = ProjectionBrush {
            centre: [50.0, 50.0],
            radius: 20.0,
            falloff: crate::sculpt::SculptFalloff::Smooth,
            opacity: 1.0,
        };

        let to_source = |screen: [f32; 2]| Some([screen[0] / 100.0, screen[1] / 100.0]);

        let painted = stamp_projection_onto_g2(
            &mut target.rgba8,
            64,
            64,
            source,
            &full_quad(),
            brush,
            to_source,
        );
        assert!(painted > 0, "nothing was painted");

        let alpha_at =
            |image: &TextureBakeImage, x: usize, y: usize| image.rgba8[(y * 64 + x) * 4 + 3];

        let centre_alpha = alpha_at(&target, 32, 32);
        assert!(centre_alpha >= 240, "the brush centre: {centre_alpha}");
        assert_eq!(alpha_at(&target, 2, 2), 0, "a corner outside the brush");
        assert_eq!(alpha_at(&target, 61, 61), 0, "the far corner");

        let covered = (0..64 * 64)
            .filter(|index| target.rgba8[index * 4 + 3] > 0)
            .count() as f32;
        let expected = std::f32::consts::PI * (0.2 * 64.0_f32).powi(2);
        assert!(
            (covered - expected).abs() < expected * 0.2,
            "covered {covered}, expected about {expected}"
        );

        stamp_projection_onto_g2(
            &mut target.rgba8,
            64,
            64,
            source,
            &full_quad(),
            ProjectionBrush {
                opacity: 0.5,
                ..brush
            },
            to_source,
        );
        assert!(
            alpha_at(&target, 32, 32) >= centre_alpha,
            "the second pass erased the first"
        );
    }

    #[test]
    fn the_soft_rim_keeps_the_paint_colour() {
        let pixels = [255_u8, 0, 0, 255].repeat(16);
        let source = RgbaView::new(&pixels, 4, 4).expect("a red square");
        let mut target = TextureBakeImage::transparent(64, 64).expect("an atlas");
        stamp_projection_onto_g2(
            &mut target.rgba8,
            64,
            64,
            source,
            &full_quad(),
            ProjectionBrush {
                centre: [50.0, 50.0],
                radius: 20.0,
                falloff: crate::sculpt::SculptFalloff::Smooth,
                opacity: 1.0,
            },
            |screen| Some([screen[0] / 100.0, screen[1] / 100.0]),
        );
        let mut soft_texels = 0;
        for index in 0..64 * 64 {
            let texel = &target.rgba8[index * 4..index * 4 + 4];
            let alpha = texel[3];
            if alpha == 0 || alpha == 255 {
                continue;
            }
            soft_texels += 1;
            assert!(
                texel[0] >= 250 && texel[1] <= 5 && texel[2] <= 5,
                "a rim texel at alpha {alpha} drifted off red: {texel:?}"
            );
        }
        assert!(soft_texels > 0, "the smooth falloff produced no soft rim");
    }

    #[test]
    fn a_stamp_writes_nothing_off_the_stencil() {
        let pixels = [0_u8, 255, 0, 255].repeat(4);
        let source = RgbaView::new(&pixels, 2, 2).expect("a green square");
        let mut target = TextureBakeImage::transparent(32, 32).expect("an atlas");
        let painted = stamp_projection_onto_g2(
            &mut target.rgba8,
            32,
            32,
            source,
            &full_quad(),
            ProjectionBrush {
                centre: [10.0, 10.0],
                radius: 30.0,
                falloff: crate::sculpt::SculptFalloff::Smooth,
                opacity: 1.0,
            },
            |_| None,
        );
        assert_eq!(painted, 0);
        assert!(target.rgba8.iter().all(|value| *value == 0));
    }
}

#[cfg(test)]
mod g2_uv_placement {
    use super::*;

    #[test]
    fn a_g2_uv_image_lands_where_it_already_was() {
        let source = [
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [255, 255, 0, 255],
        ]
        .concat();
        let view = RgbaView::new(&source, 2, 2).expect("a 2x2 source");

        let region = [
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
            [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        ];
        let options = TextureBakeOptions {
            width: 64,
            height: 64,
            boundary_feather_pixels: 0,
        };

        let placed = place_image_on_g2_uv_in_region(view, options, &region).expect("it places");

        let texel = |x: u32, y: u32| {
            let offset = (y as usize * 64 + x as usize) * 4;
            [
                placed.rgba8[offset],
                placed.rgba8[offset + 1],
                placed.rgba8[offset + 2],
                placed.rgba8[offset + 3],
            ]
        };

        let dominant = |pixel: [u8; 4]| pixel.map(|channel| channel > 128);
        assert_eq!(
            dominant(texel(1, 1)),
            [true, false, false, true],
            "top left"
        );
        assert_eq!(
            dominant(texel(62, 1)),
            [false, true, false, true],
            "top right"
        );
        assert_eq!(
            dominant(texel(1, 62)),
            [false, false, true, true],
            "bottom left"
        );
        assert_eq!(
            dominant(texel(62, 62)),
            [true, true, false, true],
            "bottom right"
        );
    }

    #[test]
    fn pixels_outside_the_region_stay_transparent() {
        let source = [[9_u8, 9, 9, 255]; 4].concat();
        let view = RgbaView::new(&source, 2, 2).expect("a 2x2 source");

        let region = [[[0.0, 0.0], [0.25, 0.0], [0.0, 0.25]]];
        let options = TextureBakeOptions {
            width: 32,
            height: 32,
            boundary_feather_pixels: 0,
        };

        let placed = place_image_on_g2_uv_in_region(view, options, &region).expect("it places");

        let alpha = |x: u32, y: u32| placed.rgba8[(y as usize * 32 + x as usize) * 4 + 3];
        assert_eq!(alpha(1, 30), 255, "inside the triangle");
        assert_eq!(alpha(16, 16), 0, "outside it");
        assert_eq!(alpha(31, 0), 0, "the far corner");
    }
}
