use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RasterSize {
    pub(super) width: u32,
    pub(super) height: u32,
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
pub(super) struct BrushDab {
    pub(super) radius: f32,
    pub(super) falloff: SculptFalloff,
    pub(super) opacity: f32,
}

#[derive(Clone, Debug)]
pub(super) struct StrokeCoverage {
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

    pub(super) fn new(layer_id: u64, tool: TextureTool, size: RasterSize) -> Self {
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
pub(super) struct RetouchStroke {
    pub(super) tool: TextureTool,

    pub(super) point: [f32; 2],

    pub(super) clone_offset: Option<[f32; 2]>,

    pub(super) reverse: bool,
}

pub(super) fn apply_retouch_pixels(
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

pub(super) fn srgb_to_linear(value: u8) -> f32 {
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

pub(super) fn exact_linear_to_srgb(value: f32) -> u8 {
    let linear = value.clamp(0.0, 1.0);
    let encoded = if linear <= SRGB_TOE_LIMIT {
        linear * 12.92
    } else {
        linear.powf(1.0 / 2.4).mul_add(1.055, -0.055)
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

pub(super) fn linear_to_srgb(value: f32) -> u8 {
    let linear = value.clamp(0.0, 1.0);
    let above = SRGB_BYTE_FLOOR.partition_point(|floor| *floor <= linear);
    u8::try_from(above.saturating_sub(1)).unwrap_or(u8::MAX)
}

fn lerp_u8(from: u8, to: u8, amount: f32) -> u8 {
    (f32::from(from) + (f32::from(to) - f32::from(from)) * amount.clamp(0.0, 1.0))
        .round()
        .clamp(0.0, 255.0) as u8
}

pub(super) struct ToneMatch {
    pub(super) exposure: f32,
    pub(super) saturation: f32,
    pub(super) temperature: f32,
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

pub(super) fn solve_tone_match(
    source: &SkinImage,
    target: &SkinImage,
    aligned: bool,
) -> Option<ToneMatch> {
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

pub(super) fn decode_texture_path(revision: u64, path: &Path) -> Result<Arc<SkinImage>, String> {
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

pub(super) fn bake_texture_project(
    request: &TextureBakeRequest,
) -> Result<TextureBakedSet, String> {
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

    let mut resampled_preview_face: Option<Arc<SkinImage>> = None;
    let mut base_face = None;
    if request.bake_base == TextureBakeBase::CurrentSkin
        && !request.hide_skin_preview
        && let Some(base) = request.base_preview.as_deref()
    {
        let reusable = request.cached_base_face.as_ref().filter(|cached| {
            cached.preview_revision == base.revision && cached.resolution == request.resolution
        });
        let (resized, from_preview_face) = match reusable {
            Some(cached) => (Arc::clone(&cached.image), cached.from_preview_face),
            None => {
                let full = base_face_at(request, base);
                let face = full.as_ref().unwrap_or(&base.face);
                let view = RgbaView::new(&face.rgba8, face.width, face.height)?;
                let baked = resize_direct_uv(view, request.resolution, request.resolution)
                    .map_err(|error| error.to_string())?;
                (
                    Arc::new(SkinImage::new(
                        base.revision,
                        baked.width,
                        baked.height,
                        baked.rgba8,
                    )?),
                    full.is_none(),
                )
            }
        };
        base_face = Some(CachedBaseFace {
            preview_revision: base.revision,
            resolution: request.resolution,
            from_preview_face,
            image: Arc::clone(&resized),
        });
        if from_preview_face {
            resampled_preview_face = Some(Arc::clone(&resized));
        }
        channels.insert(
            TextureChannel::Diffuse,
            TextureBakeImage::from_rgba8(
                resized.rgba8.as_ref().clone(),
                request.resolution,
                request.resolution,
            )
            .map_err(|error| error.to_string())?,
        );
    }

    let mut unblended_surface_channels = BTreeSet::<TextureChannel>::new();
    if request.bake_base == TextureBakeBase::CurrentSkin && !request.hide_skin_preview {
        for (channel, locator) in &request.base_surface_sources {
            let revision = request
                .base_preview
                .as_deref()
                .map_or(0, |preview| preview.revision);
            let laid_down =
                crate::vam_skin::decode_skin_texture(revision, locator, request.resolution)
                    .ok()
                    .and_then(|decoded| {
                        let view =
                            RgbaView::new(&decoded.rgba8, decoded.width, decoded.height).ok()?;
                        resize_direct_uv(view, request.resolution, request.resolution).ok()
                    })
                    .and_then(|resized| {
                        TextureBakeImage::from_rgba8(
                            resized.rgba8,
                            request.resolution,
                            request.resolution,
                        )
                        .ok()
                    });
            match laid_down {
                Some(image) => {
                    channels.insert(*channel, image);
                }
                None => {
                    unblended_surface_channels.insert(*channel);
                }
            }
        }
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

        let mut pixels = std::borrow::Cow::Borrowed(raster.image.rgba8.as_slice());
        if layer.channel.is_color() && layer.adjustments != TextureColorAdjustments::default() {
            apply_color_adjustments(pixels.to_mut(), layer.adjustments);
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
        if layer.channel == TextureChannel::Normal
            || (layer.scalar_invert && !layer.channel.is_color())
        {
            apply_channel_interpretation(
                pixels.to_mut(),
                layer.channel,
                layer.normal_strength,
                layer.scalar_invert,
            );
        }
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
        crate::skin_preview::revision_domain::TEXTURE_BAKE | (request.request_id << 8),
        &request.mapping,
        preview_base,
        resampled_preview_face
            .as_ref()
            .filter(|_| preview_base.is_some()),
        request.neutral_base_rgb,
        &images,
    )?);
    Ok(TextureBakedSet {
        unblended_surface_channels,
        request_id: request.request_id,
        source_revision: request.project_revision,
        images,
        preview,
        layer_rasters,
        base_face,
        scan_atlases,
    })
}

pub(super) const fn preview_face_is_coarser_than(face: (u32, u32), resolution: u32) -> bool {
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

pub(super) fn layer_raster_cache_matches(
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

        let mut baked = TextureBakeImage::from_rgba8(rgba8, options.width, options.height)
            .map_err(|error| format!("layer {} paint is unusable: {error}", layer.name))?;
        feather_coverage_alpha(
            &mut baked.rgba8,
            baked.width,
            baked.height,
            options.boundary_feather_pixels,
        );
        return Ok(baked);
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

pub(super) fn stroke_coverage(
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

pub(super) fn apply_layer_mask_dab(
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

pub(super) fn apply_mask_preview_dab(
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

pub(super) fn reset_mask_preview(layer: &mut TextureLayer) {
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

pub(super) fn rasterize_scan_layer(
    request: &TextureBakeRequest,
    layer: &TextureLayerBakeInput,
    options: TextureBakeOptions,
) -> Result<TextureBakeImage, String> {
    if layer.retouched
        && let Some(image) = layer.image.as_deref()
    {
        let rgba8 = if image.width == options.width && image.height == options.height {
            image.rgba8.as_ref().clone()
        } else {
            vkit_core::pixels::resize_rgba_box_premultiplied(
                RgbaView::new(&image.rgba8, image.width, image.height)?,
                options.width,
                options.height,
            )
        };
        return TextureBakeImage::from_rgba8(rgba8, options.width, options.height)
            .map_err(|error| format!("layer {} retouched atlas is unusable: {error}", layer.name));
    }
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

pub(super) fn build_baked_preview(
    revision: u64,
    mapping: &G2UvMapping,
    base: Option<&SkinPreview>,

    resampled_face: Option<&Arc<SkinImage>>,
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
        let reusable = resampled_face
            .filter(|face| face.width == diffuse.width && face.height == diffuse.height);
        let mut composite = match reusable {
            Some(face) => face.rgba8.as_ref().clone(),
            None => resize_rgba_box(
                RgbaView {
                    rgba8: &preview.face.rgba8,
                    width: preview.face.width,
                    height: preview.face.height,
                },
                diffuse.width,
                diffuse.height,
            ),
        };
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

pub(crate) fn neutral_preview(
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

pub(super) fn texture_panic_detail(payload: Box<dyn Any + Send>) -> String {
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic payload");
    format!("texture worker stopped unexpectedly: {message}")
}
