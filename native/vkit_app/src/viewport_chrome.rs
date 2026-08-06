#![cfg_attr(
    not(test),
    expect(dead_code, reason = "the anchor vocabulary leads the migration")
)]

use egui::{Rect, Vec2, pos2, vec2};

pub const EDGE_INSET: f32 = 12.0;

pub const STACK_GAP: f32 = 8.0;

fn safe_area(viewport: Rect) -> Rect {
    let inset = viewport.shrink(EDGE_INSET);
    Rect::from_min_max(
        inset.min,
        pos2(
            inset.right(),
            (inset.bottom() - crate::theme::STATUS_BAR_HEIGHT).max(inset.top()),
        ),
    )
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ChromeAnchor {
    TopLeft,
    TopCentre,
    TopRight,
    LeftCentre,
    RightCentre,
    BottomLeft,
    BottomCentre,
    BottomRight,
}

impl ChromeAnchor {
    pub const ALL: [Self; 8] = [
        Self::TopLeft,
        Self::TopCentre,
        Self::TopRight,
        Self::LeftCentre,
        Self::RightCentre,
        Self::BottomLeft,
        Self::BottomCentre,
        Self::BottomRight,
    ];

    pub(crate) const fn grows_down(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopCentre | Self::TopRight)
    }

    pub(crate) const fn is_side(self) -> bool {
        matches!(self, Self::LeftCentre | Self::RightCentre)
    }
}

pub fn slot(viewport: Rect, anchor: ChromeAnchor, size: Vec2, ordinal: usize) -> Option<Rect> {
    let safe = safe_area(viewport);
    if size.x <= 0.0 || size.y <= 0.0 || safe.width() < size.x || safe.height() < size.y {
        return None;
    }
    let x = match anchor {
        ChromeAnchor::TopLeft | ChromeAnchor::LeftCentre | ChromeAnchor::BottomLeft => safe.left(),
        ChromeAnchor::TopCentre | ChromeAnchor::BottomCentre => safe.center().x - size.x * 0.5,
        ChromeAnchor::TopRight | ChromeAnchor::RightCentre | ChromeAnchor::BottomRight => {
            safe.right() - size.x
        }
    };

    let step = (size.y + STACK_GAP) * ordinal as f32;
    let y = if anchor.is_side() {
        safe.center().y - size.y * 0.5 + step
    } else if anchor.grows_down() {
        safe.top() + step
    } else {
        safe.bottom() - size.y - step
    };
    let rect = Rect::from_min_size(pos2(x, y), size);
    safe.contains_rect(rect).then_some(rect)
}

#[derive(Clone, Debug)]
pub struct ViewportChrome {
    viewport: Rect,
    claims: Vec<(ChromeAnchor, Rect)>,
}

impl ViewportChrome {
    pub fn new(viewport: Rect) -> Self {
        Self {
            viewport,
            claims: Vec::new(),
        }
    }

    pub fn safe_area(&self) -> Rect {
        safe_area(self.viewport)
    }

    pub fn claim(&mut self, anchor: ChromeAnchor, size: Vec2) -> Option<Rect> {
        let safe = self.safe_area();
        if size.x <= 0.0 || size.y <= 0.0 || safe.width() < size.x || safe.height() < size.y {
            return None;
        }
        let x = match anchor {
            ChromeAnchor::TopLeft | ChromeAnchor::LeftCentre | ChromeAnchor::BottomLeft => {
                safe.left()
            }
            ChromeAnchor::TopCentre | ChromeAnchor::BottomCentre => safe.center().x - size.x * 0.5,
            ChromeAnchor::TopRight | ChromeAnchor::RightCentre | ChromeAnchor::BottomRight => {
                safe.right() - size.x
            }
        };
        let offset = self.stacked_offset(anchor);
        let y = if anchor.is_side() {
            safe.center().y - size.y * 0.5 + offset
        } else if anchor.grows_down() {
            safe.top() + offset
        } else {
            safe.bottom() - size.y - offset
        };
        let rect = Rect::from_min_size(pos2(x, y), size);
        if !safe.contains_rect(rect) {
            return None;
        }

        if self.claims.iter().any(|(_, taken)| taken.intersects(rect)) {
            return None;
        }
        self.claims.push((anchor, rect));
        Some(rect)
    }

    fn stacked_offset(&self, anchor: ChromeAnchor) -> f32 {
        let used: f32 = self
            .claims
            .iter()
            .filter(|(claimed, _)| *claimed == anchor)
            .map(|(_, rect)| rect.height() + STACK_GAP)
            .sum();
        used
    }

    pub fn claimed(&self) -> impl Iterator<Item = Rect> + '_ {
        self.claims.iter().map(|(_, rect)| *rect)
    }

    pub fn covers(&self, point: egui::Pos2) -> bool {
        self.claims.iter().any(|(_, rect)| rect.contains(point))
    }
}

