use super::*;

impl TextureProject {
    pub fn finish_decode(&mut self, layer_id: u64, outcome: Result<Arc<SkinImage>, String>) {
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == layer_id) else {
            return;
        };
        layer.loading = false;
        match outcome {
            Ok(image) => {
                layer.image = Some(image);
                layer.edited_image = None;
                layer.load_error = None;
            }
            Err(error) => {
                layer.image = None;
                layer.load_error = Some(error);
            }
        }
        layer.invalidate_raster();
        self.mark_dirty();
    }

    pub fn finish_bake(
        &mut self,
        outcome: Result<TextureBakedSet, String>,
        request_is_current: bool,
    ) {
        self.bake_loading = false;
        match outcome {
            Ok(baked) => {
                let first_bake = self.baked.is_none();

                self.adopt_scan_atlases(&baked.layer_rasters, &baked.scan_atlases);
                self.absorb_layer_rasters(&baked.layer_rasters);

                if let Some(base_face) = baked
                    .base_face
                    .as_ref()
                    .filter(|face| face.resolution == PREVIEW_BAKE_RESOLUTION.min(self.resolution))
                {
                    self.base_face = Some(base_face.clone());
                }
                self.baked = Some(baked);
                if first_bake {
                    self.baked_preview_enabled = true;
                }
                self.bake_error = None;
                self.bake_failed_revision = None;
                self.dirty = !request_is_current;
            }
            Err(error) => {
                self.bake_error = Some(error);
                if request_is_current {
                    self.bake_failed_revision = Some(self.edit_revision);
                }
            }
        }
    }

    pub fn cached_base_face(&self, resolution: u32) -> Option<CachedBaseFace> {
        self.base_face
            .as_ref()
            .filter(|cached| cached.resolution == resolution)
            .cloned()
    }

    pub fn cached_layer_rasters(&self, resolution: u32) -> BTreeMap<u64, CachedTextureLayerRaster> {
        self.layer_rasters
            .iter()
            .filter(|((_, cached), _)| *cached == resolution)
            .map(|((id, _), raster)| (*id, raster.clone()))
            .collect()
    }

    pub(super) fn absorb_layer_rasters(&mut self, baked: &BTreeMap<u64, CachedTextureLayerRaster>) {
        let mut rasters = std::mem::take(&mut self.layer_rasters);
        for (id, raster) in baked {
            rasters.insert((*id, raster.resolution), raster.clone());
        }

        let preview = PREVIEW_BAKE_RESOLUTION.min(self.resolution);
        let baked_at = baked.values().map(|raster| raster.resolution).max();
        rasters.retain(|(id, resolution), _| {
            (*resolution == preview || Some(*resolution) == baked_at)
                && self.layers.iter().any(|layer| layer.id == *id)
        });
        self.layer_rasters = rasters;
    }

    pub fn forget_geometry_bound_rasters(&mut self) {
        let mut rasters = std::mem::take(&mut self.layer_rasters);
        rasters.retain(|(id, _), _| {
            !self
                .layers
                .iter()
                .any(|layer| layer.id == *id && layer.source_mode == TextureSourceMode::ScanMesh)
        });
        self.layer_rasters = rasters;
    }

    pub fn forget_layer_rasters(&mut self) {
        self.layer_rasters.clear();
    }

    pub fn baked_layer_raster(&self, layer_id: u64) -> Option<&SkinImage> {
        self.baked
            .as_ref()?
            .layer_rasters
            .get(&layer_id)
            .map(|cached| cached.image.as_ref())
    }

    pub const fn bake_refused_current_edit(&self) -> bool {
        matches!(self.bake_failed_revision, Some(revision) if revision == self.edit_revision)
    }
}
