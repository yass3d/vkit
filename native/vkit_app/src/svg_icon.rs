use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use egui::{Color32, Pos2, Rect, Shape, Stroke, Vec2, epaint::PathShape};

#[derive(Clone, Debug)]
pub struct SvgIcon {
    outlines: Vec<Outline>,
}

#[derive(Clone, Debug)]
struct Outline {
    points: Vec<Pos2>,
    closed: bool,

    width: Option<f32>,
    fill: Vec<[u32; 3]>,
}

const CURVE_SEGMENTS: usize = 16;

impl SvgIcon {
    pub fn parse(source: &str) -> Result<Self, String> {
        let tree = usvg::Tree::from_str(source, &usvg::Options::default())
            .map_err(|error| format!("SVG did not parse: {error}"))?;
        let size = tree.size();
        let (width, height) = (size.width(), size.height());
        if !(width > 0.0 && height > 0.0) {
            return Err("SVG has no size".to_owned());
        }

        let span = width.max(height);
        let mut outlines = Vec::new();
        collect(tree.root(), span, &mut outlines);
        if outlines.is_empty() {
            return Err("SVG carries no drawable outline".to_owned());
        }
        Ok(Self { outlines })
    }

    #[must_use]
    pub fn shapes(&self, rect: Rect, color: Color32) -> Vec<Shape> {
        let scale = rect.width().min(rect.height());
        let origin = rect.center() - Vec2::splat(scale * 0.5);
        let place = |point: Pos2| origin + Vec2::new(point.x * scale, point.y * scale);
        self.outlines
            .iter()
            .map(|outline| {
                let points: Vec<Pos2> = outline.points.iter().copied().map(place).collect();
                if !outline.fill.is_empty() {
                    let mut mesh = egui::epaint::Mesh::default();
                    for point in &points {
                        mesh.colored_vertex(*point, color);
                    }
                    for [a, b, c] in outline.fill.iter().copied() {
                        mesh.add_triangle(a, b, c);
                    }
                    Shape::mesh(mesh)
                } else {
                    let width = outline.width.unwrap_or(0.06) * scale;
                    let stroke = Stroke::new(width, color);
                    if outline.closed {
                        Shape::Path(PathShape::closed_line(points, stroke))
                    } else {
                        Shape::Path(PathShape::line(points, stroke))
                    }
                }
            })
            .collect()
    }
}

fn triangulate(points: &[Pos2]) -> Vec<[u32; 3]> {
    if points.len() < 3 {
        return Vec::new();
    }
    let mut remaining: Vec<u32> = (0..points.len() as u32).collect();
    if signed_area(points) < 0.0 {
        remaining.reverse();
    }
    let mut triangles = Vec::with_capacity(points.len().saturating_sub(2));
    let mut without_progress = 0usize;
    while remaining.len() > 3 {
        if without_progress > remaining.len() {
            return Vec::new();
        }
        let count = remaining.len();
        let mut clipped = false;
        for position in 0..count {
            let ear = [
                remaining[(position + count - 1) % count],
                remaining[position],
                remaining[(position + 1) % count],
            ];
            if !is_ear(points, &remaining, ear) {
                continue;
            }
            triangles.push(ear);
            remaining.remove(position);
            clipped = true;
            without_progress = 0;
            break;
        }
        if !clipped {
            without_progress += 1;
            remaining.rotate_left(1);
        }
    }
    triangles.push([remaining[0], remaining[1], remaining[2]]);
    triangles
}

fn signed_area(points: &[Pos2]) -> f32 {
    let mut total = 0.0;
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        total += a.x * b.y - b.x * a.y;
    }
    total
}

fn is_ear(points: &[Pos2], remaining: &[u32], ear: [u32; 3]) -> bool {
    let [a, b, c] = ear.map(|index| points[index as usize]);
    if cross(a, b, c) <= 0.0 {
        return false;
    }
    !remaining
        .iter()
        .filter(|index| !ear.contains(index))
        .any(|&index| inside(points[index as usize], a, b, c))
}

fn cross(a: Pos2, b: Pos2, c: Pos2) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn inside(point: Pos2, a: Pos2, b: Pos2, c: Pos2) -> bool {
    cross(a, b, point) >= 0.0 && cross(b, c, point) >= 0.0 && cross(c, a, point) >= 0.0
}

fn collect(group: &usvg::Group, span: f32, outlines: &mut Vec<Outline>) {
    for node in group.children() {
        match node {
            usvg::Node::Group(inner) => collect(inner, span, outlines),
            usvg::Node::Path(path) => flatten(path, span, outlines),

            usvg::Node::Image(_) | usvg::Node::Text(_) => {}
        }
    }
}