pub fn size(width: f32, height: f32) -> Vec2 {
    vec2(width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0))
    }

    #[test]
    fn nothing_at_a_bottom_anchor_reaches_the_status_strip() {
        let viewport = viewport();
        let strip = Rect::from_min_max(
            pos2(
                viewport.left(),
                viewport.bottom() - crate::theme::STATUS_BAR_HEIGHT,
            ),
            viewport.max,
        );
        for anchor in [
            ChromeAnchor::BottomLeft,
            ChromeAnchor::BottomCentre,
            ChromeAnchor::BottomRight,
        ] {
            let rect = slot(viewport, anchor, size(28.0, 28.0), 0).unwrap();
            assert!(
                !rect.intersects(strip),
                "{anchor:?} {rect:?} is on the strip"
            );
        }
    }

    #[test]
    fn nothing_claimed_ever_overlaps_anything_else_claimed() {
        let mut chrome = ViewportChrome::new(viewport());
        let mut rects = Vec::new();

        for anchor in ChromeAnchor::ALL {
            for height in [40.0_f32, 28.0, 64.0] {
                if let Some(rect) = chrome.claim(anchor, size(180.0, height)) {
                    rects.push((anchor, rect));
                }
            }
        }
        assert!(rects.len() >= 16, "only {} claims fit", rects.len());
        for (index, (anchor, rect)) in rects.iter().enumerate() {
            for (other_anchor, other) in &rects[index + 1..] {
                assert!(
                    !rect.intersects(*other),
                    "{anchor:?} {rect:?} overlaps {other_anchor:?} {other:?}"
                );
            }
        }
    }

    #[test]
    fn a_stack_grows_inward_from_its_own_edge() {
        let mut chrome = ViewportChrome::new(viewport());
        let first = chrome
            .claim(ChromeAnchor::BottomCentre, size(200.0, 40.0))
            .unwrap();
        let second = chrome
            .claim(ChromeAnchor::BottomCentre, size(200.0, 40.0))
            .unwrap();
        assert!(second.bottom() < first.top(), "{second:?} then {first:?}");

        let mut chrome = ViewportChrome::new(viewport());
        let first = chrome
            .claim(ChromeAnchor::TopRight, size(200.0, 40.0))
            .unwrap();
        let second = chrome
            .claim(ChromeAnchor::TopRight, size(200.0, 40.0))
            .unwrap();
        assert!(second.top() > first.bottom(), "{second:?} then {first:?}");
    }

    #[test]
    fn nothing_reaches_the_viewport_edge() {
        let mut chrome = ViewportChrome::new(viewport());
        let safe = chrome.safe_area();
        for anchor in ChromeAnchor::ALL {
            let rect = chrome.claim(anchor, size(120.0, 36.0)).unwrap();
            assert!(safe.contains_rect(rect), "{anchor:?} escaped: {rect:?}");
        }
        assert_eq!(safe.left() - viewport().left(), EDGE_INSET);
    }

    #[test]
    fn a_viewport_with_no_room_refuses_the_claim() {
        let mut chrome = ViewportChrome::new(Rect::from_min_size(pos2(0.0, 0.0), vec2(40.0, 40.0)));
        assert_eq!(chrome.claim(ChromeAnchor::TopLeft, size(200.0, 40.0)), None);

        let mut chrome = ViewportChrome::new(viewport());
        let mut fitted = 0;
        for _ in 0..40 {
            if chrome
                .claim(ChromeAnchor::TopLeft, size(100.0, 100.0))
                .is_some()
            {
                fitted += 1;
            }
        }
        assert!(fitted > 0 && fitted < 40, "{fitted} of 40 fitted");
    }

    #[test]
    fn a_degenerate_size_is_refused() {
        let mut chrome = ViewportChrome::new(viewport());
        for bad in [size(0.0, 40.0), size(120.0, 0.0), size(-10.0, 40.0)] {
            assert_eq!(chrome.claim(ChromeAnchor::TopLeft, bad), None, "{bad:?}");
        }
    }

    #[test]
    fn the_chrome_answers_for_every_overlay_at_once() {
        let mut chrome = ViewportChrome::new(viewport());
        let badge = chrome
            .claim(ChromeAnchor::BottomCentre, size(200.0, 40.0))
            .unwrap();
        let gizmo = chrome
            .claim(ChromeAnchor::TopRight, size(52.0, 60.0))
            .unwrap();
        assert!(chrome.covers(badge.center()));
        assert!(chrome.covers(gizmo.center()));
        assert!(!chrome.covers(viewport().center()));
        assert_eq!(chrome.claimed().count(), 2);
    }
}
