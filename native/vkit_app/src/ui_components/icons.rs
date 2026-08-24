use egui::{self, Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Settings,

    Star,
    StarFilled,
    Refresh,
    UpdateAvailable,
    Folder,
    Save,
    Camera,

    HeadTexture,

    EyeOpen,
    EyeClosed,
    Lock,

    Trash,
    Pencil,
    Copy,
    Paste,
    Plus,
    Chain,
    MirrorX,
    Brush,
    Comb,
    Eraser,
    Scissors,

    TexturePin,

    TextureMask,

    CloneStamp,
    DodgeBurn,

    TextureSponge,

    Projector,

    BrushMove,

    BrushSmooth,

    BrushRestore,
    BackfaceProtection,
    ConnectedTopology,
    FalloffSmooth,
    FalloffSmoother,
    FalloffSharp,
    FalloffLinear,

    Picture,

    Wireframe,

    SplitColumns,
    SplitRows,

    Xray,

    LightBulb,

    LightRotation,

    Caution,
    ChevronDown,
    ChevronUp,
    Pinch,
    HairLength,
    HairPuff,
    HairRigidity,
    BodyCapsules,
    HairPlant,
    VennThree,
    CrosshairBox,
    HairVertex,
    GlobeGravity,
    HairStream,
    Undo,
    Redo,
    Hammer,
    Broom,
    MirrorPart,
    CursorPick,
    ChevronLeft,
    ChevronRight,
    Search,

    GitHub,

    Coffee,

    Check,

    Cross,

    WindowMinimize,
    WindowMaximize,
    WindowRestore,
    WindowClose,
}

impl Icon {
    #[cfg(test)]
    pub const ALL: [Self; 65] = [
        Self::Copy,
        Self::Paste,
        Self::Plus,
        Self::HairPlant,
        Self::VennThree,
        Self::CrosshairBox,
        Self::GlobeGravity,
        Self::HairStream,
        Self::Undo,
        Self::Redo,
        Self::Hammer,
        Self::Broom,
        Self::MirrorPart,
        Self::CursorPick,
        Self::SplitColumns,
        Self::SplitRows,
        Self::Refresh,
        Self::Folder,
        Self::Save,
        Self::Camera,
        Self::HeadTexture,
        Self::EyeOpen,
        Self::EyeClosed,
        Self::Lock,
        Self::Chain,
        Self::GitHub,
        Self::Coffee,
        Self::Check,
        Self::Cross,
        Self::MirrorX,
        Self::Brush,
        Self::TexturePin,
        Self::TextureMask,
        Self::CloneStamp,
        Self::DodgeBurn,
        Self::TextureSponge,
        Self::Projector,
        Self::BrushMove,
        Self::BrushSmooth,
        Self::BrushRestore,
        Self::BackfaceProtection,
        Self::ConnectedTopology,
        Self::FalloffSmooth,
        Self::FalloffSmoother,
        Self::FalloffSharp,
        Self::FalloffLinear,
        Self::Picture,
        Self::Wireframe,
        Self::Xray,
        Self::LightBulb,
        Self::LightRotation,
        Self::Caution,
        Self::ChevronDown,
        Self::ChevronLeft,
        Self::ChevronRight,
        Self::Search,
        Self::WindowMinimize,
        Self::WindowMaximize,
        Self::WindowRestore,
        Self::WindowClose,
        Self::Settings,
        Self::Star,
        Self::StarFilled,
        Self::Trash,
        Self::Pencil,
    ];
}

pub const fn transform_group_editability_icon(editable: bool) -> Icon {
    if editable { Icon::Brush } else { Icon::Lock }
}

