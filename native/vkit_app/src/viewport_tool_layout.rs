use egui::{Pos2, Rect, Vec2, pos2, vec2};

use crate::state::ViewportToolPanel;
use crate::ui_components::{MINI_POPUP_CONTENT_INSET_X, MINI_POPUP_CONTENT_INSET_Y};

const VIEWPORT_TOOL_INSET: f32 = 12.0;
pub(super) const VIEWPORT_TOOL_SIZE: f32 = 34.0;
pub(super) const VIEWPORT_TOOL_GAP: f32 = 8.0;
const VIEWPORT_TOOL_PANEL_GAP: f32 = 8.0;
const VIEWPORT_TOOL_PANEL_WIDTH: f32 = 258.0;
const VIEWPORT_TOOL_COUNT: usize = 7;
const MIN_PANEL_WIDTH: f32 = 80.0;
const MIN_PANEL_HEIGHT: f32 = 48.0;
const MIN_RIGHT_PANEL_WIDTH: f32 = 120.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ViewportToolPanelPlacement {
    pub(super) origin: Pos2,
    pub(super) width: f32,
    pub(super) available_height: f32,
}

pub(super) fn viewport_tool_button_rect(viewport: Rect, index: usize) -> Option<Rect> {
    let rect = Rect::from_min_size(
        pos2(
            viewport.left() + VIEWPORT_TOOL_INSET,
            viewport.top()
                + VIEWPORT_TOOL_INSET
                + index as f32 * (VIEWPORT_TOOL_SIZE + VIEWPORT_TOOL_GAP),
        ),
        Vec2::splat(VIEWPORT_TOOL_SIZE),
    );
    (rect.left() >= viewport.left()
        && rect.right() <= viewport.right()
        && rect.top() >= viewport.top()
        && rect.bottom() <= viewport.bottom())
    .then_some(rect)
}

pub(super) fn viewport_tool_rail_rect(viewport: Rect) -> Option<Rect> {
    let first = viewport_tool_button_rect(viewport, 0)?;
    let last = (0..VIEWPORT_TOOL_COUNT)
        .filter_map(|index| viewport_tool_button_rect(viewport, index))
        .next_back()?;
    Some(first.union(last))
}

pub(super) const fn viewport_tool_panel_padding_y(panel: ViewportToolPanel) -> Option<f32> {
    match panel {
        ViewportToolPanel::Lighting
        | ViewportToolPanel::Camera
        | ViewportToolPanel::BaseView
        | ViewportToolPanel::Wireframe
        | ViewportToolPanel::Xray
        | ViewportToolPanel::Skin
        | ViewportToolPanel::Hair => Some(MINI_POPUP_CONTENT_INSET_Y),
    }
}

pub(super) const fn viewport_tool_panel_padding_x() -> f32 {
    MINI_POPUP_CONTENT_INSET_X
}

pub(super) fn viewport_tool_panel_placement(
    viewport: Rect,
    panel: ViewportToolPanel,
) -> Option<ViewportToolPanelPlacement> {
    viewport_tool_panel_padding_y(panel)?;
    let inner = viewport.shrink(VIEWPORT_TOOL_INSET);
    let rail = viewport_tool_rail_rect(viewport)?;
    let right_origin = pos2(rail.right() + VIEWPORT_TOOL_PANEL_GAP, inner.top());
    let right_width = (inner.right() - right_origin.x).max(0.0);
    let (origin, width, available_height) = if right_width >= MIN_RIGHT_PANEL_WIDTH {
        (
            right_origin,
            right_width.min(VIEWPORT_TOOL_PANEL_WIDTH),
            inner.height(),
        )
    } else {
        let below_origin = pos2(inner.left(), rail.bottom() + VIEWPORT_TOOL_PANEL_GAP);
        (
            below_origin,
            inner.width(),
            (inner.bottom() - below_origin.y).max(0.0),
        )
    };
    (width >= MIN_PANEL_WIDTH && available_height >= MIN_PANEL_HEIGHT).then_some(
        ViewportToolPanelPlacement {
            origin,
            width,
            available_height,
        },
    )
}

pub(super) fn viewport_tool_panel_rect_with_height(
    viewport: Rect,
    panel: ViewportToolPanel,
    height: f32,
) -> Option<Rect> {
    let placement = viewport_tool_panel_placement(viewport, panel)?;
    (height >= MIN_PANEL_HEIGHT && height <= placement.available_height)
        .then(|| Rect::from_min_size(placement.origin, vec2(placement.width, height)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rail_uses_fixed_inset_and_spacing() {
        let viewport = Rect::from_min_size(pos2(20.0, 30.0), vec2(600.0, 500.0));
        let first = viewport_tool_button_rect(viewport, 0).unwrap();
        let second = viewport_tool_button_rect(viewport, 1).unwrap();
        assert_eq!(first.min, pos2(32.0, 42.0));
        assert_eq!(first.size(), Vec2::splat(VIEWPORT_TOOL_SIZE));
        assert_eq!(second.top() - first.bottom(), VIEWPORT_TOOL_GAP);
    }

    #[test]
    fn panel_is_content_height_bounded_by_viewport() {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(480.0, 520.0));
        let rect = viewport_tool_panel_rect_with_height(viewport, ViewportToolPanel::Camera, 180.0)
            .unwrap();
        assert_eq!(rect.height(), 180.0);
        assert!(viewport.contains(rect.min) && viewport.contains(rect.max));
        assert!(
            viewport_tool_panel_rect_with_height(viewport, ViewportToolPanel::Camera, 800.0,)
                .is_none()
        );
    }

    #[test]
    fn background_compatibility_slot_uses_shared_popup_insets() {
        let viewport = Rect::from_min_size(pos2(0.0, 0.0), vec2(480.0, 520.0));
        assert!(viewport_tool_panel_placement(viewport, ViewportToolPanel::BaseView).is_some());
        assert_eq!(
            viewport_tool_panel_padding_y(ViewportToolPanel::BaseView),
            Some(MINI_POPUP_CONTENT_INSET_Y)
        );
        assert_eq!(viewport_tool_panel_padding_x(), MINI_POPUP_CONTENT_INSET_X);
    }
}
