use super::*;

impl AppState {
    pub fn add_texture_source_pin(&mut self, point: [f32; 2]) {
        if self.block_mutation_while_busy() || !self.texture_pin_editable() {
            return;
        }
        let undo = self.texture_project.capture_undo_checkpoint();
        self.texture_project.add_source_pin(point);
        self.texture_project.commit_undo_checkpoint(undo);
    }

    pub fn move_texture_source_pin(&mut self, index: usize, point: [f32; 2]) {
        if self.block_mutation_while_busy() || !self.texture_pin_editable() {
            return;
        }
        let undo = self.texture_project.capture_undo_checkpoint();
        self.texture_project.move_source_pin(index, point);
        self.texture_project.commit_undo_checkpoint(undo);
    }

    pub fn add_texture_target_pin(&mut self, triangle_index: u32, barycentric: [f64; 3]) {
        if self.block_mutation_while_busy() || !self.texture_pin_editable() {
            return;
        }
        if let Some(target) = self.texture_target_pin(triangle_index, barycentric) {
            let undo = self.texture_project.capture_undo_checkpoint();
            self.texture_project.add_target_pin(target);
            self.texture_project.commit_undo_checkpoint(undo);
        }
    }

    pub fn move_texture_target_pin(
        &mut self,
        index: usize,
        triangle_index: u32,
        barycentric: [f64; 3],
    ) {
        if self.block_mutation_while_busy() || !self.texture_pin_editable() {
            return;
        }
        if let Some(target) = self.texture_target_pin(triangle_index, barycentric) {
            let undo = self.texture_project.capture_undo_checkpoint();
            self.texture_project.move_target_pin(index, target);
            self.texture_project.commit_undo_checkpoint(undo);
        }
    }

    pub fn remove_texture_pin(&mut self, index: usize) {
        if self.block_mutation_while_busy() || !self.texture_pin_editable() {
            return;
        }
        let undo = self.texture_project.capture_undo_checkpoint();
        self.texture_project.remove_pin(index);
        self.texture_project.commit_undo_checkpoint(undo);
    }

    pub fn can_broadcast_texture_pins(&self) -> bool {
        if !self.texture_pin_editable() {
            return false;
        }
        let Some(source_id) = self.texture_project.selected_layer_id else {
            return false;
        };
        let has_pins = self.texture_project.selected_layer().is_some_and(|layer| {
            layer
                .pins
                .iter()
                .any(|pair| pair.source.is_some() || pair.target.is_some())
        });
        let has_recipient = self
            .texture_project
            .layers
            .iter()
            .any(|layer| layer.id != source_id);
        has_pins && has_recipient
    }

    pub fn broadcast_texture_pins(&mut self) {
        if self.block_mutation_while_busy() || !self.texture_pin_editable() {
            return;
        }
        let undo = self.texture_project.capture_undo_checkpoint();
        self.texture_project.broadcast_pins_from_selected();
        self.texture_project.commit_undo_checkpoint(undo);
    }

    pub fn add_texture_mask_dab_at_surface(
        &mut self,
        triangle_index: u32,
        barycentric: [f64; 3],
        reverse: bool,
    ) {
        if self.block_mutation_while_busy()
            || !self.is_texturing()
            || !self.texture_project.has_editable_layer()
            || self.texture_project.active_tool != TextureTool::MaskBrush
        {
            return;
        }
        let Some((layer_id, subtract)) = self
            .texture_project
            .selected_layer()
            .map(|layer| (layer.id, layer.mask_stroke_subtracts(reverse)))
        else {
            return;
        };
        if let Some(target) = self.texture_target_pin(triangle_index, barycentric) {
            let source = self
                .texture_surface_source_point(triangle_index, barycentric)
                .map(|(_, source)| source);
            let undo = self.texture_project.capture_undo_checkpoint();
            self.texture_project
                .add_mask_dab(layer_id, target.uv, source, subtract);

            if let Some(mirrored) = self.mirrored_mask_uv(triangle_index, barycentric) {
                self.texture_project
                    .add_mask_dab(layer_id, mirrored, source, subtract);
            }
            self.texture_project.commit_undo_checkpoint(undo);
        }
    }

    fn mirrored_mask_uv(&self, triangle_index: u32, barycentric: [f64; 3]) -> Option<[f32; 2]> {
        if self.texture_project.selected_layer()?.mirror
            == vkit_core::texture_mirror::FaceMirror::Off
        {
            return None;
        }
        let mapping = self.vam_uv_mapping.as_deref()?;
        let map = self.face_mirror_map.as_deref()?;
        let index = *self.face_uv_rows.as_deref()?.get(&triangle_index)?;
        map.mirror_uv(mapping, index, barycentric)
    }

    pub(crate) fn rebuild_face_uv_rows(&mut self) {
        self.face_uv_rows = self.vam_uv_mapping.as_deref().map(|mapping| {
            Arc::new(
                mapping
                    .triangles
                    .iter()
                    .enumerate()
                    .filter(|(_, triangle)| {
                        triangle.material_region == vkit_core::vam::UvMaterialRegion::Face
                    })
                    .map(|(row, triangle)| (triangle.canonical_triangle_index, row))
                    .rev()
                    .collect(),
            )
        });
    }