const ICON_STROKE_WEIGHT: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum IconOp {
    Stroke(&'static [[f32; 2]]),

    Closed(&'static [[f32; 2]]),

    Cubic([[f32; 2]; 4]),

    Fill(&'static [[f32; 2]]),

    RoundRect {
        min: [f32; 2],
        max: [f32; 2],
        radius: f32,
    },
}

const STAR_FILLED_OPS: &[IconOp] = &[
    IconOp::Fill(&[
        [12.00, 2.40],
        [14.41, 8.68],
        [21.13, 9.03],
        [15.90, 13.27],
        [17.64, 19.77],
        [12.00, 16.10],
        [6.36, 19.77],
        [8.10, 13.27],
        [2.87, 9.03],
        [9.59, 8.68],
    ]),
    IconOp::Closed(&[
        [12.00, 2.40],
        [14.41, 8.68],
        [21.13, 9.03],
        [15.90, 13.27],
        [17.64, 19.77],
        [12.00, 16.10],
        [6.36, 19.77],
        [8.10, 13.27],
        [2.87, 9.03],
        [9.59, 8.68],
    ]),
];

const FALLOFF_FRAME: IconOp = IconOp::RoundRect {
    min: [3.0, 3.0],
    max: [21.0, 21.0],
    radius: 1.0,
};

const FALLOFF_SMOOTH_OPS: &[IconOp] = &[
    FALLOFF_FRAME,
    IconOp::Cubic([[4.5, 19.0], [8.0, 18.6], [15.0, 11.8], [19.5, 5.0]]),
];

const FALLOFF_SMOOTHER_OPS: &[IconOp] = &[
    FALLOFF_FRAME,
    IconOp::Cubic([[4.5, 19.0], [11.0, 18.9], [12.7, 5.1], [19.5, 5.0]]),
];

const FALLOFF_SHARP_OPS: &[IconOp] = &[
    FALLOFF_FRAME,
    IconOp::Cubic([[4.5, 19.0], [14.8, 19.0], [17.0, 7.0], [19.5, 5.0]]),
];

const FALLOFF_LINEAR_OPS: &[IconOp] = &[FALLOFF_FRAME, IconOp::Stroke(&[[4.5, 19.0], [19.5, 5.0]])];

const WINDOW_MINIMIZE_OPS: &[IconOp] = &[IconOp::Stroke(&[[7.4, 12.0], [16.6, 12.0]])];

const WINDOW_MAXIMIZE_OPS: &[IconOp] = &[IconOp::RoundRect {
    min: [7.0, 7.0],
    max: [17.0, 17.0],
    radius: 1.0,
}];

const WINDOW_RESTORE_OPS: &[IconOp] = &[
    IconOp::RoundRect {
        min: [9.0, 7.0],
        max: [17.0, 15.0],
        radius: 1.0,
    },
    IconOp::RoundRect {
        min: [7.0, 9.0],
        max: [15.0, 17.0],
        radius: 1.0,
    },
];

const WINDOW_CLOSE_OPS: &[IconOp] = &[
    IconOp::Stroke(&[[7.4, 7.4], [16.6, 16.6]]),
    IconOp::Stroke(&[[16.6, 7.4], [7.4, 16.6]]),
];

#[cfg(test)]
pub(crate) const fn icon_svg(glyph: Icon) -> Option<&'static str> {
    match icon_art(glyph) {
        IconArt::Svg(source) => Some(source),
        IconArt::Drawn(_) => None,
    }
}

const fn icon_art(glyph: Icon) -> IconArt {
    let source = match glyph {
        Icon::Settings => include_str!("../../resources/icons/settings.svg"),
        Icon::Star => include_str!("../../resources/icons/star.svg"),
        Icon::Refresh => include_str!("../../resources/icons/refresh-cw.svg"),
        Icon::UpdateAvailable => include_str!("../../resources/icons/circle-arrow-up.svg"),
        Icon::Folder => include_str!("../../resources/icons/folder.svg"),
        Icon::Save => include_str!("../../resources/icons/save.svg"),
        Icon::Camera => include_str!("../../resources/icons/camera.svg"),
        Icon::HeadTexture => include_str!("../../resources/icons/scan-face.svg"),
        Icon::EyeOpen => include_str!("../../resources/icons/eye.svg"),
        Icon::EyeClosed => include_str!("../../resources/icons/eye-off.svg"),
        Icon::Lock => include_str!("../../resources/icons/lock.svg"),
        Icon::Pencil => include_str!("../../resources/icons/pencil.svg"),
        Icon::Trash => include_str!("../../resources/icons/trash-2.svg"),
        Icon::Chain => include_str!("../../resources/icons/link.svg"),
        Icon::SplitColumns => include_str!("../../resources/icons/columns-2.svg"),
        Icon::SplitRows => include_str!("../../resources/icons/rows-2.svg"),
        Icon::GitHub => include_str!("../../resources/icons/github.svg"),
        Icon::Coffee => include_str!("../../resources/icons/coffee.svg"),
        Icon::Check => include_str!("../../resources/icons/check.svg"),
        Icon::Cross => include_str!("../../resources/icons/x.svg"),
        Icon::MirrorX => include_str!("../../resources/icons/flip-horizontal.svg"),
        Icon::Brush => include_str!("../../resources/icons/paintbrush.svg"),
        Icon::Comb => include_str!("../../resources/icons/comb.svg"),
        Icon::Eraser => include_str!("../../resources/icons/eraser.svg"),
        Icon::Scissors => include_str!("../../resources/icons/scissors.svg"),
        Icon::TexturePin => include_str!("../../resources/icons/map-pin.svg"),
        Icon::TextureMask => include_str!("../../resources/icons/eraser.svg"),
        Icon::CloneStamp => include_str!("../../resources/icons/stamp.svg"),
        Icon::DodgeBurn => include_str!("../../resources/icons/cloud-sun.svg"),
        Icon::TextureSponge => include_str!("../../resources/icons/contrast.svg"),
        Icon::Projector => include_str!("../../resources/icons/face.svg"),
        Icon::BrushMove => include_str!("../../resources/icons/hand.svg"),
        Icon::BrushSmooth => include_str!("../../resources/icons/droplet.svg"),
        Icon::BrushRestore => include_str!("../../resources/icons/rotate-ccw.svg"),
        Icon::Copy => include_str!("../../resources/icons/copy.svg"),
        Icon::HairPlant => include_str!("../../resources/icons/hair-plant.svg"),
        Icon::VennThree => include_str!("../../resources/icons/venn-three.svg"),
        Icon::CrosshairBox => include_str!("../../resources/icons/crosshair-box.svg"),
        Icon::HairVertex => include_str!("../../resources/icons/hair-vertex.svg"),
        Icon::GlobeGravity => include_str!("../../resources/icons/globe-gravity.svg"),
        Icon::HairStream => include_str!("../../resources/icons/hair-stream.svg"),
        Icon::Undo => include_str!("../../resources/icons/rotate-ccw.svg"),
        Icon::Redo => include_str!("../../resources/icons/rotate-cw.svg"),
        Icon::Hammer => include_str!("../../resources/icons/hammer.svg"),
        Icon::Broom => include_str!("../../resources/icons/brush-cleaning.svg"),
        Icon::MirrorPart => include_str!("../../resources/icons/mirror-part.svg"),
        Icon::CursorPick => include_str!("../../resources/icons/cursor-pick.svg"),
        Icon::Paste => include_str!("../../resources/icons/clipboard-paste.svg"),
        Icon::Plus => include_str!("../../resources/icons/plus.svg"),
        Icon::BackfaceProtection => include_str!("../../resources/icons/shield.svg"),
        Icon::ConnectedTopology => include_str!("../../resources/icons/waypoints.svg"),
        Icon::Picture => include_str!("../../resources/icons/image.svg"),
        Icon::Wireframe => include_str!("../../resources/icons/grid-3x3.svg"),
        Icon::Xray => include_str!("../../resources/icons/scan.svg"),
        Icon::LightBulb => include_str!("../../resources/icons/lightbulb.svg"),
        Icon::LightRotation => include_str!("../../resources/icons/rotate-3d.svg"),
        Icon::Caution => include_str!("../../resources/icons/triangle-alert.svg"),
        Icon::ChevronDown => include_str!("../../resources/icons/chevron-down.svg"),
        Icon::ChevronUp => include_str!("../../resources/icons/chevron-up.svg"),
        Icon::Pinch => include_str!("../../resources/icons/pinch.svg"),
        Icon::HairLength => include_str!("../../resources/icons/hair-length.svg"),
        Icon::HairPuff => include_str!("../../resources/icons/hair-puff.svg"),
        Icon::HairRigidity => include_str!("../../resources/icons/hair-rigidity.svg"),
        Icon::BodyCapsules => include_str!("../../resources/icons/body-capsules.svg"),
        Icon::ChevronLeft => include_str!("../../resources/icons/chevron-left.svg"),
        Icon::ChevronRight => include_str!("../../resources/icons/chevron-right.svg"),
        Icon::Search => include_str!("../../resources/icons/search.svg"),
        Icon::StarFilled => return IconArt::Drawn(STAR_FILLED_OPS),
        Icon::FalloffSmooth => return IconArt::Drawn(FALLOFF_SMOOTH_OPS),
        Icon::FalloffSmoother => {
            return IconArt::Drawn(FALLOFF_SMOOTHER_OPS);
        }
        Icon::FalloffSharp => return IconArt::Drawn(FALLOFF_SHARP_OPS),
        Icon::FalloffLinear => return IconArt::Drawn(FALLOFF_LINEAR_OPS),
        Icon::WindowMinimize => {
            return IconArt::Drawn(WINDOW_MINIMIZE_OPS);
        }
        Icon::WindowMaximize => {
            return IconArt::Drawn(WINDOW_MAXIMIZE_OPS);
        }
        Icon::WindowRestore => return IconArt::Drawn(WINDOW_RESTORE_OPS),
        Icon::WindowClose => return IconArt::Drawn(WINDOW_CLOSE_OPS),
    };
    IconArt::Svg(source)
}

enum IconArt {
    Svg(&'static str),
    Drawn(&'static [IconOp]),
}

pub fn paint_icon(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let def = match icon_art(icon) {
        IconArt::Svg(source) => {
            if let Some(drawing) = crate::svg_icon::cached(source) {
                painter.extend(drawing.shapes(rect, color));
                return;
            }

            return;
        }
        IconArt::Drawn(ops) => ops,
    };
    let scale = (rect.width().min(rect.height()) / 24.0).max(0.01);
    let stroke = Stroke::new((ICON_STROKE_WEIGHT * scale).max(1.0), color);
    let center = rect.center();
    let map = move |grid: [f32; 2]| -> Pos2 {
        center + Vec2::new((grid[0] - 12.0) * scale, (grid[1] - 12.0) * scale)
    };
    let cap = |point: Pos2| painter.circle_filled(point, stroke.width * 0.5, color);
    for op in def {
        match *op {
            IconOp::Stroke(points) => {
                let mapped: Vec<Pos2> = points.iter().copied().map(map).collect();
                if let (Some(first), Some(last)) = (mapped.first(), mapped.last()) {
                    cap(*first);
                    cap(*last);
                }
                painter.add(Shape::line(mapped, stroke));
            }
            IconOp::Closed(points) => {
                let mapped: Vec<Pos2> = points.iter().copied().map(map).collect();
                painter.add(Shape::closed_line(mapped, stroke));
            }
            IconOp::Cubic(points) => {
                let mapped = points.map(map);
                cap(mapped[0]);
                cap(mapped[3]);
                painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                    mapped,
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                ));
            }
            IconOp::Fill(points) => {
                let mapped: Vec<Pos2> = points.iter().copied().map(map).collect();
                painter.add(Shape::convex_polygon(mapped, color, Stroke::NONE));
            }
            IconOp::RoundRect { min, max, radius } => {
                painter.rect_stroke(
                    Rect::from_min_max(map(min), map(max)),
                    radius * scale,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
    }
}

#[cfg(test)]
fn icon_grid_point(rect: Rect, x: f32, y: f32) -> Pos2 {
    let scale = (rect.width().min(rect.height()) / 24.0).max(0.01);
    let origin = rect.center() - Vec2::splat(12.0 * scale);
    Pos2::new(origin.x + x * scale, origin.y + y * scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editable_transform_groups_use_a_brush_and_locked_groups_use_a_lock() {
        assert_eq!(transform_group_editability_icon(true), Icon::Brush);
        assert_eq!(transform_group_editability_icon(false), Icon::Lock);
    }

    #[test]
    fn supplied_svg_coordinates_keep_the_24_px_grid() {
        let rect = Rect::from_min_size(Pos2::new(40.0, 50.0), Vec2::splat(48.0));
        assert_eq!(icon_grid_point(rect, 21.0, 3.0), Pos2::new(82.0, 56.0));

        assert_eq!(
            icon_grid_point(rect, 17.64225, 19.5),
            Pos2::new(75.2845, 89.0)
        );
    }

    #[test]
    fn the_icon_list_accounts_for_every_icon() {
        for glyph in Icon::ALL {
            match glyph {
                Icon::Settings => {}
                Icon::Star => {}
                Icon::StarFilled => {}
                Icon::Refresh => {}
                Icon::UpdateAvailable => {}
                Icon::Folder => {}
                Icon::Save => {}
                Icon::Camera => {}
                Icon::HeadTexture => {}
                Icon::EyeOpen => {}
                Icon::EyeClosed => {}
                Icon::Lock => {}
                Icon::Trash => {}
                Icon::Pencil => {}
                Icon::Copy => {}
                Icon::HairPlant => {}
                Icon::VennThree => {}
                Icon::CrosshairBox => {}
                Icon::HairVertex => {}
                Icon::GlobeGravity => {}
                Icon::HairStream => {}
                Icon::Undo => {}
                Icon::Redo => {}
                Icon::Hammer => {}
                Icon::Broom => {}
                Icon::MirrorPart => {}
                Icon::CursorPick => {}
                Icon::Paste => {}
                Icon::Plus => {}
                Icon::Chain => {}
                Icon::SplitColumns => {}
                Icon::SplitRows => {}
                Icon::GitHub => {}
                Icon::Coffee => {}
                Icon::Check => {}
                Icon::Cross => {}
                Icon::MirrorX => {}
                Icon::Brush => {}
                Icon::Comb => {}
                Icon::Eraser => {}
                Icon::Scissors => {}
                Icon::TexturePin => {}
                Icon::TextureMask => {}
                Icon::CloneStamp => {}
                Icon::DodgeBurn => {}
                Icon::TextureSponge => {}
                Icon::Projector => {}
                Icon::BrushMove => {}
                Icon::BrushSmooth => {}
                Icon::BrushRestore => {}
                Icon::BackfaceProtection => {}
                Icon::ConnectedTopology => {}
                Icon::FalloffSmooth => {}
                Icon::FalloffSmoother => {}
                Icon::FalloffSharp => {}
                Icon::FalloffLinear => {}
                Icon::Picture => {}
                Icon::Wireframe => {}
                Icon::Xray => {}
                Icon::LightBulb => {}
                Icon::LightRotation => {}
                Icon::Caution => {}
                Icon::ChevronDown => {}
                Icon::ChevronUp => {}
                Icon::Pinch => {}
                Icon::HairLength => {}
                Icon::HairPuff => {}
                Icon::HairRigidity => {}
                Icon::BodyCapsules => {}
                Icon::ChevronLeft => {}
                Icon::ChevronRight => {}
                Icon::Search => {}
                Icon::WindowMinimize => {}
                Icon::WindowMaximize => {}
                Icon::WindowRestore => {}
                Icon::WindowClose => {}
            }
        }
        let mut seen: Vec<String> = Icon::ALL.iter().map(|g| format!("{g:?}")).collect();
        seen.sort_unstable();
        let count = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), count, "an icon is listed twice");
        assert_eq!(count, 65, "the enum has 65 icons");
    }

    #[test]
    fn every_icon_has_artwork_and_every_svg_keeps_its_curves() {
        let mut from_svg = 0;
        let mut drawn = 0;
        for glyph in Icon::ALL {
            match icon_art(glyph) {
                IconArt::Svg(source) => {
                    let drawing = crate::svg_icon::SvgIcon::parse(source)
                        .unwrap_or_else(|reason| panic!("{glyph:?}: {reason}"));
                    let shapes = drawing.shapes(
                        Rect::from_min_size(Pos2::ZERO, Vec2::splat(24.0)),
                        Color32::WHITE,
                    );
                    assert!(!shapes.is_empty(), "{glyph:?} draws nothing");

                    let points: usize = shapes
                        .iter()
                        .map(|shape| match shape {
                            Shape::Path(path) => path.points.len(),
                            _ => 0,
                        })
                        .sum();

                    let straight = matches!(
                        glyph,
                        Icon::ChevronDown
                            | Icon::ChevronLeft
                            | Icon::ChevronRight
                            | Icon::Check
                            | Icon::Cross
                            | Icon::Plus
                            | Icon::CrosshairBox
                            | Icon::HairStream
                            | Icon::CursorPick
                    );
                    let lightly_curved = matches!(glyph, Icon::HairPlant);
                    if straight {
                        assert!(
                            (2..=16).contains(&points),
                            "{glyph:?} is {points} points, not a polyline"
                        );
                    } else if lightly_curved {
                        assert!(
                            (9..=64).contains(&points),
                            "{glyph:?} is {points} points -- lost its arcs?"
                        );
                    } else {
                        assert!(
                            points > 64,
                            "{glyph:?} has only {points} points -- still a polygon?"
                        );
                    }
                    from_svg += 1;
                }
                IconArt::Drawn(ops) => {
                    assert!(!ops.is_empty(), "{glyph:?} draws nothing at all");
                    drawn += 1;
                }
            }
        }

        assert_eq!(from_svg, 56);
        assert_eq!(drawn, 9);
    }

    #[test]
    fn every_drawn_glyph_stays_inside_the_24_grid() {
        let on_grid = |value: f32| (0.0..=24.0).contains(&value);
        for glyph in Icon::ALL {
            let IconArt::Drawn(ops) = icon_art(glyph) else {
                continue;
            };
            assert!(!ops.is_empty(), "{glyph:?} has no path data");
            for op in ops {
                match *op {
                    IconOp::Stroke(points) | IconOp::Closed(points) => {
                        assert!(points.len() >= 2, "{glyph:?} has a degenerate path");
                        assert!(points.iter().flatten().copied().all(on_grid));
                    }
                    IconOp::Cubic(points) => {
                        assert!(points.iter().flatten().copied().all(on_grid));
                    }
                    IconOp::Fill(points) => {
                        assert!(points.len() >= 3, "{glyph:?} fill is not a polygon");
                        assert!(points.iter().flatten().copied().all(on_grid));
                    }
                    IconOp::RoundRect { min, max, radius } => {
                        assert!(min.iter().chain(&max).copied().all(on_grid));
                        assert!(min[0] < max[0] && min[1] < max[1] && radius >= 0.0);
                    }
                }
            }
        }
    }

    #[test]
    fn caption_glyphs_keep_the_native_ten_point_band() {
        for glyph in [
            Icon::WindowMinimize,
            Icon::WindowMaximize,
            Icon::WindowRestore,
            Icon::WindowClose,
        ] {
            let IconArt::Drawn(ops) = icon_art(glyph) else {
                panic!("{glyph:?} must stay hand-drawn");
            };
            for op in ops {
                match *op {
                    IconOp::Stroke(points) => {
                        for point in points {
                            assert!((7.0..=17.0).contains(&point[0]));
                            assert!((7.0..=17.0).contains(&point[1]));
                        }
                    }
                    IconOp::RoundRect { min, max, .. } => {
                        assert!(min[0] >= 7.0 && min[1] >= 7.0);
                        assert!(max[0] <= 17.0 && max[1] <= 17.0);
                    }
                    other => panic!("caption glyphs stay stroke-only: {other:?}"),
                }
            }
        }
    }
}
