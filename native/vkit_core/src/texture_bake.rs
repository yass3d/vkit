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
    feather_coverage_alpha(
        &mut output.rgba8,
        output.width,
        output.height,
        options.boundary_feather_pixels,
    );
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

    /// Take the stamped contribution back instead of laying more of it down.
    pub erase: bool,
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
                if brush.erase {
                    if erase_alpha(target_rgba8, target_width, x, y, weight * brush.opacity) {
                        painted += 1;
                    }
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

/// The inverse of [`blend_over`]: hold the colour and take the alpha back, so a
/// soft brush lifts paint with the same profile it laid it down with. Reports
/// whether anything actually moved, which lets a stroke over already-clear
/// texels report no work and skip the re-bake.
fn erase_alpha(target: &mut [u8], width: u32, x: u32, y: u32, coverage: f32) -> bool {
    let index = (y as usize * width as usize + x as usize) * 4;
    let Some(slot) = target.get_mut(index..index + 4) else {
        return false;
    };
    if slot[3] == 0 {
        return false;
    }
    let coverage = coverage.clamp(0.0, 1.0);
    if coverage <= 0.0 {
        return false;
    }
    let remaining = (f32::from(slot[3]) * (1.0 - coverage))
        .round()
        .clamp(0.0, 255.0) as u8;
    if remaining == slot[3] {
        return false;
    }
    slot[3] = remaining;
    true
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

/// The single uniform-scale, rotation and translation that best carries the
/// target pins onto the source pins.
///
/// Three pins fix a full affine exactly, and an affine is free to shear, to
/// scale one axis and not the other, and to turn itself inside out. Fixed
/// exactly by three points and then extended across a whole face, that is what
/// made a first placement look mangled. A similarity has four numbers rather
/// than six and cannot do any of those things: it can only move the picture,
/// turn it, and make it bigger or smaller as a whole.
#[derive(Clone, Copy, Debug)]
struct WarpSimilarity {
    /// Centre of the target pins, which the map pivots about.
    target_centre: [f32; 2],
    /// Centre of the source pins, where that pivot lands.
    source_centre: [f32; 2],
    /// The rotation-and-scale as `[c·cos, c·sin]`.
    turn: [f32; 2],
    /// How far the pins reach from their centre — the distance over which the
    /// warp gives way to this.
    spread: f32,
}

impl WarpSimilarity {
    fn fit(pins: &[TextureWarpPin], target_points: &[[f32; 2]]) -> Option<Self> {
        let count = target_points.len();
        if count < 2 || pins.len() != count {
            return None;
        }
        let mean = |points: &dyn Fn(usize) -> [f32; 2]| {
            let sum = (0..count).fold([0.0, 0.0], |sum: [f32; 2], index| {
                let point = points(index);
                [sum[0] + point[0], sum[1] + point[1]]
            });
            #[expect(clippy::cast_precision_loss, reason = "a handful of pins")]
            let divisor = count as f32;
            [sum[0] / divisor, sum[1] / divisor]
        };
        let target_centre = mean(&|index| target_points[index]);
        let source_centre = mean(&|index| pins[index].source);

        // Least squares over c·cos and c·sin at once: the two normal equations
        // share the denominator, so one pass over the pins answers both.
        let (mut along, mut across, mut extent) = (0.0f32, 0.0f32, 0.0f32);
        for index in 0..count {
            let target = [
                target_points[index][0] - target_centre[0],
                target_points[index][1] - target_centre[1],
            ];
            let source = [
                pins[index].source[0] - source_centre[0],
                pins[index].source[1] - source_centre[1],
            ];
            along += target[0].mul_add(source[0], target[1] * source[1]);
            across += target[0].mul_add(source[1], -(target[1] * source[0]));
            extent += target[0].mul_add(target[0], target[1] * target[1]);
        }
        if !extent.is_finite() || extent <= 1.0e-12 {
            // Every pin in one spot: nothing to take a bearing from.
            return None;
        }
        #[expect(clippy::cast_precision_loss, reason = "a handful of pins")]
        let spread = (extent / count as f32).sqrt();
        Some(Self {
            target_centre,
            source_centre,
            turn: [along / extent, across / extent],
            spread,
        })
    }

    /// The same map read the other way, for going source-to-target.
    fn inverted(self) -> Option<Self> {
        let scale_squared = self.turn[0].mul_add(self.turn[0], self.turn[1] * self.turn[1]);
        if !scale_squared.is_finite() || scale_squared <= 1.0e-16 {
            return None;
        }
        Some(Self {
            target_centre: self.source_centre,
            source_centre: self.target_centre,
            turn: [self.turn[0] / scale_squared, -self.turn[1] / scale_squared],
            // Distances are measured among the source pins now, and the map
            // between the two spaces stretches them by its own scale.
            spread: self.spread * scale_squared.sqrt(),
        })
    }

    /// Eases from what the nearest triangle says to what this says.
    ///
    /// One method rather than three copies: the picture, the click that asks
    /// where on the photo a spot on the face came from, and the click that asks
    /// the reverse all have to agree, or the image is drawn one way and the
    /// pointer answers about another.
    fn hand_over(&self, carried_on: [f32; 2], point: [f32; 2], distance: f32) -> [f32; 2] {
        let settled = self.apply(point);
        let handover = warp_handover(distance, self.spread);
        [0, 1].map(|axis| (settled[axis] - carried_on[axis]).mul_add(handover, carried_on[axis]))
    }

    fn apply(&self, point: [f32; 2]) -> [f32; 2] {
        let offset = [
            point[0] - self.target_centre[0],
            point[1] - self.target_centre[1],
        ];
        [
            self.turn[0].mul_add(offset[0], -(self.turn[1] * offset[1])) + self.source_centre[0],
            self.turn[1].mul_add(offset[0], self.turn[0] * offset[1]) + self.source_centre[1],
        ]
    }
}

/// How far past the pins the triangles still have the last word, as a multiple
/// of the pin spread. Beyond that the similarity has it entirely.
const WARP_HANDOVER_SPREADS: f32 = 0.75;

/// Smooth 0-to-1 over the handover band, so nothing changes abruptly.
fn warp_handover(distance: f32, spread: f32) -> f32 {
    let reach = spread * WARP_HANDOVER_SPREADS;
    if !reach.is_finite() || reach <= 1.0e-9 {
        return 1.0;
    }
    let t = (distance / reach).clamp(0.0, 1.0);
    t * t * 2.0f32.mul_add(-t, 3.0)
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
    let similarity = WarpSimilarity::fit(pins, target_points);
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
        let reach = squared_distance_to_segment(
            point,
            target_points[boundary.start],
            target_points[boundary.end],
        )
        .max(0.0)
        .sqrt();
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
        let carried_on = [0, 1].map(|axis| {
            barycentric[0].mul_add(
                source_uvs[0][axis],
                barycentric[1].mul_add(source_uvs[1][axis], barycentric[2] * source_uvs[2][axis]),
            )
        });
        // At the edge of the pins the triangle still decides, so nothing jumps
        // where the two meet. Further out its opinion is worth less and less —
        // an affine held that far from the points that fixed it is where the
        // stretching turns into mangling — until only the similarity is left.
        let source_point = similarity.map_or(carried_on, |similarity| {
            similarity.hand_over(carried_on, point, reach)
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
    let distance = squared_distance_to_segment(
        source_point,
        pins[boundary.start].source,
        pins[boundary.end].source,
    )
    .max(0.0)
    .sqrt();
    let similarity = WarpSimilarity::fit(pins, &target_points).and_then(WarpSimilarity::inverted);
    Ok(map_triangle(boundary.triangle).map(|(_, target)| {
        let eased = similarity.map_or(target, |similarity| {
            similarity.hand_over(target, source_point, distance)
        });
        [eased[0].clamp(0.0, 1.0), (1.0 - eased[1]).clamp(0.0, 1.0)]
    }))
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
    let distance = squared_distance_to_segment(
        point,
        target_points[boundary.start],
        target_points[boundary.end],
    )
    .max(0.0)
    .sqrt();
    let similarity = WarpSimilarity::fit(pins, &target_points);
    Ok(map_triangle(boundary.triangle).map(|(_, source)| {
        let eased = similarity.map_or(source, |similarity| {
            similarity.hand_over(source, point, distance)
        });
        eased.map(|value| value.clamp(0.0, 1.0))
    }))
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

/// Where highlights stop rising in step and start easing towards white.
const HIGHLIGHT_KNEE: f32 = 0.8;

/// Bends the top of the range so brightening cannot flatten a face to white.
///
/// A skin tone sits around 0.7, so multiplying it by anything much over 1.4
/// used to put a channel past 1.0, where it was cut off. Red goes first, then
/// green, and the picture loses its colour on the way to white — which is what
/// half a stop of exposure did to a face over a plain base, where there is no
/// texture left to disguise it.
///
/// Below the knee this is the identity, so an adjustment someone dialled in by
/// eye, and the tone match that solves for one, both behave as they did. Above
/// it the remaining headroom is eased into rather than spent, and the curve
/// meets the straight part with the same slope so there is no visible corner.
fn shoulder(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    if value <= HIGHLIGHT_KNEE {
        return value.max(0.0);
    }
    let headroom = 1.0 - HIGHLIGHT_KNEE;
    headroom.mul_add(-(-(value - HIGHLIGHT_KNEE) / headroom).exp(), 1.0)
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
        if pixel[3] == 0 {
            // Nothing is there. Adjusting the colour of a hole gives it one,
            // and anything downstream that reads colour before alpha then has
            // a tint to leak. Lowering contrast alone used to lift a clear
            // pixel to mid grey.
            continue;
        }
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
            let lifted = shoulder(value * exposure);
            let adjusted = ((lifted - 0.5) * contrast + 0.5).clamp(0.0, 1.0);
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
        // A blend mode reads the backdrop, and where the backdrop is
        // transparent there is nothing to read — its stored channels are
        // arbitrary, and here they are black. Fading the blend toward plain
        // source paint by how much backdrop actually exists is what every
        // layer editor does; without it, Multiply over an empty base is a
        // multiplication by zero.
        let blended = source + (blended - source) * destination_alpha;
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

/// Marks the transparent pixels that can be reached from the edge of the image.
///
/// A hole punched through the middle of the coverage -- an eye socket, a
/// nostril, the mouth -- is transparent in exactly the same way as the space
/// around the face. What separates them is reachability: the outside touches
/// the border and a hole does not.
fn transparent_region_touching_the_border(rgba8: &[u8], width: usize, height: usize) -> Vec<bool> {
    let transparent = |index: usize| rgba8[index * 4 + 3] == 0;
    let mut outside = vec![false; width * height];
    // An explicit stack rather than recursion: an atlas is millions of pixels
    // and a transparent border would otherwise be millions of frames deep.
    let mut pending = Vec::new();
    let push = |index: usize, outside: &mut Vec<bool>, pending: &mut Vec<usize>| {
        if !outside[index] && transparent(index) {
            outside[index] = true;
            pending.push(index);
        }
    };
    for x in 0..width {
        push(x, &mut outside, &mut pending);
        push((height - 1) * width + x, &mut outside, &mut pending);
    }
    for y in 0..height {
        push(y * width, &mut outside, &mut pending);
        push(y * width + width - 1, &mut outside, &mut pending);
    }
    while let Some(index) = pending.pop() {
        let (x, y) = (index % width, index / width);
        if x > 0 {
            push(index - 1, &mut outside, &mut pending);
        }
        if x + 1 < width {
            push(index + 1, &mut outside, &mut pending);
        }
        if y > 0 {
            push(index - width, &mut outside, &mut pending);
        }
        if y + 1 < height {
            push(index + width, &mut outside, &mut pending);
        }
    }
    outside
}

pub fn feather_coverage_alpha(rgba8: &mut [u8], width: u32, height: u32, radius: u16) {
    if radius == 0 || rgba8.len() != width as usize * height as usize * 4 {
        return;
    }
    let width = width as usize;
    let height = height as usize;
    let maximum = u16::MAX / 2;
    // Seed only from the transparent region that reaches the edge of the atlas.
    //
    // Seeding from every transparent pixel is what made the eye sockets, the
    // nostrils and the mouth bleed: those are holes *inside* the face, and a
    // distance transform cannot tell them from the space outside it. Both are
    // alpha zero. The difference is that only the outside touches the border,
    // so a flood fill from the border says which is which -- and the softness
    // then does what its name promises, which is soften the face boundary.
    let outside = transparent_region_touching_the_border(rgba8, width, height);
    let mut distance = outside
        .iter()
        .map(|&outside| if outside { 0 } else { maximum })
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
    fn a_hole_in_the_face_is_not_an_edge_of_it() {
        // A face in UV space is a covered region with holes punched through it
        // for the eyes, the nostrils and the mouth. Every one of those holes is
        // transparent in exactly the same way as the space around the face, so
        // a feather that seeds from "anything transparent" eats outward from
        // each of them -- which is what made softening look wrong around the
        // eyes while looking right along the jaw.
        let (width, height) = (24_u32, 24_u32);
        let mut rgba = solid(width, height, [255, 255, 255, 0]);
        let at = |x: usize, y: usize| (y * width as usize + x) * 4 + 3;
        for y in 4..20 {
            for x in 4..20 {
                rgba[at(x, y)] = 255;
            }
        }
        // The socket: a hole with no path to the border.
        for y in 10..14 {
            for x in 10..14 {
                rgba[at(x, y)] = 0;
            }
        }

        feather_coverage_alpha(&mut rgba, width, height, 4);

        let alpha = |x: usize, y: usize| rgba[at(x, y)];
        assert_eq!(alpha(4, 12), 0, "the outer edge still softens");
        assert!(
            alpha(5, 12) > 0 && alpha(5, 12) < 255,
            "the ramp still climbs inward from the outer edge"
        );

        // x 8 and 9 are the discriminating pair: five and six pixels inside the
        // outer edge, so beyond its radius-4 reach, and one and two pixels from
        // the socket, so squarely inside a feather the socket would cast. They
        // read 255 only if the socket is not treated as an edge.
        for x in 8..10 {
            assert_eq!(
                alpha(x, 12),
                255,
                "the socket bled outward and washed out ({x}, 12)"
            );
        }
        for y in 8..10 {
            assert_eq!(alpha(12, y), 255, "the socket bled upward at (12, {y})");
        }
        assert_eq!(alpha(10, 12), 0, "the hole itself stays a hole");
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
    fn a_blend_mode_over_a_transparent_base_degrades_to_normal_instead_of_black() {
        let colour = [200_u8, 100, 50, 255];
        for mode in [
            TextureBlendMode::Multiply,
            TextureBlendMode::Screen,
            TextureBlendMode::Overlay,
        ] {
            let mut blended = solid(1, 1, [0, 0, 0, 0]);
            composite_rgba(&mut blended, &colour, 1.0, mode).unwrap();
            let mut normal = solid(1, 1, [0, 0, 0, 0]);
            composite_rgba(&mut normal, &colour, 1.0, TextureBlendMode::Normal).unwrap();
            assert_eq!(
                blended, normal,
                "{mode:?} over nothing has no backdrop to blend with"
            );

            let mut blended = solid(1, 1, [80, 80, 80, 255]);
            composite_rgba(&mut blended, &colour, 1.0, mode).unwrap();
            let mut normal = solid(1, 1, [80, 80, 80, 255]);
            composite_rgba(&mut normal, &colour, 1.0, TextureBlendMode::Normal).unwrap();
            assert_ne!(
                blended, normal,
                "{mode:?} over an opaque backdrop must still blend"
            );
        }
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

    /// Three pins the way they actually get placed, and the way that hurts:
    /// nearly in a line across the photo — two eyes and a nose tip often are —
    /// against a target triangle with real height. The affine that fixes those
    /// exactly has to squash one direction almost flat, and it then applies
    /// that squashing to the whole face. Both triangles clear the readiness
    /// gate, so this reaches the warp in the ordinary way.
    fn skewed_pins() -> [TextureWarpPin; 3] {
        [
            TextureWarpPin {
                source: [0.30, 0.500],
                target_uv: [0.35, 0.60],
            },
            TextureWarpPin {
                source: [0.60, 0.505],
                target_uv: [0.65, 0.60],
            },
            TextureWarpPin {
                source: [0.45, 0.510],
                target_uv: [0.50, 0.35],
            },
        ]
    }

    #[test]
    fn far_from_the_pins_the_picture_settles_into_an_even_scale() {
        // The complaint this answers: with three or four pins the image arrived
        // mangled. An affine fixed by three points and then carried across a
        // whole face shears without limit; a similarity cannot shear at all.
        // Measured as the spread of the step taken for the same step on the
        // face, in two directions — even scaling keeps them equal.
        let pins = skewed_pins();
        let sample = |uv: [f32; 2]| {
            map_g2_point_to_source(&pins, uv)
                .expect("three valid pins")
                .expect("a point in the plane maps somewhere")
        };
        let far = [0.15, 0.15];
        let step = 0.02;
        let along = sample([far[0] + step, far[1]]);
        let across = sample([far[0], far[1] + step]);
        let here = sample(far);
        let length =
            |a: [f32; 2], b: [f32; 2]| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
        let (dx, dy) = (length(along, here), length(across, here));
        assert!(dx > 0.0 && dy > 0.0, "the map collapsed: {dx} by {dy}");
        let anisotropy = dx.max(dy) / dx.min(dy);
        assert!(
            anisotropy < 1.1,
            "one direction is stretched {anisotropy:.2}x more than the other. Without the              handover this fixture measures 33x, which is the mangling it exists to stop"
        );
    }

    #[test]
    fn the_pins_themselves_still_land_exactly_where_they_were_put() {
        // Easing outward must not move what the reader placed by hand.
        let pins = skewed_pins();
        for pin in pins {
            let round_trip = map_g2_point_to_source(&pins, pin.target_uv)
                .expect("three valid pins")
                .expect("a pin maps to its own source");
            for axis in 0..2 {
                assert!(
                    (round_trip[axis] - pin.source[axis]).abs() < 1.0e-3,
                    "pin moved: {round_trip:?} is not {:?}",
                    pin.source
                );
            }
        }
    }

    #[test]
    fn the_two_point_mappers_agree_out_where_the_similarity_has_taken_over() {
        // The picture, and the two directions of asking about a point, all read
        // the same map. Far outside the pins is where they used to differ.
        let pins = skewed_pins();
        let outside = [0.18, 0.20];
        let source = map_g2_point_to_source(&pins, outside)
            .expect("three valid pins")
            .expect("maps somewhere");
        let back = map_source_point_to_g2(&pins, source)
            .expect("three valid pins")
            .expect("maps back");
        for axis in 0..2 {
            assert!(
                (back[axis] - outside[axis]).abs() < 0.05,
                "a round trip through both mappers drifted: {back:?} from {outside:?}"
            );
        }
    }

    #[test]
    fn the_handover_starts_at_the_pins_and_finishes_beyond_them() {
        assert!(
            (warp_handover(0.0, 0.2) - 0.0).abs() < 1.0e-6,
            "no jump at the edge"
        );
        assert!(
            (warp_handover(10.0, 0.2) - 1.0).abs() < 1.0e-6,
            "settled far out"
        );
        let midway = warp_handover(0.2 * WARP_HANDOVER_SPREADS * 0.5, 0.2);
        assert!(
            (0.4..0.6).contains(&midway),
            "the crossover is not centred: {midway}"
        );
        assert_eq!(
            warp_handover(1.0, 0.0),
            1.0,
            "no spread means nothing to ease from"
        );
    }

    #[test]
    fn brightening_a_face_keeps_it_a_face() {
        // Over a plain base there is no texture to hide a blown highlight, so
        // the whole head went white and lost its colour on the way. A skin tone
        // sits near 0.7 of the range, which used to clip at half a stop.
        // A full stop. Half a stop was enough to pin red against the ceiling
        // before, so this is the headroom that was won; past a stop and a half
        // eight bits genuinely run out, and a picture is meant to go white
        // eventually.
        let skin = [180_u8, 140, 120, 255];
        for stops in [0.25_f32, 0.5, 0.75, 1.0] {
            let mut pixels = skin.to_vec();
            apply_color_adjustments(
                &mut pixels,
                TextureColorAdjustments {
                    exposure: stops,
                    ..TextureColorAdjustments::default()
                },
            );
            assert!(
                pixels[..3].iter().all(|channel| *channel < 255),
                "{stops} stops flattened a channel against the ceiling: {pixels:?}"
            );
            assert!(
                pixels[0] > pixels[1] && pixels[1] > pixels[2],
                "{stops} stops lost the order of the channels, so the tone changed: {pixels:?}"
            );
        }
    }

    #[test]
    fn brightening_never_stops_making_a_difference() {
        // A hard ceiling does not only look wrong, it destroys detail: two
        // different tones come out the same. The shoulder has to stay strictly
        // increasing so what was lighter stays lighter.
        // Over the range a slider can reach with a real tone in it. Far beyond
        // this the curve does land on 1.0, because a float near one has no room
        // left to be under it — but nothing gets there without being asked.
        let mut previous = -1.0;
        for step in 0..=20_u8 {
            let value = f32::from(step) * 0.1;
            let lifted = shoulder(value);
            assert!(
                lifted > previous,
                "the curve stopped rising at {value}, so two tones now read alike"
            );
            assert!(lifted < 1.0, "the curve reached the ceiling at {value}");
            previous = lifted;
        }
        // And below the knee it is the identity, so a dialled-in adjustment and
        // the tone match that solves for one both behave as they did.
        for value in [0.0_f32, 0.25, 0.5, 0.7, HIGHLIGHT_KNEE] {
            assert!((shoulder(value) - value).abs() < 1.0e-6, "moved {value}");
        }
    }

    #[test]
    fn adjusting_an_empty_pixel_leaves_it_empty() {
        // Lowering contrast used to lift a fully clear pixel to mid grey. The
        // alpha stayed zero, so it depended entirely on nothing downstream ever
        // reading colour before alpha.
        let mut clear = vec![0_u8, 0, 0, 0];
        apply_color_adjustments(
            &mut clear,
            TextureColorAdjustments {
                contrast: -0.5,
                exposure: 1.0,
                temperature: 0.8,
                ..TextureColorAdjustments::default()
            },
        );
        assert_eq!(clear, vec![0, 0, 0, 0]);
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
            erase: false,
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
                erase: false,
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
                erase: false,
            },
            |_| None,
        );
        assert_eq!(painted, 0);
        assert!(target.rgba8.iter().all(|value| *value == 0));
    }

    fn red_stamp_brush() -> ProjectionBrush {
        ProjectionBrush {
            centre: [50.0, 50.0],
            radius: 20.0,
            falloff: crate::sculpt::SculptFalloff::Smooth,
            opacity: 1.0,
            erase: false,
        }
    }

    #[test]
    fn holding_the_eraser_takes_back_what_the_stamp_laid_down() {
        let pixels = [255_u8, 0, 0, 255].repeat(16);
        let source = RgbaView::new(&pixels, 4, 4).expect("a red square");
        let mut target = TextureBakeImage::transparent(64, 64).expect("an atlas");
        let brush = red_stamp_brush();
        let to_source = |screen: [f32; 2]| Some([screen[0] / 100.0, screen[1] / 100.0]);
        stamp_projection_onto_g2(
            &mut target.rgba8,
            64,
            64,
            source,
            &full_quad(),
            brush,
            to_source,
        );
        let alpha_at = |image: &[u8], x: usize, y: usize| image[(y * 64 + x) * 4 + 3];
        let stamped = target.rgba8.clone();
        assert!(alpha_at(&stamped, 32, 32) >= 240, "the stamp did not land");

        let erased = stamp_projection_onto_g2(
            &mut target.rgba8,
            64,
            64,
            source,
            &full_quad(),
            ProjectionBrush {
                erase: true,
                ..brush
            },
            to_source,
        );
        assert!(erased > 0, "the eraser touched nothing");
        assert!(
            alpha_at(&target.rgba8, 32, 32) < alpha_at(&stamped, 32, 32) / 8,
            "the brush centre survived: {} of {}",
            alpha_at(&target.rgba8, 32, 32),
            alpha_at(&stamped, 32, 32)
        );
        assert!(
            target
                .rgba8
                .chunks_exact(4)
                .zip(stamped.chunks_exact(4))
                .all(|(now, before)| now[3] <= before[3]),
            "the eraser added paint somewhere"
        );
    }

    #[test]
    fn erasing_clear_texels_reports_no_work() {
        let pixels = [255_u8, 0, 0, 255].repeat(16);
        let source = RgbaView::new(&pixels, 4, 4).expect("a red square");
        let mut target = TextureBakeImage::transparent(64, 64).expect("an atlas");
        let erased = stamp_projection_onto_g2(
            &mut target.rgba8,
            64,
            64,
            source,
            &full_quad(),
            ProjectionBrush {
                erase: true,
                ..red_stamp_brush()
            },
            |screen: [f32; 2]| Some([screen[0] / 100.0, screen[1] / 100.0]),
        );
        assert_eq!(erased, 0, "erasing nothing must not mark the project dirty");
        assert!(target.rgba8.iter().all(|value| *value == 0));
    }

    #[test]
    fn the_eraser_reaches_paint_the_stencil_no_longer_covers() {
        let pixels = [255_u8, 0, 0, 255].repeat(16);
        let source = RgbaView::new(&pixels, 4, 4).expect("a red square");
        let mut target = TextureBakeImage::transparent(64, 64).expect("an atlas");
        let brush = red_stamp_brush();
        stamp_projection_onto_g2(
            &mut target.rgba8,
            64,
            64,
            source,
            &full_quad(),
            brush,
            |screen: [f32; 2]| Some([screen[0] / 100.0, screen[1] / 100.0]),
        );

        let erased = stamp_projection_onto_g2(
            &mut target.rgba8,
            64,
            64,
            source,
            &full_quad(),
            ProjectionBrush {
                erase: true,
                ..brush
            },
            |_| None,
        );
        assert!(
            erased > 0,
            "the eraser must not need the stencil to still be over the paint"
        );
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
