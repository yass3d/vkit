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
    filled: bool,
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
                if outline.filled {
                    Shape::Path(PathShape::convex_polygon(points, color, Stroke::NONE))
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
    let width = path.stroke().map(|stroke| stroke.width().get() / span);
    if !filled && width.is_none() {
        return;
    }
    let mut points: Vec<Pos2> = Vec::new();
    let mut closed = false;
    let mut cursor = Pos2::ZERO;
    let mut flush = |points: &mut Vec<Pos2>, closed: &mut bool| {
        if points.len() >= 2 {
            outlines.push(Outline {
                points: std::mem::take(points),
                closed: *closed,
                width,
                filled,
            });
        } else {
            points.clear();
        }
        *closed = false;
    };
    let at = |x: f32, y: f32| Pos2::new(x / span, y / span);
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

    #[test]
    fn a_stroked_shape_becomes_a_closed_outline_in_the_unit_square() {
        let icon = SvgIcon::parse(SQUARE).expect("the fixture parses");
        assert_eq!(icon.outlines.len(), 1);
        let outline = &icon.outlines[0];
        assert!(outline.closed, "a rect is closed");
        assert!(!outline.filled, "fill=none is not a fill");

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