    pub fn add_texture_retouch_dab_at_surface(
        &mut self,
        triangle_index: u32,
        barycentric: [f64; 3],
        reverse: bool,
    ) {
        if self.block_mutation_while_busy()
            || !self.is_texturing()
            || !self.texture_project.has_editable_layer()
        {
            return;
        }
        let tool = self.texture_project.active_tool;
        if !matches!(
            tool,
            TextureTool::CloneStamp | TextureTool::DodgeBurn | TextureTool::Sponge
        ) {
            return;
        }

        let painted_point = self
            .texture_project
            .selected_layer()
            .filter(|layer| layer.painted.is_some())
            .and_then(|layer| {
                self.texture_target_pin(triangle_index, barycentric)
                    .map(|target| (layer.id, [target.uv[0], 1.0 - target.uv[1]]))
            });
        let Some((layer_id, source_point)) = painted_point
            .or_else(|| self.texture_surface_source_point(triangle_index, barycentric))
        else {
            return;
        };
        let undo = self.texture_project.capture_undo_checkpoint();
        self.texture_project
            .apply_retouch_dab(layer_id, tool, source_point, reverse);
        self.texture_project.commit_undo_checkpoint(undo);
    }

    pub fn set_texture_clone_sample_at_surface(
        &mut self,
        triangle_index: u32,
        barycentric: [f64; 3],
    ) {
        if self.block_mutation_while_busy()
            || !self.is_texturing()
            || !matches!(self.texture_project.active_tool, TextureTool::CloneStamp)
        {
            return;
        }

        let painted_point = self
            .texture_project
            .selected_layer()
            .filter(|layer| layer.painted.is_some())
            .and_then(|_| {
                self.texture_target_pin(triangle_index, barycentric)
                    .map(|target| [target.uv[0], 1.0 - target.uv[1]])
            });
        if let Some(point) = painted_point.or_else(|| {
            self.texture_surface_source_point(triangle_index, barycentric)
                .map(|(_, point)| point)
        }) {
            self.texture_project
                .set_clone_sample_on_surface(point, triangle_index, barycentric);
        }
    }

    fn texture_surface_source_point(
        &self,
        triangle_index: u32,
        barycentric: [f64; 3],
    ) -> Option<(u64, [f32; 2])> {
        let target = self.texture_target_pin(triangle_index, barycentric)?;
        let layer = self.texture_project.selected_layer()?;
        let source = match layer.source_mode {
            TextureSourceMode::LandmarkPins => {
                let pins = layer
                    .pins
                    .iter()
                    .filter_map(|pair| {
                        pair.source
                            .zip(pair.target)
                            .map(|(source, target)| TextureWarpPin {
                                source,
                                target_uv: target.uv,
                            })
                    })
                    .collect::<Vec<_>>();
                map_g2_point_to_source(&pins, target.uv).ok().flatten()?
            }

            TextureSourceMode::ScanMesh | TextureSourceMode::MaterialUv => return None,
        };
        Some((layer.id, source))
    }

    fn texture_pin_editable(&self) -> bool {
        self.is_texturing()
            && self.texture_project.has_editable_layer()
            && self.texture_project.active_tool == TextureTool::PinPair
            && self
                .texture_project
                .selected_layer()
                .is_some_and(|layer| layer.source_mode == TextureSourceMode::LandmarkPins)
    }

    fn texture_target_pin(
        &self,
        triangle_index: u32,
        barycentric: [f64; 3],
    ) -> Option<TextureTargetPin> {
        if !barycentric
            .iter()
            .all(|value| value.is_finite() && *value >= -1.0e-6)
        {
            return None;
        }
        let triangle = self
            .vam_uv_mapping
            .as_deref()?
            .triangles
            .iter()
            .find(|triangle| {
                triangle.canonical_triangle_index == triangle_index
                    && triangle.material_region == vkit_core::vam::UvMaterialRegion::Face
            })?;
        let uv = [0, 1].map(|axis| {
            triangle.uvs[0][axis].mul_add(
                barycentric[0] as f32,
                triangle.uvs[1][axis].mul_add(
                    barycentric[1] as f32,
                    triangle.uvs[2][axis] * barycentric[2] as f32,
                ),
            )
        });
        Some(TextureTargetPin {
            triangle_index,
            barycentric,
            uv,
        })
    }
}

#[cfg(test)]
mod mask_mirror_tests {
    use crate::state::AppState;
    use vkit_core::texture_mirror::FaceMirror;

    #[test]
    fn a_mask_dab_follows_the_layer_it_reveals_across_the_face() {
        let mut state = AppState::default();
        let id = state.texture_project.add_image_layer(
            std::path::PathBuf::from("face.png"),
            crate::texture_project::TextureSourceMode::LandmarkPins,
        );
        state.texture_project.select_layer(id);

        let layer = state
            .texture_project
            .selected_layer()
            .expect("the layer just added");
        assert_eq!(
            layer.mirror,
            FaceMirror::Off,
            "a fresh layer paints one side"
        );
        assert!(
            state.mirrored_mask_uv(0, [1.0, 0.0, 0.0]).is_none(),
            "an unmirrored layer has no far side to reach"
        );

        for mirror in [FaceMirror::ToNegativeX, FaceMirror::ToPositiveX] {
            state
                .texture_project
                .layers
                .iter_mut()
                .find(|layer| layer.id == id)
                .expect("the layer")
                .mirror = mirror;

            assert!(
                state.vam_uv_mapping.is_none() && state.face_mirror_map.is_none(),
                "the fixture carries no symmetry, so the mirror declines quietly"
            );
            assert!(state.mirrored_mask_uv(0, [1.0, 0.0, 0.0]).is_none());
        }
    }
}