fn flatten(path: &usvg::Path, span: f32, outlines: &mut Vec<Outline>) {
    let filled = path.fill().is_some();
    let placement = path.abs_transform();
    let scale = ((placement.sx * placement.sy - placement.kx * placement.ky).abs())
        .sqrt()
        .max(f32::EPSILON);
    let width = path
        .stroke()
        .map(|stroke| stroke.width().get() * scale / span);
    if !filled && width.is_none() {
        return;
    }
    let mut points: Vec<Pos2> = Vec::new();
    let mut closed = false;
    let mut cursor = Pos2::ZERO;
    let mut flush = |points: &mut Vec<Pos2>, closed: &mut bool| {
        if points.len() >= 2 {
            let points = std::mem::take(points);
            let fill = if filled {
                triangulate(&points)
            } else {
                Vec::new()
            };
            outlines.push(Outline {
                points,
                closed: *closed,
                width,
                fill,
            });
        } else {
            points.clear();
        }
        *closed = false;
    };
    let at = |x: f32, y: f32| {
        let mut point = usvg::tiny_skia_path::Point { x, y };
        placement.map_points(std::slice::from_mut(&mut point));
        Pos2::new(point.x / span, point.y / span)
    };
    for segment in path.data().segments() {
        match segment {
            usvg::tiny_skia_path::PathSegment::MoveTo(point) => {
                flush(&mut points, &mut closed);
                cursor = at(point.x, point.y);
                points.push(cursor);
            }
            usvg::tiny_skia_path::PathSegment::LineTo(point) => {
                cursor = at(point.x, point.y);
                points.push(cursor);
            }
            usvg::tiny_skia_path::PathSegment::QuadTo(control, end) => {
                let (control, end) = (at(control.x, control.y), at(end.x, end.y));
                for step in 1..=CURVE_SEGMENTS {
                    #[expect(clippy::cast_precision_loss, reason = "a small step count")]
                    let t = step as f32 / CURVE_SEGMENTS as f32;
                    points.push(quadratic(cursor, control, end, t));
                }
                cursor = end;
            }
            usvg::tiny_skia_path::PathSegment::CubicTo(first, second, end) => {
                let (first, second, end) = (
                    at(first.x, first.y),
                    at(second.x, second.y),
                    at(end.x, end.y),
                );
                for step in 1..=CURVE_SEGMENTS {
                    #[expect(clippy::cast_precision_loss, reason = "a small step count")]
                    let t = step as f32 / CURVE_SEGMENTS as f32;
                    points.push(cubic(cursor, first, second, end, t));
                }
                cursor = end;
            }
            usvg::tiny_skia_path::PathSegment::Close => {
                closed = true;
                flush(&mut points, &mut closed);
            }
        }
    }
    flush(&mut points, &mut closed);
}

fn quadratic(start: Pos2, control: Pos2, end: Pos2, t: f32) -> Pos2 {
    let inverse = 1.0 - t;
    let weights = [inverse * inverse, 2.0 * inverse * t, t * t];
    Pos2::new(
        start.x * weights[0] + control.x * weights[1] + end.x * weights[2],
        start.y * weights[0] + control.y * weights[1] + end.y * weights[2],
    )
}

fn cubic(start: Pos2, first: Pos2, second: Pos2, end: Pos2, t: f32) -> Pos2 {
    let inverse = 1.0 - t;
    let weights = [
        inverse * inverse * inverse,
        3.0 * inverse * inverse * t,
        3.0 * inverse * t * t,
        t * t * t,
    ];
    Pos2::new(
        start.x * weights[0] + first.x * weights[1] + second.x * weights[2] + end.x * weights[3],
        start.y * weights[0] + first.y * weights[1] + second.y * weights[2] + end.y * weights[3],
    )
}

