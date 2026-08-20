use super::*;

impl TextureProject {
    pub fn tone_match_anchor(&self, layer_id: u64) -> Option<u64> {
        let index = self.layers.iter().position(|layer| layer.id == layer_id)?;
        let rasters = self.baked.as_ref().map(|baked| &baked.layer_rasters);
        self.layers[index + 1..]
            .iter()
            .find(|layer| {
                layer.visible
                    && layer.channel == TextureChannel::Diffuse
                    && (layer.image.is_some()
                        || layer.edited_image.is_some()
                        || rasters.is_some_and(|rasters| rasters.contains_key(&layer.id)))
            })
            .map(|layer| layer.id)
    }

    pub fn match_layer_color_to_previous(&mut self, layer_id: u64) {
        let Some(index) = self.layers.iter().position(|layer| layer.id == layer_id) else {
            return;
        };
        let Some(under) = self.tone_match_anchor(layer_id) else {
            return;
        };

        let rasters = self.baked.as_ref().map(|baked| &baked.layer_rasters);
        let raster = |id: u64| {
            rasters
                .and_then(|rasters| rasters.get(&id))
                .map(|cached| Arc::clone(&cached.image))
        };
        let layer_image = |index: usize| {
            let layer: &TextureLayer = &self.layers[index];
            layer
                .edited_image
                .as_ref()
                .or(layer.image.as_ref())
                .map(Arc::clone)
        };
        let (source_image, target_image, aligned) = match (raster(layer_id), raster(under)) {
            (Some(source), Some(target)) if source.width == target.width => (source, target, true),
            _ => {
                let Some(source) = layer_image(index) else {
                    return;
                };
                let Some(target) = self.layers[index + 1..]
                    .iter()
                    .position(|layer| layer.id == under)
                    .and_then(|offset| layer_image(index + 1 + offset))
                else {
                    return;
                };
                (source, target, false)
            }
        };
        let Some(solved) = solve_tone_match(&source_image, &target_image, aligned) else {
            return;
        };
        let layer = &mut self.layers[index];
        layer.adjustments.exposure = solved.exposure;
        layer.adjustments.saturation = solved.saturation;
        layer.adjustments.temperature = solved.temperature;

        self.mark_dirty();
    }
}
