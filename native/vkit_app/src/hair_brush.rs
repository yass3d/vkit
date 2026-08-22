use glam::Vec3;

use crate::i18n::TextKey;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HairBrushShape {
    #[default]
    Circle,

    Wide,

    Tall,
}

impl HairBrushShape {
    pub const ALL: [Self; 3] = [Self::Circle, Self::Wide, Self::Tall];

    #[must_use]
    pub const fn label_key(self) -> TextKey {
        match self {
            Self::Circle => TextKey::HairBrushShapeCircle,
            Self::Wide => TextKey::HairBrushShapeWide,
            Self::Tall => TextKey::HairBrushShapeTall,
        }
    }

    #[must_use]
    pub const fn is_bar(self) -> bool {
        matches!(self, Self::Wide | Self::Tall)
    }
}

pub const BAR_ASPECT: f32 = 3.0;

#[derive(Clone, Copy, Debug)]
pub struct BrushFrame {
    pub centre: Vec3,
    pub normal: Vec3,
    pub along: Vec3,
    pub across: Vec3,
    pub half_along: f32,
    pub half_across: f32,
    pub depth: f32,
}

fn on_plane(vector: Vec3, normal: Vec3, fallback: Vec3) -> Vec3 {
    let flattened = vector - normal * normal.dot(vector);
    if flattened.length_squared() > 1.0e-10 {
        return flattened.normalize();
    }
    let second = fallback - normal * normal.dot(fallback);
    if second.length_squared() > 1.0e-10 {
        return second.normalize();
    }
    normal.any_orthonormal_vector()
}

#[must_use]
pub fn brush_frame(
    shape: HairBrushShape,
    centre: Vec3,
    radius: f32,
    normal: Vec3,
    camera_right: Vec3,
    camera_up: Vec3,
    stroke: Option<Vec3>,
) -> BrushFrame {
    let normal = if normal.length_squared() > 1.0e-10 {
        normal.normalize()
    } else {
        camera_right.cross(camera_up).normalize_or_zero()
    };
    // A circle is the same in every direction, so the axes only have to be a
    // frame; a bar is aimed, and the aim is what the person drew or what the
    // screen calls sideways.
    let wanted = match (shape, stroke) {
        (HairBrushShape::Circle, _) => camera_right,
        (_, Some(stroke)) if stroke.length_squared() > 1.0e-10 => stroke,
        (HairBrushShape::Wide, _) => camera_right,
        (HairBrushShape::Tall, _) => camera_up,
    };
    let along = on_plane(wanted, normal, camera_right);
    let across = normal.cross(along).normalize_or_zero();
    let (half_along, half_across) = if shape.is_bar() {
        (radius * BAR_ASPECT, radius)
    } else {
        (radius, radius)
    };
    BrushFrame {
        centre,
        normal,
        along,
        across,
        half_along,
        half_across,
        depth: radius,
    }
}

impl BrushFrame {
    #[must_use]
    pub fn contains(&self, point: Vec3) -> bool {
        let offset = point - self.centre;
        if self.half_along == self.half_across {
            let flat = offset - self.normal * self.normal.dot(offset);
            return flat.length_squared() <= self.half_along * self.half_along
                && offset.dot(self.normal).abs() <= self.depth;
        }
        offset.dot(self.along).abs() <= self.half_along
            && offset.dot(self.across).abs() <= self.half_across
            && offset.dot(self.normal).abs() <= self.depth
    }

    #[must_use]
    pub fn outline(&self, segments: usize) -> Vec<Vec3> {
        let segments = segments.max(8);
        if self.half_along == self.half_across {
            return (0..segments)
                .map(|step| {
                    #[expect(clippy::cast_precision_loss, reason = "a ring of a few dozen steps")]
                    let angle = std::f32::consts::TAU * step as f32 / segments as f32;
                    self.centre
                        + self.along * (angle.cos() * self.half_along)
                        + self.across * (angle.sin() * self.half_across)
                })
                .collect();
        }
        let corners = [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)];
        let per_edge = segments / 4;
        let mut points = Vec::with_capacity(segments);
        for edge in 0..4 {
            let (ax, ay) = corners[edge];
            let (bx, by) = corners[(edge + 1) % 4];
            for step in 0..per_edge {
                #[expect(clippy::cast_precision_loss, reason = "a handful of steps per edge")]
                let t = step as f32 / per_edge as f32;
                points.push(
                    self.centre
                        + self.along * ((ax + (bx - ax) * t) * self.half_along)
                        + self.across * ((ay + (by - ay) * t) * self.half_across),
                );
            }
        }
        points
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RIGHT: Vec3 = Vec3::X;
    const UP: Vec3 = Vec3::Y;
    const NORMAL: Vec3 = Vec3::Z;