pub fn cached(source: &'static str) -> Option<&'static SvgIcon> {
    static CACHE: OnceLock<Mutex<HashMap<usize, Option<&'static SvgIcon>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = source.as_ptr() as usize;
    let mut cache = cache.lock().ok()?;
    *cache
        .entry(key)
        .or_insert_with(|| match SvgIcon::parse(source) {
            Ok(icon) => Some(Box::leak(Box::new(icon))),
            Err(reason) => {
                let _ = crate::diagnostics::record(
                    crate::diagnostics::Severity::Warning,
                    "ui",
                    "svg_icon_failed",
                    &reason,
                );
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"
        fill="none" stroke="currentColor" stroke-width="2">
        <rect x="4" y="4" width="16" height="16"/></svg>"#;

    const BRACKET: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
        <polygon fill="black" points="2,2 8,2 8,18 22,18 22,22 2,22"/></svg>"#;

    fn every_icon() -> Vec<(String, SvgIcon)> {
        crate::ui_components::Icon::ALL
            .iter()
            .filter_map(|&icon| {
                let source = crate::ui_components::icon_svg(icon)?;
                Some((
                    format!("{icon:?}"),
                    SvgIcon::parse(source).expect("an icon in the table must parse"),
                ))
            })
            .collect()
    }

    #[test]
    fn no_icon_sits_off_centre_in_its_own_box() {
        for (name, icon) in every_icon() {
            let (mut low, mut high) = ([f32::MAX; 2], [f32::MIN; 2]);
            for point in icon.outlines.iter().flat_map(|outline| &outline.points) {
                low = [low[0].min(point.x), low[1].min(point.y)];
                high = [high[0].max(point.x), high[1].max(point.y)];
            }
            for (axis, (start, end)) in [(low[0], high[0]), (low[1], high[1])].iter().enumerate() {
                let centre = (start + end) * 0.5;
                assert!(
                    (centre - 0.5).abs() < 0.12,
                    "{name} sits at {centre:.2} on axis {axis}; it will look nudged"
                );
            }
        }
    }

    #[test]
    fn icons_are_drawn_in_line_weights_that_match_each_other() {
        for (name, icon) in every_icon() {
            for outline in &icon.outlines {
                let Some(width) = outline.width else {
                    continue;
                };
                assert!(
                    (width - 2.0 / 24.0).abs() < 1.0e-3,
                    "{name} strokes at {width:.4}, not the pack's 2 units in 24"
                );
            }
        }
    }

    #[test]
    fn a_filled_corner_bracket_keeps_its_notch() {
        let icon = SvgIcon::parse(BRACKET).expect("the fixture parses");
        let outline = &icon.outlines[0];
        assert_eq!(
            outline.fill.len(),
            4,
            "six corners make four triangles, not a square"
        );

        let covered = |x: f32, y: f32| {
            let point = Pos2::new(x / 24.0, y / 24.0);
            outline.fill.iter().any(|&[a, b, c]| {
                inside(
                    point,
                    outline.points[a as usize],
                    outline.points[b as usize],
                    outline.points[c as usize],
                )
            })
        };
        assert!(covered(5.0, 20.0), "the corner of the L must be filled");
        assert!(covered(18.0, 20.0), "the foot of the L must be filled");
        assert!(covered(5.0, 5.0), "the upright of the L must be filled");
        assert!(
            !covered(18.0, 5.0),
            "the notch must stay empty — this is what a convex fill got wrong"
        );
    }

    #[test]
    fn a_shape_it_cannot_read_still_returns() {
        let bowtie = [
            Pos2::new(0.0, 0.0),
            Pos2::new(1.0, 1.0),
            Pos2::new(1.0, 0.0),
            Pos2::new(0.0, 1.0),
        ];
        let triangles = triangulate(&bowtie);
        assert!(triangles.len() <= bowtie.len() - 2);
        for index in triangles.iter().flatten() {
            assert!((*index as usize) < bowtie.len(), "index out of the outline");
        }

        assert!(
            triangulate(&bowtie[..2]).is_empty(),
            "two points are no shape"
        );
        assert!(triangulate(&[]).is_empty());
    }

    #[test]
    fn a_stroked_shape_becomes_a_closed_outline_in_the_unit_square() {
        let icon = SvgIcon::parse(SQUARE).expect("the fixture parses");
        assert_eq!(icon.outlines.len(), 1);
        let outline = &icon.outlines[0];
        assert!(outline.closed, "a rect is closed");
        assert!(outline.fill.is_empty(), "fill=none is not a fill");

        let width = outline.width.expect("a stroke width");
        assert!((width - 2.0 / 24.0).abs() < 1.0e-6, "{width}");

        let xs: Vec<f32> = outline.points.iter().map(|point| point.x).collect();
        let low = xs.iter().copied().fold(f32::MAX, f32::min);
        let high = xs.iter().copied().fold(f32::MIN, f32::max);
        assert!((low - 4.0 / 24.0).abs() < 1.0e-5, "{low}");
        assert!((high - 20.0 / 24.0).abs() < 1.0e-5, "{high}");
    }

    #[test]
    fn a_circle_keeps_its_curvature() {
        let icon = SvgIcon::parse(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"
               fill="none" stroke="black" stroke-width="2">
               <circle cx="12" cy="12" r="8"/></svg>"#,
        )
        .expect("the fixture parses");
        let points: Vec<Pos2> = icon
            .outlines
            .iter()
            .flat_map(|outline| outline.points.iter().copied())
            .collect();
        assert!(
            points.len() > 32,
            "a circle is not a few corners: {}",
            points.len()
        );
        for point in &points {
            let radius = ((point.x - 0.5).powi(2) + (point.y - 0.5).powi(2)).sqrt();
            assert!(
                (radius - 8.0 / 24.0).abs() < 2.0e-3,
                "{point:?} sits off the circle at {radius}"
            );
        }
    }

    #[test]
    fn shapes_land_inside_the_rect_they_are_given() {
        let icon = SvgIcon::parse(SQUARE).expect("the fixture parses");
        let rect = Rect::from_min_size(Pos2::new(100.0, 50.0), Vec2::splat(32.0));
        let shapes = icon.shapes(rect, Color32::WHITE);
        assert_eq!(shapes.len(), 1);
        let bounds = shapes[0].visual_bounding_rect();
        assert!(
            rect.expand(4.0).contains_rect(bounds),
            "{bounds:?} escaped {rect:?}"
        );
    }

    #[test]
    fn an_empty_document_is_refused() {
        assert!(
            SvgIcon::parse(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"/>"#)
                .is_err()
        );
        assert!(SvgIcon::parse("not an svg at all").is_err());
    }
}
