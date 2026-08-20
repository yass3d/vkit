use super::*;

impl TextureProject {
    #[must_use]
    pub fn tool_usable(&self, tool: TextureTool) -> bool {
        let Some(layer) = self.selected_layer() else {
            return true;
        };
        layer.source_mode.allows(tool)
            && (!tool.needs_warp()
                || layer.source_mode != TextureSourceMode::LandmarkPins
                || layer.landmark_warp_ready())
    }

    #[must_use]
    pub fn usable_tools(&self) -> Vec<TextureTool> {
        self.selected_layer()
            .map_or(TextureSourceMode::default().available_tools(), |layer| {
                layer.source_mode.available_tools()
            })
            .iter()
            .copied()
            .filter(|tool| self.tool_usable(*tool))
            .collect()
    }

    pub fn set_active_tool(&mut self, tool: TextureTool) {
        self.active_tool = if self.tool_usable(tool) {
            tool
        } else {
            self.usable_tools().first().copied().unwrap_or_default()
        };
    }

    pub fn projection_stencil(&self) -> bool {
        self.active_tool == TextureTool::Projection
    }

    pub fn stencil_needs_fresh_placement(&self) -> bool {
        self.projection_placed_for != self.selected_layer_id
    }

    pub fn centre_projection_stencil(&mut self) {
        self.projection_placement = StencilPlacement::default();
        self.projection_placed_for = self.selected_layer_id;
    }

    pub fn place_projection_stencil(&mut self, placement: StencilPlacement) {
        self.projection_placement = placement;
        self.projection_placed_for = self.selected_layer_id;
    }
}
