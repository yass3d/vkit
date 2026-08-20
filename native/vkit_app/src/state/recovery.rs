use super::*;
use crate::session_snapshot::{
    ColorAdjustmentRecord, PinRecord, RunLengthMask, SNAPSHOT_VERSION, SessionSnapshot,
    SparseDisplacement, TextureLayerRecord,
};
use crate::texture_project::{
    TextureChannel, TextureLayer, TexturePinPair, TextureSourceMode, TextureTargetPin,
};
use vkit_core::texture_bake::{TextureBlendMode, TextureColorAdjustments};
use vkit_core::texture_mirror::FaceMirror;

impl AppState {
    pub fn recovery_snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            version: SNAPSHOT_VERSION,
            figure_sex: self.figure_sex,
            look_id: self.selected_vam_edit_source_id.clone(),

            morph_values: self
                .morph_library
                .controls()
                .iter()
                .filter(|control| control.is_modified())
                .map(|control| (control.id.clone(), control.value))
                .collect(),
            eye_closure: self.eye_closure,
            sculpt: self
                .sculpt
                .displacement()
                .map(|dense| SparseDisplacement::from_dense(&dense))
                .unwrap_or_default(),
            texture_layers: self
                .texture_project
                .layers
                .iter()
                .map(layer_record)
                .collect(),
        }
    }

    pub fn restore_recovery(&mut self, snapshot: &SessionSnapshot) -> RecoveryOutcome {
        let mut outcome = RecoveryOutcome::default();
        if self.figure_sex != snapshot.figure_sex {
            self.set_figure_sex(snapshot.figure_sex);
        }

        outcome.texture_layers = self.restore_texture_layers(&snapshot.texture_layers);

        let carry = CarriedEdit {
            morph_values: MorphLibraryValueSnapshot::from_values(snapshot.morph_values.clone()),

            sculpt: snapshot
                .sculpt
                .to_dense(snapshot.sculpt.vertex_count as usize)
                .filter(|dense| !dense.is_empty()),
            eye_closure: snapshot.eye_closure,
        };

        let carry_has_work = !snapshot.morph_values.is_empty()
            || !snapshot.sculpt.is_empty()
            || snapshot.eye_closure != 0.0;

        match snapshot.look_id.as_deref() {
            Some(id) if self.vam_edit_sources.iter().any(|s| s.stable_id == id) => {
                self.select_vam_edit_source(id);

                self.pending_edit_carry = Some(carry);
                outcome.look = true;
                outcome.edits = true;
            }
            Some(_) => {
                if carry_has_work {
                    self.pending_edit_carry = Some(carry);
                }
                outcome.missing_look = true;
            }
            None => {
                if carry_has_work {
                    self.pending_edit_carry = Some(carry);
                    outcome.edits = true;
                    if self.can_enter_detail_from_template()
                        && !self.busy()
                        && !self.tab_available(Tab::Morph)
                    {
                        self.set_edit_source_mode(EditSourceMode::CustomMorph);
                        self.enter_direct_edit(Tab::Morph);
                    }
                }
            }
        }
        outcome
    }

    pub(super) fn answer_recovery(&mut self, restore: bool) {
        let Some(snapshot) = self.pending_recovery.take() else {
            return;
        };
        if !restore {
            return;
        }
        let outcome = self.restore_recovery(&snapshot);
        let (key, tone) = if outcome.missing_look {
            (TextKey::RecoveryLookMissing, StatusTone::Warning)
        } else {
            (TextKey::RecoveryRestored, StatusTone::Info)
        };

        let total_layers = snapshot.texture_layers.len();
        self.status = if outcome.texture_layers < total_layers {
            StatusMessage::with_detail(
                key,
                tone,
                format!("{}/{}", outcome.texture_layers, total_layers),
            )
        } else {
            StatusMessage::new(key, tone)
        };
    }

    fn restore_texture_layers(&mut self, records: &[TextureLayerRecord]) -> usize {
        let mut restored = 0;
        for record in records.iter().rev() {
            let Some(path) = record.source_path.clone() else {
                continue;
            };
            if !path.is_file() {
                continue;
            }

            let id = self
                .texture_project
                .add_image_layer(path, source_mode_from_id(record.source_mode));
            let Some(layer) = self
                .texture_project
                .layers
                .iter_mut()
                .find(|layer| layer.id == id)
            else {
                continue;
            };
            layer.name = record.name.clone();
            layer.channel = channel_from_id(record.channel);
            layer.visible = record.visible;
            layer.opacity = record.opacity;
            layer.blend_mode = blend_mode_from_id(record.blend_mode);
            layer.mirror = mirror_from_id(record.mirror);
            layer.normal_strength = record.normal_strength;
            layer.scalar_invert = record.scalar_invert;
            layer.mask_base = record.mask_base;
            layer.adjustments = adjustments_from(record.adjustments);
            layer.pins = record.pins.iter().map(pin_from).collect();
            layer.mask = record.mask.as_ref().and_then(mask_from);

            layer.invalidate_raster();
            restored += 1;
        }

        self.texture_project.mark_dirty();
        restored
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RecoveryOutcome {
    pub look: bool,
    pub edits: bool,
    pub texture_layers: usize,

    pub missing_look: bool,
}

fn layer_record(layer: &TextureLayer) -> TextureLayerRecord {
    let TextureLayer {
        name,
        source_path,
        source_mode,
        channel,
        visible,
        opacity,
        blend_mode,
        mirror,
        normal_strength,
        scalar_invert,
        mask_base,
        adjustments,
        pins,
        mask,

        image: _,
        edited_image: _,
        mask_preview: _,
        raster_revision: _,

        edited_regions: _,
        painted_regions: _,

        painted: _,

        id: _,
        loading: _,
        load_error: _,
        source_view_zoom: _,
        source_view_center: _,
    } = layer;
    TextureLayerRecord {
        name: name.clone(),
        source_path: source_path.clone(),
        source_mode: source_mode_id(*source_mode),
        channel: channel_id(*channel),
        visible: *visible,
        opacity: *opacity,
        blend_mode: blend_mode_id(*blend_mode),
        mirror: mirror_id(*mirror),
        normal_strength: *normal_strength,
        scalar_invert: *scalar_invert,
        mask_base: *mask_base,
        adjustments: ColorAdjustmentRecord {
            exposure: adjustments.exposure,
            contrast: adjustments.contrast,
            saturation: adjustments.saturation,
            hue_degrees: adjustments.hue_degrees,
            temperature: adjustments.temperature,
        },
        pins: pins.iter().map(pin_record).collect(),
        mask: mask
            .as_ref()
            .map(|mask| RunLengthMask::encode(mask.width, mask.height, &mask.alpha8)),
    }
}

fn adjustments_from(record: ColorAdjustmentRecord) -> TextureColorAdjustments {
    TextureColorAdjustments {
        exposure: record.exposure,
        contrast: record.contrast,
        saturation: record.saturation,
        hue_degrees: record.hue_degrees,
        temperature: record.temperature,
    }
}

fn pin_record(pin: &TexturePinPair) -> PinRecord {
    PinRecord {
        source: pin.source,
        target_triangle: pin.target.map(|target| target.triangle_index),
        target_barycentric: pin
            .target
            .map(|target| target.barycentric)
            .unwrap_or_default(),
        target_uv: pin.target.map(|target| target.uv).unwrap_or_default(),
    }
}

fn pin_from(record: &PinRecord) -> TexturePinPair {
    TexturePinPair {
        source: record.source,
        target: record
            .target_triangle
            .map(|triangle_index| TextureTargetPin {
                triangle_index,
                barycentric: record.target_barycentric,
                uv: record.target_uv,
            }),
    }
}

fn mask_from(record: &RunLengthMask) -> Option<crate::texture_project::TextureLayerMask> {
    let alpha8 = record.decode()?;
    Some(crate::texture_project::TextureLayerMask {
        revision: 0,
        width: record.width,
        height: record.height,
        alpha8: std::sync::Arc::new(alpha8),
    })
}

const fn channel_id(channel: TextureChannel) -> u8 {
    channel as u8
}

fn channel_from_id(id: u8) -> TextureChannel {
    TextureChannel::ALL
        .into_iter()
        .find(|channel| channel_id(*channel) == id)
        .unwrap_or_default()
}

const fn source_mode_id(mode: TextureSourceMode) -> u8 {
    match mode {
        TextureSourceMode::ScanMesh => 0,
        TextureSourceMode::LandmarkPins => 1,
        TextureSourceMode::MaterialUv => 2,
    }
}

const fn source_mode_from_id(id: u8) -> TextureSourceMode {
    match id {
        0 => TextureSourceMode::ScanMesh,
        2 => TextureSourceMode::MaterialUv,
        _ => TextureSourceMode::LandmarkPins,
    }
}

const fn blend_mode_id(mode: TextureBlendMode) -> u8 {
    match mode {
        TextureBlendMode::Normal => 0,
        TextureBlendMode::Multiply => 1,
        TextureBlendMode::Screen => 2,
        TextureBlendMode::Overlay => 3,
    }
}

const fn blend_mode_from_id(id: u8) -> TextureBlendMode {
    match id {
        1 => TextureBlendMode::Multiply,
        2 => TextureBlendMode::Screen,
        3 => TextureBlendMode::Overlay,
        _ => TextureBlendMode::Normal,
    }
}

const fn mirror_id(mirror: FaceMirror) -> u8 {
    match mirror {
        FaceMirror::Off => 0,
        FaceMirror::ToNegativeX => 1,
        FaceMirror::ToPositiveX => 2,
    }
}

const fn mirror_from_id(id: u8) -> FaceMirror {
    match id {
        1 => FaceMirror::ToNegativeX,
        2 => FaceMirror::ToPositiveX,
        _ => FaceMirror::Off,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn every_enum_survives_its_own_numbering() {
        for channel in TextureChannel::ALL {
            assert_eq!(channel_from_id(channel_id(channel)), channel);
        }
        for mode in [
            TextureSourceMode::ScanMesh,
            TextureSourceMode::LandmarkPins,
            TextureSourceMode::MaterialUv,
        ] {
            assert_eq!(source_mode_from_id(source_mode_id(mode)), mode);
        }
        for mode in [
            TextureBlendMode::Normal,
            TextureBlendMode::Multiply,
            TextureBlendMode::Screen,
            TextureBlendMode::Overlay,
        ] {
            assert_eq!(blend_mode_from_id(blend_mode_id(mode)), mode);
        }
        for mirror in [
            FaceMirror::Off,
            FaceMirror::ToNegativeX,
            FaceMirror::ToPositiveX,
        ] {
            assert_eq!(mirror_from_id(mirror_id(mirror)), mirror);
        }
    }

    #[test]
    fn an_unknown_value_falls_back_instead_of_failing() {
        assert_eq!(channel_from_id(200), TextureChannel::default());
        assert_eq!(source_mode_from_id(200), TextureSourceMode::default());
        assert_eq!(blend_mode_from_id(200), TextureBlendMode::Normal);
        assert_eq!(mirror_from_id(200), FaceMirror::Off);
    }

    #[test]
    fn a_fresh_session_snapshots_to_nothing_worth_keeping() {
        let state = AppState::default();
        let snapshot = state.recovery_snapshot();
        assert!(snapshot.is_readable());
        assert!(!snapshot.has_work(), "{snapshot:?}");
    }

    #[test]
    fn a_moved_slider_makes_the_session_worth_saving() {
        let state = AppState {
            eye_closure: 0.4,
            ..AppState::default()
        };
        assert!(state.recovery_snapshot().has_work());
    }

    #[test]
    fn a_look_that_is_gone_is_reported_and_not_faked() {
        let mut state = AppState::default();
        let snapshot = SessionSnapshot {
            version: SNAPSHOT_VERSION,
            look_id: Some("var:Nobody.Nothing.1:/x.vap".to_owned()),
            morph_values: BTreeMap::from([("brow".to_owned(), 0.5)]),
            ..SessionSnapshot::default()
        };
        let outcome = state.restore_recovery(&snapshot);
        assert!(outcome.missing_look);
        assert!(!outcome.look);
        assert!(!outcome.edits);
    }

    #[test]
    fn a_look_less_session_keeps_its_edits_for_the_next_head_install() {
        let mut state = AppState::default();
        let snapshot = SessionSnapshot {
            version: SNAPSHOT_VERSION,
            look_id: None,
            morph_values: BTreeMap::from([("brow".to_owned(), 0.5)]),
            eye_closure: 0.4,
            sculpt: SparseDisplacement::from_dense(&[[1.0, 0.0, 0.0]]),
            ..SessionSnapshot::default()
        };
        let outcome = state.restore_recovery(&snapshot);
        assert!(outcome.edits, "the work is carried, not discarded");
        assert!(
            !outcome.missing_look,
            "no look was ever part of this session"
        );
        assert!(
            state.pending_edit_carry.is_some(),
            "the carry waits for the base head"
        );
    }

    #[test]
    fn a_look_the_catalog_has_not_listed_yet_keeps_the_edits_pending() {
        let mut state = AppState::default();
        let snapshot = SessionSnapshot {
            version: SNAPSHOT_VERSION,
            look_id: Some("var:Author.Look.1:/x.vap".to_owned()),
            morph_values: BTreeMap::from([("brow".to_owned(), 0.5)]),
            ..SessionSnapshot::default()
        };
        let outcome = state.restore_recovery(&snapshot);
        assert!(outcome.missing_look);
        assert!(
            state.pending_edit_carry.is_some(),
            "an unresolved look must not cost the morph and sculpt work"
        );
    }

    #[test]
    fn a_texture_only_look_less_session_is_not_reported_as_a_missing_look() {
        let file = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .expect("a source file");
        let mut state = AppState::default();
        let snapshot = SessionSnapshot {
            version: SNAPSHOT_VERSION,
            look_id: None,
            texture_layers: vec![TextureLayerRecord {
                source_path: Some(file.path().to_path_buf()),
                ..TextureLayerRecord::default()
            }],
            ..SessionSnapshot::default()
        };
        let outcome = state.restore_recovery(&snapshot);
        assert_eq!(outcome.texture_layers, 1);
        assert!(
            !outcome.missing_look,
            "the layers restored fine and there was never a look to miss"
        );
        assert!(state.pending_edit_carry.is_none());
    }

    #[test]
    fn a_pending_recovery_carry_survives_selecting_the_look_once_it_appears() {
        let mut state = AppState {
            pending_edit_carry: Some(CarriedEdit {
                morph_values: MorphLibraryValueSnapshot::from_values(BTreeMap::from([(
                    "brow".to_owned(),
                    0.5,
                )])),
                sculpt: None,
                eye_closure: 0.2,
            }),
            ..AppState::default()
        };
        state.vam_edit_sources.push(VaMEditSource {
            stable_id: "var:Author.Look.1:/x.vap".to_owned(),
            label: "Look".to_owned(),
            path: PathBuf::from("x.vap"),
            sex: None,
            kind: VaMEditSourceKind::MorphPair,
            missing_morphs: 0,
            morph_refs: 0,
        });
        state.select_vam_edit_source("var:Author.Look.1:/x.vap");
        assert!(
            state.pending_edit_carry.is_some(),
            "capturing an empty stage must not destroy the carried session"
        );
    }
}

#[cfg(test)]
mod recovered_layer_kinds {
    use super::*;
    use crate::texture_project::TextureTool;

    #[test]
    fn a_recovered_g2_uv_layer_settles_the_tool_it_allows() {
        let file = tempfile::Builder::new()
            .suffix(".png")
            .tempfile()
            .expect("a source file");
        let record = TextureLayerRecord {
            source_path: Some(file.path().to_path_buf()),
            source_mode: source_mode_id(TextureSourceMode::MaterialUv),
            ..blank_layer_record()
        };

        let mut state = AppState::default();
        state
            .texture_project
            .set_active_tool(TextureTool::CloneStamp);
        let restored = state.restore_texture_layers(std::slice::from_ref(&record));

        assert_eq!(restored, 1);
        let layer = state
            .texture_project
            .selected_layer()
            .expect("the restored layer is selected");
        assert_eq!(layer.source_mode, TextureSourceMode::MaterialUv);
        assert_eq!(
            state.texture_project.active_tool,
            TextureTool::MaskBrush,
            "the tool follows the kind the layer came back as"
        );
    }

    #[test]
    fn recovered_layers_come_back_in_the_stacking_order_they_were_saved_in() {
        let files: Vec<_> = (0..2)
            .map(|_| {
                tempfile::Builder::new()
                    .suffix(".png")
                    .tempfile()
                    .expect("a source file")
            })
            .collect();
        let records: Vec<_> = ["Top", "Bottom"]
            .iter()
            .zip(&files)
            .map(|(name, file)| TextureLayerRecord {
                name: (*name).to_owned(),
                source_path: Some(file.path().to_path_buf()),
                ..blank_layer_record()
            })
            .collect();

        let mut state = AppState::default();
        assert_eq!(state.restore_texture_layers(&records), 2);
        let names: Vec<_> = state
            .texture_project
            .layers
            .iter()
            .map(|layer| layer.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["Top", "Bottom"],
            "index 0 is the top layer, exactly as recorded"
        );
    }

    fn blank_layer_record() -> TextureLayerRecord {
        TextureLayerRecord {
            name: "Recovered".to_owned(),
            source_path: None,
            source_mode: source_mode_id(TextureSourceMode::LandmarkPins),
            channel: channel_id(TextureChannel::Diffuse),
            visible: true,
            opacity: 1.0,
            blend_mode: blend_mode_id(TextureBlendMode::Normal),
            mirror: mirror_id(FaceMirror::Off),
            normal_strength: 1.0,
            scalar_invert: false,
            mask_base: 255,
            adjustments: ColorAdjustmentRecord {
                exposure: 0.0,
                contrast: 0.0,
                saturation: 0.0,
                hue_degrees: 0.0,
                temperature: 0.0,
            },
            pins: Vec::new(),
            mask: None,
        }
    }
}
