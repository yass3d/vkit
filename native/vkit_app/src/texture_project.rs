use std::{
    any::Any,
    collections::{BTreeMap, VecDeque},
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use image::ImageReader;
use vkit_core::texture_mirror::{FaceMirror, FaceMirrorMap};
use vkit_core::{
    formats::{MtlDocument, ObjDocument, OrderedObjMesh},
    pixels::{RgbaView, pack_surface_map, resize_rgba_box},
    sculpt::SculptFalloff,
    texture_bake::{
        AlphaMaskView, TextureBakeImage, TextureBakeOptions, TextureBlendMode,
        TextureColorAdjustments, TextureWarpPin, apply_color_adjustments,
        bake_projected_texture_to_g2, composite_rgba, composite_rgba_masked,
        place_image_on_g2_uv_in_region, resize_direct_uv, warp_image_to_g2_by_pins_in_region,
    },
    texture_transfer::{TextureTransferOptions, TextureTransferOutcome, transfer_texture_to_g2},
    vam::{G2UvMapping, UvMaterialRegion},
};

use crate::{
    scene::ModelTransform,
    skin_preview::{
        SkinChannel, SkinCorner, SkinImage, SkinPreview, SkinPreviewGeometry, SkinSurfaceMap,
        SkinTriangle,
    },
    state::FigureSex,
};

const MAX_TEXTURE_SOURCE_BYTES: usize = 256 * 1024 * 1024;
const MAX_TEXTURE_SOURCE_EDGE: u32 = 8192;
const MASK_PREVIEW_EDGE: u32 = 256;
const MASK_PREVIEW_MAX_ALPHA: u8 = 148;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureContainer {
    Png,

    Jpeg,
}

impl TextureContainer {
    pub const JPEG_QUALITY: u8 = 98;

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum TextureChannel {
    #[default]
    Diffuse,
    Normal,
    Roughness,
    Metallic,
    Glossiness,
    Smoothness,
    Specular,
    Mask,
}

impl TextureChannel {
    pub const ALL: [Self; 8] = [
        Self::Diffuse,
        Self::Normal,
        Self::Roughness,
        Self::Metallic,
        Self::Glossiness,
        Self::Smoothness,
        Self::Specular,
        Self::Mask,
    ];

    pub const fn export_container(self) -> TextureContainer {
        match self {
            Self::Diffuse => TextureContainer::Jpeg,
            Self::Normal
            | Self::Roughness
            | Self::Metallic
            | Self::Glossiness
            | Self::Smoothness
            | Self::Specular
            | Self::Mask => TextureContainer::Png,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Diffuse => "Diffuse",
            Self::Normal => "Normal",
            Self::Roughness => "Roughness",
            Self::Metallic => "Metallic",
            Self::Glossiness => "Glossiness",
            Self::Smoothness => "Smoothness",
            Self::Specular => "Specular",
            Self::Mask => "Mask",
        }
    }

    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Diffuse => "_diffuse",
            Self::Normal => "_normal",
            Self::Roughness => "_roughness",
            Self::Metallic => "_metallic",
            Self::Glossiness => "_gloss",
            Self::Smoothness => "_smoothness",
            Self::Specular => "_specular",
            Self::Mask => "_mask",
        }
    }

    pub const fn is_color(self) -> bool {
        matches!(self, Self::Diffuse)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextureSourceMode {
    ScanMesh,

    #[default]
    LandmarkPins,

    MaterialUv,
}

impl TextureSourceMode {
    #[must_use]
    pub fn available_tools(self) -> &'static [TextureTool] {
        match self {
            Self::MaterialUv => &[TextureTool::MaskBrush],
            Self::ScanMesh => &[
                TextureTool::MaskBrush,
                TextureTool::CloneStamp,
                TextureTool::Heal,
                TextureTool::DodgeBurn,
                TextureTool::Sponge,
            ],
            Self::LandmarkPins => &[
                TextureTool::PinPair,
                TextureTool::Projection,
                TextureTool::MaskBrush,
                TextureTool::CloneStamp,
                TextureTool::Heal,
                TextureTool::DodgeBurn,
                TextureTool::Sponge,
            ],
        }
    }

    #[must_use]
    pub fn allows(self, tool: TextureTool) -> bool {
        self.available_tools().contains(&tool)
    }

    #[must_use]
    pub const fn badge(self) -> Option<crate::ui_components::Icon> {
        match self {
            Self::MaterialUv => Some(crate::ui_components::Icon::Wireframe),
            Self::ScanMesh | Self::LandmarkPins => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TexturePbrConvention {
    #[default]
    MetallicRoughness,
    GlossinessSmoothness,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextureBakeBase {
    #[default]
    Transparent,
    CurrentSkin,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextureTool {
    #[default]
    PinPair,

    Projection,
    MaskBrush,
    CloneStamp,
    Heal,
    DodgeBurn,
    Sponge,
}

impl TextureTool {
    #[cfg(test)]
    pub const ALL: [Self; 7] = [
        Self::PinPair,
        Self::Projection,
        Self::MaskBrush,
        Self::CloneStamp,
        Self::Heal,
        Self::DodgeBurn,
        Self::Sponge,
    ];

    pub const fn is_paint_brush(self) -> bool {
        matches!(
            self,
            Self::MaskBrush | Self::CloneStamp | Self::Heal | Self::DodgeBurn | Self::Sponge
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureTargetPin {
    pub triangle_index: u32,
    pub barycentric: [f64; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TexturePinPair {
    pub source: Option<[f32; 2]>,
    pub target: Option<TextureTargetPin>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TextureMaskDab {
    pub uv: [f32; 2],

    pub radius: f32,
    pub falloff: SculptFalloff,
    pub opacity: f32,

    pub add: bool,

    pub source: Option<[f32; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StencilPlacement {
    pub offset: [f32; 2],

    pub scale: f32,

    pub rotation: f32,
}

impl Default for StencilPlacement {
    fn default() -> Self {
        Self {
            offset: [0.0; 2],
            scale: 1.0,
            rotation: 0.0,
        }
    }
}

impl StencilPlacement {
    pub const SCALE_RANGE: std::ops::RangeInclusive<f32> = 0.1..=8.0;

    #[must_use]
    pub fn panned(self, delta: [f32; 2]) -> Self {
        Self {
            offset: [self.offset[0] + delta[0], self.offset[1] + delta[1]],
            ..self
        }
    }

    #[must_use]
    pub fn zoomed(self, factor: f32, about: [f32; 2], centre: [f32; 2]) -> Self {
        let scale =
            (self.scale * factor).clamp(*Self::SCALE_RANGE.start(), *Self::SCALE_RANGE.end());
        let applied = scale / self.scale;
        let anchor = [
            about[0] - centre[0] - self.offset[0],
            about[1] - centre[1] - self.offset[1],
        ];
        Self {
            offset: [
                self.offset[0] + anchor[0] * (1.0 - applied),
                self.offset[1] + anchor[1] * (1.0 - applied),
            ],
            scale,
            ..self
        }
    }

    #[must_use]
    pub fn rotated(self, radians: f32) -> Self {
        Self {
            rotation: self.rotation + radians,
            ..self
        }
    }

    #[must_use]
    pub fn source_at(self, screen: [f32; 2], centre: [f32; 2], size: [f32; 2]) -> Option<[f32; 2]> {
        let local = [
            screen[0] - centre[0] - self.offset[0],
            screen[1] - centre[1] - self.offset[1],
        ];
        let (sine, cosine) = (-self.rotation).sin_cos();
        let rotated = [
            local[0] * cosine - local[1] * sine,
            local[0] * sine + local[1] * cosine,
        ];
        let half = [size[0] * self.scale * 0.5, size[1] * self.scale * 0.5];
        if half[0] <= 0.0 || half[1] <= 0.0 {
            return None;
        }
        let uv = [
            rotated[0] / (half[0] * 2.0) + 0.5,
            rotated[1] / (half[1] * 2.0) + 0.5,
        ];

        const EDGE_SLACK: f32 = 1.0e-3;
        let inside = uv
            .iter()
            .all(|value| (-EDGE_SLACK..=1.0 + EDGE_SLACK).contains(value));
        inside.then(|| uv.map(|value| value.clamp(0.0, 1.0)))
    }
}

#[derive(Clone, Debug)]
pub struct TextureLayerPaint {
    pub width: u32,
    pub height: u32,

    pub rgba8: Arc<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextureLayerMask {
    pub revision: u64,
    pub width: u32,
    pub height: u32,

    pub alpha8: Arc<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct TextureLayer {
    pub id: u64,
    pub name: String,
    pub source_path: Option<PathBuf>,
    pub source_mode: TextureSourceMode,
    pub channel: TextureChannel,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: TextureBlendMode,
    pub adjustments: TextureColorAdjustments,

    pub mirror: FaceMirror,
    pub normal_strength: f32,
    pub scalar_invert: bool,
    pub pins: Vec<TexturePinPair>,
    pub mask_base: u8,
    pub mask: Option<TextureLayerMask>,

    pub painted: Option<TextureLayerPaint>,

    pub mask_preview: Option<Arc<SkinImage>>,
    pub image: Option<Arc<SkinImage>>,

    pub edited_image: Option<Arc<SkinImage>>,

    pub edited_regions: VecDeque<(u64, [u32; 4])>,
    pub source_view_zoom: f32,
    pub source_view_center: [f32; 2],
    pub loading: bool,
    pub load_error: Option<String>,

    pub raster_revision: u64,
}

static RASTER_CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl TextureLayer {
    pub fn invalidate_raster(&mut self) {
        self.raster_revision = RASTER_CLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    fn image(id: u64, path: PathBuf, source_mode: TextureSourceMode) -> Self {
        Self {
            id,
            name: format!("Layer {id}"),
            source_path: Some(path),
            source_mode,
            channel: TextureChannel::Diffuse,
            visible: true,
            opacity: 1.0,
            blend_mode: TextureBlendMode::Normal,
            adjustments: TextureColorAdjustments::default(),
            mirror: FaceMirror::Off,
            normal_strength: 1.0,
            scalar_invert: false,
            pins: Vec::new(),
            mask_base: 255,
            mask: None,
            painted: None,
            mask_preview: None,
            image: None,
            edited_image: None,
            edited_regions: VecDeque::new(),
            source_view_zoom: 1.0,
            source_view_center: [0.5, 0.5],
            loading: true,
            load_error: None,
            raster_revision: 0,
        }
    }

    pub fn pin_pair_invalid(&self, index: usize) -> bool {
        let Some(pair) = self.pins.get(index) else {
            return true;
        };
        let Some((source, target)) = pair.source.zip(pair.target) else {
            return true;
        };
        if !source
            .into_iter()
            .chain(target.uv)
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        {
            return true;
        }
        self.pins.iter().enumerate().any(|(other_index, other)| {
            if other_index == index {
                return false;
            }
            let Some((other_source, other_target)) = other.source.zip(other.target) else {
                return false;
            };
            squared_point_distance(source, other_source) <= 1.0e-12
                || squared_point_distance(target.uv, other_target.uv) <= 1.0e-12
        })
    }

    pub fn landmark_warp_ready(&self) -> bool {
        let complete = self
            .pins
            .iter()
            .enumerate()
            .filter_map(|(index, pair)| {
                pair.source
                    .zip(pair.target)
                    .map(|(source, target)| (index, source, target.uv))
            })
            .collect::<Vec<_>>();
        if complete.len() < 3
            || complete
                .iter()
                .any(|(index, _, _)| self.pin_pair_invalid(*index))
        {
            return false;
        }
        for first in 0..complete.len() - 2 {
            for second in first + 1..complete.len() - 1 {
                for third in second + 1..complete.len() {
                    let source_area =
                        signed_point_area(complete[first].1, complete[second].1, complete[third].1);
                    let target_area =
                        signed_point_area(complete[first].2, complete[second].2, complete[third].2);
                    if source_area.abs() > 1.0e-8 && target_area.abs() > 1.0e-8 {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub const fn mask_stroke_subtracts(&self, reverse: bool) -> bool {
        (self.mask_base != 0) ^ reverse
    }

    fn scan(id: u64) -> Self {
        Self {
            id,
            name: format!("Layer {id}"),
            source_path: None,
            source_mode: TextureSourceMode::ScanMesh,
            channel: TextureChannel::Diffuse,
            visible: true,
            opacity: 1.0,
            blend_mode: TextureBlendMode::Normal,
            adjustments: TextureColorAdjustments::default(),
            mirror: FaceMirror::Off,
            normal_strength: 1.0,
            scalar_invert: false,
            pins: Vec::new(),
            mask_base: 255,
            mask: None,
            painted: None,
            mask_preview: None,
            image: None,
            edited_image: None,
            edited_regions: VecDeque::new(),
            source_view_zoom: 1.0,
            source_view_center: [0.5, 0.5],
            loading: false,
            load_error: None,
            raster_revision: 0,
        }
    }
}

fn squared_point_distance(left: [f32; 2], right: [f32; 2]) -> f32 {
    (left[0] - right[0]).mul_add(
        left[0] - right[0],
        (left[1] - right[1]) * (left[1] - right[1]),
    )
}

fn signed_point_area(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]).mul_add(c[1] - a[1], -(b[1] - a[1]) * (c[0] - a[0]))
}

#[derive(Clone, Debug)]
pub struct TextureBakedSet {
    pub request_id: u64,

    pub source_revision: u64,
    pub images: BTreeMap<TextureChannel, Arc<SkinImage>>,
    pub preview: Arc<SkinPreview>,

    pub layer_rasters: BTreeMap<u64, CachedTextureLayerRaster>,

    pub scan_atlases: BTreeMap<u64, Arc<SkinImage>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextureBakeQuality {
    Preview,
    Export,
}

pub const TEXTURE_RESOLUTIONS: [u32; 2] = [2048, 4096];

#[must_use]
pub fn is_texture_resolution(edge: u32) -> bool {
    TEXTURE_RESOLUTIONS.contains(&edge)
}

#[must_use]
pub fn normalize_texture_resolution(edge: u32) -> u32 {
    TEXTURE_RESOLUTIONS
        .into_iter()
        .min_by_key(|candidate| candidate.abs_diff(edge))
        .unwrap_or(2048)
}

pub const PREVIEW_BAKE_RESOLUTION: u32 = 1024;

#[must_use]
pub fn is_bakeable_resolution(edge: u32) -> bool {
    edge == PREVIEW_BAKE_RESOLUTION || is_texture_resolution(edge)
}

#[derive(Clone, Debug)]
pub struct CachedTextureLayerRaster {
    pub mirror: FaceMirror,
    pub raster_revision: u64,
    pub resolution: u32,
    pub boundary_feather_pixels: u16,
    pub image: Arc<SkinImage>,
}

#[derive(Clone, Debug)]
pub struct TextureExportSnapshot {
    pub directory: PathBuf,
    pub prefix: String,
    pub pbr_convention: TexturePbrConvention,
    pub images: BTreeMap<TextureChannel, Arc<SkinImage>>,
}

const TEXTURE_UNDO_LIMIT: usize = 8;

#[derive(Clone, Debug)]
pub(crate) struct TextureUndoSnapshot {
    layers: Vec<TextureLayer>,
    selected_layer_id: Option<u64>,
    resolution: u32,
    boundary_feather_pixels: u16,
    bake_base: TextureBakeBase,
    source_revision: u64,
}

pub const MAX_BOUNDARY_FEATHER_FRACTION: f32 = 0.25;

#[derive(Clone, Debug)]
pub struct TextureProject {
    pub active_tool: TextureTool,
    pub layers: Vec<TextureLayer>,
    pub selected_layer_id: Option<u64>,
    pub resolution: u32,
    pub boundary_feather_pixels: u16,
    pub bake_base: TextureBakeBase,
    pub output_pbr: TexturePbrConvention,
    pub workspace_split_ratio: f32,
    pub mask_brush_radius: f32,
    pub mask_brush_falloff: SculptFalloff,
    pub mask_brush_opacity: f32,
    pub mask_preview_enabled: bool,
    pub pin_opacity: f32,
    pub export_subfolder: String,
    pub export_prefix: String,
    pub bake_loading: bool,
    pub bake_error: Option<String>,
    pub dirty: bool,
    pub baked: Option<TextureBakedSet>,
    pub baked_preview_enabled: bool,

    pub hide_vam_skin_preview: bool,
    pub clone_sample: Option<[f32; 2]>,

    clone_offset: Option<[f32; 2]>,

    stroke: Option<StrokeCoverage>,

    preview_stroke: Option<StrokeCoverage>,

    pub baked_resolution: u32,

    pub bake_queued: Option<TextureBakeQuality>,

    bake_failed_revision: Option<u64>,

    pub projection_stencil: bool,

    pub projection_placement: StencilPlacement,

    pub projection_opacity: f32,

    pub retouch_reverse: bool,
    edit_revision: u64,
    next_layer_id: u64,
    undo_history: VecDeque<TextureUndoSnapshot>,
    undo_transaction: Option<TextureUndoSnapshot>,
}

impl Default for TextureProject {
    fn default() -> Self {
        Self {
            active_tool: TextureTool::PinPair,
            layers: Vec::new(),
            selected_layer_id: None,
            resolution: 2048,

            boundary_feather_pixels: (2048.0 * MAX_BOUNDARY_FEATHER_FRACTION * 0.5) as u16,
            bake_base: TextureBakeBase::Transparent,

            output_pbr: TexturePbrConvention::GlossinessSmoothness,
            workspace_split_ratio: 0.5,
            mask_brush_radius: 0.035,
            mask_brush_falloff: SculptFalloff::Smooth,
            mask_brush_opacity: 0.55,
            mask_preview_enabled: false,
            pin_opacity: 1.0,

            export_subfolder: String::new(),
            export_prefix: String::new(),
            bake_loading: false,
            bake_error: None,
            dirty: false,
            baked: None,
            baked_preview_enabled: true,
            hide_vam_skin_preview: false,
            clone_sample: None,
            clone_offset: None,
            stroke: None,
            preview_stroke: None,
            baked_resolution: 0,
            bake_queued: None,
            bake_failed_revision: None,
            projection_stencil: false,
            projection_placement: StencilPlacement::default(),
            projection_opacity: 0.55,
            retouch_reverse: false,
            edit_revision: 0,
            next_layer_id: 1,
            undo_history: VecDeque::new(),
            undo_transaction: None,
        }
    }
}

impl TextureProject {
    fn undo_snapshot(&self) -> TextureUndoSnapshot {
        TextureUndoSnapshot {
            layers: self.layers.clone(),
            selected_layer_id: self.selected_layer_id,
            resolution: self.resolution,
            boundary_feather_pixels: self.boundary_feather_pixels,
            bake_base: self.bake_base,
            source_revision: self.edit_revision,
        }
    }

    pub(crate) fn capture_undo_checkpoint(&self) -> Option<TextureUndoSnapshot> {
        self.undo_transaction
            .is_none()
            .then(|| self.undo_snapshot())
    }

    pub(crate) fn commit_undo_checkpoint(&mut self, checkpoint: Option<TextureUndoSnapshot>) {
        let Some(checkpoint) = checkpoint else {
            return;
        };
        if checkpoint.source_revision != self.edit_revision {
            self.push_undo(checkpoint);
        }
    }

    pub fn begin_undo_transaction(&mut self) {
        if self.undo_transaction.is_none() {
            self.undo_transaction = Some(self.undo_snapshot());
        }
    }

    pub fn end_undo_transaction(&mut self) {
        let checkpoint = self.undo_transaction.take();
        self.commit_undo_checkpoint(checkpoint);

        self.stroke = None;
        self.preview_stroke = None;
    }

    pub const fn edit_transaction_active(&self) -> bool {
        self.undo_transaction.is_some()
    }

    pub fn undo(&mut self) -> bool {
        self.end_undo_transaction();
        let Some(snapshot) = self.undo_history.pop_back() else {
            return false;
        };
        self.layers = snapshot.layers;
        self.selected_layer_id = snapshot
            .selected_layer_id
            .filter(|id| self.layers.iter().any(|layer| layer.id == *id))
            .or_else(|| self.layers.first().map(|layer| layer.id));
        self.resolution = normalize_texture_resolution(snapshot.resolution);
        self.boundary_feather_pixels = snapshot.boundary_feather_pixels;
        self.bake_base = snapshot.bake_base;
        self.edit_revision = self.edit_revision.saturating_add(1);
        self.dirty = true;
        self.bake_error = None;
        true
    }

    fn push_undo(&mut self, snapshot: TextureUndoSnapshot) {
        if self.undo_history.len() == TEXTURE_UNDO_LIMIT {
            self.undo_history.pop_front();
        }
        self.undo_history.push_back(snapshot);
    }

    pub fn max_boundary_feather_pixels(&self) -> u16 {
        let deepest = (self.resolution as f32 * MAX_BOUNDARY_FEATHER_FRACTION).round();
        deepest.clamp(1.0, f32::from(u16::MAX)) as u16
    }

    pub fn selected_layer(&self) -> Option<&TextureLayer> {
        let id = self.selected_layer_id?;
        self.layers.iter().find(|layer| layer.id == id)
    }

    pub fn selected_layer_mut(&mut self) -> Option<&mut TextureLayer> {
        let id = self.selected_layer_id?;
        self.layers.iter_mut().find(|layer| layer.id == id)
    }

    pub fn add_image_layer(&mut self, path: PathBuf, source_mode: TextureSourceMode) -> u64 {
        let id = self.take_layer_id();
        self.layers
            .insert(0, TextureLayer::image(id, path, source_mode));
        self.selected_layer_id = Some(id);
        self.set_active_tool(self.active_tool);
        self.mark_dirty();
        id
    }

    pub fn ensure_scan_layer(&mut self, name: String) -> Option<u64> {
        if let Some(existing) = self
            .layers
            .iter()
            .find(|layer| layer.source_mode == TextureSourceMode::ScanMesh)
        {
            let id = existing.id;
            self.invalidate_scan_projection();
            return Some(id);
        }
        let id = self.take_layer_id();
        let mut layer = TextureLayer::scan(id);
        layer.name = name;
        self.layers.insert(0, layer);
        self.selected_layer_id.get_or_insert(id);
        self.mark_dirty();
        Some(id)
    }

    fn adopt_scan_atlases(
        &mut self,
        rasters: &BTreeMap<u64, CachedTextureLayerRaster>,
        unmirrored: &BTreeMap<u64, Arc<SkinImage>>,
    ) {
        for layer in &mut self.layers {
            if layer.source_mode != TextureSourceMode::ScanMesh || layer.image.is_some() {
                continue;
            }
            let atlas = unmirrored.get(&layer.id).cloned().or_else(|| {
                rasters
                    .get(&layer.id)
                    .map(|raster| Arc::clone(&raster.image))
            });
            if let Some(atlas) = atlas {
                layer.image = Some(atlas);
            }
        }
    }

    pub fn invalidate_scan_projection(&mut self) {
        let mut touched = false;
        for layer in &mut self.layers {
            if layer.source_mode == TextureSourceMode::ScanMesh {
                layer.invalidate_raster();

                layer.image = None;
                layer.edited_image = None;
                touched = true;
            }
        }
        if touched {
            self.mark_dirty();
        }
    }

    pub fn remove_layer(&mut self, id: u64) -> bool {
        let Some(index) = self.layers.iter().position(|layer| layer.id == id) else {
            return false;
        };
        self.layers.remove(index);
        if self.selected_layer_id == Some(id) {
            self.selected_layer_id = self
                .layers
                .get(index.min(self.layers.len().saturating_sub(1)))
                .map(|layer| layer.id);
        }

        self.set_active_tool(self.active_tool);
        if !self.has_editable_layer() && self.active_tool == TextureTool::Projection {
            self.set_active_tool(TextureTool::PinPair);
        }

        if self.active_tool == TextureTool::PinPair
            && self
                .selected_layer()
                .is_some_and(|layer| layer.painted.is_some())
        {
            self.set_active_tool(TextureTool::Projection);
        }
        self.mark_dirty();
        true
    }

    pub fn select_layer(&mut self, id: u64) {
        if self.layers.iter().any(|layer| layer.id == id) {
            self.selected_layer_id = Some(id);

            self.set_active_tool(self.active_tool);

            if self.active_tool == TextureTool::PinPair
                && self
                    .selected_layer()
                    .is_some_and(|layer| layer.painted.is_some())
            {
                self.set_active_tool(TextureTool::Projection);
            }
        }
    }

    pub fn set_active_tool(&mut self, tool: TextureTool) {
        let allowed = self
            .selected_layer()
            .map(|layer| layer.source_mode)
            .is_none_or(|mode| mode.allows(tool));
        self.active_tool = if allowed {
            tool
        } else {
            self.selected_layer()
                .and_then(|layer| layer.source_mode.available_tools().first().copied())
                .unwrap_or_default()
        };

        self.mask_preview_enabled = self.active_tool == TextureTool::MaskBrush;

        self.projection_stencil = self.active_tool == TextureTool::Projection;
    }

    pub fn mark_dirty(&mut self) {
        self.edit_revision = self.edit_revision.saturating_add(1);
        self.dirty = true;
        self.bake_error = None;
    }

    pub const fn edit_revision(&self) -> u64 {
        self.edit_revision
    }

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

    pub fn add_source_pin(&mut self, point: [f32; 2]) {
        let Some(layer) = self.selected_layer_mut() else {
            return;
        };
        if let Some(pair) = layer.pins.iter_mut().find(|pair| pair.source.is_none()) {
            pair.source = Some(point);
        } else {
            layer.pins.push(TexturePinPair {
                source: Some(point),
                target: None,
            });
        }
        layer.invalidate_raster();
        self.mark_dirty();
    }

    pub fn stamp_projection(
        &mut self,
        source: &SkinImage,
        triangles: &[vkit_core::texture_bake::ProjectedTriangle],
        brush: vkit_core::texture_bake::ProjectionBrush,
        to_source: impl Fn([f32; 2]) -> Option<[f32; 2]>,
    ) -> usize {
        let edge = self.resolution;

        let view =
            match vkit_core::pixels::RgbaView::new(&source.rgba8, source.width, source.height) {
                Ok(view) => view,
                Err(error) => {
                    self.bake_error = Some(format!("projection source is unusable: {error}"));
                    return 0;
                }
            };
        let Some(layer) = self.selected_layer_mut() else {
            return 0;
        };

        if layer.source_mode != TextureSourceMode::LandmarkPins {
            return 0;
        }

        let painted = match layer.painted.as_mut() {
            Some(paint) => {
                let (width, height) = (paint.width, paint.height);
                let rgba8 = Arc::make_mut(&mut paint.rgba8);
                vkit_core::texture_bake::stamp_projection_onto_g2(
                    rgba8.as_mut_slice(),
                    width,
                    height,
                    view,
                    triangles,
                    brush,
                    to_source,
                )
            }
            None => {
                let mut rgba8 = vec![0; edge as usize * edge as usize * 4];
                let painted = vkit_core::texture_bake::stamp_projection_onto_g2(
                    rgba8.as_mut_slice(),
                    edge,
                    edge,
                    view,
                    triangles,
                    brush,
                    to_source,
                );
                if painted > 0 {
                    layer.painted = Some(TextureLayerPaint {
                        width: edge,
                        height: edge,
                        rgba8: Arc::new(rgba8),
                    });
                }
                painted
            }
        };
        if painted == 0 {
            return 0;
        }

        layer.pins.clear();
        layer.invalidate_raster();
        self.mark_dirty();
        painted
    }

    pub fn add_target_pin(&mut self, target: TextureTargetPin) {
        let Some(layer) = self.selected_layer_mut() else {
            return;
        };
        if let Some(pair) = layer.pins.iter_mut().find(|pair| pair.target.is_none()) {
            pair.target = Some(target);
        } else {
            layer.pins.push(TexturePinPair {
                source: None,
                target: Some(target),
            });
        }

        layer.painted = None;
        layer.invalidate_raster();
        self.mark_dirty();
    }

    pub fn move_source_pin(&mut self, index: usize, point: [f32; 2]) {
        if let Some(layer) = self.selected_layer_mut()
            && let Some(pair) = layer.pins.get_mut(index)
        {
            pair.source = Some(point);

            layer.painted = None;
            layer.invalidate_raster();
            self.mark_dirty();
        }
    }

    pub fn move_target_pin(&mut self, index: usize, target: TextureTargetPin) {
        if let Some(layer) = self.selected_layer_mut()
            && let Some(pair) = layer.pins.get_mut(index)
        {
            pair.target = Some(target);

            layer.painted = None;
            layer.invalidate_raster();
            self.mark_dirty();
        }
    }

    pub fn remove_pin(&mut self, index: usize) {
        if let Some(layer) = self.selected_layer_mut()
            && index < layer.pins.len()
        {
            layer.pins.remove(index);
            layer.invalidate_raster();
            self.mark_dirty();
        }
    }

    pub fn broadcast_pins_from_selected(&mut self) -> usize {
        let Some(source_id) = self.selected_layer_id else {
            return 0;
        };
        let Some(pins) = self
            .layers
            .iter()
            .find(|layer| layer.id == source_id)
            .map(|layer| layer.pins.clone())
        else {
            return 0;
        };
        if pins.is_empty() {
            return 0;
        }
        let mut copied = 0;
        for layer in &mut self.layers {
            if layer.id == source_id {
                continue;
            }
            layer.pins = pins.clone();

            layer.painted = None;
            layer.invalidate_raster();
            copied += 1;
        }
        if copied > 0 {
            self.mark_dirty();
        }
        copied
    }

    pub fn add_mask_dab(
        &mut self,
        layer_id: u64,
        uv: [f32; 2],
        source: Option<[f32; 2]>,
        subtract: bool,
    ) {
        if !uv
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            || source.is_some_and(|point| {
                !point
                    .into_iter()
                    .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
            })
        {
            return;
        }
        let radius = self.mask_brush_radius.clamp(0.002, 0.25);
        let falloff = self.mask_brush_falloff;
        let opacity = self.mask_brush_opacity.clamp(0.01, 1.0);
        let mask_edge = self.resolution;
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == layer_id) else {
            return;
        };
        let dab = TextureMaskDab {
            uv,
            radius,
            falloff,
            opacity,
            add: !subtract,
            source,
        };

        let coverage = stroke_coverage(
            &mut self.stroke,
            layer_id,
            TextureTool::MaskBrush,
            RasterSize {
                width: mask_edge,
                height: mask_edge,
            },
        );
        if source.is_some() {
            apply_mask_preview_dab(layer, dab, &mut self.preview_stroke);
        }
        apply_layer_mask_dab(layer, mask_edge, dab, coverage);
        self.mark_dirty();
    }

    pub fn clear_mask(&mut self, layer_id: u64) {
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == layer_id) else {
            return;
        };
        if layer.mask.take().is_some() {
            reset_mask_preview(layer);
            self.mark_dirty();
        }
    }

    pub fn move_layer_to(&mut self, id: u64, insertion_index: usize) {
        let Some(index) = self.layers.iter().position(|layer| layer.id == id) else {
            return;
        };
        let layer = self.layers.remove(index);
        let adjusted = insertion_index
            .saturating_sub(usize::from(index < insertion_index))
            .min(self.layers.len());
        self.layers.insert(adjusted, layer);
        if adjusted != index {
            self.mark_dirty();
        }
    }

    pub fn set_source_view(&mut self, id: u64, zoom: f32, center: [f32; 2]) {
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == id) else {
            return;
        };
        if !zoom.is_finite() || !center.into_iter().all(f32::is_finite) {
            return;
        }
        layer.source_view_zoom = zoom.clamp(1.0, 32.0);
        layer.source_view_center = center.map(|value| value.clamp(0.0, 1.0));
    }

    pub fn set_clone_sample(&mut self, point: [f32; 2]) {
        if point
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        {
            self.clone_sample = Some(point);
            self.clone_offset = None;
        }
    }

    pub fn apply_retouch_dab(
        &mut self,
        layer_id: u64,
        tool: TextureTool,
        point: [f32; 2],
        reverse: bool,
    ) {
        if !matches!(
            tool,
            TextureTool::CloneStamp
                | TextureTool::Heal
                | TextureTool::DodgeBurn
                | TextureTool::Sponge
        ) || !point
            .into_iter()
            .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        {
            return;
        }
        let radius = self.mask_brush_radius.clamp(0.002, 0.25);
        let falloff = self.mask_brush_falloff;
        let opacity = self.mask_brush_opacity.clamp(0.01, 1.0);

        let source_offset = matches!(tool, TextureTool::CloneStamp | TextureTool::Heal)
            .then(|| {
                let offset = self.clone_offset.or_else(|| {
                    self.clone_sample
                        .map(|sample| [point[0] - sample[0], point[1] - sample[1]])
                })?;
                self.clone_offset = Some(offset);
                Some(offset)
            })
            .flatten();
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == layer_id) else {
            return;
        };

        if let Some(paint) = layer.painted.as_mut() {
            let size = RasterSize {
                width: paint.width,
                height: paint.height,
            };
            let coverage = stroke_coverage(&mut self.stroke, layer_id, tool, size);
            let rgba8 = Arc::make_mut(&mut paint.rgba8);
            apply_retouch_pixels(
                rgba8,
                size,
                RetouchStroke {
                    tool,
                    point,
                    clone_offset: source_offset,
                    reverse,
                },
                BrushDab {
                    radius,
                    falloff,
                    opacity,
                },
                coverage,
            );
            layer.invalidate_raster();
            self.mark_dirty();
            return;
        }
        if layer.image.is_none() {
            return;
        }
        if layer.edited_image.is_none() {
            layer.edited_image = layer.image.as_ref().map(|image| {
                Arc::new(SkinImage {
                    revision: image.revision.wrapping_add(1),
                    width: image.width,
                    height: image.height,
                    rgba8: Arc::new(image.rgba8.as_ref().clone()),
                    uv_orientation: image.uv_orientation,
                })
            });
        }
        let Some(image) = layer.edited_image.as_mut() else {
            return;
        };
        let image = Arc::make_mut(image);
        let size = RasterSize {
            width: image.width,
            height: image.height,
        };
        let coverage = stroke_coverage(&mut self.stroke, layer_id, tool, size);
        let rgba8 = Arc::make_mut(&mut image.rgba8);
        let touched = apply_retouch_pixels(
            rgba8,
            size,
            RetouchStroke {
                tool,
                point,
                clone_offset: source_offset,
                reverse,
            },
            BrushDab {
                radius,
                falloff,
                opacity,
            },
            coverage,
        );
        image.revision = image.revision.wrapping_add(1);
        let revision = image.revision;
        if let Some(touched) = touched {
            layer.edited_regions.push_back((revision, touched));
            while layer.edited_regions.len() > EDITED_REGION_HISTORY {
                layer.edited_regions.pop_front();
            }
        } else {
            layer.edited_regions.clear();
        }
        layer.invalidate_raster();
        self.mark_dirty();
    }

    pub fn reset_layer(&mut self, layer_id: u64) {
        let Some(layer) = self.layers.iter_mut().find(|layer| layer.id == layer_id) else {
            return;
        };
        layer.edited_image = None;
        layer.painted = None;
        layer.mask = None;
        layer.mask_preview = None;
        layer.pins.clear();
        layer.adjustments = TextureColorAdjustments::default();
        layer.opacity = 1.0;
        layer.blend_mode = TextureBlendMode::Normal;
        layer.normal_strength = 1.0;
        layer.scalar_invert = false;
        layer.invalidate_raster();
        self.mark_dirty();
    }

    pub fn match_layer_color_to_previous(&mut self, layer_id: u64) {
        let Some(index) = self.layers.iter().position(|layer| layer.id == layer_id) else {
            return;
        };
        let Some(under) = self.layers[index + 1..]
            .iter()
            .find(|layer| layer.visible && layer.channel == TextureChannel::Diffuse)
            .map(|layer| layer.id)
        else {
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
        layer.invalidate_raster();
        self.mark_dirty();
    }

    pub fn discard_bake(&mut self) {
        self.baked = None;
        self.baked_resolution = 0;
        self.bake_error = None;
        self.bake_failed_revision = None;
        self.dirty = false;
    }

    pub fn has_editable_layer(&self) -> bool {
        self.selected_layer().is_some()
    }

    pub fn baked_preview(&self) -> Option<Arc<SkinPreview>> {
        self.baked_preview_enabled
            .then(|| self.baked.as_ref().map(|baked| Arc::clone(&baked.preview)))
            .flatten()
    }

    pub fn default_export_directory(
        &self,
        vam_root: Option<&Path>,
        figure_sex: FigureSex,
    ) -> Option<PathBuf> {
        let root = vam_root?;

        Some(
            root.join("Custom")
                .join("Atom")
                .join("Person")
                .join("Textures")
                .join(sanitize_component(
                    &self.export_subfolder,
                    default_texture_subfolder(figure_sex),
                )),
        )
    }

    fn take_layer_id(&mut self) -> u64 {
        let id = self.next_layer_id;
        self.next_layer_id = self.next_layer_id.saturating_add(1);
        id
    }
}

#[derive(Clone, Debug)]
pub struct ScanTextureBakeSource {
    pub document: Arc<ObjDocument>,
    pub materials: Arc<MtlDocument>,
    pub source_obj_path: PathBuf,
    pub transform: ModelTransform,
    pub symmetry_applied: bool,
}

#[derive(Clone, Debug)]
pub struct TextureLayerBakeInput {
    pub id: u64,
    pub name: String,
    pub source_mode: TextureSourceMode,
    pub channel: TextureChannel,
    pub visible: bool,
    pub opacity: f32,
    pub blend_mode: TextureBlendMode,
    pub adjustments: TextureColorAdjustments,

    pub mirror: FaceMirror,
    pub normal_strength: f32,
    pub scalar_invert: bool,
    pub pins: Vec<TexturePinPair>,
    pub mask_base: u8,
    pub mask: Option<TextureLayerMask>,

    pub painted: Option<TextureLayerPaint>,
    pub raster_revision: u64,
    pub image: Option<Arc<SkinImage>>,
}

impl From<&TextureLayer> for TextureLayerBakeInput {
    fn from(layer: &TextureLayer) -> Self {
        Self {
            id: layer.id,
            name: layer.name.clone(),
            source_mode: layer.source_mode,
            channel: layer.channel,
            visible: layer.visible,
            opacity: layer.opacity,
            blend_mode: layer.blend_mode,
            adjustments: layer.adjustments,
            mirror: layer.mirror,
            normal_strength: layer.normal_strength,
            scalar_invert: layer.scalar_invert,
            pins: layer.pins.clone(),
            mask_base: layer.mask_base,
            mask: layer.mask.clone(),
            painted: layer.painted.clone(),
            raster_revision: layer.raster_revision,
            image: layer
                .edited_image
                .as_ref()
                .or(layer.image.as_ref())
                .map(Arc::clone),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TextureBakeRequest {
    pub request_id: u64,
    pub project_revision: u64,
    pub layers: Vec<TextureLayerBakeInput>,
    pub target: Arc<OrderedObjMesh>,
    pub mapping: Arc<G2UvMapping>,

    pub face_mirror: Option<Arc<FaceMirrorMap>>,
    pub scan: Option<ScanTextureBakeSource>,
    pub base_preview: Option<Arc<SkinPreview>>,
    pub bake_base: TextureBakeBase,

    pub hide_skin_preview: bool,

    pub neutral_base_rgb: [u8; 3],
    pub resolution: u32,
    pub boundary_feather_pixels: u16,
    pub cached_layer_rasters: BTreeMap<u64, CachedTextureLayerRaster>,

    pub base_face_source: Option<vkit_core::vam::AssetLocator>,
}

#[derive(Clone, Debug)]
pub struct TextureDecodeRequest {
    pub request_id: u64,
    pub layer_id: u64,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub enum TextureWorkRequest {
    Decode(TextureDecodeRequest),

    Bake(Box<TextureBakeRequest>),
}

impl TextureWorkRequest {
    pub const fn request_id(&self) -> u64 {
        match self {
            Self::Decode(request) => request.request_id,
            Self::Bake(request) => request.request_id,
        }
    }
}

#[derive(Debug)]
pub enum TextureWorkerEvent {
    DecodeFinished {
        request_id: u64,
        layer_id: u64,
        outcome: Result<Arc<SkinImage>, String>,
    },
    BakeFinished {
        request_id: u64,
        outcome: Result<TextureBakedSet, String>,
    },
}

#[derive(Debug)]
pub struct TextureProjectCoordinator {
    sender: Sender<TextureWorkerEvent>,
    receiver: Receiver<TextureWorkerEvent>,
    active_request: Option<u64>,
}

impl Default for TextureProjectCoordinator {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver,
            active_request: None,
        }
    }
}

impl TextureProjectCoordinator {
    pub fn is_active(&self) -> bool {
        self.active_request.is_some()
    }

    pub fn start(
        &mut self,
        request: TextureWorkRequest,
        wake: impl Fn() + Send + 'static,
    ) -> Result<(), String> {
        if self.is_active() {
            return Err("a texture worker is already active".to_owned());
        }
        let request_id = request.request_id();
        let decode_layer_id = match &request {
            TextureWorkRequest::Decode(request) => Some(request.layer_id),
            TextureWorkRequest::Bake(_) => None,
        };
        let sender = self.sender.clone();
        thread::Builder::new()
            .name(format!("vkit-texture-{request_id}"))
            .spawn(move || {
                let event =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match request {
                        TextureWorkRequest::Decode(request) => TextureWorkerEvent::DecodeFinished {
                            request_id: request.request_id,
                            layer_id: request.layer_id,
                            outcome: decode_texture_path(request.request_id, &request.path),
                        },
                        TextureWorkRequest::Bake(request) => TextureWorkerEvent::BakeFinished {
                            request_id: request.request_id,
                            outcome: bake_texture_project(&request),
                        },
                    }))
                    .unwrap_or_else(|payload| {
                        let error = texture_panic_detail(payload);
                        if let Some(layer_id) = decode_layer_id {
                            TextureWorkerEvent::DecodeFinished {
                                request_id,
                                layer_id,
                                outcome: Err(error),
                            }
                        } else {
                            TextureWorkerEvent::BakeFinished {
                                request_id,
                                outcome: Err(error),
                            }
                        }
                    });
                let _ = sender.send(event);
                wake();
            })
            .map_err(|error| format!("failed to start texture worker: {error}"))?;
        self.active_request = Some(request_id);
        Ok(())
    }

    pub fn drain(&mut self) -> Vec<TextureWorkerEvent> {
        let events: Vec<_> = self.receiver.try_iter().collect();
        if events.iter().any(|event| match event {
            TextureWorkerEvent::DecodeFinished { request_id, .. }
            | TextureWorkerEvent::BakeFinished { request_id, .. } => {
                self.active_request == Some(*request_id)
            }
        }) {
            self.active_request = None;
        }
        events
    }
}

#[must_use]
pub const fn default_texture_subfolder(figure_sex: FigureSex) -> &'static str {
    match figure_sex {
        FigureSex::Female => "FemaleBase",
        FigureSex::Male => "MaleBase",
    }
}

pub fn texture_export_filename(prefix: &str, channel: TextureChannel) -> String {
    format!(
        "{}{}.{}",
        sanitize_component(prefix, "texture"),
        channel.suffix(),
        channel.export_container().extension()
    )
}

pub fn texture_export_images(
    images: &BTreeMap<TextureChannel, Arc<SkinImage>>,
    convention: TexturePbrConvention,
) -> BTreeMap<TextureChannel, Arc<SkinImage>> {
    let mut exported = images.clone();
    match convention {
        TexturePbrConvention::MetallicRoughness
            if !exported.contains_key(&TextureChannel::Roughness) =>
        {
            let source = exported
                .get(&TextureChannel::Smoothness)
                .or_else(|| exported.get(&TextureChannel::Glossiness));
            if let Some(source) = source {
                exported.insert(
                    TextureChannel::Roughness,
                    Arc::new(invert_scalar_image(source)),
                );
            }
        }
        TexturePbrConvention::GlossinessSmoothness
            if !exported.contains_key(&TextureChannel::Glossiness)
                && !exported.contains_key(&TextureChannel::Smoothness) =>
        {
            if let Some(source) = exported.get(&TextureChannel::Roughness) {
                exported.insert(
                    TextureChannel::Glossiness,
                    Arc::new(invert_scalar_image(source)),
                );
            }
        }
        _ => {}
    }
    exported
}

fn invert_scalar_image(source: &SkinImage) -> SkinImage {
    let mut rgba8 = source.rgba8.as_ref().clone();
    for pixel in rgba8.chunks_exact_mut(4) {
        pixel[0] = 255_u8.saturating_sub(pixel[0]);
        pixel[1] = 255_u8.saturating_sub(pixel[1]);
        pixel[2] = 255_u8.saturating_sub(pixel[2]);
    }
    SkinImage {
        revision: source.revision.wrapping_add(1),
        width: source.width,
        height: source.height,
        rgba8: Arc::new(rgba8),
        uv_orientation: source.uv_orientation,
    }
}

pub fn sanitize_component(value: &str, fallback: &str) -> String {
    let value = value
        .trim()
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
            {
                '_'
            } else {
                character
            }
        })
        .take(64)
        .collect::<String>();
    let value = value.trim_matches(|character: char| character == '.' || character.is_whitespace());
    if value.is_empty() {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RasterSize {
    width: u32,
    height: u32,
}

impl From<u32> for RasterSize {
    fn from(edge: u32) -> Self {
        Self {
            width: edge,
            height: edge,
        }
    }
}

impl RasterSize {
    fn fits(self, rgba8: &[u8]) -> bool {
        self.width != 0
            && self.height != 0
            && rgba8.len() == self.width as usize * self.height as usize * 4
    }
}

#[derive(Clone, Copy, Debug)]
struct BrushDab {
    radius: f32,
    falloff: SculptFalloff,
    opacity: f32,
}

#[derive(Clone, Debug)]
struct StrokeCoverage {
    layer_id: u64,
    tool: TextureTool,
    width: u32,
    height: u32,
    applied: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
struct StrokeStep {
    blend: f32,

    delta: f32,
}

impl StrokeCoverage {
    fn matches(&self, layer_id: u64, tool: TextureTool, size: RasterSize) -> bool {
        self.layer_id == layer_id
            && self.tool == tool
            && self.width == size.width
            && self.height == size.height
    }

    fn new(layer_id: u64, tool: TextureTool, size: RasterSize) -> Self {
        Self {
            layer_id,
            tool,
            width: size.width,
            height: size.height,
            applied: vec![0; size.width as usize * size.height as usize],
        }
    }

    fn advance(&mut self, index: usize, coverage: f32) -> Option<StrokeStep> {
        let slot = self.applied.get_mut(index)?;
        let previous = f32::from(*slot) / 255.0;
        let coverage = coverage.clamp(0.0, 1.0);
        if coverage <= previous {
            return None;
        }
        *slot = (coverage * 255.0).round().clamp(0.0, 255.0) as u8;
        Some(StrokeStep {
            blend: ((coverage - previous) / (1.0 - previous)).clamp(0.0, 1.0),
            delta: coverage - previous,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct RetouchStroke {
    tool: TextureTool,

    point: [f32; 2],

    clone_offset: Option<[f32; 2]>,

    reverse: bool,
}

fn apply_retouch_pixels(
    rgba8: &mut [u8],
    size: RasterSize,
    stroke: RetouchStroke,
    dab: BrushDab,
    coverage: &mut StrokeCoverage,
) -> Option<[u32; 4]> {
    let RasterSize { width, height } = size;
    let RetouchStroke {
        tool,
        point,
        clone_offset,
        reverse,
    } = stroke;
    let BrushDab {
        radius: radius_normalized,
        falloff,
        opacity,
    } = dab;
    if !size.fits(rgba8) || coverage.applied.len() != width as usize * height as usize {
        return None;
    }
    let span = [
        width.saturating_sub(1) as f32,
        height.saturating_sub(1) as f32,
    ];
    let center = [
        point[0].clamp(0.0, 1.0) * span[0],
        point[1].clamp(0.0, 1.0) * span[1],
    ];
    let radius = (radius_normalized.clamp(0.002, 0.25)
        * width.min(height).saturating_sub(1) as f32)
        .max(1.0);
    let min_x = (center[0] - radius).floor().max(0.0) as u32;
    let max_x = (center[0] + radius).ceil().min(span[0]) as u32;
    let min_y = (center[1] - radius).floor().max(0.0) as u32;
    let max_y = (center[1] + radius).ceil().min(span[1]) as u32;

    let source_offset = clone_offset.map(|offset| [offset[0] * span[0], offset[1] * span[1]]);
    if tool == TextureTool::CloneStamp && source_offset.is_none() {
        return None;
    }
    let correction = (tool == TextureTool::Heal).then(|| {
        heal_correction(
            rgba8,
            size,
            center,
            radius,
            source_offset,
            [min_x, min_y, max_x, max_y],
        )
    });

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let distance = (x as f32 - center[0]).hypot(y as f32 - center[1]);
            if distance > radius {
                continue;
            }
            let index = y as usize * width as usize + x as usize;
            let falloff_weight = falloff.weight(f64::from(distance / radius)) as f32;
            let Some(step) = coverage.advance(index, falloff_weight * opacity.clamp(0.0, 1.0))
            else {
                continue;
            };
            let offset = index * 4;
            match tool {
                TextureTool::CloneStamp => {
                    let Some(source_offset) = source_offset else {
                        continue;
                    };
                    let source = sample_bilinear(
                        rgba8,
                        size,
                        x as f32 - source_offset[0],
                        y as f32 - source_offset[1],
                    );
                    blend_source_over(&mut rgba8[offset..offset + 4], source, step.blend);
                }
                TextureTool::Heal => {
                    let Some(correction) = correction.as_ref() else {
                        continue;
                    };
                    let target = correction.resolve(
                        rgba8,
                        size,
                        x as f32,
                        y as f32,
                        source_offset,
                        [x as usize - min_x as usize, y as usize - min_y as usize],
                    );
                    for channel in 0..3 {
                        rgba8[offset + channel] =
                            lerp_u8(rgba8[offset + channel], target[channel], step.blend);
                    }
                }
                TextureTool::DodgeBurn => {
                    let linear = [0, 1, 2].map(|channel| srgb_to_linear(rgba8[offset + channel]));
                    let luminance =
                        linear[0].mul_add(0.2126, linear[1].mul_add(0.7152, linear[2] * 0.0722));
                    let midtone = 1.0 - (2.0 * luminance - 1.0).abs();

                    let amount = step.delta * DODGE_BURN_STOPS * midtone.mul_add(0.65, 0.35);

                    let gain = if reverse {
                        (-amount).exp2()
                    } else {
                        amount.exp2()
                    };
                    for channel in 0..3 {
                        rgba8[offset + channel] = linear_to_srgb(linear[channel] * gain);
                    }
                }
                TextureTool::Sponge => {
                    let rgb = [
                        f32::from(rgba8[offset]),
                        f32::from(rgba8[offset + 1]),
                        f32::from(rgba8[offset + 2]),
                    ];
                    let luminance = rgb[0].mul_add(0.2126, rgb[1].mul_add(0.7152, rgb[2] * 0.0722));

                    let saturation = if reverse { 0.0 } else { SPONGE_SATURATION_GAIN };
                    for channel in 0..3 {
                        let target =
                            (luminance + (rgb[channel] - luminance) * saturation).clamp(0.0, 255.0);
                        rgba8[offset + channel] =
                            lerp_u8(rgba8[offset + channel], target.round() as u8, step.blend);
                    }
                }
                _ => {}
            }
        }
    }
    Some([min_x, min_y, max_x, max_y])
}

fn blend_source_over(destination: &mut [u8], source: [f32; 4], strength: f32) {
    let coverage = (source[3] / 255.0) * strength.clamp(0.0, 1.0);
    if coverage <= 0.0 {
        return;
    }
    let existing = f32::from(destination[3]) / 255.0;
    let out_alpha = coverage + existing * (1.0 - coverage);
    if out_alpha <= 0.0 {
        return;
    }
    for channel in 0..3 {
        let blended = (source[channel] * coverage
            + f32::from(destination[channel]) * existing * (1.0 - coverage))
            / out_alpha;
        destination[channel] = blended.round().clamp(0.0, 255.0) as u8;
    }
    destination[3] = (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn sample_bilinear(rgba8: &[u8], size: RasterSize, x: f32, y: f32) -> [f32; 4] {
    let RasterSize { width, height } = size;
    let max_x = width.saturating_sub(1) as f32;
    let max_y = height.saturating_sub(1) as f32;
    let x = x.clamp(0.0, max_x);
    let y = y.clamp(0.0, max_y);
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let x1 = (x0 + 1.0).min(max_x);
    let y1 = (y0 + 1.0).min(max_y);
    let texel = |px: f32, py: f32| {
        let offset = (py as usize * width as usize + px as usize) * 4;
        [
            f32::from(rgba8[offset]),
            f32::from(rgba8[offset + 1]),
            f32::from(rgba8[offset + 2]),
            f32::from(rgba8[offset + 3]),
        ]
    };
    let (a, b, c, d) = (texel(x0, y0), texel(x1, y0), texel(x0, y1), texel(x1, y1));
    std::array::from_fn(|channel| {
        let top = a[channel] + (b[channel] - a[channel]) * fx;
        let bottom = c[channel] + (d[channel] - c[channel]) * fx;
        top + (bottom - top) * fy
    })
}

struct HealCorrection {
    grid: Vec<[f32; 3]>,
    origin: [f32; 2],

    scale: [f32; 2],
}

const HEAL_GRID: usize = 16;

const HEAL_SWEEPS: usize = 96;

impl HealCorrection {
    fn resolve(
        &self,
        rgba8: &[u8],
        size: RasterSize,
        x: f32,
        y: f32,
        source_offset: Option<[f32; 2]>,
        _cell: [usize; 2],
    ) -> [u8; 3] {
        let base = match source_offset {
            Some(offset) => sample_bilinear(rgba8, size, x - offset[0], y - offset[1]),
            None => [0.0, 0.0, 0.0, 255.0],
        };
        let correction = self.sample(x, y);
        std::array::from_fn(|channel| {
            (base[channel] + correction[channel])
                .round()
                .clamp(0.0, 255.0) as u8
        })
    }

    fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        let gx = ((x - self.origin[0]) / self.scale[0]).clamp(0.0, (HEAL_GRID - 1) as f32);
        let gy = ((y - self.origin[1]) / self.scale[1]).clamp(0.0, (HEAL_GRID - 1) as f32);
        let x0 = gx.floor() as usize;
        let y0 = gy.floor() as usize;
        let x1 = (x0 + 1).min(HEAL_GRID - 1);
        let y1 = (y0 + 1).min(HEAL_GRID - 1);
        let fx = gx - x0 as f32;
        let fy = gy - y0 as f32;
        let cell = |cx: usize, cy: usize| self.grid[cy * HEAL_GRID + cx];
        let (a, b, c, d) = (cell(x0, y0), cell(x1, y0), cell(x0, y1), cell(x1, y1));
        std::array::from_fn(|channel| {
            let top = a[channel] + (b[channel] - a[channel]) * fx;
            let bottom = c[channel] + (d[channel] - c[channel]) * fx;
            top + (bottom - top) * fy
        })
    }
}

fn heal_correction(
    rgba8: &[u8],
    size: RasterSize,
    center: [f32; 2],
    radius: f32,
    source_offset: Option<[f32; 2]>,
    _bounds: [u32; 4],
) -> HealCorrection {
    let origin = [center[0] - radius, center[1] - radius];
    let scale = [
        (radius * 2.0 / (HEAL_GRID - 1) as f32).max(f32::MIN_POSITIVE),
        (radius * 2.0 / (HEAL_GRID - 1) as f32).max(f32::MIN_POSITIVE),
    ];
    let mut grid = vec![[0.0_f32; 3]; HEAL_GRID * HEAL_GRID];
    let mut fixed = vec![false; HEAL_GRID * HEAL_GRID];

    for cy in 0..HEAL_GRID {
        for cx in 0..HEAL_GRID {
            let x = origin[0] + cx as f32 * scale[0];
            let y = origin[1] + cy as f32 * scale[1];
            let normalized = (x - center[0]).hypot(y - center[1]) / radius.max(f32::MIN_POSITIVE);
            if normalized < HEAL_RIM {
                continue;
            }
            let destination = sample_bilinear(rgba8, size, x, y);
            let source = match source_offset {
                Some(offset) => sample_bilinear(rgba8, size, x - offset[0], y - offset[1]),
                None => [0.0, 0.0, 0.0, 255.0],
            };
            let index = cy * HEAL_GRID + cx;
            fixed[index] = true;
            grid[index] = std::array::from_fn(|channel| destination[channel] - source[channel]);
        }
    }

    for _ in 0..HEAL_SWEEPS {
        for cy in 1..HEAL_GRID - 1 {
            for cx in 1..HEAL_GRID - 1 {
                let index = cy * HEAL_GRID + cx;
                if fixed[index] {
                    continue;
                }
                let up = grid[index - HEAL_GRID];
                let down = grid[index + HEAL_GRID];
                let left = grid[index - 1];
                let right = grid[index + 1];
                grid[index] = std::array::from_fn(|channel| {
                    0.25 * (up[channel] + down[channel] + left[channel] + right[channel])
                });
            }
        }
    }
    HealCorrection {
        grid,
        origin,
        scale,
    }
}

const HEAL_RIM: f32 = 0.86;

const DODGE_BURN_STOPS: f32 = 1.0;

const SPONGE_SATURATION_GAIN: f32 = 1.9;

static SRGB_TO_LINEAR: std::sync::LazyLock<[f32; 256]> = std::sync::LazyLock::new(|| {
    std::array::from_fn(|value| {
        let encoded = value as f32 / 255.0;
        if encoded <= 0.040_45 {
            encoded / 12.92
        } else {
            ((encoded + 0.055) / 1.055).powf(2.4)
        }
    })
});

fn srgb_to_linear(value: u8) -> f32 {
    SRGB_TO_LINEAR[value as usize]
}

static SRGB_BYTE_FLOOR: std::sync::LazyLock<[f32; 256]> = std::sync::LazyLock::new(|| {
    std::array::from_fn(|byte| {
        if byte == 0 {
            return 0.0;
        }

        let (mut low, mut high) = (0.0_f32.to_bits(), 1.0_f32.to_bits());
        while low < high {
            let middle = low + (high - low) / 2;
            if u32::from(exact_linear_to_srgb(f32::from_bits(middle))) < byte as u32 {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        f32::from_bits(low)
    })
});

const SRGB_TOE_LIMIT: f32 = 0.003_130_8;

fn exact_linear_to_srgb(value: f32) -> u8 {
    let linear = value.clamp(0.0, 1.0);
    let encoded = if linear <= SRGB_TOE_LIMIT {
        linear * 12.92
    } else {
        linear.powf(1.0 / 2.4).mul_add(1.055, -0.055)
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

fn linear_to_srgb(value: f32) -> u8 {
    let linear = value.clamp(0.0, 1.0);
    let above = SRGB_BYTE_FLOOR.partition_point(|floor| *floor <= linear);
    u8::try_from(above.saturating_sub(1)).unwrap_or(u8::MAX)
}

fn lerp_u8(from: u8, to: u8, amount: f32) -> u8 {
    (f32::from(from) + (f32::from(to) - f32::from(from)) * amount.clamp(0.0, 1.0))
        .round()
        .clamp(0.0, 255.0) as u8
}

struct ToneMatch {
    exposure: f32,
    saturation: f32,
    temperature: f32,
}

#[derive(Clone, Copy, Default)]
struct ToneStats {
    luminance: f32,
    chroma: f32,
    warmth: f32,
    weight: f32,
}

fn skin_weight(rgb: [f32; 3]) -> f32 {
    let luma = rgb[0].mul_add(0.299, rgb[1].mul_add(0.587, rgb[2] * 0.114));
    if !(0.15..=0.95).contains(&luma) {
        return 0.0;
    }
    let cb = (rgb[2] - luma).mul_add(0.564, 0.5);
    let cr = (rgb[0] - luma).mul_add(0.713, 0.5);

    let band = |value: f32, low: f32, high: f32| {
        const FEATHER: f32 = 0.05;
        ((value - (low - FEATHER)) / FEATHER)
            .min(((high + FEATHER) - value) / FEATHER)
            .clamp(0.0, 1.0)
    };
    band(cb, 0.302, 0.498) * band(cr, 0.522, 0.678)
}

fn accumulate_tone(stats: &mut ToneStats, rgb: [f32; 3], weight: f32) {
    let luminance = rgb[0].mul_add(0.2126, rgb[1].mul_add(0.7152, rgb[2] * 0.0722));
    let maximum = rgb.into_iter().fold(f32::NEG_INFINITY, f32::max);
    let minimum = rgb.into_iter().fold(f32::INFINITY, f32::min);
    stats.luminance += luminance * weight;
    stats.chroma += (maximum - minimum) * weight;
    stats.warmth += (rgb[0] - rgb[2]) * weight;
    stats.weight += weight;
}

fn normalized_rgb(pixel: &[u8]) -> [f32; 3] {
    [0, 1, 2].map(|channel| f32::from(pixel[channel]) / 255.0)
}

fn solve_tone_match(source: &SkinImage, target: &SkinImage, aligned: bool) -> Option<ToneMatch> {
    const SAMPLE_BUDGET: usize = 120_000;
    let mut source_stats = ToneStats::default();
    let mut target_stats = ToneStats::default();
    if aligned {
        let stride = (source.rgba8.len() / 4 / SAMPLE_BUDGET).max(1);
        let pairs = source
            .rgba8
            .chunks_exact(4)
            .zip(target.rgba8.chunks_exact(4));
        for (source_pixel, target_pixel) in pairs.step_by(stride) {
            if source_pixel[3] < 16 || target_pixel[3] < 16 {
                continue;
            }
            let source_rgb = normalized_rgb(source_pixel);
            let target_rgb = normalized_rgb(target_pixel);

            let weight = skin_weight(source_rgb).min(skin_weight(target_rgb));
            if weight <= 0.0 {
                continue;
            }
            accumulate_tone(&mut source_stats, source_rgb, weight);
            accumulate_tone(&mut target_stats, target_rgb, weight);
        }
    } else {
        for (image, stats) in [(source, &mut source_stats), (target, &mut target_stats)] {
            let stride = (image.rgba8.len() / 4 / SAMPLE_BUDGET).max(1);
            for pixel in image.rgba8.chunks_exact(4).step_by(stride) {
                if pixel[3] < 16 {
                    continue;
                }
                let rgb = normalized_rgb(pixel);
                let weight = skin_weight(rgb);
                if weight > 0.0 {
                    accumulate_tone(stats, rgb, weight);
                }
            }
        }
    }
    if source_stats.weight <= 0.0 || target_stats.weight <= 0.0 {
        return None;
    }
    let mean = |stats: &ToneStats| {
        (
            stats.luminance / stats.weight,
            stats.chroma / stats.weight,
            stats.warmth / stats.weight,
        )
    };
    let (source_luminance, source_chroma, source_warmth) = mean(&source_stats);
    let (target_luminance, target_chroma, target_warmth) = mean(&target_stats);

    let mut temperature = 0.0_f32;
    let mut exposure_gain = 1.0_f32;
    let mut saturation_gain = 1.0_f32;
    for _ in 0..4 {
        let warmed_luminance = temperature.mul_add(0.031_152, source_luminance);
        exposure_gain = (target_luminance / warmed_luminance.max(1.0e-4)).clamp(0.125, 8.0);
        saturation_gain =
            (target_chroma / (source_chroma.max(1.0e-4) * exposure_gain)).clamp(0.0, 2.0);
        let wanted_warmth = target_warmth / (exposure_gain * saturation_gain).max(1.0e-4);

        temperature = ((wanted_warmth - source_warmth) / 0.24).clamp(-1.0, 1.0);
    }
    Some(ToneMatch {
        exposure: exposure_gain.log2().clamp(-3.0, 3.0),
        saturation: (saturation_gain - 1.0).clamp(-1.0, 1.0),
        temperature,
    })
}

fn decode_texture_path(revision: u64, path: &Path) -> Result<Arc<SkinImage>, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read texture {}: {error}", path.display()))?;
    if bytes.is_empty() || bytes.len() > MAX_TEXTURE_SOURCE_BYTES {
        return Err(format!(
            "texture byte count {} is outside 1..={MAX_TEXTURE_SOURCE_BYTES}",
            bytes.len()
        ));
    }
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| format!("texture format detection failed: {error}"))?;
    let mut image = reader
        .decode()
        .map_err(|error| format!("texture decode failed: {error}"))?;
    if image.width() > MAX_TEXTURE_SOURCE_EDGE || image.height() > MAX_TEXTURE_SOURCE_EDGE {
        image = image.thumbnail(MAX_TEXTURE_SOURCE_EDGE, MAX_TEXTURE_SOURCE_EDGE);
    }
    let rgba = image.into_rgba8();
    Ok(Arc::new(SkinImage::new(
        revision,
        rgba.width(),
        rgba.height(),
        rgba.into_raw(),
    )?))
}

fn bake_texture_project(request: &TextureBakeRequest) -> Result<TextureBakedSet, String> {
    if !is_bakeable_resolution(request.resolution) {
        return Err(format!(
            "texture bake resolution {} is unsupported",
            request.resolution
        ));
    }
    let options = TextureBakeOptions {
        width: request.resolution,
        height: request.resolution,
        boundary_feather_pixels: request.boundary_feather_pixels,
    };
    let mut channels = BTreeMap::<TextureChannel, TextureBakeImage>::new();
    if request.bake_base == TextureBakeBase::CurrentSkin
        && let Some(base) = request.base_preview.as_deref()
    {
        let full = base_face_at(request, base);
        let face = full.as_ref().unwrap_or(&base.face);
        let view = RgbaView::new(&face.rgba8, face.width, face.height)?;
        channels.insert(
            TextureChannel::Diffuse,
            resize_direct_uv(view, request.resolution, request.resolution)
                .map_err(|error| error.to_string())?,
        );
    }

    let mut layer_rasters = BTreeMap::<u64, CachedTextureLayerRaster>::new();
    let mut scan_atlases = BTreeMap::<u64, Arc<SkinImage>>::new();
    for layer in request.layers.iter().rev().filter(|layer| layer.visible) {
        if layer.mask.is_none() && layer.mask_base == 0 {
            continue;
        }
        let cached = request
            .cached_layer_rasters
            .get(&layer.id)
            .filter(|cached| {
                layer_raster_cache_matches(
                    layer,
                    cached,
                    request.resolution,
                    request.boundary_feather_pixels,
                )
            });
        let cached_hit = cached.is_some();
        let raster = if let Some(cached) = cached {
            cached.clone()
        } else {
            let raster = rasterize_layer(request, layer, options)?;
            let revision = request.request_id.wrapping_add(layer.id);
            if let Some(unmirrored) = raster.unmirrored {
                scan_atlases.insert(
                    layer.id,
                    Arc::new(SkinImage::new(
                        revision,
                        raster.baked.width,
                        raster.baked.height,
                        unmirrored,
                    )?),
                );
            }
            CachedTextureLayerRaster {
                mirror: layer.mirror,
                raster_revision: layer.raster_revision,
                resolution: request.resolution,
                boundary_feather_pixels: request.boundary_feather_pixels,
                image: Arc::new(SkinImage::new(
                    revision,
                    raster.baked.width,
                    raster.baked.height,
                    raster.baked.rgba8,
                )?),
            }
        };

        layer_rasters.insert(layer.id, raster.clone());

        let mut pixels = raster.image.rgba8.as_ref().clone();
        if layer.channel.is_color() {
            apply_color_adjustments(&mut pixels, layer.adjustments);
        }
        let _ = crate::diagnostics::record(
            crate::diagnostics::Severity::Debug,
            "texture",
            "layer_composited",
            &format!(
                "layer={}; channel={:?}; opacity={:.3}; blend={:?}; exposure={:.3}; contrast={:.3}; cached={}",
                layer.id,
                layer.channel,
                layer.opacity,
                layer.blend_mode,
                layer.adjustments.exposure,
                layer.adjustments.contrast,
                cached_hit,
            ),
        );
        apply_channel_interpretation(
            &mut pixels,
            layer.channel,
            layer.normal_strength,
            layer.scalar_invert,
        );
        let base = channels.entry(layer.channel).or_insert(
            TextureBakeImage::transparent(request.resolution, request.resolution)
                .map_err(|error| error.to_string())?,
        );
        if let Some(mask) = layer.mask.as_ref() {
            let mask = AlphaMaskView::new(&mask.alpha8, mask.width, mask.height)
                .map_err(|error| format!("layer {} mask is invalid: {error}", layer.name))?;
            composite_rgba_masked(
                &mut base.rgba8,
                &pixels,
                request.resolution,
                request.resolution,
                mask,
                layer.opacity,
                layer.blend_mode,
            )
        } else {
            composite_rgba(&mut base.rgba8, &pixels, layer.opacity, layer.blend_mode)
        }
        .map_err(|error| format!("layer {} composite failed: {error}", layer.name))?;
    }
    if channels.is_empty() {
        return Err("there are no visible texture layers to bake".to_owned());
    }
    let images = channels
        .into_iter()
        .map(|(channel, image)| {
            SkinImage::new(
                request.request_id.wrapping_add(channel as u64),
                image.width,
                image.height,
                image.rgba8,
            )
            .map(Arc::new)
            .map(|image| (channel, image))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;

    let preview_base = if request.hide_skin_preview {
        None
    } else {
        request.base_preview.as_deref()
    };
    let preview = Arc::new(build_baked_preview(
        request.request_id,
        &request.mapping,
        preview_base,
        request.neutral_base_rgb,
        &images,
    )?);
    Ok(TextureBakedSet {
        request_id: request.request_id,
        source_revision: request.project_revision,
        images,
        preview,
        layer_rasters,
        scan_atlases,
    })
}

const fn preview_face_is_coarser_than(face: (u32, u32), resolution: u32) -> bool {
    face.0 < resolution || face.1 < resolution
}

fn base_face_at(
    request: &TextureBakeRequest,
    preview: &crate::skin_preview::SkinPreview,
) -> Option<Arc<SkinImage>> {
    if !preview_face_is_coarser_than(
        (preview.face.width, preview.face.height),
        request.resolution,
    ) {
        return None;
    }
    let locator = request.base_face_source.as_ref()?;
    match crate::vam_skin::decode_skin_texture(preview.revision, locator, request.resolution) {
        Ok(image) => Some(Arc::new(image)),
        Err(_) => None,
    }
}

fn layer_raster_cache_matches(
    layer: &TextureLayerBakeInput,
    cached: &CachedTextureLayerRaster,
    resolution: u32,
    boundary_feather_pixels: u16,
) -> bool {
    cached.mirror == layer.mirror
        && cached.raster_revision == layer.raster_revision
        && cached.resolution == resolution
        && cached.boundary_feather_pixels == boundary_feather_pixels
}

struct MirroredLayerRaster {
    baked: TextureBakeImage,
    unmirrored: Option<Vec<u8>>,
}

fn rasterize_layer(
    request: &TextureBakeRequest,
    layer: &TextureLayerBakeInput,
    options: TextureBakeOptions,
) -> Result<MirroredLayerRaster, String> {
    let mut baked = rasterize_layer_unmirrored(request, layer, options)?;
    let unmirrored = (layer.source_mode == TextureSourceMode::ScanMesh
        && layer.mirror != FaceMirror::Off)
        .then(|| baked.rgba8.clone());
    if layer.mirror != FaceMirror::Off {
        let filled = request
            .face_mirror
            .as_ref()
            .map(|map| map.apply(&mut baked, &request.mapping, layer.mirror));
        let _ = crate::diagnostics::record(
            if filled.is_none_or(|count| count == 0) {
                crate::diagnostics::Severity::Warning
            } else {
                crate::diagnostics::Severity::Debug
            },
            "texture",
            "layer_mirrored",
            &format!(
                "layer={}; side={:?}; map={}; triangles={}",
                layer.id,
                layer.mirror,
                request.face_mirror.is_some(),
                filled.unwrap_or(0)
            ),
        );
    }
    Ok(MirroredLayerRaster { baked, unmirrored })
}

fn rasterize_layer_unmirrored(
    request: &TextureBakeRequest,
    layer: &TextureLayerBakeInput,
    options: TextureBakeOptions,
) -> Result<TextureBakeImage, String> {
    if let Some(painted) = &layer.painted {
        let rgba8 = if painted.width == options.width && painted.height == options.height {
            painted.rgba8.as_ref().clone()
        } else {
            vkit_core::pixels::resize_rgba_box_premultiplied(
                vkit_core::pixels::RgbaView::new(&painted.rgba8, painted.width, painted.height)
                    .map_err(|error| format!("layer {} paint is unusable: {error}", layer.name))?,
                options.width,
                options.height,
            )
        };

        return TextureBakeImage::from_rgba8(rgba8, options.width, options.height)
            .map_err(|error| format!("layer {} paint is unusable: {error}", layer.name));
    }
    match layer.source_mode {
        TextureSourceMode::LandmarkPins => {
            let image = layer
                .image
                .as_deref()
                .ok_or_else(|| format!("layer {} has no decoded image", layer.name))?;
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
            let face_uv_triangles = request
                .mapping
                .triangles
                .iter()
                .filter(|triangle| triangle.material_region == UvMaterialRegion::Face)
                .map(|triangle| triangle.uvs)
                .collect::<Vec<_>>();
            warp_image_to_g2_by_pins_in_region(
                RgbaView::new(&image.rgba8, image.width, image.height)?,
                &pins,
                options,
                &face_uv_triangles,
            )
            .map_err(|error| format!("layer {} warp failed: {error}", layer.name))
        }
        TextureSourceMode::ScanMesh => rasterize_scan_layer(request, layer, options),
        TextureSourceMode::MaterialUv => {
            let image = layer
                .image
                .as_deref()
                .ok_or_else(|| format!("layer {} has no decoded image", layer.name))?;
            let face_uv_triangles = request
                .mapping
                .triangles
                .iter()
                .filter(|triangle| triangle.material_region == UvMaterialRegion::Face)
                .map(|triangle| triangle.uvs)
                .collect::<Vec<_>>();
            place_image_on_g2_uv_in_region(
                RgbaView::new(&image.rgba8, image.width, image.height)?,
                options,
                &face_uv_triangles,
            )
            .map_err(|error| format!("layer {} could not be placed: {error}", layer.name))
        }
    }
}

fn stroke_coverage(
    slot: &mut Option<StrokeCoverage>,
    layer_id: u64,
    tool: TextureTool,
    size: RasterSize,
) -> &mut StrokeCoverage {
    if !slot
        .as_ref()
        .is_some_and(|coverage| coverage.matches(layer_id, tool, size))
    {
        *slot = Some(StrokeCoverage::new(layer_id, tool, size));
    }
    slot.as_mut().expect("just installed")
}

fn apply_layer_mask_dab(
    layer: &mut TextureLayer,
    edge: u32,
    dab: TextureMaskDab,
    coverage: &mut StrokeCoverage,
) {
    if !is_texture_resolution(edge) {
        return;
    }
    let mask = layer.mask.get_or_insert_with(|| TextureLayerMask {
        revision: 0,
        width: edge,
        height: edge,
        alpha8: Arc::new(vec![layer.mask_base; edge as usize * edge as usize]),
    });
    let alpha8 = Arc::make_mut(&mut mask.alpha8);
    let width = mask.width;
    let height = mask.height;
    let shorter_edge = width.min(height).saturating_sub(1) as f32;
    let center = [
        dab.uv[0].clamp(0.0, 1.0) * width.saturating_sub(1) as f32,
        (1.0 - dab.uv[1].clamp(0.0, 1.0)) * height.saturating_sub(1) as f32,
    ];
    let radius = (dab.radius.clamp(0.002, 0.25) * shorter_edge).max(1.0);
    let min_x = (center[0] - radius).floor().max(0.0) as u32;
    let max_x = (center[0] + radius)
        .ceil()
        .min(width.saturating_sub(1) as f32) as u32;
    let min_y = (center[1] - radius).floor().max(0.0) as u32;
    let max_y = (center[1] + radius)
        .ceil()
        .min(height.saturating_sub(1) as f32) as u32;
    let target = if dab.add { 255.0 } else { 0.0 };
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let distance = (x as f32 - center[0]).hypot(y as f32 - center[1]);
            if distance > radius {
                continue;
            }
            let index = y as usize * width as usize + x as usize;
            let falloff_weight = dab.falloff.weight(f64::from(distance / radius)) as f32;
            let Some(step) = coverage.advance(index, falloff_weight * dab.opacity.clamp(0.0, 1.0))
            else {
                continue;
            };
            let Some(alpha) = alpha8.get_mut(index) else {
                continue;
            };
            *alpha = (f32::from(*alpha) + (target - f32::from(*alpha)) * step.blend)
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
    mask.revision = mask.revision.saturating_add(1);
}

fn apply_mask_preview_dab(
    layer: &mut TextureLayer,
    dab: TextureMaskDab,
    stroke: &mut Option<StrokeCoverage>,
) {
    if layer.mask_preview.is_none() {
        reset_mask_preview(layer);
    }
    let Some(source) = dab.source else {
        return;
    };
    let layer_id = layer.id;
    let Some(preview) = layer.mask_preview.as_mut() else {
        return;
    };
    let preview = Arc::make_mut(preview);
    let size = RasterSize {
        width: preview.width,
        height: preview.height,
    };
    let coverage = stroke_coverage(stroke, layer_id, TextureTool::MaskBrush, size);
    let pixels = Arc::make_mut(&mut preview.rgba8);
    raster_mask_preview_stroke(
        pixels,
        size,
        source,
        BrushDab {
            radius: dab.radius,
            falloff: dab.falloff,
            opacity: dab.opacity,
        },
        dab.add,
        coverage,
    );
    preview.revision = preview.revision.saturating_add(1);
}

fn reset_mask_preview(layer: &mut TextureLayer) {
    let previous_revision = layer
        .mask_preview
        .as_deref()
        .map_or(0, |preview| preview.revision);
    let alpha = if layer.mask_base == 0 {
        MASK_PREVIEW_MAX_ALPHA
    } else {
        0
    };
    let rgba8 = [255, 0, 0, alpha].repeat(
        usize::try_from(MASK_PREVIEW_EDGE)
            .unwrap_or(0)
            .saturating_pow(2),
    );
    layer.mask_preview = SkinImage::new(
        previous_revision.saturating_add(1),
        MASK_PREVIEW_EDGE,
        MASK_PREVIEW_EDGE,
        rgba8,
    )
    .ok()
    .map(Arc::new);
}

fn raster_mask_preview_stroke(
    rgba8: &mut [u8],
    size: RasterSize,
    source: [f32; 2],
    dab: BrushDab,
    add: bool,
    coverage: &mut StrokeCoverage,
) {
    let RasterSize { width, height } = size;
    let BrushDab {
        radius,
        falloff,
        opacity,
    } = dab;
    if !size.fits(rgba8) {
        return;
    }
    let center = [
        source[0].clamp(0.0, 1.0) * width.saturating_sub(1) as f32,
        source[1].clamp(0.0, 1.0) * height.saturating_sub(1) as f32,
    ];
    let pixel_radius =
        (radius.clamp(0.002, 0.25) * width.min(height).saturating_sub(1) as f32).max(1.0);
    let min_x = (center[0] - pixel_radius).floor().max(0.0) as u32;
    let max_x = (center[0] + pixel_radius)
        .ceil()
        .min(width.saturating_sub(1) as f32) as u32;
    let min_y = (center[1] - pixel_radius).floor().max(0.0) as u32;
    let max_y = (center[1] + pixel_radius)
        .ceil()
        .min(height.saturating_sub(1) as f32) as u32;
    let target = if add {
        0.0
    } else {
        f32::from(MASK_PREVIEW_MAX_ALPHA)
    };
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let distance = (x as f32 - center[0]).hypot(y as f32 - center[1]);
            if distance > pixel_radius {
                continue;
            }
            let index = y as usize * width as usize + x as usize;
            let falloff_weight = falloff.weight(f64::from(distance / pixel_radius)) as f32;
            let Some(step) = coverage.advance(index, falloff_weight * opacity.clamp(0.0, 1.0))
            else {
                continue;
            };
            let alpha = &mut rgba8[index * 4 + 3];
            *alpha = (f32::from(*alpha) + (target - f32::from(*alpha)) * step.blend)
                .round()
                .clamp(0.0, f32::from(MASK_PREVIEW_MAX_ALPHA)) as u8;
        }
    }
}

fn apply_channel_interpretation(
    rgba8: &mut [u8],
    channel: TextureChannel,
    normal_strength: f32,
    scalar_invert: bool,
) {
    if channel == TextureChannel::Normal {
        let strength = if normal_strength.is_finite() {
            normal_strength.clamp(0.0, 3.0)
        } else {
            1.0
        };
        for pixel in rgba8.chunks_exact_mut(4) {
            let x = (f32::from(pixel[0]) / 255.0 * 2.0 - 1.0) * strength;
            let y = (f32::from(pixel[1]) / 255.0 * 2.0 - 1.0) * strength;
            let z = f32::from(pixel[2]) / 255.0 * 2.0 - 1.0;
            let length = x.mul_add(x, y.mul_add(y, z * z)).sqrt().max(1.0e-6);
            for (target, value) in pixel[..3]
                .iter_mut()
                .zip([x / length, y / length, z / length])
            {
                *target = ((value * 0.5 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    } else if scalar_invert && !channel.is_color() {
        for pixel in rgba8.chunks_exact_mut(4) {
            for value in &mut pixel[..3] {
                *value = 255_u8.saturating_sub(*value);
            }
        }
    }
}

fn rasterize_scan_layer(
    request: &TextureBakeRequest,
    _layer: &TextureLayerBakeInput,
    options: TextureBakeOptions,
) -> Result<TextureBakeImage, String> {
    let scan = request
        .scan
        .as_ref()
        .ok_or_else(|| "the scan texture source is unavailable".to_owned())?;
    let mut document = (*scan.document).clone();
    for vertex in &mut document.geometry.vertices {
        *vertex = scan
            .transform
            .point_to_world(glam::DVec3::from_array(*vertex))
            .to_array();
    }
    let target = (*request.target).clone();
    let transfer = transfer_texture_to_g2(
        &document,
        &scan.materials,
        &target,
        &TextureTransferOptions {
            symmetry_applied: scan.symmetry_applied,
            output_material_library: PathBuf::from("Vkit_Texture.mtl"),
            output_diffuse_map: None,
        },
    );
    let transfer = match transfer {
        TextureTransferOutcome::Transferred(transfer) => transfer,
        TextureTransferOutcome::Skipped(receipt) => {
            return Err(format!(
                "scan texture projection was skipped ({})",
                receipt.reason.code()
            ));
        }
    };
    let source_path = scan
        .source_obj_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(&transfer.atlas_copy.source_path_from_obj);
    let source = decode_texture_path(request.request_id, &source_path)?;
    bake_projected_texture_to_g2(
        &transfer.document,
        &request.mapping,
        RgbaView::new(&source.rgba8, source.width, source.height)?,
        options,
    )
    .map_err(|error| format!("scan texture bake failed: {error}"))
}

fn build_baked_preview(
    revision: u64,
    mapping: &G2UvMapping,
    base: Option<&SkinPreview>,
    neutral_rgb: [u8; 3],
    images: &BTreeMap<TextureChannel, Arc<SkinImage>>,
) -> Result<SkinPreview, String> {
    let mut preview = if let Some(base) = base {
        base.clone()
    } else {
        neutral_preview(revision, mapping, neutral_rgb)?
    };
    preview.revision = revision;
    if let Some(diffuse) = images.get(&TextureChannel::Diffuse) {
        let mut composite = resize_rgba_box(
            RgbaView {
                rgba8: &preview.face.rgba8,
                width: preview.face.width,
                height: preview.face.height,
            },
            diffuse.width,
            diffuse.height,
        );
        composite_rgba(
            &mut composite,
            &diffuse.rgba8,
            1.0,
            TextureBlendMode::Normal,
        )
        .map_err(|error| format!("diffuse preview composite failed: {error}"))?;
        let mut face = SkinImage::new(revision, diffuse.width, diffuse.height, composite)?;
        face.uv_orientation = diffuse.uv_orientation;
        preview.face = Arc::new(face);
    }
    let normal = images.get(&TextureChannel::Normal);
    let specular = images
        .get(&TextureChannel::Specular)
        .or_else(|| images.get(&TextureChannel::Metallic));
    let gloss = images
        .get(&TextureChannel::Glossiness)
        .or_else(|| images.get(&TextureChannel::Smoothness));
    let roughness = images.get(&TextureChannel::Roughness);
    if normal.is_some() || specular.is_some() || gloss.is_some() || roughness.is_some() {
        let width = normal
            .or(specular)
            .or(gloss)
            .or(roughness)
            .map(|image| image.width)
            .unwrap_or(1);
        let height = normal
            .or(specular)
            .or(gloss)
            .or(roughness)
            .map(|image| image.height)
            .unwrap_or(1);
        let inverted_roughness = roughness.map(|image| {
            let mut rgba = resize_rgba_box(
                RgbaView {
                    rgba8: &image.rgba8,
                    width: image.width,
                    height: image.height,
                },
                width,
                height,
            );
            for pixel in rgba.chunks_exact_mut(4) {
                let inverse = 255_u8.saturating_sub(pixel[0]);
                pixel[..3].fill(inverse);
                pixel[3] = 255;
            }
            rgba
        });
        let roughness_view = inverted_roughness.as_deref().map(|rgba8| RgbaView {
            rgba8,
            width,
            height,
        });
        let packed = pack_surface_map(
            optional_skin_image_view(normal),
            optional_skin_image_view(specular),
            optional_skin_image_view(gloss).or(roughness_view),
            width,
            height,
            vkit_core::pixels::SurfacePackSettings {
                default_specular: 96,
                default_gloss: 140,

                normal_strength: 1.0,
            },
        );
        preview.face_surface = SkinSurfaceMap {
            packed: Arc::new(SkinImage::new(
                revision.wrapping_add(100),
                width,
                height,
                packed,
            )?),
        };
    }
    Ok(preview)
}

fn optional_skin_image_view(image: Option<&Arc<SkinImage>>) -> Option<RgbaView<'_>> {
    image.map(|image| RgbaView {
        rgba8: &image.rgba8,
        width: image.width,
        height: image.height,
    })
}

fn neutral_preview(
    revision: u64,
    mapping: &G2UvMapping,
    colour: [u8; 3],
) -> Result<SkinPreview, String> {
    let geometry = Arc::new(SkinPreviewGeometry::new(
        revision,
        mapping
            .triangles
            .iter()
            .filter_map(|triangle| {
                if !triangle.on_head
                    && !crate::vam_skin::is_eye_attachment(triangle.material_region)
                {
                    return None;
                }
                Some(SkinTriangle {
                    source_triangle_id: triangle.canonical_triangle_index,
                    channel: match triangle.material_region {
                        UvMaterialRegion::Face => SkinChannel::Face,
                        UvMaterialRegion::Torso => SkinChannel::Torso,

                        UvMaterialRegion::Limbs | UvMaterialRegion::Genitals => return None,
                        UvMaterialRegion::Sclera => SkinChannel::Sclera,
                        UvMaterialRegion::Iris => SkinChannel::Iris,
                        UvMaterialRegion::Pupil => SkinChannel::Pupil,
                        UvMaterialRegion::Cornea => SkinChannel::Cornea,
                        UvMaterialRegion::EyeReflection => SkinChannel::EyeReflection,
                        UvMaterialRegion::Lacrimal => SkinChannel::Lacrimal,
                        UvMaterialRegion::Tear => SkinChannel::Tear,
                        UvMaterialRegion::InnerMouth => SkinChannel::InnerMouth,
                        UvMaterialRegion::Teeth => SkinChannel::Teeth,
                        UvMaterialRegion::Gums => SkinChannel::Gums,
                        UvMaterialRegion::Tongue => SkinChannel::Tongue,
                        UvMaterialRegion::Eyelashes => SkinChannel::Eyelashes,
                    },
                    corners: std::array::from_fn(|corner| SkinCorner {
                        vertex_id: triangle.position_indices[corner],
                        uv: triangle.uvs[corner],
                    }),
                })
            })
            .collect(),
    )?);
    let flat = [colour[0], colour[1], colour[2], 255];
    let face = Arc::new(SkinImage::solid(revision.wrapping_add(1), flat));
    let torso = Arc::new(SkinImage::solid(revision.wrapping_add(2), flat));
    let white = Arc::new(SkinImage::solid(revision.wrapping_add(3), [255; 4]));
    Ok(SkinPreview {
        revision,
        geometry,
        face,

        torso,
        sclera: Arc::clone(&white),
        iris: Arc::clone(&white),
        lacrimal: Arc::clone(&white),
        inner_mouth: Arc::clone(&white),
        teeth: Arc::clone(&white),
        gums: Arc::clone(&white),
        tongue: Arc::clone(&white),
        eyelashes: white,

        face_surface: SkinSurfaceMap::matte(revision.wrapping_add(10)),
        torso_surface: SkinSurfaceMap::matte(revision.wrapping_add(11)),
        mouth_surface_atlas: SkinSurfaceMap::neutral_mouth_atlas(revision.wrapping_add(12)),
        sclera_surface: SkinSurfaceMap::matte(revision.wrapping_add(13)),
        iris_surface: SkinSurfaceMap::matte(revision.wrapping_add(14)),
        lacrimal_surface: SkinSurfaceMap::matte(revision.wrapping_add(15)),
        auxiliary_colors: [[255; 4]; 8],
        auxiliary_textured: [false; 8],
    })
}

fn texture_panic_detail(payload: Box<dyn Any + Send>) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic payload");
    format!("texture worker stopped unexpectedly: {message}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_export_reaches_past_the_preview_only_when_the_preview_is_coarser() {
        assert!(
            preview_face_is_coarser_than((2048, 2048), 4096),
            "a 2048 preview cannot answer for a 4096 export"
        );
        assert!(!preview_face_is_coarser_than((2048, 2048), 2048));
        assert!(!preview_face_is_coarser_than((2048, 2048), 1024));
        assert!(!preview_face_is_coarser_than((4096, 4096), 4096));
    }

    #[test]
    fn a_painted_layer_carries_its_adjustment_once() {
        let painted = vec![120_u8, 120, 120, 255];
        let adjustments = TextureColorAdjustments {
            exposure: 0.6,
            contrast: 0.35,
            ..TextureColorAdjustments::default()
        };

        let mut once = painted.clone();
        apply_color_adjustments(&mut once, adjustments);
        let mut twice = once.clone();
        apply_color_adjustments(&mut twice, adjustments);

        assert_ne!(
            once, twice,
            "the adjustment moves pixels, so applying it twice is visible"
        );
        assert_ne!(
            once, painted,
            "and applying it once is not a no-op either, or the test proves nothing"
        );
    }

    #[test]
    fn a_painted_atlas_keeps_the_resolution_it_was_authored_at() {
        let mut project = TextureProject {
            resolution: 2048,
            ..Default::default()
        };
        let mut layer = TextureLayer::image(
            1,
            PathBuf::from("face.png"),
            TextureSourceMode::LandmarkPins,
        );
        layer.painted = Some(TextureLayerPaint {
            width: 2048,
            height: 2048,
            rgba8: Arc::new(vec![0; 2048 * 2048 * 4]),
        });
        project.layers.push(layer);
        project.selected_layer_id = Some(1);

        project.resolution = 4096;
        let source = SkinImage::new(0, 4, 4, [200, 180, 170, 255].repeat(16)).unwrap();
        project.stamp_projection(
            &source,
            &[],
            vkit_core::texture_bake::ProjectionBrush {
                centre: [0.5, 0.5],
                radius: 0.1,
                falloff: SculptFalloff::Smooth,
                opacity: 1.0,
            },
            |_| None,
        );

        let paint = project.layers[0].painted.as_ref().expect("atlas survives");
        assert_eq!(
            (paint.width, paint.height),
            (2048, 2048),
            "raising the export size must not resample painted pixels into detail nobody drew"
        );
    }

    #[test]
    fn a_retouch_dab_records_the_box_it_touched() {
        let mut project = TextureProject::default();
        let mut layer =
            TextureLayer::image(1, PathBuf::from("face.png"), TextureSourceMode::MaterialUv);
        layer.image = Some(Arc::new(
            SkinImage::new(0, 64, 64, [120, 120, 120, 255].repeat(64 * 64)).unwrap(),
        ));
        project.layers.push(layer);
        project.mask_brush_radius = 0.05;

        project.apply_retouch_dab(1, TextureTool::DodgeBurn, [0.5, 0.5], false);
        let layer = &project.layers[0];
        let (revision, region) = *layer.edited_regions.back().expect("a dab was recorded");
        assert_eq!(
            revision,
            layer.edited_image.as_ref().unwrap().revision,
            "the box is tagged with the revision it produced"
        );

        assert!(region[0] > 24 && region[2] < 40, "{region:?}");
        assert!(region[1] > 24 && region[3] < 40, "{region:?}");
    }

    fn one_stroke(layer_id: u64, tool: TextureTool, size: impl Into<RasterSize>) -> StrokeCoverage {
        StrokeCoverage::new(layer_id, tool, size.into())
    }

    #[test]
    fn the_stencil_reads_back_the_corners_it_is_drawn_at() {
        let centre = [400.0_f32, 300.0];
        let size = [200.0_f32, 100.0];
        for placement in [
            StencilPlacement::default(),
            StencilPlacement::default().panned([37.0, -18.0]),
            StencilPlacement {
                scale: 2.5,
                ..StencilPlacement::default()
            },
            StencilPlacement {
                rotation: std::f32::consts::FRAC_PI_3,
                ..StencilPlacement::default()
            },
            StencilPlacement {
                offset: [-60.0, 25.0],
                scale: 0.6,
                rotation: -0.9,
            },
        ] {
            let middle = [
                centre[0] + placement.offset[0],
                centre[1] + placement.offset[1],
            ];
            let uv = placement
                .source_at(middle, centre, size)
                .expect("the centre is on the image");
            assert!(
                (uv[0] - 0.5).abs() < 1.0e-3 && (uv[1] - 0.5).abs() < 1.0e-3,
                "{placement:?} put its centre at {uv:?}"
            );

            let half = [
                size[0] * placement.scale * 0.5,
                size[1] * placement.scale * 0.5,
            ];
            let (sine, cosine) = placement.rotation.sin_cos();
            for (corner, expected) in [
                ([-half[0], -half[1]], [0.0_f32, 0.0]),
                ([half[0], -half[1]], [1.0, 0.0]),
                ([half[0], half[1]], [1.0, 1.0]),
                ([-half[0], half[1]], [0.0, 1.0]),
            ] {
                let screen = [
                    middle[0] + corner[0] * cosine - corner[1] * sine,
                    middle[1] + corner[0] * sine + corner[1] * cosine,
                ];
                let uv = placement
                    .source_at(screen, centre, size)
                    .unwrap_or_else(|| panic!("{placement:?} lost its corner {expected:?}"));
                assert!(
                    (uv[0] - expected[0]).abs() < 1.0e-2 && (uv[1] - expected[1]).abs() < 1.0e-2,
                    "{placement:?} corner {expected:?} read back {uv:?}"
                );
            }

            assert!(
                placement
                    .source_at([middle[0] + 10_000.0, middle[1]], centre, size)
                    .is_none()
            );
        }
    }

    #[test]
    fn the_stencil_zooms_about_the_pointer() {
        let centre = [400.0_f32, 300.0];
        let size = [200.0_f32, 100.0];
        let pointer = [460.0_f32, 320.0];
        let placement = StencilPlacement::default();
        let before = placement
            .source_at(pointer, centre, size)
            .expect("on the image");
        let after = placement
            .zoomed(2.0, pointer, centre)
            .source_at(pointer, centre, size)
            .expect("still on the image");
        assert!(
            (before[0] - after[0]).abs() < 1.0e-3 && (before[1] - after[1]).abs() < 1.0e-3,
            "the pixel under the cursor moved: {before:?} -> {after:?}"
        );
    }

    #[test]
    fn textures_export_beside_vam_own_folders_and_not_under_a_sex_directory() {
        let project = TextureProject::default();
        let root = PathBuf::from("V:/VaM");
        for (sex, expected) in [
            (FigureSex::Female, "FemaleBase"),
            (FigureSex::Male, "MaleBase"),
        ] {
            let directory = project
                .default_export_directory(Some(&root), sex)
                .expect("a root gives a directory");
            assert_eq!(
                directory,
                root.join("Custom")
                    .join("Atom")
                    .join("Person")
                    .join("Textures")
                    .join(expected)
            );
            assert!(
                !directory
                    .components()
                    .any(|part| part.as_os_str() == "Female" || part.as_os_str() == "Male"),
                "no sex directory: {}",
                directory.display()
            );
        }
    }

    #[test]
    fn the_user_names_the_texture_and_the_channel_adds_its_suffix() {
        let diffuse = texture_export_filename("winter", TextureChannel::Diffuse);
        let normal = texture_export_filename("winter", TextureChannel::Normal);
        assert!(diffuse.starts_with("winter"), "{diffuse}");
        assert!(normal.starts_with("winter"), "{normal}");
        assert_ne!(diffuse, normal, "the map type has to survive the name");
        assert!(diffuse.ends_with(".jpg"), "{diffuse}");
        assert!(normal.ends_with(".png"), "{normal}");

        assert_ne!(
            texture_export_filename("summer", TextureChannel::Diffuse),
            diffuse
        );
    }

    #[test]
    fn the_linear_to_srgb_table_answers_exactly_what_the_curve_does() {
        const SAMPLES: u32 = 200_000;
        let mut worst = 0_i32;
        let mut worst_at = 0.0_f32;
        for sample in 0..=SAMPLES {
            let linear = sample as f32 / SAMPLES as f32;
            let difference =
                i32::from(linear_to_srgb(linear)) - i32::from(exact_linear_to_srgb(linear));
            if difference.abs() > worst.abs() {
                worst = difference;
                worst_at = linear;
            }
        }
        assert_eq!(
            worst, 0,
            "table and curve disagree by {worst} at linear {worst_at}"
        );

        assert_eq!(linear_to_srgb(0.0), 0);
        assert_eq!(linear_to_srgb(1.0), 255);

        for value in 0..=255_u8 {
            assert_eq!(linear_to_srgb(srgb_to_linear(value)), value, "byte {value}");
        }
    }

    #[test]
    fn project_uses_an_accordion_and_stable_layer_selection() {
        let mut project = TextureProject::default();
        let first =
            project.add_image_layer(PathBuf::from("first.png"), TextureSourceMode::LandmarkPins);
        let second =
            project.add_image_layer(PathBuf::from("second.png"), TextureSourceMode::LandmarkPins);

        assert_eq!(project.selected_layer_id, Some(second));
        assert_eq!(
            project.selected_layer().unwrap().name,
            format!("Layer {second}")
        );
        assert!(project.remove_layer(second));
        assert_eq!(project.selected_layer_id, Some(first));
    }

    #[test]
    fn texture_stroke_transaction_undoes_as_one_edit() {
        let mut project = TextureProject::default();
        let layer_id =
            project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
        project.begin_undo_transaction();
        project.add_mask_dab(layer_id, [0.25, 0.35], Some([0.25, 0.65]), false);
        project.add_mask_dab(layer_id, [0.30, 0.40], Some([0.30, 0.60]), false);
        project.end_undo_transaction();
        assert!(project.selected_layer().unwrap().mask.is_some());

        assert!(project.undo());
        assert!(project.selected_layer().unwrap().mask.is_none());
        assert!(project.dirty);
    }

    #[test]
    fn mask_preview_tracks_hidden_alpha_without_rebaking_the_source_image() {
        let mut project = TextureProject::default();
        let layer_id =
            project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
        let raster_revision = project.selected_layer().unwrap().raster_revision;
        project.mask_brush_falloff = SculptFalloff::Sharp;
        project.mask_brush_opacity = 1.0;
        project.add_mask_dab(layer_id, [0.5, 0.5], Some([0.5, 0.5]), true);

        let layer = project.selected_layer().unwrap();
        let preview = layer.mask_preview.as_deref().unwrap();
        let center = ((preview.height as usize / 2) * preview.width as usize
            + preview.width as usize / 2)
            * 4;
        assert_eq!(&preview.rgba8[center..center + 3], &[255, 0, 0]);
        assert!(preview.rgba8[center + 3] > 100);
        assert!(preview.rgba8[center + 3] <= MASK_PREVIEW_MAX_ALPHA);
        assert!(layer.image.is_none());
        let mask = layer.mask.as_ref().unwrap();
        let mask_center = mask.height as usize / 2 * mask.width as usize + mask.width as usize / 2;
        assert!(mask.alpha8[mask_center] < 32);
        assert_eq!(layer.raster_revision, raster_revision);

        let hidden_alpha = preview.rgba8[center + 3];
        project.add_mask_dab(layer_id, [0.5, 0.5], Some([0.5, 0.5]), false);
        let restored = project
            .selected_layer()
            .unwrap()
            .mask_preview
            .as_deref()
            .unwrap();
        assert!(restored.rgba8[center + 3] < hidden_alpha);
    }

    #[test]
    fn mask_changes_preserve_the_expensive_target_color_raster_cache() {
        let mut layer = TextureLayer::image(
            1,
            PathBuf::from("face.png"),
            TextureSourceMode::LandmarkPins,
        );
        let cached = CachedTextureLayerRaster {
            mirror: FaceMirror::Off,
            raster_revision: layer.raster_revision,
            resolution: 2048,
            boundary_feather_pixels: 16,
            image: Arc::new(SkinImage::solid(1, [20, 40, 60, 255])),
        };
        apply_layer_mask_dab(
            &mut layer,
            2048,
            TextureMaskDab {
                uv: [0.5, 0.5],
                radius: 0.05,
                falloff: SculptFalloff::Smooth,
                opacity: 1.0,
                add: false,
                source: Some([0.5, 0.5]),
            },
            &mut one_stroke(1, TextureTool::MaskBrush, 2048),
        );
        let masked = TextureLayerBakeInput::from(&layer);
        assert!(layer_raster_cache_matches(&masked, &cached, 2048, 16));

        layer.invalidate_raster();
        let changed_source = TextureLayerBakeInput::from(&layer);
        assert!(!layer_raster_cache_matches(
            &changed_source,
            &cached,
            2048,
            16
        ));
    }

    #[test]
    fn pin_sides_fill_the_first_incomplete_pair() {
        let mut project = TextureProject::default();
        project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
        project.add_source_pin([0.2, 0.3]);
        project.add_target_pin(TextureTargetPin {
            triangle_index: 7,
            barycentric: [0.2, 0.3, 0.5],
            uv: [0.4, 0.6],
        });
        let pins = &project.selected_layer().unwrap().pins;
        assert_eq!(pins.len(), 1);
        assert!(pins[0].source.is_some() && pins[0].target.is_some());
        assert!(!project.selected_layer().unwrap().pin_pair_invalid(0));
    }

    fn skin_field(rgb: [u8; 3], intruder: Option<[u8; 3]>) -> SkinImage {
        let (width, height) = (32_u32, 32_u32);
        let mut rgba8 = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..height {
            for x in 0..width {
                let colour = match intruder {
                    Some(other) if x < width / 2 => other,
                    _ => rgb,
                };
                rgba8.extend_from_slice(&[colour[0], colour[1], colour[2], 255]);
            }
        }
        SkinImage::new(1, width, height, rgba8).expect("valid test image")
    }

    #[test]
    fn tone_matching_lands_the_source_on_the_target() {
        let source = skin_field([150, 112, 96], None);
        let target = skin_field([196, 150, 124], None);
        let solved = solve_tone_match(&source, &target, true).expect("both fields are skin");

        let mut matched = source.rgba8.as_ref().clone();
        vkit_core::texture_bake::apply_color_adjustments(
            &mut matched,
            TextureColorAdjustments {
                exposure: solved.exposure,
                saturation: solved.saturation,
                temperature: solved.temperature,
                ..TextureColorAdjustments::default()
            },
        );
        for (channel, (&got, &want)) in matched.iter().zip(target.rgba8.iter()).take(3).enumerate()
        {
            let (got, want) = (i32::from(got), i32::from(want));
            assert!(
                (got - want).abs() <= 6,
                "channel {channel} landed on {got}, wanted {want}"
            );
        }
    }

    #[test]
    fn tone_matching_ignores_what_is_not_skin() {
        let clean = solve_tone_match(
            &skin_field([150, 112, 96], None),
            &skin_field([196, 150, 124], None),
            true,
        )
        .expect("skin present");
        let littered = solve_tone_match(
            &skin_field([150, 112, 96], Some([20, 40, 200])),
            &skin_field([196, 150, 124], None),
            true,
        )
        .expect("skin still present in the other half");
        assert!((clean.exposure - littered.exposure).abs() < 0.05);
        assert!((clean.saturation - littered.saturation).abs() < 0.05);
        assert!((clean.temperature - littered.temperature).abs() < 0.05);
    }

    fn scan_atlas(
        project: &TextureProject,
        revision: u64,
    ) -> BTreeMap<u64, CachedTextureLayerRaster> {
        let scan = project
            .layers
            .iter()
            .find(|layer| layer.source_mode == TextureSourceMode::ScanMesh)
            .expect("a scan layer to hand the atlas to");
        BTreeMap::from([(
            scan.id,
            CachedTextureLayerRaster {
                mirror: FaceMirror::Off,
                raster_revision: scan.raster_revision,
                resolution: 2,
                boundary_feather_pixels: 0,
                image: Arc::new(SkinImage::new(revision, 2, 2, vec![7; 16]).unwrap()),
            },
        )])
    }

    fn scan_projection(project: &TextureProject, revision: u64) -> BTreeMap<u64, Arc<SkinImage>> {
        let scan = project
            .layers
            .iter()
            .find(|layer| layer.source_mode == TextureSourceMode::ScanMesh)
            .expect("a scan layer to hand the atlas to");
        BTreeMap::from([(
            scan.id,
            Arc::new(SkinImage::new(revision, 2, 2, vec![7; 16]).unwrap()),
        )])
    }

    #[test]
    fn the_scan_layer_adopts_the_projection_rather_than_its_mirrored_copy() {
        let mut project = TextureProject::default();
        project.ensure_scan_layer("Scan".to_owned()).unwrap();

        project.adopt_scan_atlases(&scan_atlas(&project, 11), &scan_projection(&project, 41));

        assert_eq!(
            project.layers[0].image.as_ref().map(|image| image.revision),
            Some(41),
            "the 2D view edits the projection; the bake mirrors on top of it"
        );
    }

    #[test]
    fn the_scan_layer_goes_under_whatever_is_already_there() {
        let mut project = TextureProject::default();
        let painted = project.add_image_layer(
            PathBuf::from("freckles.png"),
            TextureSourceMode::LandmarkPins,
        );
        let scan = project.ensure_scan_layer("Scan".to_owned()).unwrap();
        assert_eq!(
            project
                .layers
                .iter()
                .map(|layer| layer.id)
                .collect::<Vec<_>>(),
            vec![scan, painted]
        );

        assert_eq!(project.ensure_scan_layer("Scan".to_owned()), Some(scan));
        assert_eq!(project.layers.len(), 2);
    }

    #[test]
    fn a_bake_hands_the_projected_atlas_back_to_the_scan_layer() {
        let mut project = TextureProject::default();
        project.ensure_scan_layer("Scan".to_owned()).unwrap();
        assert!(project.layers[0].image.is_none());

        project.adopt_scan_atlases(&scan_atlas(&project, 11), &BTreeMap::new());
        assert_eq!(
            project.layers[0].image.as_ref().map(|image| image.revision),
            Some(11),
            "the 2D view has nothing to show without this"
        );
    }

    #[test]
    fn moving_the_scan_drops_the_atlas_it_was_projected_from() {
        let mut project = TextureProject::default();
        project.ensure_scan_layer("Scan".to_owned()).unwrap();
        project.adopt_scan_atlases(&scan_atlas(&project, 11), &BTreeMap::new());
        let before = project.layers[0].raster_revision;

        project.invalidate_scan_projection();
        assert!(
            project.layers[0].image.is_none(),
            "a stale atlas is a picture of the previous placement"
        );
        assert!(project.layers[0].raster_revision > before);

        project.adopt_scan_atlases(&scan_atlas(&project, 12), &BTreeMap::new());
        assert_eq!(
            project.layers[0].image.as_ref().map(|image| image.revision),
            Some(12)
        );
    }

    #[test]
    fn the_size_the_viewport_bakes_at_is_one_the_baker_accepts() {
        assert!(
            is_bakeable_resolution(PREVIEW_BAKE_RESOLUTION),
            "the viewport asks for {PREVIEW_BAKE_RESOLUTION} on every edit; refusing it leaves the \
             face unpainted"
        );
        for edge in TEXTURE_RESOLUTIONS {
            assert!(is_bakeable_resolution(edge), "{edge}");
        }
        assert!(!is_bakeable_resolution(1023));
        assert!(
            !is_texture_resolution(PREVIEW_BAKE_RESOLUTION),
            "the preview size stays out of the export menu"
        );
    }

    #[test]
    fn a_refused_bake_is_not_asked_for_again_until_the_project_changes() {
        let mut project = TextureProject::default();
        project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
        project.finish_bake(Err("refused".to_owned()), true);
        assert!(project.bake_refused_current_edit());

        project.add_image_layer(PathBuf::from("other.png"), TextureSourceMode::LandmarkPins);
        assert!(
            !project.bake_refused_current_edit(),
            "an edit is a new question, and deserves a fresh answer"
        );
    }

    #[test]
    fn resetting_a_layer_returns_it_to_its_freshly_loaded_state() {
        let mut project = TextureProject::default();
        let id =
            project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
        project.add_source_pin([0.2, 0.3]);
        project.add_target_pin(TextureTargetPin {
            triangle_index: 7,
            barycentric: [0.2, 0.3, 0.5],
            uv: [0.4, 0.6],
        });
        {
            let layer = project.selected_layer_mut().unwrap();
            layer.opacity = 0.25;
            layer.blend_mode = TextureBlendMode::Multiply;
            layer.adjustments.exposure = 1.5;
        }

        project.reset_layer(id);

        let layer = project.selected_layer().unwrap();
        assert!(
            layer.pins.is_empty(),
            "the projection and its fit are cleared"
        );
        assert!(layer.mask.is_none() && layer.edited_image.is_none());
        assert!(
            layer.painted.is_none(),
            "the paint atlas is part of a reset"
        );
        assert_eq!(layer.adjustments, TextureColorAdjustments::default());
        assert_eq!(layer.opacity, 1.0);
        assert_eq!(layer.blend_mode, TextureBlendMode::Normal);

        assert!(layer.source_path.is_some());
    }

    #[test]
    fn broadcasting_pins_copies_them_to_every_other_layer() {
        let mut project = TextureProject::default();
        let first =
            project.add_image_layer(PathBuf::from("first.png"), TextureSourceMode::LandmarkPins);
        let second =
            project.add_image_layer(PathBuf::from("second.png"), TextureSourceMode::LandmarkPins);
        let third =
            project.add_image_layer(PathBuf::from("third.png"), TextureSourceMode::LandmarkPins);

        project.selected_layer_id = Some(first);
        project.add_source_pin([0.2, 0.3]);
        project.add_target_pin(TextureTargetPin {
            triangle_index: 7,
            barycentric: [0.2, 0.3, 0.5],
            uv: [0.4, 0.6],
        });
        let copied = project.broadcast_pins_from_selected();
        assert_eq!(copied, 2, "every other layer receives the pins");

        let pins_of = |id: u64| {
            project
                .layers
                .iter()
                .find(|layer| layer.id == id)
                .unwrap()
                .pins
                .clone()
        };
        assert_eq!(pins_of(first).len(), 1, "the source layer is unchanged");
        let received = pins_of(second);
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].source, Some([0.2, 0.3]));
        assert!(received[0].target.is_some());
        assert_eq!(pins_of(third).len(), 1, "every other layer receives them");
    }

    #[test]
    fn incomplete_and_duplicate_texture_pin_pairs_are_invalid() {
        let mut layer = TextureLayer::image(
            1,
            PathBuf::from("face.png"),
            TextureSourceMode::LandmarkPins,
        );
        layer.pins = vec![
            TexturePinPair {
                source: Some([0.2, 0.3]),
                target: Some(TextureTargetPin {
                    triangle_index: 7,
                    barycentric: [0.2, 0.3, 0.5],
                    uv: [0.4, 0.6],
                }),
            },
            TexturePinPair {
                source: Some([0.8, 0.3]),
                target: None,
            },
        ];
        assert!(!layer.pin_pair_invalid(0));
        assert!(layer.pin_pair_invalid(1));
        assert!(!layer.landmark_warp_ready());
        layer.pins[1].target = layer.pins[0].target;
        assert!(layer.pin_pair_invalid(0));
        assert!(layer.pin_pair_invalid(1));
        assert!(!layer.landmark_warp_ready());
    }

    #[test]
    fn landmark_warp_requires_three_non_collinear_complete_pairs() {
        let mut layer = TextureLayer::image(
            1,
            PathBuf::from("face.png"),
            TextureSourceMode::LandmarkPins,
        );
        layer.pins = [[0.2, 0.2], [0.5, 0.5], [0.8, 0.8]]
            .into_iter()
            .enumerate()
            .map(|(index, uv)| TexturePinPair {
                source: Some(uv),
                target: Some(TextureTargetPin {
                    triangle_index: index as u32,
                    barycentric: [1.0, 0.0, 0.0],
                    uv,
                }),
            })
            .collect();
        assert!(!layer.landmark_warp_ready());
        layer.pins[2].target.as_mut().unwrap().uv = [0.8, 0.3];
        layer.pins[2].source = Some([0.8, 0.3]);
        assert!(layer.landmark_warp_ready());
    }

    #[test]
    fn layer_drop_uses_visual_insertion_indices() {
        let mut project = TextureProject::default();
        let first =
            project.add_image_layer(PathBuf::from("first.png"), TextureSourceMode::LandmarkPins);
        let second =
            project.add_image_layer(PathBuf::from("second.png"), TextureSourceMode::LandmarkPins);
        let third =
            project.add_image_layer(PathBuf::from("third.png"), TextureSourceMode::LandmarkPins);
        assert_eq!(
            project
                .layers
                .iter()
                .map(|layer| layer.id)
                .collect::<Vec<_>>(),
            [third, second, first]
        );

        project.move_layer_to(first, 0);
        assert_eq!(
            project
                .layers
                .iter()
                .map(|layer| layer.id)
                .collect::<Vec<_>>(),
            [first, third, second]
        );
        project.move_layer_to(first, 3);
        assert_eq!(
            project
                .layers
                .iter()
                .map(|layer| layer.id)
                .collect::<Vec<_>>(),
            [third, second, first]
        );
    }

    #[test]
    fn baked_preview_composites_transparent_diffuse_over_the_skin() {
        let mapping = G2UvMapping {
            source_path: PathBuf::new(),
            coordinate_rms_cm: 0.0,
            coordinate_max_cm: 0.0,
            uncovered_triangles: 0,
            faces: Vec::new(),
            triangles: vec![vkit_core::vam::G2UvTriangle {
                canonical_face_index: 0,
                canonical_triangle_index: 0,
                material_region: UvMaterialRegion::Face,
                on_head: true,
                position_indices: [0, 1, 2],
                uvs: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            }],
        };
        let mut images = BTreeMap::new();
        images.insert(
            TextureChannel::Diffuse,
            Arc::new(SkinImage::new(2, 1, 1, vec![255, 0, 0, 128]).unwrap()),
        );

        let neutral = [0xe8, 0xb2, 0x78];
        let preview = build_baked_preview(3, &mapping, None, neutral, &images).unwrap();
        assert_eq!(preview.face.rgba8[3], 255);
        assert!(preview.face.rgba8[0] > u32::from(neutral[0]) as u8);
        assert!(preview.face.rgba8[1] < neutral[1]);
        assert_ne!(preview.face.rgba8.as_slice(), &[255, 0, 0, 128]);

        let dark = build_baked_preview(4, &mapping, None, [20, 30, 40], &images).unwrap();
        assert!(
            dark.face.rgba8[2] < preview.face.rgba8[2],
            "the neutral base ignored the solid colour"
        );
    }

    #[test]
    fn source_view_rejects_invalid_values_and_clamps_navigation_state() {
        let mut project = TextureProject::default();
        let layer =
            project.add_image_layer(PathBuf::from("face.png"), TextureSourceMode::LandmarkPins);
        project.set_source_view(layer, 8.0, [0.2, 0.8]);
        let selected = project.selected_layer().unwrap();
        assert_eq!(selected.source_view_zoom, 8.0);
        assert_eq!(selected.source_view_center, [0.2, 0.8]);

        project.set_source_view(layer, f32::NAN, [0.5, 0.5]);
        let selected = project.selected_layer().unwrap();
        assert_eq!(selected.source_view_zoom, 8.0);
        assert_eq!(selected.source_view_center, [0.2, 0.8]);

        project.set_source_view(layer, 99.0, [-2.0, 4.0]);
        let selected = project.selected_layer().unwrap();
        assert_eq!(selected.source_view_zoom, 32.0);
        assert_eq!(selected.source_view_center, [0.0, 1.0]);
    }

    #[test]
    fn textures_go_where_vam_keeps_them() {
        let root = PathBuf::from("V:/VaM");
        let project = TextureProject::default();
        let female = project
            .default_export_directory(Some(&root), FigureSex::Female)
            .expect("a root gives a directory");
        assert!(
            female.ends_with("Custom/Atom/Person/Textures/FemaleBase")
                || female.ends_with(r"Custom\Atom\Person\Textures\FemaleBase"),
            "{}",
            female.display()
        );
        assert!(
            !female.components().any(|part| part.as_os_str() == "Female"),
            "no bare `Female` level: {}",
            female.display()
        );
        let male = project
            .default_export_directory(Some(&root), FigureSex::Male)
            .expect("a root gives a directory");
        assert!(
            male.ends_with("MaleBase") || male.ends_with(r"MaleBase"),
            "{}",
            male.display()
        );

        let named = TextureProject {
            export_subfolder: "MyLook".to_owned(),
            ..TextureProject::default()
        };
        let custom = named
            .default_export_directory(Some(&root), FigureSex::Female)
            .expect("a root gives a directory");
        assert!(custom.ends_with("MyLook"), "{}", custom.display());
        assert_eq!(custom.parent(), female.parent());
    }

    #[test]
    fn export_names_are_safe_and_channel_specific() {
        assert_eq!(
            texture_export_filename("My:Face", TextureChannel::Diffuse),
            "My_Face_diffuse.jpg"
        );

        assert_eq!(
            texture_export_filename("", TextureChannel::Normal),
            "texture_normal.png"
        );
    }

    #[test]
    fn layer_alpha_mask_subtracts_and_alt_reverse_restores_visibility() {
        let mut layer = TextureLayer::image(
            1,
            PathBuf::from("face.png"),
            TextureSourceMode::LandmarkPins,
        );
        assert!(layer.mask_stroke_subtracts(false));
        assert!(!layer.mask_stroke_subtracts(true));
        layer.mask_base = 0;
        assert!(!layer.mask_stroke_subtracts(false));
        assert!(layer.mask_stroke_subtracts(true));

        layer.mask_base = 255;
        let center_uv = [1024.0 / 2047.0, 1.0 - 1024.0 / 2047.0];
        let subtract = TextureMaskDab {
            uv: center_uv,
            radius: 0.25,
            falloff: SculptFalloff::Sharp,
            opacity: 1.0,
            add: false,
            source: None,
        };

        apply_layer_mask_dab(
            &mut layer,
            2048,
            subtract,
            &mut one_stroke(1, TextureTool::MaskBrush, 2048),
        );
        let center = 1024 * 2048 + 1024;
        assert_eq!(layer.mask.as_ref().unwrap().alpha8[center], 0);

        apply_layer_mask_dab(
            &mut layer,
            2048,
            TextureMaskDab {
                add: true,
                ..subtract
            },
            &mut one_stroke(1, TextureTool::MaskBrush, 2048),
        );
        assert_eq!(layer.mask.as_ref().unwrap().alpha8[center], 255);
    }

    #[test]
    fn a_stroke_does_not_compound_against_itself() {
        let size = RasterSize {
            width: 16,
            height: 16,
        };
        let dab = BrushDab {
            radius: 0.25,
            falloff: SculptFalloff::Smooth,
            opacity: 0.5,
        };
        let stroke = RetouchStroke {
            tool: TextureTool::Sponge,
            point: [0.5, 0.5],
            clone_offset: None,
            reverse: true,
        };
        let mut once = [200, 120, 60, 255].repeat(256);
        let mut coverage = one_stroke(1, TextureTool::Sponge, size);
        apply_retouch_pixels(&mut once, size, stroke, dab, &mut coverage);
        let mut repeated = [200, 120, 60, 255].repeat(256);
        let mut coverage = one_stroke(1, TextureTool::Sponge, size);
        for _ in 0..8 {
            apply_retouch_pixels(&mut repeated, size, stroke, dab, &mut coverage);
        }
        assert_eq!(once, repeated);

        let mut coverage = one_stroke(1, TextureTool::Sponge, size);
        apply_retouch_pixels(&mut repeated, size, stroke, dab, &mut coverage);
        let centre = (8 * 16 + 8) * 4;
        assert!(repeated[centre] < once[centre], "a second stroke goes on");
    }

    #[test]
    fn a_clone_stroke_carries_its_source_along_with_it() {
        let size = RasterSize {
            width: 32,
            height: 8,
        };

        let mut rgba8 = vec![0_u8; 32 * 8 * 4];
        for y in 0..8 {
            for x in 0..16 {
                let offset = (y * 32 + x) * 4;
                let value = if x == 4 { 255 } else { 40 };
                rgba8[offset..offset + 4].copy_from_slice(&[value, value, value, 255]);
            }
        }
        let dab = BrushDab {
            radius: 0.12,
            falloff: SculptFalloff::Linear,
            opacity: 1.0,
        };

        let offset = [16.0 / 31.0, 0.0];
        let mut coverage = one_stroke(1, TextureTool::CloneStamp, size);
        for column in [20_usize, 21, 22] {
            apply_retouch_pixels(
                &mut rgba8,
                size,
                RetouchStroke {
                    tool: TextureTool::CloneStamp,
                    point: [column as f32 / 31.0, 0.5],
                    clone_offset: Some(offset),
                    reverse: false,
                },
                dab,
                &mut coverage,
            );
        }
        let row = 4 * 32;

        assert!(rgba8[(row + 20) * 4] > 200, "the bright column was copied");
        assert!(rgba8[(row + 21) * 4] < 120, "and it did not smear along");
        assert!(rgba8[(row + 22) * 4] < 120);
    }

    #[test]
    fn a_clone_never_reads_colour_that_carries_no_alpha() {
        let size = RasterSize {
            width: 16,
            height: 16,
        };
        let mut rgba8 = vec![0_u8; 16 * 16 * 4];

        for y in 0..16 {
            for x in 8..16 {
                let offset = (y * 16 + x) * 4;
                rgba8[offset..offset + 4].copy_from_slice(&[128, 128, 128, 255]);
            }
        }
        let before = rgba8.clone();

        let mut coverage = one_stroke(1, TextureTool::CloneStamp, size);
        apply_retouch_pixels(
            &mut rgba8,
            size,
            RetouchStroke {
                tool: TextureTool::CloneStamp,
                point: [12.0 / 15.0, 0.5],
                clone_offset: Some([8.0 / 15.0, 0.0]),
                reverse: false,
            },
            BrushDab {
                radius: 0.2,
                falloff: SculptFalloff::Linear,
                opacity: 1.0,
            },
            &mut coverage,
        );
        assert_eq!(rgba8, before, "a transparent source contributes nothing");
    }

    #[test]
    fn healing_takes_texture_from_the_source_and_light_from_the_destination() {
        let size = RasterSize {
            width: 64,
            height: 16,
        };
        let mut rgba8 = vec![0_u8; 64 * 16 * 4];
        for y in 0..16_usize {
            for x in 0..64_usize {
                let stripe = if x % 4 == 0 { 20 } else { 0 };
                let base = if x < 32 { 60 } else { 170 };
                let value = (base + stripe) as u8;
                let offset = (y * 64 + x) * 4;
                rgba8[offset..offset + 4].copy_from_slice(&[value, value, value, 255]);
            }
        }
        let mut coverage = one_stroke(1, TextureTool::Heal, size);
        apply_retouch_pixels(
            &mut rgba8,
            size,
            RetouchStroke {
                tool: TextureTool::Heal,

                point: [48.0 / 63.0, 0.5],
                clone_offset: Some([32.0 / 63.0, 0.0]),
                reverse: false,
            },
            BrushDab {
                radius: 0.25,
                falloff: SculptFalloff::Linear,
                opacity: 1.0,
            },
            &mut coverage,
        );
        let centre = (8 * 64 + 48) * 4;

        assert!(
            rgba8[centre] > 140,
            "healed centre is {} and should still be bright",
            rgba8[centre]
        );

        let plain = (8 * 64 + 49) * 4;
        assert_ne!(rgba8[centre], rgba8[plain], "the source's grain arrived");
    }

    #[test]
    fn retouch_dodge_changes_only_the_working_pixels() {
        let mut rgba8 = [80, 90, 100, 255].repeat(64);
        apply_retouch_pixels(
            &mut rgba8,
            RasterSize {
                width: 8,
                height: 8,
            },
            RetouchStroke {
                tool: TextureTool::DodgeBurn,
                point: [0.5, 0.5],
                clone_offset: None,
                reverse: false,
            },
            BrushDab {
                radius: 0.25,
                falloff: SculptFalloff::Sharp,
                opacity: 1.0,
            },
            &mut one_stroke(
                1,
                TextureTool::DodgeBurn,
                RasterSize {
                    width: 8,
                    height: 8,
                },
            ),
        );
        let center = (4 * 8 + 4) * 4;
        assert!(rgba8[center] > 80);
        assert_eq!(rgba8[3], 255);
    }
}

const EDITED_REGION_HISTORY: usize = 96;

#[cfg(test)]
mod source_modes {
    use super::*;

    #[test]
    fn no_tool_is_unreachable() {
        for tool in TextureTool::ALL {
            assert!(
                [
                    TextureSourceMode::ScanMesh,
                    TextureSourceMode::LandmarkPins,
                    TextureSourceMode::MaterialUv,
                ]
                .into_iter()
                .any(|mode| mode.allows(tool)),
                "{tool:?} is offered by no kind of layer"
            );
        }
    }

    #[test]
    fn a_g2_uv_layer_offers_only_the_mask_brush() {
        assert_eq!(
            TextureSourceMode::MaterialUv.available_tools(),
            [TextureTool::MaskBrush]
        );

        assert!(!TextureSourceMode::MaterialUv.allows(TextureTool::PinPair));
    }

    #[test]
    fn selecting_a_g2_uv_layer_settles_a_tool_it_cannot_use() {
        let mut project = TextureProject::default();
        let photo = project.add_image_layer(
            std::path::PathBuf::from("photo.png"),
            TextureSourceMode::LandmarkPins,
        );
        let decal = project.add_image_layer(
            std::path::PathBuf::from("decal.png"),
            TextureSourceMode::MaterialUv,
        );

        project.select_layer(photo);
        project.set_active_tool(TextureTool::CloneStamp);
        assert_eq!(project.active_tool, TextureTool::CloneStamp);

        project.select_layer(decal);
        assert_eq!(project.active_tool, TextureTool::MaskBrush);
        assert!(project.mask_preview_enabled, "the overlay follows the tool");

        project.select_layer(photo);
        project.set_active_tool(TextureTool::CloneStamp);
        assert_eq!(project.active_tool, TextureTool::CloneStamp);
    }
}