    fn frame(shape: HairBrushShape, stroke: Option<Vec3>) -> BrushFrame {
        brush_frame(shape, Vec3::ZERO, 1.0, NORMAL, RIGHT, UP, stroke)
    }

    #[test]
    fn a_circle_reaches_the_same_distance_whichever_way_you_go() {
        let circle = frame(HairBrushShape::Circle, None);
        for (x, y) in [(0.99, 0.0), (0.0, 0.99), (0.7, 0.7), (-0.7, -0.7)] {
            assert!(
                circle.contains(Vec3::new(x, y, 0.0)),
                "({x}, {y}) is inside a unit circle"
            );
        }
        assert!(!circle.contains(Vec3::new(1.01, 0.0, 0.0)));
        assert!(!circle.contains(Vec3::new(0.8, 0.8, 0.0)));
    }

    #[test]
    fn a_bar_is_long_the_way_it_is_aimed_and_narrow_across() {
        let wide = frame(HairBrushShape::Wide, None);
        assert!(wide.contains(Vec3::new(BAR_ASPECT - 0.01, 0.0, 0.0)));
        assert!(!wide.contains(Vec3::new(BAR_ASPECT + 0.01, 0.0, 0.0)));
        assert!(wide.contains(Vec3::new(0.0, 0.99, 0.0)));
        assert!(!wide.contains(Vec3::new(0.0, 1.01, 0.0)));

        let tall = frame(HairBrushShape::Tall, None);
        assert!(tall.contains(Vec3::new(0.0, BAR_ASPECT - 0.01, 0.0)));
        assert!(!tall.contains(Vec3::new(BAR_ASPECT - 0.01, 0.0, 0.0)));
    }

    #[test]
    fn a_stroke_turns_the_bar_and_leaves_the_circle_alone() {
        let diagonal = Vec3::new(1.0, 1.0, 0.0).normalize();
        let followed = frame(HairBrushShape::Wide, Some(diagonal));
        assert!(
            followed.contains(diagonal * (BAR_ASPECT - 0.01)),
            "the long axis has to lie along the stroke"
        );
        assert!(
            !followed.contains(Vec3::new(BAR_ASPECT - 0.01, 0.0, 0.0)),
            "and no longer along the screen"
        );

        let circle = frame(HairBrushShape::Circle, Some(diagonal));
        assert!(circle.contains(Vec3::new(0.99, 0.0, 0.0)));
        assert!(circle.contains(diagonal * 0.99));
    }

    #[test]
    fn the_frame_lies_on_the_surface_rather_than_facing_the_screen() {
        let tilted = Vec3::new(0.0, 1.0, 1.0).normalize();
        let frame = brush_frame(
            HairBrushShape::Wide,
            Vec3::ZERO,
            1.0,
            tilted,
            RIGHT,
            UP,
            None,
        );
        assert!(
            frame.along.dot(tilted).abs() < 1.0e-5,
            "the long axis has to lie in the surface: {:?}",
            frame.along
        );
        assert!(
            frame.across.dot(tilted).abs() < 1.0e-5,
            "and so does the short one: {:?}",
            frame.across
        );
        assert!((frame.along.cross(frame.across).dot(tilted) - 1.0).abs() < 1.0e-4);
    }

    #[test]
    fn the_outline_closes_and_stays_on_the_surface() {
        for shape in HairBrushShape::ALL {
            let frame = frame(shape, None);
            let outline = frame.outline(32);
            assert!(outline.len() >= 8, "{shape:?} drew nothing to look at");
            for point in &outline {
                let depth = (*point - frame.centre).dot(frame.normal);
                assert!(depth.abs() < 1.0e-5, "{shape:?} left the surface");
            }
            let reach = outline
                .iter()
                .map(|point| (*point - frame.centre).length())
                .fold(0.0_f32, f32::max);
            assert!(
                (reach
                    - if shape.is_bar() {
                        (BAR_ASPECT * BAR_ASPECT + 1.0_f32).sqrt()
                    } else {
                        1.0
                    })
                .abs()
                    < 1.0e-4,
                "{shape:?} reaches {reach}"
            );
        }
    }
}
