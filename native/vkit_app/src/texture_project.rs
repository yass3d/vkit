use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet, VecDeque},
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
        feather_coverage_alpha, place_image_on_g2_uv_in_region, resize_direct_uv,
        warp_image_to_g2_by_pins_in_region,
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

use history::TextureUndoSnapshot;
pub(crate) use kernels::neutral_preview;
use kernels::{
    BrushDab, RasterSize, RetouchStroke, StrokeCoverage, apply_layer_mask_dab,
    apply_mask_preview_dab, apply_retouch_pixels, bake_texture_project, decode_texture_path,
    reset_mask_preview, solve_tone_match, stroke_coverage, texture_panic_detail,
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

    pub const fn suffix_for(self, opaque: bool) -> &'static str {
        match self {
            Self::Diffuse if !opaque => "_decal",
            _ => self.suffix(),
        }
    }

    pub const fn export_container_for(self, opaque: bool) -> TextureContainer {
        match self.export_container() {
            TextureContainer::Jpeg if opaque => TextureContainer::Jpeg,
            _ => TextureContainer::Png,
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
                TextureTool::DodgeBurn,
                TextureTool::Sponge,
            ],
            Self::LandmarkPins => &[
                TextureTool::Projection,
                TextureTool::PinPair,
                TextureTool::MaskBrush,
                TextureTool::CloneStamp,
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
    DodgeBurn,
    Sponge,
}

impl TextureTool {
    #[cfg(test)]
    pub const ALL: [Self; 6] = [
        Self::PinPair,
        Self::Projection,
        Self::MaskBrush,
        Self::CloneStamp,
        Self::DodgeBurn,
        Self::Sponge,
    ];

    pub const fn is_paint_brush(self) -> bool {
        matches!(
            self,
            Self::MaskBrush | Self::CloneStamp | Self::DodgeBurn | Self::Sponge
        )
    }

    pub const fn needs_warp(self) -> bool {
        self.is_paint_brush()
    }

    pub const fn alt_inverts(self) -> bool {
        matches!(
            self,
            Self::Projection | Self::MaskBrush | Self::DodgeBurn | Self::Sponge
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
    pub revision: u64,
    pub width: u32,
    pub height: u32,

    pub rgba8: Arc<Vec<u8>>,
}

static PAINT_CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_paint_revision() -> u64 {
    PAINT_CLOCK.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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

    pub painted_regions: VecDeque<(u64, [u32; 4])>,
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
            painted_regions: VecDeque::new(),
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
            painted_regions: VecDeque::new(),
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

    pub base_face: Option<CachedBaseFace>,

    pub scan_atlases: BTreeMap<u64, Arc<SkinImage>>,

    pub unblended_surface_channels: BTreeSet<TextureChannel>,
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
pub struct CachedBaseFace {
    pub preview_revision: u64,
    pub resolution: u32,

    pub from_preview_face: bool,
    pub image: Arc<SkinImage>,
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

    layer_rasters: BTreeMap<(u64, u32), CachedTextureLayerRaster>,

    base_face: Option<CachedBaseFace>,
    pub baked_preview_enabled: bool,

    pub hide_vam_skin_preview: bool,
    pub clone_sample: Option<[f32; 2]>,

    pub clone_sample_surface: Option<(u32, [f64; 3])>,

    clone_offset: Option<[f32; 2]>,

    stroke: Option<StrokeCoverage>,

    preview_stroke: Option<StrokeCoverage>,

    pub baked_resolution: u32,

    pub bake_queued: Option<TextureBakeQuality>,

    bake_failed_revision: Option<u64>,

    pub projection_placement: StencilPlacement,

    projection_placed_for: Option<u64>,

    pub projection_opacity: f32,

    pub retouch_reverse: bool,
    edit_revision: u64,
    next_layer_id: u64,
    history: crate::history::History<TextureUndoSnapshot>,
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
            mask_preview_enabled: true,
            pin_opacity: 1.0,

            export_subfolder: String::new(),
            export_prefix: String::new(),
            bake_loading: false,
            bake_error: None,
            dirty: false,
            baked: None,
            layer_rasters: BTreeMap::new(),
            base_face: None,
            baked_preview_enabled: true,
            hide_vam_skin_preview: false,
            clone_sample: None,
            clone_sample_surface: None,
            clone_offset: None,
            stroke: None,
            preview_stroke: None,
            baked_resolution: 0,
            bake_queued: None,
            bake_failed_revision: None,
            projection_placement: StencilPlacement::default(),
            projection_placed_for: None,
            projection_opacity: 0.55,
            retouch_reverse: false,
            edit_revision: 0,
            next_layer_id: 1,
            history: crate::history::History::new(),
            undo_transaction: None,
        }
    }
}

impl TextureProject {
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

    pub fn mark_dirty(&mut self) {
        self.edit_revision = self.edit_revision.saturating_add(1);
        self.dirty = true;
        self.bake_error = None;
    }

    pub const fn edit_revision(&self) -> u64 {
        self.edit_revision
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

    pub retouched: bool,
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
            retouched: layer.edited_image.is_some(),
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

    pub cached_base_face: Option<CachedBaseFace>,

    pub base_face_source: Option<vkit_core::vam::AssetLocator>,

    pub base_surface_sources: BTreeMap<TextureChannel, vkit_core::vam::AssetLocator>,
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

#[must_use]
pub fn is_opaque(rgba8: &[u8]) -> bool {
    rgba8.chunks_exact(4).all(|pixel| pixel[3] == 255)
}

#[must_use]
pub fn texture_export_filename(prefix: &str, channel: TextureChannel, opaque: bool) -> String {
    format!(
        "{}{}.{}",
        sanitize_component(prefix, "texture"),
        channel.suffix_for(opaque),
        channel.export_container_for(opaque).extension()
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

mod caches;
mod colour;
mod history;
mod kernels;
mod layers;
mod pins;
mod stencil;
mod strokes;

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod source_modes {
    use super::*;

    #[test]
    fn the_clone_anchor_stays_put_while_every_stroke_measures_from_it_afresh() {
        let mut project = TextureProject::default();
        project.set_clone_sample([0.25, 0.25]);
        assert_eq!(project.clone_sample, Some([0.25, 0.25]));

        project.clone_offset = Some([0.5, 0.5]);
        project.end_clone_stroke();
        assert_eq!(
            project.clone_offset, None,
            "the offset has to be released when the button is"
        );
        assert_eq!(
            project.clone_sample,
            Some([0.25, 0.25]),
            "the anchor is where Alt put it and a stroke ending does not move it"
        );

        project.clone_offset = Some([0.5, 0.5]);
        project.set_clone_sample([0.75, 0.1]);
        assert_eq!(project.clone_sample, Some([0.75, 0.1]));
        assert_eq!(project.clone_offset, None);
    }

    #[test]
    fn an_anchor_picked_on_the_canvas_has_no_surface_behind_it() {
        let mut project = TextureProject::default();
        project.set_clone_sample_on_surface([0.4, 0.4], 12, [0.5, 0.25, 0.25]);
        assert_eq!(project.clone_sample_surface, Some((12, [0.5, 0.25, 0.25])));

        project.set_clone_sample([0.6, 0.6]);
        assert_eq!(
            project.clone_sample_surface, None,
            "a flat pick has to clear the surface the marker was drawn from"
        );

        project.set_clone_sample_on_surface([0.2, 0.2], 3, [1.0, 0.0, 0.0]);
        project.set_clone_sample([9.0, 9.0]);
        assert_eq!(project.clone_sample, Some([0.2, 0.2]));
        assert_eq!(project.clone_sample_surface, Some((3, [1.0, 0.0, 0.0])));
    }

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
        project.selected_layer_mut().unwrap().pins = super::tests::ready_pins();
        project.set_active_tool(TextureTool::CloneStamp);
        assert_eq!(project.active_tool, TextureTool::CloneStamp);

        project.select_layer(decal);
        assert_eq!(project.active_tool, TextureTool::MaskBrush);

        project.select_layer(photo);
        project.set_active_tool(TextureTool::CloneStamp);
        assert_eq!(project.active_tool, TextureTool::CloneStamp);
    }
}
